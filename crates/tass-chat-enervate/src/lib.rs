//! **Flight 1 of enervate — the *chat* flight.**
//!
//! Enervate is a dual flow pivoting on the `at.telluri.act.enervate` **record**
//! (see `doc/act.md`). This crate is Flight 1: it listens to a user **chatting
//! at us** ("burn my tass", via Spacedust), resolves which tass, and **mints an
//! `at.telluri.act.enervate` record in the *user's own repo, as them*.** It
//! does **no** quintessence transfer and mints **no** attestation — those are
//! Flight 2 ([`tass-act-enervate`](../tass_act_enervate)), triggered by the
//! record this flight produces.
//!
//! # Requires the user to have OAuth'd through
//!
//! Minting into the user's repo needs *delegated write access*: the user must
//! have authorized tassle via the jacquard-axum **OAuth web flow**, whose
//! session lives in the shared `jac-stores` store (keyed by their DID). If we
//! have no session for the chatting user, this flight short-circuits to
//! `NeedsAuth` — a nudge to go through the web flow — and writes nothing.
//!
//! **Status:** the per-user OAuth resolution + mint are **stubbed** (see the
//! `TODO`s). Reads of the user's tass are real; the OAuth check currently
//! reports "no session", so the flight nudges rather than mints.

use jacquard::client::BasicClient;
use jacquard_common::types::ident::AtIdentifier;
use rust_fsm::state_machine;
use tass_engine::{Command, Event, Handled, Outcome};
use tass_phase::{FsmJob, Job, Phases};

/// The tassilize collection holding a tass's quintessence.
const TASSILIZE: &str = "at.telluri.act.tassilize";

state_machine! {
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    chat_enervate(Matched)

    // A "burn tass" chat matched. Gather + resolve the target tass (public read).
    Matched => {
        FoundTass => Resolved,
        NoTass => Skipped,                     // short-circuit: nothing to burn
    },
    // Target resolved. Do we have delegated write access for this user?
    Resolved => {
        Authorized => Minting [MintActRecord],
        Unauthorized => NeedsAuth,             // short-circuit: user hasn't OAuth'd
    },
    // Act record minted in the user's repo. Done — Flight 2 takes it from here.
    Minting(Minted) => Done,
}

/// Terminal value of a chat-enervate run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatOutcome {
    /// Minted the `at.telluri.act.enervate` record (dry-run in this slice).
    Minted,
    /// No tass to act on.
    Skipped,
    /// The user has not authorized tassle (no OAuth session) — nudge them.
    NeedsAuth,
}

impl Phases for chat_enervate::Impl {
    type Final = ChatOutcome;

    fn is_terminal(state: &chat_enervate::State) -> bool {
        matches!(
            state,
            chat_enervate::State::Done
                | chat_enervate::State::Skipped
                | chat_enervate::State::NeedsAuth
        )
    }

    fn finish(state: &chat_enervate::State) -> ChatOutcome {
        match state {
            chat_enervate::State::Done => ChatOutcome::Minted,
            chat_enervate::State::Skipped => ChatOutcome::Skipped,
            chat_enervate::State::NeedsAuth => ChatOutcome::NeedsAuth,
            other => unreachable!("finish on non-terminal phase {other:?}"),
        }
    }
}

/// Errors the driver can hit.
#[derive(Debug, thiserror::Error)]
pub enum ChatError {
    #[error("reading the user's tass failed: {0}")]
    Read(String),
    #[error("invalid actor DID: {0}")]
    Ident(String),
    #[error("driver reached an unexpected phase: {0}")]
    Unexpected(String),
}

/// Drives the chat-enervate FSM: real public read of the user's tass; the
/// OAuth check and the mint are stubbed (see `TODO`s).
pub struct ChatEnervateDriver {
    event: Event,
    dry_run: bool,
    client: BasicClient,
    // Gathered during the run:
    tass_uri: Option<String>,
    quintessence: i64,
    amount: i64,
}

impl ChatEnervateDriver {
    /// A dry-run driver (`writes=off`) for a chat `event`.
    pub fn dry_run(event: Event) -> Self {
        Self {
            event,
            dry_run: true,
            client: BasicClient::unauthenticated(),
            tass_uri: None,
            quintessence: 0,
            amount: 0,
        }
    }

    /// Whether we hold a delegated OAuth session for the chatting user.
    ///
    /// TODO(tass-chat-oauth): resolve the user's OAuth session from `jac-stores`
    /// keyed by `self.event.actor_did` (granted via the jacquard-axum web flow).
    /// Until that is wired we have no session, so the flight nudges to auth.
    fn user_has_authorized(&self) -> bool {
        false
    }
}

impl tass_phase::Driver<chat_enervate::Impl> for ChatEnervateDriver {
    type Error = ChatError;

    async fn next_event(
        &mut self,
        state: &chat_enervate::State,
    ) -> Result<chat_enervate::Input, ChatError> {
        match state {
            // Gather: point at the user's PDS, list their tass, pick one.
            chat_enervate::State::Matched => {
                tass_repo::resolve_and_point(&self.client, &self.event.actor_did)
                    .await
                    .map_err(|e| ChatError::Read(e.to_string()))?;
                let repo = AtIdentifier::new_owned(&self.event.actor_did)
                    .map_err(|e| ChatError::Ident(e.to_string()))?;
                let page =
                    tass_repo::list_records(&self.client, repo, TASSILIZE, Some(100), None, false)
                        .await
                        .map_err(|e| ChatError::Read(e.to_string()))?;
                match pick_tass(&page.records, &self.event.text) {
                    Some((uri, quintessence)) => {
                        self.tass_uri = Some(uri);
                        self.quintessence = quintessence;
                        self.amount =
                            parse_amount(&self.event.text).unwrap_or(self.quintessence);
                        Ok(chat_enervate::Input::FoundTass)
                    }
                    None => Ok(chat_enervate::Input::NoTass),
                }
            }
            // Authorize the *write*: do we have the user's OAuth session?
            chat_enervate::State::Resolved => {
                if self.user_has_authorized() {
                    Ok(chat_enervate::Input::Authorized)
                } else {
                    Ok(chat_enervate::Input::Unauthorized)
                }
            }
            chat_enervate::State::Minting => Ok(chat_enervate::Input::Minted),
            other => Err(ChatError::Unexpected(format!("{other:?}"))),
        }
    }

