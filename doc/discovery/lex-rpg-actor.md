# rpg.actor — Discovery Notes

rpg.actor is the host ecosystem for tassle. Tassle writes the Mage: the Ascension character sheet into `actor.rpg.stats`, and the rest of its energy model (`com.superbfowle.tass.*`) builds on top of rpg.actor's record + paired-record patterns. This file is an index of the rpg.actor lexicons, with notes on how each one relates to tassle.

## Publishing account

| | |
| --- | --- |
| **Handle** | [`@rpg.actor`](https://bsky.app/profile/rpg.actor) |
| **DID** | `did:plc:kwgllf365cwmxbnxitx4pjdj` |
| **PDS endpoint** | `https://rpg.actor` (self-hosted) |
| **Static lexicon URL** | `https://rpg.actor/lexicons/<NSID>.json` |
| **Developer guide** | [rpg.actor/dev-guide](https://rpg.actor/dev-guide) |
| **Systems reference** | [rpg.actor/systems](https://rpg.actor/systems) |
| **lexicon.garden identity** | [lexicon.garden/identity/did:plc:kwgllf365cwmxbnxitx4pjdj](https://lexicon.garden/identity/did:plc:kwgllf365cwmxbnxitx4pjdj) |
| **pds.ls** | [pds.ls/did:plc:kwgllf365cwmxbnxitx4pjdj](https://pds.ls/did:plc:kwgllf365cwmxbnxitx4pjdj) |

The rpg.actor PDS does not currently expose its lexicons as `com.atproto.lexicon.schema` records; they are served as static JSON at the well-known `/lexicons/<NSID>.json` path. The DNS `_atproto.rpg.actor` TXT record resolves to the DID above.

## Tassle's footprint in this namespace

The `actor.rpg.stats` lexicon is the canonical Mage sheet. Tassle reads and (eventually) mutates the `mage` system key inside it. Per-system rkey records are the current standard: `actor.rpg.stats/mage` is the canonical Mage payload, with the legacy `actor.rpg.stats/self` record kept as a deprecated compatibility mirror. The `mage` system key is one of rpg.actor's reserved native systems (see the [systems reference](https://rpg.actor/systems#mage)), so tassle operates in an officially-supported lane rather than inventing a private convention.

Tassle's own collections live under `com.superbfowle.tass.*` (see [`crates/tass-lex-schema/lexicons/`](../../crates/tass-lex-schema/lexicons/)) and reference this sheet via their `sheet` fields.

## Lexicons in this checkout

Six lexicons are snapshotted in [`doc/ref/`](../ref/). For each, the table below links the local checkout, the canonical lexicon.garden page, and the pds.ls NSID page.

| NSID | Local | lexicon.garden | pds.ls | Description |
| --- | --- | --- | --- | --- |
| `actor.rpg.stats` | [`doc/ref/actor.rpg.stats.json`](../ref/actor.rpg.stats.json) | [lexicon.garden/nsid/actor.rpg.stats](https://lexicon.garden/nsid/actor.rpg.stats) | [pds.ls/actor.rpg.stats](https://pds.ls/actor.rpg.stats) | Per-system rkey character sheet. Tassle's host record. |
| `actor.rpg.master` | [`doc/ref/actor.rpg.master.json`](../ref/actor.rpg.master.json) | [lexicon.garden/nsid/actor.rpg.master](https://lexicon.garden/nsid/actor.rpg.master) | [pds.ls/actor.rpg.master](https://pds.ls/actor.rpg.master) | Game-master validation of a player's sheet. |
| `actor.rpg.sprite` | [`doc/ref/actor.rpg.sprite.json`](../ref/actor.rpg.sprite.json) | [lexicon.garden/nsid/actor.rpg.sprite](https://lexicon.garden/nsid/actor.rpg.sprite) | [pds.ls/actor.rpg.sprite](https://pds.ls/actor.rpg.sprite) | 144×192 PNG character sprite with animation metadata. |
| `actor.rpg.generator` | [`doc/ref/actor.rpg.generator.json`](../ref/actor.rpg.generator.json) | [lexicon.garden/nsid/actor.rpg.generator](https://lexicon.garden/nsid/actor.rpg.generator) | [pds.ls/actor.rpg.generator](https://pds.ls/actor.rpg.generator) | Decomposed sprite layers + configuration. ⚠ Malformed upstream (see [doc/ref/README.md](../ref/README.md#known-data-quality-issues)). |
| `equipment.rpg.item` | [`doc/ref/equipment.rpg.item.json`](../ref/equipment.rpg.item.json) | [lexicon.garden/nsid/equipment.rpg.item](https://lexicon.garden/nsid/equipment.rpg.item) | [pds.ls/equipment.rpg.item](https://pds.ls/equipment.rpg.item) | Player-owned item, accepted from a provider's `.give`. |
| `equipment.rpg.give` | [`doc/ref/equipment.rpg.give.json`](../ref/equipment.rpg.give.json) | [lexicon.garden/nsid/equipment.rpg.give](https://lexicon.garden/nsid/equipment.rpg.give) | [pds.ls/equipment.rpg.give](https://pds.ls/equipment.rpg.give) | Provider attestation that an item was legitimately granted. |

## Conjectured relationships to tassle

These are working notes, not commitments. Bigger-picture cross-ecosystem ideas go in [`lexicon-ideas.md`](lexicon-ideas.md).

### `actor.rpg.stats` — the host record

TheMage sheet is tassle's anchor. The per-system rkey migration matters here: tassle should write `actor.rpg.stats/mage` (canonical modern path) rather than only patching the legacy `actor.rpg.stats/self > mage` field. The lexicon defines a dedicated `mageStats` def with `arete`, `quintessence`, `paradox`, `willpower`, and the nine spheres, plus a `mageVariableStat` shape for free-form fields. Tassle's `com.superbfowle.tass.{tassilize,meditate,enervate}` records reference this sheet via their `sheet` AT-URI field; the eventual "patch the sheet" command (`tass sheet --update`) should write back into `mageStats.quintessence` and friends.

### `actor.rpg.master` — the validation template

The `.master` lexicon is structurally exactly what tassle needs for **Node attestation**. Today a Mage declares their own Nodes (via `com.superbfowle.tass.node`); there is no notion of a Storyteller / cabal / consensus confirming that the Node exists in the shared world. The `.master` lexicon's `snapshotScope` enum (`none` / `custom` / `full`) is a clean template for "how strictly is this Node's state frozen":

- `none` — narrative Node, anyone can field it, no snapshot
- `custom` — Storyteller validates only rating + resonance, leaves ambient-quintessence fluid
- `full` — tournament Node, every field snapshotted, any player edit breaks attestation

The `campaign` field maps directly to a chronicle name, and `spriteCid` is the precedent for attaching a `tassFormCid` (the canonical Tass image) to a validation record.

### `actor.rpg.sprite` — material Tass visuals

A Tass object takes a coincidental form ("a silver coin", "a vial of ink", "a faded photograph"). Today tassle captures this as a freeform `tassForm` string on `com.superbfowle.tass.node` and a separate `com.superbfowle.tass.form` record for canonical templates. If Tass records should render in a UI — a Mage's inventory, a Node's accumulated objects — the `actor.rpg.sprite` shape (single PNG, animation grid, optional `source` AT-URI pointing back to the generator record that composed it) is a tidy template for "the look of this Tass". The optional `source` field is exactly the link a Tass record would want back to its originating Node.

### `actor.rpg.generator` — decomposed resonance layers

More speculative. The generator's layer model (ordered back-to-front, each layer a recolored PNG with sentinel channels for `main`/`sub1`/`sub2`/`sub3`, optional `subtractMask` for punching holes, `behindRows` for occlusion ordering) is overkill for tassle's needs. But the **abstraction** is suggestive: a Tass record carrying multiple resonance values (e.g. a Node with Dynamic + Primordial character) could be rendered as overlapping translucent layers — one per resonance axis, colored by sign and intensity — using the same ordered-composite idea. The colorway sentinel system even rhymes with resonance: instead of `main`/`sub1`/`sub2`/`sub3`, you'd have `dynamic`/`static`/`primordial`/`pattern`/`questing` color channels.

(That said: the upstream `actor.rpg.generator.json` is currently malformed — see [doc/ref/README.md](../ref/README.md#known-data-quality-issues) — so any concrete borrowing should wait for rpg.actor to fix the file.)

### `equipment.rpg.item` + `equipment.rpg.give` — Tass as inventory

This is the **most directly applicable** pattern. A Tass object is, mechanically, an inventory item: it is carried by a Mage, spent to fuel a working, and origin-trusted to a Node. The paired-record model translates almost line-for-line:

| rpg.actor | tassle analogue |
| --- | --- |
| Provider publishes `equipment.rpg.give` on their own PDS | Node (or its Storyteller) publishes a `.give`-equivalent attesting "this Tass crystallized at my Node on this date, with this resonance profile" |
| Player publishes `equipment.rpg.item` on their own PDS, referencing the `.give` | Mage publishes their Tass record referencing the Node's attestation, carrying the local state (current quintessence, wear, history of enervations) |
| `assetCid` on the `.give` locks the asset against tampering | A resonance-profile hash or quintessence-amount hash on the Node-side attestation locks the genesis state — the Mage cannot retroactively claim their Tass had more quintessence than was originally crystallized |
| `kind: "layer"` items participate in the sprite generator | "Active" Tass records could participate in sphere workings — a Node attuned to Forces could be required for Forces effects, the way a layer item fits a generator slot |
| `kind: "inventory"` items are display-only | "Passive" Tass (q.v. the `com.superbfowle.tass.form` canonical-form records) lives in the Mage's inventory without affecting any working |

A future `com.superbfowle.tass.tass` collection that supersedes or wraps the existing `tassilize` action could do worse than to literally re-use the `equipment.rpg.{item,give}` shapes with renamed fields. The integrity story (`assetCid` matching, give→item AT-URI strong-ref, deletion semantics for "destroyed" Tass) is already worked out.

## Data quality observations

- **`actor.rpg.generator.json` is malformed upstream.** See [doc/ref/README.md](../ref/README.md#known-data-quality-issues) for the diagnosis. The other five files parse cleanly.
- **`actor.rpg.stats.json` is large (162 KB, 76 defs)** because it carries the full schema for every native game system (dnd, dcc, rmmz, reverie, mage, cyberpunk2020, vampire, playtopia, pathfinder, starfinder, daggerheart, fashionista, clunscannon, spotem, plus a `custom` legacy slot). Tassle only needs the `mageStats` / `mageVariableStat` / `mageWillpower` defs for sheet mutation; the rest is informational.
- **Static-only publishing.** Unlike co/core and layers.pub (which publish their schemas as `com.atproto.lexicon.schema` records on their PDSes), rpg.actor currently serves lexicons only as static JSON. Consumers that discover schemas via firehose will not see rpg.actor schemas; they must hard-code the `https://rpg.actor/lexicons/<NSID>.json` path.

## See also

- [`lexicon-ideas.md`](lexicon-ideas.md) — cross-ecosystem design notes, including a deeper treatment of the `equipment.rpg.{item,give}` → Tass translation
- [`doc/ref/README.md`](../ref/README.md) — manifest of every snapshotted schema and its source URL
- [`doc/design.gpt.md`](../design.gpt.md) — tassle's design draft, which originally named `actor.rpg.stats/self` as the canonical sheet path (now superseded by the per-system rkey standard)
