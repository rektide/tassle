//! **Flight 2 of enervate — the *act* flight.**
//!
//! Enervate is a dual flow pivoting on the `at.telluri.act.enervate` **record**
//! (see `doc/act.md`). Flight 1 ([`tass-chat-enervate`](../tass_chat_enervate))
//! turns a chat ("burn my tass") into that record in the user's repo. **This**
//! crate is Flight 2: it reacts to the *record itself* — matched on the
//! `at.telluri.act.enervate` **collection** (not a keyword), delivered by the
//! record firehose (`tass-source-jetstream`, planned) — and:
//!
//! 1. validates the enervate record, then
//! 2. mints an **attestation** that **traces back to the enervate** (the
//!    attestation references the enervate record's at-uri).
//!
//! **Attestation lives only here, gated on a record existing.** There is
//! deliberately **no ledger / balance transfer** — the enervate record *is* the
//! declared drain; Flight 2 witnesses and attests it. (The attestation may take
//! the shape of an `equipment.rpg.give` traced to the enervate — open design.)
//!
//! Writes are **dry-run** for now: the attestation is authored by the service
//! (Mage) identity, which is not yet wired (auth is shifting). The `Attest`
//! effect logs the would-be attestation. Real writes are a later slice.

use rust_fsm::state_machine;
use tass_engine::{Command, Event, Handled, Outcome};
use tass_phase::{FsmJob, Job, Phases};

/// The act-record collection this flight reacts to.
pub const ENERVATE: &str = "at.telluri.act.enervate";

state_machine! {
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    act_enervate(Received)

    // An at.telluri.act.enervate record arrived. Validate its fields.
    Received => {
        Valid => Validated,
        Invalid => Rejected,          // short-circuit: malformed record
    },
    // Valid. Mint the attestation that traces back to this enervate.
    Validated(Confirmed) => Attesting [Attest],
    // Attestation posted. Done.
    Attesting(Attested) => Done,
}

/// Terminal value of an act-enervate run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActOutcome {
    /// Attested the enervate (dry-run in this slice).
    Attested,
    /// The record was malformed.
    Rejected,
}

impl Phases for act_enervate::Impl {
    type Final = ActOutcome;

    fn is_terminal(state: &act_enervate::State) -> bool {
        matches!(
            state,
            act_enervate::State::Done | act_enervate::State::Rejected
        )
    }

    fn finish(state: &act_enervate::State) -> ActOutcome {
        match state {
            act_enervate::State::Done => ActOutcome::Attested,
            act_enervate::State::Rejected => ActOutcome::Rejected,
            other => unreachable!("finish on non-terminal phase {other:?}"),
        }
    }
}

/// Errors the driver can hit.
#[derive(Debug, thiserror::Error)]
pub enum ActError {
    #[error("driver reached an unexpected phase: {0}")]
    Unexpected(String),
}

/// Drives the act-enervate FSM: read the record from the event, dry-run attest.
pub struct ActEnervateDriver {
    event: Event,
    dry_run: bool,
    // Parsed from the record during validation:
    tass: Option<String>,
    amount: i64,
}

impl ActEnervateDriver {
    /// A dry-run driver (`writes=off`) for an enervate-record `event`.
    pub fn dry_run(event: Event) -> Self {
        Self {
            event,
            dry_run: true,
            tass: None,
            amount: 0,
        }
    }
}

impl tass_phase::Driver<act_enervate::Impl> for ActEnervateDriver {
    type Error = ActError;

    async fn next_event(
        &mut self,
        state: &act_enervate::State,
    ) -> Result<act_enervate::Input, ActError> {
        match state {
            // Validate: the record must carry a `tass` at-uri (and an amount).
            act_enervate::State::Received => {
                let body = self.event.body.as_ref();
                let tass = body
                    .and_then(|b| b.get("tass"))
                    .and_then(|v| v.as_str())
                    .map(str::to_string);
                self.amount = body
                    .and_then(|b| b.get("amount"))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                match tass {
                    Some(uri) => {
                        self.tass = Some(uri);
                        Ok(act_enervate::Input::Valid)
                    }
                    None => Ok(act_enervate::Input::Invalid),
                }
            }
            act_enervate::State::Validated => Ok(act_enervate::Input::Confirmed),
            act_enervate::State::Attesting => Ok(act_enervate::Input::Attested),
            other => Err(ActError::Unexpected(format!("{other:?}"))),
        }
    }

