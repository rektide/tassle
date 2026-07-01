// xtask: tassle workspace task runner.
//
//   cargo xtask codegen   — regenerate crates/tass-lex/src from tass-lex-schema
//   cargo xtask samples   — regenerate crates/tass-lex-sample/samples from the builders

use clap::{Parser, Subcommand};
use jacquard_common::DefaultStr;
use jacquard_common::types::aturi::AtUri;
use jacquard_common::types::datetime::Datetime;
use miette::IntoDiagnostic;
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use tass_lex::at_telluri::{
    act::{enervate::Enervate, meditate::Meditate, tassilize::Tassilize}, node::Node,
};

#[derive(Parser)]
#[command(name = "xtask", about = "tassle workspace task runner")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Regenerate Rust lexicon types into crates/tass-lex/src
    Codegen {
        #[arg(short, long, default_value = "crates/tass-lex-schema/lexicons")]
        input: PathBuf,
        #[arg(short, long, default_value = "crates/tass-lex/src")]
        output: PathBuf,
    },
    /// Regenerate example records into crates/tass-lex-sample/samples
    Samples {
        #[arg(short, long, default_value = "crates/tass-lex-sample/samples")]
        out: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
}

fn main() -> miette::Result<ExitCode> {
    match Cli::parse().cmd {
        Cmd::Codegen { input, output } => run_codegen(&input, &output),
        Cmd::Samples { out, dry_run } => run_samples(&out, dry_run),
    }
}

fn run_codegen(input: &std::path::Path, output: &std::path::Path) -> miette::Result<ExitCode> {
    use jacquard_lexicon::codegen::{CodeGenerator, CodegenMode};
    use jacquard_lexicon::corpus::LexiconCorpus;

    eprintln!("Loading lexicons from {}...", input.display());
    let corpus = LexiconCorpus::load_from_dir(input)?;
    let count = corpus.iter().count();
    eprintln!("Loaded {count} lexicon documents");

    eprintln!("Generating code...");
    let codegen = CodeGenerator::with_mode(&corpus, "crate".to_string(), CodegenMode::Pretty);
    codegen.write_to_disk(output)?;
    eprintln!("Generated code to {}", output.display());
    Ok(ExitCode::SUCCESS)
}

const SAMPLE_DID: &str = "did:plc:samplesamplesamplesample";
const SAMPLE_AT: &str = "2026-06-21T12:00:00.000Z";

fn sample_at() -> Datetime {
    use chrono::DateTime;
    DateTime::parse_from_rfc3339(SAMPLE_AT).unwrap().into()
}

fn sample_uri(collection: &str, rkey: &str) -> String {
    format!("at://{SAMPLE_DID}/{collection}/{rkey}")
}

struct SampleDef {
    filename: &'static str,
    description: &'static str,
    record: serde_json::Value,
}

fn build_samples() -> miette::Result<Vec<SampleDef>> {
    let node_rkey = "3ksamplesample1";
    let tass_rkey = "3ksamplesample2";
    let node_uri: AtUri = sample_uri("at.telluri.node", node_rkey)
        .parse()
        .into_diagnostic()?;
    let tass_uri: AtUri = sample_uri("at.telluri.act.tassilize", tass_rkey)
        .parse()
        .into_diagnostic()?;

    let node = Node::<DefaultStr>::builder()
        .name("Crystal Spring")
        .rating(3)
        .created_at(sample_at())
        .maybe_description(Some(
            "A clear spring deep in the old forest; the water hums faintly to those who can hear."
                .into(),
        ))
        .maybe_location(Some(
            "Old-growth forest, three miles north of the caern".into(),
        ))
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

fn run_samples(out: &std::path::Path, dry_run: bool) -> miette::Result<ExitCode> {
    let samples = build_samples()?;

    if dry_run {
        for s in &samples {
            println!("  {}", s.filename);
            println!("    {}", s.description);
        }
        return Ok(ExitCode::SUCCESS);
    }

    fs::create_dir_all(out).into_diagnostic()?;
    for s in &samples {
        let path = out.join(s.filename);
        let mut file = fs::File::create(&path).into_diagnostic()?;
        let json = serde_json::to_string_pretty(&s.record).into_diagnostic()?;
        writeln!(file, "{json}").into_diagnostic()?;
        eprintln!("  wrote {}", path.display());
    }
    eprintln!("✓ {} samples written to {}", samples.len(), out.display());
    Ok(ExitCode::SUCCESS)
}
