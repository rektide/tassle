# tass-phase

**Phased async work, driven by a pure state machine.**

> The machine *decides*. The driver *does*. The executor runs the whole flock.

Some work isn't one `async fn` — it's a *sequence of phases*, each with its own
decision, its own side effect, its own way to bail out early. Resolve a target,
authorize, enact, attest, reply — where any step might skip, short-circuit, or
park until later. `tass-phase` gives that shape a spine: a pure
[rust-fsm] state machine for the *phases and decisions*, an async **driver** for
the *effects and awaits*, and an **executor** that runs many such jobs at once
and streams each result the instant it lands.

It's a Rust port of a TypeScript "executor + job" idea — with one deliberate
change. The original **polled** every job on a fixed interval. That's gone. Here
a job is **event-driven**: it `await`s the actual thing a phase is blocked on — a
reply, a timer, an exit — feeds the outcome into the machine, runs the emitted
effect, and loops. Nothing ever wakes up just to find there's nothing to do.

[rust-fsm]: https://github.com/eugene-babichenko/rust-fsm

---

## The idea in one breath

```
            ┌─────────────────────────────────────────────┐
            │  pure machine  (rust-fsm + Phases)           │
   phase ──▶│  states = phases                             │──▶ Final
            │  inputs = events   outputs = effects         │   (typed result)
            └───────────▲───────────────────┬──────────────┘
                        │                    │
                 next_event()            effect()
                        │                    │
            ┌───────────┴────────────────────▼──────────────┐
            │  Driver  (async, owns clients / session / time)│
            │  awaits reality · performs side effects        │
            └────────────────────────────────────────────────┘

   Executor: runs many jobs concurrently, yields each Final as it finishes.
```

- **`Phases`** — your `rust_fsm` machine plus `is_terminal` + `finish`. States
  are phases, inputs are events, outputs are the effects to perform. **No I/O, no
  clock — fully unit-testable.**
- **`Driver`** — the async bridge to the world. `next_event` blocks on the real
  future a phase waits for; `effect` performs a transition's side effect. Every
  HTTP client, session, and timer lives *here*.
- **`Executor`** — runs many jobs on a single task and streams completions.
- **`run` / `FsmJob`** — glue the machine and driver into a loop that returns a
  typed `Final`; `FsmJob::resume` starts from a *persisted* phase.

---

## Usage

**1. Declare the phases** with rust-fsm's DSL. States are phases; the tokens
around `=>` are events; the `[…]` items are effects. Short-circuits are just
transitions into terminal states:

```rust
use rust_fsm::state_machine;

state_machine! {
    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    burn(Matched)

    Matched => {
        FoundTarget => Resolved,
        NoTarget    => Skipped,                  // short-circuit
    },
    Resolved => {
        Owner    => Authorized [ReadBalance],    // effect on this transition
        NotOwner => Denied,                      // short-circuit
    },
    Authorized => {
        Sufficient   => Enacting [WriteEnervate],
        Insufficient => Aborted,                 // short-circuit
    },
    Enacting(Wrote)     => Attesting [WriteAttestation],
    Attesting(Attested) => Done,
}
```

**2. Teach it to finish:**

```rust
use tass_phase::Phases;

pub enum Outcome { Burned, Skipped, Denied, Aborted }

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

**3. Write the driver** — this is where the `await`s and effects go:

```rust
use tass_phase::Driver;

impl Driver<burn::Impl> for MyDriver {
    type Error = anyhow::Error;

