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

CLI-only, MVP reads + writes work end-to-end against a real atproto PDS. Web server (hedystia) is deferred; see **Deferred** below.

| Capability | Status |
|---|---|
| OAuth loopback login (`tassle login`) | ✅ |
| Read mage sheet from `actor.rpg.stats/self` (case-tolerant) | ✅ |
| Public-agent fallback for `sheet` (no login needed) | ✅ |
| Mint nodes | ✅ |
| Publish tassilize / meditate / enervate records | ✅ |
| Fluent builders for all record types | ✅ |
| Lexicons authored under `com.superbfowle.tass.*` | ✅ |
| Sample file generator (`tassle samples`) | ✅ |
| Hedystia web server (reuses auth core) | ⏳ deferred |
| Ontology restructure (per `pub.layers.ontology`) | ⏳ design discussion |
| CBOR output mode | ⏳ deferred |
| Multi-domain parameterized authority | ⏳ deferred |
| True lexicon-driven sample generator | ⏳ currently hand-coded |

# Install

```bash
pnpm install
```

Requires Node 22+ for native TypeScript type stripping. No build step for development — run `node ./tassle.ts` directly.

# Quick start

```bash
# Display your mage character sheet (works without login — your sheet is public)
node ./tassle.ts sheet

# Log in (opens browser for OAuth loopback)
node ./tassle.ts login jauntywk.bsky.social
node ./tassle.ts whoami

# Mint a Node — a place where quintessence gathers
node ./tassle.ts mint "Crystal Spring" -r 3 -R dynamic -f "a smooth river-stone"
# → ✓ minted Node "Crystal Spring" (rating 3, 15q ambient)
#   at://did:plc:.../com.superbfowle.tass.node/3xyz...

# Crystallize some of that into tass
node ./tassle.ts tassilize at://did:plc:.../com.superbfowle.tass.node/3xyz 5 -f "a silver coin"

# Spend the tass
node ./tassle.ts enervate at://did:plc:.../com.superbfowle.tass.tassilize/<rkey> 2 -p "Lock the door"

# Generate example records into samples/
node ./tassle.ts samples
```

# Commands

| Command | Description |
|---|---|
| `login <handle>` | OAuth loopback login (opens browser) |
| `logout` | Clear stored session |
| `whoami` | Show current authenticated user |
| `sheet` | Read your Mage: The Ascension sheet (`actor.rpg.stats/self`) |
| `mint <name> -r <rating>` | Mint a Node |
| `tassilize <node> <q>` | Crystallize quintessence into tass at a Node |
| `meditate <node> <amount>` | Pull quintessence from a Node's ambiance |
| `enervate <tass> <amount>` | Drain/expend tass |
| `samples` | Regenerate example records into `samples/` |

All commands take `-j/--json` for machine-readable output. Run `<cmd> --help` for full args.

# Architecture

```
tassle/
├── tassle.ts                      # bin entry; main-module check → src/cli/main.ts
├── src/
│   ├── cli/main.ts                # gunshi router (9 commands)
│   ├── auth/                      # SHARED auth core (CLI today, hedystia later)
│   │   ├── client.ts              # NodeOAuthClient factory
│   │   ├── profile.ts             # ClientProfile: loopbackProfile | webProfile
│   │   ├── agent.ts               # login flow + restore → Agent + publicAgent
│   │   ├── scopes.ts              # OAuth scope list
│   │   └── stores/file-store.ts   # file-backed state/session/AuthInfo stores
│   ├── atproto/
│   │   ├── mage-sheet.ts          # case-tolerant reader for actor.rpg.stats/self mage block
│   │   ├── pds.ts                 # listRecords/putRecord wrappers
│   │   └── tass.ts                # fluent builders: node(), tassilize(), meditate(), enervate()
│   ├── commands/                  # one gunshi command per file
│   └── samples/generate.ts        # uses builders to generate example records
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
pnpm check      # concurrently runs typecheck (tsgo) + lint (oxlint)
pnpm fix        # concurrently runs format (oxfmt) + lint --fix (oxlint)
pnpm test       # vitest (no tests yet)
```

Toolchain: gunshi (CLI), `@atproto/api` + `@atproto/oauth-client-node` (atproto), `@atproto/common-web` (TID rkeys), oxfmt/oxlint (format/lint), tsgo (typecheck), tsdown (bundle for npm), vitest (test), concurrently (script composition).

TypeScript with type stripping — `allowImportingTsExtensions`, `noEmit`, run `.ts` files directly via `node ./tassle.ts`. No build step in dev; build artifacts are only for npm package distribution.

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
