# Scope: engine vs content

The wiki-engine is the renderer, theme, and deterministic browser chrome. It
does not contain wiki content and it does not own product publication policy. In
1Context, content lives in `wiki/`, starting with `wiki/menu/**/family.toml`
workspaces and their source/talk/generated files. The Python memory-core runtime
discovers those families, invokes the engine, records evidence, and writes the
portable site artifacts used by the macOS app.

## In the engine

- **Renderer** — markdown → themed HTML and clean markdown twins.
- **Theme** — the CSS that makes pages look like pages
  (`wiki-engine/theme/css/theme.css`).
- **Chrome JS** — header, TOC drawer, view toggles, talk-page parser,
  Agent view rendering, copy-as-md buttons, scroll-direction header
  reveal, customizer drawer, search modal, AI panel
  (`wiki-engine/theme/js/enhance.js`).
- **Templates** — shared HTML shells (header, footer, page wrapper,
  talk-page wrapper), currently implemented in `src/renderer/template.mjs`.
- **Storage adapters** — planned boundary that lets the engine read content
  from different backends. Current runtime integration is static markdown on
  disk through Python family discovery.
- **Tools** — renderers, discovery-file generators, link-graph validators,
  audience-tier filters, and deterministic wiki generators
  (`wiki-engine/tools/`).
- **Schemas** — the formal frontmatter spec, the talk-page format spec,
  the docs-index.json schema.
- **Internal CLI** — planned only if it becomes cleaner than the current
  Python-owned `1context-memory-core wiki ...` command surface.

## In content (not the engine)

- Page sources (`*.md` with frontmatter)
- Talk pages (`*.talk.md`)
- Static assets (images, fonts referenced by content)
- Site-specific config (deployment URLs, branding tokens, family manifests, the
  `audience` tier policy for that site)
- The actual prose of every wiki article
- Publication policy, such as excluding operator-only goal pages from the
  installed user wiki

## The boundary in one sentence

If two different 1Context wikis would share it, it belongs in the
engine. If only this specific wiki has it, it belongs in content.

## Storage-adapter philosophy

The current engine reads files passed by the Python runtime. A future engine
adapter interface can make that boundary more explicit. The default adapter
should remain static markdown on disk; planned adapters may include:

- `PuterDBAdapter` — reads from Puter's KV store, lets a wiki run
  without checked-in markdown files
- `BookStackAdapter` — reads from a BookStack instance's REST API
  (resurrects the old BookStack module use-case as one option among
  several rather than the only one)
- `GitAdapter` — reads from a remote git repo's working tree
- `MemoryAdapter` — for tests

Adapters return content in a normalized shape (frontmatter dict +
markdown body string + slug). The renderer doesn't know or care
where it came from.
