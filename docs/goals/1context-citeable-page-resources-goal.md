# Milestone: Citeable Page Resources

## Goal

Images, files, code files, fenced code blocks, hyperlinks, and Wikipedia-style
footnote citations are first-class wiki resources. Authors keep using normal
Markdown, while agents get stable resource records they can cite without
walking the filesystem.

Immediate milestone: the static renderer emits a deterministic
`.1context/reference-index.json` for page-local assets, Markdown links,
Wikipedia-style citation footnotes, and fenced code blocks, and existing
page/asset receipts expose stable citation handles for copied assets.

## Done When

- Page-local images and files render from `<slug>.assets/` and appear in the
  reference index with hashes, media types, published hrefs, and citation URIs.
- Fenced Markdown code blocks render inline with deterministic IDs and appear
  in the reference index with language, hash, source lines, and citation URIs.
- Internal page links and external web links appear in the reference index
  without weakening existing broken-internal-link diagnostics.
- Markdown footnote citations render as superscript numbers with a bottom
  References section and appear in the reference index with citation URIs.
- `wiki.asset.add` receipts expose citation-ready metadata for image, file, and
  source-code file assets.
- `wiki.reference.list` exposes published resource records to agents without
  requiring filesystem path-walking.
- Swift site validation accepts published sites that include the reference
  index.
- Docs describe the resource contract and agent loop.

## Checklist

### 1. Baseline

- [x] Page-local assets live beside page source under `<slug>.assets/`.
- [x] `wiki.asset.add` copies page-local assets and returns Markdown snippets.
- [x] The renderer copies page-local assets to the published site.
- [x] Internal link diagnostics detect broken wiki links after render.

### 2. Renderer Index

- [x] Emit `.1context/reference-index.json` from `render-site.mjs`.
- [x] Index page-local image/file assets with hashes and citation URIs.
- [x] Index Markdown links as internal, external, or asset references.
- [x] Render and index Wikipedia-style Markdown footnote citations.
- [x] Index fenced code blocks with deterministic IDs and source lines.
- [x] Add focused renderer tests for images, files, links, footnote citations,
  and code blocks.

### 3. Runtime Receipts

- [x] Extend `PageAssetRecord` with `citation_uri`, `kind`, and `content_role`.
- [x] Classify common source-code file extensions as text/code media.
- [x] Keep existing asset add/list behavior backward-compatible.

### 4. Validators And Schemas

- [x] Add a reference-index schema.
- [x] Add reference-index metadata to route/content manifests.
- [x] Update Swift site validation to accept and sanity-check the index when
  present.

### 5. Docs

- [x] Update the user data spec with the citeable resource contract.
- [x] Update the wiki publishing API with the agent-facing loop.
- [x] Update this checklist with proof commands and current status.

### 6. Agent API

- [x] Add `wiki.reference.list` to the Rust core and daemon.
- [x] Add Swift RPC aliases for reference listing.
- [x] Add Python client/wrapper support for reference listing.
- [x] Run focused bridge and adapter proof tests.

## Proof

- `npm test --prefix wiki-engine`
- `npm test --prefix wiki-engine -- page-assets index`
- `cargo test -q -p onecontext-wiki-core`
- `cargo test -q -p onecontext-wiki-daemon -- --nocapture`
- `swift test --package-path macos --filter WikiRenderCoordinatorTests`
- `swift test --package-path macos --filter WikiCoreRPCBridgeTests`
- `uv run --project memory-core --with pytest pytest memory-core/tests/test_wiki_core_client.py -q`
- `git diff --check`

## Notes

- Current status: the static reference index, Wikipedia-style footnote
  references, code anchors, asset citation fields, schemas, Swift validation,
  Rust/Swift/Python reference-list adapters, docs, and focused tests are in
  place.
- Manual dogfood on 2026-05-21 used a disposable runtime seeded from
  `runtime/1Context`, the dev `onecontext-wiki` binary, and the wiki commands
  `ensure`, `page-create-all`, `page-create`, `asset-add`, `page-write-body`,
  `publish-status`, `publish`, and `reference-list`.
- The dogfood covered an existing configured page (`/topics`), a new top-level
  page (`/media-lab`), a new nested route (`/labs/nested-lab`), and a
  references page (`/reference-lab`) with inline PNG/SVG images, CSV/text
  download links, Swift/TypeScript source-file links, fenced
  Swift/TypeScript/JSON/Python code blocks, internal links, external web links,
  Markdown reference-style image/link definitions, and Wikipedia-style
  footnote citations.
- The first reference-style pass exposed that `[text][label]` and
  `![alt][label]` links rendered in HTML but were missing from the reference
  index. The renderer now scans reference definitions and indexes each explicit
  or collapsed reference-style usage at the use site.
- Publish completed with zero broken internal links. `wiki.reference.list`
  reported 98 total references after the footnote pass: 12 assets, 77 links, 5
  code blocks, and 4 citation footnotes. Scoped reference listing returned the
  expected page records for `/topics`, `/media-lab`, `/labs/nested-lab`, and
  `/reference-lab`.
- Browser verification over a local static server confirmed that the published
  pages load the image assets, nested-route asset paths, file links, external
  links, deterministic code-block anchors, superscript citation refs, and the
  bottom References section.
- Immediate next step: tighten any receipt naming that feels awkward in actual
  agent use.
