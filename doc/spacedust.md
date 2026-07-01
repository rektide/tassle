# The listener daemon: Spacedust → hydrate → act

> tassle's primary interface is **atproto posts addressed at us**: someone posts *"burn my tass"* at the Mage account and the listener daemon reacts. This doc covers **ingest** — how a post becomes a dispatched verb; the verb machinery itself is [act.md](act.md). Status: **live** — `tass-listen` (and `tassle listen`) connect, hydrate, and dispatch enervate today (dry-run). Supersedes [discovery/spacedust.md](discovery/spacedust.md).

## 1. The daemon

`tass-listen` is a small crate: a `run(args, profile)` entry + a `clap::Args`, exposed two ways from one code path —

- **standalone binary** `tass-listen` (a lean, focused daemon), and
- **`tassle listen`** in `tass-cli`, behind an off-by-default `listen` feature.

`run` builds config, connects a source, registers verbs, and hands off to `tass_engine::run`:

```rust
let cfg: ListenConfig = extract_cascade(&active_figment(profile)?, &["service.listen"])?;
// … CLI flags override fragment fields; account validated …
let source = SpacedustSource::connect(&cfg.spacedust, SlingshotHydrator::from_config(cfg.slingshot)).await?;
let mut dispatcher = Dispatcher::new();
dispatcher.register(Arc::new(tass_act_enervate::EnervateCommand));
tass_engine::run(source, dispatcher).await;
```

## 2. The flow, end to end

A user posts **"burn my silver coin tass"** at the Mage. The path, crate by crate:

