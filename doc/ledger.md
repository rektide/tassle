# Tassle Ledger Design

## Purpose

The ledger is the derived game-state view over Tassle action records. It is separate from `actor.rpg.stats`: Mage stats describe the character sheet and current player-facing traits, while the ledger explains how quintessence and tass moved over time.

The first ledger should be boring, local, and recomputable. It reads public records from one actor's PDS, folds them in timestamp order, and reports balances plus anomalies. It does not require an AppView, Storyteller authority, keytrace signatures, or consensus reality.

When the `hydrant` cargo feature is enabled, the local read model should be fed by the embedded Hydrant listener described in [`doc/hedystia-listener-design.md`](/doc/hedystia-listener-design.md). The non-Hydrant CLI can still do direct PDS reads, but the durable service path should use Hydrant cursor replay plus live tailing.

## Source Collections

The v1 ledger folds these collections:

| Collection | Role |
|---|---|
| `com.superbfowle.tass.node` | Declares a Node and its starting ambient quintessence pool. |
| `com.superbfowle.tass.meditate` | Draws quintessence from a Node into the Mage's pattern. |
| `com.superbfowle.tass.tassilize` | Crystallizes quintessence into a Tass object. |
| `com.superbfowle.tass.enervate` | Drains/spends quintessence from a Tass object. |

`actor.rpg.stats/mage` remains an input for caps and display context, not the ledger itself. If the sheet has Avatar, Quintessence, Paradox, or Prime values, validation can use them, but the ledger state is still derived from Tassle records.

## Fold Model

All source records are normalized into ledger events:

```text
LedgerEvent {
  uri,
  cid,
  collection,
  rkey,
  createdAt,
  kind,
  source,
  target,
  amount,
  note,
  raw,
}
```

Events are sorted by `createdAt`, then by URI for deterministic ties. Missing or invalid timestamps are anomalies; they should still be displayed, but they should not silently affect balances unless we define a fallback ordering.

There are two independent orderings:

- Hydrant event cursor order controls ingestion, idempotency, and exactly-once local processing.
- Record `createdAt` order controls the game-state ledger fold.

Bad timestamps should not block cursor progress. Store the record, record an anomaly, advance the cursor after commit, and exclude the event from balance mutation until a fallback rule is deliberately chosen.

The fold outputs:

```text
LedgerState {
  nodes: NodeBalance[],
  tass: TassBalance[],
  patternQuintessenceDelta,
  events: LedgerEvent[],
  anomalies: LedgerAnomaly[],
}
```

Suggested balance shapes:

```text
NodeBalance {
  uri,
  name,
  rating,
  startingAmbient,
  meditatedOut,
  tassilizedOut,
  remainingAmbient,
}

TassBalance {
  uri,
  node,
  form,
  startingQuintessence,
  enervatedOut,
  remainingQuintessence,
}
```

## Rules

Initial v1 rules should be visible rather than authoritative:

- A Node starts with `ambientQuintessence` if present, otherwise `rating * 5`.
- `meditate` subtracts from the referenced Node's remaining ambient pool and adds to the Mage's pattern delta.
- `tassilize` should subtract from the referenced Node's remaining ambient pool when the source is a Node. Later, it may also support Mage-pattern source when a Mage with Prime 2 crystallizes their own quintessence into Tass.
- `enervate` subtracts from the referenced Tass object's remaining quintessence.
- Negative balances are anomalies, not hidden failures.
- Unknown references are anomalies.
- Duplicate or conflicting records are shown as records; no resolver hides them.

## Tallying Modes

Useful options:

| Mode | Shape | Tradeoff |
|---|---|---|
| Full replay on every read | Read all normalized events and fold in memory. | Most obviously correct, but read cost grows without bound. |
| Event log plus snapshot | Append normalized events, update a materialized snapshot, and keep enough log to rebuild. | Recommended default: recomputable and fast reads. |
| Snapshot only | Keep balances and last cursor, discard event log. | Fastest but not acceptable for v1 because anomalies and derivation are not explainable. |
| Hydrant-only replay | Treat Hydrant's database as the only raw log and rebuild Tassle state from cursor 0. | Good disaster recovery path, but not enough for per-DID locking or local ledger inspection. |

Use event log plus snapshot first. If a snapshot is missing, stale, or version-incompatible, replay that DID's normalized event log to rebuild it. If the normalized log is missing, replay Hydrant from cursor 0 as the slower recovery path.

## Listener Fold

The embedded listener should process Hydrant events like this:

