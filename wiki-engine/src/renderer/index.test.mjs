import test from 'node:test';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';

import { renderPage } from './index.mjs';

const ENGINE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

function writeFixtureFile(path, contents) {
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, contents);
}

async function waitForPortFile(portFile, details) {
  for (let attempt = 0; attempt < 100; attempt += 1) {
    if (existsSync(portFile)) {
      const port = Number(readFileSync(portFile, 'utf8'));
      if (port > 0) return port;
    }
    await sleep(25);
  }
  throw new Error(`serve-site did not write a port file\n${details()}`);
}

async function stopChild(child) {
  if (child.exitCode !== null || child.signalCode !== null) return;
  child.kill();
  await Promise.race([once(child, 'close'), sleep(1000)]);
}

test('parent pages emit a single H1 even when markdown starts with a title heading', () => {
  const source = readFileSync(
    resolve(ENGINE_ROOT, 'tests/fixtures/for-you-2026-04-26.md'),
    'utf8'
  );
  const { html } = renderPage(source, { slug: '2026-04-26' });

  const h1Matches = html.match(/<h1>/g) || [];
  assert.equal(h1Matches.length, 1);
  assert.match(html, /<h1>For You · Paul · Sunday, April 26, 2026<\/h1>/);
});

test('renderer escapes raw html from markdown bodies', () => {
  const source = `---
title: Script Probe
slug: script-probe
section: reference
access: private
---
# Script Probe

<script>fetch('/api/wiki/probe')</script>

<img src=x onerror="alert(1)">
`;

  const { html } = renderPage(source, { slug: 'script-probe' });

  assert.doesNotMatch(html, /<script>fetch/);
  assert.doesNotMatch(html, /<img src=x/);
  assert.match(html, /&lt;script&gt;fetch/);
  assert.match(html, /&lt;img src=x onerror=&quot;alert\(1\)&quot;&gt;/);
});

test('renderer emits Wikipedia-style footnote references', () => {
  const source = `---
title: Citation Probe
slug: citation-probe
section: reference
access: private
---
# Citation Probe

First claim has a source[^first] and repeats it[^first].

Second claim has another source[^second].

[^first]: First source with [OpenAI](https://openai.com/).
[^second]: Second source.
`;

  const { html } = renderPage(source, { slug: 'citation-probe' });

  assert.match(html, /<sup id="cite-ref-citation-probe-1-1" class="opctx-footnote-ref"/);
  assert.match(html, /<sup id="cite-ref-citation-probe-1-2" class="opctx-footnote-ref"/);
  assert.match(html, /<sup id="cite-ref-citation-probe-2-1" class="opctx-footnote-ref"/);
  assert.equal((html.match(/href="#cite-note-citation-probe-1"/g) || []).length, 2);
  assert.match(html, /<section class="opctx-references" id="opctx-references"/);
  assert.match(html, /<h2 id="references">References<\/h2>/);
  assert.match(html, /<li id="cite-note-citation-probe-1" class="opctx-reference"/);
  assert.match(html, /First source with <a href="https:\/\/openai.com\/">OpenAI<\/a>\./);
  assert.doesNotMatch(html, /\[\^first\]:/);
});

