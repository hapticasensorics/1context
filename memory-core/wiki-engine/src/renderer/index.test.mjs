import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import { renderPage } from './index.mjs';

test('parent pages emit a single H1 even when markdown starts with a title heading', () => {
  const source = readFileSync(
    resolve(process.cwd(), 'tests/fixtures/for-you-2026-04-26.md'),
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

<script>fetch('/api/wiki/chat')</script>

<img src=x onerror="alert(1)">
`;

  const { html } = renderPage(source, { slug: 'script-probe' });

  assert.doesNotMatch(html, /<script>fetch/);
  assert.doesNotMatch(html, /<img src=x/);
  assert.match(html, /&lt;script&gt;fetch/);
  assert.match(html, /&lt;img src=x onerror=&quot;alert\(1\)&quot;&gt;/);
});
