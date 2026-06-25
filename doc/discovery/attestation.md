# Attestation / cosign options for tassle

> A comparison of three ATProto attestation patterns being considered for tassle's reality/master/mage cosign model. This doc is a sibling to the [round-4 resonance design](resonance-design.md), which already leans toward the keytrace.dev pattern; here we put that lean under pressure by comparing it against two alternatives before anything is built.
>
> **v0 reminder:** per the design brief, v0 ships with **no attestation at all** — master records compose into a working reality via an LSM, and character sheets reflect that. This doc is about what comes *after* v0 (v1+), when we want cryptographically-verifiable cosigns. We are comparing options now, not committing to a build.

---

## 1. The problem

Tassle is an energy ledger for Mage: The Ascension on ATProto. Per the [round-4 design](resonance-design.md), the authority model is: a **Reality** is a `pub.layers.persona.persona` record (the chronicle authority); it appoints **Masters** (atproto DIDs) via `pub.layers.graph.graphEdge`; **Mages** are players with `actor.rpg.stats` sheets; and **action records** (`com.superbfowle.tass.{tassilize,meditate,enervate}`) are self-published by mages to move energy.

The gap the round-4 design names explicitly (see [§ "The cosign pattern"](resonance-design.md)) is a way to accumulate trust on an action record:

1. A **mage self-attests** their own action (low trust — "I did this").
2. A **master cosigns** it (higher trust — a recognized authority vouches for it).
3. The **reality directly cosigns** (highest trust).
4. **Multiple authorities can cosign the same record**; trust accumulates.
5. A cosign can vouch for **specific fields only** — "I attest the amount drawn, but not the resonance profile the mage claimed."
6. Cosigns should be **retractable** (a master who later disagrees can withdraw).

Round 4 proposes that action records grow a `sigs[]` field of `dev.keytrace.signature` objects to deliver exactly this. Before that becomes load-bearing, this doc asks: is keytrace.dev actually the right primitive, or would co/core's `compute.attestation` or the `atproto-attestation` crate serve us better — or compose with keytrace?

---

## 2. The three technologies

### (a) keytrace.dev — identity-claim attestation

**What it is.** keytrace.dev is an ATProto-native **identity-verification service** built by Orta Therox (`@orta.io`, publishing DID `did:plc:hcwfdlmprcc335oixyfsw7u3`, namespace `dev.keytrace.*`). A user links their DID to an external account (GitHub, DNS, npm, ActivityPub, bsky, tangled) by posting a challenge to the external service; keytrace verifies the challenge and cryptographically attests to the link. It is the closest thing in the ATProto ecosystem to a general-purpose "a recognized authority signed off on this claim" primitive. See [`doc/ref/dev.keytrace.claim.json`](../ref/dev.keytrace.claim.json) and the manifest entry in [`doc/ref/README.md`](../ref/README.md).

**The records it publishes** (all in [`doc/ref/`](../ref/)):

