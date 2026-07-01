# Discovery: tass auth CLI surface + session store

> **This document is a subagent prompt.** It is not a design doc — it is a
> detailed brief for an investigative agent that surveys prior art and
> reports back. The findings will inform a separate design conversation
> between rektide and the primary assistant.

## Mission

Investigate how five existing atproto CLIs (and one non-atproto reference)
structure their **auth / login / session-management commands**, then
brief us on Jacquard's auth internals so we can design tassle's auth CLI
surface and session-store schema.

We have two open design questions:

1. **Auth CLI surface** — what subcommands should `tass auth ...` expose?
   How should top-level aliases (`tass login` ≡ `tass auth login`) work?
   What conveniences / modes / output formats are worth copying?
2. **Auth data storage** — what does the session store actually need to
   hold? What's the right shape for a SQLite/Turso schema that backs it?
   (Final schema design happens in a follow-up conversation; this agent
   should surface the requirements and trade-offs, not propose a final
   schema.)

## Context — what tassle is

Tassle is a CLI + (later) web app for tracking Mage: The Ascension
quintessence / tass energy flows on atproto. Records live under
`com.superbfowle.tass.*` lexicons. The project started as TypeScript
(gunshi CLI, hedystia web server planned), but is migrating to **Rust
using [Jacquard](https://github.com/rsform/jacquard)** as the canonical
atproto toolkit. See [`doc/jacquard-use.md`](../jacquard-use.md) for the
full rationale.

The current Rust skeleton lives at `crates/tass-cli/` with a single
`generate node` subcommand. No auth yet. Auth is the next layer.

**Hard constraints**:
- Jacquard is the canonical atproto library. Use `jacquard-oauth` /
  `jacquard-identity` / `jacquard::client` — do not re-implement OAuth
  primitives.
- The CLI uses clap v4 derive (per repo convention; see existing
  `crates/tass-cli/src/main.rs`).
- Auth subcommands live under `tass auth <verb>` (e.g.
  `tass auth login`). Top-level aliases like `tass login` are
  **hidden from `--help`** but functional. (Concretely: clap subcommand
  with `#[command(hide = true)]` alias, or flatten both routes to the
  same handler.)
- **Turso (libsql) for Rust** is the preferred session-store backend —
  it's SQLite-compatible so a future hedystia (Bun/TS) web server can
  read the same DB. **Open a follow-up ticket** in your report for
  verifying Turso ⇄ hedystia SQLite interop; we don't need to test it now
  but the schema must be designed with that portability in mind.

**Soft preferences / nice-to-haves**:
- Multi-account support (login as more than one DID, switch between).
- Machine-readable output (`--json`) for every auth command, for
  scripting and for the eventual web server to consume.
- CBOR output mode (consistent with the rest of the CLI).
- Interop with the existing TS-side `~/.config/tassle/` file store would
  be lovely but is not required — we can rebuild afresh.

## Reference projects to investigate

All paths are in `~/archive/`. For each, examine the actual CLI surface
(command definitions, subcommand structure) and the auth/session code.
Return file:line references.

### 1. `~/archive/disnet/skyboard/` (TS, gunshi-adjacent)

- CLI in [`cli/`](file:///home/rektide/archive/disnet/skyboard/cli/) —
  uses `commander` (not gunshi), but the command shapes and auth flow
  are directly portable.
- Auth code in
  [`cli/src/lib/auth.ts`](file:///home/rektide/archive/disnet/skyboard/cli/src/lib/auth.ts)
  and
  [`cli/src/lib/config.ts`](file:///home/rektide/archive/disnet/skyboard/cli/src/lib/config.ts).
- Loopback OAuth via `@atproto/oauth-client-node`; file-backed
  `sessionStore` and `stateStore`.
- Tassle's current TS implementation is essentially a port of this. We
  know it well; include for completeness, not novelty.

### 2. `~/archive/alice.mosphere.at/create-tangled-repo/` (TS, small)

A small Node.js script that bootstraps a new tangled repository. Not a
full CLI but likely handles credential setup (app password or OAuth) for
first-run. Look at:

- [`create-tangled-repo.js`](file:///home/rektide/archive/alice.mosphere.at/create-tangled-repo/create-tangled-repo.js)
- [`package.json`](file:///home/rektide/archive/alice.mosphere.at/create-tangled-repo/package.json)

Specifically: does it do auth at all, or does it require pre-existing
creds? What's the bootstrap story?

### 3. `~/archive/markbennett.ca/tangled-cli/` (TS, full CLI)

A full Tangled CLI in TypeScript. Has `lexicons/`, `tests/`,
`API_ANALYSIS.md`. Look at:

- [`src/`](file:///home/rektide/archive/markbennett.ca/tangled-cli/src/) —
  command structure, auth flow
- [`CLAUDE.md`](file:///home/rektide/archive/markbennett.ca/tangled-cli/CLAUDE.md)
  and
  [`API_ANALYSIS.md`](file:///home/rektide/archive/markbennett.ca/tangled-cli/API_ANALYSIS.md)
  for documented intent
- [`README.md`](file:///home/rektide/archive/markbennett.ca/tangled-cli/README.md)

Especially interesting if it has multi-account support, switching,
listing, etc.

### 4. `~/archive/did:plc:5rtpn23tmq5jocptcbkooj4b/tangled-cli/` (Rust)

A **Rust** tangled CLI. Most directly relevant since we're going Rust.
Workspace with `crates/`, `lexicons/`, `docs/`. Look at:

- [`Cargo.toml`](file:///home/rektide/archive/did:plc:5rtpn23tmq5jocptcbkooj4b/tangled-cli/Cargo.toml)
  and `crates/` for workspace layout
- [`AGENTS.md`](file:///home/rektide/archive/did:plc:5rtpn23tmq5jocptcbkooj4b/tangled-cli/AGENTS.md)
- [`README.md`](file:///home/rektide/archive/did:plc:5rtpn23tmq5jocptcbkooj4b/tangled-cli/README.md)
- Find the clap command definitions and the auth/session modules

**Critical question**: does this Rust CLI use Jacquard, atrium-rs, or
something else? If Jacquard, we have a direct architectural sibling to
compare against.

### 5. `gh` — GitHub CLI (external, non-atproto reference)

`gh auth` is widely considered a gold-standard CLI auth surface.
Binaries are pre-installed (`which gh`). Examine:

- `gh auth --help` and all subcommand `--help` outputs
- `gh auth status` behavior (multi-account, token来源 detection)
- `gh auth login` interactive flow (device code, browser, token paste)
- `gh auth refresh` for scope expansion
- `gh auth token` for piping into other tools
- The config file format at `~/.config/gh/hosts.yml`

This is our reference for **polished UX**. We don't need to copy it, but
we should know what good looks like.

### 6. Jacquard itself

[`~/archive/rsform/jacquard/`](file:///home/rektide/archive/rsform/jacquard/)
— our chosen atproto toolkit. Investigate in depth:

- `crates/jacquard-oauth/src/` — the OAuth primitives:
  - [`session.rs`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard-oauth/src/session.rs)
    — session shape
  - [`authstore.rs`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard-oauth/src/authstore.rs)
    — auth store trait
  - [`loopback.rs`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard-oauth/src/loopback.rs)
    — loopback server
  - [`scopes.rs`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard-oauth/src/scopes.rs)
    — scope/permission-set model
  - [`resolver.rs`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard-oauth/src/resolver.rs)
    — OAuth metadata resolution
  - [`dpop.rs`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard-oauth/src/dpop.rs)
    — DPoP key handling
- `crates/jacquard/src/client/` — high-level session types:
  - [`credential_session.rs`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard/src/client/credential_session.rs)
    — app-password bearer auth
  - [`bff_session.rs`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard/src/client/bff_session.rs)
  - `OAuthSession` type (probably in `client.rs`)
- [`crates/jacquard-identity/`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard-identity/)
  — handle/DID/PDS resolution
- [`examples/`](file:///home/rektide/archive/rsform/jacquard/examples/)
  — look for `oauth_timeline.rs`, `app_password_create_post.rs`, etc.
- [`crates/jacquard/Cargo.toml`](file:///home/rektide/archive/rsform/jacquard/crates/jacquard/Cargo.toml)
  — feature flags `loopback`, `streaming`, etc.
- [`llms.txt`](file:///home/rektide/archive/rsform/jacquard/llms.txt) —
  the 1000-line orientation doc; read it for the OAuth section
  especially.
- [`AGENTS.md`](file:///home/rektide/archive/rsform/jacquard/AGENTS.md)
  — conventions, including auth patterns.

Reference the published upstream at
[github.com/rsform/jacquard](https://github.com/rsform/jacquard) for any
canonical URLs in your report.

## Three investigation areas

### Area A — Auth CLI surface survey

For each of the 5 CLIs above (skyboard, create-tangled-repo,
markbennett tangled-cli, did:plc:5rtpn23tmq5jocptcbkooj4b tangled-cli,
gh), document:

1. **Command tree** — every auth-related subcommand, with the full
   invocation pattern (e.g. `gh auth login --hostname ... --git-protocol
   https --web`).
2. **Args and flags** — required positionals, options, defaults,
   env-var fallbacks.
3. **Login flow** — interactive (browser, device code, paste token) vs
   non-interactive. What happens on first run vs subsequent runs.
4. **Session persistence** — file path, format (JSON/YAML/KDL),
   permissions, encryption-at-rest?
5. **Multi-account** — supported? If yes, how is the active account
   selected (default DID, `--user` flag, workspace concept)?
6. **Status / whoami** — what does the "show me who I am" command
   output? Human format + machine format (JSON)?
7. **Scope expansion** — how does the CLI handle "we need a new scope
   that wasn't in the original grant"? Re-login? Refresh?
8. **Logout** — single account, all accounts, keeps the refresh token?
9. **Token export** — `gh auth token` style for piping.
10. **Bells and whistles** — anything notably polished, surprising, or
    clever. Switching accounts via `use`, sessions scoped to a project
    dir, etc.

Return this as a comparison table where possible, then a per-CLI
narrative for the unique bits.

### Area B — Jacquard auth briefing

This is the most important section. We're going to build on Jacquard so
we need to understand exactly what it provides and what we have to
build ourselves.

For Jacquard specifically, answer:

1. **What session types does it have?**
   - `OAuthSession` — when to use, what it stores, lifetimes
   - `CredentialSession` — when to use (app password fallback)
   - `BffSession` — what is BFF (backend-for-frontend)?
   - `AgentSession` — the higher-level wrapper
2. **What's the session-store trait shape?**
   - Exact trait signature (methods, asyncness, generics)
   - What a `SessionStore` impl needs to provide
   - What a `ClientAuthStore` is and how it differs
   - Built-in impls: `MemorySessionStore`, `FileAuthStore`, others?
3. **What does the loopback helper provide?**
   - `LoopbackConfig` shape
   - How `loopback.rs` integrates with `OAuthSession`
   - Whether it handles browser-open, callback server, etc.
4. **What's the scope/permission-set model?**
   - `Scope` type and its builders
   - `IncludeScope`, `LexPermissionSet` — what do these do?
   - How to declare needed scopes for our `com.superbfowle.tass.*`
     collections
5. **What's the auth-store trait?**
   - `authstore.rs` — what shape, what it persists
   - DPoP key storage — is that in the auth store or separate?
   - Token encryption — does Jacquard handle, or our problem?
6. **Identity resolution**
   - `IdentityResolver` trait
   - `JacquardResolver` vs `PublicResolver` — defaults?
   - Caching behavior, feature flags (`dns`, `cache`)
7. **Examples**
   - Read the OAuth examples in `examples/` end-to-end
   - Document the minimum viable login → first XRPC call flow
   - Note any example that does multi-account or session switching
8. **Feature flags**
   - What `features = [...]` do we need on the `jacquard` dep for a CLI
     that does OAuth loopback + PDS reads/writes?
   - Anything WASM-only we should avoid?

### Area C — Data store requirements

Do **not** propose a final schema — that's for the next conversation.
Instead, enumerate what the store needs to hold, with concrete pointers
into Jacquard's code where each field comes from.

1. **Per-account data**:
   - DID (primary key?)
   - Handle (denormalized; can change)
   - PDS endpoint (from DID doc)
   - OAuth sub (subject from token endpoint)
   - DPoP key (serialized how? JWK?)
   - Access token + expiry
   - Refresh token + expiry
   - Granted scopes (the original scope string + parsed set)
   - Last-seen timestamp
   - Client_id used at auth time (loopback port encoded — see skyboard
     [`auth.ts`](file:///home/rektide/archive/disnet/skyboard/cli/src/lib/auth.ts)
     for why this matters)
2. **Per-PDS / per-auth-server metadata**:
   - Authorization endpoint URL
   - Token endpoint URL
   - DPoP nonce cache (replay protection)
   - Signed metadata (PAR)
3. **CSRF/PKCE state** (transient, but stored between redirect and
   callback):
   - State token → PKCE verifier mapping
   - Created-at for expiry (typically 10 min)
4. **Replay protection** (if we run a web tier later):
   - `jti` claim cache — see jacquard-axum's `ReplayStore`

For each, note:
- Sensitivity (token = high, PDS endpoint = low)
- Write frequency (refresh tokens = often, DID = never)
- Whether Jacquard already has a serialization format we should match
- Whether it's per-account or global

Also investigate:
- What format does Jacquard's `FileAuthStore` use on disk? (So a
  Turso/SQLite port preserves semantics.)
- Is the DPoP key stored as raw bytes, JWK, PEM, or something else?
- How does Jacquard handle concurrent token refresh across processes?
  (Locking? `requestLock`?)

## Constraints to respect

- **Canonical toolkit**: Jacquard
  ([github.com/rsform/jacquard](https://github.com/rsform/jacquard)).
  Don't suggest atrium-rs unless comparing-for-contrast.
- **Hidden alias pattern**: every auth verb has both a canonical
  `tass auth <verb>` form and a top-level alias `tassle <verb>` that
  is hidden from `--help`. Show how this is done in clap v4 derive.
- **Turso for the session store**: schema must be SQLite-compatible
  (no Postgres-only types). Open a follow-up ticket in the report for
  hedystia (TS/Bun) reading the same DB via libsql.
- **Multi-account**: tassle should support being logged in as more
  than one DID at once, with a notion of "active" account. The CLI
  then takes an optional `--account <did-or-handle>` flag everywhere
  that's account-scoped.
- **No re-implementation** of OAuth primitives. Use `jacquard-oauth`.
  If Jacquard doesn't provide something (e.g. encrypted-at-rest
  tokens), note it as a gap, don't design around it.

## Out of scope for this investigation

- The lexicon / ontology restructure (canonical Mage resonances,
  layers.pub ontology pattern, multi-domain authority). Tracked
  separately.
- Implementation work. This is research only.
- Migrating the existing TS CLI commands to Rust. We'll do that
  incrementally after auth lands.

## Expected deliverables

Return a single structured markdown report with these sections:

1. **Executive summary** (3 paragraphs max):
   - What the surveyed CLIs converge on
   - Where they diverge
   - One-sentence recommendation for tassle's auth command count and
     shape

2. **Comparison table**: every auth-related command observed, with
   columns: command, what it does, which CLIs have it, notes.

3. **Per-CLI narrative**: a short section per CLI on its notable design
   choices. Skip skyboard (we know it).

4. **Jacquard auth briefing** (longest section):
   - Session type decision tree (when to use which)
   - The `SessionStore` trait shape and built-in impls
   - The minimum-viable login flow as code sketch (using Jacquard APIs,
     30-50 lines)
   - Feature flags we need
   - Gaps in Jacquard we'd need to fill ourselves

5. **Data store requirements** (Area C above): bulleted enumeration
   with sensitivity/frequency/Jacquard-format columns. No final
   schema.

6. **Recommended `tass auth` surface**: your proposed command list,
   one line each. Mark which are required-for-MVP vs nice-to-have.

7. **Top-level aliases** to expose (hidden from `--help`):
   e.g. `tass login`, `tass whoami`, `tass logout`.

8. **Bells and whistles worth copying**: a small list of polish items
   that aren't strictly required but seem worth the effort.

9. **Open questions for the design conversation**: things you couldn't
   answer from the archive and that rektide and the primary assistant
   need to decide.

10. **Tickets to file**: hedystia/Turso interop, any Jacquard gaps,
    examples we should write, etc. — as a checklist.

Cite file:line for every claim about a specific codebase. Link to
canonical upstream URLs (github.com/rsform/jacquard, etc.) where
relevant.

**Do not** write any code, modify any files, or propose final designs.
Research only. The follow-up design conversation will use your report
as input.
