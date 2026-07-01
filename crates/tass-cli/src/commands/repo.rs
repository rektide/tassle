use crate::profile_config;
use clap::{Args, Subcommand};
use miette::IntoDiagnostic;
use serde::Serialize;
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct RepoArgs {
    #[command(subcommand)]
    pub kind: RepoKind,
}

#[derive(Subcommand, Debug)]
pub enum RepoKind {
    /// List records from a public repo collection
    List(ListArgs),
}

#[derive(Args, Debug)]
pub struct ListArgs {
    /// Collection NSID to list, e.g. actor.rpg.stats
    pub collection: String,

    /// Repo DID or handle whose PDS should be queried (default: active tassle profile)
    #[arg(short, long)]
    pub repo: Option<String>,

    /// Maximum records to return
    #[arg(short, long, default_value_t = 50)]
    pub limit: i64,

    /// Pagination cursor from a previous response
    #[arg(long)]
    pub cursor: Option<String>,

    /// Return records in reverse order
    #[arg(long)]
    pub reverse: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ListOutput {
    repo: String,
    collection: String,
    pds: String,
    cursor: Option<String>,
    records: Vec<RecordOutput>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RecordOutput {
    uri: String,
    cid: Option<String>,
    rkey: String,
    value: serde_json::Value,
}

pub async fn run(
    args: RepoArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    match args.kind {
        RepoKind::List(args) => list(args, format, profile).await,
    }
}

async fn list(
    args: ListArgs,
    format: crate::commands::OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    if args.limit < 1 || args.limit > 100 {
        miette::bail!("--limit must be between 1 and 100");
    }

    let client = crate::commands::acquire_read_client(profile).await?;
    let repo_input = match args.repo {
        Some(repo) => repo,
        None => profile_config::default_did()?,
    };
    // Generic record access (tass-repo): resolve + point + list; the command
    // only maps the normalized envelope onto its output shape.
    let resolved = tass_repo::resolve_and_point(&client, &repo_input)
        .await
        .map_err(|e| miette::miette!("{e}"))?;
    let page = tass_repo::list_records(
        &client,
        resolved.did.clone(),
        &args.collection,
        Some(args.limit),
        args.cursor.clone(),
        args.reverse,
    )
    .await
    .map_err(|e| miette::miette!("{e}"))?;

    let records = page
        .records
        .into_iter()
        .map(|r| RecordOutput {
            uri: r.uri,
            cid: r.cid,
            rkey: r.rkey,
            value: r.value,
        })
        .collect();

    let listed = ListOutput {
        repo: resolved.did.as_str().to_owned(),
        collection: args.collection,
        pds: resolved.pds,
        cursor: page.cursor,
        records,
    };

    if format.is_json() {
        println!(
            "{}",
            serde_json::to_string_pretty(&listed).into_diagnostic()?
        );
    } else {
        println!("{}", listed.collection);
        println!("  repo: {}", listed.repo);
        println!("  pds:  {}", listed.pds);
        println!("  records: {}", listed.records.len());
        if let Some(cursor) = &listed.cursor {
            println!("  next cursor: {cursor}");
        }
        for record in listed.records {
            match record.cid {
                Some(cid) => println!("  {}  {}  {}", record.rkey, cid, record.uri),
                None => println!("  {}  {}", record.rkey, record.uri),
            }
        }
    }

    Ok(ExitCode::SUCCESS)
}
