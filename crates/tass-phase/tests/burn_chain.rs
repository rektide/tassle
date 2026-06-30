//! A worked example: a small "action chain" of phases with short-circuits.
//!
//! This sketches (illustratively, not authoritatively) the shape of a tassle
//! listener command like "burn my tass": resolve a target, authorize, read the
//! balance, write the enervate, attest. Each phase is an FSM state; each event
//! advances it; each effect is a side effect the driver performs. Short-circuit
//! paths (`Skipped`, `Denied`, `Aborted`) are just transitions to terminal
//! phases.
//!
//! The driver here is fully synthetic — a scripted list of events and a log of
//! effects — so the whole chain is testable with no network and no clock. The
//! `parks_and_resumes` test shows the durability seam; the `executor` test runs
//! several chains concurrently.

use std::collections::VecDeque;
use std::convert::Infallible;

use rust_fsm::state_machine;
use tass_phase::{Driver, Executor, FsmJob, Job, Phases, StateMachine};

state_machine! {
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    burn(Matched)

    // Phase: a command was keyword-matched. Resolve its target.
    Matched => {
        FoundTarget => Resolved,
        NoTarget => Skipped,            // short-circuit: nothing to act on
    },
    // Phase: target resolved. Authorize the actor.
    Resolved => {
        Owner => Authorized [ReadBalance],   // effect: read the tass balance
        NotOwner => Denied,             // short-circuit: not your tass
    },
    // Phase: authorized + balance read. Enact if there's enough.
    Authorized => {
        Sufficient => Enacting [WriteEnervate],
        Insufficient => Aborted,        // short-circuit: nothing to burn
    },
    // Phase: enervate written. Attest the change.
    Enacting(Wrote) => Attesting [WriteAttestation],
    // Phase: attestation posted. Done.
    Attesting(Attested) => Done,
}

/// The final value extracted from a terminal phase.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Burned,
    Skipped,
    Denied,
    Aborted,
}

impl Phases for burn::Impl {
    type Final = Outcome;

    fn is_terminal(state: &burn::State) -> bool {
        matches!(
            state,
            burn::State::Done | burn::State::Skipped | burn::State::Denied | burn::State::Aborted
        )
    }

    fn finish(state: &burn::State) -> Outcome {
        match state {
            burn::State::Done => Outcome::Burned,
            burn::State::Skipped => Outcome::Skipped,
            burn::State::Denied => Outcome::Denied,
            burn::State::Aborted => Outcome::Aborted,
            // Non-terminal phases are never passed here (see is_terminal).
            other => unreachable!("finish on non-terminal phase {other:?}"),
        }
    }
}

/// A synthetic driver: replays a scripted sequence of events and records the
/// effects performed. Stands in for the real driver that would hydrate the
/// post, resolve the character/tass, lend an authed session, and write records.
struct ScriptedDriver {
    events: VecDeque<burn::Input>,
    effects: Vec<&'static str>,
}

impl ScriptedDriver {
    fn new(events: impl IntoIterator<Item = burn::Input>) -> Self {
        Self {
            events: events.into_iter().collect(),
            effects: Vec::new(),
        }
    }
}

impl tass_phase::Driver<burn::Impl> for ScriptedDriver {
    type Error = Infallible;

    async fn next_event(&mut self, _state: &burn::State) -> Result<burn::Input, Infallible> {
        // A real driver would `await` here (a reply, a `sleep_until(due)`); the
        // script just pops the next outcome.
        Ok(self
            .events
            .pop_front()
            .expect("script ran out of events before reaching a terminal phase"))
    }

    async fn effect(
        &mut self,
        effect: burn::Output,
        _state: &burn::State,
    ) -> Result<(), Infallible> {
        self.effects.push(match effect {
            burn::Output::ReadBalance => "read_balance",
            burn::Output::WriteEnervate => "write_enervate",
            burn::Output::WriteAttestation => "write_attestation",
        });
        Ok(())
    }
}

