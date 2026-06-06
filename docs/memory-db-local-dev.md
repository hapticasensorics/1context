---
title: 1Context Memory DB Local Dev
slug: memory-db-local-dev
section: development
access: private
summary: "Developer lifecycle for the local Postgres + Timescale memory DB."
status: draft
last_updated: 2026-06-06
toc_enabled: true
talk_enabled: false
agent_view_enabled: true
copy_buttons_enabled: true
footer_enabled: true
---

# 1Context Memory DB Local Dev

This is the developer lifecycle for the memory DB backing the temporal object
store described in [Memory DB Design Spec](memory-db-design-spec.md).

The app and release runtime use managed Postgres by default. `onecontext-memoryd`
starts a private bundled PostgreSQL 17 cluster, connects through a private Unix
socket, creates the `onecontext` app role without superuser privileges, creates
required extensions through the bootstrap superuser, and then runs schema
bootstrap through the app role.

Build or refresh the release-grade local bundle with:

```bash
./scripts/build-managed-postgres-source.sh
```

Verify a staged bundle without allowing host build fingerprints:

```bash
./scripts/verify-managed-postgres-bundle.sh runtime/managed-postgres/macos-arm64
./scripts/smoke-managed-postgres-bundle.sh --run runtime/managed-postgres/macos-arm64
```

The verify and smoke helpers can still inspect a Homebrew-origin development
bundle with `--allow-host-fingerprints`, but that mode is not release-acceptable.

## External Dev Helper

The container helper is now explicitly for `external_postgres` development and
comparison. It is not a product runtime dependency.

```bash
./scripts/memory-db-dev.sh provision
```

That command starts a local Postgres + Timescale container and asks the Rust
memory DB CLI to create or validate the current schema from
`crates/onecontext-memory-db/schema/current.sql`.

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

For external-mode Rust writer and daemon work, export the URL from the script:

```bash
export ONECONTEXT_MEMORY_DB_URL="$(./scripts/memory-db-dev.sh url)"
export ONECONTEXT_STORAGE_BACKEND=external_postgres
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
prototype `capture.*` tables should be reset:

```bash
./scripts/memory-db-dev.sh reset
./scripts/memory-db-dev.sh provision
```

Implementation gates:

```text
schema:
  current schema creates app, perception, and search schemas only
  current schema does not create capture.* tables
  perception.series exists before perception.objects/source_records depend on it
  perception.source_records remains the dedupe/idempotency table

writer:
  memory.writeObjects chooses or creates perception.series rows
  object and source_record writes include series_id in the same transaction
  retries return stable receipts without writing removed tables

adapters:
  Codex, Claude, iMessage, browser, file, screen, audio, and metric inputs map
  their session/window/thread/path/display/stream identity to series_kind and
  series_key

reads:
  queryViewport returns summaries with series_id/series metadata
  hydrateObjects includes source_record and series provenance
  series-scoped reads return records in event order
  density defaults do not group by series_id

validation:
  focused contract tests pass
  10k write, 5k viewport, density, and hydrate paths meet local benchmark gates
```
