//! Authenticated-client construction over the active profile, behind the
//! `auth-store` feature.
//!
//! [`AuthedClient::for_active_profile`] resolves the active profile, opens its
//! turso app-password store, resumes a jacquard `CredentialSession` once
//! (validating the login + pointing it at the PDS), and then **lends** that one
//! session by reference via [`AuthedClient::session`]. Consumers borrow it —
//! e.g. `QuintClient::new(authed.session())` — so a single live session is
//! shared by many concurrent uses. That's the deliberate concurrency model:
//! one `CredentialSession`, shared through `&self`, is what jacquard's
//! `send(&self)` API is built for, and it avoids the refresh-token rotation
//! races you get from running multiple sessions over the same store (see the
//! `tass-config-session-source` ticket and the upstream refresh-coordination
//! issue).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use figment2::Figment;
use jacquard::client::credential_session::{
    CredentialLoginOptions, CredentialResumeResult, CredentialSession,
};
use jacquard::common::session::SessionHint;
use jacquard::identity::JacquardResolver;
use jacquard_common::deps::fluent_uri::Uri;

use crate::config;
use crate::Login;

/// The concrete app-password store + resolver backing an [`AuthedClient`].
///
/// Native-SQL turso backend (jac-store-fjall's engine-v2 `AuthRepository`): no
/// byte codec, no RMW lock — turso self-serializes via SQL transactions.
pub(crate) type Store = jac_stores::AppPasswordStore<jac_stores::TursoRepository>;
pub(crate) type Resolver = jacquard::identity::PublicResolver;

/// A live app-password session over the turso-backed store + public resolver.
/// What [`AuthedClient::session`] lends.
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
    login: Login,
    name: String,
}

impl AuthedClient {
    /// Resolve a profile (CLI override > `TASS_PROFILE` > config selector),
    /// resume its session non-interactively, and point it at the profile's PDS.
    pub async fn for_profile(cli_profile: Option<&str>) -> Result<Self, AuthError> {
        let figment = config::active_figment(cli_profile)
            .map_err(|e| AuthError::Config(e.to_string()))?;
        let name = config::active_name(&figment);
        let login =
            config::active_login(&figment).map_err(|e| AuthError::Config(e.to_string()))?;

        let store_path = config::resolve_store_path(&figment, &name)
            .map_err(|e| AuthError::Store(e.to_string()))?;
        let lifecycle =
            config::store_lifecycle(&figment).map_err(|e| AuthError::Store(e.to_string()))?;
        config::precheck_store(&store_path, &lifecycle)
            .map_err(|e| AuthError::Store(e.to_string()))?;

        let session = open_session_at(&store_path).await?;

        let hint = SessionHint::from_optional_input(login.account());
        match session.resume(&hint).await {
            Ok(CredentialResumeResult::Resumed(_)) => {}
            Ok(CredentialResumeResult::LoginRequired(_)) => {
                return Err(AuthError::LoginRequired { profile: name });
            }
            Err(e) => return Err(AuthError::Resume(e.to_string())),
        }

        let pds = login.pds.as_deref().ok_or(AuthError::NoPds)?;
        let endpoint = Uri::parse(pds)
            .map_err(|_| AuthError::Uri(pds.to_string()))?
            .to_owned();
        session.set_endpoint(endpoint).await;

        Ok(AuthedClient { session, login, name })
    }

    /// Convenience for [`AuthedClient::for_profile`] with no CLI override
    /// (uses `TASS_PROFILE` / the config selector).
    pub async fn for_active_profile() -> Result<Self, AuthError> {
        Self::for_profile(None).await
    }

