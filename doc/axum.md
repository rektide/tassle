# Web login — how the OAuth flow works & where our code fits

> Companion to [`oauth.md`](oauth.md) (the settled config/decisions doc). Where
> that doc says *what* the knobs are, this one says *how the flow actually
> runs* — traced through jacquard's code — and draws the line between **theirs**
> and **ours**. Implements ticket `tass-web-auth-crate`.

## The one load-bearing fact

**We do not implement OAuth.** Jacquard ships the entire atproto OAuth 2.1
profile — DPoP, PKCE, PAR, `private_key_jwt` client authentication, the
`sub`↔`issuer` identity check, token refresh, revocation — and
`jacquard-axum::oauth` ships the Axum adapter on top (routes, extractors,
private-cookie session handling). `tass-web-auth` is **wiring**: build one
`OAuthClient` from our config + key material + the turso store, expose it through
an `AppState` that satisfies jacquard-axum's bounds, and render a login page.
That's it.

The risk in this project was never "can we do OAuth." It is picking the keyset
lifecycle, the client-metadata fields, and the handful of `OAuthWebConfig` knobs
correctly. Those are below.

## The flow, end to end (and where each step lives)

The atproto OAuth flow is specified in
[`atproto-website/.../specs/oauth/en.mdx`](https://atproto.com/specs/oauth)
("Summary of Authorization Flow"). Here is that flow as jacquard runs it, with
the two things that are *ours* called out. Route names are the ones
`jacquard-axum::oauth::routes()` mounts (`oauth.rs:422`).

### 1. `POST /oauth/start` — begin authorization (`start_auth_form`)

The login page form posts `identifier` (handle/DID/PDS) + `return_to` here.
`start_auth_form` (`oauth.rs:486`) hands off to
`OAuthClient::start_auth(identifier, options)` (`client.rs:208`), which:

1. **Resolves identity** — `resolve_oauth(input)` (`resolver.rs`) walks handle→DID
   →DID document→PDS→authorization-server metadata, exactly the "Identity
   Authentication" section of the spec. The resolver is `PublicResolver`
   (= `JacquardResolver<reqwest::Client>`, `jacquard-identity/src/lib.rs:1213`) —
   real DNS/HTTP, nothing for us to do.
2. **Runs PAR** — `par()` (`request.rs:558`) generates the PKCE pair
   (`generate_pkce`), **generates a fresh per-session DPoP key**
   (`generate_dpop_key`, `request.rs:571`), and POSTs the authorization-request
   parameters to the server's PAR endpoint. That POST — like every token/PAR
   request — is **DPoP-wrapped**: `client.dpop_server_call(data_source).send(req)`
   (`request.rs:822`) signs a one-time DPoP proof JWT and threads the server's
   `DPoP-Nonce` header back into `dpop_data`.
3. **Authenticates the client (confidential only)** — `build_auth()`
   (`request.rs:902`) signs a `client_assertion` JWT
   (`urn:ietf:params:oauth:client-assertion-type:jwt-bearer`, RFC 7523) using
   **our client-auth keyset** (`keyset.create_jwt(...)`). This is the
   `private_key_jwt` method. The JWT carries `iss=sub=client_id`, `aud=issuer`,
   `iat`, and a random `jti` — exactly the spec's "Confidential Client
   Authentication" requirements. (Public/loopback clients skip this: method
   `none`, no assertion.)
4. **Persists the in-flight request** — the resulting `AuthRequestData`
   (the `state` token, PKCE verifier, the fresh DPoP key, the `request_uri`)
   is saved to the store (`save_auth_req_info`), keyed by `state`.
5. **Returns the authorize URL** — `{authorization_endpoint}?client_id=…&request_uri=…`.

`start_auth_form` then stashes `return_to` in a short-lived private cookie keyed
by `state` (`oauth.rs:640`) and 302s the browser to that authorize URL.

**Ours here:** nothing in the flow. We supplied the `OAuthClient` (with our
keyset + metadata); jacquard does the rest.

### 2. The user authenticates on their own PDS / entryway

