//! Authenticated-client construction over the active profile, behind the
//! `auth-store` feature.
//!
//! [`AuthedClient::for_active_profile`] resolves the active profile, opens its
//! fjall app-password store, and resumes a jacquard `CredentialSession` once —
//! purely to validate the login exists and discover the session key. After that
//! it keeps a [`SessionSource`]: a **cloneable** handle to the durable half of
//! the session (the `Arc` store + resolver, the resolved key, the PDS endpoint)
//! that vends fresh, fully-working owned `CredentialSession`s on demand.

use std::sync::Arc;

use jacquard::client::credential_session::{CredentialResumeResult, CredentialSession};
use jacquard::common::session::{SessionHint, SessionKey};
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
/// resolver. What [`SessionSource::session`] and [`AuthedClient::session`]
/// produce.
pub type AppPasswordSession = CredentialSession<Store, Resolver>;

/// Errors from [`AuthedClient::for_active_profile`].
#[derive(Debug)]
pub enum AuthError {
    /// Reading config / resolving the active profile failed.
    Config(String),
    /// The active profile has no `pds` to point the session at.
    NoPds,
    /// `resume` succeeded but left no session key (unexpected — store state).
    NoSessionKey,
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
            AuthError::NoSessionKey => write!(f, "resume left no session key"),
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

/// A cloneable factory for owned [`AppPasswordSession`]s sharing one authed
/// account.
///
/// # We do not super want to lean on this
///
/// This type exists to route around `CredentialSession` not being `Clone`
/// (blocked upstream by three owned `tokio::sync::RwLock` state fields — see
/// `doc` / the auth-config ticket). Prefer, in order:
///
/// 1. **Borrow at the consumer** — make the call site take `&C: XrpcClient` so
///    the owning [`AuthedClient`] lends one session and nothing here is needed.
/// 2. **Upstream fix** — get jacquard to `Arc`-wrap `CredentialSession`'s state
///    fields so it is `Clone` directly (a ~3-line change). Then this type
///    becomes unnecessary.
///
/// Reach for `SessionSource` only when a consumer insists on an *owned*
/// `XrpcClient` (e.g. `QuintClient::new(c)` takes `C` by value) and you want
/// to hand off a session more than once without re-resolving the profile.
///
/// # Why it's cheap (and correct)
///
/// `CredentialSession::access_token()` reads the bearer token from the shared
/// `SessionStore` by key — *not* from in-memory state
/// ([`credential_session.rs:313`](https://github.com/rsform/jacquard)). So a
/// fresh session built over the same `Arc` store + resolver, with `key` and
/// `endpoint` set, authenticates correctly with **no resume and no network**.
/// Token refresh-on-401 writes back to the same store, so every session vended
/// from one `SessionSource` stays consistent. The cost per [`Self::session`] is
/// one `CredentialSession` allocation plus a couple of lock writes.
#[derive(Clone)]
pub struct SessionSource {
    store: Arc<Store>,
    resolver: Arc<Resolver>,
    key: SessionKey,
    endpoint: Uri<String>,
}

impl SessionSource {
    /// Assemble from the parts a login/resume already produced.
    pub fn new(
        store: Arc<Store>,
        resolver: Arc<Resolver>,
        key: SessionKey,
        endpoint: Uri<String>,
    ) -> Self {
        Self { store, resolver, key, endpoint }
    }

    /// Vend a fresh, PDS-pointed, store-backed [`AppPasswordSession`]. Cheap —
    /// no resume, no network (see the type-level docs). The returned session is
    /// an owned `XrpcClient` carrying its own auth + refresh.
    pub async fn session(&self) -> AppPasswordSession {
        let s = CredentialSession::new(self.store.clone(), self.resolver.clone());
        s.set_endpoint(self.endpoint.clone()).await;
        *s.key.write().await = Some(self.key.clone());
        s
    }

    /// The account/session these sessions authenticate as.
    pub fn key(&self) -> &SessionKey {
        &self.key
    }

    /// The PDS endpoint every vended session is pointed at.
    pub fn endpoint(&self) -> &Uri<String> {
        &self.endpoint
    }
}

/// An authenticated client for the active profile. Cloneable, and vends owned
/// sessions on demand via its [`SessionSource`].
///
/// Build with [`AuthedClient::for_active_profile`]; hand owned sessions to
/// consumers with [`AuthedClient::session`].
#[derive(Clone)]
pub struct AuthedClient {
    source: SessionSource,
    profile: Profile,
    name: String,
}

impl AuthedClient {
    /// Resolve the active profile, resume its session once (to validate the
    /// login and discover the key), and capture a [`SessionSource`]. After this
    /// returns, [`Self::session`] vends working owned sessions cheaply.
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
        let store: Arc<Store> = Arc::new(auth.app_password());
        let resolver: Arc<Resolver> = Arc::new(JacquardResolver::default());

        // Resume once: validates the login exists and resolves the session key.
        // The probe itself is discarded — only its key is captured.
        let probe = CredentialSession::new(store.clone(), resolver.clone());
        let hint = SessionHint::from_optional_input(
            profile.did.as_deref().or(profile.handle.as_deref()),
        );
        match probe.resume(&hint).await {
            Ok(CredentialResumeResult::Resumed(_)) => {}
            Ok(CredentialResumeResult::LoginRequired(_)) => {
                return Err(AuthError::LoginRequired { profile: name });
            }
            Err(e) => return Err(AuthError::Resume(e.to_string())),
        }
        let key = probe.key.read().await.clone().ok_or(AuthError::NoSessionKey)?;

        let pds = profile.pds.as_deref().ok_or(AuthError::NoPds)?;
        let endpoint = Uri::parse(pds)
            .map_err(|_| AuthError::Uri(pds.to_string()))?
            .to_owned();

        let source = SessionSource::new(store, resolver, key, endpoint);
        Ok(AuthedClient { source, profile, name })
    }

    /// A fresh owned session for this account (cheap; see [`SessionSource`]).
    pub async fn session(&self) -> AppPasswordSession {
        self.source.session().await
    }

    /// The underlying cloneable factory.
    pub fn source(&self) -> &SessionSource {
        &self.source
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
