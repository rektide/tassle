//! The standalone `tass-listen` daemon — a thin shell over [`tass_listen::run`].
//! The same [`tass_listen::ListenArgs`] + `run` compose into `tass-cli` as
//! `tassle listen`, so this binary and the omni-CLI share one implementation.

use clap::Parser;
use tracing_subscriber::EnvFilter;

#[derive(Parser, Debug)]
#[command(name = "tass-listen", about = "Listen for tass commands posted at an account")]
struct Cli {
    #[command(flatten)]
    listen: tass_listen::ListenArgs,
    /// Config profile to use (overrides TASSLE_PROFILE and the config selector).
    #[arg(long, global = true)]
    profile: Option<String>,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    // Default to info, with the engine's per-event debug line visible so the
    // read-only tail shows something before verbs are registered.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,tass_engine=debug")),
        )
        .init();

    let cli = Cli::parse();
    tass_listen::run(cli.listen, cli.profile.as_deref()).await
}
