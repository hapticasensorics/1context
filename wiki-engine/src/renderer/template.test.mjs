import test from 'node:test';
import assert from 'node:assert/strict';

import { renderShell } from './template.mjs';

test('family pages render the Era selector as a custom pill menu', () => {
  const html = renderShell({
    frontmatter: {
      title: 'For You · Paul · Sunday, April 26, 2026',
      slug: 'for-you-2026-04-26',
      summary: 'Rolling daily brief',
      access: 'public',
    },
    bodyHtml: '<p>hello</p>',
    tocHtml: '<nav class="opctx-toc" aria-label="Contents"><ol><li><a href="#x">X</a></li></ol></nav>',
  });

  assert.match(html, /class="opctx-pill-menu-toggle opctx-version-toggle"/);
  assert.match(html, /id="opctx-version-menu"/);
  assert.doesNotMatch(html, /<select[^>]+id="opctx-version-menu"/);
  assert.doesNotMatch(html, /opctx-version-select-chevron/);
});

test('audience pages render a single-pill menu plus share modal shell', () => {
  const html = renderShell({
    frontmatter: {
      title: 'For You · Paul · Sunday, April 26, 2026',
      slug: 'for-you-2026-04-26',
      summary: 'Rolling daily brief',
      access: 'public',
    },
    bodyHtml: '<p>hello</p>',
    tocHtml: '<nav class="opctx-toc" aria-label="Contents"></nav>',
    audienceStreams: {
      active: 'public',
      order: ['private', 'internal', 'public'],
      streams: {
        private: { url: 'for-you-2026-04-26.private.html' },
        internal: { url: 'for-you-2026-04-26.internal.html' },
        public: { url: 'for-you-2026-04-26.html' },
      },
    },
    activeAudienceStream: 'public',
  });

  assert.match(html, /class="opctx-pill-menu-toggle opctx-audience-switcher-toggle"/);
  assert.match(html, /data-audience-action="share"/);
  assert.match(html, /opctx-share-modal-scrim/);
  assert.doesNotMatch(html, /role="radiogroup"/);
  assert.doesNotMatch(html, /opctx-audience-switcher-option/);
});
