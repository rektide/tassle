// Command modules. Each file exposes `Args` (clap struct) and `run(args)`.

pub mod mint;

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

/// Output format for record-producing commands.
#[derive(Debug, Clone, Copy, ValueEnum, Serialize, Deserialize, PartialEq, Eq)]
pub enum OutputFormat {
    /// Pretty-printed JSON to stdout (default)
    Json,
    /// DAG-CBOR bytes (atproto wire format). Deferred.
    Cbor,
}
