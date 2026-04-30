# Scope: engine vs content

The wiki-engine is the build pipeline + theme + tools. It does not
contain wiki content. In 1Context, content lives in `wiki/`, starting with
`wiki/menu/**/family.toml` workspaces and their source/talk/generated files.

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
- **Storage adapters** — the boundary that lets the engine read content
  from different backends. Initial adapter: static markdown on disk.
  Planned: Puter DB, BookStack API, generic KV.
- **Tools** — renderers, discovery-file generators, link-graph validators,
  audience-tier filters, and deterministic wiki generators
  (`wiki-engine/tools/`).
- **Schemas** — the formal frontmatter spec, the talk-page format spec,
  the docs-index.json schema.
- **CLI** — `wiki-engine build`, `wiki-engine dev`, `wiki-engine
  deploy` (planned, P6).

## In content (not the engine)

- Page sources (`*.md` with frontmatter)
- Talk pages (`*.talk.md`)
- Static assets (images, fonts referenced by content)
- Site-specific config (deployment URLs, branding tokens, family manifests, the
  `audience` tier policy for that site)
- The actual prose of every wiki article

## The boundary in one sentence

If two different 1Context wikis would share it, it belongs in the
engine. If only this specific wiki has it, it belongs in content.

## Storage-adapter philosophy

The engine reads content through an `Adapter` interface. The default
adapter is `StaticMarkdownAdapter` — reads `.md` from a local
directory. Planned adapters:

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
