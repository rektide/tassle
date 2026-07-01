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
    /// Log in and persist the session into the profile's jac-stores (turso)
    /// store, then write the non-secret profile fragment. Defaults to
    /// app-password (createSession); `--oauth` (or a profile `auth_mode =
    /// "oauth"`) instead runs the localhost OAuth loopback flow.
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
    /// DID, handle, or PDS host to log in as.
    pub actor: String,

    /// App password. If omitted, falls back to TASS_PASSWORD, then an
    /// interactive prompt. (Requires the `auth-store` feature.) Ignored with
    /// `--oauth`.
    #[arg(long)]
    pub password: Option<String>,

    /// Log in with OAuth over a localhost loopback flow instead of an app
    /// password: opens your browser to your PDS and catches the redirect on an
    /// ephemeral 127.0.0.1 server. Also selected when the profile sets
    /// `auth_mode = "oauth"`. (Requires the `auth-store` feature.)
    #[arg(long)]
    pub oauth: bool,
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
        let figment = tass_config::config::active_figment(profile)?;
        // OAuth if `--oauth` was passed or the selected profile declares
        // `auth_mode = "oauth"`; otherwise app-password.
        let profile_oauth = tass_config::config::active_login(&figment)
            .ok()
            .and_then(|l| l.auth_mode)
            .as_deref()
            == Some("oauth");
        if args.oauth || profile_oauth {
            return login_oauth(&figment, &args.actor, format).await;
        }
        return login_real(&figment, args, format).await;
    }
    #[cfg(not(feature = "auth-store"))]
    {
        let _ = profile;
        if args.oauth {
            miette::bail!("--oauth requires building with `--features auth-store`");
        }
        login_profile_only(args, format).await
    }
}

