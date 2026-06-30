# tass-phase

**Phased async work, driven by a pure state machine.**

A small substrate for work that moves through a sequence of *phases*, performing
async side effects between them, and returns a typed final value. The state
machine is pure ([rust-fsm]); all I/O, timers, and context live in an async
*driver*; an *executor* runs many such jobs concurrently and streams each result
the instant it finishes.

It is a Rust port of a TypeScript "executor + job" idea, re-thought for async
Rust. The original drove each job by **polling** it on a fixed interval. That
polling model is deliberately **not** ported — here a job is **event-driven**: it
`await`s reality (a reply, a timer, an exit), feeds the outcome into the machine,
runs the emitted effect, and loops. It never spins on a clock.

The crate is runtime-agnostic (it only `await`s futures), but is built to run on
tokio.

[rust-fsm]: https://github.com/eugene-babichenko/rust-fsm

---

## The three layers

| Layer | What it is | Where it lives |
| --- | --- | --- |
| **Pure machine** | A `rust_fsm` `StateMachineImpl`: states = *phases*, inputs = *events*, outputs = *effects to perform*. No I/O, no time. | `state_machine! { … }` + `impl Phases` |
| **Driver** | The async bridge: `await`s the next event for the current phase, performs the effect a transition emits. All clients/sessions/timers live here. | `impl Driver<M>` |
| **Executor** | Runs many jobs concurrently on one task, yielding each result as it completes. | `Executor` |

`run(&mut machine, &mut driver)` glues a machine + driver into a loop that
returns the typed final value. `FsmJob` packages the pair into a `Job`.

## Core types

```rust
/// A rust-fsm machine whose states are phases, plus finish semantics.
pub trait Phases: StateMachineImpl {
    type Final;
    fn is_terminal(state: &Self::State) -> bool;
    fn finish(state: &Self::State) -> Self::Final;
}

/// The async bridge to the world. No polling: block on the real thing.
pub trait Driver<M: Phases> {
    type Error;
    async fn next_event(&mut self, state: &M::State) -> Result<M::Input, Self::Error>;
    async fn effect(&mut self, effect: M::Output, state: &M::State) -> Result<(), Self::Error>;
}

/// Drive a machine to a terminal phase, returning its final value.
pub async fn run<M, D>(machine: &mut StateMachine<M>, driver: &mut D)
    -> Result<M::Final, RunError<D::Error>>;
```

## Usage

Define the phases with rust-fsm's DSL. States are phases, the items in `()` /
before `=>` are events, the items in `[]` are effects. Short-circuits are just
transitions to terminal phases:

```rust
use rust_fsm::state_machine;

state_machine! {
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    burn(Matched)

    Matched => {
        FoundTarget => Resolved,
        NoTarget    => Skipped,                 // short-circuit
    },
    Resolved => {
        Owner    => Authorized [ReadBalance],   // effect on this transition
        NotOwner => Denied,                     // short-circuit
    },
    Authorized => {
        Sufficient   => Enacting [WriteEnervate],
        Insufficient => Aborted,                // short-circuit
    },
    Enacting(Wrote)    => Attesting [WriteAttestation],
    Attesting(Attested) => Done,
}
```

Teach it how to finish:

```rust
use tass_phase::Phases;

enum Outcome { Burned, Skipped, Denied, Aborted }

impl Phases for burn::Impl {
    type Final = Outcome;
    fn is_terminal(s: &burn::State) -> bool {
        matches!(s, burn::State::Done | burn::State::Skipped
                  | burn::State::Denied | burn::State::Aborted)
    }
    fn finish(s: &burn::State) -> Outcome {
        match s {
            burn::State::Done    => Outcome::Burned,
            burn::State::Skipped => Outcome::Skipped,
            burn::State::Denied  => Outcome::Denied,
            _                    => Outcome::Aborted,
        }
    }
}
```

Write the driver — this is where the `await`s and effects go:

```rust
use tass_phase::Driver;

impl Driver<burn::Impl> for MyDriver {
    type Error = anyhow::Error;

    async fn next_event(&mut self, state: &burn::State) -> Result<burn::Input, Self::Error> {
        // Block on the actual thing this phase waits for: hydrate a post,
        // read a balance, `tokio::time::sleep_until(due)`, … then report it.
        Ok(match state {
            burn::State::Matched   => self.resolve_target().await?,
            burn::State::Resolved  => self.authorize().await?,
            burn::State::Authorized => self.check_balance().await?,
            burn::State::Enacting  => burn::Input::Wrote,
            burn::State::Attesting => burn::Input::Attested,
            _ => unreachable!("non-terminal phases only"),
        })
    }

    async fn effect(&mut self, effect: burn::Output, _state: &burn::State)
        -> Result<(), Self::Error>
    {
        match effect {
            burn::Output::ReadBalance      => self.read_balance().await?,
            burn::Output::WriteEnervate    => self.write_enervate().await?,
            burn::Output::WriteAttestation => self.attest().await?,
        }
        Ok(())
    }
}
```

