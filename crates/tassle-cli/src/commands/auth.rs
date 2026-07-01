use crate::profile_config;
use clap::{Args, Subcommand};
#[cfg(not(feature = "auth-store"))]
use jacquard::client::BasicClient;
#[cfg(not(feature = "auth-store"))]
use jacquard::identity::resolver::IdentityResolver;
#[cfg(not(feature = "auth-store"))]
use jacquard_common::types::ident::AtIdentifier;
use miette::IntoDiagnostic;
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct AuthArgs {
    #[command(subcommand)]
    pub kind: AuthKind,
}

#[derive(Subcommand, Debug)]
pub enum AuthKind {
    /// App-password login: createSession, persist the session into the profile's
    /// jac-store-fjall store, and write the non-secret profile fragment.
    /// (Without the `auth-store` feature, falls back to a profile-only stub.)
    Login(LoginArgs),
    /// Show every profile and its session state; mark the active profile.
    Status(StatusArgs),
    /// Switch the active profile (writes `profile = <name>` to base config.toml).
    Switch(SwitchArgs),
    /// Read or write a key in the active profile config fragment
    ///
    /// Deprecated: prefer `tassle config set`. Retained for now as the legacy
    /// did-keyed fragment editor; will be removed or hidden as an alias.
    Set(SetArgs),
}

#[derive(Args, Debug)]
pub struct LoginArgs {
    /// DID or handle to log in as.
    pub actor: String,

    /// App password. If omitted, falls back to TASSLE_PASSWORD, then an
    /// interactive prompt. (Requires the `auth-store` feature.)
    #[arg(long)]
    pub password: Option<String>,
}

#[derive(Args, Debug)]
pub struct SetArgs {
    /// Dotted key to read, or key=value to write
    pub assignment: String,

    /// Remove the dotted key from the active profile config
    #[arg(short = 'u', long)]
    pub unset: bool,
}

#[derive(Args, Debug)]
pub struct StatusArgs {}

#[derive(Args, Debug)]
pub struct SwitchArgs {
    /// Profile name to make active (must have a config.toml.d/<name>.toml fragment).
    pub profile: String,
}

pub async fn run(
    args: AuthArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    match args.kind {
        AuthKind::Login(args) => login(args, format, profile).await,
        AuthKind::Status(args) => status(args, format, profile).await,
        AuthKind::Switch(args) => switch(args, format),
        AuthKind::Set(args) => set(args, format),
    }
}

fn split_assignment(input: &str) -> (&str, Option<&str>) {
    match input.split_once('=') {
        Some((key, value)) => (key.trim(), Some(value.trim())),
        None => (input.trim(), None),
    }
}

fn set(args: SetArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    let json = format.is_json();
    let (key, value) = split_assignment(&args.assignment);
    if key.is_empty() {
        miette::bail!("config key is required");
    }
    if args.unset && value.is_some() {
        miette::bail!("--unset expects a key, not key=value");
    }

    if args.unset {
        let (profile, removed) = profile_config::unset_profile_value(key)?;
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "profile": profile,
                    "key": key,
                    "removed": removed,
                })
            );
        } else if removed {
            println!("unset {key}");
            println!("  file: {}", profile.path.display());
        } else {
            println!("{key} was already unset");
            println!("  file: {}", profile.path.display());
        }
        return Ok(ExitCode::SUCCESS);
    }

    match value {
        Some(value) => {
            let (profile, rendered) = profile_config::write_profile_value(key, value)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "profile": profile,
                        "key": key,
                        "value": rendered,
                        "written": true,
                    })
                );
            } else {
                println!("{} = {}", key, rendered);
                println!("  file: {}", profile.path.display());
            }
        }
        None => {
            let (profile, value) = profile_config::read_profile_value(key)?;
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "profile": profile,
                        "key": key,
                        "value": value,
                        "written": false,
                    })
                );
            } else if let Some(value) = value {
                println!("{} = {}", key, value);
            } else {
                println!("{} is unset", key);
                println!("  file: {}", profile.path.display());
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}

async fn login(
    args: LoginArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    #[cfg(feature = "auth-store")]
    {
        return login_real(args, format, profile).await;
    }
    #[cfg(not(feature = "auth-store"))]
    {
        return login_profile_only(args, format).await;
    }
}

