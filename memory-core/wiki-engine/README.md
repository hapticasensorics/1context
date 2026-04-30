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

Imported into 1Context as a first-class memory-core subsystem. This package owns
deterministic wiki rendering and generation; the Python `onectx` runtime owns
ports, storage, state machines, jobs, evidence, and agent hiring.

This first import intentionally keeps the e08 renderer standalone. The next
step is adapting it from flat experiment paths to `wiki/menu/**/family.toml`
family workspaces.

Current verification loop:

```sh
npm install
npm test
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
├── schemas/              ← render manifest schema
├── docs/architecture.md  ← layered model + key types
├── README.md             ← you are here
├── SCOPE.md              ← engine vs content boundary
├── ROADMAP.md            ← P1–P7
└── CHANGELOG.md          ← SemVer track
```

## See also

- [SCOPE.md](./SCOPE.md) — what belongs in the engine vs in content
- [ROADMAP.md](./ROADMAP.md) — what's next
- [docs/architecture.md](./docs/architecture.md) — design sketch