test('serve-site answers static wiki state without writable host storage', async () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-static-state-'));
  const site = resolve(tmp, 'site');
  const portFile = resolve(tmp, 'port.txt');
  mkdirSync(site, { recursive: true });
  writeFixtureFile(resolve(site, 'index.html'), '<!doctype html><title>Static State</title>');
  writeFixtureFile(resolve(site, 'deep/note.html'), '<!doctype html><title>Deep Note</title><h1>Deep Note</h1>');
  writeFixtureFile(resolve(site, 'deep/talk/attachments/proof.eml'), [
    'From: agent@example.com',
    'Subject: Attachment proof',
    '',
    'This should open as readable text in the browser.',
    '',
  ].join('\n'));
  writeFixtureFile(resolve(site, 'deep/talk/attachments/proof.txt'), 'plain text attachment proof\n');
  writeFixtureFile(resolve(site, 'deep/talk/attachments/context.md'), '# Context Attachment\n\nMarkdown attachment proof.\n');
  writeFixtureFile(resolve(site, 'deep/talk/attachments/panel.PNG'), Buffer.from([
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a,
  ]));
  writeFixtureFile(resolve(site, 'deep/note.md'), `---
title: Deep Note
slug: deep-note
route: /deep/note
section: reference
access: private
---
# Deep Note

Searchable nested note body.
`);
  writeFixtureFile(
    resolve(site, 'deep/note.talk.html'),
    '<!doctype html><title>Talk · Deep Note</title><h1>Talk · Deep Note</h1>'
  );
  writeFixtureFile(
    resolve(site, 'deep/note/talk/index.html'),
    '<!doctype html><title>Talk · Deep Note</title><base href="/deep/note/talk/"><h1>Talk · Deep Note</h1>'
  );
  writeFixtureFile(resolve(site, '.1context/content-index.json'), JSON.stringify({
    pages: [
      {
        route: '/deep/note',
        kind: 'page',
        slug: 'deep-note',
        title: 'Deep Note',
        markdown_path: 'deep/note.md',
      },
      {
        route: '/deleted-route',
        kind: 'page',
        slug: 'deleted-route',
        title: 'Deleted Route',
        markdown_path: 'deleted-route.md',
      },
    ],
  }));

  const server = spawn(process.execPath, [
    resolve(ENGINE_ROOT, 'tools/serve-site.mjs'),
    site,
  ], {
    cwd: ENGINE_ROOT,
    env: {
      ...process.env,
      HOST: '127.0.0.1',
      PORT: '0',
      PORT_FILE: portFile,
    },
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stdout = '';
  let stderr = '';
  server.stdout.on('data', (chunk) => { stdout += chunk; });
  server.stderr.on('data', (chunk) => { stderr += chunk; });

  try {
    const port = await waitForPortFile(portFile, () => `stdout:\n${stdout}\nstderr:\n${stderr}`);
    const stateUrl = `http://127.0.0.1:${port}/api/wiki/state`;

    const stateResponse = await fetch(stateUrl);
    assert.equal(stateResponse.status, 200);
    const state = await stateResponse.json();
    assert.deepEqual(state.settings, {});
    assert.deepEqual(state.bookmarks, []);
    assert.equal(state._storage.exists, false);
    assert.equal(state._storage.writable, false);
    assert.equal(state._storage.mode, 'static');

    const writeResponse = await fetch(stateUrl, {
      method: 'PATCH',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ settings: { theme: 'dark' } }),
    });
    assert.equal(writeResponse.status, 405);
    assert.equal(writeResponse.headers.get('allow'), 'GET, HEAD, OPTIONS');
    assert.equal((await writeResponse.json()).error, 'static_state_read_only');

    const stateOptionsResponse = await fetch(stateUrl, { method: 'OPTIONS' });
    assert.equal(stateOptionsResponse.status, 204);
    assert.equal(stateOptionsResponse.headers.get('allow'), 'GET, HEAD, OPTIONS');

    const searchUrl = `http://127.0.0.1:${port}/api/wiki/search?q=deep%20note`;
    const searchOptionsResponse = await fetch(searchUrl, { method: 'OPTIONS' });
    assert.equal(searchOptionsResponse.status, 204);
    assert.equal(searchOptionsResponse.headers.get('allow'), 'GET, HEAD, OPTIONS');

    const searchHeadResponse = await fetch(searchUrl, { method: 'HEAD' });
    assert.equal(searchHeadResponse.status, 200);
    assert.match(searchHeadResponse.headers.get('content-type'), /^application\/json\b/);

    const searchWriteResponse = await fetch(searchUrl, { method: 'POST' });
    assert.equal(searchWriteResponse.status, 405);
    assert.equal(searchWriteResponse.headers.get('allow'), 'GET, HEAD, OPTIONS');
    assert.equal((await searchWriteResponse.json()).error, 'method_not_allowed');

    const searchResponse = await fetch(searchUrl);
    assert.equal(searchResponse.status, 200);
    const search = await searchResponse.json();
    assert.equal(search.matches.length, 1);
    assert.equal(search.matches[0].title, 'Deep Note');
    assert.equal(search.matches[0].route, '/deep/note');

    const deletedSearchResponse = await fetch(`http://127.0.0.1:${port}/api/wiki/search?q=deleted%20route`);
    assert.equal(deletedSearchResponse.status, 200);
    assert.equal((await deletedSearchResponse.json()).matches.length, 0);

    writeFixtureFile(resolve(site, 'deleted-route.html'), '<!doctype html><title>Deleted Route</title><h1>Deleted Route</h1>');
    writeFixtureFile(resolve(site, 'deleted-route.md'), `---
title: Deleted Route
slug: deleted-route
route: /deleted-route
section: reference
access: private
---
# Deleted Route

Restored route body.
`);
    const restoredRouteResponse = await fetch(`http://127.0.0.1:${port}/deleted-route`);
    assert.equal(restoredRouteResponse.status, 200);
    const restoredSearchResponse = await fetch(`http://127.0.0.1:${port}/api/wiki/search?q=deleted%20route`);
    assert.equal(restoredSearchResponse.status, 200);
    const restoredSearch = await restoredSearchResponse.json();
    assert.equal(restoredSearch.matches.length, 1);
    assert.equal(restoredSearch.matches[0].route, '/deleted-route');

    const canonicalTalkResponse = await fetch(`http://127.0.0.1:${port}/deep/note/talk`);
    assert.equal(canonicalTalkResponse.status, 200);
    assert.match(canonicalTalkResponse.headers.get('content-type'), /^text\/html\b/);
    assert.match(await canonicalTalkResponse.text(), /Talk · Deep Note/);

    const dotTalkResponse = await fetch(`http://127.0.0.1:${port}/deep/note.talk`);
    assert.equal(dotTalkResponse.status, 404);

    const dotTalkSlashResponse = await fetch(`http://127.0.0.1:${port}/deep/note.talk/`);
    assert.equal(dotTalkSlashResponse.status, 404);

    const emlResponse = await fetch(`http://127.0.0.1:${port}/deep/talk/attachments/proof.eml`);
    assert.equal(emlResponse.status, 200);
    assert.match(emlResponse.headers.get('content-type'), /^text\/plain\b/);
    assert.match(await emlResponse.text(), /Subject: Attachment proof/);

    const txtResponse = await fetch(`http://127.0.0.1:${port}/deep/talk/attachments/proof.txt`);
    assert.equal(txtResponse.status, 200);
    assert.match(txtResponse.headers.get('content-type'), /^text\/plain\b/);
    assert.match(await txtResponse.text(), /plain text attachment proof/);

    const mdResponse = await fetch(`http://127.0.0.1:${port}/deep/talk/attachments/context.md`);
    assert.equal(mdResponse.status, 200);
    assert.match(mdResponse.headers.get('content-type'), /^text\/markdown\b/);
    assert.match(await mdResponse.text(), /Markdown attachment proof/);

    const pngResponse = await fetch(`http://127.0.0.1:${port}/deep/talk/attachments/panel.PNG`);
    assert.equal(pngResponse.status, 200);
    assert.match(pngResponse.headers.get('content-type'), /^image\/png\b/);
  } finally {
    await stopChild(server);
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('section sub-pages derive their own canonical route from parent route', () => {
  const source = `---
title: Parent Page
slug: parent-page
route: /nested/parent
section: project
access: private
---
# Parent Page

<!-- section: { slug: "child-section", talk: true, date: "2026-05-19" } -->
## Child Section

Section body.
`;

  const { sections } = renderPage(source, { slug: 'parent-page' });

  assert.equal(sections.length, 1);
  assert.equal(sections[0].frontmatter.route, '/nested/parent/child-section');
  assert.equal(sections[0].frontmatter.parent_route, '/nested/parent');
  assert.equal(sections[0].frontmatter.md_url, '/nested/parent/child-section.md');
  assert.equal(sections[0].frontmatter.talk_route, '/nested/parent/child-section/talk');
  assert.equal(sections[0].frontmatter.talk_url, '/nested/parent/child-section.talk.md');
  assert.match(sections[0].md, /^route: \/nested\/parent\/child-section$/m);
  assert.match(sections[0].md, /^parent_route: \/nested\/parent$/m);
  assert.match(sections[0].md, /^talk_route: \/nested\/parent\/child-section\/talk$/m);
});

test('frontmatter sections derive sub-pages by H2 anchor', () => {
  const source = `---
title: Parent Page
slug: parent-page
route: /nested/parent
section: project
access: private
sections:
  - slug: frontmatter-section
    anchor: Child_Section
    talk: true
    date: "2026-05-20"
---
# Parent Page

## Child Section

Section body.
`;

  const { sections } = renderPage(source, { slug: 'parent-page' });

  assert.equal(sections.length, 1);
  assert.equal(sections[0].slug, 'frontmatter-section');
  assert.equal(sections[0].anchor, 'Child_Section');
  assert.equal(sections[0].frontmatter.route, '/nested/parent/frontmatter-section');
  assert.equal(sections[0].frontmatter.parent_anchor, 'Child_Section');
  assert.equal(sections[0].frontmatter.section_date, '2026-05-20');
  assert.equal(sections[0].frontmatter.talk_route, '/nested/parent/frontmatter-section/talk');
  assert.match(sections[0].md, /^route: \/nested\/parent\/frontmatter-section$/m);
  assert.doesNotMatch(sections[0].md, /^sections:/m);
});

test('frontmatter sections fail loudly when anchor does not match an H2', () => {
  const source = `---
title: Parent Page
slug: parent-page
route: /nested/parent
section: project
access: private
sections:
  - slug: missing-section
    anchor: not-present
---
# Parent Page

## Child Section

Section body.
`;

  assert.throws(
    () => renderPage(source, { slug: 'parent-page' }),
    /frontmatter sections\[\] anchor "not-present" doesn't match any H2/
  );
});

test('inline section marker wins over matching frontmatter section entry', () => {
  const source = `---
title: Parent Page
slug: parent-page
route: /nested/parent
section: project
access: private
sections:
  - slug: frontmatter-section
    anchor: Child_Section
    talk: false
---
# Parent Page

<!-- section: { slug: "inline-section", talk: true, date: "2026-05-21" } -->
## Child Section

Section body.
`;

  const { sections } = renderPage(source, { slug: 'parent-page' });

  assert.equal(sections.length, 1);
  assert.equal(sections[0].slug, 'inline-section');
  assert.equal(sections[0].frontmatter.route, '/nested/parent/inline-section');
  assert.equal(sections[0].frontmatter.talk_route, '/nested/parent/inline-section/talk');
  assert.equal(sections[0].frontmatter.section_date, '2026-05-21');
});

test('frontmatter sections reject malformed object-list entries', () => {
  const source = `---
title: Parent Page
slug: parent-page
route: /nested/parent
section: project
access: private
sections:
  - slug: missing-anchor
---
# Parent Page

## Child Section

Section body.
`;

  assert.throws(
    () => renderPage(source, { slug: 'parent-page' }),
    /field "sections" entry missing required string key "anchor"/
  );
});

test('render-to-dir emits rendered routes for section talk stubs', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-section-talk-'));
  try {
    const input = resolve(tmp, 'parent-page.md');
    const output = resolve(tmp, 'site');
    writeFileSync(input, `---
title: Parent Page
slug: parent-page
route: /nested/parent
section: project
access: private
---
# Parent Page

<!-- section: { slug: "child-section", talk: true, date: "2026-05-19" } -->
## Child Section

Section body.
`);

    const result = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-to-dir.mjs'),
      input,
      output,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
    });

    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.ok(existsSync(resolve(output, 'nested/parent/child-section.html')));
    assert.ok(existsSync(resolve(output, 'nested/parent/child-section/index.html')));
    assert.ok(existsSync(resolve(output, 'nested/parent/child-section.md')));
    assert.ok(existsSync(resolve(output, 'nested/parent/child-section.talk.html')));
    assert.ok(existsSync(resolve(output, 'nested/parent/child-section.talk.md')));
    assert.ok(existsSync(resolve(output, 'nested/parent/child-section/talk/index.html')));

    const sectionMd = readFileSync(resolve(output, 'nested/parent/child-section.md'), 'utf8');
    assert.match(sectionMd, /^talk_route: \/nested\/parent\/child-section\/talk$/m);

    const talkMd = readFileSync(resolve(output, 'nested/parent/child-section.talk.md'), 'utf8');
    assert.match(talkMd, /^route: \/nested\/parent\/child-section$/m);
    assert.match(talkMd, /^talk_route: \/nested\/parent\/child-section\/talk$/m);
    assert.match(talkMd, /^md_url: \/nested\/parent\/child-section\.talk\.md$/m);

    const talkRouteHtml = readFileSync(
      resolve(output, 'nested/parent/child-section/talk/index.html'),
      'utf8'
    );
    assert.match(talkRouteHtml, /<base href="\/nested\/parent\/child-section\/talk\/">/);
    assert.match(talkRouteHtml, /<h1>Talk · Child Section<\/h1>/);
    assert.match(talkRouteHtml, /<h2 id="Discussion">Discussion<\/h2>/);
    assert.match(
      talkRouteHtml,
      /<link rel="alternate" type="text\/markdown" href="\/nested\/parent\/child-section\.talk\.md">/
    );
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-to-dir emits root section routes at canonical markdown twin paths', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-root-section-'));
  try {
    const input = resolve(tmp, 'home.md');
    const output = resolve(tmp, 'site');
    writeFileSync(input, `---
title: Home
slug: home
route: /
section: site
access: private
---
# Home

<!-- section: { slug: "reader-agent-proof", talk: true, date: "2026-05-20" } -->
## Reader Agent Proof

Root section body.
`);

    const result = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-to-dir.mjs'),
      input,
      output,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
    });

    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.ok(existsSync(resolve(output, 'reader-agent-proof.html')));
    assert.ok(existsSync(resolve(output, 'reader-agent-proof/index.html')));
    assert.ok(existsSync(resolve(output, 'reader-agent-proof.md')));
    assert.ok(existsSync(resolve(output, 'reader-agent-proof.talk.html')));
    assert.ok(existsSync(resolve(output, 'reader-agent-proof.talk.md')));
    assert.ok(existsSync(resolve(output, 'reader-agent-proof/talk/index.html')));
    assert.equal(existsSync(resolve(output, 'index/reader-agent-proof.md')), false);

    const sectionMd = readFileSync(resolve(output, 'reader-agent-proof.md'), 'utf8');
    assert.match(sectionMd, /^route: \/reader-agent-proof$/m);
    assert.match(sectionMd, /^md_url: \/reader-agent-proof\.md$/m);
    assert.match(sectionMd, /^talk_route: \/reader-agent-proof\/talk$/m);

    const talkMd = readFileSync(resolve(output, 'reader-agent-proof.talk.md'), 'utf8');
    assert.match(talkMd, /^route: \/reader-agent-proof$/m);
    assert.match(talkMd, /^talk_route: \/reader-agent-proof\/talk$/m);
    assert.match(talkMd, /^md_url: \/reader-agent-proof\.talk\.md$/m);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-to-dir route override preserves page talk route and markdown twin', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-page-route-'));
  try {
    const input = resolve(tmp, 'field-notes.md');
    const output = resolve(tmp, 'site');
    writeFileSync(input, `---
title: Field Notes
slug: field-notes
section: project
access: private
---
# Field Notes

These notes exercise nested page-level routes.
`);

    const result = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-to-dir.mjs'),
      input,
      output,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
      env: {
        ...process.env,
        ONECONTEXT_WIKI_ROUTE_JSON: JSON.stringify({ route: '/field/notes' }),
        ONECONTEXT_WIKI_SITE_NAV_JSON: JSON.stringify({
          groups: [
            {
              label: 'Visible',
              items: [
                {
                  href: '/field/notes/deep-dive',
                  label: 'Deep Dive',
                  sub: 'Nested visible page',
                },
              ],
            },
          ],
        }),
      },
    });

    assert.equal(result.status, 0, result.stderr || result.stdout);
    assert.ok(existsSync(resolve(output, 'field/notes.html')));
    assert.ok(existsSync(resolve(output, 'field/notes/index.html')));
    assert.ok(existsSync(resolve(output, 'field/notes.md')));

    const pageMd = readFileSync(resolve(output, 'field/notes.md'), 'utf8');
    assert.match(pageMd, /^route: \/field\/notes$/m);
    assert.match(pageMd, /^md_url: \/field\/notes\.md$/m);
    assert.match(pageMd, /^talk_route: \/field\/notes\/talk$/m);
    assert.match(pageMd, /^talk_url: \/field\/notes\.talk\.md$/m);

    const pageHtml = readFileSync(resolve(output, 'field/notes.html'), 'utf8');
    assert.match(
      pageHtml,
      /<link rel="alternate" type="text\/markdown" href="\/field\/notes\.md">/
    );
    assert.match(pageHtml, /Deep Dive/);
    assert.match(pageHtml, /href="\/field\/notes\/deep-dive"/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-to-dir preserves root article talk route as /talk', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-root-talk-route-'));
  try {
    const input = resolve(tmp, 'home.md');
    const talkFolder = resolve(tmp, 'home.talk');
    const output = resolve(tmp, 'site');
    writeFixtureFile(input, `---
title: Home
slug: home
section: site
access: private
---
# Home

Root route body with enough words for the rendered reader surface.
`);
    writeFixtureFile(resolve(talkFolder, '_meta.yaml'), `title: Talk · Home
slug: home.talk
section: site
access: private
summary: Discussion for the root page.
talk_enabled: false
see_also:
  - text: Home
    url: /
`);
    writeFixtureFile(resolve(talkFolder, '2026-05-20T07-00Z.proposal.root-talk.md'), `---
kind: proposal
author: worker-aw
ts: 2026-05-20T07:00:00Z
---
Keep the root page talk surface at /talk instead of /index/talk.
`);
    writeFixtureFile(resolve(talkFolder, 'attachments/root-proof.txt'), 'root talk attachment\n');

    const pageResult = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-to-dir.mjs'),
      input,
      output,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
      env: {
        ...process.env,
        ONECONTEXT_WIKI_ROUTE_JSON: JSON.stringify({ route: '/', slug: 'home' }),
      },
    });
    assert.equal(pageResult.status, 0, pageResult.stderr || pageResult.stdout);

    const talkResult = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-to-dir.mjs'),
      talkFolder,
      output,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
      env: {
        ...process.env,
        ONECONTEXT_WIKI_ROUTE_JSON: JSON.stringify({
          route: '/',
          talk_route: '/talk',
          slug: 'home.talk',
        }),
      },
    });
    assert.equal(talkResult.status, 0, talkResult.stderr || talkResult.stdout);

    assert.ok(existsSync(resolve(output, 'index.html')));
    assert.ok(existsSync(resolve(output, 'index.md')));
    assert.ok(existsSync(resolve(output, 'index.talk.html')));
    assert.ok(existsSync(resolve(output, 'index.talk.md')));
    assert.ok(existsSync(resolve(output, 'talk/index.html')));
    assert.ok(existsSync(resolve(output, 'talk/attachments/root-proof.txt')));
    assert.equal(existsSync(resolve(output, 'index/index.html')), false);
    assert.equal(existsSync(resolve(output, 'index.talk/index.html')), false);

    const pageMd = readFileSync(resolve(output, 'index.md'), 'utf8');
    assert.match(pageMd, /^route: \/$/m);
    assert.match(pageMd, /^talk_route: \/talk$/m);
    assert.match(pageMd, /^talk_url: \/index\.talk\.md$/m);

    const talkMd = readFileSync(resolve(output, 'index.talk.md'), 'utf8');
    assert.match(talkMd, /^route: \/$/m);
    assert.match(talkMd, /^talk_route: \/talk$/m);
    assert.match(talkMd, /^md_url: \/index\.talk\.md$/m);
    assert.match(talkMd, /^see_also: \[\{"text":"Home","url":"\/"\}\]$/m);
    assert.doesNotMatch(talkMd, /\[object Object\]/);

    const talkRouteHtml = readFileSync(resolve(output, 'talk/index.html'), 'utf8');
    assert.match(talkRouteHtml, /<base href="\/talk\/">/);
    assert.match(talkRouteHtml, /<h1>Talk · Home<\/h1>/);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-site dogfoods a larger route map with talk pages and markdown twins', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-render-site-dogfood-'));
  try {
    const wikiRoot = resolve(tmp, 'user-wiki');
    const sourceRoot = resolve(wikiRoot, 'source');
    const output = resolve(tmp, 'site');
    const resultJson = resolve(tmp, 'render-result.json');

    writeFixtureFile(resolve(wikiRoot, 'wiki.toml'), `title = "Worker C Dogfood Wiki"
navigation = ["home", "guide", "deep-dive"]
primary_navigation = ["home", "guide", "deep-dive"]
utility_navigation = ["account-settings", "help"]

[[pages]]
id = "home"
slug = "home"
title = "Home"
summary = "Primary home surface"
route = "/"
family_group = "primary"
family_id = "home"
nav_order = 1

[[pages]]
id = "guide"
slug = "guide"
title = "Guide"
summary = "Primary guide with section talk"
route = "/guide"
family_group = "primary"
family_id = "guide"
nav_order = 2

[[pages]]
id = "deep-dive"
slug = "deep-dive"
title = "Deep Dive"
summary = "Nested primary route"
route = "/projects/deep-dive"
family_group = "primary"
family_id = "deep-dive"
nav_order = 3

[[pages]]
id = "account-settings"
slug = "account-settings"
title = "Account Settings"
summary = "Utility account route"
route = "/settings/account"
family_group = "utility"
family_id = "account-settings"
nav_order = 1

[[pages]]
id = "help"
slug = "help"
title = "Help"
summary = "Utility help route"
route = "/help"
family_group = "utility"
family_id = "help"
nav_order = 2
`);

    writeFixtureFile(
      resolve(sourceRoot, 'families/primary/home/source/home.md'),
      `---
title: Home
slug: home
section: site
access: private
summary: Primary home surface.
---
# Home

The home page gives the reader enough prose to exercise the enhanced browser view.

## Start Here

Primary surface body.
`
    );
    writeFixtureFile(
      resolve(sourceRoot, 'families/primary/home/talk/home.talk/_meta.yaml'),
      `title: Talk · Home
slug: home.talk
section: site
access: private
summary: Discussion for the root home page.
status: draft
talk_enabled: false
`
    );
    writeFixtureFile(
      resolve(sourceRoot, 'families/primary/home/talk/home.talk/2026-05-19T11-00Z.proposal.root-talk.md'),
      `---
kind: proposal
author: worker-aw
ts: 2026-05-19T11:00:00Z
---
Keep the root page talk route at /talk.
`
    );

    writeFixtureFile(
      resolve(sourceRoot, 'families/primary/guide/source/guide.md'),
      `---
page_id: guide
title: Guide
slug: guide
section: project
access: private
summary: Primary guide with page talk and section talk.
---
# Guide

This guide has enough body text to exercise reader mode, agent mode, page talk, section talk, markdown twins, and nested route rendering in one render.

<!-- section: { slug: "details", talk: true, date: "2026-05-19" } -->
## Details

The details section gets its own HTML route, markdown twin, and talk stub.

## See also

- [Deep Dive](/projects/deep-dive)
`
    );

    writeFixtureFile(
      resolve(sourceRoot, 'families/primary/guide/talk/guide.talk/_meta.yaml'),
      `title: Talk · Guide
slug: guide.talk
section: project
access: private
summary: Discussion for the guide page.
status: draft
talk_enabled: false
lede: Keep proposals tied to exact rendered surfaces.
see_also:
  - text: Guide
    url: /guide
`
    );
    writeFixtureFile(
      resolve(sourceRoot, 'families/primary/guide/talk/guide.talk/2026-05-19T12-00Z.proposal.menu-shape.md'),
      `---
kind: proposal
author: worker-c
ts: 2026-05-19T12:00:00Z
---
Verify menu open and close behavior across the larger route map.

Attach route-local proof as [evidence.txt](attachments/evidence.txt).
`
    );
    writeFixtureFile(
      resolve(sourceRoot, 'families/primary/guide/talk/guide.talk/attachments/evidence.txt'),
      'guide talk attachment evidence\n'
    );

    writeFixtureFile(
      resolve(sourceRoot, 'families/primary/deep-dive/source/deep-dive.md'),
      `---
title: Deep Dive
slug: deep-dive
section: project
access: private
summary: Nested primary route.
---
# Deep Dive

Nested page content.

## Nested Topic

Nested route body.
`
    );

    writeFixtureFile(
      resolve(sourceRoot, 'families/utility/account-settings/source/account-settings.md'),
      `---
title: Account Settings
slug: account-settings
section: system
access: private
summary: Utility account route.
---
# Account Settings

Utility page content.

## Preferences

Utility route body.
`
    );

    writeFixtureFile(
      resolve(sourceRoot, 'families/utility/help/source/help.md'),
      `---
title: Help
slug: help
section: reference
access: private
summary: Utility help route.
---
# Help

Help page content.

## Support

Utility help body.
`
    );

    writeFixtureFile(
      resolve(sourceRoot, 'families/hidden/lab/source/lab.md'),
      `---
title: Hidden Lab
slug: lab
route: /hidden/lab
section: reference
access: private
summary: Hidden page that renders but does not enter navigation.
---
# Hidden Lab

This hidden page should render and appear in manifests without appearing in the brand menu.

## Private Note

Hidden route body.
`
    );

    const result = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-site.mjs'),
      '--source-root',
      sourceRoot,
      '--output',
      output,
      '--result-json',
      resultJson,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
    });

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const renderResult = JSON.parse(readFileSync(resultJson, 'utf8'));
    assert.equal(renderResult.status, 'published');
    assert.equal(renderResult.source_input_count, 6);
    assert.equal(renderResult.talk_input_count, 2);
    assert.equal(renderResult.link_diagnostics.status, 'ok');
    assert.equal(renderResult.link_diagnostics.broken_internal_count, 0);

    for (const relativePath of [
      'index.html',
      'index.md',
      'index.talk.html',
      'index.talk.md',
      'talk/index.html',
      'guide.html',
      'guide/index.html',
      'guide.md',
      'guide.talk.html',
      'guide.talk.md',
      'guide/talk/index.html',
      'guide/talk/attachments/evidence.txt',
      'guide/details.html',
      'guide/details/index.html',
      'guide/details.md',
      'guide/details.talk.html',
      'guide/details.talk.md',
      'guide/details/talk/index.html',
      'projects/deep-dive.html',
      'projects/deep-dive/index.html',
      'settings/account.html',
      'settings/account/index.html',
      'hidden/lab.html',
      'hidden/lab/index.html',
      '.1context/route-manifest.json',
      '.1context/content-index.json',
      '.1context/reference-index.json',
    ]) {
      assert.ok(existsSync(resolve(output, relativePath)), `${relativePath} should exist`);
    }
    for (const relativePath of [
      'index/index.html',
      'index.talk/index.html',
      'guide.talk/index.html',
      'guide/details.talk/index.html',
    ]) {
      assert.equal(existsSync(resolve(output, relativePath)), false, `${relativePath} should not be emitted`);
    }

    const routeManifest = JSON.parse(
      readFileSync(resolve(output, '.1context/route-manifest.json'), 'utf8')
    );
    const contentIndex = JSON.parse(
      readFileSync(resolve(output, '.1context/content-index.json'), 'utf8')
    );
    const routes = new Map(routeManifest.routes.map((entry) => [entry.route, entry]));
    for (const route of [
      '/',
      '/guide',
      '/guide/talk',
      '/guide/details',
      '/guide/details/talk',
      '/talk',
      '/projects/deep-dive',
      '/settings/account',
      '/help',
      '/hidden/lab',
    ]) {
      assert.ok(routes.has(route), `${route} should be in the route manifest`);
    }
    assert.equal(routeManifest.route_count, routeManifest.routes.length);
    assert.equal(contentIndex.markdown_twin_count, contentIndex.markdown_twins.length);
    assert.equal(contentIndex.page_count, 7);
    assert.equal(contentIndex.talk_count, 3);
    assert.equal(routeManifest.reference_index.path, '.1context/reference-index.json');
    assert.equal(contentIndex.reference_index.path, '.1context/reference-index.json');
    assert.equal(routes.get('/guide').page_id, 'guide');

    const markdownTwins = new Map(
      contentIndex.markdown_twins.map((entry) => [entry.path, entry])
    );
    assert.equal(markdownTwins.get('guide.md').page_id, 'guide');
    assert.equal(markdownTwins.get('index.talk.md').route, '/talk');
    assert.equal(markdownTwins.get('index.talk.md').route_index_path, 'talk/index.html');
    assert.equal(markdownTwins.get('guide.md').md_url, '/guide.md');
    assert.equal(markdownTwins.get('guide.talk.md').route, '/guide/talk');
    assert.equal(markdownTwins.get('guide/details.md').route, '/guide/details');
    assert.equal(markdownTwins.get('guide/details.talk.md').route, '/guide/details/talk');
    assert.equal(markdownTwins.get('hidden/lab.md').route, '/hidden/lab');

    const guideHtml = readFileSync(resolve(output, 'guide.html'), 'utf8');
    assert.match(guideHtml, /<link rel="alternate" type="text\/markdown" href="\/guide\.md">/);
    assert.match(guideHtml, /href="\/projects\/deep-dive"/);
    assert.match(guideHtml, /Account Settings/);
    assert.doesNotMatch(guideHtml, /Hidden Lab/);

    const hiddenHtml = readFileSync(resolve(output, 'hidden/lab.html'), 'utf8');
    assert.match(hiddenHtml, /<link rel="alternate" type="text\/markdown" href="\/hidden\/lab\.md">/);
    assert.doesNotMatch(hiddenHtml, /href="\/lab\.md"/);

    const homeTalkRouteHtml = readFileSync(resolve(output, 'talk/index.html'), 'utf8');
    assert.match(homeTalkRouteHtml, /<base href="\/talk\/">/);
    assert.match(homeTalkRouteHtml, /<h1>Talk · Home<\/h1>/);

    const guideTalkRouteHtml = readFileSync(resolve(output, 'guide/talk/index.html'), 'utf8');
    assert.match(guideTalkRouteHtml, /<base href="\/guide\/talk\/">/);
    assert.match(
      guideTalkRouteHtml,
      /<link rel="alternate" type="text\/markdown" href="\/guide\.talk\.md">/
    );
    assert.match(guideTalkRouteHtml, /<h1>Talk · Guide<\/h1>/);

    const sectionTalkRouteHtml = readFileSync(
      resolve(output, 'guide/details/talk/index.html'),
      'utf8'
    );
    assert.match(sectionTalkRouteHtml, /<base href="\/guide\/details\/talk\/">/);
    assert.match(sectionTalkRouteHtml, /<h1>Talk · Details<\/h1>/);
    assert.match(
      sectionTalkRouteHtml,
      /<link rel="alternate" type="text\/markdown" href="\/guide\/details\.talk\.md">/
    );
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-site brand menu preserves page nav_order across repeated group labels', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-render-site-nav-order-'));
  try {
    const wikiRoot = resolve(tmp, 'user-wiki');
    const sourceRoot = resolve(wikiRoot, 'source');
    const output = resolve(tmp, 'site');
    const resultJson = resolve(tmp, 'render-result.json');

    writeFixtureFile(resolve(wikiRoot, 'wiki.toml'), `title = "Nav Order Wiki"
navigation = ["shared-early", "other-middle", "shared-late"]
primary_navigation = ["shared-early", "other-middle", "shared-late"]

[[pages]]
id = "shared-early"
slug = "shared-early"
title = "Shared Early"
summary = "Early shared-group page"
route = "/shared-early"
family_group = "shared"
family_group_title = "Shared"
family_id = "shared-early"
nav_order = 1

[[pages]]
id = "other-middle"
slug = "other-middle"
title = "Other Middle"
summary = "Middle other-group page"
route = "/other-middle"
family_group = "other"
family_group_title = "Other"
family_id = "other-middle"
nav_order = 2

[[pages]]
id = "shared-late"
slug = "shared-late"
title = "Shared Late"
summary = "Late shared-group page"
route = "/shared-late"
family_group = "shared"
family_group_title = "Shared"
family_id = "shared-late"
nav_order = 3
`);

    for (const [relativePath, title, slug] of [
      ['shared/shared-early/source/shared-early.md', 'Shared Early', 'shared-early'],
      ['other/other-middle/source/other-middle.md', 'Other Middle', 'other-middle'],
      ['shared/shared-late/source/shared-late.md', 'Shared Late', 'shared-late'],
    ]) {
      writeFixtureFile(
        resolve(sourceRoot, `families/${relativePath}`),
        `---
title: ${title}
slug: ${slug}
section: reference
access: private
summary: Menu order proof.
---
# ${title}

## Proof

Menu order body for ${title}.
`
      );
    }

    const result = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-site.mjs'),
      '--source-root',
      sourceRoot,
      '--output',
      output,
      '--result-json',
      resultJson,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
    });
    assert.equal(result.status, 0, result.stderr || result.stdout);

    const html = readFileSync(resolve(output, 'shared-early.html'), 'utf8');
    const labels = [...html.matchAll(/opctx-brand-menu-label">([^<]+)</g)].map((match) => match[1]);
    assert.deepEqual(labels, ['Shared Early', 'Other Middle', 'Shared Late']);
    const headings = [...html.matchAll(/opctx-brand-menu-heading">([^<]+)</g)].map((match) => match[1]);
    assert.deepEqual(headings, ['Shared', 'Other', 'Shared']);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-site marks generated site pages in route metadata', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-generated-site-kind-'));
  try {
    const wikiRoot = resolve(tmp, 'user-wiki');
    const sourceRoot = resolve(wikiRoot, 'source');
    const output = resolve(tmp, 'site');
    const resultJson = resolve(tmp, 'render-result.json');

    writeFixtureFile(resolve(wikiRoot, 'wiki.toml'), `title = "Generated Site Kind"
primary_navigation = ["home", "topics"]
utility_navigation = ["system-status"]

[[site_pages]]
id = "home"
enabled = true
title = "Home"
route = "/"
template = "site/home.md"

[[site_pages]]
id = "system-status"
enabled = true
title = "System Status"
route = "/system/status"
template = "site/system-status.md"

[[pages]]
id = "topics"
enabled = true
title = "Topics"
slug = "topics"
route = "/topics"
family_group = "reference"
family_id = "topics"
`);
    writeFixtureFile(
      resolve(wikiRoot, 'templates/site/home.md'),
      `---
title: "{{ title }}"
slug: "{{ slug }}"
route: "{{ route }}"
section: site
access: private
---
# {{ title }}

Generated home body.
`
    );
    writeFixtureFile(
      resolve(wikiRoot, 'templates/site/system-status.md'),
      `---
title: "{{ title }}"
slug: "{{ slug }}"
route: "{{ route }}"
section: system
access: private
---
# {{ title }}

Generated nested utility body.
`
    );
    writeFixtureFile(
      resolve(sourceRoot, 'families/reference/topics/source/topics.md'),
      `---
page_id: topics
title: Topics
slug: topics
route: /topics
section: reference
access: private
---
# Topics

Source-backed topic body.
`
    );

    const result = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-site.mjs'),
      '--source-root',
      sourceRoot,
      '--output',
      output,
      '--result-json',
      resultJson,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
    });

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const routeManifest = JSON.parse(
      readFileSync(resolve(output, '.1context/route-manifest.json'), 'utf8')
    );
    const contentIndex = JSON.parse(
      readFileSync(resolve(output, '.1context/content-index.json'), 'utf8')
    );
    const routes = new Map(routeManifest.routes.map((entry) => [entry.route, entry]));
    assert.equal(routes.get('/').source_kind, 'generated_site_page');
    assert.equal(routes.get('/system/status').source_kind, 'generated_site_page');
    assert.equal(routes.get('/system/status').slug, 'system-status');
    assert.equal(routes.get('/system/status').markdown_path, 'system/status.md');
    assert.equal(routes.get('/topics').source_kind, 'source_page');
    assert.equal(routes.get('/topics').page_id, 'topics');
    const twins = new Map(contentIndex.markdown_twins.map((entry) => [entry.path, entry]));
    assert.equal(twins.get('index.md').source_kind, 'generated_site_page');
    assert.equal(twins.get('system/status.md').source_kind, 'generated_site_page');
    assert.equal(twins.get('system/status.md').slug, 'system-status');
    assert.equal(twins.get('topics.md').source_kind, 'source_page');
    assert.equal(twins.get('topics.md').page_id, 'topics');
    assert.match(readFileSync(resolve(output, 'index.md'), 'utf8'), /^source_kind: generated_site_page$/m);
    assert.match(readFileSync(resolve(output, 'system/status.md'), 'utf8'), /^slug: system-status$/m);
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});

