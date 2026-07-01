# Spacedust + jetstream: the tassle listener daemon

> tassle's primary interface is **atproto posts addressed at us**: a person posts something like *"burn my tass"* at our Mage account, and a standalone listener daemon reacts — reads their records, performs the action, and posts an attestation. That command stream comes from [Spacedust](https://spacedust.microcosm.blue/), microcosm's "configurable ATProto notifications firehose." A *second* source — a **jetstream** firehose of `com.superbfowle.tass.*` records **at large** — feeds the ledger fold so we always know each tass's real remaining quintessence. The daemon leans on our own crates: **`tass-phase`** models the work, **`tassle-config`** provides jacquard auth, and a shared local **turso** database backs the auth store and (later) job persistence. Epic: [`tass-listener-svc`](#tickets). Sibling: [attestation.md](attestation.md) (cosign *trust*, distinct from §8's action receipts).

---

## 1. Two sources, two jobs

Two genuinely different event sources — conflating them was the original mistake here:

| Source | Mechanism | Yields | Job |
| --- | --- | --- | --- |
| **posts-at-us** | Spacedust `wantedSubjectDids=<MAGE_DID>` | link *pointers* → hydrate | the **command** interface ("burn my tass", "meditate") |
| **tass-at-large** | jetstream `wantedCollections=com.superbfowle.tass.*` | full record bodies, cursor replay | the **ledger fold** (every enervate/tassilize/meditate, network-wide) |

`enervate` records authored by **anyone, anywhere** are an event we track — independent of whether someone is talking *at* us — because the fold needs them to keep every tass's balance correct. The command handler that *writes* an enervate and the fold that *observes* enervates everywhere are two consumers of two different streams. Both normalize to one `EventSource` envelope (`tass-sync-source`).

The tass-at-large source uses **jacquard's own jetstream** (`jacquard_common::jetstream`: `JetstreamParams`, `TungsteniteSubscriptionClient`/`SubscriptionClient`, `JetstreamMessage`), per `jacquard/examples/subscribe_jetstream.rs` — not the microcosm jetstream crate — so the whole daemon stays on the jacquard stack. (Hydrant was deleted; tassle is turso-native now, which also erased the old fjall version-skew hazard.)

---

## 2. What Spacedust is, mechanically

A WebSocket service. One real endpoint: `GET /subscribe` (upgraded to a WS channel — `server.rs:317`). It consumes jetstream internally, runs every record through `collect_links` (walks the JSON, emits one event per link/reference — `consumer.rs:89`, `links/src/record.rs`), and fans those out to subscribers who filter. Framing: *all social interactions in atproto are links* — a reply, mention, quote, like, follow is a record whose link **target** carries a DID. "A post at us" = "a link whose target carries our DID."

### The `/subscribe` filters (the "at us" mechanism)

Query params (also live-updatable by a JSON text frame on the open socket — `subscriber.rs:154`):

| Param | What it matches | Limit | Tassle use |
| --- | --- | --- | --- |
| `wantedSubjectDids` | DIDs to receive links about — bare-DID links **and** DIDs from at-uri targets (`lib.rs:40`) | 10 000 | **Primary filter.** `wantedSubjectDids=<MAGE_DID>` catches everything aimed at us. |
| `wantedSubjectPrefixes` | prefix match on the target | 100 | `at://<MAGE_DID>/` for links to any of our records. |
| `wantedSubjects` | exact target at-uri / uri / DID | 50 000 | watch one "command thread" post. |
| `wantedSources` | link **source** = `<collection NSID>:<dotted path>` (`lib.rs:48`) | 1 000 | narrow to *posts only*, dropping likes/follows. |

