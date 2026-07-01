use crate::profile_config;
use clap::{Args, Subcommand};
use jacquard::client::BasicClient;
use jacquard_common::types::ident::AtIdentifier;
use miette::IntoDiagnostic;
use serde::Serialize;
use serde_json::Value;
use std::process::ExitCode;

const STATS_COLLECTION: &str = "actor.rpg.stats";

#[derive(Args, Debug)]
pub struct MageArgs {
    #[command(subcommand)]
    pub kind: MageKind,
}

#[derive(Subcommand, Debug)]
pub enum MageKind {
    /// List/read actor.rpg.stats records; alias: stats
    #[command(alias = "stats")]
    List(ListArgs),
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Stats rkey/system to read (default: mage)
    pub rkey: Option<String>,

    /// Actor DID or handle to read (default: active tassle profile)
    #[arg(short, long)]
    pub actor: Option<String>,

    /// List all actor.rpg.stats records instead of reading one rkey
    #[arg(long)]
    pub all: bool,

    /// Maximum records to return with --all
    #[arg(short, long, default_value_t = 50)]
    pub limit: i64,

    /// Pagination cursor for --all
    #[arg(long)]
    pub cursor: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsOutput {
    source: StatsSource,
    summary: StatsSummary,
    mage: Option<tass_mage::NormalizedMageStats>,
    raw: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsListOutput {
    repo: String,
    collection: String,
    pds: String,
    cursor: Option<String>,
    records: Vec<StatsSummary>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsSource {
    uri: String,
    cid: Option<String>,
    rkey: String,
    shape: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsSummary {
    uri: String,
    cid: Option<String>,
    rkey: String,
    shape: String,
    system: Option<String>,
    fields: Vec<String>,
}

pub async fn run(args: MageArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    match args.kind {
        MageKind::List(args) => list(args, format).await,
    }
}

async fn get_stats_record(
    client: &BasicClient,
    repo: AtIdentifier,
    rkey: &str,
) -> miette::Result<Option<tass_repo::RecordEnvelope>> {
    tass_repo::get_record(client, repo, STATS_COLLECTION, rkey)
        .await
        .map_err(|e| miette::miette!("{e}"))
}

async fn list_stats_records(
    client: &BasicClient,
    repo: AtIdentifier,
    pds: String,
    args: &ListArgs,
) -> miette::Result<StatsListOutput> {
    if args.limit < 1 || args.limit > 100 {
        miette::bail!("--limit must be between 1 and 100");
    }
    let repo_str = repo.as_str().to_owned();
    let page = tass_repo::list_records(
        client,
        repo,
        STATS_COLLECTION,
        Some(args.limit),
        args.cursor.clone(),
        false,
    )
    .await
    .map_err(|e| miette::miette!("{e}"))?;
    let records = page
        .records
        .into_iter()
        .map(|env| summarize_record(&env.uri, env.cid.as_deref(), &env.value))
        .collect();

    Ok(StatsListOutput {
        repo: repo_str,
        collection: STATS_COLLECTION.to_owned(),
        pds,
        cursor: page.cursor,
        records,
    })
}

fn rkey_from_uri(uri: &str) -> &str {
    uri.rsplit('/').next().unwrap_or(uri)
}

fn object<'a>(value: &'a Value, field: &str) -> Option<&'a serde_json::Map<String, Value>> {
    value.get(field)?.as_object()
}

fn fields(value: &Value) -> Vec<String> {
    value
        .as_object()
        .map(|object| object.keys().cloned().collect())
        .unwrap_or_default()
}

fn stats_payload(value: &Value, rkey: &str) -> (Option<Value>, String, Option<String>) {
    if let Some(system) = value.get("system").and_then(Value::as_str) {
        if let Some(data) = value.get("data") {
            return (
                Some(data.clone()),
                "per-system-envelope".to_owned(),
                Some(system.to_owned()),
            );
        }
    }

    if rkey == "self" {
        return (None, "legacy-self-aggregate".to_owned(), None);
    }

    if let Some(system) = object(value, rkey) {
        return (
            Some(Value::Object(system.clone())),
            "legacy-inline-system".to_owned(),
            Some(rkey.to_owned()),
        );
    }

    (None, "unknown".to_owned(), None)
}

fn summarize_record(uri: &str, cid: Option<&str>, value: &Value) -> StatsSummary {
    let rkey = rkey_from_uri(uri).to_owned();
    let (payload, shape, system) = stats_payload(value, &rkey);
    let summary_fields = payload
        .as_ref()
        .map(fields)
        .unwrap_or_else(|| fields(value));
    StatsSummary {
        uri: uri.to_owned(),
        cid: cid.map(ToOwned::to_owned),
        rkey,
        shape,
        system,
        fields: summary_fields,
    }
}

async fn list(args: ListArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    let client = BasicClient::unauthenticated();
    let actor = match args.actor.clone() {
        Some(actor) => actor,
        None => profile_config::default_did()?,
    };
    // Generic record access (tass-repo): resolve + point the client at the PDS.
    let resolved = tass_repo::resolve_and_point(&client, &actor)
        .await
        .map_err(|e| miette::miette!("{e}"))?;
    let repo = resolved.did;
    let pds = resolved.pds;

    if args.all {
        let output = list_stats_records(&client, repo, pds, &args).await?;
        if format.is_json() {
            println!(
                "{}",
                serde_json::to_string_pretty(&output).into_diagnostic()?
            );
        } else {
            println!("{}", output.collection);
            println!("  repo: {}", output.repo);
            println!("  pds:  {}", output.pds);
            println!("  records: {}", output.records.len());
            if let Some(cursor) = &output.cursor {
                println!("  next cursor: {cursor}");
            }
            for record in output.records {
                print_summary(&record);
            }
        }
        return Ok(ExitCode::SUCCESS);
    }

    let rkey = args.rkey.clone().unwrap_or_else(|| "mage".to_owned());
    let rkeys = if rkey == "mage" {
        vec!["mage".to_owned(), "self".to_owned()]
    } else {
        vec![rkey]
    };

    for rkey in rkeys {
        let Some(record) = get_stats_record(&client, repo.clone(), &rkey).await? else {
            continue;
        };
        let summary = summarize_record(&record.uri, record.cid.as_deref(), &record.value);
        let value = record.value;
        let raw = if rkey == "mage" || rkey == "self" {
            tass_mage::mage_block(&value)
                .map(|block| Value::Object(block.clone()))
                .unwrap_or_else(|| value.clone())
        } else {
            stats_payload(&value, &rkey)
                .0
                .unwrap_or_else(|| value.clone())
        };
        let mage = if summary.system.as_deref() == Some("mage") || rkey == "self" {
            tass_mage::normalize(&value)
        } else {
            None
        };
        let output = StatsOutput {
            source: StatsSource {
                uri: summary.uri.clone(),
                cid: summary.cid.clone(),
                rkey: summary.rkey.clone(),
                shape: summary.shape.clone(),
            },
            summary,
            mage,
            raw,
        };

        if format.is_json() {
            println!(
                "{}",
                serde_json::to_string_pretty(&output).into_diagnostic()?
            );
        } else {
            print_record(&output);
        }
        return Ok(ExitCode::SUCCESS);
    }

    miette::bail!("no actor.rpg.stats record found for requested rkey")
}

fn print_summary(record: &StatsSummary) {
    println!("  {} ({})", record.rkey, record.shape);
    println!("    uri: {}", record.uri);
    if let Some(system) = &record.system {
        println!("    system: {system}");
    }
    if !record.fields.is_empty() {
        println!("    fields: {}", record.fields.join(", "));
    }
}

fn print_record(output: &StatsOutput) {
    println!("actor.rpg.stats/{}", output.source.rkey);
    println!("  source: {}", output.source.uri);
    println!("  shape:  {}", output.source.shape);
    if let Some(system) = &output.summary.system {
        println!("  system: {system}");
    }
    if let Some(mage) = &output.mage {
        println!("  arete:        {}", display_opt(mage.arete));
        println!("  willpower:    {}", display_opt(mage.willpower));
        if let Some(temporary) = mage.willpower_temporary {
            println!("  temp willpower: {temporary}");
        }
        println!("  quintessence: {}", display_opt(mage.quintessence));
        if let Some(millis) = mage.milli_quintessence {
            println!("  milliQ:       {millis} millis");
        }
        println!("  paradox:      {}", display_opt(mage.paradox));
        println!("  spheres:");
        for (sphere, value) in &mage.spheres {
            println!("    {sphere}: {value}");
        }
        if !mage.missing.is_empty() {
            println!("  missing: {}", mage.missing.join(", "));
        }
    } else if !output.summary.fields.is_empty() {
        println!("  fields: {}", output.summary.fields.join(", "));
    }
}

fn display_opt(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "absent".to_owned())
}
