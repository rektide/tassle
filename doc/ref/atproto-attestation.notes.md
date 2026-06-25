# atproto-attestation — schema notes (no published lexicon JSON)

> These are notes on the Rust crate [`atproto-attestation`](https://tangled.org/ngerakines.me/atproto-crates/tree/main/crates/atproto-attestation) from the [`ngerakines.me/atproto-crates`](https://tangled.org/ngerakines.me/atproto-crates) workspace. **There is no published ATProto lexicon JSON for this namespace** — the crate is a *library* that defines an attestation *convention* (`signatures[]` on any record), not a record-publishing account. This file summarizes the schema as it appears in the source so the convention is referenceable alongside the other `doc/ref/` lexicons.

## Identity & provenance

- **Author:** Nick Gerakines (`<nick.gerakines@gmail.com>`) — also the author of [lexicon.garden](https://lexicon.garden).
- **Author handle / DID:** `@ngerakines.me` → `did:plc:cbkjy5n7bk3ax2wplmtjofq2` (confirmed via DNS TXT `_atproto.ngerakines.me`).
- **Repo host:** [tangled.org](https://tangled.org) (an atproto-native git host). The repo record lives under repo DID `did:plc:ce6clisfsopkw4xddevgml26`; raw blobs are served via `mirror.tangled.network/xrpc/sh.tangled.git.temp.getBlob`.
- **License:** MIT (workspace `Cargo.toml`).
- **Version / status:** `0.15.0-alpha.1` — **alpha**, actively developed as part of the `atproto-crates` 0.15 workspace (Rust edition 2024, MSRV 1.90).
- **Upstream spec:** the source doc-comments state it "follows the requirements documented in `bluesky-attestation-tee/documentation/spec/attestation.md`" — i.e. it is a Rust implementation of a **Bluesky-org** attestation spec (the `bluesky-attestation-tee` repo, TEE = Trusted Execution Environment). This crate is the generic crypto/CID layer of that spec.

## What it is

A library for creating and verifying **cryptographic attestations over ATProto records** using a **CID-first workflow**. It supports two modes:

- **Inline attestation** — the signature is embedded directly in the attested record's `signatures[]` array.
- **Remote attestation** — a separate *proof record* (stored in the attestor's own repo) carries the signature; the attested record references it via a `com.atproto.repo.strongRef` in its `signatures[]` array.

The central security property is **replay-attack prevention via repository binding**: the signed payload always includes the DID of the repository that houses the record, so a signature produced for `did:plc:A` cannot be replayed against a clone of the record in `did:plc:B`.

## The schema (the `signatures[]` convention)

The crate does **not** define an NSID. It appends a `signatures` array to *any* record. Each entry is an attestation object whose shape is supplied by the caller (the `$type` is caller-chosen — the README uses placeholders like `com.example.inlineSignature`). The fields the crate itself populates/reads:

### Inline entry shape

```json
{
  "$type": "com.example.inlineSignature",
  "key": "did:key:zQ3sh...",
  "issuer": "did:plc:issuer123",
  "issuedAt": "2024-01-01T00:00:00.000Z",
  "cid": "bafyrei...",
  "signature": {
    "$bytes": "base64-encoded-normalized-signature-bytes"
  }
}
```

### Remote entry shape (in the attested record's `signatures[]`)

A `strongRef` (`{ uri, cid }`) pointing at a proof record stored in the attestor's repository. The proof record holds the CID + signature bytes.

### `$sig` metadata (fed into the CID)

The README calls the attestation metadata `$sig`. It must include `$type`; the crate **auto-injects `repository`** (the housing repo DID) so it is bound into the content CID. Typical inline metadata: `{ $type, key, issuer, issuedAt }`. Typical remote metadata: `{ $type, issuer, purpose }`.

## Signing algorithm (`create_signature` / `create_inline_attestation` / `create_remote_attestation`)

1. **Build the content CID** (`create_attestation_cid` → `create_dagbor_cid`): deterministic **DAG-CBOR** canonical serialization of the tuple `(record, attestation_metadata, repository_did)`, then a multihash/CID.
2. **Sign the CID bytes** with ECDSA (`atproto_identity::key::sign`).
3. **Normalize to low-S** (`normalize_signature`) to prevent signature malleability.
4. **Embed or reference:**
   - inline → push `{ $type, key, issuer, issuedAt, cid, signature: { $bytes } }` into the record's `signatures[]`;
   - remote → emit a `(attested_record, proof_record)` pair; the attested record's `signatures[]` carries a strongRef to the proof record (which lives in the attestor's repo).

## Verification algorithm (`verify_record`)

For each entry in a record's `signatures[]`:

1. **Resolve the attestor's key** via a `KeyResolver` (e.g. `IdentityDocumentKeyResolver`) — i.e. resolve the issuer DID document and select the verification method / public key.
2. **For remote attestations**, resolve the strongRef by fetching the proof record through a `RecordResolver` (HTTP `com.atproto.repo.getRecord`).
3. **Reconstruct the content CID** from `(record, metadata, repository_did)`.
4. **Verify the ECDSA signature** over the CID bytes against the resolved public key.
5. **Replay check:** the `repository` DID passed to `verify_record` **must equal** the one used at signing time — otherwise verification fails. (This is the load-bearing anti-replay control.)

## Cryptography

- **Curves:** P-256 (`p256`), P-384, K-256 / secp256k1 (`k256`). Chosen via `KeyType::{P256Private, P384Private, K256Private}`.
- **Signature encoding:** raw ECDSA bytes, base64-encoded under `signature.$bytes`; low-S normalized.
- **Canonicalization:** DAG-CBOR (deterministic CBOR) → CID. `sha2` + `multihash` + `cid` crates.
- **Key publication:** the signing public key is referenced by `did:key` in inline metadata, or by the attestor's DID document verification methods (resolved at verify time). There is **no dedicated "public key record" collection** like keytrace's `serverPublicKey` — keys live in DID docs / `did:key`.

## Property matrix (for the comparison in [`../discovery/attestation.md`](../discovery/attestation.md))

| Concern | atproto-attestation |
| --- | --- |
| Signing entity | any DID holding an ECDSA key (P-256/P-384/K-256) |
| Signature scheme | ECDSA over DAG-CBOR content CID, low-S normalized |
| Multi-signer | **Yes** — `signatures[]` array, many independent entries |
| Field-level attestation | **No** — signs the CID of the *whole record* (content-addressed); no `signedFields` concept |
| Retraction | **No built-in mechanism** — no `retractedAt`; would need app-level revocation (delete proof record / separate revocation record) |
| Replay protection | **Yes** — `repository` DID bound into the signed CID |
| Key publication | DID document verification methods / `did:key` (no dedicated record collection) |
| Hardware anchoring | **None in this crate** — pure software ECDSA (the upstream `bluesky-attestation-tee` *spec* targets TEEs, but this is the generic layer) |
| Records published | none of its own — attaches to any record's `signatures[]` |
| Maintenance | alpha (`0.15.0-alpha.1`), active, by the lexicon.garden author |

## Why there is no `doc/ref/<NSID>.json`

The crate publishes no records and registers no NSID. The `signatures[]` shape is an open convention: the `$type` of each attestation entry is chosen by the caller, so there is no canonical lexicon file to snapshot. These notes stand in as the reference. If a consumer (e.g. tassle) adopts the pattern, *they* define the `$type` (e.g. `com.superbfowle.tass.cosign`) and the `signatures[]` field on their own records.

## See also

- Source: [`crates/atproto-attestation/src/`](https://tangled.org/ngerakines.me/atproto-crates/tree/main/crates/atproto-attestation) (`lib.rs`, `attestation.rs`, `cid.rs`, `signature.rs`, `verification.rs`, `input.rs`)
- docs.rs: <https://docs.rs/atproto-attestation>
- Upstream spec (cited): `bluesky-attestation-tee/documentation/spec/attestation.md`
- Comparison doc: [`../discovery/attestation.md`](../discovery/attestation.md)
