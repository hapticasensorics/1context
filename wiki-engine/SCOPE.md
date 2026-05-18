# Scope: engine vs content

The wiki-engine is the renderer, theme, and deterministic browser chrome. It
does not contain wiki content and it does not own product publication policy. In
1Context, content lives in `runtime/1Context/user-wiki/source` for shipped
defaults and in the user's `~/1Context/user-wiki/source` runtime for local data.
Callers invoke the engine with explicit roots and decide when a successful
render is promoted.

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
- **Whole-site CLI** — `tools/render-site.mjs`, which renders a source tree into
  a staging site and writes machine-readable result metadata.
- **Tools** — renderers, discovery-file generators, link-graph validators,
  audience-tier filters, and deterministic wiki generators
  (`wiki-engine/tools/`).
- **Schemas** — route, content-index, markdown-twin, page-result, and render
  result contracts.

## In content (not the engine)

- Page sources (`*.md` with frontmatter)
- Talk folders (`*.talk/`)
- Static assets (images, fonts referenced by content)
- Site-specific config (deployment URLs, branding tokens, family manifests, the
  `audience` tier policy for that site)
- The actual prose of every wiki article
- Publication policy, such as excluding operator-only goal pages from the
  installed user wiki

## The boundary in one sentence

If two different 1Context wikis would share it, it belongs in the
engine. If only this specific wiki has it, it belongs in content.

## Source philosophy

The current contract is intentionally file-shaped: callers prepare a
`user-wiki/source` tree, invoke the engine, then validate and promote the
generated site. A richer adapter layer can wait until a second real source
backend exists.
