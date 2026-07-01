use crate::profile_config;
use clap::{Args, Subcommand};
use jacquard::client::BasicClient;
use miette::IntoDiagnostic;
use serde::Serialize;
use serde_json::Value;
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct SelfArgs {
    #[command(subcommand)]
    pub kind: SelfKind,
}

#[derive(Subcommand, Debug)]
pub enum SelfKind {
    /// Inspect actor.rpg.stats/self aggregate contents
    Stats(StatsArgs),
    /// Alias for `stats`
    List(StatsArgs),
}

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Actor DID or handle to read (default: active tassle profile)
    #[arg(short, long)]
    pub actor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SelfOutput {
    uri: String,
    cid: Option<String>,
    systems: Vec<SystemSummary>,
    raw: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SystemSummary {
    key: String,
    kind: String,
    fields: Vec<String>,
}

pub async fn run(args: SelfArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    match args.kind {
        SelfKind::Stats(args) | SelfKind::List(args) => stats(args, format).await,
    }
}

fn summarize_systems(raw: &Value) -> Vec<SystemSummary> {
    let Some(obj) = raw.as_object() else {
        return Vec::new();
    };
    let mut systems = Vec::new();
    for (key, value) in obj {
        if key.starts_with('$') || matches!(key.as_str(), "createdAt" | "updatedAt") {
            continue;
        }
        let kind = match value {
            Value::Object(_) => "object",
            Value::Array(_) => "array",
            Value::String(_) => "string",
            Value::Number(_) => "number",
            Value::Bool(_) => "bool",
            Value::Null => "null",
        };
        let fields = value
            .as_object()
            .map(|object| object.keys().cloned().collect())
            .unwrap_or_default();
        systems.push(SystemSummary {
            key: key.clone(),
            kind: kind.to_owned(),
            fields,
        });
    }
    systems.sort_by(|a, b| a.key.cmp(&b.key));
    systems
}

async fn stats(args: StatsArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    let client = BasicClient::unauthenticated();
    let actor = match args.actor {
        Some(actor) => actor,
        None => profile_config::default_did()?,
    };
    // Generic record access (tass-repo): resolve + point + getRecord.
    let resolved = tass_repo::resolve_and_point(&client, &actor)
        .await
        .map_err(|e| miette::miette!("{e}"))?;
    let Some(env) = tass_repo::get_record(&client, resolved.did.clone(), "actor.rpg.stats", "self")
        .await
        .map_err(|e| miette::miette!("{e}"))?
    else {
        miette::bail!("no actor.rpg.stats/self record for {}", resolved.did.as_str());
    };
    let raw = env.value;
    let output = SelfOutput {
        uri: env.uri,
        cid: env.cid,
        systems: summarize_systems(&raw),
        raw,
    };

    if format.is_json() {
        println!(
            "{}",
            serde_json::to_string_pretty(&output).into_diagnostic()?
        );
    } else {
        println!("actor.rpg.stats/self");
        println!("  uri: {}", output.uri);
        if let Some(cid) = &output.cid {
            println!("  cid: {cid}");
        }
        println!("  systems: {}", output.systems.len());
        for system in output.systems {
            println!("  {} ({})", system.key, system.kind);
            if !system.fields.is_empty() {
                println!("    fields: {}", system.fields.join(", "));
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}
