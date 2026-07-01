//! Selector-driven **read** client: resolve a [`CredentialSelector`] to a live
//! client that reads either unauthenticated or over a stored session.
//!
//! [`ReadClient`] is one enum over jacquard's three [`AgentSession`] shapes —
//! unauthenticated ([`BasicClient`]), an app-password [`CredentialSession`], and
//! an [`OAuthSession`] — unified under a single type by implementing
//! [`XrpcClient`] + [`IdentityResolver`] by delegation. Because all three carry
//! the same resolver/transport (`PublicResolver`), their `HttpClient::Error`
//! coincides, so the enum can present one `Error` type. That makes `ReadClient`
//! a drop-in for any consumer generic over `C: XrpcClient + IdentityResolver`
//! (notably `tass_repo`), the same way [`AuthedClient`](crate::AuthedClient)
//! serves write paths.
//!
//! The selector maps onto jac-store-fjall primitives: `@active` /
//! `@active-if-available` read the store's `active_account` meta pointer; a
//! bare handle/DID resolves directly against the session store. `@none` and a
//! missing/unresumable active session fall back to unauthenticated reads, so a
//! default profile with no login still reads public data.
//!
//! Concurrency: like `AuthedClient`, this holds **one** live session and lends
//! it by `&self` — the borrow model that sidesteps the multi-session refresh
//! race (see `tass-config-session-source`).

use std::future::Future;
use std::path::Path;

use jacquard::client::credential_session::CredentialResumeResult;
use jacquard::client::BasicClient;
use jacquard::common::session::SessionHint;
use jacquard_common::http_client::HttpClient;
use jacquard_common::types::string::BosStr;
use jacquard_common::xrpc::{CallOptions, XrpcClient, XrpcRequest, XrpcResponse, XrpcResult};
use jacquard_identity::resolver::{DidDocResponse, IdentityError, IdentityResolver, ResolverOptions};
use jacquard_identity::{JacquardResolver, PublicResolver};
use jacquard_oauth::client::OAuthClient;

use crate::auth::{open_session_at, AppPasswordSession, AuthError, Resolver};
use crate::config::{self, CredentialSelector};

/// The turso-backed OAuth session store (mirrors [`crate::auth::Store`] for the
/// app-password side).
type OAuthAuthStore = jac_store_fjall::OAuthStore<jac_store_fjall::TursoRepository>;

/// A restored OAuth session over the turso OAuth store + public resolver.
pub type OAuthReadSession = jacquard_oauth::client::OAuthSession<Resolver, OAuthAuthStore>;

/// A read client resolved from a [`CredentialSelector`]: unauthenticated, an
/// app-password session, or a restored OAuth session — one type consumers can
/// hold and pass to `tass_repo` regardless of how auth was resolved.
pub enum ReadClient {
    /// Public reads with no credential ([`BasicClient::unauthenticated`]).
    Unauthenticated(BasicClient),
    /// Reads over a resumed app-password [`CredentialSession`].
    AppPassword(AppPasswordSession),
    /// Reads over a restored [`OAuthSession`](jacquard_oauth::client::OAuthSession).
    OAuth(OAuthReadSession),
}

// --- Trait delegation: mirror jacquard's own `impl … for Agent<A>`, matching on
// the arm. All arms share `Error = <Resolver as HttpClient>::Error`. ---

impl HttpClient for ReadClient {
    type Error = <Resolver as HttpClient>::Error;

    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = Result<http::Response<Vec<u8>>, Self::Error>> + Send {
        async move {
            match self {
                ReadClient::Unauthenticated(c) => c.send_http(request).await,
                ReadClient::AppPassword(c) => c.send_http(request).await,
                ReadClient::OAuth(c) => c.send_http(request).await,
            }
        }
    }
}

impl XrpcClient for ReadClient {
    async fn base_uri(&self) -> jacquard_common::deps::fluent_uri::Uri<String> {
        match self {
            ReadClient::Unauthenticated(c) => c.base_uri().await,
            ReadClient::AppPassword(c) => c.base_uri().await,
            ReadClient::OAuth(c) => c.base_uri().await,
        }
    }

