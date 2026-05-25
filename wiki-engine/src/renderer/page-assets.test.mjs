import test from 'node:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ENGINE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function writeFixture(path, contents) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents);
}

test('render-site publishes page-local assets beside the rendered route', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-page-assets-'));
  try {
    const sourceRoot = resolve(tmp, 'user-wiki/source');
    const pageDir = resolve(sourceRoot, 'families/reference/topics/source');
    const output = resolve(tmp, 'site');
    const resultJson = resolve(tmp, 'render-result.json');

    writeFixture(
      resolve(pageDir, 'topics.md'),
      `---
page_id: topics
title: Topics
slug: topics
route: /topics
section: reference
access: private
---

# Topics

![Current topic taxonomy sketch](./topics.assets/topic-map.png)

[Download supporting data](./topics.assets/supporting-data.csv)
`
    );
    writeFixture(resolve(pageDir, 'topics.assets/topic-map.png'), 'fake image bytes');
    writeFixture(resolve(pageDir, 'topics.assets/supporting-data.csv'), 'topic,count\nrust,1\n');

    execFileSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-site.mjs'),
      '--source-root',
      sourceRoot,
      '--output',
      output,
      '--result-json',
      resultJson,
    ], { cwd: ENGINE_ROOT, stdio: 'pipe' });

    assert.ok(existsSync(resolve(output, 'topics.assets/topic-map.png')));
    assert.ok(existsSync(resolve(output, 'topics.assets/supporting-data.csv')));

    const html = readFileSync(resolve(output, 'topics.html'), 'utf8');
    assert.match(html, /src="\/topics\.assets\/topic-map\.png"/);
    assert.match(html, /href="\/topics\.assets\/supporting-data\.csv"/);

    const result = JSON.parse(readFileSync(resultJson, 'utf8'));
    assert.ok(result.assets.includes('topics.assets/topic-map.png'));
    assert.ok(result.assets.includes('topics.assets/supporting-data.csv'));
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-site emits citeable references for assets, links, and code blocks', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-reference-index-'));
  try {
    const sourceRoot = resolve(tmp, 'user-wiki/source');
    const pageDir = resolve(sourceRoot, 'families/reference/topics/source');
    const targetDir = resolve(sourceRoot, 'families/reference/target/source');
    const output = resolve(tmp, 'site');
    const resultJson = resolve(tmp, 'render-result.json');

    writeFixture(
      resolve(pageDir, 'topics.md'),
      `---
page_id: topics
title: Topics
slug: topics
route: /topics
section: reference
access: private
---

# Topics

![Current topic taxonomy sketch](./topics.assets/topic-map.png)

[Download supporting data](./topics.assets/supporting-data.csv)

[Target page](/target)

[OpenAI](https://openai.com)

Publish evidence is citeable.[^publish-proof]

\`\`\`swift
let status = try await wiki.publish(trigger: "agent")
\`\`\`

[^publish-proof]: Publish receipt archived in the local run folder.
`
    );
    writeFixture(
      resolve(targetDir, 'target.md'),
      `---
page_id: target
title: Target
slug: target
route: /target
section: reference
access: private
---

# Target
`
    );
    writeFixture(resolve(pageDir, 'topics.assets/topic-map.png'), 'fake image bytes');
    writeFixture(resolve(pageDir, 'topics.assets/supporting-data.csv'), 'topic,count\nrust,1\n');

    execFileSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-site.mjs'),
      '--source-root',
      sourceRoot,
      '--output',
      output,
      '--result-json',
      resultJson,
    ], { cwd: ENGINE_ROOT, stdio: 'pipe' });

    const result = JSON.parse(readFileSync(resultJson, 'utf8'));
    assert.equal(result.reference_index, '.1context/reference-index.json');
    assert.equal(result.page_asset_count, 2);
    assert.equal(result.code_block_count, 1);
    assert.equal(result.citation_count, 1);
    assert.equal(result.link_count, 4);

    const index = JSON.parse(readFileSync(resolve(output, '.1context/reference-index.json'), 'utf8'));
    assert.equal(index.schema_version, 'wiki.reference-index.v1');
    assert.equal(index.asset_count, 2);
    assert.equal(index.link_count, 4);
    assert.equal(index.code_block_count, 1);
    assert.equal(index.citation_count, 1);
    assert.equal(index.reference_count, 8);

    const image = index.assets.find((asset) => asset.filename === 'topic-map.png');
    assert.equal(image.kind, 'image');
    assert.equal(image.citation_uri, 'user-wiki://page/topics/assets/topic-map.png');
    assert.equal(image.published_href, '/topics.assets/topic-map.png');
    assert.equal(image.referenced, true);
    assert.ok(image.referenced_from[0].line_start > 0);

    const data = index.assets.find((asset) => asset.filename === 'supporting-data.csv');
    assert.equal(data.kind, 'file');
    assert.equal(data.media_type, 'text/csv');

    const assetLink = index.links.find((link) => link.href === './topics.assets/topic-map.png');
    assert.equal(assetLink.kind, 'image_link');
    assert.equal(assetLink.target_kind, 'asset');
    assert.equal(assetLink.target_path, 'topics.assets/topic-map.png');

    const internalLink = index.links.find((link) => link.href === '/target');
    assert.equal(internalLink.target_kind, 'internal_route');

    const webLink = index.links.find((link) => link.href === 'https://openai.com');
    assert.equal(webLink.target_kind, 'external_web');

    const [code] = index.code_blocks;
    assert.equal(code.language, 'swift');
    assert.equal(code.page_id, 'topics');
    assert.match(code.citation_uri, /^user-wiki:\/\/page\/topics\/code\/code-topics-001-/);
    const html = readFileSync(resolve(output, 'topics.html'), 'utf8');
    assert.match(html, new RegExp(`id="${code.id}"`));
    assert.match(html, new RegExp(`data-1context-code-id="${code.id}"`));
    assert.match(html, /<sup id="cite-ref-topics-1-1" class="opctx-footnote-ref"/);
    assert.match(html, /<section class="opctx-references" id="opctx-references"/);

    const [citation] = index.citations;
    assert.equal(citation.kind, 'citation');
    assert.equal(citation.page_id, 'topics');
    assert.equal(citation.label, 'publish-proof');
    assert.equal(citation.number, 1);
    assert.equal(citation.html_anchor, '/topics#cite-note-topics-1');
    assert.equal(citation.citation_uri, 'user-wiki://page/topics/citations/cite-note-topics-1');

    const routeManifest = JSON.parse(readFileSync(resolve(output, '.1context/route-manifest.json'), 'utf8'));
    const contentIndex = JSON.parse(readFileSync(resolve(output, '.1context/content-index.json'), 'utf8'));
    assert.equal(routeManifest.reference_index.reference_count, 8);
    assert.equal(routeManifest.reference_index.citation_count, 1);
    assert.equal(contentIndex.reference_index.reference_count, 8);
    assert.equal(contentIndex.reference_index.citation_count, 1);
    assert.ok(contentIndex.export_allowlist.includes('.1context/reference-index.json'));
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-site indexes markdown reference-style links', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-reference-style-links-'));
  try {
    const sourceRoot = resolve(tmp, 'user-wiki/source');
    const pageDir = resolve(sourceRoot, 'families/reference/topics/source');
    const targetDir = resolve(sourceRoot, 'families/reference/target/source');
    const output = resolve(tmp, 'site');
    const resultJson = resolve(tmp, 'render-result.json');

    writeFixture(
      resolve(pageDir, 'topics.md'),
      `---
page_id: topics
title: Topics
slug: topics
route: /topics
section: reference
access: private
---

# Topics

![Reference topic image][topic-image]

[Download supporting data][topic-data]

[Target page][target-ref]

[OpenAI][openai-ref]

[Target again][target-ref]

[topic-image]: ./topics.assets/topic-map.png
[topic-data]: ./topics.assets/supporting-data.csv
[target-ref]: /target
[openai-ref]: https://openai.com
`
    );
    writeFixture(
      resolve(targetDir, 'target.md'),
      `---
page_id: target
title: Target
slug: target
route: /target
section: reference
access: private
---

# Target
`
    );
    writeFixture(resolve(pageDir, 'topics.assets/topic-map.png'), 'fake image bytes');
    writeFixture(resolve(pageDir, 'topics.assets/supporting-data.csv'), 'topic,count\nrust,1\n');

    execFileSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-site.mjs'),
      '--source-root',
      sourceRoot,
      '--output',
      output,
      '--result-json',
      resultJson,
    ], { cwd: ENGINE_ROOT, stdio: 'pipe' });

    const result = JSON.parse(readFileSync(resultJson, 'utf8'));
    assert.equal(result.page_asset_count, 2);
    assert.equal(result.link_count, 5);
    assert.equal(result.reference_count, 7);

    const index = JSON.parse(readFileSync(resolve(output, '.1context/reference-index.json'), 'utf8'));
    assert.equal(index.link_count, 5);
    assert.equal(index.links.filter((link) => link.href === '/target').length, 2);

    const imageLink = index.links.find((link) => link.href === './topics.assets/topic-map.png');
    assert.equal(imageLink.kind, 'image_link');
    assert.equal(imageLink.text, 'Reference topic image');
    assert.equal(imageLink.target_kind, 'asset');

    const dataLink = index.links.find((link) => link.href === './topics.assets/supporting-data.csv');
    assert.equal(dataLink.kind, 'link');
    assert.equal(dataLink.target_kind, 'asset');

    const internalLink = index.links.find((link) => link.href === '/target');
    assert.equal(internalLink.target_kind, 'internal_route');

    const webLink = index.links.find((link) => link.href === 'https://openai.com');
    assert.equal(webLink.target_kind, 'external_web');

    const image = index.assets.find((asset) => asset.filename === 'topic-map.png');
    assert.equal(image.referenced, true);
    assert.equal(image.referenced_from[0].href, './topics.assets/topic-map.png');

    const html = readFileSync(resolve(output, 'topics.html'), 'utf8');
    assert.match(html, /src="\/topics\.assets\/topic-map\.png"/);
    assert.match(html, /href="\/topics\.assets\/supporting-data\.csv"/);
    assert.match(html, /href="\/target"/);
    assert.match(html, /href="https:\/\/openai\.com"/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('generated home page includes the configured rolling activity feed', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-home-feed-'));
  try {
    const userWiki = resolve(tmp, 'user-wiki');
    const sourceRoot = resolve(userWiki, 'source');
    const pageDir = resolve(sourceRoot, 'families/reference/topics/source');
    const output = resolve(tmp, 'site');
    const resultJson = resolve(tmp, 'render-result.json');

    writeFixture(
      resolve(userWiki, 'wiki.toml'),
      `schema_version = 1
title = "1Context"

[site.home_feed]
enabled = true
max_items = 5
sources = ["page_ledger"]

[[site_pages]]
id = "home"
enabled = true
title = "Home"
route = "/"
template = "site/e08/index.md"

[[pages]]
id = "topics"
enabled = true
title = "Topics"
slug = "topics"
route = "/topics"
family_group = "reference"
family_id = "topics"
type = "topic-index"
`
    );
    writeFixture(
      resolve(userWiki, 'templates/site/e08/index.md'),
      `---
page_id: "{{ page_id }}"
title: "{{ wiki_title }}"
slug: index
route: /
section: site
access: private
---

# {{ wiki_title }}

## Recent Changes

{{ activity_feed }}
`
    );
    writeFixture(
      resolve(pageDir, 'topics.md'),
      `---
page_id: topics
title: Topics
slug: topics
route: /topics
section: reference
access: private
---

# Topics
`
    );
    writeFixture(
      resolve(userWiki, '.1context/page-ledger.jsonl'),
      '{"schema_version":1,"event":"page.body_written","page":"topics","at":"2026-05-20T12:00:00Z"}\n'
    );

    execFileSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-site.mjs'),
      '--source-root',
      sourceRoot,
      '--output',
      output,
      '--result-json',
      resultJson,
    ], { cwd: ENGINE_ROOT, stdio: 'pipe' });

    const html = readFileSync(resolve(output, 'index.html'), 'utf8');
    assert.match(html, /Recent Changes/);
    assert.match(html, /href="\/topics"/);
    assert.match(html, /body written/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});
