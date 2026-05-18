# Architecture sketch

A working sketch — concrete enough to start building against,
loose enough to revise as we discover constraints.

Current integration note: callers own source edits, policy, queueing, and
publication. The Node wiki-engine owns deterministic rendering of an explicit
`user-wiki/source` tree into a staging site plus machine-readable manifests.

## Layered model

```
┌──────────────────────────────────────────────────────────┐
│ Outputs                                                  │
│  HTML  ·  .md twin  ·  talk HTML                         │
│  route-manifest.json  ·  content-index.json              │
└──────────────────────────────────────────────────────────┘
                          ▲
                          │ rendered by
                          │
┌──────────────────────────────────────────────────────────┐
│ Renderer + theme                                         │
│  marked + custom directives  ·  HTML templates           │
│  CSS theme  ·  enhance.js (chrome interactivity)         │
└──────────────────────────────────────────────────────────┘
                          ▲
                          │ reads normalized content from
                          │
┌──────────────────────────────────────────────────────────┐
│ Source tree                                              │
│  user-wiki/source/families/**/source/*.md                │
│  user-wiki/source/families/**/*.talk/                    │
└──────────────────────────────────────────────────────────┘
                          ▲
                          │ source of truth lives in
                          │
┌──────────────────────────────────────────────────────────┐
│ Content                                                  │
│  Markdown files + frontmatter  ·  Talk-page siblings     │
└──────────────────────────────────────────────────────────┘
```

## Key types (sketch)

```ts
interface Frontmatter {
  title: string;
  slug: string;
  summary?: string;
  section?: string;
  tags?: string[];
  access?: 'public' | 'internal' | 'shared';
  audience?: 'public' | 'internal' | 'both';
  last_updated?: string;
  source_type?: 'authored' | 'imported';
  // ...other optional fields per the schema
}

interface Page {
  slug: string;
  frontmatter: Frontmatter;
  body: string;     // markdown
  talk?: TalkPage;  // sibling discussion, if exists
}

interface Adapter {
  listPages(): Promise<string[]>;     // slugs
  readPage(slug: string): Promise<Page>;
  writePage(slug: string, page: Page): Promise<void>;
  listRevisions?(slug: string): Promise<Revision[]>;
}
```

## Current build flow

1. Caller prepares `user-wiki/source`.
2. Caller invokes `wiki-engine/tools/render-site.mjs --source-root ... --output ...`.
3. The engine discovers source pages and talk folders under `source/families`.
4. The engine invokes the per-input renderer for each source page and talk
   folder.
5. The engine emits themed HTML, clean markdown twins, route-index helpers,
   static browser assets, `.1context/route-manifest.json`, and
   `.1context/content-index.json`.
6. The engine writes a result JSON envelope with success or failure details.
7. The caller validates the staged site and promotes it if policy allows.

## Open architectural questions

- Where does the renderer's custom-directive registry live? Probably
  `wiki-engine/src/renderer/directives/` with one file per directive
  for readability.
- Does the engine ship as ESM, CJS, or both? Probably ESM-only (Node 20+).
- Do we ever expose an adapter interface? Only after a second real source
  backend exists.
