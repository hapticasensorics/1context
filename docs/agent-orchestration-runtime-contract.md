# Agent Orchestration Runtime Contract

- Status: accepted main-branch release contract
- Last updated: 2026-06-11
- Owner: agent harness, context engine, wiki/mail runtime

## Decision

The main branch release path is harness-only orchestration.

1Context owns agent scheduling, identity, lineage, mail, receipts, and
completion through the agent harness. Codex is a worker runtime that can run a
bounded turn, receive authorized context, emit events, and write required proof.
Codex is not the scheduler for the wiki company in the main-branch product
path.

## Runtime Shape

```text
1Context orchestrator
  -> agent harness birth/start/complete records
  -> context engine starts or resumes one bounded Codex worker thread
  -> worker receives one job packet
  -> worker uses granted 1Context tools
  -> harness records final message, mail receipt, talk receipt, and turn proof
```

The worker prompt must reflect this resolved mode:

```text
You are a harness-born 1Context wiki agent.
Complete this single bounded turn.
Do not spawn subagents; the 1Context harness owns orchestration.
```

If an agent needs more work, it asks the harness/orchestrator for the next
agent request through its final receipt.
