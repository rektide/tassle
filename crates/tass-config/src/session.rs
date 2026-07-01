//! [`PreparedProfile`]: the resolved-but-not-yet-acted-on profile handle, behind
//! the `auth-store` feature.
//!
//! Every auth entry point used to repeat the same dance — resolve the active
//! figment, read its [`Login`], compute the store path, read the lifecycle
//! policy, precheck the DB — before doing anything auth-specific.
//! [`PreparedProfile`] runs that dance **once** (via a bon builder,
//! [`PreparedProfile::resolve`]) and hands back an "almost ready to go" handle.
//! The auth-specific terminals are builders *over* it:
//!
//! - store/session openers — [`app_password_session`](PreparedProfile::app_password_session),
//!   [`oauth_store`](PreparedProfile::oauth_store);
//! - login flows — [`app_password_login`](PreparedProfile::app_password_login),
//!   [`oauth_login`](PreparedProfile::oauth_login) (bon builders with the flow's
//!   optional knobs);
//! - store reads — [`active_account`](PreparedProfile::active_account).
//!
//! Compose by **nesting the product**, not by passing half-built builders: build
//! a `PreparedProfile`, then start a terminal builder from it
//! (`prepared.oauth_login().actor(h).call().await`). This is the shape
//! [`AuthedClient`](crate::AuthedClient) and the read path are thin front-ends on.

use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use bon::bon;
use figment2::Figment;
use jacquard::client::credential_session::{
    CredentialLoginOptions, CredentialResumeResult, CredentialSession,
};
use jacquard::common::session::SessionHint;
use jacquard::common::types::string::Did;
use jacquard::identity::JacquardResolver;
use jacquard::oauth::client::OAuthClient;
use jacquard::oauth::loopback::{LoopbackConfig, LoopbackPort};
use jacquard::oauth::types::AuthorizeOptions;

use crate::auth::{
    AppPasswordSession, AuthError, LoginOutcome, OAuthLoginOutcome, OAuthStore,
};
use crate::config::{self, StoreLifecycle};
use crate::Login;

/// A profile resolved down to everything an auth action needs — config
/// ([`figment`](Self::figment)), identity hints ([`login`](Self::login)), store
/// location ([`store_path`](Self::store_path)) and lifecycle — with the store
/// precheck already run, but **no store opened and no session built yet**. The
/// "almost ready to go" handle the auth terminals build on.
///
/// Build one with [`PreparedProfile::resolve`]; it is cheap to hold and the
/// terminals borrow it by `&self`, so a single prepared profile can drive more
/// than one action (e.g. read the active account, then resume a session).
#[derive(Clone)]
pub struct PreparedProfile {
    figment: Figment,
    name: String,
    login: Login,
    store_path: PathBuf,
    #[allow(dead_code)] // retained for callers/introspection; precheck already ran
    lifecycle: StoreLifecycle,
}

#[bon]
impl PreparedProfile {
    /// Run the shared resolve-and-precheck dance once and return the prepared
    /// handle. Supply **either** an explicit `figment` **or** a `cli_profile`
    /// override (the CLI `--profile` value); with neither, the active figment is
    /// resolved from `TASS_PROFILE` / the config `profile` selector.
    ///
    /// ```ignore
    /// // From the active profile, honoring a CLI --profile override:
    /// let p = PreparedProfile::resolve().maybe_cli_profile(cli).call().await?;
    /// // From a figment the caller already built:
    /// let p = PreparedProfile::resolve().figment(fig).call().await?;
    /// ```
    #[builder]
    pub async fn resolve(
        figment: Option<Figment>,
        #[builder(into)] cli_profile: Option<String>,
    ) -> Result<PreparedProfile, AuthError> {
        let figment = match figment {
            Some(figment) => figment,
            None => config::active_figment(cli_profile.as_deref())
                .map_err(|e| AuthError::Config(e.to_string()))?,
        };
        let name = config::active_name(&figment);
        let login =
            config::active_login(&figment).map_err(|e| AuthError::Config(e.to_string()))?;
        let store_path = config::resolve_store_path(&figment, &name)
            .map_err(|e| AuthError::Store(e.to_string()))?;
        let lifecycle =
            config::store_lifecycle(&figment).map_err(|e| AuthError::Store(e.to_string()))?;
        config::precheck_store(&store_path, &lifecycle)
            .map_err(|e| AuthError::Store(e.to_string()))?;
        Ok(PreparedProfile {
            figment,
            name,
            login,
            store_path,
            lifecycle,
        })
    }

