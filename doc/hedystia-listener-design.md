# Hedystia Listener Design

## Status

Draft notes for moving Tassle from a CLI-only energy ledger into an embedded Rust listener/enactor service. This builds on the local rpg.actor reference lexicons in [`doc/ref/`](/doc/ref/) and Hydrant's embeddable/indexer API in [`~/archive/ptr.pet/hydrant`](/home/rektide/archive/ptr.pet/hydrant). The Rust CLI should compile this surface only when `tassle-cli` is built with a `hydrant` cargo feature.

The older TypeScript/Bun Hedystia shape remains useful as product language, but the first implementation should follow Hydrant's embedded `examples/statusphere.rs` pattern: build a `hydrant::config::Config`, create `Hydrant::new`, configure filters, call `subscribe(cursor)`, and run Hydrant and the listener in the same Tokio process.

## Source Signals

- `actor.rpg.stats` is the character-sheet host record. Its current lexicon says per-system rkeys are primary and legacy `self` records are compatibility-only, so the service should prefer `actor.rpg.stats/mage` over `actor.rpg.stats/self` for new sheet writes. See [`doc/ref/actor.rpg.stats.json`](/doc/ref/actor.rpg.stats.json) and [`doc/discovery/lex-rpg-actor.md`](/doc/discovery/lex-rpg-actor.md).
- `actor.rpg.stats#mageStats` already contains the fields Tassle needs to mutate or authorize against: `meditation`, `arete`, `prime`, `quintessence`, `paradox`, `willpower`, and the other spheres.
- `equipment.rpg.give` is the closest upstream pattern for service/provider attestation: a provider publishes a record on its own PDS saying it gave an item to a recipient DID. See [`doc/ref/equipment.rpg.give.json`](/doc/ref/equipment.rpg.give.json).
- `equipment.rpg.item` is the recipient-side inventory record that accepts the give and carries local/current item state. See [`doc/ref/equipment.rpg.item.json`](/doc/ref/equipment.rpg.item.json).
- `actor.rpg.master` is a template for Storyteller validation of another actor's RPG data. Its `player`, `system`, `campaign`, `snapshotScope`, and `stats` fields map cleanly onto Node and sheet validation. See [`doc/ref/actor.rpg.master.json`](/doc/ref/actor.rpg.master.json).
- Hydrant can run as a filtered ATProto indexer, replay historical events, tail live events through `Hydrant::subscribe(cursor)` or `/stream`, expose ergonomic XRPC reads like `blue.microcosm.repo.getRecordByUri`, and optionally maintain backlinks via `blue.microcosm.links.*`. See [`~/archive/ptr.pet/hydrant/docs/xrpc/README.md`](/home/rektide/archive/ptr.pet/hydrant/docs/xrpc/README.md) and [`~/archive/ptr.pet/hydrant/examples/statusphere.rs`](/home/rektide/archive/ptr.pet/hydrant/examples/statusphere.rs).

## Core Model Shift

Current CLI commands directly publish action records. The Hedystia service should treat user-published records as intents and service-published records as attestations/effects.

```mermaid
flowchart LR
    Mage["Mage repo"] -->|publishes intent| Hydrant["Embedded Hydrant indexer"]
    Hydrant -->|subscribe(cursor) replay + live| Listener["Tassle listener"]
    Listener --> Store["Per-DID fjall ledger store"]
    Store --> Verifier["authorize + recompute"]
    Verifier --> Scheduler["due jobs"]
    Scheduler --> Enactor["service enactor"]
    Enactor -->|attestation records| ServicePDS["Tassle / Node authority repo"]
    Enactor -->|OAuth putRecord| MageSheet["actor.rpg.stats/mage"]
```

The listener never trusts an intent just because it exists. It verifies ownership, authorization, refs, available energy, sheet capacity, attestation status, and due time before enqueueing or enacting.

## Verbs

### `mintNode`

