//! Throwaway spike (tass-auth-spike): proves the jac-store-fjall + jacquard 0.12
//! stack composes end to end before any real plumbing.
//!
//! Opens a fjall-backed AppPasswordStore, wraps a jacquard CredentialSession
//! around it, and calls `resume()` against an empty store — expecting
//! `LoginRequired`. No network: `Any` hint on an empty store returns before the
//! resolver is consulted.
//!
//! Run with: `cargo run -p tassle-cli --example auth_spike --features auth-store`

use std::sync::Arc;

use jac_store_fjall::FjallAuth;
use jacquard::client::credential_session::{CredentialResumeResult, CredentialSession};
use jacquard::common::session::SessionHint;
use jacquard::identity::JacquardResolver;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // A throwaway temp dir so repeated runs are clean.
    let dir = tempfile::tempdir()?;
    println!("auth_spike: store at {}", dir.path().display());

    // 1. fjall-backed app-password store (default Cbor codec). Implements both
    //    SessionStore<SessionKey, AtpSession> and SessionSelector<CredentialSessionMatch>.
    let auth = FjallAuth::open(dir.path())?;
    let store = Arc::new(auth.app_password());

    // 2. Identity resolver / HTTP client (constructed but not yet used — resume on
    //    an empty store with an Any hint never reaches the resolver).
    let resolver = Arc::new(JacquardResolver::default());

    // 3. CredentialSession wires the store + resolver.
    let session = CredentialSession::new(store, resolver);

    // 4. resume() returns Result<CredentialResumeResult, _>; LoginRequired is a
    //    *value*, not an error.
    let hint = SessionHint::any();
    match session.resume(&hint).await? {
        CredentialResumeResult::LoginRequired(challenge) => {
            println!(
                "auth_spike: LoginRequired (empty store) ✓  identifier={:?} session_id={:?}",
                challenge.identifier, challenge.session_id
            );
        }
        CredentialResumeResult::Resumed(session) => {
            println!(
                "auth_spike: Resumed (unexpected on an empty store!) did={} handle={}",
                session.did, session.handle
            );
        }
    }

    Ok(())
}
