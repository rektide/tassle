//! Manual verification: tail Spacedust links aimed at a DID.
//!
//!   cargo run -p tass-spacedust --example tail -- did:plc:yourmage
//!
//! Prints one line per link event referencing the account. Read-only; this is
//! the slice-1 "does the stream work end-to-end" check.

use tass_spacedust::{SpacedustConfig, Subscriber};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let account = std::env::args()
        .nth(1)
        .expect("usage: tail <did-or-handle-resolved-to-did>");

    let cfg = SpacedustConfig::for_account(account);
    let mut sub = Subscriber::connect(&cfg).await?;
    eprintln!(
        "connected: watching {} at {}",
        cfg.account.as_deref().unwrap_or("(none)"),
        cfg.endpoint
    );

    while let Some(link) = sub.next_event().await? {
        println!(
            "{:<6} {:<45} {} -> {}",
            link.operation, link.source, link.source_record, link.subject
        );
    }
    eprintln!("stream closed");
    Ok(())
}
