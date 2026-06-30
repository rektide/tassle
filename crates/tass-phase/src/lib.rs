//! Phased async work, driven by a pure state machine.
//!
//! This is a Rust port of a TypeScript "executor + job" idea (the
//! `zim-claude` jobs/executor), re-thought for async Rust. The original drove
//! each job by *polling* it on a fixed interval (`poll(now)` ticking a pure
//! xstate machine). That polling model is deliberately **not** ported. Here a
//! job is *event-driven*: it awaits reality (a network reply, a timer firing),
//! feeds the outcome into the machine, performs the emitted effect, and loops —
//! never spinning on a clock.
//!
//! Three layers, mirroring the original:
//!
//! 1. **The pure machine** — a [`rust_fsm`] [`StateMachineImpl`]. States are the
//!    *phases* of the work; inputs are the *events* that advance them; outputs
//!    are the *effects* to perform. No I/O, no time, fully unit-testable. You
//!    add the terminal/finish details by implementing [`Phases`].
//! 2. **The async [`Driver`]** — bridges the pure machine to the world. It
//!    `await`s the next event for the current phase ([`Driver::next_event`]) and
//!    performs the side effect a transition emits ([`Driver::effect`]). This is
//!    where all the I/O, timers, and context (clients, sessions) live.
//! 3. **The [`Executor`]** — runs many jobs concurrently on a single task and
//!    streams each result the instant it finishes (the original's `onDone`).
//!
//! [`run`] glues machine + driver into a loop that returns a typed final value
//! ([`Phases::Final`]). [`FsmJob`] packages the two into a [`Job`], and supports
//! [`FsmJob::resume`] — starting from a *persisted* phase, which is the seam a
//! durable, scheduled queue (e.g. a `dueAt`/fjall worker) builds on.
//!
//! The library is runtime-agnostic: it only `await`s futures and never touches a
//! specific reactor. Consumers are expected to run it on tokio.
//!
//! # Example
//!
//! See `tests/burn_chain.rs` for a worked "action chain" — a sequence of phases
//! with short-circuits — driven, resumed from a serialized phase, and run
//! concurrently through the [`Executor`].

use std::future::Future;
use std::pin::Pin;

use futures_util::stream::FuturesUnordered;
use futures_util::StreamExt;

pub use rust_fsm::{StateMachine, StateMachineImpl};

/// A [`StateMachineImpl`] whose states are *phases* of a unit of work, with a
/// notion of being finished and a typed final value.
///
/// `StateMachineImpl` already supplies the alphabet: `State` (the phases),
/// `Input` (the events that advance them), and `Output` (the effects a
/// transition emits). `Phases` adds the two things [`run`] needs to stop and to
/// hand back a result.
pub trait Phases: StateMachineImpl {
    /// The value produced when the machine reaches a terminal phase.
    type Final;

    /// True when `state` is terminal — [`run`] stops and no further events are
    /// awaited.
    fn is_terminal(state: &Self::State) -> bool;

    /// Extract the final value from a terminal `state`.
    ///
    /// Only called by [`run`] once [`is_terminal`](Phases::is_terminal) holds.
    fn finish(state: &Self::State) -> Self::Final;
}

/// The async bridge from a pure machine to the world.
///
/// A `Driver` is where every effect, every `await`, and all context (HTTP
/// clients, an authed session, timers) lives. The pure machine stays I/O-free;
/// the driver makes it move.
///
/// The contract: [`next_event`](Driver::next_event) is called only for a
/// non-terminal phase and must eventually resolve to a *valid* event for that
/// phase. If it yields an event with no transition, [`run`] returns
/// [`RunError::Stuck`] rather than looping.
pub trait Driver<M: Phases> {
    /// Error from awaiting an event or performing an effect (e.g. a network
    /// failure). Use [`std::convert::Infallible`] if a driver cannot fail.
    type Error;

    /// Await the next input event for the machine sitting in `state`.
    ///
    /// This is the no-polling heart of the design: block on the actual thing
    /// the phase is waiting for (a reply, a `sleep_until(due)`, a child exit),
    /// then report what happened as an [`Input`](StateMachineImpl::Input).
    fn next_event(
        &mut self,
        state: &M::State,
    ) -> impl Future<Output = Result<M::Input, Self::Error>>;

    /// Perform the side effect emitted by the transition that just produced
    /// `state`. `state` is the phase the machine *entered*, so the driver can
    /// key behavior on either the effect or the destination phase.
    fn effect(
        &mut self,
        effect: M::Output,
        state: &M::State,
    ) -> impl Future<Output = Result<(), Self::Error>>;
}

/// Why a [`run`] ended without reaching a terminal phase.
#[derive(Debug)]
pub enum RunError<E> {
    /// The driver produced an event for which the current phase has no
    /// transition — a bug in the driver/machine pairing.
    Stuck,
    /// The driver failed awaiting an event or performing an effect.
    Driver(E),
}

impl<E: std::fmt::Display> std::fmt::Display for RunError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RunError::Stuck => write!(f, "no transition for the produced event"),
            RunError::Driver(e) => write!(f, "driver error: {e}"),
        }
    }
}

impl<E: std::error::Error + 'static> std::error::Error for RunError<E> {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RunError::Stuck => None,
            RunError::Driver(e) => Some(e),
        }
    }
}

