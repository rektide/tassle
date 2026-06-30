//! Authenticated-client construction over the active profile, behind the
//! `auth-store` feature.
//!
//! [`AuthedClient::for_active_profile`] resolves the active profile, opens its
//! fjall app-password store, resumes a jacquard `CredentialSession` once
//! (validating the login + pointing it at the PDS), and then **lends** that one
//! session by reference via [`AuthedClient::session`]. Consumers borrow it —
//! e.g. `QuintClient::new(authed.session())` — so a single live session is
//! shared by many concurrent uses. That's the deliberate concurrency model:
//! one `CredentialSession`, shared through `&self`, is what jacquard's
//! `send(&self)` API is built for, and it avoids the refresh-token rotation
//! races you get from running multiple sessions over the same store (see the
//! `tass-config-session-source` ticket and the upstream refresh-coordination
//! issue).

use std::sync::Arc;

use jacquard::client::credential_session::{CredentialResumeResult, CredentialSession};
use jacquard::common::session::SessionHint;
use jacquard::identity::JacquardResolver;
use jacquard_common::deps::fluent_uri::Uri;

use crate::config;
use crate::Profile;

/// The concrete app-password store + resolver backing an [`AuthedClient`].
type Store = jac_store_fjall::AppPasswordStore<
    jac_store_fjall::KvRepository<jac_store_fjall::FjallEngine, jac_store_fjall::codec::Cbor>,
>;
type Resolver = jacquard::identity::PublicResolver;

/// A live app-password session over the default fjall+Cbor store + public
/// resolver. What [`AuthedClient::session`] lends.
pub type AppPasswordSession = CredentialSession<Store, Resolver>;

/// Errors from [`AuthedClient::for_active_profile`].
#[derive(Debug)]
pub enum AuthError {
    /// Reading config / resolving the active profile failed.
    Config(String),
    /// The active profile has no `pds` to point the session at.
    NoPds,
    /// Opening the fjall session store failed.
    Store(String),
    /// `session.resume` failed (transport / server error).
    Resume(String),
    /// No resumable session in the store — the caller must `tassle auth login`
    /// first. Write paths never prompt interactively.
    LoginRequired { profile: String },
    /// The profile's PDS string wasn't a valid URI.
    Uri(String),
}

impl std::fmt::Display for AuthError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AuthError::Config(e) => write!(f, "config/active profile: {e}"),
            AuthError::NoPds => write!(f, "active profile has no `pds`"),
            AuthError::Store(e) => write!(f, "opening session store: {e}"),
            AuthError::Resume(e) => write!(f, "session resume failed: {e}"),
            AuthError::LoginRequired { profile } => {
                write!(f, "no resumable session for profile `{profile}`; run `tassle auth login` first")
            }
            AuthError::Uri(e) => write!(f, "invalid PDS URI: {e}"),
        }
    }
}

impl std::error::Error for AuthError {}

/// An authenticated client for the active profile: one live, PDS-pointed
/// [`CredentialSession`] it lends to consumers by reference.
///
/// Build with [`AuthedClient::for_active_profile`]; borrow the session with
/// [`AuthedClient::session`]. Because the session is borrowed (not cloned or
/// re-spawned), all consumers share the same `CredentialSession` — the safe
/// shape for concurrency until upstream refresh coordination lands. (Not
/// `Clone`: `CredentialSession` isn't, and cloning it is exactly what we're
/// avoiding.)
pub struct AuthedClient {
    session: AppPasswordSession,
    profile: Profile,
    name: String,
}

impl AuthedClient {
    /// Resolve a profile (CLI override > `TASSLE_PROFILE` > config selector),
    /// resume its session non-interactively, and point it at the profile's PDS.
    pub async fn for_profile(cli_profile: Option<&str>) -> Result<Self, AuthError> {
        let figment = config::active_figment(cli_profile)
            .map_err(|e| AuthError::Config(e.to_string()))?;
        let name = config::active_name(&figment);
        let profile =
            config::active_profile(&figment).map_err(|e| AuthError::Config(e.to_string()))?;

        let cfg_dir =
            config::tassle_config_dir().map_err(|e| AuthError::Config(e.to_string()))?;
        let store_path = profile
            .store_path
            .clone()
            .unwrap_or_else(|| cfg_dir.join("store").join(format!("{name}.fjall")));

        let auth = jac_store_fjall::FjallAuth::open(&store_path)
            .map_err(|e| AuthError::Store(e.to_string()))?;
        let store = Arc::new(auth.app_password());
        let resolver = Arc::new(JacquardResolver::default());
        let session = CredentialSession::new(store, resolver);

        let hint = SessionHint::from_optional_input(
            profile.did.as_deref().or(profile.handle.as_deref()),
        );
        match session.resume(&hint).await {
            Ok(CredentialResumeResult::Resumed(_)) => {}
            Ok(CredentialResumeResult::LoginRequired(_)) => {
                return Err(AuthError::LoginRequired { profile: name });
            }
            Err(e) => return Err(AuthError::Resume(e.to_string())),
        }

        let pds = profile.pds.as_deref().ok_or(AuthError::NoPds)?;
        let endpoint = Uri::parse(pds)
            .map_err(|_| AuthError::Uri(pds.to_string()))?
            .to_owned();
        session.set_endpoint(endpoint).await;

        Ok(AuthedClient { session, profile, name })
    }

    /// Convenience for [`AuthedClient::for_profile`] with no CLI override
    /// (uses `TASSLE_PROFILE` / the config selector).
    pub async fn for_active_profile() -> Result<Self, AuthError> {
        Self::for_profile(None).await
    }

    /// Lend the live session. Consumers (e.g.
    /// `QuintClient::new(authed.session())`) borrow it; the session stays owned
    /// here and is shared by every borrower.
    pub fn session(&self) -> &AppPasswordSession {
        &self.session
    }

    /// The profile name this client was resumed from.
    pub fn profile_name(&self) -> &str {
        &self.name
    }

    pub fn did(&self) -> Option<&str> {
        self.profile.did.as_deref()
    }

    pub fn handle(&self) -> Option<&str> {
        self.profile.handle.as_deref()
    }

    pub fn pds(&self) -> Option<&str> {
        self.profile.pds.as_deref()
    }
}
