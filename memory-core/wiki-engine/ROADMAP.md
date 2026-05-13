# Roadmap

The wiki engine is now a private renderer package inside memory-core. It renders
markdown sources and talk folders; the Python runtime owns family discovery,
policy, evidence, and site publication.

## Done

- Markdown to themed HTML and clean markdown twins.
- LKML-flavored talk-folder rendering.
- 1Context theme, header chrome, view toggles, search modal, and agent surfaces.
- Custom directives for infoboxes, main-article links, see-also blocks, and
  audience markers.
- Family workspace integration through
  `1context-memory-core wiki render --json`.
- Per-family render manifests and route records for the Python runtime.
- Portable site artifacts consumed by the macOS shell.

## Next

- Turn the current file-oriented renderer calls into a clear adapter boundary:
  `list_pages`, `read_page`, `write_page`, and optional revision/freshness
  methods.
- Keep `StaticMarkdownAdapter` as the default while leaving room for durable
  storage adapters later.
- Finish audience-tier validation: fail-closed defaults, link-graph gates, and
  explicit public/internal/private render receipts.
- Tighten route and manifest checks so generated site publication can reject
  missing pages, broken internal links, and policy-excluded operator pages.
- Package the Node renderer as a real internal CLI only when it reduces
  friction compared with the current Python-owned command surface.
- Defer external repo extraction until the public Swift shell and memory-core
  contract are stable enough for a published package boundary.

## Non-goals

- General-purpose static-site generation. The engine stays wiki-flavored:
  talk pages, link graphs, audience tiers, agent surfaces, and deterministic
  manifests.
- Browser-based rich editing. Source remains files, agents, and reviewable
  proposals.
- Real-time collaborative editing. Coordination remains async through talk pages
  and future proposal workflows.
