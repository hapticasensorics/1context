# wiki-engine

A static wiki engine for AI-collaborative knowledge bases.

Takes markdown source files (with frontmatter) and produces:
- themed HTML for human readers,
- clean `.md` twins for agent consumers,
- LKML-flavored talk pages for coordination,
- agent-discovery surfaces (`llms.txt`, `llms-full.txt`, `docs-index.json`),
- per-page metadata in a structured JSON manifest.

Designed to be storage-adapter-agnostic — current adapter is
static-markdown-on-disk; planned adapters include Puter DB and a
generic key-value backend.

## Status

This is now a first-class renderer package. It is deliberately outside the
memory system: agents and runtime services own source files, proposals, and
policy, while this package turns a `user-wiki/source` tree into a portable
static site.

The structured entry point is `tools/render-site.mjs`. It takes explicit roots,
renders every configured source page and talk folder into a staging directory,
writes `.1context/route-manifest.json` and `.1context/content-index.json`, and
returns a result JSON envelope for Swift or harness callers.

Current verification loop:

```sh
npm ci
npm test
node tools/render-site.mjs --source-root /path/to/1Context/user-wiki/source --output /tmp/1context-wiki-site --result-json /tmp/1context-wiki-render.json
node tools/render-to-dir.mjs tests/fixtures/for-you-2026-04-26.md /tmp/1context-wiki-engine-fixture
node tools/render-to-dir.mjs tests/fixtures/basic.talk /tmp/1context-wiki-engine-talk-fixture
```

Talk convention banners are loaded from source frontmatter via
`talk_conventions_path` or `talk_conventions_file`. `talk_conventions` remains
the display key/kind; it is no longer mapped through private experiment paths.

## Layout

```
wiki-engine/
├── theme/
│   ├── css/theme.css     ← all engine styling
│   └── js/enhance.js     ← chrome interactivity
├── tools/                ← render-to-dir.mjs and deterministic helpers
├── tests/fixtures/       ← small source + talk-folder render fixtures
├── schemas/              ← render result, route, content, and twin schemas
├── docs/architecture.md  ← layered model + key types
├── README.md             ← you are here
├── SCOPE.md              ← engine vs content boundary
├── ROADMAP.md            ← current renderer/package priorities
└── CHANGELOG.md          ← SemVer track
```

## See also

- [SCOPE.md](./SCOPE.md) — what belongs in the engine vs in content
- [ROADMAP.md](./ROADMAP.md) — what's next
- [docs/architecture.md](./docs/architecture.md) — design sketch
