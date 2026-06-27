# tassle

> An energy ledger on atproto / rpg.actor, based around Mage: The Ascension Quintessence, Tass, and Nodes.

In Mage: The Ascension, the raw energy of reality is **Quintessence** — that which flows through the patterns of reality. Awakened beings (Mages) can affect change in the universe, but to make it permanent, they must imbue the patterns with quintessence.

# Lexipedia

**Quintessence** also crystallizes (and can be crystallized) into **Tass** — a manifest version of quintessence, taking the coincidental form of a significant imbued object in reality.

**Nodes** are places in the world where quintessence naturally gathers and springs forth, in the form of raw quintessence and often tass too.

Awakened beings carry quintessence in their own pattern (capped at their **Avatar** rating).

**Prime** is the sphere of magic concerned with working quintessence energies.

## Actions

Records published to act on the energy ledger. Each is its own collection under `com.superbfowle.tass.*`:

- **`tassilize`** — formation of tass at a node; genesis record of a tass object and its energy endowment
- **`meditate`** — pull quintessence from a node's ambiance into the mage's pattern
- **`enervate`** — a registered drain or expenditure of tass ("draw the sinew out"; tap the tass and withdraw its current)

## Status

Rust/Jacquard is now the primary CLI path. The older TypeScript CLI remains in-tree as a working reference for OAuth and mutation flows while those features move to Rust.

| Capability | Status |
|---|---|
| Rust clap CLI (`crates/tassle-cli`) | ✅ primary |
| Rust Jacquard public XRPC read (`repo list`) | ✅ |
| Rust generated builders for tassle lexicons | ✅ |
| Rust sample generator (`samples`) | ✅ |
| TypeScript OAuth loopback login and writes | ✅ legacy/reference |
| Read mage sheet from `actor.rpg.stats/self` in TS | ✅ legacy/reference |
| Lexicons authored under `com.superbfowle.tass.*` | ✅ |
| Rust `mage list` / `mage stats` | ✅ |
| Rust OAuth/write commands | ⏳ next |
| Hedystia web server (reuses auth core) | ⏳ deferred |
| Ontology restructure (per `pub.layers.ontology`) | ⏳ design discussion |
| CBOR output mode | ⏳ deferred |
| Multi-domain parameterized authority | ⏳ deferred |
| True lexicon-driven sample generator | ⏳ currently hand-coded |

# Install

```bash
cd crates
cargo build -p tassle-cli
```

The active CLI is the Rust binary in `crates/tassle-cli`. Run it from the Rust workspace with `cargo run -p tassle-cli -- ...`.

The legacy TypeScript CLI still requires `pnpm install` and Node 22+ if you need to compare behavior while porting OAuth/write paths.

# Quick start

```bash
cd crates

# Public-read a live rpg.actor collection through Jacquard XRPC
cargo run -p tassle-cli -- repo list actor.rpg.stats --repo jauntywk.bsky.social

# Read normalized Mage sheet stats from the active profile
cargo run -p tassle-cli -- mage list

# List every actor.rpg.stats rkey for the active profile
cargo run -p tassle-cli -- mage list --all

# Generate a Node record as validated JSON
cargo run -p tassle-cli -- generate node "Crystal Spring" -r 3 -R dynamic -t "a smooth river-stone"

# Generate example records into samples/ from Rust builders
cargo run -p tassle-cli -- samples
```

# Commands

| Rust command | Description |
|---|---|
| `auth login <did-or-handle>` | Save a local profile/default actor; OAuth tokens come later |
| `auth set <key>` / `auth set <key=value>` | Read or write a dotted key in the active profile TOML fragment |
| `mage list [rkey]` | Read `actor.rpg.stats/<rkey>`; defaults to normalized Mage stats from `actor.rpg.stats/mage`, fallback to `self.mage` |
| `mage stats` | Alias for `mage list` |
| `self stats` / `self list` | Inspect `actor.rpg.stats/self` aggregate contents |
| `repo list <collection> --repo <did-or-handle>` | Public-list records from an actor's PDS using Jacquard XRPC |
| `generate node <name> -r <rating>` | Generate and validate a Node record as JSON or DAG-CBOR |
| `samples` | Regenerate example records into `samples/` from Rust builders |

