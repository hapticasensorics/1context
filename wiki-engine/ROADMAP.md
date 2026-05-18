# Roadmap

The wiki engine is a private renderer package with explicit input and output
roots. It renders markdown sources and talk folders; callers own source edits,
policy, queueing, and publication.

## Done

- Markdown to themed HTML and clean markdown twins.
- LKML-flavored talk-folder rendering.
- 1Context theme, header chrome, view toggles, search modal, and agent surfaces.
- Custom directives for infoboxes, main-article links, see-also blocks, and
  audience markers.
- Whole-site rendering through `tools/render-site.mjs`.
- Route and content manifests under `.1context/` for Swift validation and
  agent indexing.
- Portable site artifacts consumed by the macOS shell and local web server.

## Next

- Finish audience-tier validation: fail-closed defaults, link-graph gates, and
  explicit public/internal/private render receipts.
- Tighten route and manifest checks so generated site publication can reject
  missing pages, broken internal links, and policy-excluded operator pages.
- Add a narrow adapter boundary only when a second storage backend exists; until
  then, keep static markdown as the simple contract.
- Keep the Node renderer as an internal CLI unless publishing it reduces
  release friction.

## Non-goals

- General-purpose static-site generation. The engine stays wiki-flavored:
  talk pages, link graphs, audience tiers, agent surfaces, and deterministic
  manifests.
- Browser-based rich editing. Source remains files, agents, and reviewable
  proposals.
- Real-time collaborative editing. Coordination remains async through talk pages
  and future proposal workflows.
