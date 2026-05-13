# Architecture sketch

A working sketch — concrete enough to start building against,
loose enough to revise as we discover constraints.

Current integration note: the Python memory-core runtime owns
`wiki/menu/**/family.toml` discovery, policy filtering, evidence, route records,
and site publication. The Node wiki-engine owns deterministic rendering of the
source or talk inputs it is given.

## Layered model

```
┌──────────────────────────────────────────────────────────┐
│ Outputs                                                  │
│  HTML  ·  .md twin  ·  talk.html  ·  llms.txt           │
│  llms-full.txt  ·  docs-index.json                       │
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
│ Storage adapter                                          │
│  StaticMarkdownAdapter  ·  PuterDBAdapter (planned)      │
│  BookStackAdapter (planned)  ·  GitAdapter (planned)     │
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

1. Python discovers wiki families from `wiki/menu/**/family.toml`.
2. For each family, Python selects source and talk inputs according to family
   policy.
3. Python invokes `wiki-engine/tools/render-to-dir.mjs` for each selected input.
4. The engine emits themed HTML, clean markdown twins, route helper files, and
   static browser assets.
5. Python writes `render-manifest.json` with family, engine, input, output,
   tier-source, route, and check records.
6. Python records render evidence and writes portable site files for the local
   macOS web shell.

## Future adapter build flow

1. **Discovery.** Adapter `listPages()` → set of slugs to render.
2. **Read.** For each slug, adapter `readPage(slug)` → `Page` with
   frontmatter + body.
3. **Audience filter.** Drop pages whose `audience` doesn't match
   the build target. Strip `<!-- audience:other -->` blocks from
   surviving pages.
4. **Render.** For each page:
   a. `marked` parses body into AST
   b. Custom directives expand (`:::infobox`, `:::main-article`,
      `:::see-also`)
   c. TOC built from H2/H3
   d. Page assembled into themed HTML via templates
   e. Clean `.md` twin written (frontmatter + body, no chrome)
5. **Talk pages.** For each `*.talk.md`, parse + render via the
   existing talk parser.
6. **Discovery files.** Generate `llms.txt`, `llms-full.txt`,
   `docs-index.json` from the rendered set.
7. **Validate.** Link-graph check, frontmatter schema check,
   anchor stability check.
8. **Emit.** Write everything to `dist/`.

## Open architectural questions

- Where does the renderer's custom-directive registry live? Probably
  `wiki-engine/src/renderer/directives/` with one file per directive
  for readability.
- Does the engine ship as ESM, CJS, or both? Probably ESM-only
  (Node 20+).
- Do we expose the `Adapter` interface in TypeScript and let
  third-party adapters be regular npm packages? Probably yes — that's
  how MkDocs, Docusaurus, etc. handle plugins.
- How does dev mode (hot reload) interact with adapters? For static markdown,
  a file watcher is enough. Other adapters may need polling or webhooks. The
  adapter interface probably needs an optional `subscribe()` method.
