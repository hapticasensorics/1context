## What This Map Shows

This diagram defines the smallest state-machine idea for 1context: ticks become guard evaluations, scoped state transitions, commands, evidence checks, and ledger events.

## How to read it

- Ticks come from schedules, file changes, artifact readiness, manual starts, user messages, job completion, activity detection, or approvals.
- Guards read ledger and scoped state. They should stay pure.
- Commands run named steps, spawn jobs, wait for events, or emit ledger events.
- A spawned job supplies job ids and flat job params.
- `hire_agent` creates or resumes a concrete hired agent and records the birth certificate.
- The harness does the work, uses tools, and produces artifacts.
- Evidence checks decide whether the job outcome is trusted.
- The outcome decides whether the state machine continues, waits, stops, or records failure.

## Why it matters

The state machine should make async work inspectable. Agents can reason freely inside jobs, but the overall memory system should remain understandable through scoped state, artifacts, outcomes, and ledger events.
