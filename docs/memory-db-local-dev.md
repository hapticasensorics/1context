---
title: 1Context Memory DB Local Dev
slug: memory-db-local-dev
section: development
access: private
summary: "Developer lifecycle for the local Postgres + Timescale memory DB."
status: draft
last_updated: 2026-05-24
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Memory DB Local Dev

This is the dev-only lifecycle for the memory DB backing the temporal object
store described in [Memory DB Design Spec](memory-db-design-spec.md).

The goal is a boring local database target for the Rust writer, daemon, viewer,
and tests:

```bash
./scripts/memory-db-dev.sh provision
```

That command starts a local Postgres + Timescale container, creates the current
schema from `crates/onecontext-memory-db/schema/current.sql` when the database
is empty, rejects deleted migration/capture-era schemas, and verifies the
central `perception.*` tables.

## Requirements

Install one container engine:

```bash
brew install docker colima
colima start
```

Docker Desktop or Podman also work; the script auto-detects `docker` first and
then `podman`.

The script executes `psql` inside the container for status and verification, so
host `psql` is not required for the dev database lifecycle.

## Defaults

```text
image:          timescale/timescaledb-ha:pg17
container:      onecontext-memory-db-dev
volume:         onecontext-memory-db-dev-pgdata
host port:      15432
database:       onecontext_memory
user:           onecontext
password:       onecontext_dev
```

The default connection string is:

```bash
postgres://onecontext:onecontext_dev@127.0.0.1:15432/onecontext_memory
```

## Commands

Start and bootstrap the current schema:

```bash
./scripts/memory-db-dev.sh provision
```

Print the connection string:

```bash
./scripts/memory-db-dev.sh url
```

Run the schema smoke check:

```bash
./scripts/memory-db-dev.sh verify
```

Run the local backfill/write benchmark against a disposable Timescale database:

```bash
./scripts/benchmark-memory-backfill.sh
```

The benchmark writes per-run summaries under:

```text
test-results/memory-db-benchmarks/<run-id>/
```

Open `psql` inside the container:

```bash
./scripts/memory-db-dev.sh psql
```

Stop the container without deleting data:

```bash
./scripts/memory-db-dev.sh stop
```

Delete the dev database container and volume:

```bash
./scripts/memory-db-dev.sh reset
```

## Environment Overrides

```bash
ONECONTEXT_MEMORY_DB_PORT=15433 \
ONECONTEXT_MEMORY_DB_NAME=onecontext_memory_agent1 \
./scripts/memory-db-dev.sh provision
```

Supported overrides:

```text
ONECONTEXT_MEMORY_DB_CONTAINER_ENGINE  docker or podman
ONECONTEXT_MEMORY_DB_IMAGE             container image
ONECONTEXT_MEMORY_DB_CONTAINER         container name
ONECONTEXT_MEMORY_DB_VOLUME            volume name
ONECONTEXT_MEMORY_DB_PORT              localhost port
ONECONTEXT_MEMORY_DB_USER              database user
ONECONTEXT_MEMORY_DB_PASSWORD          database password
ONECONTEXT_MEMORY_DB_NAME              database name
```

## Writer Handoff

For the Rust writer and daemon work, export the URL from the script:

```bash
export ONECONTEXT_MEMORY_DB_URL="$(./scripts/memory-db-dev.sh url)"
```

The writer path should insert validated perception objects into:

```text
perception.source_records
perception.objects
```

Normal reads and writes go through the Rust memory protocol and Timescale-backed
tables. Source cursors advance only after the DB writer commits.

## V2 Cutover Checklist

Perception DB V2 is a perception-only contract. Dev databases that still contain
prototype `capture.*` tables should be reset instead of carried forward:

```bash
./scripts/memory-db-dev.sh reset
./scripts/memory-db-dev.sh provision
```

Implementation gates:

```text
schema:
  current schema creates app, perception, and search schemas only
  current schema does not create or backfill capture.* tables
  perception.series exists before perception.objects/source_records depend on it
  perception.source_records remains the dedupe/idempotency table

writer:
  memory.writeObjects chooses or creates perception.series rows
  object and source_record writes include series_id in the same transaction
  retries return stable receipts without dual-writing legacy tables

adapters:
  Codex, Claude, iMessage, browser, file, screen, audio, and metric inputs map
  their session/window/thread/path/display/stream identity to series_kind and
  series_key

reads:
  queryViewport returns summaries with series_id/series metadata
  hydrateObjects includes source_record and series provenance
  series-scoped reads return records in event order
  density defaults do not group by series_id

benchmarks:
  focused contract tests pass
  10k write, 5k viewport, density, and hydrate paths meet local benchmark gates
  benchmark-memory-backfill writes through Perception DB and records durable
  summaries in test-results/memory-db-benchmarks
```