/// Drive `machine` to a terminal phase using `driver`, returning the final
/// value.
///
/// The loop: while the phase is non-terminal, await the next event, consume it
/// (advancing the phase and possibly emitting an effect), and perform any
/// emitted effect. Event-driven throughout — it only ever waits on the
/// driver's futures.
pub async fn run<M, D>(
    machine: &mut StateMachine<M>,
    driver: &mut D,
) -> Result<M::Final, RunError<D::Error>>
where
    M: Phases,
    D: Driver<M>,
{
    while !M::is_terminal(machine.state()) {
        let event = driver
            .next_event(machine.state())
            .await
            .map_err(RunError::Driver)?;
        match machine.consume(&event) {
            Ok(Some(effect)) => {
                driver
                    .effect(effect, machine.state())
                    .await
                    .map_err(RunError::Driver)?;
            }
            Ok(None) => {}
            Err(_) => return Err(RunError::Stuck),
        }
    }
    Ok(M::finish(machine.state()))
}

/// A unit of phased work that produces a value when run.
///
/// Implemented by [`FsmJob`], but kept as a trait so the [`Executor`] can hold
/// any runnable that yields a uniform `Output`.
pub trait Job {
    /// What the job produces when complete.
    type Output;

    /// Run the job to completion.
    fn run(self) -> impl Future<Output = Self::Output>;
}

/// A [`Job`] backed by a pure machine ([`Phases`]) and an async [`Driver`].
///
/// Start fresh with [`new`](FsmJob::new), or resume a *persisted* phase with
/// [`resume`](FsmJob::resume) — the latter is how a durable, scheduled queue
/// rehydrates a parked job (e.g. one that was waiting for a `dueAt`).
pub struct FsmJob<M: Phases, D> {
    machine: StateMachine<M>,
    driver: D,
}

impl<M, D> FsmJob<M, D>
where
    M: Phases,
    D: Driver<M>,
{
    /// A job that starts at the machine's initial phase.
    pub fn new(driver: D) -> Self {
        Self {
            machine: StateMachine::new(),
            driver,
        }
    }

    /// A job that resumes from a previously persisted `phase`.
    ///
    /// This is the durability seam: serialize [`phase`](FsmJob::phase) when a
    /// job parks (say, until a `dueAt`), and rebuild it here later.
    pub fn resume(phase: M::State, driver: D) -> Self {
        Self {
            machine: StateMachine::from_state(phase),
            driver,
        }
    }

    /// The current phase — serialize this to park a job durably.
    pub fn phase(&self) -> &M::State {
        self.machine.state()
    }
}

impl<M, D> Job for FsmJob<M, D>
where
    M: Phases,
    D: Driver<M>,
{
    type Output = Result<M::Final, RunError<D::Error>>;

    async fn run(mut self) -> Self::Output {
        run(&mut self.machine, &mut self.driver).await
    }
}

/// Runs many jobs concurrently on a single task, yielding each result the
/// instant it completes.
///
/// This is the [`Job`]-running analog of the original `JobExecutor`, minus the
/// fixed-interval poll loop: [`FuturesUnordered`] interleaves the jobs'
/// `await`s, so a slow job never blocks a fast one, and completions stream out
/// in finish order (the original's `onDone`).
///
/// Single-task and not `Send`-bound on purpose: jobs can borrow shared,
/// non-`Send`/non-`'static` context (e.g. a lent `&session`) for the lifetime
/// `'a`. For CPU-bound or cross-thread parallelism, spawn onto a runtime's task
/// set instead.
pub struct Executor<'a, O> {
    running: FuturesUnordered<Pin<Box<dyn Future<Output = O> + 'a>>>,
}

impl<'a, O> Executor<'a, O> {
    /// An empty executor.
    pub fn new() -> Self {
        Self {
            running: FuturesUnordered::new(),
        }
    }

    /// Number of jobs still running.
    pub fn len(&self) -> usize {
        self.running.len()
    }

    /// True when no jobs are running.
    pub fn is_empty(&self) -> bool {
        self.running.is_empty()
    }

    /// Add a future; it begins making progress on the next poll
    /// ([`next`](Executor::next) / [`drain`](Executor::drain)).
    pub fn spawn<F>(&mut self, fut: F)
    where
        F: Future<Output = O> + 'a,
    {
        self.running.push(Box::pin(fut));
    }

    /// Add a [`Job`], running it for its output.
    pub fn spawn_job<J>(&mut self, job: J)
    where
        J: Job<Output = O> + 'a,
    {
        self.running.push(Box::pin(job.run()));
    }

    /// Await the next completed result, or `None` when all jobs are done.
    ///
    /// Yields the instant *any* job finishes — the streaming completion the
    /// original exposed as `onDone`.
    pub async fn next(&mut self) -> Option<O> {
        self.running.next().await
    }

    /// Drive every job to completion, collecting results in finish order.
    pub async fn drain(mut self) -> Vec<O> {
        let mut out = Vec::with_capacity(self.running.len());
        while let Some(result) = self.running.next().await {
            out.push(result);
        }
        out
    }
}

impl<'a, O> Default for Executor<'a, O> {
    fn default() -> Self {
        Self::new()
    }
}
