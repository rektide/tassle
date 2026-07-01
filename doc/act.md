# The action system: verbs as phased FSMs

> How tassle *does things*. A **verb** (enervate, later meditate) is a small state machine whose phases are the steps of the work, driven asynchronously and run concurrently. This doc describes the action layer end to end; [spacedust.md](spacedust.md) describes how events reach it. Status: **live** — `tass-act-enervate` runs through this system today (dry-run writes).

## 1. Three layers

The action system is three crates stacked, generic → specific:

| Layer | Crate | Role |
| --- | --- | --- |
| substrate | **`tass-phase`** | the generic phased-work engine: a pure FSM + an async `Driver` + a concurrent `Executor`. No domain, no I/O of its own. |
| mechanism | **`tass-engine`** | dispatch + run loop + wide-event tracing + the `Command`/`EventSource`/`Hydrator` seams. **No verbs.** |
| verbs | **`tass-act-*`** | one crate per verb (`tass-act-enervate`, …), each its own FSM + `Driver` + domain logic. |

Verbs depend on `tass-engine` and `tass-phase` and on the domain crates (`tass-repo`, `tass-quint`, `tass-config`); nothing depends *down* onto a verb except the daemon that registers it.

## 2. `tass-phase` — the substrate

A unit of work is a `Phases` machine (a `rust_fsm` state machine) plus a `Driver`:

- **Phases (pure):** `State`s are the phases, `Input`s are the events that advance them, `Output`s are the effects a transition emits. `Phases` adds `is_terminal` + `finish` (the typed final value). No I/O, no clock — fully unit-testable. Short-circuits are just transitions to terminal states.
- **Driver (async):** `next_event(&mut self, state)` awaits reality and reports the next `Input`; `effect(&mut self, output, state)` performs the side effect. **Both take `&mut self`, so the Driver is the data accumulator** — the pure FSM carries no payload, so a "gather" step stashes what it fetched *into the Driver* and later steps read it back out.
- **`run(machine, driver)`** loops: while non-terminal, await an event, `consume` it (advancing the phase, maybe emitting an effect), perform the effect. Event-driven — it only ever waits on the Driver's futures.
- **`FsmJob`** packages a machine + Driver as a `Job`. `FsmJob::new` starts fresh; `FsmJob::resume(phase, driver)` rebuilds from a persisted phase (the durability seam — see [spacedust.md § Persistence]).
- **`Executor`** runs many `Job`s concurrently on one task (`FuturesUnordered`) and streams each result as it finishes. It is **single-task / non-`Send` on purpose**, so a job can borrow shared non-`'static` context — e.g. a lent `&session`.

`tass-phase` is finished and abstract; its `tests/burn_chain.rs` is a *synthetic* illustration (scripted driver, no network), not shipped behavior.

## 3. `tass-engine` — the mechanism

The engine turns a stream of events into verb jobs and logs the outcome. Key types:

