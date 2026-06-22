// `tassle samples` — generate example records into samples/ using the
// Rust fluent builders. Mirrors the TS samples command but uses the
// schema-validated generated types.
//
// Records use a fixed createdAt so diffs are stable across regenerations.

use clap::Args;
use jacquard_common::types::aturi::AtUri;
use jacquard_common::types::datetime::Datetime;
use jacquard_common::DefaultStr;
use miette::IntoDiagnostic;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use tassle_lexicons::com_superbfowle::tass::{
	enervate::Enervate, meditate::Meditate, node::Node, tassilize::Tassilize,
};

/// Placeholder DID for sample at-uris (not a real publisher).
const SAMPLE_DID: &str = "did:plc:samplesamplesamplesample";

/// Fixed timestamp so diffs are stable. Regenerated on every run.
const SAMPLE_AT: &str = "2026-06-21T12:00:00.000Z";

fn sample_at() -> Datetime {
    use chrono::DateTime;
    DateTime::parse_from_rfc3339(SAMPLE_AT).unwrap().into()
}

fn sample_uri(collection: &str, rkey: &str) -> String {
    format!("at://{SAMPLE_DID}/{collection}/{rkey}")
}

#[derive(Args, Debug)]
pub struct SamplesArgs {
    /// Output directory (default: ./samples from CWD or repo root)
    #[arg(short, long, default_value = "../../samples")]
    pub out: PathBuf,

    /// Print sample filenames without writing
    #[arg(long)]
    pub dry_run: bool,
}

/// Definition of one sample file.
struct SampleDef {
    filename: &'static str,
    description: &'static str,
    record: serde_json::Value,
}

fn build_samples() -> miette::Result<Vec<SampleDef>> {
    let node_rkey = "3ksamplesample1";
    let tass_rkey = "3ksamplesample2";
    let node_uri: AtUri = sample_uri("com.superbfowle.tass.node", node_rkey).parse().into_diagnostic()?;
    let tass_uri: AtUri = sample_uri("com.superbfowle.tass.tassilize", tass_rkey).parse().into_diagnostic()?;

    let node = Node::<DefaultStr>::builder()
        .name("Crystal Spring")
        .rating(3)
        .created_at(sample_at())
        .maybe_description(Some(
            "A clear spring deep in the old forest; the water hums faintly to those who can hear.".into(),
        ))
        .maybe_location(Some("Old-growth forest, three miles north of the caern".into()))
        .maybe_resonance(Some("dynamic".into()))
        .maybe_tass_form(Some("a smooth river-stone, warm to the touch".into()))
        .build();

    let tassilize = Tassilize::<DefaultStr>::builder()
        .node(node_uri.clone())
        .quintessence(5)
        .created_at(sample_at())
        .maybe_form(Some("a silver coin, untarnished".into()))
        .maybe_note(Some("Pulled from the spring's surface at dawn.".into()))
        .build();

    let meditate = Meditate::<DefaultStr>::builder()
        .node(node_uri.clone())
        .amount(3)
        .created_at(sample_at())
        .build();

    let enervate = Enervate::<DefaultStr>::builder()
        .tass(tass_uri.clone())
        .amount(2)
        .created_at(sample_at())
        .maybe_purpose(Some(
            "Lock the door behind us — looks like it was just unlocked all along.".into(),
        ))
        .build();

    Ok(vec![
        SampleDef {
            filename: "node-crystal-spring.example.json",
            description: "A rating-3 Node with dynamic resonance and a default ambient pool (15q).",
            record: serde_json::to_value(&node).into_diagnostic()?,
        },
        SampleDef {
            filename: "tassilize-silver-coin.example.json",
            description: "Genesis record: 5q crystallized at the Crystal Spring node as a silver coin.",
            record: serde_json::to_value(&tassilize).into_diagnostic()?,
        },
        SampleDef {
            filename: "meditate-dawn-pull.example.json",
            description: "Meditating at the Crystal Spring, drawing 3q into the mage's pattern.",
            record: serde_json::to_value(&meditate).into_diagnostic()?,
        },
        SampleDef {
            filename: "enervate-spend.example.json",
            description: "Spending 2q from the silver-coin tass to fuel a coincidence.",
            record: serde_json::to_value(&enervate).into_diagnostic()?,
        },
    ])
}

pub fn run(args: SamplesArgs) -> miette::Result<ExitCode> {
    let samples = build_samples()?;

    if args.dry_run {
        for s in &samples {
            println!("  {}", s.filename);
            println!("    {}", s.description);
        }
        return Ok(ExitCode::SUCCESS);
    }

    fs::create_dir_all(&args.out).into_diagnostic()?;

    for s in &samples {
        let path = args.out.join(s.filename);
        let mut file = fs::File::create(&path).into_diagnostic()?;
        let json = serde_json::to_string_pretty(&s.record).into_diagnostic()?;
        writeln!(file, "{json}").into_diagnostic()?;
        eprintln!("  wrote {}", path.display());
    }

    eprintln!("✓ {} samples written to {}", samples.len(), args.out.display());
    Ok(ExitCode::SUCCESS)
}
