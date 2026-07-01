//! `tass-engine`: the non-domain **mechanism** of the listener daemon.
//!
//! It owns the plumbing and nothing domain-specific:
//!
//! - [`EventSource`] — where hydrated events come from (Spacedust+hydration,
//!   jetstream, a test vec). Source-agnostic.
//! - [`Dispatcher`] — routes each [`Event`] to the first matching [`Command`]
//!   by keyword spotting.
//! - [`run`] — pulls events, dispatches them onto a [`tass_phase::Executor`]
//!   (concurrent, single-task), and emits one **wide-event** tracing line per
//!   handled command.
//! - [`Effect`] — the reusable effect vocabulary verbs compose from (a *menu,
//!   not a mandate*).
//!
//! The **verbs themselves live elsewhere** — `tass-act-enervate`,
//! `tass-act-meditate` — each implementing [`Command`] with its own
//! `tass-phase` FSM + Driver. The engine never mentions enervate or meditate.
//! See `doc/discovery/spacedust.md`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Instant;

use tass_phase::Executor;
use tracing::Instrument;

/// A hydrated inbound message the engine dispatches on.
///
/// The pointer→body hydration (Slingshot / direct PDS) happens in the source
/// adapter *before* dispatch, because keyword spotting needs `text`.
#[derive(Debug, Clone)]
pub struct Event {
    /// DID of the account that authored the referring record (the poster).
    pub actor_did: String,
    /// AT-URI of the referring record (the post).
    pub source_record: String,
    /// The referring record's rev.
    pub source_rev: String,
    /// The link target — carries the account we watch.
    pub subject: String,
    /// Collection of the referring record (e.g. `app.bsky.feed.post`).
    pub collection: String,
    /// Hydrated human text — what keyword spotting reads.
    pub text: String,
}

/// The reusable effect vocabulary a verb's FSM composes from — a menu, not a
/// mandate. Verbs (in `tass-act-*`) emit these; their `Driver` performs them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    /// Fetch actor context (the actor's mage characters, available tass).
    Gather,
    /// Pick the character/tass from the message (matchers over gathered context).
    ResolveTarget,
    /// Check the actor is allowed to act (e.g. own-tass-only).
    Authorize,
    /// Read current derived state (a tass balance, a node pool).
    ReadState,
    /// Write the action record (the domain verb).
    WriteEffect,
    /// Write the attestation receipt.
    Attest,
    /// Post a public reply.
    Reply,
}

