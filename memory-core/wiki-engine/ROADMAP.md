# Roadmap

Private-4 import note: this roadmap came from the e08 engine and is now being
adapted into the root `wiki-engine/` subsystem. The active placement and import
plan lives in `../docs/proposals/wiki-engine-private2-import.md`.

Numbered priorities. Each item gets a `[TODO]` topic on
`/wiki-engine.talk.md` so contributors can claim, propose, and
discuss without polluting this file.

## P1 — Markdown → HTML renderer ✅ imported from e08

The first renderer now lives in `src/renderer/` and produces both themed HTML
and clean markdown twins from one markdown source.

Remaining memory-core work:
- promote `tools/render-to-dir.mjs` into family/wiki-level commands
- route source/talk/generated paths through `wiki/menu/**/family.toml`
- emit render manifests/evidence for the Python runtime
- keep renderer directives, TOC, sections, family indexes, and audience streams
  deterministic

## P2 — Migrate the engine code into `wiki-engine/` ✅ done in v0.2.0

- `css/theme.css` → `wiki-engine/theme/css/` ✅
- `preview/js/enhance.js` → `wiki-engine/theme/js/` ✅
- `tools/build-discovery-files.py` → `wiki-engine/tools/` ✅
- site deploy/promotion scripts were intentionally left behind during the
  memory-core import; runtime/evidence integration will own that boundary
- `vite.config.mjs` stays at repo root (vite expects it there;
  defer move until P6 packages the engine as a CLI)
- All import paths + script-relative paths updated

## P3 — Storage-adapter interface

Carve the abstraction so the engine doesn't assume markdown-on-disk.

- Define the `Adapter` shape (`list_pages()`, `read_page(slug)`,
  `write_page(slug, content)`, `list_revisions(slug)`)
- Implement `StaticMarkdownAdapter` (extracts current behavior)
- Implement `PuterDBAdapter` so a wiki can run without checked-in
  markdown
- Implement `MemoryAdapter` for tests

## P4 — Private-4 family workspace adapter

Once P2 + P3 land, finish the engine/content separation for the memory core:

- discover `wiki/menu/**/family.toml`
- read source pages, talk folders, templates, and conventions from `wiki/`
- write generated outputs under each family workspace
- emit machine-readable manifests that `src/onectx/wiki/` can record as
  evidence

## P5 — Audience-tier pipeline

From the audience-tier research synthesis: build-time filter using
frontmatter `audience:` field + `<!-- audience:internal -->` markers,
fail-closed defaults, link-graph CI gate.

## P6 — CLI

Once P1–P5 land, package as a CLI:

- `wiki-engine init` — scaffold a new wiki
- `wiki-engine build` — produce `dist/`
- `wiki-engine dev` — local dev server with hot reload
- `wiki-engine deploy <env>` — push to a configured target

## P7 — Engine repo extraction

Extract `wiki-engine/` into its own repo
(`hapticasensorics/wiki-engine`). Publish as an npm package. Content
repos depend on the published version.

## Non-goals (intentionally out of scope)

- Becoming a general-purpose static site generator. We're not
  competing with Hugo. We're opinionated about wiki-flavored
  features (talk pages, link graph, audience tiers, agent surfaces)
  and aggressively narrow about everything else.
- WYSIWYG editing. Source is markdown; editing is via files /
  agents / PRs. No browser-based rich editor in scope.
- Real-time collaborative editing. Async via PR + talk pages.