Run `<cmd> --help` for full args. The older TypeScript commands (`login`, `sheet`, `mint`, `tassilize`, `meditate`, `enervate`) are the behavior reference while Rust parity lands.

Rust profile defaults are stored as TOML fragments under `${XDG_CONFIG_HOME:-~/.config}/tassle/config.toml.d/`. `auth login` currently resolves and stores the profile DID/PDS only; it does not perform OAuth yet.

# Architecture

```
tassle/
├── crates/
│   ├── Cargo.toml                 # Rust workspace
│   ├── tassle-cli/                # primary clap/Jacquard CLI
│   ├── tassle-lexicons/           # generated Rust lexicon types/builders
│   ├── tassle-codegen/            # Jacquard codegen wrapper
│   └── tassle-validate/           # schema validation helper
├── src/                           # legacy TypeScript CLI/reference implementation
├── lexicons/                      # canonical lexicon JSON (data-driven schema declarations)
│   ├── com.superbfowle.tass.node.json
│   ├── com.superbfowle.tass.tassilize.json
│   ├── com.superbfowle.tass.meditate.json
│   ├── com.superbfowle.tass.enervate.json
│   ├── com.superbfowle.tass.resonance.json
│   └── com.superbfowle.tass.form.json
└── samples/                       # generated example records (regenerable via `tassle samples`)
```

## Auth lifecycle (shared between CLI and future web server)

The OAuth loopback flow has 11 atomic steps; the **shared core** (`src/auth/`) covers all but 3. The CLI-specific bits (loopback server, file store) and future web-specific bits (public client_id, hedystia/db store) are thin transport layers on top.

```mermaid
flowchart TD
    subgraph shared["SHARED CORE (src/auth/)"
        S1[1. resolve handle to DID]
        S2[2. fetch DID doc → PDS + auth server]
        S3[3. generate DPoP keypair]
        S5[5. build authorize URL: PKCE + DPoP + scope]
        S7[7. exchange code for tokens]
        S8[8. persist session]
        S9[9. restore session by DID]
        S10[10. refresh expired token]
        S11[11. make XRPC call via Agent]
    end
    subgraph cli["CLI-specific"
        C4[4a. loopback client_id 127.0.0.1:PORT]
        C6[6a. http.createServer receives code]
        C8[(~/.config/tassle/ files)]
    end
    subgraph web["WEB-specific (later)"
        W4[4b. public client_id at .well-known]
        W6[6b. hedystia /callback route]
        W8[(hedystia/db session table)]
    end
    S1 --> S2 --> S3
    C4 -.-> S5
    W4 -.-> S5
    S5 --> C6 & W6 --> S7 --> S8
    C8 -.store.-> S8
    W8 -.store.-> S8
    S8 --> S9 --> S10 --> S11
```

**Key persistence detail**: the loopback port is saved in `~/.config/tassle/session.json` as `oauthPort` so the same `client_id` is reconstructed on restore. Without this, token refresh silently fails after ~1 hour when the access token expires.

## Persistence

Pure files, no framework, no database:

```
~/.config/tassle/
├── session.json        # which user is logged in (did, handle, pds, oauthPort)
├── auth/               # OAuth session tokens + DPoP key, keyed by DID base64url
└── state/              # transient CSRF/PKCE state during login handshake
```

The loopback callback server is just `node:http`'s `createServer` — ~30 lines that live only for the ~10 seconds of the OAuth dance. OAuth machinery itself is `@atproto/oauth-client-node` (the only framework piece); we provide file-backed `stateStore` + `sessionStore` impls.

## Fluent builders

Each record type has a bon-style builder:

```typescript
import { node, tassilize } from "./src/atproto/tass.ts";

const n = node()
  .name("Crystal Spring")
  .rating(3)                              // validates 1-5
  .resonance("dynamic")
  .tassForm("a smooth river-stone")
  .build();                               // createdAt filled, ambientQuintessence defaults to rating*5

const t = tassilize()
  .node(n.uri)
  .quintessence(5)
  .form("a silver coin")
  .build();
```