Create a Node as a service-authoritative inventory object.

- Service publishes the canonical `com.superbfowle.tass.node` record.
- Service publishes a Node grant/attestation record, patterned after `equipment.rpg.give`, naming the recipient DID and the Node URI.
- Recipient may publish an inventory/acceptance record if we want the Node to appear in their rpg.actor-style inventory.
- Ownership is derived from the service attestation, not from a user self-assertion.

### `attestNode`

Attest that a Node exists and define its operational rules.

- Attests owner DID, authority DID, rating, capacity, resonance, meditation generation score, regen policy, and any chronicle/campaign scope.
- Should use strong refs for the Node record and any source records.
- Can borrow `actor.rpg.master.snapshotScope`: `none`, `custom`, or `full`.

### `meditate`

User intent to draw quintessence from a Node into the Mage's pattern.

- Listener verifies the Node attestation and whether the actor can meditate at that Node.
- Listener computes the Node's current ambient pool from event history and scheduled regen, not just from a self-asserted field.
- Listener checks the Mage's sheet, especially `mageStats.meditation`, `arete`, `quintessence`, and capacity rules.
- Enactor emits an attested meditation effect and patches `actor.rpg.stats/mage.quintessence` through the user's stored OAuth session.

### `tassilize`

User intent or service action to crystallize Node quintessence into Tass.

- Service verifies available Node energy and owner/authority rules.
- Service emits a genesis attestation that says this Tass came from this Node at this time with this amount and resonance profile.
- The recipient-side Tass inventory record can then reference the genesis attestation, mirroring `equipment.rpg.item.give`.

### `enervate`

Spend or drain an owned Tass/Node energy source.

- Only the owner, authorized holder, or Node authority can enervate a source.
- If the Node is service-owned, user intents request enervation but the service enacts it only when authorized.
- Enervation should strong-ref the source state it drains so double-spend checks are deterministic.

### `weave`

Cast energy into a pattern.

- This should be a first-class verb rather than overloading `enervate`.
- Inputs can include sheet quintessence, Tass, Node ambient energy, sphere requirements, working description, target pattern, and paradox risk.
- Enactor applies the energy spend and any resulting sheet mutation, such as reduced `quintessence` or increased `paradox`.

## Scheduling

Many records should not be emitted immediately. The listener creates due jobs and a worker emits records at the due time.

Examples:

- Node regeneration: lazy or scheduled restoration toward `rating * 5` capacity.
- Meditation completion: intent observed now, effect emitted after the in-fiction duration.
- Tass crystallization: intent observed now, genesis attestation emitted after the Node's cadence or ritual duration.
- Weave resolution: intent observed now, effect emitted after validation, countersignature, or due time.

Use the Tassle service store as the durable queue. Each job should include `inputUri`, `inputCid`, `kind`, `dueAt`, `status`, `attempts`, and idempotency keys. Enactment should be idempotent by `inputUri + inputCid + effectKind`.

## Hydrant Pipe

The pragmatic integration is same-process embedding in the Rust CLI, gated behind a `hydrant` cargo feature. Without that feature, the base CLI remains a lean generator/reader and does not compile Hydrant or the listener command surface.

Implementation flow:

1. Add an optional `hydrant` dependency to `crates/tassle-cli` and expose it through a `hydrant` feature.
2. In feature builds, compile `tassle hydrant`, `tassle listen`, `tassle worker`, `tassle serve`, and `tassle dev-service` command modules behind `#[cfg(feature = "hydrant")]`.
3. Build a `hydrant::config::Config` from Hydrant env plus Tassle CLI defaults. Default `database_path` should live under the Tassle data dir, not the current working directory.
4. Configure `FilterMode::Filter`, `set_signals`, and `set_collections` for Tassle collections before `run()`, as in `examples/statusphere.rs`.
5. Load the last committed listener cursor from the Tassle service store and call `hydrant.subscribe(Some(cursor))`. Use `Some(0)` for a full replay/rebuild.
6. Run `tokio::select!` over `hydrant.run()?`, optional Hydrant API serving, listener stream handling, workers, and the HTTP app.
7. The listener stores raw stream events, normalized ledger events, anomalies, and the advanced cursor in one durable commit. The cursor only advances after that commit succeeds.
8. The listener uses `hydrant.repos` for public indexed reads and authenticated ATProto OAuth sessions for writes.

