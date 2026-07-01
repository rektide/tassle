//! Application-state wiring: a concrete [`AppState`] holding one
//! [`OAuthClient`] built from a [`ServiceConfig`] over a turso `OAuthStore`,
//! plus the [`OAuthWebConfig`] and cookie [`Key`] jacquard-axum's extractors
//! reach for via [`FromRef`].
//!
//! [`AppState`] is pinned to production types (`PublicResolver` +
//! `OAuthStore<TursoRepository>`); the whole stack is monomorphic — no trait
//! objects. The jacquard-axum route/extractor generics are satisfied with these
//! at the call site in `tass-web`'s `main`.

use std::sync::Arc;

use axum::extract::FromRef;
use axum_extra::extract::cookie::Key;
use jacquard::identity::PublicResolver;
use jacquard::oauth::client::OAuthClient;
use jacquard_axum::oauth::{OAuthWebConfig, OAuthWebState};
use jac_stores::{OAuthStore, TursoRepository};
use tass_config::ServiceConfig;

/// The concrete OAuth session store: jac-stores' `OAuthStore` over the
/// native-SQL turso backend. `ClientAuthStore` is implemented for any
/// `OAuthStore<R: OAuthRepo>`, so this satisfies jacquard's store bound.
pub type SessionStore = OAuthStore<TursoRepository>;

/// Application state carrying the one [`OAuthClient`] `tass-web` mounts.
///
/// Holds the three things jacquard-axum's extractors reach for via [`FromRef`]:
/// the OAuth client, the web config, and the private-cookie signing key. The
/// client-auth keyset and per-session DPoP keys are *not* here — the keyset is
/// baked into the `OAuthClient`'s `ClientData` at construction; DPoP keys live
/// in the session store (jacquard manages them per session).
#[derive(Clone)]
pub struct AppState {
    /// The one OAuth client shared by every request handler.
    pub oauth: Arc<OAuthClient<PublicResolver, SessionStore>>,
    /// Route paths + cookie names (`OAuthWebConfig`).
    pub web_config: OAuthWebConfig,
    /// Private-cookie signing key (the session cookie holds only an encoded
    /// `SessionKey`, signed/encrypted with this).
    pub cookie_key: Key,
}

impl OAuthWebState<PublicResolver, SessionStore> for AppState {
    fn oauth_client(&self) -> &OAuthClient<PublicResolver, SessionStore> {
        self.oauth.as_ref()
    }
}

impl FromRef<AppState> for OAuthWebConfig {
    fn from_ref(state: &AppState) -> Self {
        state.web_config.clone()
    }
}

impl FromRef<AppState> for Key {
    fn from_ref(state: &AppState) -> Self {
        state.cookie_key.clone()
    }
}

/// Boot production [`AppState`]: load/generate the client-auth keyset + cookie
/// key under `state_dir()`, derive the client metadata from `service`, and
/// construct the [`OAuthClient`] over `store` with a default [`PublicResolver`].
///
/// `store` must already be opened (the turso DB open is `tass-web`'s job, so
/// this crate stays backend-agnostic at the API edge). Sync: keyset/cookie
/// loading is fs I/O and `OAuthClient::new_from_resolver` is sync — keeping this
/// sync avoids a clippy `unused_async`.
#[tracing::instrument(skip_all, fields(public_url = ?service.public_url, confidential = service.oauth.is_confidential()))]
pub fn boot_prod(
    service: ServiceConfig,
    store: SessionStore,
    web_config: OAuthWebConfig,
) -> miette::Result<AppState> {
    let keyset = crate::keyset::load_or_generate(&service.oauth)?;
    let cookie_key = crate::cookie_key::load_or_generate(&service)?;
    let client_data = crate::metadata::client_data(&service, keyset)?;
    let oauth = OAuthClient::new_from_resolver(store, PublicResolver::default(), client_data);
    tracing::info!(
        client_id = %oauth.registry.client_data.config.client_id.as_str(),
        has_keyset = oauth.registry.client_data.keyset.is_some(),
        "oauth client ready"
    );
    Ok(AppState {
        oauth: Arc::new(oauth),
        web_config,
        cookie_key,
    })
}
