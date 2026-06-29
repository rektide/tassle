use crate::config;
use crate::profile_config;
use clap::{Args, Subcommand};
use figment2::ops::{OperationStatus, RecordedIntent};
use miette::IntoDiagnostic;
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub kind: ConfigKind,
}

#[derive(Subcommand, Debug)]
pub enum ConfigKind {
    /// Show every loaded config source and the profile-selection audit log.
    Files,
    /// List available profiles (drop-in fragments), marking the active one.
    List,
    /// Print the active profile (or a single field by dotted key).
    Get {
        /// Optional dotted key (e.g. `did`, `pds`); omit for the whole profile.
        key: Option<String>,
    },
    /// Read or write a dotted key in the active profile fragment.
    ///
    /// `config set key=value` writes; `config set key` reads; `config set -u key`
    /// removes. The special key `profile` edits the base config.toml selector
    /// (i.e. switches the active profile).
    Set {
        /// Dotted key to read, or `key=value` to write.
        assignment: String,
        /// Remove the dotted key from the active profile fragment.
        #[arg(short = 'u', long)]
        unset: bool,
    },
}

pub fn run(args: ConfigArgs) -> miette::Result<ExitCode> {
    match args.kind {
        ConfigKind::Files => files(),
        ConfigKind::List => list(),
        ConfigKind::Get { key } => get(key),
        ConfigKind::Set { assignment, unset } => set(&assignment, unset),
    }
}

fn files() -> miette::Result<ExitCode> {
    let figment = config::active_figment(None)?;
    println!("active profile: {}", config::active_name(&figment));
    println!("config file:    {}", config::config_file()?.display());
    println!("drop-ins dir:   {}", config::dropins_dir()?.display());
    println!("operation log:");
    for rec in figment.operation_records() {
        println!(
            "  #{:<3} {:<8} {}",
            rec.id,
            status_str(rec.status),
            summarize(&rec.intent)
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn list() -> miette::Result<ExitCode> {
    let active_name = config::active_figment(None)
        .ok()
        .as_ref()
        .map(config::active_name)
        .unwrap_or_default();
    let profiles = config::available_profiles()?;
    if profiles.is_empty() {
        println!("(no profile fragments in {})", config::dropins_dir()?.display());
        return Ok(ExitCode::SUCCESS);
    }
    for p in profiles {
        let mark = if p == active_name { "*" } else { " " };
        println!("{mark} {p}");
    }
    Ok(ExitCode::SUCCESS)
}

fn get(key: Option<String>) -> miette::Result<ExitCode> {
    let figment = config::active_figment(None)?;
    match key {
        None => {
            let p = config::active_profile(&figment)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&p).into_diagnostic()?
            );
        }
        Some(k) => {
            let v: serde_json::Value = figment
                .extract_inner(&k)
                .map_err(|e| miette::miette!("'{k}' not found: {e}"))?;
            println!("{v}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn set(assignment: &str, unset: bool) -> miette::Result<ExitCode> {
    let (key, value) = split_assignment(assignment);
    if key.is_empty() {
        miette::bail!("config key is required");
    }
    if unset && value.is_some() {
        miette::bail!("--unset expects a key, not key=value");
    }

    // The `profile` key lives in the base config.toml (the active selector);
    // every other key belongs to the active profile's drop-in fragment.
    let target = if key == "profile" {
        config::config_file()?
    } else {
        let active = config::active_name(&config::active_figment(None)?);
        let dir = config::dropins_dir()?;
        std::fs::create_dir_all(&dir).into_diagnostic()?;
        dir.join(format!("{active}.toml"))
    };

    if unset {
        let removed = profile_config::unset_value_at(&target, key)?;
        println!(
            "{} {}",
            if removed { "unset" } else { "(already unset)" },
            key
        );
        println!("  file: {}", target.display());
        return Ok(ExitCode::SUCCESS);
    }

    match value {
        Some(value) => {
            let rendered = profile_config::write_value_at(&target, key, value)?;
            println!("{key} = {rendered}");
            println!("  file: {}", target.display());
        }
        None => {
            // Bare `config set key` reads — same view as `config get key`.
            let figment = config::active_figment(None)?;
            let v: serde_json::Value = figment
                .extract_inner(key)
                .map_err(|e| miette::miette!("'{key}' not found: {e}"))?;
            println!("{v}");
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn split_assignment(input: &str) -> (&str, Option<&str>) {
    match input.split_once('=') {
        Some((key, value)) => (key.trim(), Some(value.trim())),
        None => (input.trim(), None),
    }
}

fn status_str(status: OperationStatus) -> &'static str {
    match status {
        OperationStatus::Applied => "applied",
        OperationStatus::Noop => "noop",
        OperationStatus::Failed => "failed",
    }
}

fn summarize(intent: &RecordedIntent) -> String {
    match intent {
        RecordedIntent::Profile { profile, reason } => {
            let why = reason
                .as_deref()
                .map(|r| format!("  ({r})"))
                .unwrap_or_default();
            format!("select profile '{}'{why}", profile.as_str())
        }
        RecordedIntent::Provide { provider, coalesce } => {
            format!("load drop-in via {provider} [{coalesce:?}]")
        }
        RecordedIntent::Diagnostic { level, message } => {
            format!("{level:?}: {message}")
        }
        RecordedIntent::Assert { path, reason } => {
            let why = reason
                .as_deref()
                .map(|r| format!("  ({r})"))
                .unwrap_or_default();
            format!("assert present '{path}'{why}")
        }
        RecordedIntent::Scope { policy, len } => {
            format!("scope ({len} ops, {policy:?})")
        }
        RecordedIntent::Custom { kind, .. } => format!("custom {kind}"),
        RecordedIntent::Operator => "operator".to_string(),
    }
}
