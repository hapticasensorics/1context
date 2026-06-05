# Storage

This folder contains archived prototype storage artifacts.

The active durable memory substrate is Perception DB, exposed through
`onecontext-memoryd` and backed by the local Postgres/Timescale development
database. New wiki updates and source backfills should ingest/query Codex and
Claude session history through the Perception DB protocol.

The old LanceDB `lakestore/` tree is no longer an active dependency or source of
truth. Keep it only for forensic inspection of older prototype runs until the
local archive is deleted.
