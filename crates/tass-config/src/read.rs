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

use jacquard::client::credential_session::CredentialResumeResult;
use jacquard::client::BasicClient;
use jacquard::common::deps::fluent_uri::Uri;
use jacquard::common::error::XrpcResult;
use jacquard::common::http_client::HttpClient;
use jacquard::common::session::SessionHint;
use jacquard::common::types::string::{Did, Handle};
use jacquard::common::xrpc::{CallOptions, XrpcClient, XrpcRequest, XrpcResponse};
use jacquard::common::BosStr;
use jacquard::identity::resolver::{
    DidDocResponse, IdentityError, IdentityResolver, ResolverOptions,
};
use jacquard::oauth::client::OAuthClient;

use crate::auth::{AppPasswordSession, AuthError, Resolver};
use crate::config::CredentialSelector;
use crate::session::PreparedProfile;

/// The turso-backed OAuth session store (mirrors [`crate::auth::Store`] for the
/// app-password side).
type OAuthAuthStore = jac_stores::OAuthStore<jac_stores::TursoRepository>;

/// A restored OAuth session over the turso OAuth store + public resolver.
pub type OAuthReadSession = jacquard::oauth::client::OAuthSession<Resolver, OAuthAuthStore>;

/// A read client resolved from a [`CredentialSelector`]: unauthenticated, an
/// app-password session, or a restored OAuth session — one type consumers can
/// hold and pass to `tass_repo` regardless of how auth was resolved.
///
/// Carries the authed identity's DID ([`own_did`](Self::own_did)) so a caller
/// can gate cross-actor reads: see [`for_target`](Self::for_target).
pub struct ReadClient {
    inner: Inner,
    /// The DID this client authenticates as; `None` when unauthenticated.
    own_did: Option<String>,
}

/// The three underlying client shapes, unified by a shared `HttpClient::Error`.
enum Inner {
    /// Public reads with no credential ([`BasicClient::unauthenticated`]).
    Unauthenticated(Box<BasicClient>),
    /// Reads over a resumed app-password [`CredentialSession`].
    AppPassword(Box<AppPasswordSession>),
    /// Reads over a restored OAuth session ([`OAuthReadSession`]).
    OAuth(Box<OAuthReadSession>),
}

impl ReadClient {
    /// A fresh unauthenticated client (public reads).
    pub fn unauthenticated() -> ReadClient {
        ReadClient {
            inner: Inner::Unauthenticated(Box::new(BasicClient::unauthenticated())),
            own_did: None,
        }
    }

    fn app_password(session: AppPasswordSession, own_did: String) -> ReadClient {
        ReadClient {
            inner: Inner::AppPassword(Box::new(session)),
            own_did: Some(own_did),
        }
    }

    fn oauth(session: OAuthReadSession, own_did: String) -> ReadClient {
        ReadClient {
            inner: Inner::OAuth(Box::new(session)),
            own_did: Some(own_did),
        }
    }

    /// The DID this client is authenticated as, or `None` if unauthenticated.
    pub fn own_did(&self) -> Option<&str> {
        self.own_did.as_deref()
    }

    /// A client safe to read `target_did`'s repo with: `self` when it is
    /// unauthenticated or authed for that same identity, otherwise a fresh
    /// **unauthenticated** client.
    ///
    /// This is the cross-PDS guard. An authed session's bearer token is scoped
    /// to its own PDS; pointing it at another actor's PDS (to read their repo)
    /// would ship that token to a server that has no business seeing it. Reads
    /// of *other* actors therefore drop to unauthenticated. Identity resolution
    /// (PLC/DNS) is unaffected — only the record read against the PDS is gated.
    pub fn for_target(self, target_did: &str) -> ReadClient {
        match self.own_did.as_deref() {
            Some(did) if did != target_did => ReadClient::unauthenticated(),
            _ => self,
        }
    }
}

// --- Trait delegation: mirror jacquard's own `impl … for Agent<A>`, matching on
// the arm. All arms share `Error = <Resolver as HttpClient>::Error`. ---

#[allow(clippy::manual_async_fn)]
impl HttpClient for ReadClient {
    type Error = <Resolver as HttpClient>::Error;

    fn send_http(
        &self,
        request: http::Request<Vec<u8>>,
    ) -> impl Future<Output = Result<http::Response<Vec<u8>>, Self::Error>> + Send {
        async move {
            match &self.inner {
                Inner::Unauthenticated(c) => c.send_http(request).await,
                Inner::AppPassword(c) => c.send_http(request).await,
                Inner::OAuth(c) => c.send_http(request).await,
            }
        }
    }
}

// The `-> impl Future { async move { … } }` forms below mirror jacquard's own
// `impl … for Agent<A>` signatures verbatim (including the `Self: Sync` bounds
// and the `+ Send` on `send_http`), which `async fn` can't express identically.
#[allow(clippy::manual_async_fn)]
impl XrpcClient for ReadClient {
    async fn base_uri(&self) -> Uri<String> {
        match &self.inner {
            Inner::Unauthenticated(c) => c.base_uri().await,
            Inner::AppPassword(c) => c.base_uri().await,
            Inner::OAuth(c) => c.base_uri().await,
        }
    }

    async fn set_base_uri(&self, uri: Uri<String>) {
        match &self.inner {
            Inner::Unauthenticated(c) => c.set_base_uri(uri).await,
            Inner::AppPassword(c) => c.set_base_uri(uri).await,
            Inner::OAuth(c) => c.set_base_uri(uri).await,
        }
    }

