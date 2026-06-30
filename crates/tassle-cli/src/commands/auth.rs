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

    /// Emit machine-readable JSON
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Args, Debug)]
pub struct SetArgs {
    /// Dotted key to read, or key=value to write
    pub assignment: String,

    /// Remove the dotted key from the active profile
    #[arg(short = 'u', long)]
    pub unset: bool,

    /// Emit machine-readable JSON
    #[arg(short, long)]
    pub json: bool,
}

pub async fn run(args: AuthArgs) -> miette::Result<ExitCode> {
    match args.kind {
        AuthKind::Login(args) => login(args).await,
        AuthKind::Set(args) => set(args),
    }
}

fn split_assignment(input: &str) -> (&str, Option<&str>) {
    match input.split_once('=') {
        Some((key, value)) => (key.trim(), Some(value.trim())),
        None => (input.trim(), None),
    }
}

fn set(args: SetArgs) -> miette::Result<ExitCode> {
    let (key, value) = split_assignment(&args.assignment);
    if key.is_empty() {
        miette::bail!("config key is required");
    }
    if args.unset && value.is_some() {
        miette::bail!("--unset expects a key, not key=value");
    }

    if args.unset {
        let (profile, removed) = profile_config::unset_profile_value(key)?;
        if args.json {
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
            if args.json {
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
            if args.json {
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

async fn login(args: LoginArgs) -> miette::Result<ExitCode> {
    #[cfg(feature = "auth-store")]
    {
        return login_real(args).await;
    }
    #[cfg(not(feature = "auth-store"))]
    {
        return login_profile_only(args).await;
    }
}

/// Real app-password login: createSession over jacquard + persist into the
/// profile's jac-store-fjall store. Requires the `auth-store` feature.
#[cfg(feature = "auth-store")]
async fn login_real(args: LoginArgs) -> miette::Result<ExitCode> {
    use std::sync::Arc;
    use jac_store_fjall::FjallAuth;
    use jacquard::client::credential_session::{
        CredentialLoginOptions, CredentialResumeResult, CredentialSession,
    };
    use jacquard::common::session::SessionHint;
    use jacquard::identity::JacquardResolver;

    // Active profile name (resolves the store path + the fragment to write).
    let figment = crate::config::active_figment(None)?;
    let profile_name = crate::config::active_name(&figment);
    let active = crate::config::active_profile(&figment)?;

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

    // Open the profile's fjall store (explicit store_path, else a per-profile default).
    let store_path = active.store_path.clone().unwrap_or_else(|| {
        crate::config::tassle_config_dir()
            .expect("config dir")
            .join("store")
            .join(format!("{profile_name}.fjall"))
    });
    std::fs::create_dir_all(&store_path).into_diagnostic()?;
    let auth = FjallAuth::open(&store_path)
        .map_err(|e| miette::miette!("failed to open auth store at {}: {e}", store_path.display()))?;
    let store = Arc::new(auth.app_password());

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
    let dir = crate::config::dropins_dir()?;
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

    if args.json {
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
async fn login_profile_only(args: LoginArgs) -> miette::Result<ExitCode> {
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

    if args.json {
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
