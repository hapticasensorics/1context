# Cleanup Policy

1Context is pre-release. Cleanup work should optimize for the current product
contract, not historical compatibility.

## Non-Negotiable Rules

- No backwards-compatibility shims.
- No migration runners.
- No legacy alias tables.
- No scaffold states or scaffold receipts as success cases.
- No repair paths for old local data.
- No synthetic proofs as product evidence.
- No tests whose only purpose is preserving stale behavior.

When stale behavior is referenced only by tests, docs, scripts, fixtures, or
build wiring, remove or rewrite those references. Do not recreate the old path
under names like adapter, bridge, normalizer, compatibility table, repair mode,
upgrade path, or fallback mode.

## Allowed Exceptions

- Forensic retention at explicit ingestion boundaries.
- Archived docs outside active product docs.
- Test fixtures that assert stale inputs are rejected.
- Dev-only probes that are isolated from normal product behavior.

## Cleanup Gates

Every cleanup slice should answer:

- What current product contract survives?
- What legacy, migration, fallback, scaffold, or synthetic proof path was
  removed?
- What active references were deleted or rewritten?
- What command proves the current contract still works?

The preferred sequence is delete first, then type what survives. Typing dead
systems dignifies them and keeps them alive.

Use `docs/cleanup-verification-matrix.md` for the concrete command matrix and
`docs/cleanup-deletion-program.md` for the slice order.

## Generated And Proof Artifacts

Generated artifacts belong in ignored output directories or external sample
packs. Checked-in generated snapshots, screenshots, videos, proof JSON,
`latest.json` files, and regenerated fixtures are not product contracts.

Product evidence must come from real measurements or deterministic tests, not
from fixture proof generated from expectations.

## Data Model

For pre-release local data, a fresh current schema bootstrap is enough. The
database may create a current empty dev/test schema and may fail loudly on
incompatible existing schemas. It should not upgrade, repair, backfill,
reapply, or preserve old schema shapes.

## Parsing And Contracts

Raw JSON or string parsing is allowed at external ingress, forensic retention,
and invalid-input fixtures. After ingress, product code should use typed
schemas, parser libraries, and explicit errors.

Shared contract extraction should start only after duplicated current contracts
are identified in more than one active crate or app surface.
