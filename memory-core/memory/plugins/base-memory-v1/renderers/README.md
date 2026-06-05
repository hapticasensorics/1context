# Renderers

This folder records archived Python renderer contracts from the prototype
memory-core era.

The active wiki-update path now backfills Codex and Claude session history into
Perception DB with `onecontext-memoryd`, queries that API, and injects a
Perception source packet into the hired agent prompt and harness birth request.

Do not add new LanceDB/LakeStore renderers here. New durable source windows
should be modeled through Perception DB reads plus explicit agent-harness
context-injection proof.
