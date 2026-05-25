# writeObjects Index Profile

Scope: Perception DB migrations 004-008 and the `write_objects` duplicate,
mutable update, edge, and receipt paths.

Dev DB snapshot used for this pass:

- `perception.source_records`: 3014 rows
- `perception.objects`: 3014 rows
- `perception.object_edges`: 2 rows
- Applied migrations: 001 through 011

## Writer Query Shapes

The common `write_objects` source identity paths all join a chunk temp table to
`perception.source_records` by `source_id` and `source_record_key`:

```sql
-- Exact duplicate lookup.
SELECT stage.ordinal
FROM write_objects_stage stage
JOIN perception.source_records source_records
  ON source_records.source_id = stage.source_id
 AND source_records.source_record_key = stage.source_record_key
 AND source_records.source_record_hash = stage.source_record_hash;

-- Conflict / mutable lookup.
SELECT source_records.source_record_id
FROM perception.source_records source_records
JOIN write_objects_stage stage
  ON stage.source_id = source_records.source_id
 AND stage.source_record_key = source_records.source_record_key
 AND stage.source_record_hash <> source_records.source_record_hash;

-- Receipt lookup.
SELECT stage.ordinal, source_records.object_id, source_records.object_event_start
FROM write_objects_stage stage
JOIN perception.source_records source_records
  ON source_records.source_id = stage.source_id
 AND source_records.source_record_key = stage.source_record_key
 AND source_records.source_record_hash = stage.source_record_hash
ORDER BY stage.ordinal;
```

The edge target lookup joins by target object:

```sql
SELECT target.object_event_start
FROM write_object_edges_stage edges
JOIN perception.source_records target
  ON target.user_id = edges.user_id
 AND target.object_id = edges.to_object_id;
```

## Current Index Coverage

Migration 004 already creates:

- `UNIQUE (source_id, source_record_key)`
- `UNIQUE (object_id)`
- `source_records_user_time_idx`
- `source_records_source_seen_idx`
- `source_records_series_time_idx`

Migration 007 already creates directional edge read indexes:

- `object_edges_from_idx` on `(user_id, from_object_id, edge_kind)`
- `object_edges_to_idx` on `(user_id, to_object_id, edge_kind)`

## EXPLAIN Findings

On a 200-row staged duplicate batch, PostgreSQL planned the exact duplicate,
conflict, mutable, and receipt lookups as nested loops with an inner-unique
index scan on `source_records_source_id_source_record_key_key`. The edge target
lookup used `source_records_object_id_key`.

A temporary candidate covering index:

```sql
CREATE INDEX source_records_write_identity_probe_idx
ON perception.source_records (source_id, source_record_key, source_record_hash)
INCLUDE (source_record_id, object_id, object_event_start, user_id, series_id, kind);
```

changed exact duplicate and receipt lookups to index-only scans and reduced
estimated cost, but it duplicated the wide source identity key path and added
the hash plus several included columns to every source record write. Because
`source_id, source_record_key` is unique, the hash column does not improve
lookup selectivity; it only avoids some heap reads after the existing unique
lookup. Duplicate writes also update `last_seen_at` and `seen_count`, so the
heap page is already on the hot path for exact duplicates.

## Recommendation

Do not add a new migration index for this pass.

The current narrow unique indexes cover the writer's exact duplicate, mutable
conflict, edge target, and receipt lookups with one-row index probes. A covering
source-record hash index is a reasonable future candidate only if production
profiles show the final receipt lookup or exact duplicate temp-table build
dominating write time after larger backfills. At that point prefer measuring:

```sql
EXPLAIN (ANALYZE, BUFFERS)
SELECT stage.ordinal, source_records.object_id, source_records.object_event_start
FROM write_objects_stage stage
JOIN perception.source_records source_records
  ON source_records.source_id = stage.source_id
 AND source_records.source_record_key = stage.source_record_key
 AND source_records.source_record_hash = stage.source_record_hash
ORDER BY stage.ordinal;
```

before accepting the extra write amplification.
