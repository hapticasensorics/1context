# 1Context Memory Runtime Artifact Contract

The shipped app may include a `memory-runtime` artifact, but it must never
include the `memory-core` source checkout.

The current artifact is intentionally small:

- `manifest.json`: generated release manifest for the artifact contents.
- `wiki-site/`: static local wiki seed copied into app-owned support storage.

Allowed files are static HTML and JSON only. The artifact must not contain
Python, Node, shell scripts, source checkouts, generated developer wiki routes,
plugin prompts, virtual environments, caches, or local machine paths.

Release builds produce the artifact through
`scripts/build-memory-runtime-artifact.sh` and copy the result into
`1Context.app/Contents/Resources/memory-runtime`.
