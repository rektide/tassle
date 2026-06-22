# layers.pub — Discovery Notes

Layers is the most conjectural of tassle's three reference ecosystems. Where rpg.actor is tassle's host and co/core is its named workqueue/token inspiration, layers.pub is a **design language to learn from**: it is a theory-neutral, modular lexicon system for linguistic annotation that has already worked through several problems tassle is approaching cold — most notably how to keep a schema uncommitted to any one cosmology while still letting specific cosmologies plug in.

## Publishing account

| | |
| --- | --- |
| **Handle** | [`@layers.pub`](https://bsky.app/profile/layers.pub) |
| **DID** | `did:plc:grodm6zgmudwmhy3uyzoagaf` |
| **Source repo** | [layers-pub/layers](https://github.com/layers-pub/layers) on GitHub |
| **Static lexicon URL** | `https://raw.githubusercontent.com/layers-pub/layers/main/lexicons/pub/layers/<DIR>/<FILE>.json` |
| **Developer docs** | [docs.layers.pub](https://docs.layers.pub) |
| **Lexicon overview** | [docs.layers.pub/foundations/lexicon-overview](https://docs.layers.pub/foundations/lexicon-overview) |
| **lexicon.garden identity** | [lexicon.garden/identity/did:plc:grodm6zgmudwmhy3uyzoagaf](https://lexicon.garden/identity/did:plc:grodm6zgmudwmhy3uyzoagaf) |
| **pds.ls** | [pds.ls/did:plc:grodm6zgmudwmhy3uyzoagaf](https://pds.ls/did:plc:grodm6zgmudwmhy3uyzoagaf) |

Note: `layers.pub` itself is an empty GitHub Pages placeholder; the actual docs site is at `docs.layers.pub`. The repo is licensed CC-BY-SA-4.0, which is worth knowing if any schema text is ever copied verbatim.

## Lexicons in this checkout

37 record-bearing schemas + `defs.json` files are snapshotted in [`doc/ref/`](../ref/). XRPC `get*`/`list*` query lexicons are intentionally skipped. The full set is organized into four tiers (the [official overview](https://docs.layers.pub/foundations/lexicon-overview) has the dependency graph):

**Core pipeline (4):** `defs`, `expression.expression`, `segmentation.{segmentation,defs}`
**Annotation (3):** `annotation.{annotationLayer,clusterSet,defs}`
**Parallel support (15):** `ontology.{ontology,typeDef,defs}`, `corpus.{corpus,membership,defs}`, `resource.{entry,collection,template,filling,templateComposition,collectionMembership,defs}`, `judgment.{experimentDef,judgmentSet,agreementReport,defs}`, `alignment.alignment`
**Integration layers (9):** `graph.{graphNode,graphEdge,graphEdgeSet,defs}`, `persona.persona`, `media.{media,defs}`, `eprint.{eprint,dataLink,defs}`
**Cross-cutting (2):** `changelog.{entry,defs}`

The table below highlights the lexicons most relevant to tassle's design; the rest are listed for completeness in [`doc/ref/README.md`](../ref/README.md#files).

| NSID | Local | Why it matters to tassle |
| --- | --- | --- |
| `pub.layers.defs` | [`…/pub.layers.defs.json`](../ref/pub.layers.defs.json) | Foundational primitives (`objectRef`, `constraint`, `annotationMetadata`, `featureMap`, `knowledgeRef`, `alignmentLink`). `featureMap` is the open key/value map tassle's freeform resonance fields want. |
| `pub.layers.ontology.ontology` | [`…/pub.layers.ontology.ontology.json`](../ref/pub.layers.ontology.ontology.json) | The closest analogue to `com.superbfowle.tass.resonance`. A named ontology with `domain`, `parentRef`, `personaRef`, `knowledgeRefs`. |
| `pub.layers.ontology.typeDef` | [`…/pub.layers.ontology.typeDef.json`](../ref/pub.layers.ontology.typeDef.json) | The richer version of a resonance canonical: `typeKind`, `gloss`, `parentTypeRef`, `allowedRoles`, `allowedValues`. |
| `pub.layers.graph.graphNode` | [`…/pub.layers.graph.graphNode.json`](../ref/pub.layers.graph.graphNode.json) | Typed graph node for entities/concepts/claims. Models a resonance canonical as a graph node. |
| `pub.layers.graph.graphEdge` | [`…/pub.layers.graph.graphEdge.json`](../ref/pub.layers.graph.graphEdge.json) | Typed directed edge. Models `opposedTo` and resonance affinity relationships. |
| `pub.layers.persona.persona` | [`…/pub.layers.persona.persona.json`](../ref/pub.layers.persona.persona.json) | Agent personas + theoretical frameworks. Models an Avatar or a Paradigm. |
| `pub.layers.changelog.entry` | [`…/pub.layers.changelog.entry.json`](../ref/pub.layers.changelog.entry.json) | Structured change tracking with sub-record precision via `objectRef`. Models action records as sheet mutations. |
| `pub.layers.eprint.eprint` | [`…/pub.layers.eprint.eprint.json`](../ref/pub.layers.eprint.eprint.json) | Scholarly metadata + source citations. Anchors sphere rules to Mage source-book pages. |
| `pub.layers.eprint.dataLink` | [`…/pub.layers.eprint.dataLink.json`](../ref/pub.layers.eprint.dataLink.json) | Links a publication to the data it produced. Anchors a rules citation to the tassle record implementing it. |
| `pub.layers.resource.collection` | [`…/pub.layers.resource.collection.json`](../ref/pub.layers.resource.collection.json) | Named collection of entries. Models the canonical Tass-form registry. |
| `pub.layers.resource.entry` | [`…/pub.layers.resource.entry.json`](../ref/pub.layers.resource.entry.json) | Single entry in a collection. Models one canonical Tass form ("a silver coin"). |
| `pub.layers.alignment.alignment` | [`…/pub.layers.alignment.alignment.json`](../ref/pub.layers.alignment.alignment.json) | Cross-record correspondence. Links Tass records from the same Node, enervations to the same working. |

## Conjectured relationships to tassle

These are working notes, not commitments. The whole-ecosystem ideas go in [`lexicon-ideas.md`](lexicon-ideas.md).

### Theory-neutral schema as a design discipline

The single biggest lesson from layers.pub is the one stated in its first paragraph: *theory-neutral schema*. Layers represents all linguistic labels, categories, and formalisms as data values, not as schema — the same `annotationLayer` shape serves generative syntax, dependency grammar, construction grammar, and any future framework, because the framework-specific stuff lives in the **data** (in `ontology` records and `persona` records) rather than the **schema**.

Tassle's resonance system already half-accidentally does this. The `system` field on `com.superbfowle.tass.resonance` (`'mage'`, `'reverie'`, `'custom'`) is exactly a layers-pub-style theory switch: the schema doesn't commit to a specific cosmology, it provides scaffolding for any cosmology to plug in. The Mage Triat (Dynamic ↔ Static ↔ Primordial) and the Reverie axes (entropy/liberty/skeptic/receptive/authority/oblivion) are two different `system` values that use the same underlying `resonanceValue` shape (`{axis, value}` where `axis` is an at-URI or freeform string).

Layers' example suggests going further: instead of an enum string for `system`, the field could be an AT-URI strong-ref to a *persona* record that defines the cosmology in full. Mage Triat, Reverie axes, Vampire Road of Humanity, whatever — each is a published persona record that resonance records reference.

### `pub.layers.ontology` — the resonance registry, elaborated

The closest direct analogue in layers.pub. A `pub.layers.ontology.ontology` record is:

> An annotation ontology: a collection of typed definitions (entity types, situation types, relation types, roles).

Compare to tassle's `com.superbfowle.tass.resonance`:

> A canonical resonance type declaration. Each record defines one named axis that entities can be characterized along. Tagged with a 'system' so multiple game cosmologies coexist.

Same shape, different domain. The ontology schema has several pieces tassle's resonance schema lacks:

- **`parentRef`** — an ontology can declare a parent ontology. This is the clean way to express "this custom cosmology extends Mage Triat with two extra axes" without forking the resonance records themselves.
- **`personaRef`** — an ontology references a persona that defines its theoretical framework. This is the formal way to say "this cosmology comes from Mage: the Ascension, Revised Edition".
- **`knowledgeRefs[]`** — links to external KBs. For tassle this would link to the Mage wiki / source-book pages that define each resonance.
- **`typeDef` records with `allowedRoles` and `allowedValues`** — the resonance `value` (-1 to +1) is the simplest possible `allowedValues`. An ontology `typeDef` can express richer constraints like "Dynamic values must be ≥ 0 when paired with a Primordial axis ≤ 0".

A future `com.superbfowle.tass.resonance` v2 that borrows from `pub.layers.ontology` would gain formal cosmology hierarchies and source citations without changing the core `resonanceValue`/`resonanceProfile` data model.

### `pub.layers.graph` — resonance as typed property graph

The resonance axis/value pairs (`-1` to `+1`, with opposed poles) form a graph where each canonical resonance is a node and `opposedTo` is an edge. The Mage Triat is a 3-node graph:

```
   Dynamic
     ⇆
Static   Primordial
     ⇆
```

Tassle's `com.superbfowle.tass.resonanceProfile` is essentially a node-with-attributes — an entity (a Node, a Tass object, a Mage's Avatar) with positions on each axis. The `pub.layers.graph` trio (`graphNode`, `graphEdge`, `graphEdgeSet`) is the worked-out schema for this:

- `graphNode` — one canonical resonance with its properties
- `graphEdge` — one typed relationship between two canonicals (`opposedTo`, `complementary`, `derivative-of`)
- `graphEdgeSet` — a batch of edges (the whole Triat relationship set in one record)

Borrowing this would let resonance canonicals publish their full relationship structure (not just `opposedTo` as a single field) and let consumers reason over the graph rather than parsing ad-hoc string fields.

### `pub.layers.persona` — Avatars, Paradigms, and chronicle frameworks

A persona record in layers.pub is:

> Persona records define annotation frameworks and analyst perspectives.

For tassle, the same shape models three different Mage concepts that tassle currently has no good home for:

1. **An Avatar** — the Mage's essential nature. A Mystic Avatar seeks Primordial, a Questing Avatar seeks Dynamic, a Pattern Avatar seeks Static. Today tassle captures Avatar only as an integer rating in `actor.rpg.stats/mage.avatar`; a persona record would let a Mage publish "my Avatar is Questing, here is how it manifests, here is the cosmology it works in".
2. **A Paradigm** — the lens through which the Mage works magick (a Hermetic Mage versus a Virtual Adept versus a Verbena). A persona record with `framework` fields expresses this.
3. **A Storyteller's chronicle** — the rules-of-play contract for a specific campaign, including which resonance systems are recognized, which source books are authoritative, and what the Paradox rules are.

The `persona` record's structure (name, background, theoretical affiliation, expertise, framework references) is a clean template for all three.

### `pub.layers.changelog` — action records as sheet mutations

The changelog lexicon's principle — *structured change tracking for any record, with sub-record precision via `objectRef`* — is exactly what tassle's action records (`tassilize`, `meditate`, `enervate`) are, in miniature. A `pub.layers.changelog.entry`:

- targets any record via AT-URI
- categorizes change sections (annotations, segmentation, text, ontology, corpus, etc.)
- lists individual change items with type (`added`/`changed`/`removed`/`fixed`/`deprecated`), field path, before/after values
- optional semantic versioning for versioned records

Tassle's action records currently capture only "amount changed" without a structured before/after. A changelog-shaped `com.superbfowle.tass.action` (or a richer update to the existing records) would give every Mage a fully auditable career history: "on date D, working W caused quintessence 5 → 2 and paradox 0 → 1, refs Node N and Tass T". The `objectRef` precision would let a single action record describe a multi-field sheet patch (quintessence drop + paradox gain + sphere XP gain from the same working) without writing multiple records.

### `pub.layers.eprint` — Mage source-book citations

This is the relationship that sounds most stretchy and is actually quite practical. Every sphere rule, resonance definition, and Node property in tassle derives from a specific page in a Mage: the Ascension source book (Revised edition, M20, etc.). Today tassle captures none of that provenance — the `com.superbfowle.tass.resonance` record just says "cosmology: Wyld" without saying "Wyld is defined in Mage: the Ascension Revised, p. 167, and is opposed by Weaver (p. 167) and complemented by Wyrm (p. 168)".

The `pub.layers.eprint` pair does exactly this:

- `eprint` records the publication itself (DOI / arXiv / ACL Anthology / any platform identifier, plus citation metadata)
- `dataLink` connects a publication to the data it produced (in tassle's case: connects a source-book citation to the tassle record that implements the rule from that page)

A tassle-with-eprint would let a player verify "yes, this Paradox rule really is from M20 p. 203" by following the strong-ref chain from the action record to the eprint to the dataLink. Useful for chronicle arbiters; useful for porting tassle to other game systems; useful for academic-style engagement with the underlying RPG.

### `pub.layers.resource.{entry,collection}` — the Tass-form registry

`com.superbfowle.tass.form` is already a (very small) resource collection: each record names one canonical Tass form ("a silver coin", "a vial of ink"). The `pub.layers.resource` schema is the elaborated version:

- `collection` — the registry itself (a named, citable, versioned collection)
- `entry` — one form in the collection (lemma, form, language, MWE components)
- `template` + `filling` + `templateComposition` — parameterized generation of new forms from patterns

Tassle probably doesn't need the template/filling machinery (Tass forms are not morphologically generative in the linguistic sense). But the `collection` + `entry` shape — a published, citable, versioned registry of canonical forms — is the right abstraction for "the official list of Tass forms" versus "ad-hoc forms Mages have used in their own records".

### `pub.layers.alignment` — provenance chains for Tass

An alignment record establishes cross-record correspondence: token alignment across languages, span correspondence across segmentations, annotation equivalence across frameworks. For tassle, the same shape models:

- two Tass records from the same Node at different times (alignment `kind: genesis-cohort`)
- multiple enervations that fueled the same greater working (`kind: same-working`)
- a Mage's pattern state before and after a milestone (`kind: state-evolution`)

Tassle's current `sheet` AT-URI field is a degenerate one-link alignment. The full alignment schema gives a published, queryable way to express "these N records are all part of the same chronicle arc".

## Data quality observations

- **All 37 files parse cleanly.** No upstream bugs found.
- **The repo carries both record-bearing lexicons and XRPC `get*`/`list*` queries.** Tassle's checkout intentionally includes only the records + defs; if you want the full set, see the [source repo](https://github.com/layers-pub/layers/tree/main/lexicons/pub/layers).
- **`pub.layers.defs.json` lives at `lexicons/pub/layers/defs.json`**, not in a `defs/` subdirectory — different convention from co/core's flat layout. The checkout file is named `pub.layers.defs.json` (without a redundant `.defs` suffix).
- **Layers is at v0.7.0-draft status (per the project README).** The schemas are still moving. Borrowing should treat them as design inspiration rather than stable types to import verbatim.
- **License is CC-BY-SA-4.0.** If any schema text is ever copied verbatim into tassle's own lexicons, the share-alike obligation applies.

## See also

- [`lexicon-ideas.md`](lexicon-ideas.md) — cross-ecosystem design notes, including a deeper treatment of the theory-neutral schema discipline
- [`doc/ref/README.md`](../ref/README.md) — manifest of every snapshotted schema and its source URL
- [docs.layers.pub/foundations/design-principles](https://docs.layers.pub/foundations/design-principles) — the project's own write-up of the theory-neutral discipline