    /// App-password login (createSession): resume-or-`createSession` against this
    /// profile's store. `actor` is the DID/handle to log in as; `password` the
    /// app password. On success the store durably holds the fresh session JWTs
    /// and the returned [`LoginOutcome`] carries the non-secret identity for the
    /// caller to persist into the profile fragment.
    ///
    /// A missing session is not an error — `LoginRequired` drives the
    /// `createSession`. This is the one app-password login path; CLI login and
    /// any future web/backfill login share it.
    #[builder]
    pub async fn app_password_login(
        &self,
        #[builder(into)] actor: String,
        #[builder(into)] password: String,
        /// Optional 2FA / email auth-factor token, when the account requires one.
        #[builder(into)]
        auth_factor_token: Option<String>,
    ) -> Result<LoginOutcome, AuthError> {
        let session = self.app_password_session().await?;
        let hint = SessionHint::from_optional_input(Some(actor.as_str()));
        let atp = match session.resume(&hint).await {
            Ok(CredentialResumeResult::Resumed(s)) => s,
            Ok(CredentialResumeResult::LoginRequired(challenge)) => session
                .login_from_challenge(
                    challenge,
                    CredentialLoginOptions {
                        password: password.into(),
                        identifier: Some(actor.clone().into()),
                        allow_takendown: None,
                        auth_factor_token: auth_factor_token.map(Into::into),
                        pds: None,
                    },
                )
                .await
                .map_err(|e| AuthError::Resume(e.to_string()))?,
            Err(e) => return Err(AuthError::Resume(e.to_string())),
        };
        Ok(LoginOutcome {
            profile_name: self.name.clone(),
            store_path: self.store_path.clone(),
            did: atp.did.to_string(),
            handle: atp.handle.to_string(),
            pds: atp.pds.as_ref().map(ToString::to_string),
        })
    }

    /// OAuth loopback (localhost) login: drive jacquard's
    /// [`OAuthClient::login_with_local_server`] — stand up an ephemeral
    /// `127.0.0.1` callback server, print (and open) the authorize URL pointed at
    /// the user's PDS/entryway, exchange the code, and **persist the session**
    /// (tokens + per-session DPoP key) into this profile's turso OAuth store.
    /// `actor` is the handle / DID / PDS host to start from.
    ///
    /// A **public (native) client**: `client_id = http://localhost`, no keyset,
    /// `token_endpoint_auth_method = none` — the CLI OAuth shape, distinct from
    /// the confidential web client. The loopback listener port is ephemeral per
    /// login and the authorization server ignores it, so nothing about the port
    /// is persisted (see `doc/oauth.md`). Scopes default to
    /// `atproto transition:generic` (jacquard's `default_localhost`).
    #[builder]
    pub async fn oauth_login(
        &self,
        #[builder(into)] actor: String,
        /// Loopback callback-server config. Defaults to an OS-assigned
        /// (ephemeral) `127.0.0.1` port with the browser opened automatically.
        loopback: Option<LoopbackConfig>,
    ) -> Result<OAuthLoginOutcome, AuthError> {
        let store = self.oauth_store().await?;
        // `login_with_local_server` overrides the redirect with the ephemeral
        // loopback address it binds, so the config's fixed-port default is
        // irrelevant — default to an OS-assigned port to avoid clashes.
        let cfg = loopback.unwrap_or(LoopbackConfig {
            port: LoopbackPort::Ephemeral,
            ..LoopbackConfig::default()
        });
        let client = OAuthClient::with_default_config(store);
        let session = client
            .login_with_local_server(actor.as_str(), AuthorizeOptions::default(), cfg)
            .await
            .map_err(|e| AuthError::Resume(e.to_string()))?;

        // Pull the authoritative identity out of the persisted session data.
        let data = session.data.read().await;
        Ok(OAuthLoginOutcome {
            profile_name: self.name.clone(),
            store_path: self.store_path.clone(),
            did: data.account_did.to_string(),
            session_id: data.session_id.to_string(),
            pds: Some(data.host_url.as_str().to_string()),
        })
    }
}

