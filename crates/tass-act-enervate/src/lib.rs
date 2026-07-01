//! The **enervate** verb, plugged into `tass-engine` as a [`Command`].
//!
//! Triggered by "burn (my) tass". It owns its own `tass-phase` FSM and a
//! [`EnervateDriver`] that performs the effects — composing the effect
//! vocabulary rather than sharing a generic machine (see
//! `doc/discovery/spacedust.md`). The phase graph mirrors `tass-phase`'s
//! `burn_chain` illustration, made real:
//!
//! ```text
//! Matched   → FoundTass ⇒ Resolved   | NoTass       ⇒ Skipped
//! Resolved  → Owner     ⇒ Authorized [ReadBalance] | NotOwner ⇒ Denied
//! Authorized→ Sufficient⇒ Enacting   [WriteEnervate]| Insufficient ⇒ Aborted
//! Enacting  → Wrote     ⇒ Attesting  [WriteAttestation]
//! Attesting → Attested  ⇒ Done
//! ```
//!
//! This slice **reads** the actor's tass for real but **dry-runs writes**
//! (`writes=off`): the enact/attest effects log what they *would* write. Real
//! writes need a lent authed session (a later slice).

use jacquard::client::BasicClient;
use jacquard_common::types::ident::AtIdentifier;
use rust_fsm::state_machine;
use tass_engine::{Command, Event, Handled, Outcome};
use tass_phase::{FsmJob, Job, Phases};

/// The tassilize collection whose records hold a tass's quintessence.
const TASSILIZE: &str = "com.superbfowle.tass.tassilize";

state_machine! {
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    enervate(Matched)

    // A "burn tass" command matched. Gather + resolve the target tass.
    Matched => {
        FoundTass => Resolved,
        NoTass => Skipped,                       // short-circuit: nothing to burn
    },
    // Target resolved. Authorize the actor (own-tass-only).
    Resolved => {
        Owner => Authorized [ReadBalance],
        NotOwner => Denied,                      // short-circuit: not your tass
    },
    // Authorized + balance known. Enact if there is enough to draw.
    Authorized => {
        Sufficient => Enacting [WriteEnervate],
        Insufficient => Aborted,                 // short-circuit: nothing available
    },
    // Enervate written. Attest the change.
    Enacting(Wrote) => Attesting [WriteAttestation],
    // Attestation posted. Done.
    Attesting(Attested) => Done,
}

/// Terminal value of an enervate run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnervateOutcome {
    /// Ran through enact + attest (dry-run in this slice).
    Burned,
    /// No tass to act on.
    Skipped,
    /// The actor did not own the tass.
    Denied,
    /// Nothing available to draw.
    Aborted,
}

impl Phases for enervate::Impl {
    type Final = EnervateOutcome;

    fn is_terminal(state: &enervate::State) -> bool {
        matches!(
            state,
            enervate::State::Done
                | enervate::State::Skipped
                | enervate::State::Denied
                | enervate::State::Aborted
        )
    }

    fn finish(state: &enervate::State) -> EnervateOutcome {
        match state {
            enervate::State::Done => EnervateOutcome::Burned,
            enervate::State::Skipped => EnervateOutcome::Skipped,
            enervate::State::Denied => EnervateOutcome::Denied,
            enervate::State::Aborted => EnervateOutcome::Aborted,
            other => unreachable!("finish on non-terminal phase {other:?}"),
        }
    }
}

/// Errors the driver can hit awaiting an event or performing an effect.
#[derive(Debug, thiserror::Error)]
pub enum EnervateError {
    #[error("reading the actor's tass failed: {0}")]
    Read(String),
    #[error("invalid actor DID: {0}")]
    Ident(String),
    #[error("real writes are not wired yet (writes=off)")]
    WritesNotWired,
    #[error("driver reached an unexpected phase: {0}")]
    Unexpected(String),
}

/// Drives the enervate FSM: real reads of the actor's tass, dry-run writes.
pub struct EnervateDriver {
    event: Event,
    dry_run: bool,
    client: BasicClient,
    // Gathered during the run:
    tass_uri: Option<String>,
    quintessence: i64,
    amount: i64,
}

impl EnervateDriver {
    /// A dry-run driver (`writes=off`) for `event`.
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
}

impl tass_phase::Driver<enervate::Impl> for EnervateDriver {
    type Error = EnervateError;

    async fn next_event(
        &mut self,
        state: &enervate::State,
    ) -> Result<enervate::Input, EnervateError> {
        match state {
            // Gather: point at the actor's PDS, list their tass, pick one.
            enervate::State::Matched => {
                tass_repo::resolve_and_point(&self.client, &self.event.actor_did)
                    .await
                    .map_err(|e| EnervateError::Read(e.to_string()))?;
                let repo = AtIdentifier::new_owned(&self.event.actor_did)
                    .map_err(|e| EnervateError::Ident(e.to_string()))?;
                let page = tass_repo::list_records(&self.client, repo, TASSILIZE, Some(100), None, false)
                    .await
                    .map_err(|e| EnervateError::Read(e.to_string()))?;

                match pick_tass(&page.records, &self.event.text) {
                    Some((uri, quintessence)) => {
                        self.tass_uri = Some(uri);
                        self.quintessence = quintessence;
                        Ok(enervate::Input::FoundTass)
                    }
                    None => Ok(enervate::Input::NoTass),
                }
            }
            // Authorize: we read the actor's *own* repo, so the tass is theirs.
            enervate::State::Resolved => Ok(enervate::Input::Owner),
            // Sufficiency: amount from the message, else the whole balance.
            enervate::State::Authorized => {
                self.amount = parse_amount(&self.event.text).unwrap_or(self.quintessence);
                if self.amount > 0 && self.quintessence >= self.amount {
                    Ok(enervate::Input::Sufficient)
                } else {
                    Ok(enervate::Input::Insufficient)
                }
            }
            enervate::State::Enacting => Ok(enervate::Input::Wrote),
            enervate::State::Attesting => Ok(enervate::Input::Attested),
            other => Err(EnervateError::Unexpected(format!("{other:?}"))),
        }
    }

