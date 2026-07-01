# Web login — `tassle-web-auth` + `tassle-web`

> **Status:** early design, exploring. Goal: a small Axum web server whose one
> job (for now) is to let people **log in with their atproto account** so they
> can use webapps. Two crates: **`tassle-web-auth`** (reusable OAuth-login
> plumbing) consumed by **`tassle-web`** (the app). Tenancy: **one app, many
> users** — a single `client_id` / keyset, N end-user sessions.
>
> **Supersedes** the web/OAuth parts of
> [`auth-design.md`](auth-design.md): the `tassle-app` composition-root crate is
> abandoned. Auth-web wiring lives in its own crate (`tassle-web-auth`), which is
> the "grows a personality → split out" case that doc's §2 anticipated.
> Related: [`jacquard-use.md`](jacquard-use.md).

## The one load-bearing fact: we are not writing OAuth

Jacquard already ships the entire atproto OAuth 2.1 flow, and an Axum adapter on
top of it. Concretely, in `~/archive/nonbinary.computer/jacquard`:

- **`jacquard-oauth`** — the OAuth client itself: DPoP, PKCE, PAR, identity
  resolution, token refresh, scopes. The entry points we care about:
  - `OAuthClient::start_auth(identifier, options) -> authorize URL`
  - `OAuthClient::callback(params) -> OAuthSession`
  - plus `restore(did, session_id)`, `revoke(...)`, and the
    `ClientAuthStore` / `SessionStore` **traits** it persists through.
- **`jacquard-axum::oauth`** — a ready-made Axum layer over that client:
  - Extractors: `ExtractOAuthSession` (strict/API), `BrowserOAuthSession`
    (redirects to login), and `…Optional…` variants.
  - `routes(&OAuthWebConfig) -> Router` mounting, out of the box:
    `/oauth-client-metadata.json`, `start_auth` (GET+POST),
    `callback`, `logout`.
  - Private-cookie session handling (stores only an encoded `SessionKey`,
    never tokens), `return_to` round-tripping, `OAuthWebConfig` for paths.

And the durable store those want is **`jac-store-fjall`** (`~/src/jac-store-fjall`)
— the pure-Rust `ClientAuthStore` / `SessionStore` backend. (Jacquard itself only
ships in-memory and a "not secure, development only" `FileAuthStore`.) tassle
already uses it as the session store per [`auth-design.md`](auth-design.md) §4.

So the stack already exists end to end:

```
   tassle-web  (the app: pages, "protected" routes, session UI)
        │  consumes
   tassle-web-auth  (thin: build the OAuthClient, config, login page, wiring)
        │  ├── jacquard-axum::oauth   ── extractors + /oauth/* routes + cookies
        │  └── jacquard-oauth         ── OAuthClient (start_auth / callback / restore)
        │           │ persists through
        │      ClientAuthStore + SessionStore  (traits)
        │           │ impl
        └────► jac-store-fjall        ── durable fjall/turso/canopy backend
```

**`tassle-web-auth`'s actual job is wiring, not protocol.** It:

1. constructs one `OAuthClient` from (a) a `jac-store-fjall` store and (b) our
   client metadata + keyset;
2. holds the cookie-encryption `Key` and an `OAuthWebConfig`;
3. exposes app state that satisfies `jacquard-axum`'s
   `OAuthWebState` / `FromRef` bounds;
4. serves a **static-ish login page** (the one bit of real UI);
5. re-exports the extractors so `tassle-web` route handlers just ask for
   `BrowserOAuthSession(session)`.

The risk in this project is *not* "can we implement OAuth" — it's picking the
handful of config knobs (client type, scopes, keyset lifecycle, deployment URL)
correctly. The rest of this doc is those knobs.

## Client type: confidential (recommended)

atproto OAuth clients are either **public** (no server-side signing key) or
**confidential** (a web service holds an ES256 client-auth key, advertised via
JWKS). Since `tassle-web` *is* a web service, it should be a **confidential
client**:

- **longer session / token lifetimes** — the reason to bother;
- **incident revocation** — drop a key from the published JWKS to invalidate
  sessions bound to it;
- the client-auth key is one key for the whole deployment (common to all user
  sessions), rotatable — *distinct from* per-session DPoP keys, which Jacquard
  already manages per session inside the store.