The Authorization Server (the user's PDS, or an entryway) authenticates the user,
shows its own consent UI, and redirects the browser back to our
`redirect_uri` (`= public_url + callback_path`) with `?code=…&state=…&iss=…`.
We render no consent UI — the spec puts all of that on the user's own server.

### 3. `GET /oauth/callback` — exchange the code (`callback_handler`)

`callback_handler` (`oauth.rs:501`) calls `OAuthClient::callback(params)`
(`client.rs:273` → `callback_core` `:295`), which:

1. **Looks up the in-flight request by `state`** — `get_auth_req_info`, then
   deletes it (single-use).
2. **Re-fetches authserver metadata** and **verifies `iss`** matches the metadata
   `issuer` (`client.rs:319`). This is the spec's "`iss` query parameter" check
   (`authorization_response_iss_parameter_supported: true` is mandatory).
3. **Exchanges the code** — `exchange_code()` (`request.rs:700`) POSTs to the
   token endpoint with the `code` and the PKCE `code_verifier`, again
   DPoP-wrapped and (confidential) client-assertion-signed.
4. **Verifies the identity** — `verify_issuer(&metadata, &sub)`
   (`request.rs:733`) confirms the `sub` DID actually resolves back to this
   authorization server. This is the load-bearing atproto authentication step
   (spec: "it is critical … to verify that the account identified by the `sub`
   field is consistent with the Authorization Server `issuer`"). Jacquard does it
   for us.
5. **Builds and persists the session** — `ClientSessionData` (tokens + the DPoP
   key + endpoints) is stored via `create_session`.

`callback_handler` then encodes the `SessionKey` (`{did, session_id}`) into a
private cookie (`set_session_cookie`, `oauth.rs:593`) and 302s to `return_to`
(or `after_callback_redirect`). **The cookie holds only the `SessionKey`, never
the tokens** — tokens + DPoP key live in the turso store.

**Ours here:** still nothing in the flow. We supplied the store that holds the
session.

### 4. Protected routes — `BrowserOAuthSession` extractor

A handler asks for `BrowserOAuthSession(session)` (imported from
`jacquard_axum::oauth`, `oauth.rs:251`). The extractor reads the cookie, decodes
the `SessionKey`, calls `OAuthClient::restore(did, session_id)` (`client.rs:390`)
to rebuild the `OAuthSession` from the store. On a missing/expired session it
302s to the login page (with `return_to` preserved); on a valid one the handler
gets a live `OAuthSession` it can make DPoP-bound XRPC calls through.

Token refresh is automatic: `OAuthSession::send_with_opts` (`client.rs:936`)
detects a `401 invalid_token`, calls `refresh()` (`client.rs:870`) — another
DPoP-wrapped, client-assertion-signed token-endpoint POST (`request.rs:634`) —
and retries, with a per-`(did, session_id)` mutex serializing concurrent
refreshes (spec: "refresh tokens are generally single-use … clients may need
locking primitives").

**Ours here:** the handler. The session, the refresh, the DPoP proof on every
request — all jacquard.

### 5. `POST /oauth/logout`

`logout_handler` (`oauth.rs:528`) restores the session, calls
`OAuthSession::logout()` (`client.rs:819`): best-effort `revoke()`
(`request.rs:761`, DPoP-wrapped) at the server's revocation endpoint, then
deletes the session from the store. The session cookie is cleared.

### 6. `GET /oauth-client-metadata.json` — publish our metadata

`client_metadata_handler` (`oauth.rs:454`) re-derives the client metadata from
our `ClientData` on every request via `atproto_client_metadata(&config, &keyset)`
(`atproto.rs:339`) and serves it as JSON. Authorization servers fetch this URL
(the `client_id`) during PAR to validate us. **Our `ClientData` (which we build
at startup) is the single source for this.**

## The two keys (the thing everyone gets confused)

There are **two completely distinct signing keys** in this flow. Conflating them
is the classic mistake. Spec: "Types of Clients" + "DPoP".

| | Client-auth **keyset** | Per-session **DPoP key** |
|---|---|---|
| **What it signs** | the `client_assertion` JWT (`private_key_jwt`) | a DPoP proof JWT on every token + resource request |
| **Scope** | one key (rotatable set) for the whole deployment | one fresh key per authorization flow |
| **Who manages it** | **us** — generate / persist / rotate / publish | **jacquard** — `generate_dpop_key` in `par()`, stored in `ClientSessionData.dpop_data` |
| **Where it lives** | `<state>/keyset/*.json` (secret files) | the turso store (inside the session blob) |
| **Published?** | public half → client metadata `jwks` (`public_jwks()`) | never — stays server-side, proves possession |
| **Rotation** | prepend a key file, restart, drain, remove | implicit: each new login mints a new one |

The keyset is the *one genuinely new thing we own*. The DPoP key is jacquard's,
fully automatic. **The private keyset `d` parameter never touches a cookie, a
log, or the store** — only the in-memory `Keyset`; only its public half
(`public_jwks()` strips `d`, `keyset.rs:95`) is published.

## What's ours vs jacquard's

| Concern | Ours (`tass-web-auth` + `tass-web`) | Jacquard (`jacquard-oauth` + `jacquard-axum`) |
|---|---|---|
| PAR, PKCE, DPoP-key generation | — | `request::par` |
| DPoP-wrapped HTTP (token, refresh, revoke, resource) | — | `dpop_server_call`, `OAuthSession::send_with_opts` |
| `client_assertion` JWT (`private_key_jwt`) signing | — | `request::build_auth` (uses **our** keyset) |
| `sub`↔`issuer` identity verification | — | `exchange_code` → `verify_issuer` |
| Token refresh + concurrent-refresh lock | — | `OAuthSession::refresh` |
| Session persistence (`ClientSessionData`) | supply the **store** | `SessionRegistry` ↔ `ClientAuthStore` |
| Routes: start/callback/logout/metadata | mount them | `oauth::routes()` |
| Extractors (strict + browser, + optional variants) | use them | `ExtractOAuthSession`, `BrowserOAuthSession`, … |
| Private session cookie (encoded `SessionKey`) | supply the `Key` | `set_session_cookie` etc. |
| Client-auth **keyset** lifecycle | **generate / load / rotate / persist** | consumes it via `ClientData` |
| **Client metadata** (`AtprotoClientMetadata`) | **build from `ServiceConfig`** | `atproto_client_metadata` + `client_metadata_handler` |
| Cookie signing **Key** | generate / load | consumes via `FromRef<Key>` |
| Login page (the one UI) | **render it** | redirects to it (`login_page_path`) |
| `AppState` wiring (`OAuthWebState` + `FromRef`) | **the three impls** | the bounds |

## No re-exports — use jacquard directly

`tass-web-auth` **does not** re-export `jacquard` or `jacquard-axum` types. Code
that needs them imports them from their real home, so it is always obvious which
crate a thing belongs to:

```rust
// in tass-web route handlers — import from jacquard directly:
use jacquard_axum::oauth::BrowserOAuthSession;
use jacquard::oauth::client::OAuthSession;
```

`tass-web-auth`'s public surface is only what is genuinely ours: the `AppState`
wiring struct, the keyset/cookie-key/metadata builders, the login-page handler,
and the `boot` constructor. This keeps the boundary legible: if you're naming a
`jacquard::*` or `jacquard_axum::*` path, you're using their code; if you're
naming a `tass_web_auth::*` path, it's ours.

## `OAuthWebConfig` — every knob, and what we do with it

`jacquard_axum::oauth::OAuthWebConfig` (`oauth.rs:80`) owns the route paths and
cookie names. Defaults are sensible for a generic jacquard app, but **one must
be overridden** for tassle. Constructed once at boot, held in `AppState`,
exposed via `FromRef`.

| field | default | tassle | why |
|---|---|---|---|
| `cookie_name` | `jacquard_oauth_session` | leave, or rename `tass_oauth_session` | cosmetic; the private cookie holding the encoded `SessionKey` |
| `return_cookie_prefix` | `jacquard_oauth_return_` | leave | prefix for the state-keyed `return_to` cookies |
| `start_auth_path` | `/oauth/start` | leave | where the login form POSTs |
| **`login_page_path`** | **`/oauth/login`** | **set to `/login`** | **must match where we mount the login handler** — `BrowserOAuthSession` redirects here on a missing session (`oauth.rs:707`); a mismatch sends users to a 404 |
| `callback_path` | `/oauth/callback` | leave | `redirect_uris = public_url + this` |
| `logout_path` | `/oauth/logout` | leave | |
| `after_callback_redirect` | `/` | leave (or a dashboard later) | post-login landing when no `return_to` |
| `after_logout_redirect` | `Some("/")` | leave | `None` ⇒ 204 instead of redirect |
| `session_header` | `x-jacquard-session` | leave | fallback for API/headless (non-cookie) clients |

The one action item: **`login_page_path = Some("/login".into())`**, because
[`oauth.md`](oauth.md) mounts the login page at `/login`, not the jacquard
default `/oauth/login`. Everything else can stay default initially.

`OAuthWebConfig` is jacquard's type; we construct it in `tass-web`'s `main` and
do not wrap it.

## Client metadata — confidential vs loopback

`atproto_client_metadata(&metadata, &keyset)` (`atproto.rs:339`) converts our
`AtprotoClientMetadata` into the wire format, deriving the auth method and
`application_type` from `public_url` + keyset presence — **emergent, never a
knob** (per [`oauth.md`](oauth.md) "DPoP is not a knob"):

- **Confidential / hosted** — `public_url = https://telluri.at` + a keyset:
  `application_type = web`, `token_endpoint_auth_method = private_key_jwt`,
  inline `jwks` (public half of our keyset), `token_endpoint_auth_signing_alg =
  ES256`. Built with `AtprotoClientMetadata::new(redirect_uris, client_id,
  scopes)` + `.with_prod_info(name, logo, tos, privacy)`.
- **Public / loopback** (dev) — `http://localhost` + no keyset:
  `application_type = native`, `token_endpoint_auth_method = none`, no `jwks`.
  Built with `AtprotoClientMetadata::new_localhost(redirect_uris, scopes)`, which
  encodes redirect URIs + scope into the `client_id` query string.

`client_id` **stability**: for confidential clients the `client_id` *is* the
public metadata URL (`https://telluri.at/oauth-client-metadata.json`). Changing
the hostname re-keys every session (the spec binds confidential sessions to the
client-auth key advertised at that URL). So **confirm `telluri.at` before first
production run**. A staging origin is just a separate `[service.staging]`
variant — the [`ServiceConfig`](../crates/tass-config/src/service.rs) cascade
already supports it.

`redirect_uris` derive from `public_url + OAuthWebConfig::callback_path`
(confidential) or the loopback addresses + port (dev), so they can never drift
from the mounted callback route.

## Wiring: `AppState`

The one struct of ours that jacquard-axum's extractors demand. It is generic
over the resolver `T` and store `S` so the **same struct** serves production and
the mock-resolver test suite (mirroring
[`oauth_web_tests.rs:158`](https://github.com/rsform/jacquard/blob/main/crates/jacquard-axum/tests/oauth_web_tests.rs)):

```rust
// tass-web-auth::state  (sketch — types imported from jacquard directly)
pub struct AppState<T, S>
where
    T: OAuthResolver + DpopExt + LexiconSchemaResolver + Send + Sync + 'static,
    S: ClientAuthStore + Send + Sync + 'static,
{
    pub oauth: Arc<OAuthClient<T, S>>,
    pub web_config: OAuthWebConfig,
    pub cookie_key: Key,
}

impl<T, S> OAuthWebState<T, S> for AppState<T, S> { /* oauth_client() */ }
impl<T, S> FromRef<AppState<T, S>> for OAuthWebConfig { /* web_config */ }
impl<T, S> FromRef<AppState<T, S>> for Key { /* cookie_key */ }

pub type ProdAppState = AppState<PublicResolver, OAuthStore<TursoRepository>>;
```

Production pins both type parameters (monomorphic — no trait objects); tests
substitute `MockClient` + `MemoryAuthStore`. The three trait impls are exactly
the bounds the extractors declare (`oauth.rs:230-232`).

`tass-web`'s `main` opens the shared turso `[store]`, builds `AppState`, and
mounts the router:

```rust
// crates/tass-web/src/main.rs (sketch)
let web_config = OAuthWebConfig { login_page_path: Some("/login".into()), ..Default::default() };
let state = tass_web_auth::boot_prod(service, oauth_store, web_config)?;
let app = Router::new()
    .route("/login", get(tass_web_auth::login_page))
    .merge(jacquard_axum::oauth::routes::<
        PublicResolver, OAuthStore<TursoRepository>, tass_web_auth::ProdAppState,
    >(&state.web_config))
    // …app routes, using jacquard_axum::oauth::BrowserOAuthSession directly…
    .with_state(state);
```

Note `routes::<…>` is called with **jacquard's own types** (`PublicResolver`,
`OAuthStore<TursoRepository>`) — we don't hide them behind our own aliases at
call sites, only at the `ProdAppState` convenience alias.

## Dev vs prod

Selected by `ServiceConfig`, not a mode flag (see [`oauth.md`](oauth.md)
"Config shape"):

```toml
# dev: public/loopback
[service]
bind = "127.0.0.1:3000"
[service.oauth]
keyset_paths = []            # public client (is_confidential() = false)
scopes = ["atproto"]

# prod: confidential — telluri.at
[service]
bind = "127.0.0.1:3000"
public_url = "https://telluri.at"
[service.oauth]
keyset_paths = ["current"]   # generate-on-first-run under <state>/keyset/
client_name  = "telluri.at"
scopes = ["atproto"]         # add "transition:generic" when a webapp needs repo writes
```

`is_confidential()` (`service.rs:156`) + the `public_url` scheme decide which
metadata branch `tass-web-auth` builds. DPoP is on in both (hardcoded `true` by
`atproto_client_metadata`, `atproto.rs:399`).

## Open questions

1. **`login_page_path`.** Default `/oauth/login` vs our `/login`. **Recommend:**
   `/login` (shorter, matches [`oauth.md`](oauth.md)); set it explicitly in the
   `OAuthWebConfig` we build.
2. **`scope-check` feature.** Adds eager `include:` scope resolution at callback
   (needs `LexiconSchemaResolver`, which `PublicResolver` satisfies). Login-only
   (`atproto`) has nothing to resolve. **Recommend:** off initially; on when repo
   writes land. The `routes()` `LexiconSchemaResolver` bound is satisfied either
   way, so it's a pure feature flag.
3. **`kid` scheme for the keyset.** **Recommend:** `telluri-{YYYY-MM}` for
   readable rotation history.
4. **`cookie_name` branding.** Leave as `jacquard_oauth_session` or rename
   `tass_oauth_session`. Cosmetic; no functional difference.

## References

- Spec: [`atproto-website .../specs/oauth/en.mdx`](https://atproto.com/specs/oauth)
  — "Summary of Authorization Flow", "Types of Clients", "DPoP", "Confidential
  Client Authentication", "Client ID Metadata Document".
- Jacquard flow: `crates/jacquard-oauth/src/request.rs` (`par:558`,
  `exchange_code:700`, `refresh:634`, `revoke:761`, `build_auth:902`),
  `crates/jacquard-oauth/src/client.rs` (`start_auth:208`, `callback:273`,
  `restore:390`, `OAuthSession::send_with_opts:936`, `logout:819`).
- Jacquard axum adapter: `crates/jacquard-axum/src/oauth.rs` (`routes:422`,
  `callback_handler:501`, `BrowserOAuthSession:251`, `OAuthWebConfig:80`,
  `client_metadata_handler:454`).
- Worked example: `crates/jacquard-axum/tests/oauth_web_tests.rs` (the
  `MockClient` + `AppState` pattern).
- Reference server: `examples/axum_oauth_session.rs` (confidential client,
  generate-on-first-run keyset + cookie key, login page — `tass-web-auth` is
  this minus the dev-only `FileAuthStore` and plus `ServiceConfig`-driven boot).
- Settled config/decisions: [`oauth.md`](oauth.md),
  [`crates/tass-config/src/service.rs`](../crates/tass-config/src/service.rs).