- **`Event`** — a hydrated inbound message: `{ actor_did, source_record, source_rev, subject, collection, text }`. Hydration happens *before* dispatch (the source's job), because keyword spotting needs `text`.
- **`Command`** — a verb's plug: `name()`, `matches(&Event) -> bool` (keyword spotting), `handle(Event) -> Handled` (a boxed future yielding an `Outcome`).
- **`Dispatcher`** — holds registered `Command`s; `route(&Event)` returns the first whose `matches` fires.
- **`Outcome`** — the uniform result a verb reports: `Acted` / `DryRun` / `Skipped(reason)` / `Denied(reason)` / `Failed(msg)`.
- **`EventSource`** — where events come from (source-agnostic); **`Hydrator`** — `at_uri → record JSON` (the pointer→body seam); **`parse_at_uri`** — split `at://did/collection/rkey`.
- **`run(source, dispatcher)`** — the loop:

```
loop {
    select! {
        completed job  => (its wide event already logged)
        next event     => match dispatcher.route(&event) {
            Some(cmd) => Executor.spawn( per-command span { cmd.handle(event) → wide event } )
            None      => debug!("no command matched")
        }
    }
}   // drain in-flight jobs when the source ends
```

Every dispatched command runs inside a **per-command span** and emits **one wide-event INFO line** on completion (command, actor, source_record, subject, outcome, latency). That's the canonical log line for "we saw X and did Y."

### The effect vocabulary (a convention)

`tass-engine` defines an `Effect` enum — `Gather`, `ResolveTarget`, `Authorize`, `ReadState`, `WriteEffect`, `Attest`, `Reply` — as the **shared naming vocabulary** verbs are meant to compose from: *a menu, not a mandate.* Today each verb's `rust_fsm` machine declares **its own** `Output` effects (enervate names `ReadBalance`/`WriteEnervate`/`WriteAttestation`); reuse is at the *concept* level. Converging verbs onto the shared enum is optional and can come later — the point is that there is **no generic parent FSM** verbs specialize; each verb owns its phase graph.

## 4. A verb, worked: `tass-act-enervate`

The enervate FSM (the `burn_chain` shape, made real):

```
Matched    → FoundTass  ⇒ Resolved    | NoTass       ⇒ Skipped
Resolved   → Owner      ⇒ Authorized [ReadBalance]  | NotOwner ⇒ Denied
Authorized → Sufficient ⇒ Enacting   [WriteEnervate]| Insufficient ⇒ Aborted
Enacting   → Wrote      ⇒ Attesting  [WriteAttestation]
Attesting  → Attested   ⇒ Done
```

`EnervateOutcome` (`Burned` / `Skipped` / `Denied` / `Aborted`) is the FSM's `Final`; `EnervateCommand::handle` maps it to `tass_engine::Outcome`.

**`EnervateDriver`** carries the `Event`, a `dry_run` flag, an unauthenticated `BasicClient`, and the gathered `tass_uri` / `quintessence` / `amount`:

| Phase | `next_event` does | effect |
| --- | --- | --- |
| `Matched` | `resolve_and_point` to the actor's PDS, `list_records` their `com.superbfowle.tass.tassilize`, `pick_tass` (form-word match in the message, else first) → `FoundTass`/`NoTass` | — |
| `Resolved` | it's the actor's *own* repo → `Owner` | `ReadBalance` (log the balance) |
| `Authorized` | `amount` = `parse_amount(text)` (a bare integer like "burn 5") else the whole balance; enough? → `Sufficient`/`Insufficient` | `WriteEnervate` (**dry-run: logs the would-be write**) |
| `Enacting` | `Wrote` | `WriteAttestation` (dry-run: logs) |
| `Attesting` | `Attested` | — |

`EnervateCommand::matches`: `text` contains "burn" **and** "tass". Registered into the daemon's `Dispatcher` in `tass-listen`.

**Current posture: dry-run.** `writes=off` is hardcoded — `WriteEnervate`/`WriteAttestation` only log; a real write path returns `WritesNotWired`. Nothing mutates a repo yet.

## 5. Modes: `reads` / `writes` / `verbosity`

The intended per-verb knobs (from `[service.listen.<verb>]`, see [spacedust.md](spacedust.md)) map onto the Driver, orthogonally:

- **`reads`** `off`/`on` — whether the Driver does its gather/read effects.
- **`writes`** `off`/`own`/`all` — `off` = dry-run (log the would-be write); `own` = write records to our repo; `all` = also post a public reply. **`dry-run` = `reads=on, writes=off`**, an emergent state, not a special mode.
- **`verbosity`** — how much the per-command span logs beyond the always-on wide-event line.

Status: the knobs are designed and configurable; **enervate currently ignores them and runs dry-run**. Wiring the Driver's `dry_run`/read behavior to the resolved config (and threading a lent `AuthedClient` session for real writes) is the next slice.

## 6. Adding a verb

1. New crate `tass-act-<verb>` depending on `tass-engine`, `tass-phase`, `rust-fsm`, and whatever domain crates it needs.
2. Declare the FSM with `state_machine!` (its phases/events/effects) and `impl Phases`.
3. Write a `Driver` (`next_event` + `effect`) that gathers, resolves, authorizes, enacts, attests.
4. Write a `Command` (`name`/`matches`/`handle`); `handle` builds `FsmJob::new(driver)` and maps the `Final` to `Outcome`.
5. Register it in `tass-listen`'s `Dispatcher`.

Unit-test the phase graph with a **scripted driver** (replay `Input`s, assert the `Final`) — no network, the `burn_chain`/`tass-act-enervate` test pattern.

## 7. Status & tickets

- **Built:** `tass-phase`, `tass-engine` (dispatch/run/wide-event/seams), `tass-act-enervate` (dry-run), registered in `tass-listen`.
- **Next:** thread a lent `AuthedClient` session + honor `writes` (real enervate write); wire per-verb config knobs; `tass-act-meditate`; `tass-job-persistence` (durable parked phases via `FsmJob::resume`).

Tickets: `tass-engine`, `tass-act-enervate`, `tass-act-meditate`, `tass-job`, `tass-job-persistence`, `tass-recipient-alloc`, `tass-attest-age-payload` under the **`tass-listener-svc`** epic.

## See also

- [spacedust.md](spacedust.md) — how events reach the dispatcher (Spacedust source + hydration + `tass-listen`).
- `crates/tass-phase/` (`tests/burn_chain.rs`), `crates/tass-engine/src/lib.rs`, `crates/tass-act-enervate/src/lib.rs`.