1. Load `lastCursor` from the Tassle service store.
2. Call `hydrant.subscribe(Some(lastCursor.unwrap_or(0)))` before `hydrant.run()` starts producing live events.
3. For each event, ignore non-record events unless they affect account activity metadata.
4. For record events in the source collections, parse `event.record`, build the AT-URI from DID/collection/rkey, and normalize to `LedgerEvent`.
5. Write the raw event mirror, normalized event, anomalies, balance snapshot update, and new cursor in one durable commit.
6. If parsing fails, write a parse anomaly and still advance the cursor in the same commit.
7. On restart, resume from the committed cursor. Hydrant replays missing persisted events before switching to live tailing.

Idempotency keys should include Hydrant event id for ingestion and `uri + cid + kind` for semantic ledger effects. Deletes should become ledger events too, even if v1 only records a tombstone anomaly and leaves historical balances visible.

## Storage Layout

Use a `LedgerStore` trait so the fold does not commit to one physical layout too early.

Recommended first physical layout:

```text
<data>/hydrant/              # Hydrant database
<data>/service/              # global listener cursor, jobs, OAuth/session state
<data>/ledger/<did-key>/     # one fjall database per actor DID
```

Inside each per-DID ledger database, use partitions/logical buckets like:

```text
events_by_cursor             # hydrant event id -> normalized LedgerEvent/raw pointer
events_by_created_at         # createdAt/uri -> event id
records_latest               # uri -> latest cid/status
balances                     # node/tass/pattern snapshot rows
anomalies                    # anomaly id -> anomaly payload
meta                         # schema version, last folded cursor, last rebuilt timestamp
```

Per-DID fjall stores are not Fjall's most storage-efficient shape, but they match the domain well: one actor can be locked, snapshotted, deleted, backed up, or rebuilt independently. Bound the cost with lazy-open-on-touch and an LRU of open DID stores so a large network does not keep thousands of journals, file descriptors, and compaction workers active.

Alternative layout to keep available behind the same trait:

| Layout | Why use it | Why not first |
|---|---|---|
| One shared fjall DB, DID-prefixed keys | Fewer files, one journal, better storage density. | Coarser locking and weaker per-actor lifecycle story. |
| One shared fjall DB, partition per DID | Middle ground with some separation. | Still shares journal and may create many partitions. |
| Per-DID fjall DB | Fine-grained locking, backup, btrfs subvolumes, failure isolation. | More files and open-resource pressure. |

If the data root is on btrfs, a later storage ticket should create each `<data>/ledger/<did-key>/` as a subvolume for cheap snapshots, send/receive, and per-DID quotas. Non-btrfs filesystems should fall back to normal directories.

## Commands

The ledger work should land as read commands first:

| Command | Purpose |
|---|---|
| `tass ledger balance` | Show Node ambient pools, Tass balances, Mage pattern delta, and anomalies. |
| `tass ledger history` | Show ordered ledger events with source/target/amount/purpose and AT-URIs. |
| `tass ledger inspect <uri>` | Explain how one Node or Tass balance was derived. |

Top-level aliases can come later if they are genuinely more usable, but the implementation should live under a ledger module so it does not get conflated with `mage list`.

The read commands should work from the materialized ledger store when available. A `--rebuild` flag can force replay from the per-DID event log. A later `--from-hydrant` or maintenance command can rebuild from Hydrant cursor 0 if the Tassle ledger store was deleted.

## Validation Hook

Once the fold exists, mutating commands should use it before writing:

- `meditate`: warn or block if drawing past known Node ambient pool or known Avatar cap.
- `tassilize`: warn or block if crystallizing more than available from the selected source.
- `enervate`: warn or block if spending unavailable Tass.
- `--dry-run`: show the proposed event and resulting balance changes without publishing.

The first implementation can warn and require an explicit force flag for questionable writes. The important property is that the CLI explains the ledger effect before publishing.

## CEL Filtering

Ledger commands should share the same eventual CEL model as other list/read commands. Each command should expose a stable item envelope before filtering:

```text
{
  uri,
  cid,
  collection,
  rkey,
  source,
  value,
  normalized
}
```

CEL predicates can then filter event streams or balance rows consistently. CEL projection can come later after the envelope stabilizes.

## Non-Goals

- No public AppView or consensus indexer for v1. Embedded Hydrant is a local cache/event source.
- No Storyteller or Reality authority merge.
- No keytrace signatures.
- No hidden mutation of `actor.rpg.stats`.
- No attempt to make `mage list` / `mage stats` a history view.
