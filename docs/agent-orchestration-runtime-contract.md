# Agent Orchestration Runtime Contract

- Status: accepted main-branch release contract
- Last updated: 2026-06-06
- Owner: agent harness, Codex adapter, wiki/mail runtime

## Decision

The main branch release path is harness-only orchestration.

1Context owns agent scheduling, identity, lineage, mail, receipts, and
completion through the agent harness. Codex is a worker runtime that can run a
bounded turn, receive authorized context, emit events, and write required proof.
Codex is not the scheduler for the wiki company in the main-branch product
path.

Native Codex multi-agent V2 is not part of the release contract. Do not add
per-agent negotiation for native Codex subagents, encrypted/plaintext MAv2
configuration, or prompt branches that tell an agent it may spawn native
subagents.

## Runtime Shape

```text
1Context orchestrator
  -> agent harness birth/start/complete records
  -> Codex adapter starts or resumes one bounded worker thread
  -> worker receives one job packet
  -> worker uses granted 1Context tools
  -> harness records final message, mail receipt, talk receipt, and adapter proof
```

The worker prompt must reflect this resolved mode:

```text
You are a harness-born 1Context wiki agent.
Complete this single bounded turn.
Do not spawn subagents; the 1Context harness owns orchestration.
```

There is no agent-level `required/preferred/disabled` native-MAv2 policy on
main. If an agent needs more work, it asks the harness/orchestrator for the next
agent request through its final receipt.

## Experiment Rule

Native Codex multi-agent work may exist on experiment branches only. If that
work returns to main, it must first become a simple runtime capability with a
hard proof that the selected Codex runtime supports the exact mode 1Context
needs. Desired support must never leak into prompts unless the runtime has
already granted it.

Until that happens, the shipped fallback is not a downgrade path. Harness-only
is the product path.
