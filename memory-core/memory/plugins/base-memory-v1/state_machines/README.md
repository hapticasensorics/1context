# State Machines

State machines are plugin-owned control definitions for async memory work.
They do not replace agent judgment; they give agent work explicit events,
artifacts, evidence, permissions, and recovery states.

```text
agent output -> artifact -> evidence -> transition
```

The reusable DSL/runtime lives in `memory-core/src/onectx/state_machines/`.
This folder contains the plugin definitions loaded by that runtime.

Current rule:

- Keep the machine source readable.
- Do not add generated diagrams or compiled artifacts to the plugin.
- Do not restore the broad `uv run 1context state-machines ...` developer CLI.
- Add narrow `1context-memory-core ... --json` contract verbs only when the app
  or production memory loop needs them.

Runtime output belongs under ignored `memory/runtime/state-machines/`.