Filter logic (`subscriber.rs:123`): the three subject filters are **OR**'d; that group is **AND**'d with `wantedSources`; empty = match-all. "Posts that mention or reply to us" = `wantedSubjectDids=<MAGE_DID>` **AND** `wantedSources ∈ { app.bsky.feed.post:reply.parent.uri, app.bsky.feed.post:facets[].features[app.bsky.richtext.facet#mention].did }`. We keep Spacedust's default **21-second delay buffer** (`server.rs:299`), not `instant`, so a post-then-delete never fires a burn.

### Events are pointers — hydrate them

The payload is a pointer, not the content (`lib.rs:89`): `{operation, source, source_record, source_rev, subject}`, no body (`lib.rs:96`). To read the command text we **hydrate `source_record`** — Slingshot `getRecordByUri?at_uri=<source_record>` → `{uri, cid, value}`, PDS `getRecord` fallback. So the microcosm tools compose: **Spacedust targets, Slingshot hydrates, Constellation recovers** (§6).

---

## 3. Object model: one stream per source, many command handlers

A **Source** is a live connection to a feed (the Spacedust WS via `wantedSubjectDids`; the jetstream subscription). It owns transport, cursor, reconnect, and produces normalized events. **A source stream is shared — command handlers do *not* each open their own stream.** A **dispatcher** consumes the source stream and, per event, keyword-spots which command applies and spawns the matching **command handler** (a `tass-phase` job, §4) onto a shared **Executor**.