    async fn set_base_uri(&self, uri: jacquard_common::deps::fluent_uri::Uri<String>) {
        match self {
            ReadClient::Unauthenticated(c) => c.set_base_uri(uri).await,
            ReadClient::AppPassword(c) => c.set_base_uri(uri).await,
            ReadClient::OAuth(c) => c.set_base_uri(uri).await,
        }
    }

    async fn opts(&self) -> CallOptions {
        match self {
            ReadClient::Unauthenticated(c) => c.opts().await,
            ReadClient::AppPassword(c) => c.opts().await,
            ReadClient::OAuth(c) => c.opts().await,
        }
    }

    async fn set_opts(&self, opts: CallOptions) {
        match self {
            ReadClient::Unauthenticated(c) => c.set_opts(opts).await,
            ReadClient::AppPassword(c) => c.set_opts(opts).await,
            ReadClient::OAuth(c) => c.set_opts(opts).await,
        }
    }

    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync,
    {
        async move {
            match self {
                ReadClient::Unauthenticated(c) => c.send(request).await,
                ReadClient::AppPassword(c) => c.send(request).await,
                ReadClient::OAuth(c) => c.send(request).await,
            }
        }
    }

    fn send_with_opts<R>(
        &self,
        request: R,
        opts: CallOptions,
    ) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync,
    {
        async move {
            match self {
                ReadClient::Unauthenticated(c) => c.send_with_opts(request, opts).await,
                ReadClient::AppPassword(c) => c.send_with_opts(request, opts).await,
                ReadClient::OAuth(c) => c.send_with_opts(request, opts).await,
            }
        }
    }
}

impl IdentityResolver for ReadClient {
    fn options(&self) -> &ResolverOptions {
        match self {
            ReadClient::Unauthenticated(c) => c.options(),
            ReadClient::AppPassword(c) => c.options(),
            ReadClient::OAuth(c) => c.options(),
        }
    }

    fn resolve_handle<S: BosStr + Sync>(
        &self,
        handle: &jacquard_common::types::string::Handle<S>,
    ) -> impl Future<Output = Result<jacquard_common::types::string::Did, IdentityError>>
    where
        Self: Sync,
    {
        async move {
            match self {
                ReadClient::Unauthenticated(c) => c.resolve_handle(handle).await,
                ReadClient::AppPassword(c) => c.resolve_handle(handle).await,
                ReadClient::OAuth(c) => c.resolve_handle(handle).await,
            }
        }
    }

    fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        did: &jacquard_common::types::string::Did<S>,
    ) -> impl Future<Output = Result<DidDocResponse, IdentityError>>
    where
        Self: Sync,
    {
        async move {
            match self {
                ReadClient::Unauthenticated(c) => c.resolve_did_doc(did).await,
                ReadClient::AppPassword(c) => c.resolve_did_doc(did).await,
                ReadClient::OAuth(c) => c.resolve_did_doc(did).await,
            }
        }
    }
}

// --- Acquisition ---

/// Resolve a [`CredentialSelector`] to a live [`ReadClient`] for the active
/// profile (CLI override > `TASSLE_PROFILE` > config selector). The one path
/// read commands use to obtain their client.
///
/// - `@none` → unauthenticated.
/// - `@active` → the store's `active_account` session; errors if unset/unresumable.
/// - `@active-if-available` → the same, but falls back to unauthenticated instead
///   of erroring (the default, so a login-less profile still reads public data).
/// - a handle/DID → that identity's stored session, resolved directly.
pub async fn read_client(
    selector: &CredentialSelector,
    cli_profile: Option<&str>,
) -> Result<ReadClient, AuthError> {
    match selector {
        CredentialSelector::None => Ok(unauthenticated()),
        CredentialSelector::Active => resume(cli_profile, Target::Active, true).await,
        CredentialSelector::ActiveIfAvailable => resume(cli_profile, Target::Active, false).await,
        CredentialSelector::Named(name) => {
            resume(cli_profile, Target::Named(name.clone()), true).await
        }
    }
}

fn unauthenticated() -> ReadClient {
    ReadClient::Unauthenticated(BasicClient::unauthenticated())
}

/// Which identity's session to resume.
enum Target {
    /// The store's `active_account` meta pointer.
    Active,
    /// A specific handle or DID.
    Named(String),
}

