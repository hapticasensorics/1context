# Wiki Company Orchestrator V1

This folder describes how the default wiki company runs.

It is not the company itself. The company definition lives in
`context-engine/packs/wiki-company-v1/`. This orchestrator says how Context
Engine should plan packets, wake agents, route reports, require receipts, and
publish accepted wiki changes.

## Relationship

```text
wiki-company-v1                 what company exists
wiki-company-orchestrator-v1    how that company runs
onecontext-context-engine       interpreter and executor
```

## Files

```text
orchestrator.toml   id, pack id, active harness, modes
phases.toml         high-level update lifecycle
packet-policy.toml  recent-first and backfill packet rules
routing.toml        role to talk/mail destinations
receipts.toml       required proof before a turn counts as done
```

This folder intentionally does not contain prompts, agent identities, or job
contracts. Keep those in the pack. It also does not contain run history; for the
current release slice, mail records what happened until Postgres/Timescale owns
rich execution history.
