# Python FSM DSL

This is a standalone port of the Python finite-state-machine DSL from
`/Users/paulhan/dev/1Context-private-4`.

The package intentionally lives under `devtools/` so it can be reviewed,
tested, and evolved without putting Python back onto the shipped Rust context
engine runtime path.

It preserves the private-4 authoring surface:

```python
from onectx.state_machines.v0_1 import Machine, event, sequence, step
```

Included pieces:

- versioned FSM authoring API: `onectx.state_machines.v0_1`
- compiled IR loader and language runtime selection
- Mermaid diagram rendering
- transition planning and scope-state persistence helpers
- durable JSON work queue helpers
- typed-control-fabric design notes from private-4

Local proof:

```bash
uv run --project devtools/python-fsm-dsl pytest
```