Mechanically (per the atproto spec, `~/archive/bluesky-social/atproto-website`,
`specs/oauth` → "Types of Clients", "Client ID Metadata Document"):

- `client_id` = the **public HTTPS URL** of our metadata doc, e.g.
  `https://tassle.example/oauth-client-metadata.json` (must be `https:`, no port;
  path must exactly match the URL it's served from).
- `token_endpoint_auth_method: private_key_jwt`, `token_endpoint_auth_signing_alg: ES256`.
- `jwks` (inline) **or** `jwks_uri` — Jacquard's `atproto_client_metadata(config, keyset)`
  builds this for us; `jacquard-axum`'s `client_metadata_handler` already serves it.
- `dpop_bound_access_tokens: true`, `response_types: [code]`,
  `grant_types: [authorization_code, refresh_token]`.

**Localhost dev exception:** for local development atproto allows a
`http://localhost` `client_id` (no public URL needed), which downgrades to a
*public* client. Jacquard exposes this as
`AtprotoClientMetadata::default_localhost()`. So the plan is:
**localhost/public in dev, HTTPS/confidential in prod**, selected by config
(and this dovetails with tassle's figment-profile config from
[`auth-design.md`](auth-design.md) §3).

### Keyset lifecycle (the one genuinely new thing to own)

Everything else is Jacquard's; the client-auth **keyset** is ours to manage:

- generate an ES256 keypair once; persist it (candidate: a dedicated
  keyspace/table in the same `jac-store-fjall` store, or a mounted secret file);
- publish the **public** half via JWKS in the client metadata;
- support **>1 key** so rotation is add-new → publish-both → drain-old → remove;
- **never** let the private key touch a cookie or a log.

Open question: do we store the keyset in the auth store, or keep it as
deployment config (env/secret file)? Leaning secret-file/env for prod (ops
rotate it out of band) with an in-store option for single-binary dev. → track as
a ticket.

## Scopes: ask for the minimum, start transitional

Scope reference: `jacquard-oauth/src/scopes.rs` (typed `Scope` / `Scopes` /
`ScopesBuilder`) and the spec's "Authorization Scopes" section.

Rules that matter for us:

- `atproto` is **required** in every request — it's the marker that says "atproto
  profile"; without it you get no PDS access and the server returns the account
  DID in `sub`.
- **Login-only** (which is our stated purpose today) needs *only* `atproto`. A
  pure "Login with atproto" client requests just `atproto`, still runs the full
  flow, and verifies the `sub` DID. If all `tassle-web` does for now is
  authenticate people, **`atproto` alone is the honest scope.**
- The moment a webapp wants to read/write the user's repo, add
  **`transition:generic`** (App-Password-equivalent: read/write any record,
  blobs, prefs, service proxying — but *no* account-management, *no* DMs).
  `transition:email` and `transition:chat.bsky` are additive opt-ins.
- Transitional scopes are explicitly **temporary**; the granular
  [Permissions spec](https://atproto.com/specs/permission) (`repo:`, `rpc:`,
  `blob:`, `include:` permission-sets) is where this is heading. Design so the
  requested `Scopes` is **config**, not hardcoded, so we can tighten later.

Recommended default: **`atproto`** now (login is the only feature);
**`atproto transition:generic`** the moment the first real webapp needs repo
access. All scopes a client *might ever* request must be declared in the client
metadata `scope` field, so declare the superset there and request the subset per
flow.

## The login flow (what actually happens)

Matches `jacquard-axum::oauth` route names; `tassle-web` mounts
`oauth::routes(&config)` and adds a login page.

```
1. GET /login                → static-ish HTML: one form, "handle or DID" input
                                (+ hidden return_to). This is the ONLY real UI.
2. POST /oauth/start         → start_auth_form: OAuthClient::start_auth(...)
   (identifier, return_to)     → 302 to the user's Authorization Server (their PDS/entryway)
                                 return_to stashed in a short-lived private cookie keyed by state
3.  … user authenticates on their own PDS …
4. GET /oauth/callback       → callback_handler: OAuthClient::callback(params)
   (code, state, iss)          → verifies sub DID ↔ issuer, persists session in jac-store-fjall
                                 sets private session cookie = encoded SessionKey (NOT tokens)
                                 302 back to return_to (or after_callback_redirect)
5. GET /whatever (protected) → BrowserOAuthSession extractor restores the session
                                from the cookie via the store; on miss → 302 /login
6. POST /oauth/logout        → logout_handler: session.logout(), clears cookie
```

Two things to note:

- The **cookie holds only a `SessionKey`** (`{did, session_id}`, encrypted with
  our `Key`). Tokens + DPoP key live in `jac-store-fjall`. The durable store is
  exactly what makes browser sessions safe.
- Identity input accepts **handle, DID, or PDS/entryway hostname** (the spec
  supports starting from a server when a user forgot their handle). The login
  page's single field should say so.

## The static-ish login page

The instinct ("static-ish HTML, login is the whole point") is right and the spec
agrees — an atproto client renders *no* consent UI itself; all authentication
and authorization happens on the user's **own** Authorization Server. Our page is
just:

- one text input (handle / DID / PDS host), one submit button → `POST /oauth/start`;
- optional error banner (e.g. "couldn't resolve that handle");
- a hidden `return_to`.

Implementation options, cheapest first:

1. **A single hand-written `.html` string / `askama` template**, served by one
   `GET /login` handler. No framework. Matches "static-ish."
2. Static file + `tower-http::ServeDir` if we want to edit HTML without
   recompiling.

Recommendation: **(1) one template now.** "Eventually a real web framework" is a
later, separate decision — don't pull one in for a single form.

## Crate boundary — what goes where

| | `tassle-web-auth` (library) | `tassle-web` (binary/app) |
|---|---|---|
| OAuthClient construction (store + metadata + keyset) | ✅ owns | consumes |
| `OAuthWebConfig`, cookie `Key`, app-state `FromRef` glue | ✅ owns | supplies secrets/config |
| Re-export `jacquard-axum` extractors + `oauth::routes` | ✅ | uses |
| Login page template + `GET /login` | ✅ provides default | may override |
| Keyset generation / rotation / persistence | ✅ owns | provides key material |
| App pages, protected routes, business logic | — | ✅ owns |

`tassle-web-auth` depends on: `jacquard-axum` (oauth feature), `jacquard-oauth`,
`jac-store-fjall`, `axum` + `axum-extra` (PrivateCookieJar). `tassle-web` depends
on `tassle-web-auth` + `axum`, and picks a `jac-store-fjall` backend feature
(`backend-fjall` by default; `backend-turso` if it wants multi-process/SQL).

> **jacquard version alignment** — same trap [`auth-design.md`](auth-design.md)
> §2.1 flags: tassle pulls `jacquard` from git; `jac-store-fjall` pulls
> `jacquard = "0.12"` from crates.io. `tassle-web-auth` sits on the boundary
> between them, so the trait impls only type-check if both resolve to one
> jacquard. Pin them together before wiring.

## Open questions to resolve before coding

1. **Keyset storage** — in the auth store vs. secret file/env. (leaning: env/file
   for prod, in-store for dev)
2. **Session `Key` (cookie encryption) source** — env secret; rotation story?
3. **Which `jac-store-fjall` backend** for `tassle-web` (fjall single-process is
   fine for one deployment; turso if multi-process).
4. **Scopes default** — ship `atproto`-only until a webapp needs repo writes,
   then `transition:generic`? (recommended yes)
5. **Deployment / `client_id` URL** — needs a real public HTTPS origin for
   confidential mode; confirm the domain so metadata `client_id` is stable.
6. **jacquard source/version pin** — resolve §2.1 (git vs crates.io) before the
   crate boundary can compile.

## References

- Jacquard: `~/archive/nonbinary.computer/jacquard` —
  `crates/jacquard-axum/src/oauth.rs` (the web layer we build on),
  `crates/jacquard-oauth/src/{client,scopes,atproto}.rs`,
  `crates/jacquard-axum/tests/oauth_web_tests.rs` (worked example).
- atproto OAuth spec: `~/archive/bluesky-social/atproto-website`,
  `src/app/[locale]/specs/oauth/en.mdx`; guides under `guides/oauth-*`,
  `guides/about-oauth`, and the interactive scope builder.
- Reference starter (SvelteKit, non-authoritative):
  <https://tangled.org/charlebois.info/sveltekit-atproto-starter>.
- tassle: [`auth-design.md`](auth-design.md) (app-password MVP; `tassle-app` now
  dead), [`jacquard-use.md`](jacquard-use.md).
</content>
