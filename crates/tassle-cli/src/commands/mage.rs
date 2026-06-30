use crate::profile_config;
use clap::{Args, Subcommand};
use jacquard::client::BasicClient;
use jacquard::identity::resolver::IdentityResolver;
use jacquard_common::types::ident::AtIdentifier;
use jacquard_common::types::string::{Nsid, RecordKey};
use jacquard_common::xrpc::atproto::{GetRecord, GetRecordError, GetRecordOutput, ListRecords};
use jacquard_common::xrpc::{XrpcClient, XrpcError};
use miette::IntoDiagnostic;
use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
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
    mage: Option<NormalizedMageStats>,
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

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct NormalizedMageStats {
    arete: Option<i64>,
    willpower: Option<i64>,
    willpower_temporary: Option<i64>,
    /// Player-facing whole points — always the floor of the `quint` millis
    /// (resolved via `tass_quint`). Derived, not the raw sheet field.
    quintessence: Option<i64>,
    /// Raw Tassle extension field (`quint`), in milli-quintessence. `None`
    /// when the sheet only carries the legacy `quintessence` integer.
    quint: Option<i64>,
    paradox: Option<i64>,
    spheres: BTreeMap<String, i64>,
    missing: Vec<String>,
}

pub async fn run(args: MageArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    match args.kind {
        MageKind::List(args) => list(args, format).await,
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
        collection: Nsid::new_static(STATS_COLLECTION).into_diagnostic()?,
        rkey: RecordKey::any_owned(rkey).into_diagnostic()?,
        cid: None,
    };
    let response = client
        .send(request)
        .await
        .map_err(|err| miette::miette!("getRecord {STATS_COLLECTION}/{rkey} failed: {err}"))?;
    match response.into_output() {
        Ok(output) => Ok(Some(output)),
        Err(XrpcError::Xrpc(GetRecordError::RecordNotFound(_))) => Ok(None),
        Err(err) => Err(miette::miette!(
            "failed to decode {STATS_COLLECTION}/{rkey}: {err}"
        )),
    }
}

async fn list_stats_records(
    client: &BasicClient,
    repo: AtIdentifier,
    args: &ListArgs,
) -> miette::Result<StatsListOutput> {
    if args.limit < 1 || args.limit > 100 {
        miette::bail!("--limit must be between 1 and 100");
    }
    let request = ListRecords {
        repo: repo.clone(),
        collection: Nsid::new_static(STATS_COLLECTION).into_diagnostic()?,
        cursor: args.cursor.clone().map(Into::into),
        limit: Some(args.limit),
        reverse: None,
    };
    let response = client
        .send(request)
        .await
        .map_err(|err| miette::miette!("listRecords {STATS_COLLECTION} failed: {err}"))?;
    let output = response
        .into_output()
        .map_err(|err| miette::miette!("failed to decode listRecords output: {err}"))?;
    let records = output
        .records
        .into_iter()
        .map(|record| {
            let value = serde_json::to_value(&record.value).into_diagnostic()?;
            Ok(summarize_record(
                record.uri.as_str(),
                record.cid.as_ref().map(|cid| cid.as_str()),
                &value,
            ))
        })
        .collect::<miette::Result<Vec<_>>>()?;

    Ok(StatsListOutput {
        repo: repo.as_str().to_owned(),
        collection: STATS_COLLECTION.to_owned(),
        pds: client.base_uri().await.to_string(),
        cursor: output.cursor.map(|cursor| cursor.to_string()),
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

fn extract_mage(value: &Value, rkey: &str) -> Option<Value> {
    let (payload, _, system) = stats_payload(value, rkey);
    match (payload, system.as_deref()) {
        (Some(payload), Some("mage")) => Some(payload),
        _ => object(value, "mage").map(|mage| Value::Object(mage.clone())),
    }
}

fn number_field(obj: &serde_json::Map<String, Value>, names: &[&str]) -> Option<i64> {
    names.iter().find_map(|name| obj.get(*name)?.as_i64())
}

fn willpower_field(obj: &serde_json::Map<String, Value>) -> Option<i64> {
    number_field(obj, &["willpower", "Willpower"]).or_else(|| {
        obj.get("willpower")?
            .as_object()?
            .get("permanent")?
            .as_i64()
    })
}

fn willpower_temporary_field(obj: &serde_json::Map<String, Value>) -> Option<i64> {
    obj.get("willpower")?
        .as_object()?
        .get("temporary")?
        .as_i64()
}

fn normalize_mage(raw: &Value) -> miette::Result<NormalizedMageStats> {
    let obj = raw
        .as_object()
        .ok_or_else(|| miette::miette!("mage stats payload is not an object"))?;
    let mut missing = Vec::new();
    let arete = number_field(obj, &["arete", "Arete"]);
    let willpower = willpower_field(obj);
    let willpower_temporary = willpower_temporary_field(obj);
    let quintessence_raw = number_field(obj, &["quintessence", "Quintessence"]);
    let quint_raw = number_field(obj, &["quint", "Quint"]);
    // quint is the source of truth when the Tassle extension field is present;
    // otherwise hydrate from the legacy integer. quintessence always shows the
    // rounded-down points. See the tass-quint crate.
    let resolved = tass_quint::resolve(quint_raw, quintessence_raw);
    let quintessence = resolved.map(|q| q.points());
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
        let aliases = aliases
            .into_iter()
            .filter(|alias| !alias.is_empty())
            .collect::<Vec<_>>();
        if let Some(value) = number_field(obj, &aliases) {
            spheres.insert(canonical.to_owned(), value);
        } else {
            missing.push(canonical.to_owned());
        }
    }

    Ok(NormalizedMageStats {
        arete,
        willpower,
        willpower_temporary,
        quintessence,
        quint: quint_raw,
        paradox,
        spheres,
        missing,
    })
}

async fn list(args: ListArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    let client = BasicClient::unauthenticated();
    let (repo, pds) = resolve_actor(&client, args.actor.clone()).await?;
    let pds_uri = jacquard_common::deps::fluent_uri::Uri::parse(pds.clone())
        .map_err(|_| miette::miette!("resolved PDS endpoint is not a valid URI: {pds}"))?
        .to_owned();
    client.set_base_uri(pds_uri).await;

    if args.all {
        let output = list_stats_records(&client, repo, &args).await?;
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
        let value = serde_json::to_value(&record.value).into_diagnostic()?;
        let summary = summarize_record(
            record.uri.as_str(),
            record.cid.as_ref().map(|cid| cid.as_str()),
            &value,
        );
        let raw = if rkey == "mage" || rkey == "self" {
            extract_mage(&value, &rkey).unwrap_or_else(|| value.clone())
        } else {
            stats_payload(&value, &rkey)
                .0
                .unwrap_or_else(|| value.clone())
        };
        let mage = if summary.system.as_deref() == Some("mage") || rkey == "self" {
            extract_mage(&value, &rkey)
                .map(|raw| normalize_mage(&raw))
                .transpose()?
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
        if let Some(millis) = mage.quint {
            println!("  quint:        {millis} millis");
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
