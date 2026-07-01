//! Wiring for the `tass-web` axum OAuth login service.
//!
//! This crate is **wiring, not protocol**: jacquard (`jacquard-oauth` +
//! `jacquard-axum`) implements the entire atproto OAuth 2.1 web flow — DPoP,
//! PAR, PKCE, `private_key_jwt`, the `sub`↔`issuer` identity check, token
//! refresh, revocation. See `doc/axum.md` for the end-to-end flow and the line
//! between "theirs" and "ours".
//!
//! What's ours: build one [`AppState`] (holding jacquard's
//! [`OAuthClient`](jacquard::oauth::client::OAuthClient) constructed from a
//! [`ServiceConfig`](tass_config::ServiceConfig), the client-auth keyset, and a
//! turso session store), the keyset/cookie-key generate-on-first-run
//! lifecycle, the client-metadata derivation, and the login page.
//!
//! This crate does **not** re-export jacquard or jacquard-axum types. Code that
//! needs them imports them from their real home
//! (`jacquard::…`, `jacquard_axum::oauth::…`, `jac_stores::…`) so the boundary
//! stays legible — if a path names one of those crates, it's their code; if it
//! names `tass_web_auth`, it's ours.

pub mod cookie_key;
pub mod keyset;
pub mod login;
pub mod metadata;
pub mod state;

pub use state::{boot_prod, AppState};

use jacquard::common::deps::smol_str::SmolStr;
use jacquard_axum::oauth::OAuthWebConfig;

/// The [`OAuthWebConfig`] `tass-web` mounts with: the login page at `/login`,
/// every other route/cookie name at jacquard-axum's default.
///
/// `OAuthWebConfig::login_page_path` defaults to `/oauth/login`, but we mount
/// the login handler at `/login` — so this must be overridden, or the
/// `BrowserOAuthSession` extractor redirects unauthenticated users to a 404.
pub fn default_web_config() -> OAuthWebConfig {
    OAuthWebConfig {
        login_page_path: Some(SmolStr::from("/login")),
        ..OAuthWebConfig::default()
    }
}
