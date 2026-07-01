use axum::{
    routing::get,
    Router,
};
use clap::Parser;
use figment2::{Figment, providers::Serialized};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[derive(Parser, Debug, Deserialize, Serialize)]
#[command(name = "tass-web", about = "Hedystia web server for tass")]
struct Args {
    #[arg(long, default_value = "0.0.0.0:3000")]
    bind: SocketAddr,
}

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    #[serde(default = "default_bind")]
    bind: SocketAddr,
}

fn default_bind() -> SocketAddr {
    SocketAddr::from(([0, 0, 0, 0], 3000))
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bind: default_bind(),
        }
    }
}

async fn health() -> &'static str {
    "ok"
}

async fn root() -> &'static str {
    "Hedystia — tass web server"
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| "tass_web=info,axum=info".into()))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let args = Args::parse();

    let config: Config = Figment::new()
        .merge(Serialized::defaults(Config::default()))
        .merge(Serialized::defaults(args))
        .extract()
        .map_err(|e| miette::miette!("config error: {}", e))?;

    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health));

    info!("listening on {}", config.bind);
    let listener = TcpListener::bind(config.bind).await
        .map_err(|e| miette::miette!("bind error: {}", e))?;

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| miette::miette!("server error: {}", e))?;

    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}
