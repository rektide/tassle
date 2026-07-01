//! A client for [Spacedust](https://spacedust.microcosm.blue/), microcosm's
//! configurable ATProto notifications firehose.
//!
//! This crate is deliberately small and **connection-oriented only**: it opens
//! the WebSocket, subscribes by `wantedSubjectDids` (the "posts at us" filter),
//! answers pings, and yields normalized [`LinkEvent`]s. Deciding *what to do*
//! with an event — dispatch, matching, action chains — is not this crate's job
//! (that's the engine). See `doc/discovery/spacedust.md`.
//!
//! The payload Spacedust delivers is a *pointer*, not a record body: a
//! [`LinkEvent`] carries the at-uri of the referring record (`source_record`)
//! and the target it linked to (`subject`). Reading the actual post text is a
//! later hydration step (Slingshot), also outside this crate.
//!
//! ```no_run
//! use tass_spacedust::{SpacedustConfig, Subscriber};
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let cfg = SpacedustConfig::for_account("did:plc:example");
//! let mut sub = Subscriber::connect(&cfg).await?;
//! while let Some(link) = sub.next_event().await? {
//!     println!("{} referenced {}", link.source_record, link.subject);
//! }
//! # Ok(()) }
//! ```

use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};
use url::Url;

/// The public microcosm Spacedust instance (confirmed host).
pub const DEFAULT_ENDPOINT: &str = "wss://spacedust.microcosm.blue/subscribe";

/// How to connect and what to subscribe to.
///
/// Field names deserialize from a `[service.listen]`-style config block; this
/// crate only reads them, it does not resolve the config cascade (that's
/// `tass-config`'s `extract_cascade`).
#[derive(Debug, Clone, Deserialize)]
pub struct SpacedustConfig {
    /// The `wss://…/subscribe` endpoint. Defaults to [`DEFAULT_ENDPOINT`].
    #[serde(default = "default_endpoint")]
    pub endpoint: String,
    /// The DID we listen for — becomes `wantedSubjectDids`. This is the account
    /// people address ("posts at us"). Optional at parse time so config may omit
    /// it and a CLI flag supply it; [`subscribe_url`](Self::subscribe_url) and
    /// [`Subscriber::connect`] require it to be set.
    #[serde(default)]
    pub account: Option<String>,
    /// Optional `wantedSources` narrowing (e.g. only post replies/mentions).
    /// AND-ed with the subject filter server-side; empty = all sources.
    #[serde(default)]
    pub wanted_sources: Vec<String>,
    /// Bypass Spacedust's 21s delay buffer. Default `false` — we want the
    /// delayed stream so a post-then-delete never fires.
    #[serde(default)]
    pub instant: bool,
}

fn default_endpoint() -> String {
    DEFAULT_ENDPOINT.to_string()
}

impl SpacedustConfig {
    /// A config for one account against the default public instance.
    pub fn for_account(account: impl Into<String>) -> Self {
        Self {
            endpoint: default_endpoint(),
            account: Some(account.into()),
            wanted_sources: Vec::new(),
            instant: false,
        }
    }

    /// Build the full subscribe URL with query parameters (DID percent-encoded).
    pub fn subscribe_url(&self) -> Result<String, SpacedustError> {
        let account = self.account.as_deref().ok_or(SpacedustError::NoAccount)?;
        let mut url =
            Url::parse(&self.endpoint).map_err(|e| SpacedustError::Url(e.to_string()))?;
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("wantedSubjectDids", account);
            for source in &self.wanted_sources {
                q.append_pair("wantedSources", source);
            }
            if self.instant {
                q.append_pair("instant", "true");
            }
        }
        Ok(url.into())
    }
}

