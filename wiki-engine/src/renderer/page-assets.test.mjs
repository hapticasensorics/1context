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