    async fn effect(
        &mut self,
        effect: act_enervate::Output,
        _state: &act_enervate::State,
    ) -> Result<(), ActError> {
        match effect {
            // Mint the attestation, tracing back to the enervate record.
            //
            // TODO(auth): real write needs the service (Mage) identity — auth
            // sourcing is being re-architected; see the auth notes. The
            // attestation may be an equipment.rpg.give traced to the enervate.
            act_enervate::Output::Attest => {
                tracing::info!(
                    enervate = %self.event.source_record, // the record we attest
                    tass = ?self.tass,
                    amount = self.amount,
                    dry_run = self.dry_run,
                    "would mint attestation tracing back to enervate",
                );
            }
        }
        Ok(())
    }
}

/// The act-enervate command: matches by **collection** (record-triggered), not
/// by keyword.
pub struct ActEnervateCommand;

impl Command for ActEnervateCommand {
    fn name(&self) -> &str {
        "enervate-act"
    }

    fn matches(&self, event: &Event) -> bool {
        event.collection == ENERVATE
    }

    fn handle(&self, event: Event) -> Handled {
        Box::pin(async move {
            let driver = ActEnervateDriver::dry_run(event);
            match FsmJob::new(driver).run().await {
                Ok(ActOutcome::Attested) => Outcome::DryRun, // dry-run; Acted once writes land
                Ok(ActOutcome::Rejected) => Outcome::Skipped("malformed enervate record"),
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

    fn record_event(body: Option<serde_json::Value>) -> Event {
        Event {
            actor_did: "did:plc:them".into(),
            source_record: "at://did:plc:them/at.telluri.act.enervate/3l".into(),
            source_rev: "r".into(),
            subject: "at://did:plc:them/at.telluri.act.tassilize/3k".into(),
            collection: ENERVATE.into(),
            text: String::new(),
            body,
        }
    }

    #[test]
    fn matches_by_collection_not_keyword() {
        let cmd = ActEnervateCommand;
        assert!(cmd.matches(&record_event(None)));
        let mut other = record_event(None);
        other.collection = "app.bsky.feed.post".into();
        assert!(!cmd.matches(&other));
    }

    // Scripted driver to prove the phase graph without any I/O.
    struct Scripted(VecDeque<act_enervate::Input>);
    impl tass_phase::Driver<act_enervate::Impl> for Scripted {
        type Error = Infallible;
        async fn next_event(
            &mut self,
            _s: &act_enervate::State,
        ) -> Result<act_enervate::Input, Infallible> {
            Ok(self.0.pop_front().expect("script exhausted"))
        }
        async fn effect(
            &mut self,
            _e: act_enervate::Output,
            _s: &act_enervate::State,
        ) -> Result<(), Infallible> {
            Ok(())
        }
    }

    async fn drive(events: impl IntoIterator<Item = act_enervate::Input>) -> ActOutcome {
        let mut machine = StateMachine::<act_enervate::Impl>::new();
        let mut d = Scripted(events.into_iter().collect());
        run(&mut machine, &mut d).await.unwrap()
    }

    #[tokio::test]
    async fn attests_valid_record() {
        use act_enervate::Input::*;
        assert_eq!(drive([Valid, Confirmed, Attested]).await, ActOutcome::Attested);
    }

    #[tokio::test]
    async fn rejects_malformed_record() {
        assert_eq!(
            drive([act_enervate::Input::Invalid]).await,
            ActOutcome::Rejected
        );
    }

    #[tokio::test]
    async fn real_driver_validates_body() {
        // A well-formed enervate record body validates; a bodyless one rejects.
        let good = record_event(Some(serde_json::json!({
            "tass": "at://did:plc:them/at.telluri.act.tassilize/3k",
            "amount": 5
        })));
        let mut m = StateMachine::<act_enervate::Impl>::new();
        let mut d = ActEnervateDriver::dry_run(good);
        assert_eq!(run(&mut m, &mut d).await.unwrap(), ActOutcome::Attested);

        let mut m2 = StateMachine::<act_enervate::Impl>::new();
        let mut d2 = ActEnervateDriver::dry_run(record_event(None));
        assert_eq!(run(&mut m2, &mut d2).await.unwrap(), ActOutcome::Rejected);
    }
}