Candidate Hydrant environment for `tassle hydrant env`:

```text
HYDRANT_DATABASE_PATH=.data/hydrant
HYDRANT_API_BIND=127.0.0.1:3147
HYDRANT_FULL_NETWORK=false
HYDRANT_FILTER_SIGNALS=com.superbfowle.tass.*
HYDRANT_FILTER_COLLECTIONS=com.superbfowle.tass.*,actor.rpg.stats,equipment.rpg.give,equipment.rpg.item,actor.rpg.master
HYDRANT_ENABLE_FIREHOSE=true
HYDRANT_ENABLE_CRAWLER=true
```

Hydrant's HTTP API is optional in the embedded service. Do not expose Hydrant management endpoints publicly. If public access is needed, only proxy `/xrpc/*`, `/stream`, `/stats`, and health endpoints.

The sidecar mode can remain a later escape hatch through `tassle hydrant run --api-bind ...`, but it should not be the first architecture.

## Data Dirs

Use one Tassle data root for all durable state. In development, `.data` is fine. For installed CLI use, prefer `TASSLE_DATA_DIR`, then `XDG_DATA_HOME/tassle`, then `~/.local/share/tassle`.

Suggested layout:

```text
<data>/hydrant/              # Hydrant fjall database
<data>/service/              # global service state: source cursor, jobs, accounts, OAuth sessions
<data>/ledger/<did-key>/     # per-DID derived ledger store
```

`<did-key>` should be an encoded path segment, not the raw DID string. Percent-encoding or multibase/base32 keeps `did:plc:...` portable and leaves room for `did:web` values.

The per-DID store gives fine-grained locking, backup, deletion, and btrfs subvolume support. The cost is more small files and more open handles than Fjall's usual one-database-many-partitions shape, so implementation should lazy-open DID stores and keep an LRU of active stores.

## Service Store Shape

Initial logical buckets/partitions:

- `oauth_state`: transient OAuth state store for login/callback.
- `oauth_session`: ATProto OAuth session JSON and DPoP state keyed by DID.
- `accounts`: DID, handle, PDS, current auth/session metadata.
- `service_accounts`: authority DIDs the service can write as, including the Tassle/Node authority account.
- `hydrant_sources`: Hydrant source metadata and last consumed cursor.
- `hydrant_events`: optional global raw event mirror: event id, type, repo, collection, rkey, uri, cid, record JSON, indexedAt.
- `records_index`: latest known record by URI/CID for quick local reads.
- `intents`: normalized user intent records with actor DID, verb, status, dueAt, and verification findings.
- `jobs`: durable scheduled work queue.
- `effects`: emitted service effects and sheet patches, keyed idempotently to the source intent.
- `nodes`: derived Node state: owner, authority, capacity, recomputed ambient pool, generation score, last settled event.
- `tass_inventory`: derived Tass state: holder, genesis attestation, current quintessence, spent amount, status.

Auth tokens belong in the Tassle service store, not Hydrant. Hydrant is an indexer/cache and should not hold user OAuth sessions.

## Authorization Rules

- The service can only patch a user's `actor.rpg.stats/mage` if that user has OAuth-authorized the Hedystia app with `repo:actor.rpg.stats` scope.
- The service can publish service-side attestations only from a service/authority DID it controls.
- Node ownership should be resolved from service-published attestations or grant records, not from mutable user claims.
- A user can request an action by publishing an intent, but the service enacts only if current derived state allows it.
- Derived totals are advisory on records. The verifier recomputes Node ambient pool, Tass remaining quintessence, and Mage pattern changes from the event/attestation graph.