Run one job, or many:

```rust
use tass_phase::{Executor, FsmJob, Job};

// one
let outcome = FsmJob::new(MyDriver::new(/* … */)).run().await?;

// many, streaming completions as they finish
let mut exec = Executor::new();
for cmd in commands {
    exec.spawn_job(FsmJob::new(MyDriver::for_command(cmd)));
}
while let Some(result) = exec.next().await {
    record(result?);   // handled the instant each job completes
}
```

### Resuming a parked job (the durability seam)

`FsmJob::resume(phase, driver)` starts from a *persisted* phase instead of the
initial one. Serialize `job.phase()` when a job parks (say, until a `dueAt`);
rebuild it later in another process. The FSM state **is** the resume point:

```rust
let parked: String = serde_json::to_string(job.phase())?;   // store it
// …later…
let phase: burn::State = serde_json::from_str(&parked)?;
let outcome = FsmJob::resume(phase, MyDriver::new(/* … */)).run().await?;
```

See `tests/burn_chain.rs` for the full worked example: happy path, every
short-circuit, park-and-resume, and concurrent execution — all with a synthetic
driver, so no network or clock is needed to test the machine.

---

## Why event-driven, not polling

The original ticked each job at a fixed interval and sampled the world every
tick (`poll(now, alive)`). In async Rust that's wasteful and laggy. Here the
driver `await`s the *specific* future a phase is blocked on — a reply, a child
exit, `sleep_until(deadline)` — and a `FuturesUnordered`-backed executor
interleaves many jobs on one task. A slow job never blocks a fast one, and
nothing wakes up just to discover there's nothing to do.

The executor is single-task and **not** `Send`-bound on purpose: jobs can borrow
shared, non-`Send`/non-`'static` context (e.g. a lent `&session`) for the
executor's lifetime. For CPU-bound or cross-thread parallelism, spawn onto a
runtime task set instead.

---

## Possible tassle / spacedust applications

> These are **illustrative possibilities**, not commitments. They sketch how the
> primitive *could* be used in the listener daemon (see
> `doc/discovery/spacedust.md`); the actual design lives in those tickets.

- **Action chains as phased jobs.** A matched command ("burn my tass",
  "meditate") could be modeled as a phase machine — `resolve character →
  resolve tass → authorize → write record → attest → reply` — where each step
  runs/skips/short-circuits. The driver does the network I/O (hydrate the post,
  lend the Mage's authed session, write records); the typed `Final` is the
  outcome the wide-event line reports.

- **Deferred / due work.** A `meditate` that completes after an in-fiction
  duration, or future node regen, could *park* its phase to fjall with a
  `dueAt`, and a worker could `sleep_until(due)` (not poll) before resuming via
  `FsmJob::resume`. The parked phase doubles as durable resume state.

- **Concurrent fan-out.** As commands stream off the firehose, an `Executor`
  could run each command's chain concurrently and surface results as they land,
  rather than serializing them.

Because the machine is pure, the *decisions* of any such chain stay unit-testable
with a synthetic driver, independent of fjall and atproto.

### Relationship to `tass-job`

`tass-job` is a **planned crate** (a ticket in `doc/discovery/spacedust.md` §7
and beads — there is no code yet), scoped as a *durable due-job queue + worker*:
`{inputUri, inputCid, kind, dueAt, status, attempts}`, idempotent enactment,
retry/backoff, backed by `jac-store-fjall`.

`tass-phase` is the **generic, in-memory substrate** that such a queue would
build on, not a competitor to it. The intended split:

| | `tass-phase` (this crate) | `tass-job` (planned) |
| --- | --- | --- |
| Phase progression | ✅ `Phases` + `run` | reuses `tass-phase` |
| Effects / awaiting | ✅ `Driver` | reuses `tass-phase` |
| Concurrent run + streaming | ✅ `Executor` | extends it with scheduling |
| Persist a parked phase | seam only (`resume`/`phase`) | ✅ fjall storage |
| `dueAt` scheduling | ❌ (caller's job) | ✅ `sleep_until(due)` worker |
| Retry / backoff / `attempts` | ❌ | ✅ |
| Idempotency / dedupe ledger | ❌ | ✅ |

In short: `tass-job` ≈ `tass-phase` + fjall persistence + `dueAt` scheduling +
retry. If `tass-job` is built, it should depend on `tass-phase` for the phase
engine and add only the durable/scheduling parts. The overlap is the `Executor`
concurrency, which `tass-job`'s worker would specialize, not reinvent.

---

## Status

Early. The phase engine, driver bridge, executor, and the resume seam exist and
are tested. Durability, scheduling, and retry are intentionally out of scope —
that's a layer above (see `tass-job`).

Tracked in beads as `tass-hlr`.
