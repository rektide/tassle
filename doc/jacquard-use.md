# Jacquard — applicability to tassle

Source: [`rsform/jacquard`](https://github.com/rsform/jacquard) (v0.12.0, MPL-2.0, by Orual @ Tangled [@nonbinary.computer/jacquard](https://tangled.org/@nonbinary.computer/jacquard))

## What it is

A complete alternative to `atrium-rs` for Rust atproto development. Same coverage (XRPC client, OAuth, identity, repo/MST/CAR, server-side, codegen) with two big differentiators:

1. **Validated string newtypes with borrow-or-share backings** — every type is `Foo<S: BosStr = DefaultStr>` where `S` can be `SmolStr` (owned, inline-optimized), `&str` (zero-copy), `CowStr<'a>`, `String`, or `Cow<'a, str>`. Pick your trade-off per call site.
2. **Codegen in both directions** — lexicon JSON → Rust (like atrium's lexgen) AND Rust → lexicon JSON via `#[derive(LexiconSchema)]`. Schema-as-code or code-as-schema, your choice.

Plus: generated fluent **builders**, runtime lexicon **validation**, KDL-driven multi-source lexicon **fetching** (git, atproto DID records, HTTPS), `atproto!{}` literal macro (like `serde_json::json!`), and `Data<S>`/`RawData<'a>` value types that preserve CBOR fidelity (bytes, CID links, blobs) which `serde_json::Value` can't represent.

## Where it helps tassle

The planned **Rust codegen for CI** path is where jacquard shines. Compared to atrium's lexgen:

| Need | atrium lexgen | jacquard-lexgen |
|------|---------------|-----------------|
| JSON → Rust types | ✓ | ✓ |
| **Fluent builders** (bon-style, what we already have in TS) | ✗ | ✓ ([`codegen/builder_gen/`](https://github.com/rsform/jacquard/blob/main/crates/jacquard-lexicon/src/codegen/builder_gen/)) |
| **Runtime validation** (min/max/maxLength/maxGraphemes/blob MIME) | ✗ | ✓ ([`validation.rs`](https://github.com/rsform/jacquard/blob/main/crates/jacquard-lexicon/src/validation.rs)) — compiled into each generated `.validate()` |
| **Multi-source lexicon fetch** (git + atproto records + HTTPS, KDL config) | ✗ | ✓ ([`lexicons.kdl`](https://github.com/rsform/jacquard/blob/main/lexicons.kdl)) |
| Rust → JSON (reverse codegen) | ✗ | ✓ (`#[derive(LexiconSchema)]`) |
| CBOR / dag-cbor | via `serde_ipld_dagcbor` separately | integrated (`Data::to_dag_cbor()`) |

For our `com.superbfowle.tass.*` lexicons, jacquard produces:
- Typed `Node`, `Tassilize`, `Meditate`, `Enervate`, `Resonance`, `Form` structs
- Fluent `Node::builder().name("...").rating(3).build()` matching our TS API
- `.validate()` methods that enforce every constraint in the lexicon (so `rating` 1-5, `quintessence` 0-100, etc. are checked at compile-time-of-validation, not hand-coded)
- AT-URI constructors (`Node::uri(did, rkey)`)
- XRPC request/response markers (if we add queries/procedures later)

### Specifically useful subsystems

1. **`crates/` workspace codegen** — drop a `lexicons.kdl` next to our `lexicons/*.json`, run `jacquard-codegen`, get a Rust crate with all our types. This becomes the canonical schema check.
2. **Canonical resonance publishing** — the `source "..." type="atproto" { endpoint "did:plc:..." }` KDL syntax lets us fetch canonical resonance/form records straight from any DID's repo. This is the multi-domain authority discovery mechanism we sketched.
3. **CBOR output mode** (deferred) — `Data::to_dag_cbor()` is one call. We get the wire format for free if we add a Rust side.
4. **Strong refs** — tassle's lexicon ideas doc flags that cross-record refs are bare AT-URIs (no CID). Jacquard's `Cid<S>` + `CidLink` types are the canonical upgrade path.

## Where it does NOT help

- **The TypeScript CLI** — jacquard is Rust-only. The current `src/cli/*`, `src/auth/*`, `src/atproto/*` benefit zero directly. Could compile to WASM and call from Bun, but that's heavy.
- **The hedystia web server** — Bun/TS. Same. (`jacquard-axum` exists but only matters if we replaced hedystia with axum, which contradicts the stated direction.)
- **Reading rpg.actor's loose sheet** — their JSON shape doesn't match their own lexicon cleanly (Capitalized keys, `Force` singular). `RawData`/`Data` help, but no library fixes that the upstream is loose.

## Trade-offs

| Pro | Con |
|-----|-----|
| More ergonomic than atrium-rs (less boilerplate) | Single maintainer (Orual, ~97% of 428 commits) — bus-factor risk |
| Builder generation matches our TS bon-style API | Pre-1.0; 0.11→0.12 had "many breaking changes" |
| Runtime validation compiled into types | Not aligned with bluesky-social's reference stack |
| KDL-driven multi-source fetch is exactly our multi-domain story | `chrono` (not `jiff`) — diverges from our preferred Rust conventions |
| Active: 0.13 in flight, last release 2026-06-13 | `jacquard-axum` was briefly out of workspace during 0.12 redesign |
| 646+ lexicons already generated and tested | `jacquard-lexgen` root_module hard-coded to `crate` |

## Recommendation for tassle

**Use jacquard instead of atrium's lexgen** for the Rust codegen-for-CI track. Specifically:

1. **Workspace layout** (`crates/` at project root):
   ```
   crates/
   ├── Cargo.toml (workspace)
   ├── tassle-lexicons/         # generated, .gitignore'd or committed?
   │   └── src/...              # output of jacquard-codegen
   └── tassle-codegen/          # thin binary wrapping jacquard-lexgen
       ├── Cargo.toml           # git dep on rsform/jacquard
       └── src/main.rs          # ~30 lines: read ../lexicons/, write tassle-lexicons/
   ```

2. **A `lexicons.kdl`** at project root:
   ```kdl
   source "tassle" type="path" priority=100 {
       path "../lexicons"
       pattern "**/*.json"
   }
   ```
   Future: add `source "mage-canonical" type="atproto" { endpoint "did:plc:..." }` when canonical Mage resonances get published.

3. **CI step**: `cargo run -p tassle-codegen && git diff --exit-code crates/tassle-lexicons/` — fails if generated types drifted from lexicons.

4. **Do NOT** migrate the TS CLI to Rust. Keep hedystia/gunshi. The Rust crate is purely a type-validation artifact (and a future beachhead if we ever need high-perf ingestion).

5. **For CBOR output mode** (deferred): when we get there, the cleanest path is to shell out to a small Rust binary that uses jacquard's `Data` to encode. Avoids pulling CBOR libs into the TS dep tree.

## Open questions

- Do we depend on jacquard via git (it's not all published on crates.io) or vendor the subset?
- Do we commit generated `crates/tassle-lexicons/` or generate in CI? (Committing gives reviewers visibility but adds churn.)
- Do we ever want the reverse direction (`#[derive(LexiconSchema)]`) — authoring lexicons in Rust and emitting JSON? Probably not for tassle, but worth knowing it's there.
