use crate::profile_config;
use clap::{Args, Subcommand};
use jacquard_common::DefaultStr;
use miette::IntoDiagnostic;
use serde::Serialize;
use serde_json::Value;
use std::process::ExitCode;
use tass_lex_rpg::actor_rpg::stats::MageStats;

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
    uri: String,
    cid: Option<String>,
    rkey: String,
    system: Option<String>,
    mage: Option<MageStats<DefaultStr>>,
    raw: Value,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsListOutput {
    repo: String,
    collection: String,
    pds: String,
    cursor: Option<String>,
    records: Vec<tass_repo::RecordEnvelope>,
}

pub async fn run(
    args: MageArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    match args.kind {
        MageKind::List(args) => list(args, format, profile).await,
    }
}

async fn list(
    args: ListArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    let client = crate::commands::acquire_read_client(profile).await?;
    let actor = match args.actor.clone() {
        Some(actor) => actor,
        None => profile_config::default_did()?,
    };
    // Resolve + point (tass-repo), with the cross-PDS guard: an authed client
    // for another identity is downgraded before touching this actor's PDS.
    let (client, resolved) = crate::commands::resolve_read(client, &actor).await?;
    let repo = resolved.did;
    let pds = resolved.pds;

    if args.all {
        if args.limit < 1 || args.limit > 100 {
            miette::bail!("--limit must be between 1 and 100");
        }
        let repo_str = repo.as_str().to_owned();
        let page = tass_repo_mage::list_stats_records(
            &client,
            repo,
            Some(args.limit),
            args.cursor.clone(),
            false,
        )
        .await
        .map_err(|e| miette::miette!("{e}"))?;
        let output = StatsListOutput {
            repo: repo_str,
            collection: tass_repo_mage::STATS_COLLECTION.to_owned(),
            pds,
            cursor: page.cursor,
            records: page.records,
        };
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
    let rkeys = vec![rkey];

    for rkey in rkeys {
        let Some(record) = tass_repo_mage::get_stats_record(&client, repo.clone(), &rkey)
            .await
            .map_err(|e| miette::miette!("{e}"))?
        else {
            continue;
        };
        let raw = record.value.clone();
        let system = record
            .value
            .get("system")
            .and_then(Value::as_str)
            .map(ToOwned::to_owned);
        // Typed mage extraction: current records must use the lowercase
        // rpg.actor mageStats payload; legacy PascalCase sheets are rejected.
        let mage = if system.as_deref() == Some("mage") {
            tass_repo_mage::mage_stats_from_record_value(&record.value)
                .map_err(|e| miette::miette!("{e}"))?
        } else {
            None
        };
        let output = StatsOutput {
            uri: record.uri.clone(),
            cid: record.cid.clone(),
            rkey: record.rkey.clone(),
            system,
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

fn print_summary(record: &tass_repo::RecordEnvelope) {
    let system = record.value.get("system").and_then(Value::as_str);
    println!("  {}", record.rkey);
    println!("    uri: {}", record.uri);
    if let Some(system) = system {
        println!("    system: {system}");
    }
    if let Some(fields) = record.value.get("data").and_then(Value::as_object)
        && !fields.is_empty()
    {
        println!(
            "    fields: {}",
            fields.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    }
}

fn print_record(output: &StatsOutput) {
    println!("actor.rpg.stats/{}", output.rkey);
    println!("  source: {}", output.uri);
    if let Some(system) = &output.system {
        println!("  system: {system}");
    }
    if let Some(mage) = &output.mage {
        println!("  arete:        {}", display_opt(mage.arete));
        if let Some(wp) = &mage.willpower {
            println!("  willpower:    {}", display_opt(wp.permanent));
            if let Some(temporary) = wp.temporary {
                println!("  temp willpower: {temporary}");
            }
        }
        println!("  quintessence: {}", display_opt(mage.quintessence));
        if let Some(millis) = mage.milli_quintessence {
            println!("  milliQ:       {millis} millis");
        }
        println!("  paradox:      {}", display_opt(mage.paradox));
        println!("  spheres:");
        for (name, value) in [
            ("correspondence", mage.correspondence),
            ("entropy", mage.entropy),
            ("forces", mage.forces),
            ("life", mage.life),
            ("matter", mage.matter),
            ("mind", mage.mind),
            ("prime", mage.prime),
            ("spirit", mage.spirit),
            ("time", mage.time),
        ] {
            if let Some(v) = value {
                println!("    {name}: {v}");
            }
        }
    } else if let Some(fields) = output.raw.get("data").and_then(Value::as_object)
        && !fields.is_empty()
    {
        println!(
            "  fields: {}",
            fields.keys().cloned().collect::<Vec<_>>().join(", ")
        );
    }
}

fn display_opt(value: Option<i64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "absent".to_owned())
}