    /// App-password login for the profile selected by `figment`: resume-or-
    /// `createSession` against that profile's store. `actor` is the DID/handle
    /// to log in as; `password` is the app password (prompting stays with the
    /// caller). On success the store durably holds the fresh session JWTs and
    /// the returned [`LoginOutcome`] carries the non-secret identity for the
    /// caller to persist into the profile fragment.
    ///
    /// Unlike [`AuthedClient::for_profile`], a missing session is not an error:
    /// `LoginRequired` drives the `createSession`. This is the one authed-client
    /// construction path — CLI login and any future web/backfill login share it
    /// rather than re-deriving the store + `CredentialSession` dance.
    pub async fn login(
        figment: &Figment,
        actor: &str,
        password: String,
    ) -> Result<LoginOutcome, AuthError> {
        let name = config::active_name(figment);
        let store_path = config::resolve_store_path(figment, &name)
            .map_err(|e| AuthError::Store(e.to_string()))?;
        let lifecycle =
            config::store_lifecycle(figment).map_err(|e| AuthError::Store(e.to_string()))?;
        config::precheck_store(&store_path, &lifecycle)
            .map_err(|e| AuthError::Store(e.to_string()))?;

        let session = open_session_at(&store_path).await?;
        let hint = SessionHint::from_optional_input(Some(actor));
        let atp = match session.resume(&hint).await {
            Ok(CredentialResumeResult::Resumed(s)) => s,
            Ok(CredentialResumeResult::LoginRequired(challenge)) => session
                .login_from_challenge(
                    challenge,
                    CredentialLoginOptions {
                        password: password.into(),
                        identifier: Some(actor.to_string().into()),
                        allow_takendown: None,
                        auth_factor_token: None,
                        pds: None,
                    },
                )
                .await
                .map_err(|e| AuthError::Resume(e.to_string()))?,
            Err(e) => return Err(AuthError::Resume(e.to_string())),
        };

        Ok(LoginOutcome {
            profile_name: name,
            store_path,
            did: atp.did.to_string(),
            handle: atp.handle.to_string(),
            pds: atp.pds.as_ref().map(ToString::to_string),
        })
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
        self.login.did.as_deref()
    }

    pub fn handle(&self) -> Option<&str> {
        self.login.handle.as_deref()
    }

    pub fn pds(&self) -> Option<&str> {
        self.login.pds.as_deref()
    }
}

/// The non-secret identity produced by a successful [`AuthedClient::login`], for
/// the caller to persist into the profile fragment. The session JWTs themselves
/// are already durably in the store — this is only what belongs in config.
#[derive(Debug, Clone)]
pub struct LoginOutcome {
    /// The profile name the login was performed under.
    pub profile_name: String,
    /// The store DB the session was persisted into.
    pub store_path: PathBuf,
    pub did: String,
    pub handle: String,
    pub pds: Option<String>,
}

/// The turso-backed OAuth session store (mirrors [`Store`] for the OAuth side).
pub(crate) type OAuthStore = jac_stores::OAuthStore<jac_stores::TursoRepository>;

/// The non-secret identity produced by a successful [`oauth_login`], for the
/// caller to persist into the profile fragment. Like [`LoginOutcome`], the
/// tokens + DPoP key are already durably in the OAuth store — this is only what
/// belongs in config. OAuth carries no handle (the flow resolves a DID + PDS),
/// so `handle` is absent unless the caller had it from the input.
#[derive(Debug, Clone)]
pub struct OAuthLoginOutcome {
    /// The profile name the login was performed under.
    pub profile_name: String,
    /// The store DB the session was persisted into.
    pub store_path: PathBuf,
    pub did: String,
    pub pds: Option<String>,
    /// The `session_id` the loopback flow assigned; the profile fragment keys the
    /// stored session by it (`did || session_id`).
    pub session_id: String,
}