test('render-site annotates broken internal links in generated reader output', () => {
  const tmp = mkdtempSync(resolve(tmpdir(), '1ctx-render-site-link-warning-'));
  try {
    const wikiRoot = resolve(tmp, 'user-wiki');
    const sourceRoot = resolve(wikiRoot, 'source');
    const output = resolve(tmp, 'site');
    const resultJson = resolve(tmp, 'render-result.json');

    writeFixtureFile(resolve(wikiRoot, 'wiki.toml'), `title = "Link Warning Wiki"
navigation = ["source", "target"]
primary_navigation = ["source", "target"]

[[pages]]
id = "source"
slug = "source"
title = "Source"
route = "/source"
family_group = "primary"
family_id = "source"
nav_order = 1

[[pages]]
id = "target"
slug = "target"
title = "Target"
route = "/target"
family_group = "primary"
family_id = "target"
nav_order = 2
`);

    writeFixtureFile(resolve(sourceRoot, 'families/primary/source/source/source.md'), `---
title: Source
slug: source
section: project
access: private
---
# Source

This page has a valid [Target](/target), a broken [Missing](/missing-target), and a broken [Relative Missing](./relative-missing).

## Links

The render should publish with warnings, not fail.
`);

    writeFixtureFile(resolve(sourceRoot, 'families/primary/target/source/target.md'), `---
title: Target
slug: target
section: project
access: private
---
# Target

Target body.
`);

    const result = spawnSync(process.execPath, [
      resolve(ENGINE_ROOT, 'tools/render-site.mjs'),
      '--source-root',
      sourceRoot,
      '--output',
      output,
      '--result-json',
      resultJson,
    ], {
      cwd: ENGINE_ROOT,
      encoding: 'utf8',
    });

    assert.equal(result.status, 0, result.stderr || result.stdout);
    const renderResult = JSON.parse(readFileSync(resultJson, 'utf8'));
    assert.equal(renderResult.status, 'published');
    assert.equal(renderResult.link_diagnostics.status, 'warning');
    assert.equal(renderResult.link_diagnostics.broken_internal_count, 2);

    const diagnostics = JSON.parse(
      readFileSync(resolve(output, '.1context/link-diagnostics.json'), 'utf8')
    );
    assert.equal(diagnostics.status, 'warning');
    assert.deepEqual(
      diagnostics.issues.map((issue) => issue.target).sort(),
      ['/missing-target', '/relative-missing']
    );

    const routeManifest = JSON.parse(
      readFileSync(resolve(output, '.1context/route-manifest.json'), 'utf8')
    );
    assert.equal(routeManifest.link_diagnostics.status, 'warning');
    assert.equal(routeManifest.link_diagnostics.issue_count, 2);
    assert.equal(routeManifest.link_diagnostics.path, '.1context/link-diagnostics.json');

    const canonicalHtml = readFileSync(resolve(output, 'source.html'), 'utf8');
    const routeIndexHtml = readFileSync(resolve(output, 'source/index.html'), 'utf8');
    for (const html of [canonicalHtml, routeIndexHtml]) {
      assert.match(html, /class="opctx-link-warning"/);
      assert.match(html, /2 internal links point to missing pages\./);
      assert.match(html, /href="\/missing-target" class="opctx-broken-link"/);
      assert.match(html, /href="\.\/relative-missing" class="opctx-broken-link"/);
      assert.equal((html.match(/opctx-link-warning/g) || []).length, 1);
      assert.equal((html.match(/data-1context-link-state="broken"/g) || []).length, 2);
    }
  } finally {
    rmSync(tmp, { recursive: true, force: true });
  }
});