async fn resume(
    cli_profile: Option<&str>,
    target: Target,
    required: bool,
) -> Result<ReadClient, AuthError> {
    let figment =
        config::active_figment(cli_profile).map_err(|e| AuthError::Config(e.to_string()))?;
    let profile = config::active_name(&figment);
    let login = config::active_login(&figment).map_err(|e| AuthError::Config(e.to_string()))?;
    let store_path =
        config::resolve_store_path(&figment, &profile).map_err(|e| AuthError::Store(e.to_string()))?;
    let lifecycle =
        config::store_lifecycle(&figment).map_err(|e| AuthError::Store(e.to_string()))?;
    config::precheck_store(&store_path, &lifecycle).map_err(|e| AuthError::Store(e.to_string()))?;

    // The identity string to resume by (a DID for `@active`, the given name
    // otherwise). Absent for `@active` when no active account is set.
    let ident: Option<String> = match target {
        Target::Active => match active_account_at(&store_path).await? {
            Some(did) => Some(did.as_str().to_string()),
            None => return absent(required, &profile),
        },
        Target::Named(name) => Some(name),
    };

    // App-password vs OAuth chosen by the profile's login kind.
    let kind = login.auth_mode.as_deref().unwrap_or("app_password");
    match kind {
        "oauth" => resume_oauth(&store_path, ident.as_deref(), required, &profile).await,
        _ => resume_app_password(&store_path, ident.as_deref(), required, &profile).await,
    }
}

/// Fall back to unauthenticated, or error, when no session is available.
fn absent(required: bool, profile: &str) -> Result<ReadClient, AuthError> {
    if required {
        Err(AuthError::LoginRequired {
            profile: profile.to_string(),
        })
    } else {
        Ok(unauthenticated())
    }
}

/// Read the store's `active_account` DID pointer, if the store exists and one is
/// set. Never creates the store (a missing DB = no active account).
async fn active_account_at(
    store_path: &Path,
) -> Result<Option<jacquard_common::types::string::Did>, AuthError> {
    use jac_store_fjall::RepoCore;
    if !store_path.exists() {
        return Ok(None);
    }
    let repo = jac_store_fjall::TursoRepository::open_local(store_path)
        .await
        .map_err(|e| AuthError::Store(e.to_string()))?;
    repo.active_account()
        .await
        .map_err(|e| AuthError::Store(e.to_string()))
}

async fn resume_app_password(
    store_path: &Path,
    ident: Option<&str>,
    required: bool,
    profile: &str,
) -> Result<ReadClient, AuthError> {
    let session = open_session_at(store_path).await?;
    let hint = SessionHint::from_optional_input(ident);
    match session.resume(&hint).await {
        Ok(CredentialResumeResult::Resumed(_)) => Ok(ReadClient::AppPassword(session)),
        Ok(CredentialResumeResult::LoginRequired(_)) => absent(required, profile),
        Err(e) => Err(AuthError::Resume(e.to_string())),
    }
}

async fn resume_oauth(
    store_path: &Path,
    ident: Option<&str>,
    required: bool,
    profile: &str,
) -> Result<ReadClient, AuthError> {
    use jac_store_fjall::OAuthRepo;
    let repo = jac_store_fjall::TursoRepository::open_local(store_path)
        .await
        .map_err(|e| AuthError::Store(e.to_string()))?;
    let store = OAuthAuthStore::new(repo);

    // Find the stored session key for the target identity (DID or handle).
    let key = match ident {
        Some(id) if id.starts_with("did:") => store.repo().first_session_by_did(id).await,
        Some(id) => store.repo().first_session_by_handle(id).await,
        None => store.repo().first_session().await,
    }
    .map_err(|e| AuthError::Store(e.to_string()))?;

    let Some(key) = key else {
        return absent(required, profile);
    };

    // Restore (store-only; no interactive auth) with the native localhost client
    // metadata (public client, no keyset) — the CLI OAuth shape.
    let client = OAuthClient::with_default_config(store);
    let session = client
        .restore(key.did(), key.session_id())
        .await
        .map_err(|e| AuthError::Resume(e.to_string()))?;
    Ok(ReadClient::OAuth(session))
}