    async fn next_event(&mut self, state: &burn::State) -> Result<burn::Input, Self::Error> {
        // Block on the *specific* thing this phase waits for, then report it.
        Ok(match state {
            burn::State::Matched    => self.resolve_target().await?,
            burn::State::Resolved   => self.authorize().await?,
            burn::State::Authorized => self.check_balance().await?,
            burn::State::Enacting   => burn::Input::Wrote,
            burn::State::Attesting  => burn::Input::Attested,
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

**4. Run one, or a whole flock:**

```rust
use tass_phase::{Executor, FsmJob, Job};

// one
let outcome = FsmJob::new(MyDriver::new(/* … */)).run().await?;

// many — completions stream out as each job finishes
let mut exec = Executor::new();
for cmd in commands {
    exec.spawn_job(FsmJob::new(MyDriver::for_command(cmd)));
}
while let Some(result) = exec.next().await {
    record(result?);   // handled the instant that job completes
}
```

The machine is pure, so every *decision* stays unit-testable with a synthetic
driver — no network, no clock. See `tests/burn_chain.rs` for the full worked
example: happy path, every short-circuit, park-and-resume, and concurrent
execution.

---

## The concurrency model (and how to still fan out)

The `Executor` runs on **one task** (a `FuturesUnordered`). Read that carefully:
one task, not one core's worth of work.

**An `.await` on I/O doesn't occupy the task or a core.** When a job awaits a
network reply it *parks*, and the task moves on to the next job; the socket is
driven by tokio's (multi-threaded) reactor. So a single task happily juggles
**thousands of concurrent I/O-bound jobs** — it's just dispatching. The only
thing that genuinely runs *on* the task, stealing from other jobs, is
**synchronous CPU work between awaits** (crypto, big JSON walks) — or, the
footgun, a *blocking* sync call.

Why single-task? Not a tokio limitation — tokio's default runtime is
multi-threaded and *wants* to spread work. It's a deliberate fit for a **borrow
model**: single-task means jobs can hold non-`Send`/non-`'static` context — a
lent `&session`, an un-`Clone`-able client — for the executor's lifetime. You get
that ergonomic borrow *and* massive I/O concurrency.

### Fan out anyway — per effect, not per job

The single task is **coordination**. The heavy **labor** can still spread across
cores: an effect is free to `tokio::spawn` / `spawn_blocking` its self-contained,
owned, `Send` work and await the handle back on the driver task. **No change to
`tass-phase` required** — the trait already allows it. Only owned data crosses
the boundary; `&session` never does:

```rust
async fn effect(&mut self, e: burn::Output, _s: &burn::State) -> Result<(), Self::Error> {
    match e {
        // I/O + session → stays on the driver task (it's only awaiting anyway)
        burn::Output::WriteEnervate => self.session.create_record(/* … */).await?,

        // CPU, no session → hand owned data to the pool, await the result
        burn::Output::WriteAttestation => {
            let payload = self.diff.clone();                    // owned + Send
            let blob = tokio::task::spawn_blocking(move || age_encrypt(payload)).await??;
            self.session.post_attestation(blob).await?;         // …back on the task
        }
        _ => {}
    }
    Ok(())
}
```

This lands a lovely alignment for free: **auth work is I/O-bound and stays home;
CPU work needs no session, is `Send`, and fans out.** No two "execution modes,"
no bifurcated `Job` types — just offload the heavy bit where it happens.

Rules of thumb:

- I/O-bound (session, network, fjall)? Leave it on the task.
- CPU-heavy and self-contained? `spawn` / `spawn_blocking` it.
- *Blocking* sync call? `spawn_blocking` it regardless — it stalls the whole
  executor otherwise.

> **Not yet handled: backpressure.** `spawn` is unbounded — off a firehose,
> intake can outrun completion and grow memory without limit. A bounded-intake
> `Executor` is the next real step (tracked as `tass-phase-backpressure`).

---

## Resuming a parked job — the durability seam

`FsmJob::resume(phase, driver)` starts from a *persisted* phase instead of the
initial one. The FSM state **is** the resume point: serialize `job.phase()` when
a job parks (say, until a `dueAt`), rebuild it later — even in another process.

```rust
let parked: String = serde_json::to_string(job.phase())?;   // store it
// …later, elsewhere…
let phase: burn::State = serde_json::from_str(&parked)?;
let outcome = FsmJob::resume(phase, MyDriver::new(/* … */)).run().await?;
```

This is the seam a durable, scheduled queue builds on (see `tass-job` below).

---

## Possible tassle / spacedust applications

> **Illustrative possibilities, not commitments** — sketches of how the primitive
> *could* serve the listener daemon (`doc/discovery/spacedust.md`). The real
> design lives in those tickets.

- **Action chains as phased jobs.** A matched command ("burn my tass",
  "meditate") could be a phase machine — `resolve character → resolve tass →
  authorize → write record → attest → reply` — each step run/skip/short-circuit.
  The driver does the I/O (hydrate, lend the Mage's session, write records); the
  typed `Final` is what the wide-event line reports.
- **Deferred / due work.** A `meditate` that completes after an in-fiction
  duration, or future node regen, could *park* its phase to fjall with a `dueAt`
  and a worker could `sleep_until(due)` (not poll) before resuming.
- **Concurrent fan-out.** As commands stream off the firehose, an `Executor`
  could run each chain concurrently and surface results as they land.

### Relationship to `tass-job`

`tass-job` is a **planned crate** (a ticket, no code yet), scoped as a *durable
due-job queue + worker*: `{inputUri, inputCid, kind, dueAt, status, attempts}`,
idempotent enactment, retry/backoff, backed by `jac-store-fjall`.

`tass-phase` is the **generic, in-memory substrate** it would build on — not a
competitor:

| | `tass-phase` (this crate) | `tass-job` (planned) |
| --- | --- | --- |
| Phase progression | ✅ `Phases` + `run` | reuses `tass-phase` |
| Effects / awaiting | ✅ `Driver` | reuses `tass-phase` |
| Concurrent run + streaming | ✅ `Executor` | extends with scheduling |
| Persist a parked phase | seam only (`resume`/`phase`) | ✅ fjall storage |
| `dueAt` scheduling | ❌ (caller's job) | ✅ `sleep_until(due)` |
| Retry / backoff / `attempts` | ❌ | ✅ |
| Idempotency / dedupe ledger | ❌ | ✅ |

In short: **`tass-job` ≈ `tass-phase` + fjall + `dueAt` + retry.** If built, it
should depend on `tass-phase` for the engine and add only the durable/scheduling
parts.

---

## Design notes

- **Runtime-agnostic core.** The library only `await`s futures — deps are just
  `rust-fsm` + `futures-util`. `tokio` is a dev-dependency (examples/tests).
  Built to run on tokio; not welded to it.
- **The machine never runs itself.** No actors, no internal timers, no `after`.
  It transitions only when the driver feeds it an event — which is what makes it
  trivially testable and safely resumable.
- **Effects are Mealy outputs**, emitted on transitions, performed by the driver
  keyed on the effect *or* the phase it entered.

---

## Status

Early but real. The phase engine, driver bridge, executor, and resume seam exist
and are tested. Durability, scheduling, retry (→ `tass-job`) and bounded intake
(→ `tass-phase-backpressure`) are intentionally out of scope.

Tracked in beads as `tass-phase`.
