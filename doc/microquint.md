# Microquintessence

Mage: The Ascension expresses a mage's pattern quintessence as a small integer (0–20). That is enough for a human-facing character sheet, but not enough to represent the sub-point flows Tassle cares about: a `meditate` that pulls a fraction of a point, a `tassilize` that crystallizes half a point of resonance, an `enervate` that drains a Tass down by a quarter point. **Microquintessence** is the finer-grained representation that holds the real balance at sub-point resolution; the legacy integer is a derivation of it for display and backwards-compat clients.

This document describes microquintessence in general — what it is, why it lives alongside the legacy integer, and the **inline sync policies** that keep the two fields coherent without ever introducing a separate sync pass. Implementation specifics live in the `tass-quint` and `tass-quint-jac` crates and the `tass-quint-policy` epic; this doc is the conceptual anchor.

## The two fields and their roles

On an `actor.rpg.stats#mageStats` record, the mage block carries two fields that describe the same quantity at different resolutions:

| Field | Resolution | Role (default policy) |
|---|---|---|
| `quintessence` | whole points (integer 0–20) | legacy, derived display value — what every pre-Tassle client reads and writes |
| `milliQuintessence` | thousandths of a point (integer) | source of truth — the real balance at milli granularity |

The milli resolution is an implementation choice (`PER_POINT = 1000`), not a property of microquintessence itself. A future host lexicon could carry `microQuintessence` at a finer resolution (`10_000`, `100_000`) and the same model would apply. Milli is what shipped first; the doc speaks in milli because that is the only resolution the code currently knows about.

## Why both fields, not just milli

Three constraints force the dual-field shape:

1. **Backwards compatibility.** The host lexicon (`rpg.actor`) already publishes `quintessence` as an integer 0–20. Pre-Tassle clients (other character-sheet tools, hand-edited records, third-party viewers) read and write that integer. Killing the field would orphan every sheet that wasn't written through us.
2. **Display floor.** Players read whole points. A mage with `milliQuintessence: 7300` "has 7 points" — the sheet's `quintessence` field must say `7`, not `7.3`. Reading `floor(milli / 1000)` from the milli field is the correct derivation; the legacy integer is where that floor is supposed to live.
3. **External mutation.** Any client can mutate `actor.rpg.stats` over ATProto. We do not own the record. Even when our writer always replicates the floor into `quintessence`, someone else can publish a sheet with a `quintessence` value that disagrees with `milliQuintessence` (or with no milli value at all). We must cope with sheets we did not write.

So we keep both and pick a **direction of authority**: which field is the system of record, which is the derived copy. The default direction is milli-is-truth.

## The drift problem

Because the two fields describe the same quantity, any mutation that touches one but not the other leaves them out of sync. Drift has two causes:

- **Floor drift** — `quintessence != floor(milliQuintessence / 1000)`. The displayed integer disagrees with the milli balance. Caused by hand edits to the legacy field, by older writers that didn't know about milli, or by clients that intentionally change one and forget the other.
- **Timestamp drift** — the record-level `updatedAt` is newer than `milliQuintessenceUpdatedAt` *and* the write did not go through a tass-quint writer. `milliQuintessenceUpdatedAt` (Tassle's stamp, set when the milli value changed) is more specific than the record-level `updatedAt` (set on any mutation of the record). When the record's `updatedAt` advances without the milli stamp advancing, something else mutated the mage block — the two fields may have drifted independently, even if the values look consistent right now.

The timestamp check is the real signal of "did a tass-quint write touch this sheet last?" It is the first piece of provenance the policy layer is built on.

## Inline sync policies — not a sync pass

The crucial design choice: **there is no `sync` command, no `sync()` API, no out-of-band reconciliation pass.** The coherence check is folded into the existing `QuintClient` read / write / adjust paths. Callers keep calling the same methods they call today:

- `QuintClient::read(repo, rkey)` still returns `Option<Quint>`. It just returns the *coherent* one — the value the active sync direction says is authoritative, repaired from drift before it leaves the library. A read MUST NOT issue a write to repair the sheet.
- `QuintClient::write_with(repo, rkey, q, opts)` still writes a `Quint` and returns the applied one. The patch it persists is built so that, regardless of incoming drift, the record it leaves behind is coherent by construction (milli-is-truth: replicate the floor; quintessence-is-truth: hydrate the milli from the requested points).
- `QuintClient::adjust_with(...)` is `read` + `write_with` — both halves above apply for free.

The seam for this is a `SyncPolicy` / `Coherence` trait (or fn) operating on the raw mage-block fields (`milliQuintessence`, `quintessence`, `milliQuintessenceUpdatedAt`, `updatedAt`) and returning a small enum — `InSync` / `RefreshFloor` / `HydrateMilli` — that the existing methods consult internally. **Nothing outside the `tass-quint` family calls this seam directly.** It exists to make the rule pluggable (different realities can carry different drift heuristics), not to grow the API surface.

Reasons this stays inline:

- Drift is cheap to detect (compare two ints + two timestamps) inside an RMW that already has the record fetched.
- A separate sync pass forces every caller to remember to run it; an inline check can't be forgotten.
- An inline check can't race with a concurrent writer the way a sync-then-read can — the check and the write are one operation.
- It keeps the public API at "give me the value" / "set the value"; the policy is a property of how the library does that, not a verb callers learn.

## Policies

Three policies, all expressible through the seam above. None introduces a new entry point on `QuintClient`.

### Default — milli is truth, floor replicated on write

Today's behavior, unchanged. `resolve()` prefers `milliQuintessence`; `sheet_patch(q)` writes both `milliQuintessence = q.millis()` and `quintessence = q.points()` (the floor), and stamps `milliQuintessenceUpdatedAt = now` on the way out. Reads resolve via milli and show the floor.

This is the policy every current user has. Drift on a sheet is repaired on read (value returned from milli, displayed floor recomputed) and repaired on the next write (the patch writes both fields). The sheet is never silently mutated by a read.

### Sync direction (opt-in)

`enum SyncDirection { MilliIsTruth, QuintessenceIsTruth }`, default `MilliIsTruth`. A chronicle that only ever writes the legacy integer — or treats the player-facing integer as the authoritative "this is what the player said their quintessence is" — can declare `QuintessenceIsTruth`. The impact:

- `read` returns `Quint::from_points(quintessence)` (milli is ignored for derived value, treated as a stale cache at best).
- `write_with` still writes both fields for backwards compat with milli-aware clients, but the source of truth is the integer field — the requested `q` is written as `q.points()` to `quintessence` and `q.millis()` to `milliQuintessence` simultaneously.
- The stale-refresh action becomes *hydrate milli from quintessence × 1000* instead of *replicate floor from milli*. Direction selects the repair action; the rest of the path is the same.

### Avatar-cap enforcement (opt-in)

A per-character cap = the mage's Avatar rating, not the global `MAX_POINTS = 20`. When enabled in `WriteOpts` / the policy configuration:

- On `write_with` / `adjust_with`, read the Avatar rating from the sheet (the `actor.rpg.stats#mageStats` Avatar field — pending `tass-lex-mage-codegen`; falls back to `MAX_POINTS` until that lands).
- Compute the cap in milli (compare at full resolution, never lossy).
- If the proposed new value would exceed the cap, **clamp** the write to the cap and **report the overflow** back to the caller. The excess is not silently dropped — the caller gets a structured result with `applied` and `overflow` so an enervate pipeline can route the remainder somewhere useful (see below).

Off by default. Off path is byte-for-byte the current behavior; opt-in is per write / per reality.

## Where this stops

Microquintessence and its sync policies are operations on the mage's pattern field — the *carrier* for personal quintessence. They do not know about Tass, Nodes, recipients, or transfers. Two related scopes are deliberately outside this doc:

- **Tass and Node amounts** have their own integer quantities (`tassilize.amount`, `node.ambientQuintessence`). The same "finer resolution + drift" idea could apply there later, but they carry their own fields, writers, and host lexicons; generalizing microquintessence to those is a separate design, not an extension of this one.
- **Enervate target routing** is the *consumer* of the policy layer: when an `enervate` routes its drained quintessence to a destination other than the author's pattern (a named Tass, a Node's ambient pool, a recipient Mage), the target-side policy decides "how much can land here?" — the pattern target reuses the Avatar cap above, a Node target applies its own ambient-pool cap, a Tass target applies its remaining-capacity. Those policies sit in the enervate pipeline (see the `tass-enervate-targets` ticket), not in `tass-quint`. Microquintessence just gives them a coherent input value to route.

## Properties the design preserves

- **No silent data loss.** A drift detected is a drift reported (on read, surfaced as a stale flag; on write, the patch repairs both fields). An Avatar-cap overflow is reported as overflow, not dropped.
- **No read-time mutation.** A read returns a coherent value but never writes back to repair. The sheet is only mutated by an explicit write through `write_with` / `adjust_with`.
- **No new verbs.** `read`, `write`, `adjust` are the whole surface. Policy is a property of how those methods work, not an additional call callers must remember.
- **Pluggable, not prescribed.** The default is milli-is-truth + floor replication; chronicles opt into direction or cap enforcement explicitly. Not adopting a policy leaves behavior unchanged.
- **Provenance via timestamp.** `milliQuintessenceUpdatedAt` is the narrow stamp; the record `updatedAt` is the broad stamp. Drift detection lives in their comparison.

## References

- `doc/ledger.md` — why the ledger does not silently mutate `actor.rpg.stats`; microquint writes are explicit commands, same rule.
- `crates/tass-quint/src/lib.rs` — `Quint`, `PER_POINT`, `resolve()`, `sheet_patch()` (today's milli-is-truth default).
- `crates/tass-quint-jac/src/lib.rs` — `QuintClient`, `WriteOpts`, ` Stamp`, `milliQuintessenceUpdatedAt` stamping.
- `tass-quint-policy` epic and children (`tass-quint-stale-sync`, `tass-quint-avatar-cap`, `tass-quint-sync-direction`, `tass-enervate-targets`) — the implementation plan for the policies above.