# Tassle Ledger Design

## Purpose

The ledger is the derived game-state view over Tassle action records. It is separate from `actor.rpg.stats`: Mage stats describe the character sheet and current player-facing traits, while the ledger explains how quintessence and tass moved over time.

The first ledger should be boring, local, and recomputable. It reads public records from one actor's PDS, folds them in timestamp order, and reports balances plus anomalies. It does not require an AppView, Storyteller authority, keytrace signatures, or consensus reality.

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

## Commands

The ledger work should land as read commands first:

| Command | Purpose |
|---|---|
| `tassle ledger balance` | Show Node ambient pools, Tass balances, Mage pattern delta, and anomalies. |
| `tassle ledger history` | Show ordered ledger events with source/target/amount/purpose and AT-URIs. |
| `tassle ledger inspect <uri>` | Explain how one Node or Tass balance was derived. |

Top-level aliases can come later if they are genuinely more usable, but the implementation should live under a ledger module so it does not get conflated with `mage list`.

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

- No AppView/indexer for v1.
- No Storyteller or Reality authority merge.
- No keytrace signatures.
- No hidden mutation of `actor.rpg.stats`.
- No attempt to make `mage list` / `mage stats` a history view.