impl PreparedProfile {
    /// The selected profile name (`"default"` when none).
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The resolved [`Login`] fragment (identity + `session_id` + `auth_mode`).
    pub fn login(&self) -> &Login {
        &self.login
    }

    /// The turso auth-store DB path this profile resolves to.
    pub fn store_path(&self) -> &Path {
        &self.store_path
    }

    /// The profile-resolved figment this was built from.
    pub fn figment(&self) -> &Figment {
        &self.figment
    }

    /// Open the profile's turso app-password store + resolver and build a fresh
    /// (unresumed) [`CredentialSession`]. The single app-password store-open path.
    pub async fn app_password_session(&self) -> Result<AppPasswordSession, AuthError> {
        ensure_parent(&self.store_path)?;
        let repo = jac_stores::TursoRepository::open_local(&self.store_path)
            .await
            .map_err(|e| AuthError::Store(e.to_string()))?;
        let store = Arc::new(jac_stores::AppPasswordStore::new(repo));
        // `PublicResolver::default()` = reqwest client + system DNS + public
        // handle fallback + a bounded in-memory cache (mini_moka), all with
        // jacquard's default `ResolverOptions` / `CacheConfig`. The commented
        // `.with_*()` calls below are the knobs available to us; each shows the
        // value `default()` already applies, so uncommenting one only *overrides*
        // that single default (imports: `jacquard::identity::{PlcSource,
        // CacheConfig}`, `std::time::Duration`). See jacquard-identity.
        let resolver = Arc::new(
            JacquardResolver::default()
            // --- resolution options (ResolverOptions) ---
            // .with_plc_source(PlcSource::default())     // https://plc.directory/ (vs PlcSource::slingshot_default())
            // .with_public_fallback_for_handle(true)     // fall back to public resolveHandle
            // .with_validate_doc_id(true)                // DID-doc `id` must match requested DID
            // .with_request_timeout(Some(Duration::from_secs(20)))  // (n0_future Duration)
            // .with_system_dns()                         // TXT-record handle resolution — already on
            // --- bounded in-memory cache (mini_moka; already on) ---
            // .with_cache()                              // == CacheConfig::default():
            // .with_cache_config(
            //     CacheConfig::default()
            //         .with_handle_cache(2000, Duration::from_secs(24 * 3600))     // handle→DID:    2000 / 24h
            //         .with_did_doc_cache(1000, Duration::from_secs(72 * 3600))    // DID→doc:       1000 / 72h
            //         .with_authority_cache(1000, Duration::from_secs(168 * 3600)) // authority→DID: 1000 / 1wk
            //         .with_schema_cache(1000, Duration::from_secs(168 * 3600)),   // NSID→schema:   1000 / 1wk
            // )
        );
        Ok(CredentialSession::new(store, resolver))
    }

    /// Open the profile's turso OAuth session store.
    pub async fn oauth_store(&self) -> Result<OAuthStore, AuthError> {
        ensure_parent(&self.store_path)?;
        let repo = jac_stores::TursoRepository::open_local(&self.store_path)
            .await
            .map_err(|e| AuthError::Store(e.to_string()))?;
        Ok(OAuthStore::new(repo))
    }

    /// The store's `active_account` DID pointer, if the store exists and one is
    /// set. Never creates the DB (a missing store = no active account).
    pub async fn active_account(&self) -> Result<Option<Did>, AuthError> {
        use jac_stores::RepoCore;
        if !self.store_path.exists() {
            return Ok(None);
        }
        let repo = jac_stores::TursoRepository::open_local(&self.store_path)
            .await
            .map_err(|e| AuthError::Store(e.to_string()))?;
        repo.active_account()
            .await
            .map_err(|e| AuthError::Store(e.to_string()))
    }
}

/// Ensure the store DB's parent directory exists before `open_local` touches it.
fn ensure_parent(store_path: &Path) -> Result<(), AuthError> {
    if let Some(parent) = store_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| AuthError::Store(e.to_string()))?;
    }
    Ok(())
}