fn happy_events() -> [burn::Input; 5] {
    [
        burn::Input::FoundTarget,
        burn::Input::Owner,
        burn::Input::Sufficient,
        burn::Input::Wrote,
        burn::Input::Attested,
    ]
}

#[tokio::test]
async fn happy_path_burns_and_runs_every_effect() {
    // Drive a machine + driver directly so we can inspect the effect log.
    let mut machine = StateMachine::<burn::Impl>::new();
    let mut driver = ScriptedDriver::new(happy_events());
    let outcome = tass_phase::run(&mut machine, &mut driver).await.unwrap();
    assert_eq!(outcome, Outcome::Burned);
    assert_eq!(
        driver.effects,
        ["read_balance", "write_enervate", "write_attestation"]
    );

    // The FsmJob convenience produces the same final value.
    let job = FsmJob::new(ScriptedDriver::new(happy_events()));
    assert_eq!(job.phase(), &burn::State::Matched);
    assert_eq!(job.run().await.unwrap(), Outcome::Burned);
}

#[tokio::test]
async fn short_circuits_skip_remaining_phases() {
    let cases = [
        (vec![burn::Input::NoTarget], Outcome::Skipped),
        (
            vec![burn::Input::FoundTarget, burn::Input::NotOwner],
            Outcome::Denied,
        ),
        (
            vec![
                burn::Input::FoundTarget,
                burn::Input::Owner,
                burn::Input::Insufficient,
            ],
            Outcome::Aborted,
        ),
    ];
    for (events, expected) in cases {
        let job = FsmJob::new(ScriptedDriver::new(events));
        assert_eq!(job.run().await.unwrap(), expected);
    }
}

#[tokio::test]
async fn parks_and_resumes_from_a_serialized_phase() {
    // Drive to a mid-chain phase, then "park": serialize the phase.
    let mut machine = StateMachine::<burn::Impl>::new();
    let mut driver = ScriptedDriver::new([burn::Input::FoundTarget, burn::Input::Owner]);
    machine
        .consume(&driver.next_event(machine.state()).await.unwrap())
        .unwrap();
    let effect = machine
        .consume(&driver.next_event(machine.state()).await.unwrap())
        .unwrap();
    assert!(effect.is_some()); // ReadBalance emitted entering Authorized
    assert_eq!(machine.state(), &burn::State::Authorized);

    // Persist the phase (this is what a durable dueAt worker would store).
    let parked = serde_json::to_string(machine.state()).unwrap();

    // ...later, in a fresh process: rehydrate and finish the rest of the chain.
    let phase: burn::State = serde_json::from_str(&parked).unwrap();
    let resumed = FsmJob::resume(
        phase,
        ScriptedDriver::new([burn::Input::Sufficient, burn::Input::Wrote, burn::Input::Attested]),
    );
    assert_eq!(resumed.run().await.unwrap(), Outcome::Burned);
}

#[tokio::test]
async fn executor_streams_completions_concurrently() {
    let mut exec: Executor<'_, Result<Outcome, _>> = Executor::new();
    exec.spawn_job(FsmJob::new(ScriptedDriver::new([burn::Input::NoTarget])));
    exec.spawn_job(FsmJob::new(ScriptedDriver::new([
        burn::Input::FoundTarget,
        burn::Input::NotOwner,
    ])));
    exec.spawn_job(FsmJob::new(ScriptedDriver::new([
        burn::Input::FoundTarget,
        burn::Input::Owner,
        burn::Input::Sufficient,
        burn::Input::Wrote,
        burn::Input::Attested,
    ])));

    let mut outcomes = exec.drain().await.into_iter().map(Result::unwrap).collect::<Vec<_>>();
    outcomes.sort_by_key(|o| format!("{o:?}"));
    assert_eq!(outcomes, [Outcome::Burned, Outcome::Denied, Outcome::Skipped]);
}