/// OAuth loopback (localhost) login for the profile selected by `figment`.
///
/// Drives jacquard's [`OAuthClient::login_with_local_server`]: it stands up an
/// ephemeral `127.0.0.1` callback server, prints (and opens) the authorize URL
/// pointed at the user's PDS/entryway, waits for the redirect, exchanges the
/// code, and **persists the session** (tokens + per-session DPoP key) into the
/// profile's turso OAuth store. `actor` is the handle / DID / PDS host to start
/// from. Returns the non-secret identity for the caller to persist into the
/// profile fragment.
///
/// This is a **public (native) client**: `client_id = http://localhost`, no
/// keyset, `token_endpoint_auth_method = none` — the CLI OAuth shape, distinct
/// from the confidential web client. The loopback listener port is ephemeral per
/// login and the authorization server ignores it, so nothing about the port is
/// persisted (see `doc/oauth.md`). The scopes default to
/// `atproto transition:generic` (jacquard's `default_localhost`).
pub async fn oauth_login(
    figment: &Figment,
    actor: &str,
) -> Result<OAuthLoginOutcome, AuthError> {
    use jacquard::oauth::client::OAuthClient;
    use jacquard::oauth::loopback::{LoopbackConfig, LoopbackPort};
    use jacquard::oauth::types::AuthorizeOptions;

    let name = config::active_name(figment);
    let store_path = config::resolve_store_path(figment, &name)
        .map_err(|e| AuthError::Store(e.to_string()))?;
    let lifecycle =
        config::store_lifecycle(figment).map_err(|e| AuthError::Store(e.to_string()))?;
    config::precheck_store(&store_path, &lifecycle)
        .map_err(|e| AuthError::Store(e.to_string()))?;

    if let Some(parent) = store_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AuthError::Store(e.to_string()))?;
    }
    let repo = jac_stores::TursoRepository::open_local(&store_path)
        .await
        .map_err(|e| AuthError::Store(e.to_string()))?;
    let store = OAuthStore::new(repo);

    // Public localhost client (no keyset). `login_with_local_server` overrides the
    // redirect with the ephemeral loopback address it binds, so the fixed-port
    // default is irrelevant — ask for an OS-assigned port to avoid clashes.
    let client = OAuthClient::with_default_config(store);
    let cfg = LoopbackConfig {
        port: LoopbackPort::Ephemeral,
        ..LoopbackConfig::default()
    };
    let session = client
        .login_with_local_server(actor, AuthorizeOptions::default(), cfg)
        .await
        .map_err(|e| AuthError::Resume(e.to_string()))?;

    // Pull the authoritative identity out of the persisted session data.
    let data = session.data.read().await;
    let did = data.account_did.to_string();
    let session_id = data.session_id.to_string();
    let pds = Some(data.host_url.as_str().to_string());
    drop(data);

    Ok(OAuthLoginOutcome {
        profile_name: name,
        store_path,
        did,
        pds,
        session_id,
    })
}

/// Open the profile's turso app-password store at `store_path` and build a
/// fresh (unresumed) [`CredentialSession`] over it. The single store-open path,
/// shared by [`AuthedClient::for_profile`] and [`AuthedClient::login`] so the
/// `TursoRepository` + `AppPasswordStore` + resolver dance lives in one place.
pub(crate) async fn open_session_at(store_path: &Path) -> Result<AppPasswordSession, AuthError> {
    if let Some(parent) = store_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AuthError::Store(e.to_string()))?;
    }
    let repo = jac_stores::TursoRepository::open_local(store_path)
        .await
        .map_err(|e| AuthError::Store(e.to_string()))?;
    let store = Arc::new(jac_stores::AppPasswordStore::new(repo));
    let resolver = Arc::new(JacquardResolver::default());
    Ok(CredentialSession::new(store, resolver))
}

/// Read the access JWT of the session stored for `did`/`session_id` in the store
/// at `store_path`, if present. Returns `Ok(None)` when the store file or the
/// session is absent — it never creates the store. Expiry/liveness decoding is
/// intentionally left to the caller so this crate needs no base64/JWT deps.
pub async fn stored_access_jwt(
    store_path: &Path,
    did: &str,
    session_id: Option<&str>,
) -> Result<Option<String>, AuthError> {
    use jacquard::common::session::{SessionKey, SessionStore};
    use jacquard::common::types::did::Did;

    if !store_path.exists() {
        return Ok(None);
    }
    let Ok(did) = Did::new_owned(did) else {
        return Ok(None);
    };
    let repo = jac_stores::TursoRepository::open_local(store_path)
        .await
        .map_err(|e| AuthError::Store(e.to_string()))?;
    let store = jac_stores::AppPasswordStore::new(repo);
    let key = SessionKey::new(did, session_id.unwrap_or("session"));
    Ok(store
        .get(&key)
        .await
        .map(|s| s.access_jwt.as_str().to_string()))
}