So there is no heavyweight "Listener owns a stream" object — that was the confusion. There are: **Sources** (streams), a **dispatcher** (event → which command), **command handlers** (the FSMs that do the work), and the **Executor** (runs them concurrently). Toggling is two-level: disable a *source* (stop the stream) or disable a *command handler* (dispatcher won't spawn it).

### Per-command knobs (orthogonal, not a rolled-up mode)

Config attaches to each **command handler**, not to a stream:

| Knob | Controls | Values |
| --- | --- | --- |
| `verbosity` | how much this handler logs (§5) | `silent` / `summary` / `verbose` |
| `reads` | API reads — hydrate the post, resolve character/tass, read balances | `off` / `on` |
| `writes` | API writes — create records, post replies | `off` / `own` / `all` |

**"dry-run" = `reads=on, writes=off`** — not a special mode:

```
reads=off, writes=off → pure match: "saw a burn-tass command", nothing else
reads=on,  writes=off → dry-run: gather + resolve + compute the would-be effect, log it
reads=on,  writes=own → enact records into our repo, no public reply
reads=on,  writes=all → records + reply to the user's post
```

Config lives in `tassle-config` under `[service.listen]`, with **nested per-verb tables that fall through** to the service level (reusing the existing figment2 profile-fallback helper — `select_profile_from_config` / `DropIns`). So `writes` control is granular: a service-level default, overridden per verb.

```toml
[service.listen]                    # service-level defaults (fall-through parent)
account   = "did:plc:MAGE"          # → Spacedust wantedSubjectDids
endpoint  = "wss://spacedust.microcosm.blue/subscribe"   # confirmed host; configurable
reads     = "on"
writes    = "off"                   # default posture: dry-run
verbosity = "summary"

[service.listen.enervate]           # "burn my tass" — falls through to [service.listen]
writes    = "own"                   # override just this verb's writes
verbosity = "verbose"

[service.listen.meditate]
writes    = "all"
```

Sources (the two streams) are the daemon's fixed infrastructure; the per-verb tables are what you toggle and tune.

---

## 4. The work model: `tass-phase` (how we're building it)

Each verb is a unit of **phased async work** modeled with the **`tass-phase`** crate — a pure FSM (the *phases*) + an async `Driver` (the I/O) + a concurrent `Executor`. `tass-phase` is a **finished, abstract** library: its only deps are `rust-fsm` and `futures-util`, and its `tests/burn_chain.rs` is a **synthetic illustration** (scripted driver, no network, no clock) — not shipped behavior. The shape:

- **Phases (pure FSM):** states are the phases of the work; inputs advance them; outputs are effects to perform. Short-circuits are just transitions to terminal phases. Fully I/O-free and unit-testable. **One FSM per verb** — each action composes only the steps it needs from a shared effect vocabulary (a menu, not a mandate); there is no generic parent FSM that verbs specialize. Reuse is at the *effect* level.
- **Driver (async bridge):** awaits reality (`next_event`) and performs effects (`effect`). Both take `&mut self`, so **the Driver is the data accumulator** — the pure FSM carries no payload, so `Gather` stashes the fetched mages/tass into the Driver and later steps read them back out. All context lives here: the hydrated post, resolvers, shared turso db, and the **lent `tassle-config` `AuthedClient` session**.
- **Executor:** runs many verb jobs concurrently on one task and streams each result the instant it finishes. This is "a model to track what work needs to happen" — every in-flight command is a job on the Executor.

The **effect vocabulary** (`Gather`, `ResolveTarget`, `Authorize`, `ReadState`, `WriteEffect`, `Attest`, `Reply`) and the `Driver` trait glue live in **`tass-engine` (mechanism, no verbs)**. The verbs themselves are fine-grained crates — **`tass-act-enervate`, `tass-act-meditate`** — each owning its FSM + Driver + domain parameters, depending on `tass-engine` for the vocabulary and on the domain crates (`tassle-ledger` / `tass-quint` / `tassle-config`) for what the effects actually do.

The **"gather dependencies, then search/solve with matchers"** shape maps directly onto the front phases: a *Gather* phase whose effect fetches the actor's context (their mage character records and available tass), then a *Resolve* phase where the matchers run over the message **with that context** to solve for a concrete intent (or short-circuit). Sketch (extending the `burn_chain` FSM):

```
Detected   → Gathering [FetchActorContext]         // gather deps: mage names, available tass
Gathering  → Matched   ⇒ Resolved  |  NoMatch ⇒ Skipped     // solve expression with matchers
Resolved   → Owner     ⇒ Authorized [ReadBalance]  |  NotOwner ⇒ Denied   // authz: own tass only
Authorized → Sufficient⇒ Enacting  [WriteEnervate] |  Insufficient ⇒ Aborted
Enacting   → Wrote     ⇒ Attesting  [WriteAttestation]
Attesting  → Attested  ⇒ Done
```

Matchers in the *Resolve* transition, all keyword spotting over the gathered context:
- **command**: keyword-spot the verb ("burn (my) tass" | "meditate").
- **character** (multi-mage): pick the mage whose **character-name word** appears in the message.
- **tass** (multi-tass): match a tass `form`/name word; **if unspecified, pick a random** owned tass.

### The two v1.1 commands

- **burn-tass (primary)** → the FSM above; writes a `com.superbfowle.tass.enervate` against the user's tassilize. **enervate implies the user as the effect target for now** (you burn your own tass); routing burned quintessence to a recipient is the future model, [`tass-recipient-alloc`](#tickets). Authz: **own tass only**.
- **meditate (second and final v1.1)** → keyword-spot "meditate" → resolve character + Node → write `com.superbfowle.tass.meditate` (`node`, `amount` 0–20). Deferred completion (effect after an in-fiction duration) is the durability seam, [§7](#7-turso-the-work-executor-and-the-refresh-borrow-model).

### Writing as the Mage — reuse `tassle-config`

Writes authenticate as the Mage via the **existing** `tassle-config` auth engine: `AuthedClient::for_profile(mage)` opens the jac-store session (turso backend), resumes a jacquard `CredentialSession`, points it at the PDS, and **lends** `&session`. The Driver borrows that session to perform write effects. The daemon does not implement auth — it consumes `tassle-config`.

---

## 5. Wide-event tracing

One canonical **wide-event** log line per processed command, at **INFO**, assembled across a per-command span: `source, command, matched, mode, actor, target_uri, phases, effect_uris, dry_run, dedupe_key, latency_ms, outcome/error`. It always emits.

`verbosity` is a **per-command filter directive**, not a suppressor. Each command runs inside a span tagged with its name (`span!(target: "tassle::command", command = "burn-tass")`); verbosity scopes the *extra* detail: `silent` = only the wide event; `summary` = + key phase spans; `verbose` = + per-phase DEBUG with payloads. Mechanically, `tracing-subscriber` `EnvFilter` span-field scoping (`tassle::command[{command=burn-tass}]=debug`) turns one command up while others stay at their wide event; **per-layer filters** let different sinks show different subsystems; `tracing_subscriber::reload` allows live changes. ([`tass-wide-event`](#tickets).)

---

## 6. Backfill when we drop offline — two mechanisms

- **jetstream (tass-at-large): real replay.** Cursor-based; resume from the last *durably committed* cursor (`Some(0)` = full rebuild). The fold advances the cursor only after the commit succeeds, so it never loses tass events across a restart.
- **Spacedust (posts-at-us): no replay → catch up via Constellation.** On reconnect, enumerate posts that referenced the Mage during the gap from the [Constellation](https://constellation.microcosm.blue/) backlink index (`/links?target=<MAGE_DID>&collection=app.bsky.feed.post&path=.reply.parent.uri`, plus the mention path), replay them, and **dedupe by idempotency key** (`source_record`+`source_rev`). ([`tass-backfill-constellation`](#tickets).)

The Executor is **in-memory** until [`tass-job-persistence`](#tickets) lands, so a restart drops in-flight command jobs — Constellation re-derives the command work, and idempotent dedupe prevents double-burns.

---

## 7. turso, the work Executor, and the refresh-borrow model

**turso (project-wide).** tassle is switching to turso; `jac-store` now defaults to its native-SQL turso backend (`jac-store-fjall` `backend-turso` / `jac-store-turso`). The daemon owns **one shared local turso database**, built via jac-store's turso builders ([`tass-store-provider`](#tickets)), shared with **jac-store-auth** for jacquard session/`AuthRepository` storage (so refreshed tokens persist) and, later, job persistence.

**Executor + deferred persistence.** For v1.1 the `tass-phase` Executor is **in-memory** — no durable queue. `tass-phase` provides the seam: `FsmJob::resume(phase, driver)` rehydrates a job from a serialized phase. Persisting parked phases into turso plus a `dueAt` scheduler is deferred ([`tass-job-persistence`](#tickets), low-low-low priority). Deferred meditate completion is the first thing that will actually need it.

**The refresh-borrow model (deferred, explained).** A jacquard `CredentialSession` holds an access + refresh token; when the access token expires it refreshes and must persist the new pair. The **borrow model** (`AuthedClient` owns one session, lends `&session`, is deliberately not `Clone`) exists because two *copies* of a session refreshing concurrently would each rotate the refresh token out from under the other and get the account logged out. In the daemon, many Executor jobs write as the Mage, so they all **share one session by reference** (an `Arc<AuthedClient>` lent into each Driver) — refresh happens once, coordinated, and the turso-backed store persists the rotated tokens. That's sufficient for a single-process daemon. **Multi-process** coordination (two daemons sharing one account) is the genuinely hard part and is out of scope for now ([`tass-refresh-coordination`](#tickets)); jac-store's turso `AuthRepository` is expected to help here.

---

## 8. Attestation output

After acting, the Mage posts an **attestation record** in our own NSID referencing the action record and the affected user record, capturing the **before → after** of the changed quantity — a lightweight *action receipt*, distinct from the *cosign trust* options in [attestation.md](attestation.md). Ticketed improvement: an **AGE-encrypted** payload of the diff so the affected user (key holder) can verify it without the raw numbers hitting the firehose ([`tass-attest-age-payload`](#tickets)).

---

## 9. Crate decomposition

```
tass-phase              phased-work FSM + async Driver + Executor      (done, abstract)
tass-sync-source        EventSource trait + normalized envelope
tass-source-spacedust   WS subscribe + reconnect + hydration          → impls the trait
tass-source-jetstream   tass-at-large via jacquard_common::jetstream  → impls the trait
tass-store-provider     one shared local turso db (jac-store builders)
tass-engine             MECHANISM ONLY — source stream → dispatcher → Executor; effect
                        vocabulary + Driver glue; config wiring; wide-event.  NO verbs.
tass-act-enervate       the enervate verb: own FSM + Driver + domain params
tass-act-meditate       the meditate verb: own FSM + Driver + domain params
tass-listen             small standalone binary: load [service.listen], tass_engine::run()
tassle-cli              `tassle listen` behind a feature → same tass_engine::run()   [low-pri]
tassle-config           jacquard auth (AuthedClient) + [service.listen] fall-through config
```

Engine is mechanism; the verbs are `tass-act-*` crates that depend on it and on the domain crates (`tassle-ledger` / `tass-quint`). Mage auth is `tassle-config`; storage is turso. Both the standalone `tass-listen` binary and the `tassle listen` CLI subcommand are thin wrappers over one `tass_engine::run(config)` — a small focused daemon **and** the omni-CLI, from one brain.

---

## 10. Open questions

1. **Command grammar** beyond keyword spotting — disambiguation when multiple (or zero) character/tass words match.
2. **Recipient model** ([`tass-recipient-alloc`](#tickets)): past "enervate implies the user," what does controlled allocation look like — a recipient field, a transfer record, authorization?
3. **Naming/shape** — this doc drops the separate "Listener" concept in favor of Source (stream) + dispatcher + command handlers. Confirm that lands.
4. **Multi-process refresh** ([`tass-refresh-coordination`](#tickets)) — deferred; how much does jac-store's turso `AuthRepository` actually give us here?

## Tickets

Epic **`tass-listener-svc`** — *Configurable listener daemon (Spacedust commands + tass-at-large fold)*:

- `tass-sync-source` — EventSource trait + normalized envelope
- `tass-source-spacedust` — Spacedust WS source + hydration
- `tass-source-jetstream` — jetstream source via `jacquard_common::jetstream`
- `tass-job` — action-chain + Executor via `tass-phase` (in-memory)
- `tass-job-persistence` — persist parked phases to turso (low-low-low)
- `tass-store-provider` — shared local turso db (auth store + job persistence)
- `tass-engine` — mechanism only: dispatcher + Executor + effect vocabulary + knobs + wide-event (no verbs)
- `tass-listener-config` — `[service.listen]` with nested per-verb fall-through
- `tass-wide-event` — wide-event tracing + per-command verbosity filtering
- `tass-target-resolve` — resolve character + tass from message words
- `tass-act-enervate` — the enervate verb (own FSM + Driver); "burn my tass"
- `tass-act-meditate` — the meditate verb (own FSM + Driver)
- `tass-backfill-constellation` — Constellation catch-up + idempotent dedupe
- `tass-ledger-fold` — ledger fold over the EventSource stream
- `tass-listen` — small standalone service binary
- `tass-cli-listen` — CLI integration (low priority)
- `tass-recipient-alloc` — controlled allocation to recipients (future)
- `tass-attest-age-payload` — AGE-encrypted before→after on attestations

(`tass-embed-hydrant` closed — hydrant deleted.)

## See also

- `crates/tass-phase/` — the work model; `tests/burn_chain.rs` is the worked burn chain.
- `jacquard/examples/subscribe_jetstream.rs` — the jetstream subscribe pattern.
- [attestation.md](attestation.md), [ledger.md](../ledger.md).
- `~/archive/microcosm.blue/microcosm-rs/spacedust/` — `server.rs`, `subscriber.rs`, `consumer.rs`, `links/src/record.rs`, `lib.rs`.