- [`dev.keytrace.claim`](../ref/dev.keytrace.claim.json) — an identity-claim *record* (key `tid`) linking a DID to an external account; carries `type`, `claimUri`, `identity`, a `nonce`, lifecycle timestamps (`lastVerifiedAt`, `failedAt`, `retractedAt`), and a `sigs[]` array of attestations.
- [`dev.keytrace.serverPublicKey`](../ref/dev.keytrace.serverPublicKey.json) — a *verification service's* signing key, published as a **JWK** (RFC 7517) with `validFrom`/`validUntil` windows. This is the "official stamp" key.
- [`dev.keytrace.signature`](../ref/dev.keytrace.signature.json) — the attestation *object* (embedded in a claim's `sigs[]`): `kid`, `src` (AT-URI to the signing key record), `attestation` (base64 signature), `signedFields[]`, `signedAt`, `comment`, `retractedAt`.
- [`dev.keytrace.userPublicKey`](../ref/dev.keytrace.userPublicKey.json) — a user's own **PGP** public key (`publicKeyArmored`, `keyType`, `fingerprint`, `expiresAt`).
- [`dev.keytrace.statement`](../ref/dev.keytrace.statement.json) — a self-signed public *statement* (PGP `sig` over `content`, `keyRef` → the user's `userPublicKey`).
- [`dev.keytrace.profile`](../ref/dev.keytrace.profile.json) — singleton display settings.
- [`dev.keytrace.reverseLookup`](../ref/dev.keytrace.reverseLookup.json) — an XRPC *query* ("which DIDs verifiably claimed identity X?"). Note: present in the [GitHub source](https://github.com/orta/keytrace) but **not yet published** on the PDS.

**Who signs.** Two distinct signers, by design. A **verification service** (e.g. keytrace.dev itself) holds the private half of a JWK whose public part is a `serverPublicKey` record — it signs *claims*. Separately, an **end user** holds a PGP private key whose public half is a `userPublicKey` record — they sign their own *statements*. So the model already distinguishes "authority signs someone else's record" (server/JWK) from "subject signs their own record" (user/PGP). This maps almost exactly onto tassle's master-cosigns-action vs. mage-self-attests-action split.

**What's signed.** The fields named in `signedFields[]` — e.g. `['did', 'subject', 'type', 'verifiedAt']`. This is **field-level attestation**, and it is keytrace's standout feature for our use case: an authority can say "I vouch for the amount, not the resonance claim."

**How it's verified.** Resolve `signature.src` → the `serverPublicKey` (or `userPublicKey`) record; check the JWK is inside its `validFrom`/`validUntil` window and belongs to a trusted verifier; then cryptographically check the base64 `attestation` over the reconstructed `signedFields` payload. The [`@keytrace/claims`](https://www.npmjs.com/package/@keytrace/claims) npm library does this for external developers, which matters because tassle is TypeScript (see §4).

**Retraction.** First-class. `retractedAt` exists on `claim`, `signature`, `userPublicKey`, and `statement`. A withdrawing authority sets `retractedAt` on its `signature` object; consumers see the sig's `src` still resolves but check `retractedAt` and down-weight.

**Multi-signer.** Yes — a claim's `sigs[]` is an array of `dev.keytrace.signature#main`, so many authorities (or the subject + authorities) can each contribute an entry. Trust accumulates per-entry.

**Field-level attestation.** Yes, via `signedFields[]` — the headline feature.

**Upstream momentum.** Active: npm packages ([`@keytrace/claims`](https://www.npmjs.com/package/@keytrace/claims), [`@keytrace/lexicon`](https://www.npmjs.com/package/@keytrace/lexicon), [`@keytrace/runner`](https://www.npmjs.com/package/@keytrace/runner)), a live site at [keytrace.dev](https://keytrace.dev), and 6 NSIDs published on its PDS. Still pre-1.0 and authored primarily by one person (Orta), so there is key-person risk.

**License.** Lexicons are published records (no explicit license file cited in the lexicons); the [`orta/keytrace`](https://github.com/orta/keytrace) repo should be checked before any code reuse. Adopting the *pattern* (publishing `serverPublicKey` records and embedding `signature` objects) does not require a code dependency — only the `@keytrace/claims` *verifier* library would, and that is the part to license-check.

---

### (b) co/core `dev.cocore.compute.attestation` — hardware-anchored machine attestation

**What it is.** co/core (`@cocore.dev`, DID `did:plc:5quuhkmwe2q4k3azfsgg7kdz`, namespace `dev.cocore.*`) is a **compute marketplace**: requesters post compute jobs, providers fulfill them, exchanges settle payments. The [`dev.cocore.compute.attestation`](../ref/dev.cocore.compute.attestation.json) lexicon is the mechanism by which a **provider machine proves its own hardware/software state** so that a requester can trust the confidentiality and integrity of work done on it. It is an *infrastructure-integrity* attestation, not an identity or content attestation. See the manifest in [`doc/ref/README.md`](../ref/README.md).

**The records it publishes** (the attestation surface is one record, but it plugs into a larger set):

- [`dev.cocore.compute.attestation`](../ref/dev.cocore.compute.attestation.json) — the attestation *record* itself (key `tid`): a snapshot of a provider machine's state, **self-signed by its Secure Enclave**, content-addressed so many receipts strong-ref the same record until state changes.
- It is consumed by other co/core records not snapshotted here as the attestation focus: `dev.cocore.compute.provider` (holds the `attestationPubKey` this record's `publicKey` must equal), `dev.cocore.compute.receipt` (strong-refs an attestation to prove a job ran under it), and the dispute/settlement flow.

**Who signs.** The **provider machine's Apple Secure Enclave**. The signing key is a P-256 key sealed to the Secure Enclave identity; the same device also exposes an X25519 `encryptionPubKey` to prove a single device controls both signing and request-encryption keys. This is emphatically *not* a DID-held software key — it is hardware-bound.

**What's signed.** The whole record. `selfSignature` is a Secure Enclave P-256 signature (DER) over a **sorted-key canonical JSON of every other field** in the record. There is no field-level selection — the signature covers the entire machine-state snapshot.

**How it's verified.** Verifiers MUST reconstruct the canonical JSON byte-for-byte and check the P-256 signature. For the highest trust tier, the verifier additionally walks the `mdaCertChain` (Apple Managed Device Attestation, DER, leaf-first) to the Apple Enterprise Attestation Root CA, enforcing BasicConstraints, **and requires the leaf's public key to equal the record's `publicKey`** — so the chain is bound to the signing key, not stapled on. The confidentiality `tier` ([`dev.cocore.compute.defs#tier`](../ref/dev.cocore.compute.defs.json): `attested-confidential` / `best-effort`) is **recomputed by the verifier** from evidence (`cdHash` ∈ known-good, posture booleans like `hardenedRuntime`/`getTaskAllow=false`/`inProcessBackend`), never trusted from the record's self-asserted `tier` field. Freshness is enforced via `expiresAt` (default 24h).

**Retraction.** No explicit retraction. Instead it is **time-bounded** (`expiresAt`) and **content-addressed**: receipts strong-ref a specific attestation by CID, so when the machine's state changes (binary upgrade, OS update, key rotation) a *new* attestation record is published and subsequent receipts reference it. Old attestations are not edited or revoked — they just age out.

**Multi-signer.** No. One attestation = one `selfSignature` from one device. Trust here is single-source-by-design: the device attests itself, and a chain of trust (Apple MDA → leaf key → signature) anchors it. There is no "many authorities cosign one record" concept.

**Field-level attestation.** No — whole-record canonical JSON.

**Hardware anchoring.** This is co/core's defining property and the whole point: Secure Enclave, Apple MDA cert chain, code-signing `cdHash`, hardened-runtime/library-validation/anti-debug posture. It is the only one of the three with real hardware root-of-trust.

**Upstream momentum.** Active and substantial — `cocore.dev` is a shipping compute marketplace with a deep schema (17 lexicons in [`doc/ref/`](../ref/), covering attestation/receipt/settlement/dispute/exchange). It is, however, Apple-platform-specific (Secure Enclave + MDA are Apple primitives) and tightly coupled to the compute-marketplace domain.

**License.** Lexicons are published records on the PDS; the project lives at [cocore.dev](https://cocore.dev). Reuse of the *pattern* is pattern-only; there is no npm/Rust library to depend on for our case (and the signing side requires Apple hardware we do not have).

---

### (c) atproto-attestation — generic CID-first record signatures

**What it is.** A **Rust library** (not a record-publishing account) by Nick Gerakines (`@ngerakines.me`, DID `did:plc:cbkjy5n7bk3ax2wplmtjofq2`), part of the [`ngerakines.me/atproto-crates`](https://tangled.org/ngerakines.me/atproto-crates) workspace. It implements a **CID-first attestation convention**: any record can carry a `signatures[]` array of cryptographic signatures over a deterministic content identifier of the record. The source states it follows the spec in `bluesky-attestation-tee/documentation/spec/attestation.md` — a Bluesky-org spec. Full notes (there is **no lexicon JSON**, so a `.notes.md` stands in) are in [`doc/ref/atproto-attestation.notes.md`](../ref/atproto-attestation.notes.md).

**The records it publishes.** None of its own — it attaches to *any* record's `signatures[]`. It supports two modes:

- **Inline** — the signature object is embedded directly: `{ $type, key (did:key), issuer, issuedAt, cid, signature: { $bytes } }`.
- **Remote** — a separate *proof record* (stored in the attestor's own repo) carries the signature; the attested record references it via a `com.atproto.repo.strongRef` in its `signatures[]`.

**Who signs.** Any DID holding an ECDSA private key (P-256, P-384, or K-256/secp256k1). The public key is referenced by `did:key` (inline) or resolved from the attestor's DID-document verification methods (remote). There is **no dedicated key-record collection** — keys live in DID docs or `did:key` URIs, unlike keytrace's `serverPublicKey`/`userPublicKey` records.

**What's signed.** A **content CID**: the library canonicalizes `(record, attestation_metadata, repository_did)` to **DAG-CBOR**, computes a CID, and signs the CID *bytes*. This is whole-record, content-addressed signing — robust against canonicalization ambiguity because DAG-CBOR is deterministic.

**How it's verified.** `verify_record` iterates the `signatures[]`: resolve the attestor's key via a `KeyResolver` (DID document); for remote entries, fetch the proof record via a `RecordResolver`; reconstruct the content CID from `(record, metadata, repository)`; verify the ECDSA signature over the CID bytes. The **load-bearing anti-replay control** is that the `repository` DID passed to verify *must equal* the one used at signing — the repo DID is bound into the signed CID, so a signature for `did:plc:A` cannot be replayed against a clone in `did:plc:B`.

**Retraction.** **No built-in mechanism.** No `retractedAt` field. Revocation would have to be app-level (delete the proof record for remote mode; a separate revocation record for inline).

**Multi-signer.** Yes — `signatures[]` is an array, and each entry is independently verifiable against its own issuer/key.

**Field-level attestation.** **No.** It signs the CID of the whole record. There is no `signedFields` analogue. (One could hack partial attestation by signing a CID of a sub-object, but the library does not model that.)

**Hardware anchoring.** None in this crate — pure software ECDSA. (The upstream `bluesky-attestation-tee` *spec* targets Trusted Execution Environments, but this crate is the generic crypto/CID layer, not the TEE-binding layer.)

**Upstream momentum.** Alpha (`0.15.0-alpha.1`), actively developed as part of the 0.15 `atproto-crates` workspace, by the author of [lexicon.garden](https://lexicon.garden) — credible maintainer, but explicitly alpha and single-author.

**License.** **MIT** (workspace `Cargo.toml`) — the most clearly and permissively licensed of the three, and the only one that ships usable *code*.

---

## 3. Side-by-side comparison

| Concern | keytrace.dev | co/core `compute.attestation` | atproto-attestation |
| --- | --- | --- | --- |
| Primary use case | identity-claim verification (DID ↔ external account) | provider-machine hardware/software integrity for compute jobs | generic record-content attestation (any record) |
| Signing entity | verification service (JWK) **or** end user (PGP) | the provider machine's Apple Secure Enclave | any DID holding an ECDSA key |
| Signature scheme | JWK (server) / PGP (user); base64 attestation | Secure Enclave P-256 (DER) over canonical JSON | ECDSA P-256/P-384/K-256, low-S, over DAG-CBOR CID |
| Key publication | dedicated records: `serverPublicKey` (JWK, windowed) / `userPublicKey` (PGP) | `publicKey` in the attestation record, bound to MDA cert chain | DID-doc verification methods / `did:key` (no dedicated record) |
| Multi-signer support | **Yes** — `claim.sigs[]` array | **No** — single `selfSignature` per record | **Yes** — `signatures[]` array |
| Field-level attestation | **Yes** — `signedFields[]` | **No** — whole-record canonical JSON | **No** — whole-record CID |
| Retraction mechanism | **Yes** — `retractedAt` on claim/sig/key/statement | time-bounded (`expiresAt`) + content-addressed (new record on change) | **None** — app-level only |
| Replay protection | implicit (signed fields name the subject DID) | content-addressing + `expiresAt` | **Yes** — `repository` DID bound into signed CID |
| Trust model | accumulator: N recognized verifiers each add a sig | single hardware root-of-trust (Apple chain) | accumulator: N issuers each add a sig |
| ATProto records used | claim, serverPublicKey, signature, userPublicKey, statement, profile, reverseLookup | compute.attestation (+ provider/receipt that consume it) | none of its own — attaches to any record |
| Hardware anchoring | none | **strong** — Secure Enclave, MDA chain, cdHash | none (the *spec* targets TEEs; this crate doesn't) |
| Maintenance status | active, npm libs, live site; pre-1.0, single-author | active, shipping marketplace; Apple-platform-only | alpha (`0.15.0-alpha.1`), single-author |
| License | published records; repo license TBD | published records; project at cocore.dev | **MIT** (code) |
| Fit for tassle stack (TS/Node) | **excellent** — `@keytrace/claims` npm verifier | poor — Swift/Apple signing side, compute-domain-coupled | poor — Rust crate, would need a TS port/wasm |

---

## 4. Mapping to tassle

Tassle is a TypeScript/Node project ([`package.json`](../../package.json): `.ts` run directly via node, tsdown, vitest), publishing `com.superbfowle.tass.*` records. The cosign model needs: reality + master signing keys, mage self-attestation, accumulating cosigns on action records, field-level vouching, retraction, and a TS-friendly verifier. Mapping each technology onto that:

### keytrace.dev → tassle

This is the round-4 design's choice and the fit is genuinely good. The **reality** publishes its signing key as a [`dev.keytrace.serverPublicKey`](../ref/dev.keytrace.serverPublicKey.json) (JWK, with `validFrom`/`validUntil` for rotation). Each **appointed master** publishes their own `serverPublicKey` (same lexicon, different DID) — their cosigns are recognizable as "vouched by master X in reality Y" via the [appointment `graphEdge`](resonance-design.md). **Mages** self-attest with a [`dev.keytrace.userPublicKey`](../ref/dev.keytrace.userPublicKey.json) (PGP), exactly the user-key path keytrace already provides. **Cosigns accumulate** on each action record's `sigs[]` (an array of [`dev.keytrace.signature`](../ref/dev.keytrace.signature.json) objects); the mage's self-sig is one entry, each master's cosign is another, the reality's direct cosign is the highest-weight entry. **Field-level vouching** is free via `signedFields[]` — a master can attest `["actor","node","amount"]` without vouching for the resonance claim. **Retraction** is first-class (`retractedAt`). **Verification** for a consumer = resolve each `sig.src` → key record → check window + authority chain → verify the base64 attestation over `signedFields`, using [`@keytrace/claims`](https://www.npmjs.com/package/@keytrace/claims) (TypeScript — matches our stack). Integration cost is **low on the lexicon surface** (zero new tassle lexicons — round 4's "zero new lexicons" result) and **one npm dependency** (`@keytrace/claims`) plus whatever crypto its JWK/PGP verification needs. The main cost is operational key management (generating/rotating reality + master JWKs).

### co/core `compute.attestation` → tassle

Poor fit, included for the discipline it teaches rather than for adoption. co/core's attestation is **hardware-anchored to Apple Secure Enclave + MDA**, single-signer, whole-record, and domain-coupled to compute jobs. Tassle has no TEEs, no provider machines, and needs *multi-authority accumulating* cosigns — the opposite of co/core's single-device self-attestation. We cannot reuse the signing side (requires Apple hardware) and the schema fields (`cdHash`, `metallibHash`, `mdaCertChain`, `inProcessBackend`) are meaningless for an RPG ledger. **However**, co/core contributes two ideas worth borrowing: (1) **content-addressing** — the attestation is a stable record that consumers strong-ref by CID, so an attestation about a Node's rating is independently citable; and (2) the **"verifier MUST recompute, never trust the self-asserted field"** discipline (`tier` is advisory; the verifier reconstructs trust from evidence). That second idea is directly portable: a tassle consumer should never trust a self-asserted "this working was sanctioned" flag — it should recompute sanctioning from the cosign graph. Integration cost for *adoption* is high and unjustified; cost for *borrowing the discipline* is zero.

### atproto-attestation → tassle

Attractive engineering, awkward packaging. Its strengths align well with tassle: **multi-signer** `signatures[]`, **content-addressed** signing (DAG-CBOR CID is canonicalization-safe — more robust than keytrace's "sign a field list and hope the verifier reconstructs the payload identically"), and **repository-bound replay protection** (a cosign for a record in the mage's repo cannot be replayed against a clone elsewhere). Its weaknesses are exactly tassle's hard requirements: **no field-level attestation** (signs the whole record — we lose "I vouch for the amount, not the resonance"), **no retraction**, **no dedicated key records** (reality/master keys would live in DID docs, complicating rotation and windowing), and — decisively — **it is a Rust crate** and tassle is TypeScript. Adopting it means either porting the verify path to TS, compiling to wasm, or running a Rust sidecar, all for an **alpha** library. Integration cost is **high** (language mismatch + missing features), which is why it is the fallback rather than the primary. The one piece worth carrying forward regardless of choice is the **CID-first signing discipline** as a future hardening of whatever we build (see §5).

---

## 5. Recommendation

**Primary for v1+: keytrace.dev's pattern** — it is the only one of the three that satisfies all six cosign requirements out of the box (multi-signer, field-level, retractable, dedicated authority + subject key records, accumulator trust model) and it is the only one with a TypeScript verifier library (`@keytrace/claims`) that fits tassle's stack. This confirms the round-4 design's lean. Concretely: reality + masters publish `dev.keytrace.serverPublicKey` (JWK); mages publish `dev.keytrace.userPublicKey` (PGP); action records carry `sigs[]` of `dev.keytrace.signature` with `signedFields[]`; consumers verify with `@keytrace/claims` and recompute trust from the cosign graph (the co/core discipline). Zero new tassle lexicons for the attestation layer.

**Fallback: a tassle-native `signatures[]` shaped on atproto-attestation's CID-first model.** If keytrace stalls (single-author, pre-1.0) or its verifier proves awkward to embed, define a tassle-local cosign `$type` (e.g. `com.superbfowle.tass.cosign`) whose entries sign a **DAG-CBOR CID** of `(record, signedFields, repository)` — a deliberate hybrid: keytrace's UX surface (`sigs[]`, `signedFields`, `retractedAt`, key records) with atproto-attestation's canonicalization-safe CID signing under the hood. This hedges against canonicalization ambiguity in keytrace's field-list signing and keeps the whole thing in tassle's own namespace (MIT-licensed source we control) at the cost of one new lexicon and a hand-rolled verifier.

**Hybrid worth doing regardless:** adopt co/core's **"verifier must recompute trust from evidence"** rule even when using keytrace — never let a record's self-asserted sanctioning flag be load-bearing; always rederive it from the cosign graph and key-validity windows.

---

## 6. Open questions

1. **`@keytrace/claims` fitness.** Is the TypeScript verifier actually complete (JWK *and* PGP paths), browser/CLI friendly, and license-compatible? It is the single external dependency this recommendation hinges on — worth a spike before committing.

2. **Field-list vs CID canonicalization.** keytrace signs a *list of fields*; the verifier must reconstruct the payload identically. Is there a canonicalization-ambiguity risk (field ordering, encoding) that a CID-first approach (atproto-attestation) would eliminate? If yes, the hybrid fallback in §5 becomes more attractive.

3. **Key rotation & windows for reality/master keys.** keytrace's `serverPublicKey` has `validFrom`/`validUntil`. What's the rotation story when a reality's signing key is compromised — do all prior cosigns stay valid (windowed) and only new ones fail? Does a master losing their key invalidate their historical cosigns?

4. **Retraction propagation at scale.** `retractedAt` is per-signature. How does an appview/LSM efficiently down-weight retracted cosigns across the whole action history without re-scanning every record? Do we need a retraction index?

5. **Do we need any hardware anchoring?** co/core's Secure Enclave path is irrelevant for an RPG ledger, but is there *any* tassle value in binding a reality's key to something stronger than a software JWK (e.g. a `did:web` + key in a hardware-backed store)? Probably no — flagging only to confirm the "no TEE" assumption.

## See also

- [Round-4 resonance design](resonance-design.md) — the authority model this doc serves (Reality = `persona`, masters via `graphEdge`, cosigns via `dev.keytrace.signature`).
- [`doc/ref/dev.keytrace.signature.json`](../ref/dev.keytrace.signature.json), [`…serverPublicKey.json`](../ref/dev.keytrace.serverPublicKey.json), [`…userPublicKey.json`](../ref/dev.keytrace.userPublicKey.json), [`…claim.json`](../ref/dev.keytrace.claim.json) — keytrace schemas.
- [`doc/ref/dev.cocore.compute.attestation.json`](../ref/dev.cocore.compute.attestation.json), [`…compute.defs.json`](../ref/dev.cocore.compute.defs.json) — co/core schemas.
- [`doc/ref/atproto-attestation.notes.md`](../ref/atproto-attestation.notes.md) — atproto-attestation schema notes.
- [`doc/ref/README.md`](../ref/README.md) — full reference manifest.