/// What handling a command reported — the terminal value summarized in the
/// wide event.
#[derive(Debug, Clone)]
pub enum Outcome {
    /// Effects were performed (writes enabled).
    Acted,
    /// Matched and resolved, but no writes (reads-on / writes-off dry run).
    DryRun,
    /// Nothing to act on.
    Skipped(&'static str),
    /// The actor was not authorized.
    Denied(&'static str),
    /// Something went wrong.
    Failed(String),
}

/// A handled-command future. `'static` and un-`Send` — it may borrow a lent
/// session for the run, matching [`tass_phase::Executor`]'s single-task model.
pub type Handled = Pin<Box<dyn Future<Output = Outcome>>>;

/// A verb the engine can dispatch. Implemented in `tass-act-*` crates, one per
/// verb, each wrapping its own `tass-phase` FSM.
pub trait Command {
    /// Stable short name for config keys and the wide event (e.g. `"enervate"`).
    fn name(&self) -> &str;
    /// Keyword spotting: does this event look like this verb's command?
    fn matches(&self, event: &Event) -> bool;
    /// Handle a matched event, producing an [`Outcome`].
    fn handle(&self, event: Event) -> Handled;
}

/// Routes events to the first matching [`Command`].
#[derive(Default, Clone)]
pub struct Dispatcher {
    commands: Vec<Arc<dyn Command>>,
}

impl Dispatcher {
    /// An empty dispatcher.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a command. First-registered wins on a tie.
    pub fn register(&mut self, command: Arc<dyn Command>) -> &mut Self {
        self.commands.push(command);
        self
    }

    /// The first command whose matcher fires for `event`, if any.
    pub fn route(&self, event: &Event) -> Option<Arc<dyn Command>> {
        self.commands.iter().find(|c| c.matches(event)).cloned()
    }
}

/// Fetches a record body (as JSON) by AT-URI — the pointer→body step for
/// sources that yield pointers (Spacedust). Impls: Slingshot now
/// (`tass-slingshot`), direct-PDS later (`tass-hydrate-pds`).
pub trait Hydrator {
    /// Error yielded when a record can't be fetched.
    type Error: std::fmt::Display;
    /// Fetch the record at `at_uri` as JSON.
    fn hydrate(
        &self,
        at_uri: &str,
    ) -> impl Future<Output = Result<serde_json::Value, Self::Error>>;
}

/// Split `at://<did>/<collection>/<rkey>` into `(did, collection, rkey)`.
/// Returns `None` if any segment is missing or empty.
pub fn parse_at_uri(at_uri: &str) -> Option<(&str, &str, &str)> {
    let rest = at_uri.strip_prefix("at://")?;
    let mut parts = rest.splitn(3, '/');
    let (did, collection, rkey) = (parts.next()?, parts.next()?, parts.next()?);
    if did.is_empty() || collection.is_empty() || rkey.is_empty() {
        return None;
    }
    Some((did, collection, rkey))
}

/// A source of hydrated events. Source-agnostic: a Spacedust+hydration adapter,
/// a jetstream consumer, or a test vec all implement this.
pub trait EventSource {
    /// Error yielded while reading the stream.
    type Error: std::fmt::Display;
    /// The next event, `None` when the stream ends.
    fn next(&mut self) -> impl Future<Output = Option<Result<Event, Self::Error>>>;
}

/// Run the listener: pull events from `source`, dispatch each to its matching
/// command on a concurrent [`Executor`], and emit one wide-event line per
/// handled command. Returns when the source ends and in-flight work drains.
pub async fn run<S: EventSource>(mut source: S, dispatcher: Dispatcher) {
    let mut exec: Executor<'_, ()> = Executor::new();
    loop {
        tokio::select! {
            biased;
            // Drain completions so spawned jobs make progress. Guarded so we
            // never poll an empty executor (which would return immediately).
            Some(()) = exec.next(), if !exec.is_empty() => {}
            incoming = source.next() => match incoming {
                Some(Ok(event)) => match dispatcher.route(&event) {
                    Some(command) => exec.spawn(dispatch_one(command, event)),
                    None => tracing::debug!(
                        source_record = %event.source_record,
                        "no command matched",
                    ),
                },
                Some(Err(e)) => tracing::warn!(error = %e, "source error"),
                None => break,
            }
        }
    }
    // Source ended; let in-flight commands finish.
    while exec.next().await.is_some() {}
}

/// Wrap a command's handling in a per-command span and emit the wide event when
/// it finishes. The returned future yields `()`; the outcome is captured into
/// the wide-event line.
fn dispatch_one(command: Arc<dyn Command>, event: Event) -> Pin<Box<dyn Future<Output = ()>>> {
    let start = Instant::now();
    let span = tracing::info_span!(
        "command",
        command = command.name(),
        actor = %event.actor_did,
        source_record = %event.source_record,
        subject = %event.subject,
    );
    let handled = command.handle(event);
    Box::pin(
        async move {
            let outcome = handled.await;
            // The wide event: one INFO line summarizing the event + our action.
            tracing::info!(
                ?outcome,
                latency_ms = start.elapsed().as_millis() as u64,
                "handled",
            );
        }
        .instrument(span),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn ev(text: &str) -> Event {
        Event {
            actor_did: "did:plc:them".into(),
            source_record: "at://did:plc:them/app.bsky.feed.post/3l".into(),
            source_rev: "rev".into(),
            subject: "at://did:plc:mage/app.bsky.feed.post/3k".into(),
            collection: "app.bsky.feed.post".into(),
            text: text.into(),
        }
    }

    struct VecSource(VecDeque<Event>);
    impl EventSource for VecSource {
        type Error = Infallible;
        async fn next(&mut self) -> Option<Result<Event, Infallible>> {
            self.0.pop_front().map(Ok)
        }
    }

    /// A stub verb: matches "burn ... tass", counts calls, always dry-runs.
    struct BurnStub(Arc<AtomicUsize>);
    impl Command for BurnStub {
        fn name(&self) -> &str {
            "enervate"
        }
        fn matches(&self, event: &Event) -> bool {
            let t = event.text.to_lowercase();
            t.contains("burn") && t.contains("tass")
        }
        fn handle(&self, _event: Event) -> Handled {
            let count = self.0.clone();
            Box::pin(async move {
                count.fetch_add(1, Ordering::SeqCst);
                Outcome::DryRun
            })
        }
    }

    #[test]
    fn parse_at_uri_splits_or_rejects() {
        assert_eq!(
            parse_at_uri("at://did:plc:x/app.bsky.feed.post/3k"),
            Some(("did:plc:x", "app.bsky.feed.post", "3k"))
        );
        assert_eq!(parse_at_uri("at://did:plc:x/app.bsky.feed.post"), None);
        assert_eq!(parse_at_uri("https://example.com"), None);
    }

    #[test]
    fn dispatcher_routes_by_keyword() {
        let mut d = Dispatcher::new();
        d.register(Arc::new(BurnStub(Arc::new(AtomicUsize::new(0)))));
        assert!(d.route(&ev("please burn my tass")).is_some());
        assert!(d.route(&ev("just saying hi")).is_none());
    }

    #[tokio::test]
    async fn run_handles_only_matching_events() {
        let hits = Arc::new(AtomicUsize::new(0));
        let mut d = Dispatcher::new();
        d.register(Arc::new(BurnStub(hits.clone())));

        let source = VecSource(VecDeque::from([
            ev("burn my tass"),
            ev("hello there"),
            ev("BURN the TASS please"),
        ]));

        run(source, d).await;
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }
}
