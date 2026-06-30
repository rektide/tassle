// tassle-codegen: generate Rust types from Tass lexicons using jacquard.
//
// Mirrors jacquard-lexgen's `jacquard-codegen` binary but reads from this
// corpus crate's lexicons/ dir. Run from the repository root:
//
//     cargo run -p tassle-codegen -- --input crates/tass-lex-schema/lexicons --output crates/tassle-lexicons/src
//
// CI uses this to regenerate types and verify they match what's committed.

use clap::Parser;
use jacquard_lexicon::codegen::{CodeGenerator, CodegenMode};
use jacquard_lexicon::corpus::LexiconCorpus;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "tassle-codegen",
    version,
    about = "Generate Rust types from Tass lexicons"
)]
struct Args {
    /// Directory containing Lexicon JSON files (default: crates/tass-lex-schema/lexicons)
    #[arg(short = 'i', long, default_value = "crates/tass-lex-schema/lexicons")]
    input: PathBuf,

    /// Output directory for generated Rust code
    #[arg(short = 'o', long, default_value = "crates/tassle-lexicons/src")]
    output: PathBuf,

    /// Emit fully-qualified paths (for proc-macro consumers). Default is pretty mode.
    #[arg(long = "macro")]
    macro_mode: bool,
}

fn main() -> miette::Result<()> {
    let args = Args::parse();
    let mode = if args.macro_mode {
        CodegenMode::Macro
    } else {
        CodegenMode::Pretty
    };

    eprintln!("Loading lexicons from {}...", args.input.display());
    let corpus = LexiconCorpus::load_from_dir(&args.input)?;

    let count = corpus.iter().count();
    eprintln!("Loaded {count} lexicon documents");

    eprintln!("Generating code (mode: {mode:?})...");
    let codegen = CodeGenerator::with_mode(&corpus, "crate".to_string(), mode);
    codegen.write_to_disk(&args.output)?;

    eprintln!("Generated code to {}", args.output.display());
    Ok(())
}
