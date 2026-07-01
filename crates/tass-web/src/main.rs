//! `tass-web` — the axum OAuth login service.
//!
//! Boots one [`tass_web_auth::AppState`] from the active figment profile's
//! `[service]` config + the shared turso `[store]`, mounts jacquard-axum's
//! OAuth routes + a login page, and serves. Everything OAuth (DPoP, PAR,
//! callback, session cookies) is jacquard-axum's; this binary is the
//! composition root. See `doc/axum.md`.

use std::net::SocketAddr;

use axum::{routing::get, Router};
use clap::Parser;
use jacquard::identity::PublicResolver;
use jacquard_axum::oauth::routes as oauth_routes;
use jac_stores::{OAuthStore, TursoRepository};
use tass_config::{config, dirs, service::service_config};
use tass_web_auth::{login, AppState, default_web_config};

#[derive(Parser, Debug)]
#[command(name = "tass-web", about = "tass web server: atproto OAuth login + app routes")]
struct Args {
    /// Figment profile to activate (else TASS_PROFILE / the config default).
    #[arg(long)]
    profile: Option<String>,
    /// Override the config root, verbatim (no appname appended).
    #[arg(long)]
    config_dir: Option<std::path::PathBuf>,
    /// Override the appname leaf under each XDG base.
    #[arg(long)]
    appname: Option<String>,
    /// Override the local listen address (else `[service].bind`).
    #[arg(long)]
    bind: Option<SocketAddr>,
    /// Service variant: reads `[service]` < `[service.<variant>]`.
    #[arg(long, default_value = "web")]
    service_variant: String,
}

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "tass_web=info,axum=info".into()),
        )
        .init();

    let args = Args::parse();
    dirs::set_overrides(dirs::Overrides {
        appname: args.appname,
        config_dir: args.config_dir,
        state_dir: None,
    });

    let figment = config::active_figment(args.profile.as_deref())?;
    let service = service_config(&figment, Some(&args.service_variant))?;

    // Open the shared turso [store] (the same DB the CLI uses).
    let profile = config::active_name(&figment);
    let db_path = config::resolve_store_path(&figment, &profile)?;
    config::precheck_store(&db_path, &config::store_lifecycle(&figment)?)?;
    let db_display = db_path.display().to_string();
    let repo = TursoRepository::open_local(db_path)
        .await
        .map_err(|e| miette::miette!("open store {db_display}: {e}"))?;
    let store = OAuthStore::new(repo);

    let web_config = default_web_config();
    let state = tass_web_auth::boot_prod(service.clone(), store, web_config.clone())?;

    let app = Router::new()
        .route("/", get(root))
        .route("/health", get(health))
        .route("/login", get(login::login_page))
        .merge(oauth_routes::<PublicResolver, OAuthStore<TursoRepository>, AppState>(
            &web_config,
        ))
        .with_state(state);

    let bind = args
        .bind
        .or(service.bind)
        .unwrap_or_else(|| "127.0.0.1:3000".parse().expect("valid fallback addr"));
    tracing::info!(
        %bind,
        public_url = ?service.public_url,
        confidential = service.oauth.is_confidential(),
        "tass-web listening"
    );
    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| miette::miette!("bind {bind}: {e}"))?;
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .map_err(|e| miette::miette!("server error: {e}"))?;
    Ok(())
}

async fn root() -> &'static str {
    "tass web — sign in at /login"
}

async fn health() -> &'static str {
    "ok"
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("failed to install CTRL+C signal handler");
}
