# `doc/ref/` — Upstream ATProto Lexicon Reference Checkouts

## Purpose

This directory holds local snapshots of upstream ATProto lexicon schemas that **tassle** depends on or draws design inspiration from. These are **manual snapshots** copied from their publishing accounts — they are *not* generated from tassle's own sources, and they may lag behind upstream. Always consult the source links below for the current canonical version.

Tassle's own lexicons live in [`lexicons/`](../../lexicons/) at the repository root:

- [`lexicons/com.superbfowle.tass.node.json`](../../lexicons/com.superbfowle.tass.node.json)
- [`lexicons/com.superbfowle.tass.form.json`](../../lexicons/com.superbfowle.tass.form.json)
- [`lexicons/com.superbfowle.tass.resonance.json`](../../lexicons/com.superbfowle.tass.resonance.json)
- [`lexicons/com.superbfowle.tass.enervate.json`](../../lexicons/com.superbfowle.tass.enervate.json)
- [`lexicons/com.superbfowle.tass.meditate.json`](../../lexicons/com.superbfowle.tass.meditate.json)
- [`lexicons/com.superbfowle.tass.tassilize.json`](../../lexicons/com.superbfowle.tass.tassilize.json)

## Sources

### rpg.actor

- **Publishing DID:** [`did:plc:kwgllf365cwmxbnxitx4pjdj`](https://lexicon.garden/identity/did:plc:kwgllf365cwmxbnxitx4pjdj) — handle `@rpg.actor` (PDS endpoint is `https://rpg.actor` itself)
- **Base URL (fetched from):** `https://rpg.actor/lexicons/<NSID>.json`
- **Developer docs:** [https://rpg.actor](https://rpg.actor) · [dev guide](https://rpg.actor/dev-guide)
- **lexicon.garden identity:** [https://lexicon.garden/identity/did:plc:kwgllf365cwmxbnxitx4pjdj](https://lexicon.garden/identity/did:plc:kwgllf365cwmxbnxitx4pjdj)
- **pds.ls:** [https://pds.ls/did:plc:kwgllf365cwmxbnxitx4pjdj](https://pds.ls/did:plc:kwgllf365cwmxbnxitx4pjdj)
- **Note:** The 6 `rpg.actor` / `equipment.rpg` files were already present in this directory prior to this fetch; they are preserved as-is.

### co/core

- **Publishing DID:** [`did:plc:5quuhkmwe2q4k3azfsgg7kdz`](https://lexicon.garden/identity/did:plc:5quuhkmwe2q4k3azfsgg7kdz) — handle `@cocore.dev`
- **Base URL (fetched from):** `https://cocore.dev/lexicons/<NSID>.json`
- **Developer docs:** [https://cocore.dev](https://cocore.dev)
- **lexicon.garden identity:** [https://lexicon.garden/identity/did:plc:5quuhkmwe2q4k3azfsgg7kdz](https://lexicon.garden/identity/did:plc:5quuhkmwe2q4k3azfsgg7kdz)
- **pds.ls:** [https://pds.ls/did:plc:5quuhkmwe2q4k3azfsgg7kdz](https://pds.ls/did:plc:5quuhkmwe2q4k3azfsgg7kdz)
- **Note:** 13 of 17 files were pulled from the static `cocore.dev` mirror. The remaining 4 NSIDs (`dev.cocore.defs`, `dev.cocore.account.friend`, `dev.cocore.account.tokenGrant`, `dev.cocore.account.tokenPatronage`) return HTTP 404 on the static site and were instead reconstructed from the account's `com.atproto.lexicon.schema` records on its PDS, normalized to the same key layout (`lexicon`, `id`, `description`, `defs`).

### layers.pub

- **Publishing DID:** [`did:plc:grodm6zgmudwmhy3uyzoagaf`](https://lexicon.garden/identity/did:plc:grodm6zgmudwmhy3uyzoagaf) — handle `@layers.pub`
- **Base URL (fetched from):** `https://raw.githubusercontent.com/layers-pub/layers/main/lexicons/pub/layers/<DIR>/<FILE>.json`
- **Source repo:** [layers-pub/layers](https://github.com/layers-pub/layers) (GitHub)
- **Developer docs:** [https://layers.pub](https://layers.pub)
- **lexicon.garden identity:** [https://lexicon.garden/identity/did:plc:grodm6zgmudwmhy3uyzoagaf](https://lexicon.garden/identity/did:plc:grodm6zgmudwmhy3uyzoagaf)
- **pds.ls:** [https://pds.ls/did:plc:grodm6zgmudwmhy3uyzoagaf](https://pds.ls/did:plc:grodm6zgmudwmhy3uyzoagaf)
- **Note:** Only record-bearing schemas and `defs.json` files are included; the `get*.json` / `list*.json` XRPC query lexicons are intentionally skipped.

### marque.at

- **Publishing DID:** [`did:plc:nckosudltxrtrjkt4zz4jy5y`](https://lexicon.garden/identity/did:plc:nckosudltxrtrjkt4zz4jy5y) — handle `@marque.at`
- **Base URL (fetched from):** `com.atproto.lexicon.schema` records on PDS `https://margin.cafe` — no static JSON mirror exists (`marque.at/lexicons/<NSID>.json` returns 404).
- **Developer docs:** [https://marque.at](https://marque.at)
- **lexicon.garden identity:** [https://lexicon.garden/identity/did:plc:nckosudltxrtrjkt4zz4jy5y](https://lexicon.garden/identity/did:plc:nckosudltxrtrjkt4zz4jy5y)
- **pds.ls:** [https://pds.ls/did:plc:nckosudltxrtrjkt4zz4jy5y](https://pds.ls/did:plc:nckosudltxrtrjkt4zz4jy5y)
- **Note:** DID↔handle binding confirmed via DNS TXT on both `_atproto.marque.at` and `_lexicon.marque.at` (both resolve to this DID). Publishes lexicons **only** via `com.atproto.lexicon.schema` records on its PDS (`margin.cafe`); there is no static mirror. All 10 files were reconstructed from the PDS records and normalized to the standard `{lexicon, id, description?, defs}` layout (the wrapping `$type` field is stripped). The namespace covers domain registration (`at.marque.domain`), DNS zone + DNSSEC management (`at.marque.dns` and its `getRecords` / `getDsRecords` queries), a partner/reseller checkout API (`at.marque.partner.*`, Stripe-based), and two auth permission-sets (`authFull`, `partnerApi`).

## Files

| File | Source | NSID | Description | Notes |
| --- | --- | --- | --- | --- |
| [`actor.rpg.generator.json`](actor.rpg.generator.json) | [rpg.actor](https://rpg.actor) | `actor.rpg.generator` | Generator metadata and decomposed layers for a sprite built with the rpg.acto… | **Malformed upstream** — see [Known data quality issues](#known-data-quality-issues) |
| [`actor.rpg.master.json`](actor.rpg.master.json) | [rpg.actor](https://rpg.actor) | `actor.rpg.master` | A game master's validation of a player's RPG data for a specific system. One … |  |
| [`actor.rpg.sprite.json`](actor.rpg.sprite.json) | [rpg.actor](https://rpg.actor) | `actor.rpg.sprite` | A sprite sheet for an RPG character avatar. |  |
| [`actor.rpg.stats.json`](actor.rpg.stats.json) | [rpg.actor](https://rpg.actor) | `actor.rpg.stats` | RPG character statistics for multiple game systems. Per-system rkey records a… |  |
| [`equipment.rpg.give.json`](equipment.rpg.give.json) | [rpg.actor](https://rpg.actor) | `equipment.rpg.give` | A provider's attestation that an item was given to a player. Lives on the pro… |  |
| [`equipment.rpg.item.json`](equipment.rpg.item.json) | [rpg.actor](https://rpg.actor) | `equipment.rpg.item` | A player's owned item, accepted from a provider's give record. Lives on the p… |  |
| [`dev.cocore.account.defs.json`](dev.cocore.account.defs.json) | [co/core](https://cocore.dev) | `dev.cocore.account.defs` | Shared type definitions for dev.cocore.account.* methods. |  |
| [`dev.cocore.account.friend.json`](dev.cocore.account.friend.json) | [co/core](https://cocore.dev) | `dev.cocore.account.friend` | A one-way declaration that the publishing DID trusts the subject DID enough t… | Sourced from PDS lexicon record; static site returns 404 for this NSID |
| [`dev.cocore.account.profile.json`](dev.cocore.account.profile.json) | [co/core](https://cocore.dev) | `dev.cocore.account.profile` | A user's cocore-side profile. Auto-provisioned at first sign-in from the user… |  |
| [`dev.cocore.account.tokenGrant.json`](dev.cocore.account.tokenGrant.json) | [co/core](https://cocore.dev) | `dev.cocore.account.tokenGrant` | Records that an exchange has issued its one-time signup token grant to a reci… | Sourced from PDS lexicon record; static site returns 404 for this NSID |
| [`dev.cocore.account.tokenPatronage.json`](dev.cocore.account.tokenPatronage.json) | [co/core](https://cocore.dev) | `dev.cocore.account.tokenPatronage` | Records a patronage rebate: a periodic distribution of treasury balance back … | Sourced from PDS lexicon record; static site returns 404 for this NSID |
| [`dev.cocore.compute.attestation.json`](dev.cocore.compute.attestation.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.attestation` | A snapshot of a provider machine's hardware and software state, signed by its… |  |
| [`dev.cocore.compute.defs.json`](dev.cocore.compute.defs.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.defs` | Shared type definitions for dev.cocore.compute.* records. |  |
| [`dev.cocore.compute.dispute.json`](dev.cocore.compute.dispute.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.dispute` | An exchange-signed adjudication of a complaint about a settled receipt. Publi… |  |
| [`dev.cocore.compute.exchangeAttestation.json`](dev.cocore.compute.exchangeAttestation.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.exchangeAttestation` | An exchange's self-published statement of operating posture: software commit,… |  |
| [`dev.cocore.compute.exchangePolicy.json`](dev.cocore.compute.exchangePolicy.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.exchangePolicy` | An exchange's published terms of service. Records every parameter that affect… |  |
| [`dev.cocore.compute.job.json`](dev.cocore.compute.job.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.job` | A request for computational work. Published by the requester in their own rep… |  |
| [`dev.cocore.compute.paymentAuthorization.json`](dev.cocore.compute.paymentAuthorization.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.paymentAuthorization` | A requester's signed authorization permitting a named exchange to charge them… |  |
| [`dev.cocore.compute.provider.json`](dev.cocore.compute.provider.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.provider` | A compute provider's public profile. One record per physical machine. The DID… |  |
| [`dev.cocore.compute.receipt.json`](dev.cocore.compute.receipt.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.receipt` | A signed receipt of a single completed compute job. Published by the provider… |  |
| [`dev.cocore.compute.settlement.json`](dev.cocore.compute.settlement.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.settlement` | An exchange's signed proof of payment for a receipt. Published by the exchang… |  |
| [`dev.cocore.compute.termsAcceptance.json`](dev.cocore.compute.termsAcceptance.json) | [co/core](https://cocore.dev) | `dev.cocore.compute.termsAcceptance` | A user's affirmative acceptance of an exchange's terms of service / privacy p… |  |
| [`dev.cocore.defs.json`](dev.cocore.defs.json) | [co/core](https://cocore.dev) | `dev.cocore.defs` | _(shared type definitions; no description)_ | Sourced from PDS lexicon record; static site returns 404 for this NSID |
| [`pub.layers.alignment.alignment.json`](pub.layers.alignment.alignment.json) | [layers.pub](https://layers.pub) | `pub.layers.alignment.alignment` | Alignment records for parallel structure correspondence. Handles interlinear … |  |
| [`pub.layers.annotation.annotationLayer.json`](pub.layers.annotation.annotationLayer.json) | [layers.pub](https://layers.pub) | `pub.layers.annotation.annotationLayer` | A named layer of annotations over an expression. All annotation types use thi… |  |
| [`pub.layers.annotation.clusterSet.json`](pub.layers.annotation.clusterSet.json) | [layers.pub](https://layers.pub) | `pub.layers.annotation.clusterSet` | Groups annotations into equivalence classes. Used for coreference resolution … |  |
| [`pub.layers.annotation.defs.json`](pub.layers.annotation.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.annotation.defs` | _(shared type definitions; no description)_ |  |
| [`pub.layers.changelog.defs.json`](pub.layers.changelog.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.changelog.defs` | _(shared type definitions; no description)_ |  |
| [`pub.layers.changelog.entry.json`](pub.layers.changelog.entry.json) | [layers.pub](https://layers.pub) | `pub.layers.changelog.entry` | A changelog entry describing changes to any Layers record. |  |
| [`pub.layers.corpus.corpus.json`](pub.layers.corpus.corpus.json) | [layers.pub](https://layers.pub) | `pub.layers.corpus.corpus` | A corpus: a curated collection of expressions. |  |
| [`pub.layers.corpus.defs.json`](pub.layers.corpus.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.corpus.defs` | _(shared type definitions; no description)_ |  |
| [`pub.layers.corpus.membership.json`](pub.layers.corpus.membership.json) | [layers.pub](https://layers.pub) | `pub.layers.corpus.membership` | A record indicating that a expression belongs to a corpus, with optional spli… |  |
| [`pub.layers.defs.json`](pub.layers.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.defs` | Shared definitions for the Layers lexicons. Provides abstract anchoring primi… |  |
| [`pub.layers.eprint.dataLink.json`](pub.layers.eprint.dataLink.json) | [layers.pub](https://layers.pub) | `pub.layers.eprint.dataLink` | A link from an eprint to the Layers data it produced or is associated with. G… |  |
| [`pub.layers.eprint.defs.json`](pub.layers.eprint.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.eprint.defs` | _(shared type definitions; no description)_ |  |
| [`pub.layers.eprint.eprint.json`](pub.layers.eprint.eprint.json) | [layers.pub](https://layers.pub) | `pub.layers.eprint.eprint` | A link between a Layers data record and an eprint. |  |
| [`pub.layers.expression.expression.json`](pub.layers.expression.expression.json) | [layers.pub](https://layers.pub) | `pub.layers.expression.expression` | An Expression is the primary document model in Layers. It represents any ling… |  |
| [`pub.layers.graph.defs.json`](pub.layers.graph.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.graph.defs` | Shared object definitions for the graph namespace. |  |
| [`pub.layers.graph.graphEdge.json`](pub.layers.graph.graphEdge.json) | [layers.pub](https://layers.pub) | `pub.layers.graph.graphEdge` | A single directed typed edge between any two Layers objects. Supports multidi… |  |
| [`pub.layers.graph.graphEdgeSet.json`](pub.layers.graph.graphEdgeSet.json) | [layers.pub](https://layers.pub) | `pub.layers.graph.graphEdgeSet` | A batch of typed, directed edges between Layers objects. Use for bulk edge cr… |  |
| [`pub.layers.graph.graphNode.json`](pub.layers.graph.graphNode.json) | [layers.pub](https://layers.pub) | `pub.layers.graph.graphNode` | A standalone node in the property graph. Represents entities, concepts, situa… |  |
| [`pub.layers.judgment.agreementReport.json`](pub.layers.judgment.agreementReport.json) | [layers.pub](https://layers.pub) | `pub.layers.judgment.agreementReport` | An inter-annotator agreement report summarizing agreement metrics across judg… |  |
| [`pub.layers.judgment.defs.json`](pub.layers.judgment.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.judgment.defs` | Shared object definitions for the judgment namespace. |  |
| [`pub.layers.judgment.experimentDef.json`](pub.layers.judgment.experimentDef.json) | [layers.pub](https://layers.pub) | `pub.layers.judgment.experimentDef` | Definition of an annotation or judgment experiment. |  |
| [`pub.layers.judgment.judgmentSet.json`](pub.layers.judgment.judgmentSet.json) | [layers.pub](https://layers.pub) | `pub.layers.judgment.judgmentSet` | A set of judgments from a single annotator for an experiment. |  |
| [`pub.layers.media.defs.json`](pub.layers.media.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.media.defs` | _(shared type definitions; no description)_ |  |
| [`pub.layers.media.media.json`](pub.layers.media.media.json) | [layers.pub](https://layers.pub) | `pub.layers.media.media` | Media source records for audio, video, image, and document data associated wi… |  |
| [`pub.layers.ontology.defs.json`](pub.layers.ontology.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.ontology.defs` | _(shared type definitions; no description)_ |  |
| [`pub.layers.ontology.ontology.json`](pub.layers.ontology.ontology.json) | [layers.pub](https://layers.pub) | `pub.layers.ontology.ontology` | An annotation ontology: a collection of typed definitions (entity types, situ… |  |
| [`pub.layers.ontology.typeDef.json`](pub.layers.ontology.typeDef.json) | [layers.pub](https://layers.pub) | `pub.layers.ontology.typeDef` | A type definition within an ontology. Covers entity types, situation types, r… |  |
| [`pub.layers.persona.persona.json`](pub.layers.persona.persona.json) | [layers.pub](https://layers.pub) | `pub.layers.persona.persona` | Persona records define annotation frameworks and analyst perspectives. Differ… |  |
| [`pub.layers.resource.collection.json`](pub.layers.resource.collection.json) | [layers.pub](https://layers.pub) | `pub.layers.resource.collection` | A named collection of linguistic resource entries. Abstract enough to represe… |  |
| [`pub.layers.resource.collectionMembership.json`](pub.layers.resource.collectionMembership.json) | [layers.pub](https://layers.pub) | `pub.layers.resource.collectionMembership` | Links an entry to a collection. Separate record enables many-to-many relation… |  |
| [`pub.layers.resource.defs.json`](pub.layers.resource.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.resource.defs` | Shared object definitions for the resource namespace. |  |
| [`pub.layers.resource.entry.json`](pub.layers.resource.entry.json) | [layers.pub](https://layers.pub) | `pub.layers.resource.entry` | A linguistic resource entry: a lexical item, frame element filler, morphologi… |  |
| [`pub.layers.resource.filling.json`](pub.layers.resource.filling.json) | [layers.pub](https://layers.pub) | `pub.layers.resource.filling` | A filled template: a template with all slots mapped to specific fillers, prod… |  |
| [`pub.layers.resource.template.json`](pub.layers.resource.template.json) | [layers.pub](https://layers.pub) | `pub.layers.resource.template` | A parameterized text template with named variable slots. Generalizes bead's T… |  |
| [`pub.layers.resource.templateComposition.json`](pub.layers.resource.templateComposition.json) | [layers.pub](https://layers.pub) | `pub.layers.resource.templateComposition` | A composition of templates (sequence, tree, or other structure). Used for mul… |  |
| [`pub.layers.segmentation.defs.json`](pub.layers.segmentation.defs.json) | [layers.pub](https://layers.pub) | `pub.layers.segmentation.defs` | _(shared type definitions; no description)_ |  |
| [`pub.layers.segmentation.segmentation.json`](pub.layers.segmentation.segmentation.json) | [layers.pub](https://layers.pub) | `pub.layers.segmentation.segmentation` | A segmentation record that binds one or more tokenizations to an expression. … |  |
| [`at.marque.authFull.json`](at.marque.authFull.json) | [marque.at](https://marque.at) | `at.marque.authFull` | (permission-set; no description) |  |
| [`at.marque.dns.getDsRecords.json`](at.marque.dns.getDsRecords.json) | [marque.at](https://marque.at) | `at.marque.dns.getDsRecords` | Get the DNSSEC DS records for a managed zone. |  |
| [`at.marque.dns.getRecords.json`](at.marque.dns.getRecords.json) | [marque.at](https://marque.at) | `at.marque.dns.getRecords` | Get the active DNS records for a managed zone. |  |
| [`at.marque.dns.json`](at.marque.dns.json) | [marque.at](https://marque.at) | `at.marque.dns` | DNS zone records for a domain where Marque handles the nameservers. Stored in… |  |
| [`at.marque.domain.json`](at.marque.domain.json) | [marque.at](https://marque.at) | `at.marque.domain` | A domain registration managed by Marque. Stored in the user's repository. |  |
| [`at.marque.partner.checkAvailability.json`](at.marque.partner.checkAvailability.json) | [marque.at](https://marque.at) | `at.marque.partner.checkAvailability` | Check availability and pricing for a batch of domains, for partner integrations. |  |
| [`at.marque.partner.createCheckout.json`](at.marque.partner.createCheckout.json) | [marque.at](https://marque.at) | `at.marque.partner.createCheckout` | Create a hosted Stripe Checkout session to register one or more domains for t… |  |
| [`at.marque.partner.getOrder.json`](at.marque.partner.getOrder.json) | [marque.at](https://marque.at) | `at.marque.partner.getOrder` | Get the status of a partner checkout order and the resulting domain details. |  |
| [`at.marque.partner.listPricing.json`](at.marque.partner.listPricing.json) | [marque.at](https://marque.at) | `at.marque.partner.listPricing` | List the TLDs Marque offers and their prices, for partner integrations. |  |
| [`at.marque.partnerApi.json`](at.marque.partnerApi.json) | [marque.at](https://marque.at) | `at.marque.partnerApi` | (permission-set; no description) |  |

## Known data quality issues

- **[`actor.rpg.generator.json`](actor.rpg.generator.json) is malformed upstream and does not parse as JSON.** It is preserved exactly as published (the bug is in the source account's record, not introduced here). The structural defect: around lines 9–10 the `record: { type: "object", properties: { bodyType: { ... } } }` opening is missing — the `"key": "literal:self"` field is followed immediately by the *body's* field properties (`type`, `maxLength`, `description` for `bodyType`, then `skin`, …) which leak directly into the `main` def at the wrong nesting level. The file then carries extra closing braces at the end to compensate for the missing opening structure. Compare with the well-formed [`actor.rpg.master.json`](actor.rpg.master.json) (lines 6–13) for the intended `record → properties` shape. The top-level `description` ("Generator metadata and decomposed layers for a sprite built with the rpg.actor sprite generator.") is intact and is reflected in the table above; only the `defs.main` body is broken.