async fn status(
    _args: StatusArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    use miette::IntoDiagnostic;
    let json = format.is_json();
    let profiles = tass_config::config::available_profiles()?;
    let active_figment = tass_config::config::active_figment(profile)?;
    let active_name = tass_config::config::active_name(&active_figment);

    let mut rows: Vec<serde_json::Value> = Vec::new();
    for name in &profiles {
        let figment = tass_config::config::build_figment(Some(name))?;
        let p = tass_config::config::active_login(&figment)?;
        let store_path =
            tass_config::config::resolve_store_path(&figment, name).unwrap_or_default();
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
        println!(
            "(no profiles in {})",
            tass_config::config::dropins_dir()?.display()
        );
    } else {
        for r in &rows {
            let mark = if r["active"].as_bool() == Some(true) {
                "*"
            } else {
                " "
            };
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
    let profiles = tass_config::config::available_profiles()?;
    if !profiles.iter().any(|p| p == &args.profile) {
        miette::bail!(
            "unknown profile '{}'; available: {}",
            args.profile,
            if profiles.is_empty() {
                "(none)".to_string()
            } else {
                profiles.join(", ")
            }
        );
    }
    let dir = tass_config::config::tass_config_dir()?;
    std::fs::create_dir_all(&dir).into_diagnostic()?;
    let base = tass_config::config::config_file()?;
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
    let absent = || serde_json::json!({ "state": "absent" });
    let Some(did) = did else {
        return absent();
    };
    // Store access lives in tass-config; the CLI only decodes expiry.
    let access_jwt = match tass_config::stored_access_jwt(store_path, did, session_id).await {
        Ok(Some(jwt)) => jwt,
        Ok(None) | Err(_) => return absent(),
    };

    match jwt_exp(&access_jwt) {
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

/// Real app-password login: delegate the store + createSession dance to
/// `tass_config::AuthedClient::login`, then persist the non-secret profile
/// fragment. Requires the `auth-store` feature.
#[cfg(feature = "auth-store")]
async fn login_real(
    figment: &figment2::Figment,
    args: LoginArgs,
    format: crate::commands::OutputFormat,
) -> miette::Result<ExitCode> {
    use tass_config::AuthedClient;

    // App password: --password > TASS_PASSWORD > interactive prompt. Reading
    // the password stays a CLI concern; the auth dance itself does not.
    let password = args
        .password
        .clone()
        .or_else(|| {
            std::env::var("TASS_PASSWORD")
                .ok()
                .filter(|s| !s.is_empty())
        })
        .map(Ok)
        .unwrap_or_else(|| {
            rpassword::prompt_password("App password: ")
                .map_err(|e| miette::miette!("failed to read password: {e}"))
        })?;

    // The one authed-client construction path (tass-config): resolve store,
    // resume-or-createSession, persist the JWTs. Returns the identity to record.
    let outcome = AuthedClient::login(figment, &args.actor, password)
        .await
        .map_err(|e| miette::miette!("login: {e}"))?;

    // Persist the NON-secret profile fragment: did/handle/pds/session_id.
    let dir = tass_config::config::dropins_dir()?;
    std::fs::create_dir_all(&dir).into_diagnostic()?;
    let frag = dir.join(format!("{}.toml", outcome.profile_name));
    profile_config::write_value_at(&frag, "did", &outcome.did)?;
    profile_config::write_value_at(&frag, "handle", &outcome.handle)?;
    profile_config::write_value_at(&frag, "session_id", "session")?;
    if let Some(pds) = &outcome.pds {
        profile_config::write_value_at(&frag, "pds", pds)?;
    }

    if format.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "profile": outcome.profile_name,
                "did": outcome.did,
                "handle": outcome.handle,
                "store": outcome.store_path.to_string_lossy(),
            })
        );
    } else {
        println!("logged in as {} ({})", outcome.handle, outcome.did);
        println!("  profile: {}", outcome.profile_name);
        println!("  store:   {}", outcome.store_path.display());
        println!("  fragment: {}", frag.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// OAuth loopback (localhost) login: delegate the whole browser + one-shot
/// callback-server dance to `tass_config::oauth_login`, then persist the
/// non-secret profile fragment (marking `auth_mode = "oauth"` so subsequent
/// reads restore over the OAuth store). Requires the `auth-store` feature.
///
/// `oauth_login` prints the authorize URL to stdout and opens the browser; this
/// only wraps its result and records config.
#[cfg(feature = "auth-store")]
async fn login_oauth(
    figment: &figment2::Figment,
    actor: &str,
    format: crate::commands::OutputFormat,
) -> miette::Result<ExitCode> {
    let outcome = tass_config::oauth_login(figment, actor)
        .await
        .map_err(|e| miette::miette!("oauth login: {e}"))?;

    // Persist the NON-secret profile fragment: did/pds/session_id + auth_mode so
    // reads know to restore over the OAuth (not app-password) store.
    let dir = tass_config::config::dropins_dir()?;
    std::fs::create_dir_all(&dir).into_diagnostic()?;
    let frag = dir.join(format!("{}.toml", outcome.profile_name));
    profile_config::write_value_at(&frag, "did", &outcome.did)?;
    profile_config::write_value_at(&frag, "auth_mode", "oauth")?;
    profile_config::write_value_at(&frag, "session_id", &outcome.session_id)?;
    if let Some(pds) = &outcome.pds {
        profile_config::write_value_at(&frag, "pds", pds)?;
    }

    if format.is_json() {
        println!(
            "{}",
            serde_json::json!({
                "profile": outcome.profile_name,
                "did": outcome.did,
                "pds": outcome.pds,
                "session_id": outcome.session_id,
                "auth_mode": "oauth",
                "store": outcome.store_path.to_string_lossy(),
            })
        );
    } else {
        println!("logged in via OAuth as {}", outcome.did);
        println!("  profile: {}", outcome.profile_name);
        if let Some(pds) = &outcome.pds {
            println!("  pds:     {pds}");
        }
        println!("  store:   {}", outcome.store_path.display());
        println!("  fragment: {}", frag.display());
    }
    Ok(ExitCode::SUCCESS)
}

/// Profile-only bootstrap stub (ADR 0001): resolve DID/handle → PDS and save the
/// profile without authenticating. Used when the `auth-store` feature is off.
#[cfg(not(feature = "auth-store"))]
async fn login_profile_only(
    args: LoginArgs,
    format: crate::commands::OutputFormat,
) -> miette::Result<ExitCode> {
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
        println!(
            "saved tassle profile (no auth — build with --features auth-store for real login)"
        );
        println!("  did:  {}", profile.did);
        if let Some(handle) = profile.handle {
            println!("  handle: {handle}");
        }
        println!("  pds:  {}", profile.pds);
        println!("  file: {}", profile.path.display());
    }

    Ok(ExitCode::SUCCESS)
}
