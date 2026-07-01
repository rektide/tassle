// tass: the Rust CLI.

mod commands;
mod profile_config;

use clap::{Parser, Subcommand};
use commands::OutputFormat;
use std::process::ExitCode;

#[derive(Parser, Debug)]
#[command(
    name = "tass",
    version,
    about = "Tassle — Mage: The Ascension quintessence/tass energy ledger"
)]
struct Cli {
    /// Output format (global; accepted on every subcommand).
    #[arg(global = true, long, value_enum, default_value_t = OutputFormat::Table)]
    format: OutputFormat,

    /// Profile (login) to use for this invocation, overriding TASS_PROFILE and
    /// the config file's `profile` selector. Global; accepted on every
    /// subcommand (honoured by the figment-backed `auth`/`config` commands).
    #[arg(global = true, long)]
    profile: Option<String>,

    /// Config root for this invocation (precedence: this flag > TASS_CONFIG_DIR
    /// > XDG_CONFIG_HOME > ~/.config/<appname>). Global.
    #[arg(global = true, long, value_name = "DIR")]
    config_dir: Option<std::path::PathBuf>,

    /// App name for this invocation — retargets all config/state/cache dirs
    /// (precedence: this flag > TASS_APPNAME > "tass"). Handy for dev/test
    /// isolation. Global.
    #[arg(global = true, long, value_name = "NAME")]
    appname: Option<String>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Profile and authentication commands
    Auth(commands::auth::AuthArgs),
    /// figment-backed config: profiles, loaded sources, active values
    Config(commands::config::ConfigArgs),
    /// Generate records (node, tassilize, etc.) as JSON or CBOR
    #[command(alias = "gen")]
    Generate(commands::generate::GenerateArgs),
    /// Mage character sheet commands
    Mage(commands::mage::MageArgs),
    /// Set/adjust mage pattern-quintessence (milliQuintessence). (auth-store)
    #[cfg(feature = "auth-store")]
    Quint(commands::quint::QuintArgs),
    /// Read public repository records through Jacquard XRPC
    Repo(commands::repo::RepoArgs),
    /// Listen for tass commands posted at an account via Spacedust. (listen)
    #[cfg(feature = "listen")]
    Listen(tass_listen::ListenArgs),
}

#[tokio::main]
async fn main() -> miette::Result<ExitCode> {
    let cli = Cli::parse();

    // Install directory overrides before any config resolution touches dirs.
    tass_config::dirs::set_overrides(tass_config::dirs::Overrides {
        appname: cli.appname.clone(),
        config_dir: cli.config_dir.clone(),
        state_dir: None,
    });

    let format = cli.format;
    let profile = cli.profile.as_deref();
    match cli.command {
        Command::Auth(args) => commands::auth::run(args, format, profile).await,
        Command::Config(args) => commands::config::run(args, format, profile),
        Command::Generate(args) => match args.kind {
            commands::generate::GenerateKind::Node(a) => commands::generate::node::run(a, format),
            commands::generate::GenerateKind::NodeItem(a) => {
                commands::generate::node_item::run(a, format)
            }
        },
        Command::Mage(args) => commands::mage::run(args, format, profile).await,
        #[cfg(feature = "auth-store")]
        Command::Quint(args) => commands::quint::run(args, format, profile).await,
        Command::Repo(args) => commands::repo::run(args, format, profile).await,
        #[cfg(feature = "listen")]
        Command::Listen(args) => tass_listen::run(args, profile)
            .await
            .map(|()| ExitCode::SUCCESS),
    }
}
