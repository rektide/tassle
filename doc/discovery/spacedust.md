# Spacedust + jetstream: the tassle listener daemon

> tassle's primary interface is **atproto posts addressed at us**: a person posts something like *"burn my tass"* at our Mage account, and a standalone listener daemon reacts — reads their records, performs the action, and posts an attestation. That command stream comes from [Spacedust](https://www.microcosm.blue/), microcosm's "configurable ATProto notifications firehose." A *second* source — a [jetstream](https://github.com/bluesky-social/jetstream) firehose of `com.superbfowle.tass.*` records **at large** — feeds the ledger fold so we always know each tass's real remaining quintessence. The daemon is **fjall-native** (hydrant is deleted) and built around two-level **sources + listeners**. Epic: [`tass-listener-svc`](#tickets). Sibling doc: [attestation.md](attestation.md) (cosign *trust*, distinct from this doc's action receipts). Source studied: `~/archive/microcosm.blue/microcosm-rs/{spacedust,links,jetstream}`.

---

## 1. Two sources, two jobs

There are two genuinely different event sources, and conflating them was the original mistake in this doc:

| Source | Mechanism | Yields | Job |
| --- | --- | --- | --- |
| **posts-at-us** | Spacedust `wantedSubjectDids=<MAGE_DID>` | link *pointers* → hydrate | the **command** interface ("burn my tass", "meditate") |
| **tass-at-large** | jetstream `wantedCollections=com.superbfowle.tass.*` | full record bodies, cursor replay | the **ledger fold** (every enervate/tassilize/meditate, network-wide) |

`enervate` records authored by **anyone, anywhere** are an event we track — independent of whether someone is talking *at* us — because the fold needs them to keep each tass's balance correct. The command handler that *writes* an enervate and the fold that *observes* enervates everywhere are two consumers of two different sources. Both normalize to one `EventSource` envelope (`tass-sync-source`).

> **Why not hydrant for tass-at-large?** Hydrant was deleted; tassle is fjall-native. The firehose source is now a plain `jetstream` consumer — the same crate Spacedust and Slingshot already use — with cursor-based replay. This also erased the fjall version-skew hazard hydrant introduced, which is what makes the shared-store design in §7 easy.

---

## 2. What Spacedust is, mechanically

A WebSocket service. One real endpoint: `GET /subscribe` (upgraded to a WS channel — `server.rs:317`). It consumes jetstream internally, runs every record through `collect_links` (walks the record JSON, emits one event per link/reference — `consumer.rs:89`, `links/src/record.rs`), and fans those link-events out to subscribers who filter for the ones they care about. The framing that makes this work: *all social interactions in atproto are links* — a reply, mention, quote, like, follow is a record whose link **target** carries some DID. "A post at us" = "a link whose target carries our DID."

### The `/subscribe` filters (the "at us" mechanism)

Query params (also live-updatable by sending a JSON text frame on the open socket — `subscriber.rs:154`):

| Param | What it matches | Limit | Tassle use |
| --- | --- | --- | --- |
| `wantedSubjectDids` | DIDs to receive links about — bare-DID links **and** DIDs extracted from at-uri targets (`lib.rs:40`) | 10 000 | **Primary filter.** `wantedSubjectDids=<MAGE_DID>` catches everything aimed at us. |
| `wantedSubjectPrefixes` | prefix match on the target | 100 | `at://<MAGE_DID>/` for links to any of our records; narrower scoping. |
| `wantedSubjects` | exact target at-uri / uri / DID | 50 000 | watch one "command thread" post where every reply is a command. |
| `wantedSources` | link **source** = `<collection NSID>:<dotted path>` (`lib.rs:48`) | 1 000 | narrow to *textual commands only* (posts), dropping likes/follows. |

Filter logic (`subscriber.rs:123`): the three subject filters are **OR**'d; that group is **AND**'d with `wantedSources`; empty group = match-all. So "posts that mention or reply to us" =

```
wantedSubjectDids = <MAGE_DID>
        AND
wantedSources    = { "app.bsky.feed.post:reply.parent.uri",
                     "app.bsky.feed.post:facets[].features[app.bsky.richtext.facet#mention].did" }
```

The `instant` scalar param bypasses Spacedust's default **21-second delay buffer** (`server.rs:299`), which exists so a post-then-delete never fires. We want the **default delayed** stream: someone who posts and instantly deletes "burn my tass" should not trigger a burn.

### Events are pointers — hydrate them

The payload is a pointer, not the content (`lib.rs:89`): `{operation, source, source_record, source_rev, subject}`, no record body (`lib.rs:96`). To read the command text we **hydrate `source_record`** — Slingshot `getRecordByUri?at_uri=<source_record>` returns `{uri, cid, value}`, with a plain PDS `getRecord` as fallback. So the microcosm tools compose: **Spacedust targets, Slingshot hydrates, Constellation recovers** (§6).

---

## 3. Object model: sources and listeners (two levels)

- A **Source** is a live connection to a feed (the Spacedust WS subscription; the jetstream subscription). It owns transport, cursor, reconnect, and produces normalized `EventSource` events. You can toggle a *whole source* off.
- A **Listener** is a named reaction *bound to a source*: `{ name, source, matcher, action_chain, reads, writes, verbosity }`. This is the unit you independently enable/disable and tune. **Many listeners share one source** (several command listeners on Spacedust; the ledger-fold listener on jetstream).

Toggling happens at **both** levels: kill a source (stop connecting) or disable a listener (source stays up, that reaction stops).

### Per-listener knobs (orthogonal, not a rolled-up mode)

| Knob | Controls | Values |
| --- | --- | --- |
| `verbosity` | how much this listener logs (see §5) | `silent` / `summary` / `verbose` |
| `reads` | API reads — hydrate the post, resolve character/tass, look up balances | `off` / `on` |
| `writes` | API writes — create records, post replies | `off` / `own` / `all` |

Behavior falls out of the product — **"dry-run" is just `reads=on, writes=off`**, not a special mode:

```
reads=off, writes=off  → pure match: "saw a burn-tass command", nothing else
reads=on,  writes=off  → dry-run: hydrate, resolve, compute the would-be effect, log it
reads=on,  writes=own  → enact records into our repo, no public reply
reads=on,  writes=all  → records + reply to the user's post
```

Example config (two-level; Spacedust endpoint configurable, public default):

```toml
[sources.spacedust]
enabled  = true
endpoint = "wss://spacedust.microcosm.blue/subscribe"   # configurable; confirm host
account  = "did:plc:MAGE"          # → wantedSubjectDids

[sources.jetstream]
enabled  = true                     # tass-at-large fold

[listeners.burn-tass]
source    = "spacedust"
reads     = "on"
writes    = "off"                   # ship as dry-run
verbosity = "verbose"

[listeners.meditate]
source    = "spacedust"
reads     = "on"
writes    = "all"

[listeners.enervate-fold]           # enervates AT LARGE → ledger
source    = "jetstream"
reads     = "on"
writes    = "off"                   # fold only touches the local ledger
```

---

## 4. The command pipeline (action chain)

A listener that matches runs an ordered **action chain** of steps that each run / skip / short-circuit. For v1.1 the chains are **static, in code** (typed `Vec<Step>`), shaped to grow later into a named-step registry. Matching is **keyword spotting**.

```
spacedust event → matcher (keyword spot) → [ resolve character → resolve tass
   → authorize → write action record → write attestation → reply ] → wide event
```

### "burn my tass" → enervate (primary)

1. **Match** "burn (my) tass" in the hydrated post text.
2. **Resolve character** (multi-mage problem): a user may own several mage records (`actor.rpg.stats` rkeys). Pick the one whose **character-name word** appears in the message.
3. **Resolve tass** (multi-tass): match a tass `form`/name word in the message; **if unspecified, pick a random** owned tass (`tassilize.json`: `form`, `quintessence`).
4. **Authorize**: a user may only burn their **own** tass. (Simple and sufficient for v1.1.)
5. **Enact**: write a `com.superbfowle.tass.enervate` (`tass` at-uri of the user's tassilize, `amount`) — gated by `writes`. **For now, enervate implies the user as the effect target** (you burn your own tass); routing burned quintessence to a recipient (the Mage or another party) is the future, more sophisticated model in [`tass-recipient-alloc`](#tickets).
6. **Attest**: post an attestation in our NSID recording the change (see §8 / [`tass-attest-age-payload`](#tickets)).
7. **Reply**: optional, only when `writes=all`.

### "meditate" → meditate (MVP v1.1 second flow, and the last)

Keyword-spot "meditate" → resolve character + target Node → write `com.superbfowle.tass.meditate` (`node` at-uri, `amount` 0–20). Deferred completion (effect after an in-fiction duration) is modeled as a **due job** (§6). Shares the character/tass resolver and the chain runtime.

Everything else (tassilize, resonance, forms, cosigns) is out of scope for v1.1.

### Writing as the Mage — reuse the auth engine

Writes authenticate as the Mage via the **existing** `tassle-config` auth engine: `AuthedClient::for_profile(mage)` opens the `jac-store-fjall` session store, resumes a `CredentialSession`, points it at the PDS, and **lends** `&session` (the borrow model — `AuthedClient` is deliberately not `Clone`). The daemon is the first long-running, concurrent, autonomous consumer of that session, so it is also the real stress test for cross-task token refresh (`tass-refresh-coordination`); it consumes a `SessionSource`, not a raw client.

---

## 5. Wide-event tracing

One canonical **wide-event** log line per processed event, at **INFO**, assembled across a per-listener span: `source, listener, matched, mode, actor, command, target_uri, steps, effect_uris, dry_run, dedupe_key, latency_ms, outcome/error`. It always emits — that's the point of it.

`verbosity` is a **per-listener filter directive**, not a suppressor of the wide event. Each listener runs inside a span tagged with its name (`span!(target: "tassle::listener", listener = "burn-tass")`); verbosity scopes how much *extra* prints:

- `silent` → only the INFO wide event.
- `summary` → + key spans.
- `verbose` → + per-step DEBUG events with payloads.

Mechanically this is `tracing-subscriber` `EnvFilter` span-field scoping — `tassle::listener[{listener=burn-tass}]=debug` turns up one listener while others stay at their wide event. **Per-layer filters** (`Layer::with_filter`) let different sinks show different subsystems ("a trace shows for one subsystem but not another"), and `tracing_subscriber::reload` allows live verbosity changes from config. ([`tass-wide-event`](#tickets).)

---

## 6. Backfill when we drop offline — two mechanisms

- **jetstream (tass-at-large): real replay.** The source takes a **cursor**; `connect_cursor(Some(cursor))` resumes from the last *durably committed* cursor (and `Some(0)` is a full rebuild). The fold advances the cursor only after the fjall commit succeeds, so it never loses tass events across a restart.
- **Spacedust (posts-at-us): no replay → catch up via Constellation.** On reconnect, enumerate posts that referenced the Mage during the gap from the [Constellation](https://constellation.microcosm.blue/) backlink index (`/links?target=<MAGE_DID>&collection=app.bsky.feed.post&path=.reply.parent.uri`, plus the mention path), replay them through the listeners, and **dedupe by idempotency key** (`source_record`+`source_rev`) so a command never double-burns. ([`tass-backfill-constellation`](#tickets).)

Idempotency lives in the job store (§7) — the same mechanism that makes the chain durable also makes backfill safe.

---

## 7. fjall, the job mechanism, and a shared store

tassle is fjall-native, and `jac-store-fjall` is growing **DB builders** that expose the database-instance creation formerly used internally. That gives us a clean **`StoreProvider`** ([`tass-store-provider`](#tickets)) that hands out fjall partition/keyspace handles to sessions, the ledger, and jobs from one data root. Because hydrant is gone, every consumer resolves fjall from crates.io and stays version-aligned, so a single shared keyspace with handle-passing is straightforward — no version reconciliation.

The **durable job queue** ([`tass-job`](#tickets)) is extracted into its own crate: `Job = {inputUri, inputCid, kind, dueAt, status, attempts}`, enactment idempotent by `inputUri+inputCid+effectKind`, a worker enacting at `dueAt` with retry/backoff. It is three things at once: the action-chain's deferred/retryable step runtime, the dedupe ledger for backfill/replay, and a reusable scheduler for future work (meditate completion, node regen).

---

## 8. Attestation output

After acting, the Mage posts an **attestation record in our own NSID** referencing the action record and the affected user record, capturing the **before → after** of the changed quantity. This is a lightweight *action receipt*, distinct from the *cosign trust* options in [attestation.md](attestation.md). The one concrete improvement already ticketed ([`tass-attest-age-payload`](#tickets)): carry an **AGE-encrypted** payload describing exactly what changed, so the affected user (key holder) can verify the diff without the raw ledger numbers being published to the firehose.

---

## 9. Crate decomposition

Granular, so the engine is a library reusable by the daemon and (later) the CLI:

```
tass-sync-source        EventSource trait + normalized envelope
tass-source-spacedust   WS subscribe + reconnect + hydration   → impls the trait
tass-source-jetstream   tass-at-large firehose + cursor         → impls the trait
tass-job                durable due-job queue + worker (reusable)
tass-store-provider     shared fjall handles via jac-store-fjall builders
tass-engine             listener registry, keyword matcher, action-chain runtime, knobs, wide-event
tass-listend            standalone service binary (sources + engine + worker + config)
tassle-cli              thin, feature-gated wiring → same engine (tassle listen / worker)   [low-pri]
tassle-config           extended with the two-level source/listener config schema
```

`tass-engine` sits above `tassle-ledger` + `tass-quint`; Mage auth comes from `tassle-config`'s `AuthedClient`/`SessionSource`.

---

## 10. Open questions

1. **Confirm the Spacedust public host** (`spacedust.microcosm.blue`?) and decide self-host vs. public for a load-bearing primary interface (upstream is best-effort uptime).
2. **Command grammar** beyond keyword spotting — disambiguation when multiple character/tass words match, or none.
3. **Recipient model** ([`tass-recipient-alloc`](#tickets)): when we move past "enervate implies the user," what does controlled allocation look like — a recipient field on enervate, a separate transfer record, authorization?
4. **Cross-task refresh** ([`tass-refresh-coordination`](#tickets)) under the long-running daemon — the borrow-model session shared across handler tasks.
5. **AGE payload** key holder + discovery ([`tass-attest-age-payload`](#tickets)).

## Tickets

Epic **`tass-listener-svc`** — *Configurable listener daemon (Spacedust commands + tass-at-large fold)*. Children:

- `tass-sync-source` — EventSource trait + normalized envelope
- `tass-source-spacedust` — Spacedust WS source + hydration
- `tass-source-jetstream` — jetstream tass-at-large source + cursor replay
- `tass-job` — reusable durable job queue + worker (fjall, idempotent)
- `tass-store-provider` — shared fjall StoreProvider via jac-store-fjall builders
- `tass-engine` — registry + keyword matcher + action-chain runtime + knobs
- `tass-listener-config` — two-level sources + per-listener knobs
- `tass-wide-event` — wide-event tracing + per-listener verbosity filtering
- `tass-target-resolve` — resolve character + tass from message words
- `tass-handler-burn` — burn-tass handler (primary)
- `tass-handler-meditate` — meditate handler (MVP v1.1 second flow)
- `tass-backfill-constellation` — Constellation catch-up + idempotent dedupe
- `tass-ledger-fold` — ledger fold over the EventSource stream
- `tass-listend` — standalone service binary
- `tass-cli-listen` — CLI integration (low priority)
- `tass-recipient-alloc` — controlled allocation to recipients (future)
- `tass-attest-age-payload` — AGE-encrypted before→after on attestations

(`tass-embed-hydrant` is closed — hydrant deleted.)

## See also

- [attestation.md](attestation.md) — cosign *trust* options (distinct from §8's action receipts).
- [ledger.md](../ledger.md) — the per-DID fold the jetstream source feeds.
- `~/archive/microcosm.blue/microcosm-rs/spacedust/` — `server.rs` (subscribe + params), `subscriber.rs` (filters), `consumer.rs` + `links/src/record.rs` (link extraction), `lib.rs` (payload).