    async fn effect(
        &mut self,
        effect: chat_enervate::Output,
        _state: &chat_enervate::State,
    ) -> Result<(), ChatError> {
        match effect {
            // Mint an at.telluri.act.enervate record in the USER'S repo, as them.
            //
            // TODO(tass-chat-oauth): build a write client from the user's
            // jac-stores OAuth session and putRecord at.telluri.act.enervate
            // { tass, amount, createdAt } into their repo. Only reachable once
            // user_has_authorized() is real.
            chat_enervate::Output::MintActRecord => {
                tracing::info!(
                    actor = %self.event.actor_did,
                    tass = ?self.tass_uri,
                    amount = self.amount,
                    dry_run = self.dry_run,
                    "would mint at.telluri.act.enervate in the user's repo",
                );
            }
        }
        Ok(())
    }
}

/// Pick a tass: prefer one whose `form` word appears in the message, else the
/// first. Returns its at-uri and current quintessence.
fn pick_tass(records: &[tass_repo::RecordEnvelope], text: &str) -> Option<(String, i64)> {
    let haystack = text.to_lowercase();
    let quint =
        |r: &tass_repo::RecordEnvelope| r.value.get("quintessence").and_then(|v| v.as_i64()).unwrap_or(0);
    for r in records {
        if let Some(form) = r.value.get("form").and_then(|v| v.as_str()) {
            if !form.is_empty() && haystack.contains(&form.to_lowercase()) {
                return Some((r.uri.clone(), quint(r)));
            }
        }
    }
    records.first().map(|r| (r.uri.clone(), quint(r)))
}

/// Parse an explicit amount ("burn 5 tass"): the first bare integer. `None` =
/// the whole balance.
fn parse_amount(text: &str) -> Option<i64> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .find_map(|s| s.parse::<i64>().ok())
}

/// The chat-enervate command: keyword-spots "burn ... tass".
pub struct ChatEnervateCommand;

impl Command for ChatEnervateCommand {
    fn name(&self) -> &str {
        "enervate-chat"
    }

    fn matches(&self, event: &Event) -> bool {
        let t = event.text.to_lowercase();
        t.contains("burn") && t.contains("tass")
    }

    fn handle(&self, event: Event) -> Handled {
        Box::pin(async move {
            let driver = ChatEnervateDriver::dry_run(event);
            match FsmJob::new(driver).run().await {
                Ok(ChatOutcome::Minted) => Outcome::DryRun, // dry-run; Acted once OAuth mint lands
                Ok(ChatOutcome::Skipped) => Outcome::Skipped("no tass to burn"),
                Ok(ChatOutcome::NeedsAuth) => {
                    Outcome::Skipped("user has not authorized — OAuth web-flow required")
                }
                Err(e) => Outcome::Failed(e.to_string()),
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;
    use std::convert::Infallible;
    use tass_phase::{run, StateMachine};

    fn chat(text: &str) -> Event {
        Event {
            actor_did: "did:plc:them".into(),
            source_record: "at://did:plc:them/app.bsky.feed.post/1".into(),
            source_rev: "r".into(),
            subject: "at://did:plc:mage/app.bsky.feed.post/2".into(),
            collection: "app.bsky.feed.post".into(),
            text: text.into(),
            body: None,
        }
    }

    #[test]
    fn matches_burn_tass_only() {
        let cmd = ChatEnervateCommand;
        assert!(cmd.matches(&chat("please burn my tass")));
        assert!(!cmd.matches(&chat("burn the toast")));
    }

    #[test]
    fn parses_amount_or_none() {
        assert_eq!(parse_amount("burn 5 tass"), Some(5));
        assert_eq!(parse_amount("burn my tass"), None);
    }

    struct Scripted(VecDeque<chat_enervate::Input>);
    impl tass_phase::Driver<chat_enervate::Impl> for Scripted {
        type Error = Infallible;
        async fn next_event(
            &mut self,
            _s: &chat_enervate::State,
        ) -> Result<chat_enervate::Input, Infallible> {
            Ok(self.0.pop_front().expect("script exhausted"))
        }
        async fn effect(
            &mut self,
            _e: chat_enervate::Output,
            _s: &chat_enervate::State,
        ) -> Result<(), Infallible> {
            Ok(())
        }
    }

    async fn drive(events: impl IntoIterator<Item = chat_enervate::Input>) -> ChatOutcome {
        let mut machine = StateMachine::<chat_enervate::Impl>::new();
        let mut d = Scripted(events.into_iter().collect());
        run(&mut machine, &mut d).await.unwrap()
    }

    #[tokio::test]
    async fn paths() {
        use chat_enervate::Input::*;
        assert_eq!(drive([FoundTass, Authorized, Minted]).await, ChatOutcome::Minted);
        assert_eq!(drive([NoTass]).await, ChatOutcome::Skipped);
        assert_eq!(drive([FoundTass, Unauthorized]).await, ChatOutcome::NeedsAuth);
    }
}
