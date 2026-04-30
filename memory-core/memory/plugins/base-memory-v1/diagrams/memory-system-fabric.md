# Memory System Fabric

## How to read it

This is the top-level state machine for the memory system, not just the wiki
growth submachine.

Read it as the operating cycle:

- clocks and activity make memory work available
- raw Codex/Claude rows are imported and normalized
- a cheap route planner decides which work should exist
- lived-experience packets are rendered and attached before agent birth
- Claude hired agents run under bounded concurrency
- outputs are validated as talk/wiki artifacts, including skip/forget/no-change
- the wiki fabric routes page-level roles
- the reader loop builds deterministic navigation surfaces
- ledger outcomes feed the next tick

The generated IR diagram beside this note is intentionally stricter than the
narrative diagram: if the IR does not show a transition, the compiled DSL does
not yet represent it clearly.
