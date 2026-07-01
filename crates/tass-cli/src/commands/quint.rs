//! `tassle quint set|inc` — adjust mage pattern-quintessence (milliQuintessence)
//! on the active profile's `actor.rpg.stats/mage` sheet.
//!
//! Behind the `auth-store` feature: writes need an authenticated session, so
//! the whole command is gated. Build with `--features auth-store`.

use clap::{Args, Subcommand};
use std::process::ExitCode;
use tass_quint::Quint;
use tass_repo_mage::{QuintClient, WriteOpts};
use tass_config::AuthedClient;

use crate::commands::OutputFormat;

/// The mage record rkey these commands target.
const MAGE_RKEY: &str = "mage";

#[derive(Args, Debug)]
pub struct QuintArgs {
    #[command(subcommand)]
    pub kind: QuintKind,
}

#[derive(Subcommand, Debug)]
pub enum QuintKind {
    /// Set the mage pattern-quintessence to an absolute value (in points).
    Set(SetArgs),
    /// Adjust the mage pattern-quintessence by a signed delta (in points).
    /// Defaults to +1.0 when no delta is given.
    Inc(IncArgs),
}

#[derive(Args, Debug)]
pub struct SetArgs {
    /// Absolute value in whole points (e.g. `3.5` = 3500 milli-quintessence).
    pub points: f64,
    /// Stamp with this ISO-8601 time instead of "now".
    #[arg(long, value_name = "ISO-8601")]
    pub at: Option<String>,
    /// Don't stamp `milliQuintessenceUpdatedAt`.
    #[arg(long)]
    pub unstamped: bool,
}

#[derive(Args, Debug)]
pub struct IncArgs {
    /// Delta in whole points; signed (`1.5`, `-0.25`). Defaults to `+1.0`.
    pub points: Option<f64>,
    /// Stamp with this ISO-8601 time instead of "now".
    #[arg(long, value_name = "ISO-8601")]
    pub at: Option<String>,
    /// Don't stamp `milliQuintessenceUpdatedAt`.
    #[arg(long)]
    pub unstamped: bool,
}

pub async fn run(
    args: QuintArgs,
    _format: OutputFormat,
    profile: Option<&str>,
) -> miette::Result<ExitCode> {
    let authed = AuthedClient::for_profile(profile)
        .await
        .map_err(|e| miette::miette!("auth: {e}"))?;
    let did = authed
        .did()
        .ok_or_else(|| miette::miette!("active profile has no `did`"))?
        .to_owned();
    let qc = QuintClient::new(authed.session());

    match args.kind {
        QuintKind::Set(a) => {
            require_finite(a.points, "points")?;
            let q = Quint::from_points_f64(a.points);
            let applied = qc
                .write_with(&did, MAGE_RKEY, q, stamp_opts(a.at, a.unstamped))
                .await
                .map_err(|e| miette::miette!("quint set: {e}"))?;
            println!(
                "set {did}/mage -> {} points ({} milli{})",
                applied.points(),
                applied.millis(),
                range_note(applied)
            );
        }
        QuintKind::Inc(a) => {
            let delta = a.points.unwrap_or(1.0);
            require_finite(delta, "points")?;
            let delta_q = Quint::from_points_f64(delta);
            let applied = qc
                .adjust_with(&did, MAGE_RKEY, delta_q, stamp_opts(a.at, a.unstamped))
                .await
                .map_err(|e| miette::miette!("quint inc: {e}"))?;
            println!(
                "inc {did}/mage by {delta} -> {} points ({} milli{})",
                applied.points(),
                applied.millis(),
                range_note(applied)
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn stamp_opts(at: Option<String>, unstamped: bool) -> WriteOpts {
    let mut opts = WriteOpts::default();
    if unstamped {
        opts = opts.unstamped();
    } else if let Some(ts) = at {
        opts = opts.at(ts);
    }
    opts
}

fn require_finite(value: f64, name: &str) -> miette::Result<()> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(miette::miette!("`{name}` must be a finite number, got {value}"))
    }
}

fn range_note(q: Quint) -> String {
    if q.is_out_of_range() {
        format!("  ⚠ out of expected [0, {}] range", tass_quint::MAX_POINTS)
    } else {
        String::new()
    }
}