/// One link event from Spacedust: a record (`source_record`) whose field
/// (`source`) links to a `subject` we're watching.
///
/// Mirrors Spacedust's `ClientLinkEvent`. The record body is *not* included —
/// hydrate `source_record` separately to read it.
#[derive(Debug, Clone, Deserialize)]
pub struct LinkEvent {
    /// `"create"` or `"delete"`.
    pub operation: String,
    /// Link source: `<collection NSID>:<dotted record path>`
    /// (e.g. `app.bsky.feed.post:reply.parent.uri`).
    pub source: String,
    /// AT-URI of the referring record (the post to hydrate).
    pub source_record: String,
    /// The referring record's rev.
    pub source_rev: String,
    /// The link target — an at-uri or DID carrying the account we watch.
    pub subject: String,
}

/// The wire envelope Spacedust sends. Currently always `kind = "link"`.
#[derive(Debug, Clone, Deserialize)]
struct ClientEvent {
    kind: String,
    #[serde(default)]
    #[allow(dead_code)]
    origin: String,
    link: LinkEvent,
}

/// Errors from connecting to or reading from Spacedust.
#[derive(Debug, thiserror::Error)]
pub enum SpacedustError {
    #[error("no account set: config [service.listen].account or --account is required")]
    NoAccount,
    #[error("invalid endpoint URL: {0}")]
    Url(String),
    #[error("websocket connect failed: {0}")]
    Connect(#[source] tokio_tungstenite::tungstenite::Error),
    #[error("websocket error: {0}")]
    Ws(#[source] tokio_tungstenite::tungstenite::Error),
    #[error("failed to decode event: {0}")]
    Decode(#[source] serde_json::Error),
    #[error("malformed source_record at-uri: {0}")]
    BadUri(String),
    #[error("hydration failed: {0}")]
    Hydrate(String),
}

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A live Spacedust subscription. Poll it with [`Subscriber::next_event`].
///
/// Pings are answered transparently; non-link frames are skipped. This is a
/// single connection with no reconnect — that (and Constellation backfill) is a
/// layer above this crate.
pub struct Subscriber {
    ws: Ws,
}

impl Subscriber {
    /// Connect and subscribe using `cfg`.
    pub async fn connect(cfg: &SpacedustConfig) -> Result<Self, SpacedustError> {
        let url = cfg.subscribe_url()?;
        tracing::info!(
            endpoint = %cfg.endpoint,
            account = cfg.account.as_deref().unwrap_or_default(),
            "connecting to spacedust",
        );
        let (ws, _resp) = connect_async(url.as_str())
            .await
            .map_err(SpacedustError::Connect)?;
        Ok(Self { ws })
    }

    /// The next link event, answering pings and skipping non-link frames along
    /// the way. `Ok(None)` when the connection closes cleanly.
    pub async fn next_event(&mut self) -> Result<Option<LinkEvent>, SpacedustError> {
        while let Some(msg) = self.ws.next().await {
            match msg.map_err(SpacedustError::Ws)? {
                Message::Text(text) => {
                    let event: ClientEvent =
                        serde_json::from_str(text.as_str()).map_err(SpacedustError::Decode)?;
                    if event.kind == "link" {
                        return Ok(Some(event.link));
                    }
                    tracing::debug!(kind = %event.kind, "skipping non-link frame");
                }
                Message::Ping(payload) => {
                    self.ws
                        .send(Message::Pong(payload))
                        .await
                        .map_err(SpacedustError::Ws)?;
                }
                Message::Close(_) => return Ok(None),
                _ => {}
            }
        }
        Ok(None)
    }
}

// --- EventSource adapter: Spacedust links + a Hydrator → engine Events ---

use tass_engine::{Event, EventSource, Hydrator};

/// Build a `tass_engine::Event` from a link and its hydrated record body.
///
/// `actor_did` and `collection` come from the `source_record` at-uri (no
/// hydration needed); `text` is the record's `text` field (empty if absent).
fn event_from(link: LinkEvent, record: &serde_json::Value) -> Result<Event, SpacedustError> {
    let (actor_did, collection) = {
        let (did, collection, _rkey) = tass_engine::parse_at_uri(&link.source_record)
            .ok_or_else(|| SpacedustError::BadUri(link.source_record.clone()))?;
        (did.to_string(), collection.to_string())
    };
    let text = record
        .get("text")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    Ok(Event {
        actor_did,
        collection,
        source_record: link.source_record,
        source_rev: link.source_rev,
        subject: link.subject,
        text,
    })
}

/// An [`EventSource`] over Spacedust: read the next link, hydrate its
/// `source_record` via `H`, and yield an engine [`Event`]. `delete` links are
/// skipped (nothing to hydrate).
pub struct SpacedustSource<H> {
    subscriber: Subscriber,
    hydrator: H,
}

impl<H: Hydrator> SpacedustSource<H> {
    /// Connect and pair the subscription with a hydrator.
    pub async fn connect(cfg: &SpacedustConfig, hydrator: H) -> Result<Self, SpacedustError> {
        Ok(Self {
            subscriber: Subscriber::connect(cfg).await?,
            hydrator,
        })
    }
}

impl<H: Hydrator> EventSource for SpacedustSource<H> {
    type Error = SpacedustError;

    async fn next(&mut self) -> Option<Result<Event, SpacedustError>> {
        loop {
            let link = match self.subscriber.next_event().await {
                Ok(Some(link)) => link,
                Ok(None) => return None,
                Err(e) => return Some(Err(e)),
            };
            if link.operation != "create" {
                continue; // deletes have nothing to hydrate
            }
            return Some(match self.hydrator.hydrate(&link.source_record).await {
                Ok(record) => event_from(link, &record),
                Err(e) => Err(SpacedustError::Hydrate(e.to_string())),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_from_extracts_actor_collection_and_text() {
        let link = LinkEvent {
            operation: "create".to_string(),
            source: "app.bsky.feed.post:reply.parent.uri".to_string(),
            source_record: "at://did:plc:them/app.bsky.feed.post/3l".to_string(),
            source_rev: "rev".to_string(),
            subject: "at://did:plc:mage/app.bsky.feed.post/3k".to_string(),
        };
        let record = serde_json::json!({ "$type": "app.bsky.feed.post", "text": "burn my tass" });
        let ev = event_from(link, &record).unwrap();
        assert_eq!(ev.actor_did, "did:plc:them");
        assert_eq!(ev.collection, "app.bsky.feed.post");
        assert_eq!(ev.text, "burn my tass");
        assert_eq!(ev.subject, "at://did:plc:mage/app.bsky.feed.post/3k");
    }

    #[test]
    fn subscribe_url_encodes_did_and_params() {
        let cfg = SpacedustConfig {
            endpoint: DEFAULT_ENDPOINT.to_string(),
            account: Some("did:plc:abc123".to_string()),
            wanted_sources: vec!["app.bsky.feed.post:reply.parent.uri".to_string()],
            instant: false,
        };
        let url = cfg.subscribe_url().unwrap();
        assert!(url.starts_with("wss://spacedust.microcosm.blue/subscribe?"));
        // DID colons are percent-encoded in the query value.
        assert!(url.contains("wantedSubjectDids=did%3Aplc%3Aabc123"));
        assert!(url.contains("wantedSources=app.bsky.feed.post"));
        assert!(!url.contains("instant"));
    }

    #[test]
    fn decodes_a_link_event() {
        let raw = r#"{
            "kind": "link",
            "origin": "live",
            "link": {
                "operation": "create",
                "source": "app.bsky.feed.post:reply.parent.uri",
                "source_record": "at://did:plc:them/app.bsky.feed.post/3l",
                "source_rev": "abc",
                "subject": "at://did:plc:mage/app.bsky.feed.post/3k"
            }
        }"#;
        let event: ClientEvent = serde_json::from_str(raw).unwrap();
        assert_eq!(event.kind, "link");
        assert_eq!(event.link.operation, "create");
        assert_eq!(event.link.subject, "at://did:plc:mage/app.bsky.feed.post/3k");
    }
}
