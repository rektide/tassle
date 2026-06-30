//! Authenticated-client construction over the active profile, behind the
//! `auth-store` feature.
//!
//! [`AuthedClient::for_active_profile`] is the single path write commands
//! should go through: it resolves the active profile from figment, opens the
//! profile's fjall app-password store, resumes the jacquard `CredentialSession`
//! (non-interactively — a `LoginRequired` becomes an error telling the caller
//! to `tassle auth login`), and points the session at the profile's PDS. The
//! session is a jacquard `XrpcClient` carrying its own auth + refresh, ready to
//! hand to e.g. `tass_quint_jac::QuintClient::new`.
//!
//! This does **not** assert the session carries any particular scope (e.g.
//! `repo:actor.rpg.stats`) yet — see the ticket notes for that follow-up.

use std::sync::Arc;

use jacquard::client::credential_session::{CredentialResumeResult, CredentialSession};
use jacquard::common::session::SessionHint;
use jacquard::identity::JacquardResolver;
use jacquard_common::xrpc::XrpcClient;

use crate::config;
use crate::Profile;

/// The concrete app-password session type for the default fjall+Cbor store.
///
/// Exposed so callers can name it in `QuintClient<AppPasswordSession>`-style
/// signatures without re-spelling the store generics. The resolver half is the
/// default `PublicResolver` (`JacquardResolver<reqwest::Client>`).
pub type AppPasswordSession = CredentialSession<
    jac_store_fjall::AppPasswordStore<
        jac_store_fjall::KvRepository<jac_store_fjall::FjallEngine, jac_store_fjall::codec::Cbor>,
    >,
    jacquard::identity::PublicResolver,
>;

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

/// An authenticated jacquard client resumed from the active profile, pointed at
/// the profile's PDS. The bundled result of "give me a ready client for the
/// active profile" — what every write path composes.
pub struct AuthedClient {
    session: AppPasswordSession,
    profile: Profile,
    name: String,
}

impl AuthedClient {
    /// Resolve the active profile, resume its session non-interactively, and
    /// point it at the profile's PDS.
    pub async fn for_active_profile() -> Result<Self, AuthError> {
        let figment =
            config::active_figment(None).map_err(|e| AuthError::Config(e.to_string()))?;
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

        let hint = SessionHint::from_optional_input(profile.did.as_deref().or(profile.handle.as_deref()));
        match session.resume(&hint).await {
            Ok(CredentialResumeResult::Resumed(_)) => {}
            Ok(CredentialResumeResult::LoginRequired(_)) => {
                return Err(AuthError::LoginRequired { profile: name });
            }
            Err(e) => return Err(AuthError::Resume(e.to_string())),
        }

        let pds = profile.pds.as_deref().ok_or(AuthError::NoPds)?;
        let pds_uri = jacquard_common::deps::fluent_uri::Uri::parse(pds)
            .map_err(|_| AuthError::Uri(pds.to_string()))?
            .to_owned();
        session.set_base_uri(pds_uri).await;

        Ok(AuthedClient { session, profile, name })
    }

    /// The resumed session — a jacquard `XrpcClient` with auth + refresh
    /// handled, already pointed at the profile's PDS.
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
