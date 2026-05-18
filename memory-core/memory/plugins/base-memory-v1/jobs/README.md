# Jobs

Jobs are reusable work contracts with human-readable ids. They choose an agent,
prompt fragments, permissions, expected inputs, expected outputs, and completion
states. Runtime params belong in the hire/cycle artifact that requested the
work, not in the static job definition.

This folder is definition-only. The removed broad `uv run 1context ...` CLI is
not the contract. The app-facing memory-core executable exposes only narrow
JSON commands through `1context-memory-core`; future job execution should add
explicit contract verbs only when the app or production memory loop needs them.

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
