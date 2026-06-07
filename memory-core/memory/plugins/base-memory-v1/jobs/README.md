# Jobs

Jobs are reusable work contracts with human-readable ids. They choose an agent,
prompt fragments, permissions, expected inputs, expected outputs, and completion
states. Runtime params belong in the hire/cycle artifact that requested the
work, not in the static job definition.

This folder is definition-first. The removed broad `uv run 1context ...` CLI is
not the contract. The app-facing memory-core executable exposes only narrow JSON
commands through `1context-memory-core`.

The old Python `agent launch-plan` verb has been retired. The shipped
wiki-company runtime now resolves jobs and harness turns through
`onecontext-context-engine` using the runtime defaults under
`runtime/1Context/context-engine/`.

Completion vocabulary:

```text
done
skip
no_change
needs_approval
failure
```

State machines should branch on those names only when a runner is intentionally
connected. Job manifests are prompt and permission contracts, not a hidden
Python orchestration layer.