async fn status(
    args: StatusArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    use miette::IntoDiagnostic;
    let json = format.is_json();
    let profiles = tassle_config::config::available_profiles()?;
    let active_figment = tassle_config::config::active_figment(profile)?;
    let active_name = tassle_config::config::active_name(&active_figment);

    let mut rows: Vec<serde_json::Value> = Vec::new();
    for name in &profiles {
        let figment = tassle_config::config::build_figment(Some(name))?;
        let p = tassle_config::config::active_login(&figment)?;
        let store_path =
            tassle_config::config::resolve_store_path(&figment, name).unwrap_or_default();
        let session = session_status(&store_path, p.did.as_deref(), p.session_id.as_deref()).await;
        rows.push(serde_json::json!({
            "profile": name,
            "active": name == &active_name,
            "did": p.did,
            "handle": p.handle,
            "pds": p.pds,
            "store_path": store_path,
            "session": session,
        }));
    }

    if json {
        println!("{}", serde_json::to_string_pretty(&rows).into_diagnostic()?);
    } else if rows.is_empty() {
        println!("(no profiles in {})", tassle_config::config::dropins_dir()?.display());
    } else {
        for r in &rows {
            let mark = if r["active"].as_bool() == Some(true) { "*" } else { " " };
            let prof = r["profile"].as_str().unwrap_or("?");
            let did = r["did"].as_str().unwrap_or("(no did)");
            let handle = r["handle"].as_str().unwrap_or("-");
            println!(
                "{mark} {prof:<14} {did:<32} {handle:<22} {:<24} {}",
                format_session(&r["session"]),
                r["store_path"].as_str().unwrap_or("?"),
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn switch(args: SwitchArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    use miette::IntoDiagnostic;
    let profiles = tassle_config::config::available_profiles()?;
    if !profiles.iter().any(|p| p == &args.profile) {
        miette::bail!(
            "unknown profile '{}'; available: {}",
            args.profile,
            if profiles.is_empty() { "(none)".to_string() } else { profiles.join(", ") }
        );
    }
    let dir = tassle_config::config::tassle_config_dir()?;
    std::fs::create_dir_all(&dir).into_diagnostic()?;
    let base = tassle_config::config::config_file()?;
    let rendered = profile_config::write_value_at(&base, "profile", &args.profile)?;
    if format.is_json() {
        println!(
            "{}",
            serde_json::json!({ "active": args.profile, "value": rendered })
        );
    } else {
        println!("active profile: {}", args.profile);
        println!("  file: {}", base.display());
    }
    Ok(ExitCode::SUCCESS)
}

fn format_session(s: &serde_json::Value) -> String {
    match s.get("state").and_then(|v| v.as_str()) {
        Some("ok") => match s.get("exp").and_then(|v| v.as_str()) {
            Some(exp) => format!("session exp {exp}"),
            None => "session present".to_string(),
        },
        Some("expired") => "session EXPIRED".to_string(),
        Some("absent") => "no session".to_string(),
        _ => "session: n/a".to_string(),
    }
}

/// Look up the profile's stored AtpSession and decode its access-JWT exp.
/// Requires `auth-store`; never creates an empty store (skips if path absent).
#[cfg(feature = "auth-store")]
async fn session_status(
    store_path: &std::path::Path,
    did: Option<&str>,
    session_id: Option<&str>,
) -> serde_json::Value {
    use jac_store_fjall::{AppPasswordStore, TursoRepository};
    use jacquard::common::session::{SessionKey, SessionStore};
    use jacquard::common::types::did::Did;

    let absent = || serde_json::json!({ "state": "absent" });
    let Some(did_str) = did else { return absent(); };
    if !store_path.exists() {
        return absent();
    }
    let Ok(did) = Did::new_owned(did_str) else { return absent(); };
    let Ok(repo) = TursoRepository::open_local(store_path).await else { return absent(); };
    let key = SessionKey::new(did, session_id.unwrap_or("session"));
    let store = AppPasswordStore::new(repo);
    let Some(session) = store.get(&key).await else { return absent(); };

    match jwt_exp(session.access_jwt.as_str()) {
        Some(exp) => {
            let now = chrono::Utc::now().timestamp();
            let state = if exp < now { "expired" } else { "ok" };
            let iso = chrono::DateTime::from_timestamp(exp, 0)
                .map(|d| d.to_rfc3339())
                .unwrap_or_else(|| exp.to_string());
            serde_json::json!({ "state": state, "exp": iso, "exp_unix": exp })
        }
        None => serde_json::json!({ "state": "ok" }),
    }
}

#[cfg(not(feature = "auth-store"))]
async fn session_status(
    _: &std::path::Path,
    _: Option<&str>,
    _: Option<&str>,
) -> serde_json::Value {
    serde_json::json!({ "state": "n/a", "note": "build with --features auth-store for session state" })
}

#[cfg(feature = "auth-store")]
fn jwt_exp(token: &str) -> Option<i64> {
    use base64::Engine as _;
    let mut parts = token.split('.');
    let _header = parts.next()?;
    let payload = parts.next()?;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(payload)
        .ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v.get("exp").and_then(|e| e.as_i64())
}

/// Real app-password login: createSession over jacquard + persist into the
/// profile's jac-store-fjall store. Requires the `auth-store` feature.
#[cfg(feature = "auth-store")]
async fn login_real(
    args: LoginArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    use std::sync::Arc;
    use jac_store_fjall::{AppPasswordStore, TursoRepository};
    use jacquard::client::credential_session::{
        CredentialLoginOptions, CredentialResumeResult, CredentialSession,
    };
    use jacquard::common::session::SessionHint;
    use jacquard::identity::JacquardResolver;

    // Active profile name (resolves the store path + the fragment to write).
    let figment = tassle_config::config::active_figment(profile)?;
    let profile_name = tassle_config::config::active_name(&figment);

    // App password: --password > TASSLE_PASSWORD > interactive prompt.
    let password = args
        .password
        .clone()
        .or_else(|| std::env::var("TASSLE_PASSWORD").ok().filter(|s| !s.is_empty()))
        .map(Ok)
        .unwrap_or_else(|| {
            rpassword::prompt_password("App password: ")
                .map_err(|e| miette::miette!("failed to read password: {e}"))
        })?;

    // Open the profile's turso store ([store] config: explicit path, else the
    // shared/per-profile DB under state).
    let store_path = tassle_config::config::resolve_store_path(&figment, &profile_name)?;
    let lifecycle = tassle_config::config::store_lifecycle(&figment)?;
    tassle_config::config::precheck_store(&store_path, &lifecycle)?;
    if let Some(parent) = store_path.parent() {
        std::fs::create_dir_all(parent).into_diagnostic()?;
    }
    let repo = TursoRepository::open_local(&store_path)
        .await
        .map_err(|e| miette::miette!("failed to open auth store at {}: {e}", store_path.display()))?;
    let store = Arc::new(AppPasswordStore::new(repo));

    // resume-or-login. resume() returns LoginRequired as a value, not an error.
    let resolver = Arc::new(JacquardResolver::default());
    let session = CredentialSession::new(store, resolver);
    let hint = SessionHint::from_optional_input(Some(args.actor.as_str()));
    let atp = match session.resume(&hint).await {
        Ok(CredentialResumeResult::Resumed(s)) => s,
        Ok(CredentialResumeResult::LoginRequired(challenge)) => session
            .login_from_challenge(
                challenge,
                CredentialLoginOptions {
                    password: password.into(),
                    identifier: Some(args.actor.clone().into()),
                    allow_takendown: None,
                    auth_factor_token: None,
                    pds: None,
                },
            )
            .await
            .map_err(|e| miette::miette!("createSession failed: {e}"))?,
        Err(e) => return Err(miette::miette!("session resume failed: {e}")),
    };

    // The AtpSession JWTs were persisted into the fjall store by jacquard.
    // Persist the NON-secret profile fragment: did/handle/pds/session_id.
    let dir = tassle_config::config::dropins_dir()?;
    std::fs::create_dir_all(&dir).into_diagnostic()?;
    let frag = dir.join(format!("{profile_name}.toml"));
    let did = atp.did.to_string();
    let handle = atp.handle.to_string();
    profile_config::write_value_at(&frag, "did", &did)?;
    profile_config::write_value_at(&frag, "handle", &handle)?;
    profile_config::write_value_at(&frag, "session_id", "session")?;
    if let Some(pds) = &atp.pds {
        profile_config::write_value_at(&frag, "pds", &pds.to_string())?;
    }

    if format.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "profile": profile_name,
                "did": did,
                "handle": handle,
                "store": store_path.to_string_lossy(),
            })
        );
    } else {
        println!("logged in as {handle} ({did})");
        println!("  profile: {profile_name}");
        println!("  store:   {}", store_path.display());
        println!("  fragment: {}", frag.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// Profile-only bootstrap stub (ADR 0001): resolve DID/handle → PDS and save the
/// profile without authenticating. Used when the `auth-store` feature is off.
#[cfg(not(feature = "auth-store"))]
async fn login_profile_only(args: LoginArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    let client = BasicClient::unauthenticated();
    let ident: AtIdentifier = AtIdentifier::new_owned(&args.actor).into_diagnostic()?;
    let (did, handle, pds) = match ident {
        AtIdentifier::Did(did) => {
            let pds = client
                .pds_for_did(&did)
                .await
                .map_err(|err| miette::miette!("failed to resolve PDS for {}: {err}", did))?;
            (did.to_string(), None, pds.to_string())
        }
        AtIdentifier::Handle(handle) => {
            let (did, pds) = client
                .pds_for_handle(&handle)
                .await
                .map_err(|err| miette::miette!("failed to resolve PDS for {}: {err}", handle))?;
            (did.to_string(), Some(handle.to_string()), pds.to_string())
        }
    };

    let profile = profile_config::save_profile(&did, handle.as_deref(), &pds)?;

    if format.is_json() {
        println!(
            "{}",
            serde_json::to_string_pretty(&profile).into_diagnostic()?
        );
    } else {
        println!("saved tassle profile (no auth — build with --features auth-store for real login)");
        println!("  did:  {}", profile.did);
        if let Some(handle) = profile.handle {
            println!("  handle: {handle}");
        }
        println!("  pds:  {}", profile.pds);
        println!("  file: {}", profile.path.display());
    }

    Ok(ExitCode::SUCCESS)
}
