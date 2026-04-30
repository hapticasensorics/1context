# Architecture sketch

A working sketch — concrete enough to start building against,
loose enough to revise as we discover constraints.

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

## Build flow

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
- How does dev mode (hot reload) interact with adapters? For
  static-md, file watcher. For PuterDB, polling or webhooks. The
  adapter interface needs an optional `subscribe()` method.
