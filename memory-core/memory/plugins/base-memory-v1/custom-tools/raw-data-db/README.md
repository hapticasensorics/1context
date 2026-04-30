# Raw Data DB Tool

`raw_data.query` reads a local SQLite database and returns JSON rows. It is intentionally read-only.

The database path comes from `ONECTX_RAW_DATA_DB`. If that env var is not set, the command looks for:

```text
memory/runtime/raw-data/raw-data.sqlite
```

Example:

```bash
printf '{"sql":"select * from observations limit 5","limit":5}' \
  | ONECTX_RAW_DATA_DB=memory/runtime/raw-data/raw-data.sqlite \
    uv run python memory/plugins/base-memory-v1/custom-tools/raw-data-db/query.py
```

This is a portable implementation example. A host daemon can replace the command with a stronger native binding later while keeping the same `raw_data.query` contract.
