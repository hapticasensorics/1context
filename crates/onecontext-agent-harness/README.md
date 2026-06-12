# onecontext-agent-harness

Workspace folder for the 1Context Agent Harness system.

```text
core/            runtime-neutral agent unit store and proof ledger
daemon/          one-shot JSON command adapter for the harness binary
adapters/codex/  Codex app-server adapter and harness bridge
```

The harness owns agent birth, lineage, turn lifecycle, adapter evidence, proof
status, and retirement. Durable mail and delivery truth lives in the sibling
`onecontext-agent-mail` crate.
