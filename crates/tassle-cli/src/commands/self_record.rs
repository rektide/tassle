use crate::profile_config;
use clap::{Args, Subcommand};
use jacquard::client::BasicClient;
use jacquard::identity::resolver::IdentityResolver;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::{Nsid, RecordKey};
use jacquard_common::xrpc::XrpcClient;
use jacquard_common::xrpc::atproto::GetRecord;
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

async fn resolve_actor(
    client: &BasicClient,
    actor: Option<String>,
) -> miette::Result<(AtIdentifier, String)> {
    let actor = match actor {
        Some(actor) => actor,
        None => profile_config::default_did()?,
    };
    let ident: AtIdentifier = AtIdentifier::new_owned(&actor).into_diagnostic()?;
    match ident {
        AtIdentifier::Did(did) => {
            let pds = client
                .pds_for_did(&did)
                .await
                .map_err(|err| miette::miette!("failed to resolve PDS for {actor}: {err}"))?;
            Ok((AtIdentifier::Did(did), pds.to_string()))
        }
        AtIdentifier::Handle(handle) => {
            let (did, pds) = client
                .pds_for_handle(&handle)
                .await
                .map_err(|err| miette::miette!("failed to resolve PDS for {actor}: {err}"))?;
            Ok((AtIdentifier::Did(did), pds.to_string()))
        }
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
    let (repo, pds) = resolve_actor(&client, args.actor).await?;
    let pds_uri = jacquard_common::deps::fluent_uri::Uri::parse(pds.clone())
        .map_err(|_| miette::miette!("resolved PDS endpoint is not a valid URI: {pds}"))?
        .to_owned();
    client.set_base_uri(pds_uri).await;

    let request = GetRecord {
        repo,
        collection: Nsid::new_static("actor.rpg.stats").into_diagnostic()?,
        rkey: RecordKey::any_owned("self").into_diagnostic()?,
        cid: None,
    };
    let response = client
        .send(request)
        .await
        .map_err(|err| miette::miette!("getRecord actor.rpg.stats/self failed: {err}"))?;
    let record = response
        .into_output()
        .map_err(|err| miette::miette!("failed to decode actor.rpg.stats/self: {err}"))?;
    let raw = serde_json::to_value(&record.value).into_diagnostic()?;
    let output = SelfOutput {
        uri: record.uri.as_str().to_owned(),
        cid: record.cid.map(|cid| cid.as_str().to_owned()),
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
