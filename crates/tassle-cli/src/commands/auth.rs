use crate::profile_config;
use clap::{Args, Subcommand};
use jacquard::client::BasicClient;
use jacquard::identity::resolver::IdentityResolver;
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
    /// Resolve and save a local profile as the default actor; OAuth comes later
    Login(LoginArgs),
    /// Read or write a key in the active profile config fragment
    Set(SetArgs),
}

#[derive(Args, Debug)]
pub struct LoginArgs {
    /// DID or handle for the profile to save
    pub actor: String,

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
        println!("saved tassle profile");
        println!("  did:  {}", profile.did);
        if let Some(handle) = profile.handle {
            println!("  handle: {handle}");
        }
        println!("  pds:  {}", profile.pds);
        println!("  file: {}", profile.path.display());
        println!("  default repo for reads is now this DID");
    }

    Ok(ExitCode::SUCCESS)
}
