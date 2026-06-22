# co/core — Discovery Notes

The tassle README explicitly names co/core as its inspiration:

> ## Inspirations
> - co/core workqueue and tokens

This file maps that inspiration concretely. Co/core's compute lifecycle (job → receipt → settlement) is the workqueue model tassle's energy ledger is built on; its exchange-policy + token-accounting records (grant, floor, refresh, patronage) are the template for how a Node's ambient quintessence and a Mage's pattern budget should behave on ATProto.

## Publishing account

| | |
| --- | --- |
| **Handle** | [`@cocore.dev`](https://bsky.app/profile/cocore.dev) |
| **DID** | `did:plc:5quuhkmwe2q4k3azfsgg7kdz` |
| **PDS endpoint** | `https://jellybaby.us-east.host.bsky.network` |
| **AppView DID** | `did:web:appview.cocore.dev` |
| **AppView XRPC** | `https://appview.cocore.dev/xrpc` |
| **Static lexicon URL** | `https://cocore.dev/lexicons/<NSID>.json` |
| **Console / dev docs** | [console.cocore.dev/docs](https://console.cocore.dev/docs/lexicons) |
| **lexicon.garden identity** | [lexicon.garden/identity/did:plc:5quuhkmwe2q4k3azfsgg7kdz](https://lexicon.garden/identity/did:plc:5quuhkmwe2q4k3azfsgg7kdz) |
| **pds.ls** | [pds.ls/did:plc:5quuhkmwe2q4k3azfsgg7kdz](https://pds.ls/did:plc:5quuhkmwe2q4k3azfsgg7kdz) |

Co/core publishes its lexicons in **two** places: as static JSON at `cocore.dev/lexicons/<NSID>.json` and as `com.atproto.lexicon.schema` records on the publishing PDS. Four NSIDs (`dev.cocore.defs`, `dev.cocore.account.friend`, `dev.cocore.account.tokenGrant`, `dev.cocore.account.tokenPatronage`) currently return 404 on the static mirror and were recovered from the PDS schema collection — see [doc/ref/README.md](../ref/README.md#sources) for details.

## Lexicons in this checkout

Seventeen lexicons are snapshotted in [`doc/ref/`](../ref/).

### Shared definitions (3)

| NSID | Local | lexicon.garden | Description |
| --- | --- | --- | --- |
| `dev.cocore.defs` | [`doc/ref/dev.cocore.defs.json`](../ref/dev.cocore.defs.json) | [lexicon.garden/nsid/dev.cocore.defs](https://lexicon.garden/nsid/dev.cocore.defs) | AppView-shaped wrappers: `indexedRecord`, `verifyFinding`, `activityStat`, `activityWindows` (hour/day/week/month). |
| `dev.cocore.account.defs` | [`doc/ref/dev.cocore.account.defs.json`](../ref/dev.cocore.account.defs.json) | [lexicon.garden/nsid/dev.cocore.account.defs](https://lexicon.garden/nsid/dev.cocore.account.defs) | `apiKeyView` shape — never contains the secret. |
| `dev.cocore.compute.defs` | [`doc/ref/dev.cocore.compute.defs.json`](../ref/dev.cocore.compute.defs.json) | [lexicon.garden/nsid/dev.cocore.compute.defs](https://lexicon.garden/nsid/dev.cocore.compute.defs) | The workhorse: `money`, `tokenCounts`, `modelPrice`, `tokenRate`, `trustLevel`, `tier`, `settlementStatus`. |

### Compute records (10)

| NSID | Local | lexicon.garden | Description |
| --- | --- | --- | --- |
| `dev.cocore.compute.provider` | [`…compute.provider.json`](../ref/dev.cocore.compute.provider.json) | [lexicon.garden/…compute.provider](https://lexicon.garden/nsid/dev.cocore.compute.provider) | One record per physical provider machine. |
| `dev.cocore.compute.attestation` | [`…compute.attestation.json`](../ref/dev.cocore.compute.attestation.json) | [lexicon.garden/…compute.attestation](https://lexicon.garden/nsid/dev.cocore.compute.attestation) | Secure-Enclave-signed snapshot of a provider machine's state. Content-addressed; many receipts strong-ref it. |
| `dev.cocore.compute.job` | [`…compute.job.json`](../ref/dev.cocore.compute.job.json) | [lexicon.garden/…compute.job](https://lexicon.garden/nsid/dev.cocore.compute.job) | Requester's request for work. Tassle's `tassilize` analogue. |
| `dev.cocore.compute.paymentAuthorization` | [`…compute.paymentAuthorization.json`](../ref/dev.cocore.compute.paymentAuthorization.json) | [lexicon.garden/…paymentAuthorization](https://lexicon.garden/nsid/dev.cocore.compute.paymentAuthorization) | Requester's standing authorization for an exchange to charge them up to a ceiling. |
| `dev.cocore.compute.receipt` | [`…compute.receipt.json`](../ref/dev.cocore.compute.receipt.json) | [lexicon.garden/…compute.receipt](https://lexicon.garden/nsid/dev.cocore.compute.receipt) | Signed proof of one completed job. Tassle's per-Tass-genesis record analogue. |
| `dev.cocore.compute.settlement` | [`…compute.settlement.json`](../ref/dev.cocore.compute.settlement.json) | [lexicon.garden/…compute.settlement](https://lexicon.garden/nsid/dev.cocore.compute.settlement) | Exchange's signed proof of payment for a receipt. Status: settled/refunded/disputed. |
| `dev.cocore.compute.exchangePolicy` | [`…compute.exchangePolicy.json`](../ref/dev.cocore.compute.exchangePolicy.json) | [lexicon.garden/…exchangePolicy](https://lexicon.garden/nsid/dev.cocore.compute.exchangePolicy) | The most exciting record for tassle: fee schedule, token grant/floor/refresh, patronage distribution. |
| `dev.cocore.compute.exchangeAttestation` | [`…compute.exchangeAttestation.json`](../ref/dev.cocore.compute.exchangeAttestation.json) | [lexicon.garden/…exchangeAttestation](https://lexicon.garden/nsid/dev.cocore.compute.exchangeAttestation) | Exchange's self-published operating-posture statement. |
| `dev.cocore.compute.dispute` | [`…compute.dispute.json`](../ref/dev.cocore.compute.dispute.json) | [lexicon.garden/…compute.dispute](https://lexicon.garden/nsid/dev.cocore.compute.dispute) | Exchange-signed adjudication of a complaint about a settled receipt. |
| `dev.cocore.compute.termsAcceptance` | [`…compute.termsAcceptance.json`](../ref/dev.cocore.compute.termsAcceptance.json) | [lexicon.garden/…termsAcceptance](https://lexicon.garden/nsid/dev.cocore.compute.termsAcceptance) | User's acceptance of an exchange's terms, pinned by `termsVersion`. |

### Account records (4)

| NSID | Local | lexicon.garden | Description |
| --- | --- | --- | --- |
| `dev.cocore.account.profile` | [`…account.profile.json`](../ref/dev.cocore.account.profile.json) | [lexicon.garden/…account.profile](https://lexicon.garden/nsid/dev.cocore.account.profile) | User's cocore-side profile, auto-provisioned at first sign-in. |
| `dev.cocore.account.friend` | [`…account.friend.json`](../ref/dev.cocore.account.friend.json) | [lexicon.garden/…account.friend](https://lexicon.garden/nsid/dev.cocore.account.friend) | One-way declaration of trust for routing friends-only jobs. |
| `dev.cocore.account.tokenGrant` | [`…account.tokenGrant.json`](../ref/dev.cocore.account.tokenGrant.json) | [lexicon.garden/…tokenGrant](https://lexicon.garden/nsid/dev.cocore.account.tokenGrant) | Exchange-issued one-time signup grant, written to the exchange's repo. |
| `dev.cocore.account.tokenPatronage` | [`…account.tokenPatronage.json`](../ref/dev.cocore.account.tokenPatronage.json) | [lexicon.garden/…tokenPatronage](https://lexicon.garden/nsid/dev.cocore.account.tokenPatronage) | Periodic patronage rebate proportional to activity during the window. |

## Conjectured relationships to tassle

These are working notes, not commitments. Bigger-picture cross-ecosystem ideas go in [`lexicon-ideas.md`](lexicon-ideas.md).

### The workqueue triangle: job → receipt → settlement

Tassle's `tassilize → meditate/enervate` lifecycle is the same shape as co/core's `job → receipt → settlement`. Mapping each role:

| co/core role | tassle role | note |
| --- | --- | --- |
| requester | the Mage who triggers the action | writes the request record on their own PDS |
| provider | the Node | "the machine doing the work" → "the place where quintessence crystallizes" |
| exchange | the Storyteller / chronicle consensus | adjudicates who actually owes whom |
| `compute.job` | `com.superbfowle.tass.tassilize` | "I, Mage X, request that Node N crystallize Y quintessence into Tass of form F" |
| `compute.receipt` | a genesis record for the resulting Tass | "Provider Node N attests: I produced Tass T for Mage X on date D, with form F and resonance R" |
| `compute.settlement` | the Mage's sheet update | "Storyteller S settles: Mage X's pattern now holds Y quintessence, Tass T exists in their inventory" |
| `compute.paymentAuthorization` | the `sheet` AT-URI field on action records | "I, Mage X, authorize updates to my `actor.rpg.stats/mage` record up to ceiling C" |
| `compute.dispute` | Storyteller-mediated Tass conflicts | two Mages claim the same Tass; Storyteller adjudicates and writes a dispute record |

The co/core lifecycle adds two things tassle's current action records lack: (1) a strong-ref chain from genesis to consumption (every Tass record should ref its Node's attestation, every enervation should ref the Tass being spent), and (2) an explicit status field on the settlement-side record (`settled` / `refunded` / `disputed`) so a Mage's chronicle history is auditable.

### `dev.cocore.compute.defs#money` — the unit shape

```json
{ "amount": 1500, "currency": "QUINT" }
```

`amount` is integer minor units; `currency` is 3–8 uppercase chars. This is the right shape for any quintessence-denominated value tassle writes:

- Node ambient-quintessence pool
- Tass record's `currentQuintessence` / `originalQuintessence`
- Action records' `amount` field (currently a bare integer in `tassilize`/`meditate`/`enervate`)
- A future Avatar-rating-denominated ceiling

Using a named currency code (`QUINT`, `TASS`, `PARADOX` for the paradox pool) instead of a bare integer lets the same record carry multiple related quantities without ambiguity, and it composes with the `money`-shape `price`/`priceCeiling`/`tokenRate` machinery co/core has already worked out.

### `dev.cocore.compute.exchangePolicy` — Node operating posture

This is the richest single record in the co/core schema and the strongest pull on tassle's design. Every field maps to a Node or chronicle concept:

| exchangePolicy field | tassle analogue |
| --- | --- |
| `fee.bps` (500 = 5% to treasury) | Tithe to the resonant authority of the Node (a Prime 2 working imposes a 1-quintessence tithe per tassilize; the bps field is the general shape) |
| `fee.currency` | The currency the tithe is denominated in |
| `tokenGrant` (cocore.dev: 1,000,000) | First-Awakening endowment from a Mage's Avatar |
| `tokenFloor` (cocore.dev: 100,000) | Minimum pattern reserve — a Mage cannot dispatch a working that would zero them out |
| `weeklyRefresh.amountPerDid` + `cadenceMinutes` | **Node ambient-quintessence regen** — the "use-it-to-keep-it" lazy mint is almost exactly theMage rule that Nodes regenerate toward their cap (rating × 5) on each touch |
| `weeklyRefresh` firing only on network activity | A Node only regenerates when a Mage interacts with it (no offline trickle) |
| `patronageDistribution.cadenceDays` + `proportionToActivity` | Cabal-level redistribution: at end of chronicle arc, Node output is shared among Mages who engaged with it, proportional to their engagement |
| `treasuryDid` | The Node itself as the accumulating authority |
| `termsUri` + `termsVersion` | Chronicle rules / Paradigm acceptance, version-pinned so changes force re-acceptance |
| `active` (soft-delete) | Retired Nodes stay valid for receipts that arrived before retirement |

The `weeklyRefresh` lazy-mint pattern is worth calling out specifically: a Node that regenerates `rating × 5` quintessence per week **but only when a Mage touches it** prevents the "dormant Node accrues infinite backlog" failure mode and matches the in-fiction rule that Nodes must be observed / meditated at to flow.

### `dev.cocore.account.tokenGrant` + `tokenPatronage` — sovereign token balances

These two records together are the cleanest ATProto-native model for "quintessence in a Mage's pattern" — a sovereign, per-DID token balance with clear provenance and periodic top-up:

- **`tokenGrant`** is the one-time endowment (Awakening). The exchange writes it to its own repo, naming the recipient DID — exactly the pattern an Avatar-writing tool would use to record "this Mage Awakened on this date with this much quintessence from this Avatar".
- **`tokenPatronage`** is the periodic rebate. Each record carries a `period` object with `windowStart`/`windowEnd`/`patronageDuringPeriod` and the `amount` awarded. This is the structure for "during chronicle window W, Mage X engaged with Nodes N1, N2, N3 for total resonance R; their pattern was credited C quintessence in proportion".

Crucially, both records live in the **publisher's** repo (the exchange's), not the recipient's. Tassle's analogue: the Node (or the Storyteller operating it) writes patronage/grant records to its own PDS; the Mage's pattern is the sum of records others have written about them. This is a different model from "the Mage writes their own quintessence to their sheet" and worth thinking about — it's more auditable but less sovereign.

### `dev.cocore.compute.provider` + `attestation` — Node-as-provider

The provider record's shape carries several ideas worth borrowing for a future `com.superbfowle.tass.node` upgrade:

- `priceList[]` with `modelPrice` per model ≈ a Node's resonance offerings (which spheres this Node can produce Tass for, at what conversion rate)
- `attestationPubKey` ≈ the Node's signing key for attesting its Tass (paired with a hardware-attestation analogue like "this Node was created by an actual Prime working on this date, not fabricated")
- The strong-ref chain `provider ↔ attestation ↔ receipt` is the chain a Tass record should form: a Node references its attestation record (proving it was legitimately created); each Tass record references both the Node and the attestation
- `supportedCurrencies` ≈ which resonance types this Node produces

### `dev.cocore.compute.receipt`'s `price` + `tokens`

The receipt's token accounting (input tokens in, output tokens out, priced in `money`) is the model for "what did this enervation cost / produce":

- input tokens ≈ the Mage's effort (Paradox risk, Arete rolls, time)
- output tokens ≈ the effect magnitude achieved
- `price.amount` ≈ quintessence consumed from the Mage's pattern
- `price.currency` ≈ `QUINT`

Tassle's `com.superbfowle.tass.enervate` currently records only `amount` (a bare integer). Upgrading it to the `compute.defs#money` shape plus a `tokens` object would let a single enervation record say "this cost 3 quintessence and 2 paradox, producing 5 effect-units of Forces 3 working" — much richer audit trail.

### `dev.cocore.account.friend` — cabal resonance edges

A one-way declaration of trusted routing for friends-only compute is structurally identical to "I, Mage X, declare my Avatar is resonant with Mage Y's Avatar for purposes of ritual workings." The `friend` record's lifecycle (one-way declaration, no acceptance required, can be revoked) is the right shape for cabal membership edges: each Mage declares their own cabal affinity, no central roster.

### AppView architecture

Co/core separates the AppView (`did:web:appview.cocore.dev`) from the publishing PDS, with an explicit "the AppView is a cache, not a ledger" framing. The `dev.cocore.defs#indexedRecord` shape (`{uri, cid, collection, repo, rkey, body, indexedAt}`) is what a tassle appview would return for indexed Tass records — a firehose consumer that collects every `com.superbfowle.tass.*` record across every Mage's PDS and serves read queries over them. The `verifyReceipt` / `verifySettlement` endpoints are templates for a `verifyTass` query that walks the Node→attestation→Tass→enervation chain and reports findings using the `verifyFinding` shape (`{severity, code, message}`).

## Data quality observations

- **All 17 files parse cleanly.** The 4 PDS-recovered files (`dev.cocore.defs`, `dev.cocore.account.{friend,tokenGrant,tokenPatronage}`) were reconstructed from `com.atproto.lexicon.schema` records and normalized to the same layout as the static-site files; their content matches the lexicon.garden renderings.
- **The lexicons page at `console.cocore.dev/docs/lexicons` is JavaScript-rendered.** For machine-readable consumption, use the static JSONs or the PDS records.
- **Compute records have heavy hardware-attestation machinery.** The `compute.attestation` schema is large and detailed (Apple Secure Enclave, MDA cert chains, cdHash, hardened-runtime posture booleans, etc.). Tassle almost certainly does not need this level of hardware anchoring — a Storyteller signature is sufficient — but the `tier` enum (`attested-confidential` / `best-effort`) and the principle "the verifier MUST recompute the tier from evidence and never trust a self-asserted value" are good discipline for any future Node-attestation scheme.

## See also

- [`lexicon-ideas.md`](lexicon-ideas.md) — cross-ecosystem design notes, including a deeper treatment of the workqueue-token parallel
- [`doc/ref/README.md`](../ref/README.md) — manifest of every snapshotted schema and its source URL
- [`doc/design.gpt.md`](../design.gpt.md) — tassle's design draft, which originally named the workqueue inspiration without elaboration
