// `tassle gen node` — construct and validate a Node record.

use clap::Args;
use jacquard_common::DefaultStr;
use jacquard_common::types::datetime::Datetime;
use jacquard_lexicon::schema::LexiconSchema;
use std::process::ExitCode;
use tass_lex::at_telluri::node::Node;

#[derive(Args, Debug)]
pub struct NodeArgs {
    /// Display name for the Node
    pub name: String,

    /// Node rating 1-5 (determines max ambient quintessence)
    #[arg(short, long)]
    pub rating: i64,

    /// Freeform description
    #[arg(short, long)]
    pub description: Option<String>,

    /// Where in the world this Node sits
    #[arg(short, long)]
    pub location: Option<String>,

    /// Resonance type (dynamic, static, primordial, pattern, questing)
    #[arg(short = 'R', long)]
    pub resonance: Option<String>,

    /// Coincidental form tass takes at this Node
    #[arg(short, long)]
    pub tass_form: Option<String>,

    /// Override default ambient quintessence (rating * 5)
    #[arg(short, long)]
    pub ambient_quintessence: Option<i64>,

    /// Skip schema validation before output
    #[arg(long)]
    pub no_validate: bool,
}

pub fn run(args: NodeArgs, format: crate::commands::OutputFormat) -> miette::Result<ExitCode> {
    let mut builder = Node::<DefaultStr>::builder()
        .name(args.name.clone())
        .rating(args.rating)
        .created_at(Datetime::from(chrono::Utc::now().fixed_offset()));

    if let Some(d) = args.description {
        builder = builder.maybe_description(Some(d.into()));
    }
    if let Some(l) = args.location {
        builder = builder.maybe_location(Some(l.into()));
    }
    if let Some(r) = args.resonance {
        builder = builder.maybe_resonance(Some(r.into()));
    }
    if let Some(f) = args.tass_form {
        builder = builder.maybe_tass_form(Some(f.into()));
    }
    if let Some(q) = args.ambient_quintessence {
        builder = builder.ambient_quintessence(q);
    }

    let node = builder.build();

    if !args.no_validate {
        if let Err(err) = node.validate() {
            eprintln!("validation failed:");
            eprintln!("  {err}");
            return Ok(ExitCode::FAILURE);
        }
    }

    crate::commands::emit(&node, format)
}
