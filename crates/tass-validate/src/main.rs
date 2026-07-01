// tass-validate: validate a JSON record against a tass lexicon.
//
// Reads JSON from stdin, validates against the named NSID, exits 0/1.
//
//   echo '{"$type":"at.telluri.node","name":"X","rating":3,...}' \
//     | tass-validate at.telluri.node
//
// On success: silent (exit 0), or `--verbose` prints `{"valid":true,...}`.
// On failure: structured errors as JSON to stderr (exit 1).
//
// Scope: SCHEMA validation only — type correctness, required fields present,
// constraint ranges/lengths, ref resolution. Does NOT check cross-record
// invariants (e.g. "node must exist before tassilize") or business rules
// (e.g. "quintessence can't exceed node.rating * 5"). Those belong in a
// higher-level validator.

use clap::Parser;
use jacquard_common::types::value::Data;
use jacquard_common::DefaultStr;
use jacquard_lexicon::lexicon::LexiconDoc;
use jacquard_lexicon::schema::{LexiconSchema, LexiconSchemaRef};
use jacquard_lexicon::validation::SchemaValidator;
use std::io::Read;
use tass_lexicons::at_telluri::{
    act::{enervate::Enervate, meditate::Meditate, tassilize::Tassilize},
    node::Node, resonance::Resonance,
};

// Manually submit each generated type to inventory. The codegen emits a
// manual `impl LexiconSchema` (not the derive), so the auto-registration
// that `#[derive(LexiconSchema)]` would do is missing. These submissions
// restore it — without them, SchemaValidator::global() reports every NSID
// as UnresolvedRef.
//
// If/when jacquard-codegen emits the derive (or the inventory::submit
// calls), this block can be deleted.

fn node_doc() -> LexiconDoc<'static> { Node::<DefaultStr>::lexicon_doc() }
fn tassilize_doc() -> LexiconDoc<'static> { Tassilize::<DefaultStr>::lexicon_doc() }
fn meditate_doc() -> LexiconDoc<'static> { Meditate::<DefaultStr>::lexicon_doc() }
fn enervate_doc() -> LexiconDoc<'static> { Enervate::<DefaultStr>::lexicon_doc() }
fn resonance_doc() -> LexiconDoc<'static> { Resonance::<DefaultStr>::lexicon_doc() }

inventory::submit! { LexiconSchemaRef { nsid: "at.telluri.node", def_name: "main", provider: node_doc } }
inventory::submit! { LexiconSchemaRef { nsid: "at.telluri.act.tassilize", def_name: "main", provider: tassilize_doc } }
inventory::submit! { LexiconSchemaRef { nsid: "at.telluri.act.meditate", def_name: "main", provider: meditate_doc } }
inventory::submit! { LexiconSchemaRef { nsid: "at.telluri.act.enervate", def_name: "main", provider: enervate_doc } }
inventory::submit! { LexiconSchemaRef { nsid: "at.telluri.resonance", def_name: "main", provider: resonance_doc } }

#[derive(Parser, Debug)]
#[command(
    name = "tass-validate",
    version,
    about = "Validate a JSON record against a tassle lexicon (stdin → exit code)"
)]
struct Args {
    /// NSID of the lexicon's main record def (e.g. at.telluri.node)
    nsid: String,

    /// Pretty-print the result JSON even on success
    #[arg(long)]
    verbose: bool,
}

#[derive(serde::Serialize)]
struct Outcome {
    valid: bool,
    nsid: String,
    structural_errors: Vec<String>,
    constraint_errors: Vec<String>,
}

fn main() -> miette::Result<()> {
    let args = Args::parse();

    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| miette::miette!("failed to read stdin: {e}"))?;

    let data: Data<DefaultStr> = serde_json::from_str(&input)
        .map_err(|e| miette::miette!("failed to parse JSON: {e}"))?;

    let result = SchemaValidator::global().validate_by_nsid(&args.nsid, &data);

    let outcome = Outcome {
        valid: result.is_valid(),
        nsid: args.nsid.clone(),
        structural_errors: result
            .structural_errors()
            .iter()
            .map(|e| e.to_string())
            .collect(),
        constraint_errors: result
            .constraint_errors()
            .iter()
            .map(|e| e.to_string())
            .collect(),
    };

    let json = serde_json::to_string_pretty(&outcome).unwrap();
    if outcome.valid {
        if args.verbose {
            println!("{json}");
        }
        return Ok(());
    }

    eprintln!("{json}");
    std::process::exit(1);
}
