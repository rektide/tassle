// `tassle generate` — generate records.
//
// Each subcommand targets one record type. All use the fluent builders
// from tass-lex, validate in-process, and emit JSON or CBOR.

pub mod node;
pub mod node_item;

use clap::{Args, Subcommand};

#[derive(Args, Debug)]
pub struct GenerateArgs {
    #[command(subcommand)]
    pub kind: GenerateKind,
}

#[derive(Subcommand, Debug)]
pub enum GenerateKind {
    /// Generate a Node — a place where quintessence gathers
    Node(node::NodeArgs),
    /// Generate a Node as a mage's owned equipment (give + item pair)
    NodeItem(node_item::NodeItemArgs),
}
