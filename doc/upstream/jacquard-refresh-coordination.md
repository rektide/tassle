# Upstream issue: jacquard — `CredentialSession` refresh is uncoordinated (unsafe under concurrent use)

File this against **[rsform/jacquard](https://github.com/rsform/jacquard)** (verified against `jacquard`/`jacquard-common` **0.12.1**). Paste the block below into a GitHub issue.

---

## Title

`CredentialSession::refresh()` is uncoordinated — concurrent refreshes race on ATProto's rotating refresh JWT and can brick the session

## Summary

`CredentialSession::refresh()` reads the refresh JWT from the shared `SessionStore`, POSTs `com.atproto.server.refreshSession`, and writes the new session back — with **no mutual exclusion**, either within a single `CredentialSession` instance or across multiple instances sharing one store + key. ATProto rotates refresh JWTs on each refresh (the previous refresh JWT is invalidated after a short grace window). So two or more concurrent refreshes collapse onto the same rotating token: some fail with `invalid_grant`, and last-writer-wins on `store.set` can persist a refresh JWT that's about to be invalidated, so the *next* refresh fails too — the session dies and the user must log in again.

This affects both (a) a **single** `CredentialSession` driven concurrently (multiple in-flight `send` calls that 401 at once), and (b) **multiple** `CredentialSession` instances over the same store + key (e.g. a "session factory"/`Clone`-like handle, or a process that opens the store more than once).

## Where it happens

`jacquard` 0.12.1, `crates/jacquard/src/client/credential_session.rs`:

- `send_with_opts` triggers refresh on 401 / `TokenExpired` with no lock around it (lines **686–716**, refresh at **705–707**):
  ```rust
  if is_expired(&resp) {
      let auth = self.refresh().await?;     // no mutex, no single-flight
      opts.auth = Some(auth);
      self.client.xrpc(base_uri.borrow()).with_options(opts).send(&request).await
  }
  ```
- `refresh()` itself (lines **333–366**) is a plain read-network-write against the shared store:
  ```rust
  let session = self.store.get(&key).await;          // shared refresh JWT
  opts.auth = Some(AuthorizationToken::Bearer(session.ref refresh_jwt));
  let response = … .send(&RefreshSession).await?;    // rotates the refresh JWT
  …
  self.store.set(key, new_session).await?;           // last writer wins
  ```
- `access_token()` (lines **313–317**) reads the access JWT from the shared store by key on every call — good, token *state* is consistent across instances; it's the refresh **act** that's uncoordinated.

## Reproduction (conceptual)

One `CredentialSession` `S` over a `SessionStore`, with an expired (or about-to-expire) access JWT. Fire **N ≥ 2** concurrent `S.send(...)` calls (e.g. `tokio::join!` of several XRPC requests):

1. All N calls pass through `send_with_opts`, see the expired token / 401, and each calls `S.refresh()`.
2. All N `refresh()`s read the same refresh JWT `R1` from the store and POST `refreshSession(R1)` concurrently.
3. ATProto rotates refresh JWTs: the first response returns `R2` and invalidates `R1` (after a grace window of a few seconds). Any `refreshSession(R1)` call landing outside that window returns `invalid_grant` → `AuthError::RefreshFailed`.
4. The successful callers each `store.set(key, …)` a different `R{n}`; last writer wins. Whichever `R{n}` ATProto invalidates next rotation is the one now persisted → the next refresh 401-loop fails → the session is wedged until a fresh `createSession` login.

With two *separate* `CredentialSession` instances over the same store + key the same race occurs, and is worse because each instance persists its own refresh result, multiplying the rotation churn.

## Impact

- Intermittent `RefreshFailed` / `invalid_grant` under concurrent load, even for a single correctly-resumed session.
- Session "bricking" requiring re-login when a stale refresh JWT wins the `store.set` race.
- Blocks any safe "multiple handles to one authed account" pattern — including deriving a cloneable/factory client, embedding the session in multiple async tasks, or sharing one store across cooperating sessions. (For us this specifically blocks a `Clone`-able session handle; `CredentialSession` can't be `Clone` today because its `options`/`key`/`endpoint` fields are owned `tokio::sync::RwLock`, and even if it were, un-coordinated refresh makes concurrent clones unsafe.)

## Suggested fix

**Single-flight refresh keyed by `SessionKey`.** The first 401 for a given key initiates `refresh()`; concurrent 401s for the same key await the in-flight refresh and reuse its result instead of firing their own `refreshSession`. Concretely, one of:

1. A per-`CredentialSession` async mutex around `refresh()` — fixes the single-instance concurrent case only.
2. **(Preferred)** A per-`SessionKey` single-flight registry (e.g. held on the `SessionStore` or a small lock map alongside it) so refresh is coalesced across *all* sessions over that store + key. `SessionStore` could expose a `refresh_or_join(key, refresh_fn)` helper, or `CredentialSession` could keep an internal `HashMap<SessionKey, Shared<RefreshFuture>>`.

Either way, the network `refreshSession` call for a given key happens at most once per refresh epoch, and concurrent callers observe the resulting token.

## Related nice-to-have

If `options` / `key` / `endpoint` were stored as `Arc<RwLock<_>>` rather than owned `RwLock<_>`, `CredentialSession` could `#[derive(Clone)]`, and — combined with single-flight refresh on the shared cell — clones would be both cheap *and* safe (one logical session, many handles). That would obsolete the "session factory" workaround downstream consumers are currently forced to build.

## Environment

- `jacquard` / `jacquard-common` 0.12.1 (crates.io).
- Reproduced via analysis of the 0.12.1 source (`credential_session.rs:313–317`, `:333–366`, `:686–716`).

---

### Notes for tassle (not part of the upstream text)

- Filed on our side as `tass-config-session-source` / relates to `tass-refresh-coordination`.
- Our chosen mitigation is the **borrow model**: one live `CredentialSession`, lent by `&self` to all consumers (`QuintClient::new(&session)`), so we never run multiple sessions over one store. This sidesteps the multi-instance escalation but does **not** fix the single-instance concurrent-refresh edge — which is exactly what this upstream issue asks jacquard to fix.