## CLI Shape

Start with process-level embedding of Hydrant.

```text
tassle hydrant env             # print Hydrant/Tassle env for the embedded filtered indexer
tassle hydrant run             # run embedded Hydrant only, optionally with local API
tassle listen                  # run embedded Hydrant + listener fold loop
tassle worker                  # run due-job scheduler/enactor loop
tassle serve                   # run Tassle HTTP app only
tassle dev-service             # run Hydrant + listener + worker + HTTP app for local dev
```

Implementation modules:

```text
crates/tassle-cli/src/commands/hydrant.rs       # env + raw Hydrant run commands
crates/tassle-cli/src/commands/listen.rs        # embedded Hydrant + listener entrypoint
crates/tassle-cli/src/commands/worker.rs        # due-job worker entrypoint
crates/tassle-cli/src/commands/serve.rs         # HTTP app entrypoint
crates/tassle-cli/src/service/data_dir.rs       # Tassle data-root resolution
crates/tassle-cli/src/service/hydrant.rs        # Config defaults + filter setup
crates/tassle-cli/src/service/listener.rs       # cursor-aware EventStream consumer
crates/tassle-cli/src/service/processor.rs      # classify records into intents, attestations, effects
crates/tassle-cli/src/service/ledger_store.rs   # LedgerStore trait + fjall layouts
crates/tassle-cli/src/service/verifier.rs       # ownership, attestation, energy, and sheet checks
crates/tassle-cli/src/service/scheduler.rs      # due jobs
crates/tassle-cli/src/service/enactor.rs        # PDS writes + sheet patches
crates/tassle-cli/src/web/app.rs                # HTTP app routes
```

The immediate implementation can start with `hydrant env`, `hydrant run`, and `listen` only. `worker`, `serve`, and `dev-service` should be compiled under the same feature but can land as tickets once storage and folding are stable.

## Build Order

1. Add the `hydrant` cargo feature and compile-gated command modules.
2. Resolve the Hydrant/Jacquard dependency skew before sharing DID/TID/CID types across the boundary.
3. Add data-root resolution and a `LedgerStore` trait with a per-DID fjall implementation.
4. Add embedded Hydrant setup, filter defaults, and cursor persistence without enacting anything.
5. Normalize and index incoming `com.superbfowle.tass.*` events into the ledger store.
6. Add `tassle ledger balance/history/inspect` reads over the materialized fold.
7. Add Node ownership/attestation records and strong refs.
8. Add DB-backed OAuth stores behind the existing auth adapter seam so the service can restore user agents.
9. Add scheduler and idempotent effect emission.
10. Add sheet patching for `actor.rpg.stats/mage` after explicit OAuth login.
11. Add `weave` as a separate verb after meditation/tassilize/enervate have settled semantics.

## Open Questions

- Should `com.superbfowle.tass.node` live only in the authority repo, or can a user publish a candidate Node that an authority later attests?
- Do we model Node and Tass inventory by reusing `equipment.rpg.give/item`, by defining Tassle-native analogues, or by supporting both?
- Is `enervate` strictly spending owned Tass, or can it also drain Node ambient energy? If both, split the source type in the schema.
- Should scheduled Node regen be eager timed emission, or lazy mint on touch? Lazy mint keeps energy bounded and is probably safer.
- Which attestation primitive ships first: keytrace-style field signatures, atproto-attestation-style whole-record signatures, or service-published strong-ref attestations without cryptographic signatures?
- Should Tassle use Hydrant's `user-keyspace` feature to share one fjall database, or keep `<data>/hydrant` and `<data>/ledger/<did-key>` physically separate for lifecycle isolation?
- Should `tassle listen` expose Hydrant's local HTTP API by default, or only when explicitly requested?