Required setters take strict types and `build()` throws if missing. Optional setters accept `T | undefined` and skip when undefined — that's the tweak for CLI ergonomics so commands can pass `ctx.values.foo` straight through without conditionals. Numeric setters validate ranges.

# Lexicons

Six collections authored under `com.superbfowle.tass.*`:

| NSID | Kind | Purpose |
|---|---|---|
| `com.superbfowle.tass.node` | record | A Node — place where quintessence gathers. Rated 1-5, ambient pool defaults to `rating * 5`. |
| `com.superbfowle.tass.tassilize` | record | Genesis record of tass forming at a Node. |
| `com.superbfowle.tass.meditate` | record | Pull quintessence from a Node's ambiance. |
| `com.superbfowle.tass.enervate` | record | Drain/expend tass. |
| `com.superbfowle.tass.resonance` | record + defs | Canonical resonance type registry. Reusable `#resonanceValue` and `#resonanceProfile` defs entities embed. |
| `com.superbfowle.tass.form` | record | Named Tass form (physical shape) with `materializeCost` and `totalCapacity`. |

Records are currently written with `validate: false` on the PDS — lexicons aren't yet registered there via `com.atproto.lexicon` records. Schema is enforced client-side via the builders.

# Samples

`samples/` contains example records generated from the builders via `tassle samples`. Fixed `createdAt` for stable diffs; placeholder DID (`did:plc:samplesamplesamplesample`) since the canonical publisher isn't decided. Four examples cover the full lifecycle: node → tassilize → enervate, plus a meditate.

# Development

```bash
cd crates
cargo check -p tassle-cli
cargo run -p tassle-cli -- repo list actor.rpg.stats --repo jauntywk.bsky.social

# Legacy TypeScript reference path:
pnpm check      # concurrently runs typecheck (tsgo) + lint (oxlint)
pnpm fix        # concurrently runs format (oxfmt) + lint --fix (oxlint)
pnpm test       # vitest (no tests yet)
```

Rust toolchain: clap v4 derive, Jacquard for atproto/XRPC, generated `tassle-lexicons` builders, `miette` for diagnostics, `tokio` for async runtime.

Legacy TypeScript toolchain: gunshi, `@atproto/api`, `@atproto/oauth-client-node`, oxfmt/oxlint, tsgo, tsdown, vitest, concurrently.

# Inspirations

- [`disnet/skyboard`](https://github.com/disnet/skyboard) — ported the OAuth loopback pattern and the file-backed session store shape; `ClientProfile` abstraction generalizes skyboard's hardcoded loopback into a reusable shim for future web use.
- [`pub.layers.ontology`](https://docs.layers.pub/lexicons/ontology) — design reference for the upcoming ontology restructure (small per-resonance ontologies, not one big vocabulary).
- co/core workqueue and tokens

# Deferred

- **Hedystia web server** — `webProfile()` is already stubbed in `src/auth/profile.ts`; will swap in a hedystia/db-backed session store impl and a `/callback` route. Auth core stays shared.
- **Ontology restructure** — move resonance to its own authority root (not `com.superbfowle.tass.*`), adopt `pub.layers.ontology` pattern: small per-resonance ontologies (Dynamic, Static, Primordial) instead of one flat vocabulary. Parameterized authority since domains aren't fixed yet (`tass.superbfowle.com`, `resonance.superbfowle.com`, `tassleat.example`, etc.).
- **CBOR output** — `--output screen|cbor|publish` global flag for canonical publishing workflow. Needs `@atproto/lex-cbor` (or equivalent dag-cbor encoder).
- **Multi-domain parameterized authority** — env/flag for NSID prefix; placeholder authority tokens (`_tass`, `_resonance`) that resolve later when we have real domains.
- **True lexicon-driven sample generator** — read `lexicons/*.json`, synthesize records from the schema. Currently the generator is hand-coded against builders.
- **Mage canonical resonance records** — Dynamic / Static / Primordial / Wyld / Weaver / Wyrm triad. Defined as ontology records once the restructure lands; published by your PDS so other Mage users reference your at-uris.
- **Wire `#resonanceProfile` into Node/Tassilize** — entities don't yet embed the resonance profile defs; deferred pending the ontology restructure.
