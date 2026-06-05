# Jobs

Jobs are reusable work contracts with human-readable ids. They choose an agent,
prompt fragments, permissions, expected inputs, expected outputs, and completion
states. Runtime params belong in the hire/cycle artifact that requested the
work, not in the static job definition.

This folder is definition-first. The removed broad `uv run 1context ...` CLI is
not the contract. The app-facing memory-core executable exposes only narrow JSON
commands through `1context-memory-core`.

The current runner-adjacent verb is:

```bash
1context-memory-core agent launch-plan memory.hourly.scribe --provider codex --json
```

It resolves a job, agent profile, harness, prompt fragments, references, model,
and runtime params into a session packet under
`memory/runtime/agent-sessions/<run-id>/`. The packet includes `prompt.md`,
`launch.json`, `run.sh`, and a workspace directory. The base-memory-v1 wiki
roles declare the Codex CLI harness by default; use `--provider codex` to make
that explicit, or omit provider to use the agent's declared Codex harness. State
machines still own the decision of which job to hire and which completion event
should fire after the session finishes.

Completion vocabulary:

```text
done
skip
no_change
needs_approval
failure
```

State machines should branch on those names once a runner is intentionally
connected. Until then, job manifests are prompt and permission contracts, not a
hidden orchestration layer.
