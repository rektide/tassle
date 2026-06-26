use clap::{Args, Subcommand};
use crate::profile_config;
use jacquard::client::BasicClient;
use jacquard::identity::resolver::IdentityResolver;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::Nsid;
use jacquard_common::xrpc::XrpcClient;
use jacquard_common::xrpc::atproto::ListRecords;
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

    /// Emit machine-readable JSON, including raw values
    #[arg(short, long)]
    pub json: bool,
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

fn rkey_from_uri(uri: &str) -> &str {
    uri.rsplit('/').next().unwrap_or(uri)
}

async fn resolve_repo(
    client: &BasicClient,
    repo: &str,
) -> miette::Result<(AtIdentifier, String)> {
    let ident: AtIdentifier = AtIdentifier::new_owned(repo).into_diagnostic()?;
    match ident {
        AtIdentifier::Did(did) => {
            let pds = client.pds_for_did(&did).await.map_err(|err| {
                miette::miette!("failed to resolve PDS for {repo}: {err}")
            })?;
            Ok((AtIdentifier::Did(did), pds.to_string()))
        }
        AtIdentifier::Handle(handle) => {
            let (did, pds) = client.pds_for_handle(&handle).await.map_err(|err| {
                miette::miette!("failed to resolve PDS for {repo}: {err}")
            })?;
            Ok((AtIdentifier::Did(did), pds.to_string()))
        }
    }
}

pub async fn run(args: RepoArgs) -> miette::Result<ExitCode> {
    match args.kind {
        RepoKind::List(args) => list(args).await,
    }
}

async fn list(args: ListArgs) -> miette::Result<ExitCode> {
    if args.limit < 1 || args.limit > 100 {
        miette::bail!("--limit must be between 1 and 100");
    }

    let client = BasicClient::unauthenticated();
    let repo_input = match args.repo {
        Some(repo) => repo,
        None => profile_config::default_did()?,
    };
    let (repo, pds) = resolve_repo(&client, &repo_input).await?;
    let pds_uri = jacquard_common::deps::fluent_uri::Uri::parse(pds.clone())
        .map_err(|_| miette::miette!("resolved PDS endpoint is not a valid URI: {pds}"))?
        .to_owned();
    client.set_base_uri(pds_uri).await;

    let collection = Nsid::new_owned(&args.collection).into_diagnostic()?;
    let request = ListRecords {
        repo: repo.clone(),
        collection,
        cursor: args.cursor.clone().map(Into::into),
        limit: Some(args.limit),
        reverse: if args.reverse { Some(true) } else { None },
    };

    let response = client
        .send(request)
        .await
        .map_err(|err| miette::miette!("listRecords failed: {err}"))?;
    let output = response
        .into_output()
        .map_err(|err| miette::miette!("failed to decode listRecords output: {err}"))?;

    let records = output
        .records
        .into_iter()
        .map(|record| {
            let uri = record.uri.as_str().to_owned();
            Ok(RecordOutput {
                rkey: rkey_from_uri(&uri).to_owned(),
                cid: record.cid.map(|cid| cid.as_str().to_owned()),
                value: serde_json::to_value(&record.value).into_diagnostic()?,
                uri,
            })
        })
        .collect::<miette::Result<Vec<_>>>()?;

    let listed = ListOutput {
        repo: repo.as_str().to_owned(),
        collection: args.collection,
        pds,
        cursor: output.cursor.map(|cursor| cursor.to_string()),
        records,
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&listed).into_diagnostic()?);
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