    async fn opts(&self) -> CallOptions {
        match &self.inner {
            Inner::Unauthenticated(c) => c.opts().await,
            Inner::AppPassword(c) => c.opts().await,
            Inner::OAuth(c) => c.opts().await,
        }
    }

    async fn set_opts(&self, opts: CallOptions) {
        match &self.inner {
            Inner::Unauthenticated(c) => c.set_opts(opts).await,
            Inner::AppPassword(c) => c.set_opts(opts).await,
            Inner::OAuth(c) => c.set_opts(opts).await,
        }
    }

    fn send<R>(&self, request: R) -> impl Future<Output = XrpcResult<XrpcResponse<R>>>
    where
        R: XrpcRequest + Send + Sync + serde::Serialize,
        <R as XrpcRequest>::Response: Send + Sync,
        Self: Sync,
    {
        async move {
            match &self.inner {
                Inner::Unauthenticated(c) => c.send(request).await,
                Inner::AppPassword(c) => c.send(request).await,
                Inner::OAuth(c) => c.send(request).await,
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
            match &self.inner {
                Inner::Unauthenticated(c) => c.send_with_opts(request, opts).await,
                Inner::AppPassword(c) => c.send_with_opts(request, opts).await,
                Inner::OAuth(c) => c.send_with_opts(request, opts).await,
            }
        }
    }
}

#[allow(clippy::manual_async_fn)]
impl IdentityResolver for ReadClient {
    fn options(&self) -> &ResolverOptions {
        match &self.inner {
            Inner::Unauthenticated(c) => c.options(),
            Inner::AppPassword(c) => c.options(),
            Inner::OAuth(c) => c.options(),
        }
    }

    fn resolve_handle<S: BosStr + Sync>(
        &self,
        handle: &Handle<S>,
    ) -> impl Future<Output = Result<Did, IdentityError>>
    where
        Self: Sync,
    {
        async move {
            match &self.inner {
                Inner::Unauthenticated(c) => c.resolve_handle(handle).await,
                Inner::AppPassword(c) => c.resolve_handle(handle).await,
                Inner::OAuth(c) => c.resolve_handle(handle).await,
            }
        }
    }

    fn resolve_did_doc<S: BosStr + Sync>(
        &self,
        did: &Did<S>,
    ) -> impl Future<Output = Result<DidDocResponse, IdentityError>>
    where
        Self: Sync,
    {
        async move {
            match &self.inner {
                Inner::Unauthenticated(c) => c.resolve_did_doc(did).await,
                Inner::AppPassword(c) => c.resolve_did_doc(did).await,
                Inner::OAuth(c) => c.resolve_did_doc(did).await,
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
        CredentialSelector::None => Ok(ReadClient::unauthenticated()),
        CredentialSelector::Active => resume(cli_profile, Target::Active, true).await,
        CredentialSelector::ActiveIfAvailable => resume(cli_profile, Target::Active, false).await,
        CredentialSelector::Named(name) => {
            resume(cli_profile, Target::Named(name.clone()), true).await
        }
    }
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
    let prepared = PreparedProfile::resolve()
        .maybe_cli_profile(cli_profile)
        .call()
        .await?;
    let profile = prepared.name().to_string();

    // The identity string to resume by (a DID for `@active`, the given name
    // otherwise). Absent for `@active` when no active account is set.
    let ident: Option<String> = match target {
        Target::Active => match prepared.active_account().await? {
            Some(did) => Some(did.as_str().to_string()),
            None => return absent(required, &profile),
        },
        Target::Named(name) => Some(name),
    };

    // App-password vs OAuth chosen by the profile's login kind.
    let kind = prepared.login().auth_mode.as_deref().unwrap_or("app_password");
    match kind {
        "oauth" => resume_oauth(&prepared, ident.as_deref(), required, &profile).await,
        _ => resume_app_password(&prepared, ident.as_deref(), required, &profile).await,
    }
}

/// Fall back to unauthenticated, or error, when no session is available.
fn absent(required: bool, profile: &str) -> Result<ReadClient, AuthError> {
    if required {
        Err(AuthError::LoginRequired {
            profile: profile.to_string(),
        })
    } else {
        Ok(ReadClient::unauthenticated())
    }
}

async fn resume_app_password(
    prepared: &PreparedProfile,
    ident: Option<&str>,
    required: bool,
    profile: &str,
) -> Result<ReadClient, AuthError> {
    let session = prepared.app_password_session().await?;
    let hint = SessionHint::from_optional_input(ident);
    match session.resume(&hint).await {
        // Take the DID from the resumed session (authoritative — the `ident` may
        // have been a handle), so the cross-PDS gate compares real DIDs.
        Ok(CredentialResumeResult::Resumed(atp)) => {
            Ok(ReadClient::app_password(session, atp.did.to_string()))
        }
        Ok(CredentialResumeResult::LoginRequired(_)) => absent(required, profile),
        Err(e) => Err(AuthError::Resume(e.to_string())),
    }
}

async fn resume_oauth(
    prepared: &PreparedProfile,
    ident: Option<&str>,
    required: bool,
    profile: &str,
) -> Result<ReadClient, AuthError> {
    use jac_stores::OAuthRepo;
    let store = prepared.oauth_store().await?;

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
    let own_did = key.did().to_string();

    // Restore (store-only; no interactive auth) with the native localhost client
    // metadata (public client, no keyset) — the CLI OAuth shape.
    let client = OAuthClient::with_default_config(store);
    let session = client
        .restore(&key.did(), key.session_id())
        .await
        .map_err(|e| AuthError::Resume(e.to_string()))?;
    Ok(ReadClient::oauth(session, own_did))
}
