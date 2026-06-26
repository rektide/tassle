use crate::profile_config;
use clap::{Args, Subcommand};
use jacquard::client::BasicClient;
use jacquard::identity::resolver::IdentityResolver;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::{Nsid, RecordKey};
use jacquard_common::xrpc::atproto::{GetRecord, GetRecordError, GetRecordOutput};
use jacquard_common::xrpc::{XrpcClient, XrpcError};
use miette::IntoDiagnostic;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::process::ExitCode;

#[derive(Args, Debug)]
pub struct MageArgs {
    #[command(subcommand)]
    pub kind: MageKind,
}

#[derive(Subcommand, Debug)]
pub enum MageKind {
    /// Read Mage stats from actor.rpg.stats, the player's state/history anchor
    Stats(StatsArgs),
    /// Alias for `stats`
    List(StatsArgs),
}

#[derive(Args, Debug)]
pub struct StatsArgs {
    /// Actor DID or handle to read (default: active tassle profile)
    #[arg(short, long)]
    pub actor: Option<String>,

    /// Explicit rkey override for debugging; skips fallback order
    #[arg(short, long)]
    pub rkey: Option<String>,

    /// Emit normalized JSON with raw source payload
    #[arg(short, long)]
    pub json: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StatsOutput {
    source: StatsSource,
    stats: NormalizedStats,
    raw: Value,
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
struct NormalizedStats {
    arete: Option<i64>,
    willpower: Option<i64>,
    quintessence: Option<i64>,
    paradox: Option<i64>,
    spheres: BTreeMap<String, i64>,
    missing: Vec<String>,
}

pub async fn run(args: MageArgs) -> miette::Result<ExitCode> {
    match args.kind {
        MageKind::Stats(args) => stats(args).await,
        MageKind::List(args) => stats(args).await,
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

async fn get_stats_record(
    client: &BasicClient,
    repo: AtIdentifier,
    rkey: &str,
) -> miette::Result<Option<GetRecordOutput>> {
    let request = GetRecord {
        repo,
        collection: Nsid::new_static("actor.rpg.stats").into_diagnostic()?,
        rkey: RecordKey::any_owned(rkey).into_diagnostic()?,
        cid: None,
    };
    let response = client
        .send(request)
        .await
        .map_err(|err| miette::miette!("getRecord actor.rpg.stats/{rkey} failed: {err}"))?;
    match response.into_output() {
        Ok(output) => Ok(Some(output)),
        Err(XrpcError::Xrpc(GetRecordError::RecordNotFound(_))) => Ok(None),
        Err(err) => Err(miette::miette!(
            "failed to decode actor.rpg.stats/{rkey}: {err}"
        )),
    }
}

fn object<'a>(value: &'a Value, field: &str) -> Option<&'a serde_json::Map<String, Value>> {
    value.get(field)?.as_object()
}

fn extract_mage(value: &Value, rkey: &str) -> Option<(Value, String)> {
    if rkey == "mage" && value.get("system")?.as_str()? == "mage" {
        return Some((value.get("data")?.clone(), "per-system-envelope".to_owned()));
    }
    object(value, "mage").map(|mage| (Value::Object(mage.clone()), "legacy-self-inline".to_owned()))
}

fn number_field(obj: &serde_json::Map<String, Value>, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|name| obj.get(*name)?.as_i64())
}

fn normalize_stats(raw: &Value) -> miette::Result<NormalizedStats> {
    let obj = raw
        .as_object()
        .ok_or_else(|| miette::miette!("mage stats payload is not an object"))?;
    let mut missing = Vec::new();
    let arete = number_field(obj, &["arete", "Arete"]);
    let willpower = number_field(obj, &["willpower", "Willpower"]);
    let quintessence = number_field(obj, &["quintessence", "Quintessence"]);
    let paradox = number_field(obj, &["paradox", "Paradox"]);

    for (name, value) in [
        ("arete", arete),
        ("willpower", willpower),
        ("quintessence", quintessence),
        ("paradox", paradox),
    ] {
        if value.is_none() {
            missing.push(name.to_owned());
        }
    }

    let mut spheres = BTreeMap::new();
    for (canonical, aliases) in [
        ("correspondence", ["correspondence", "Correspondence", ""]),
        ("entropy", ["entropy", "Entropy", ""]),
        ("forces", ["forces", "Forces", "Force"]),
        ("life", ["life", "Life", ""]),
        ("matter", ["matter", "Matter", ""]),
        ("mind", ["mind", "Mind", ""]),
        ("prime", ["prime", "Prime", ""]),
        ("spirit", ["spirit", "Spirit", ""]),
        ("time", ["time", "Time", ""]),
    ] {
        let aliases = aliases.into_iter().filter(|alias| !alias.is_empty()).collect::<Vec<_>>();
        if let Some(value) = number_field(obj, &aliases) {
            spheres.insert(canonical.to_owned(), value);
        } else {
            missing.push(canonical.to_owned());
        }
    }

    Ok(NormalizedStats {
        arete,
        willpower,
        quintessence,
        paradox,
        spheres,
        missing,
    })
}

async fn stats(args: StatsArgs) -> miette::Result<ExitCode> {
    let client = BasicClient::unauthenticated();
    let (repo, pds) = resolve_actor(&client, args.actor).await?;
    let pds_uri = jacquard_common::deps::fluent_uri::Uri::parse(pds.clone())
        .map_err(|_| miette::miette!("resolved PDS endpoint is not a valid URI: {pds}"))?
        .to_owned();
    client.set_base_uri(pds_uri).await;

    let rkeys = match args.rkey {
        Some(rkey) => vec![rkey],
        None => vec!["mage".to_owned(), "self".to_owned()],
    };

    for rkey in rkeys {
        let Some(record) = get_stats_record(&client, repo.clone(), &rkey).await? else {
            continue;
        };
        let value = serde_json::to_value(&record.value).into_diagnostic()?;
        let Some((raw, shape)) = extract_mage(&value, &rkey) else {
            continue;
        };
        let output = StatsOutput {
            source: StatsSource {
                uri: record.uri.as_str().to_owned(),
                cid: record.cid.map(|cid| cid.as_str().to_owned()),
                rkey,
                shape,
            },
            stats: normalize_stats(&raw)?,
            raw,
        };

        if args.json {
            println!("{}", serde_json::to_string_pretty(&output).into_diagnostic()?);
        } else {
            println!("Mage stats");
            println!("  source: {}", output.source.uri);
            println!("  shape:  {}", output.source.shape);
            println!("  arete:        {}", display_opt(output.stats.arete));
            println!("  willpower:    {}", display_opt(output.stats.willpower));
            println!("  quintessence: {}", display_opt(output.stats.quintessence));
            println!("  paradox:      {}", display_opt(output.stats.paradox));
            println!("  spheres:");
            for (sphere, value) in output.stats.spheres {
                println!("    {sphere}: {value}");
            }
            if !output.stats.missing.is_empty() {
                println!("  missing: {}", output.stats.missing.join(", "));
            }
        }
        return Ok(ExitCode::SUCCESS);
    }

    miette::bail!("no Mage stats found in actor.rpg.stats/mage or self.mage")
}

fn display_opt(value: Option<i64>) -> String {
    value.map(|value| value.to_string()).unwrap_or_else(|| "absent".to_owned())
}
