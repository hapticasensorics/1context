# Lakestore

Generated LanceDB/Lance tables live here.

The table files are local runtime data and ignored by git. This README is
tracked so a fresh checkout shows where the store belongs.

```text
storage/lakestore/
  events.lance/
  sessions.lance/
  artifacts.lance/
  evidence.lance/
  documents.lance/
```

Create or refresh the tables with:

```bash
uv run 1context storage init
```