1. **Spacedust** matches the post against `wantedSubjectDids=<MAGE_DID>`, holds it 21s (the delete-debounce buffer), then emits a **`LinkEvent`** *pointer* — `source_record` (the post's at-uri), `subject` (the Mage record it referenced), no body.
2. **`tass-spacedust` — `SpacedustSource::next()`**:
   - `Subscriber::next_event()` reads the WS frame, answers pings, returns the `LinkEvent`; `delete`s are skipped.
   - **Hydrate**: `SlingshotHydrator::hydrate(source_record)` — an unauthenticated jacquard `BasicClient` pointed at Slingshot, via `tass_repo::get_record` → the post's JSON.
   - **`event_from(link, record)`**: `actor_did` + `collection` parsed from the `source_record` at-uri (no fetch); `text` from `record["text"]`. → a **`tass_engine::Event`**.
3. **`tass-engine` — `run`**: `Dispatcher::route(&event)` → `EnervateCommand::matches` (text has "burn" + "tass") → hit. `dispatch_one` opens a per-command span and spawns `handle(event)` on the `tass-phase` **Executor**.
4. **`tass-act-enervate`** drives its FSM (gather the actor's tass → authorize → dry-run write → dry-run attest → `Done`); see [act.md § 4](act.md). Returns `Outcome::DryRun`.
5. Back in the engine, the span emits **one wide-event INFO line** (command, actor, source_record, subject, outcome, latency). Next event.

**Everything above is real today**, except step 4's writes are dry-run (`writes=off`, hardcoded for now).

## 3. Spacedust, mechanically

A WebSocket service; one endpoint `GET /subscribe`. It consumes jetstream internally, runs every record through `collect_links` (one event per link/reference), and fans those to subscribers who filter. Framing: *all social interactions in atproto are links* — a reply/mention/quote/like/follow is a record whose link **target** carries a DID. "A post at us" = "a link whose target carries our DID."

**Filters** (query params, also live-updatable on the socket):

| Param | Matches | Tassle use |
| --- | --- | --- |
| `wantedSubjectDids` | DIDs to receive links about (bare-DID links + DIDs inside at-uri targets) | **primary** — `wantedSubjectDids=<MAGE_DID>` catches everything aimed at us |
| `wantedSubjectPrefixes` | target prefix | `at://<MAGE_DID>/` for links to any of our records |
| `wantedSubjects` | exact target | watch one "command thread" post |
| `wantedSources` | `<collection NSID>:<dotted path>` | narrow to *posts only* (drop likes/follows) |

Subject filters are OR'd; that group is AND'd with `wantedSources`. We keep the default **21s delay** (not `instant`) so a post-then-delete never fires. The payload is a **pointer** (`source_record`/`source_rev`/`subject`), never the body — hence hydration.

## 4. Hydration

Spacedust hands a pointer; the command text lives in the referenced record. **`tass-slingshot`** hydrates it: `SlingshotHydrator` implements `tass_engine::Hydrator` by pointing an unauthenticated `BasicClient` at [Slingshot](https://slingshot.microcosm.blue/) and reusing `tass_repo::get_record` — jacquard-native, no bespoke HTTP. Slingshot is a convenience cache that serves the standard `com.atproto.repo.getRecord`. The alternative — fetching straight from the owning PDS — is ticketed (`tass-hydrate-pds`) as a fallback/alternative; hydration is a swappable `Hydrator` behind the engine seam.

## 5. Config: `[service.listen]`, composed from fragments

Config is read via `tass-config`'s `extract_cascade` (the same one-layer, defaults-filled read `[store]` uses). **`ListenConfig` composes crate-owned fragments** rather than re-declaring fields:

```rust
pub struct ListenConfig {
    #[serde(flatten)] pub spacedust: tass_spacedust::SpacedustConfig,  // account/endpoint/…
    #[serde(default)] pub slingshot: tass_slingshot::SlingshotConfig,  // base
}
```
```toml
[service.listen]
account   = "did:plc:mage"          # → wantedSubjectDids
endpoint  = "wss://spacedust.microcosm.blue/subscribe"

[service.listen.slingshot]
base = "https://slingshot.microcosm.blue"
```

Each behavior crate owns its config fragment; `tass-config` never learns their shapes (see the config convention exemplified by `[store] → StoreConfig`). CLI flags (`--account`/`--endpoint`/`--slingshot`) override fields; `account` is validated at run.

**Per-verb fall-through** (planned): `[service.listen.<verb>]` tables fall through to `[service.listen]` via `extract_cascade(&fig, &["service.listen", "service.listen.enervate"])`, giving granular `reads`/`writes`/`verbosity` per verb. The knobs are designed ([act.md § 5](act.md)); wiring them into the verb Drivers is the next slice.

## 6. Wide-event tracing

One canonical **wide-event** INFO line per handled command, assembled across a per-command span (command, actor, source_record, subject, outcome, latency). `verbosity` (planned, per-verb) scopes *extra* detail via `tracing-subscriber` `EnvFilter` span-field scoping — it never suppresses the wide event. The standalone binary defaults to `info,tass_engine=debug` so the tail is visible before/around verbs.

## 7. The second source, and backfill (planned)

- **tass-at-large fold (planned, `tass-source-jetstream`):** a jetstream consumer over `com.superbfowle.tass.*` (via `jacquard_common::jetstream`) feeding a per-DID ledger fold, so we know every tass's real remaining quintessence independent of any command. Cursor-based → real replay on restart.
- **Spacedust has no replay:** on reconnect, catch up from [Constellation](https://constellation.microcosm.blue/)'s backlink index and dedupe by `source_record`+`source_rev` (`tass-backfill-constellation`). The Executor is in-memory until `tass-job-persistence`, so a restart drops in-flight jobs; Constellation re-derives them and idempotent dedupe prevents double-burns.

## 8. Storage & auth (planned wiring)

tassle is turso-native (`jac-stores`, native-SQL backend). The daemon will own **one shared local turso DB** (`tass-store-provider`) shared with the auth store. Real writes authenticate as the Mage via `tass-config`'s `AuthedClient` (turso-backed `CredentialSession`), **lent** `&session` into a verb's Driver — the borrow model (one session, refresh coordinated once). Multi-process refresh coordination is deferred (`tass-refresh-coordination`). Today the daemon only reads (unauthenticated) and dry-runs writes, so none of this is load-bearing yet.

## 9. Crate map

```
tass-phase          FSM + Driver + Executor              (substrate)         built
tass-engine         Event, Command, Dispatcher, run,     (mechanism)         built
                    EventSource + Hydrator seams, wide-event
tass-spacedust      Subscriber + SpacedustSource<H>      (source)            built
tass-slingshot      SlingshotHydrator                    (hydrator)          built
tass-act-enervate   the enervate verb (dry-run)          (verb)              built
tass-listen         run() + ListenArgs; bin + tassle listen                  built
tass-source-jetstream  tass-at-large fold source                             planned
tass-store-provider    shared turso db                                       planned
tass-config         jacquard auth + [service.listen] config                  built (auth exists)
```

## Status & tickets

Live: connect → hydrate → dispatch → enervate (dry-run) → wide event. Epic **`tass-listener-svc`**; see [act.md § 7](act.md) for the verb-side tickets, plus `tass-spacedust`, `tass-source-jetstream`, `tass-store-provider`, `tass-listener-config`, `tass-wide-event`, `tass-backfill-constellation`, `tass-hydrate-pds`, `tass-listen`, `tass-cli-listen`.

## See also

- [act.md](act.md) — the verb FSM / action system this dispatches into.
- [discovery/spacedust.md](discovery/spacedust.md) — the superseded design-exploration doc.
- `crates/tass-listen/`, `crates/tass-spacedust/`, `crates/tass-slingshot/`.
- `~/archive/microcosm.blue/microcosm-rs/spacedust/` — upstream `server.rs`, `subscriber.rs`, `lib.rs`.
