# Storage Module

This module now provides small shared helpers such as stable ids and UTC
timestamps. The former Python `LakeStore` API is archived and intentionally
raises if called.

Use the mature 1Context Perception DB path instead:

```text
onecontext-memoryd protocol memory.ingestSources
onecontext-memoryd protocol memory.queryViewport
```

The Rust/macOS Perception DB stack owns durable agent-session ingestion,
querying, source identity, and API compatibility. Python memory-core should stay
a thin coordinator around those surfaces, not a parallel database.