    async fn effect(
        &mut self,
        effect: enervate::Output,
        _state: &enervate::State,
    ) -> Result<(), EnervateError> {
        match effect {
            enervate::Output::ReadBalance => {
                tracing::debug!(tass = ?self.tass_uri, quintessence = self.quintessence, "read balance");
            }
            enervate::Output::WriteEnervate => {
                if !self.dry_run {
                    return Err(EnervateError::WritesNotWired);
                }
                tracing::info!(
                    tass = ?self.tass_uri,
                    amount = self.amount,
                    dry_run = true,
                    "would write enervate",
                );
            }
            enervate::Output::WriteAttestation => {
                tracing::info!(dry_run = self.dry_run, "would write attestation");
            }
        }
        Ok(())
    }
}

/// Pick a tass from `records`: prefer one whose `form` word appears in the
/// message, else the first. Returns its at-uri and current quintessence.
fn pick_tass(records: &[tass_repo::RecordEnvelope], text: &str) -> Option<(String, i64)> {
    let haystack = text.to_lowercase();
    let quint = |r: &tass_repo::RecordEnvelope| {
        r.value.get("quintessence").and_then(|v| v.as_i64()).unwrap_or(0)
    };

    // Name match on the tass's `form`.
    for r in records {
        if let Some(form) = r.value.get("form").and_then(|v| v.as_str()) {
            if !form.is_empty() && haystack.contains(&form.to_lowercase()) {
                return Some((r.uri.clone(), quint(r)));
            }
        }
    }
    // Fallback: the first tass.
    records.first().map(|r| (r.uri.clone(), quint(r)))
}

/// Parse an explicit amount from the message (e.g. "burn 5 tass"): the first
/// bare integer token. `None` means "the whole balance".
fn parse_amount(text: &str) -> Option<i64> {
    text.split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .find_map(|s| s.parse::<i64>().ok())
}

/// The enervate command: keyword-spots "burn ... tass" and runs the FSM.
pub struct EnervateCommand;

impl Command for EnervateCommand {
    fn name(&self) -> &str {
        "enervate"
    }

    fn matches(&self, event: &Event) -> bool {
        let t = event.text.to_lowercase();
        t.contains("burn") && t.contains("tass")
    }

    fn handle(&self, event: Event) -> Handled {
        Box::pin(async move {
            let driver = EnervateDriver::dry_run(event);
            let job = FsmJob::new(driver);
            match job.run().await {
                // Dry-run: a completed chain "would have burned".
                Ok(EnervateOutcome::Burned) => Outcome::DryRun,
                Ok(EnervateOutcome::Skipped) => Outcome::Skipped("no tass to burn"),
                Ok(EnervateOutcome::Denied) => Outcome::Denied("not the actor's tass"),
                Ok(EnervateOutcome::Aborted) => Outcome::Skipped("no quintessence available"),
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

    #[test]
    fn matches_burn_tass_only() {
        let cmd = EnervateCommand;
        let ev = |t: &str| Event {
            actor_did: "did:plc:x".into(),
            source_record: "at://did:plc:x/app.bsky.feed.post/1".into(),
            source_rev: "r".into(),
            subject: "at://did:plc:mage/app.bsky.feed.post/2".into(),
            collection: "app.bsky.feed.post".into(),
            text: t.into(),
        };
        assert!(cmd.matches(&ev("please burn my tass")));
        assert!(!cmd.matches(&ev("burn the toast")));
        assert!(!cmd.matches(&ev("hello")));
    }

    #[test]
    fn parses_amount_or_none() {
        assert_eq!(parse_amount("burn 5 tass"), Some(5));
        assert_eq!(parse_amount("burn my tass"), None);
    }

    // A scripted driver (no network) to prove the phase graph — the burn_chain
    // pattern, over the real enervate FSM.
    struct Scripted(VecDeque<enervate::Input>);
    impl tass_phase::Driver<enervate::Impl> for Scripted {
        type Error = Infallible;
        async fn next_event(
            &mut self,
            _s: &enervate::State,
        ) -> Result<enervate::Input, Infallible> {
            Ok(self.0.pop_front().expect("script exhausted"))
        }
        async fn effect(
            &mut self,
            _e: enervate::Output,
            _s: &enervate::State,
        ) -> Result<(), Infallible> {
            Ok(())
        }
    }

    async fn drive(events: impl IntoIterator<Item = enervate::Input>) -> EnervateOutcome {
        let mut machine = StateMachine::<enervate::Impl>::new();
        let mut d = Scripted(events.into_iter().collect());
        run(&mut machine, &mut d).await.unwrap()
    }

    #[tokio::test]
    async fn happy_path_burns() {
        use enervate::Input::*;
        let out = drive([FoundTass, Owner, Sufficient, Wrote, Attested]).await;
        assert_eq!(out, EnervateOutcome::Burned);
    }

    #[tokio::test]
    async fn short_circuits() {
        use enervate::Input::*;
        assert_eq!(drive([NoTass]).await, EnervateOutcome::Skipped);
        assert_eq!(drive([FoundTass, NotOwner]).await, EnervateOutcome::Denied);
        assert_eq!(
            drive([FoundTass, Owner, Insufficient]).await,
            EnervateOutcome::Aborted
        );
    }
}
