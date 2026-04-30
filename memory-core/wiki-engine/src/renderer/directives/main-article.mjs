// `:::main-article slug` — Wikipedia-style "Main article: Foo" hatnote.
// The argument is the slug of the article being pointed at; the
// human-readable label is derived from the slug.
//
// Authored:
//   :::main-article wiki-engine
//   :::
//
// Renders to:
//   <p class="opctx-main-article">Main article:
//     <a href="/wiki-engine.html">Wiki engine</a></p>
//
// Body content (between the opening fence and the closing :::) is
// ignored — this directive is single-line in spirit. Putting body
// content there is a no-op, so future agents can safely add notes
// inside without breaking anything.

import { matchFence, startFence, humanTitleFromSlug } from './_shared.mjs';

const NAME = 'main-article';

const ESCAPE = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const escape = (s) => String(s).replace(/[&<>"']/g, (c) => ESCAPE[c]);

export default {
  name: NAME,
  level: 'block',
  start: startFence,
  tokenizer(src) {
    const m = matchFence(src, NAME);
    if (!m) return undefined;
    return {
      type: NAME,
      raw: m.raw,
      slug: m.args,
    };
  },
  renderer(token) {
    const slug = token.slug;
    if (!slug) {
      // Self-document the failure mode rather than silently emit nothing.
      return `<p class="opctx-main-article opctx-directive-error">Main article: <em>(missing slug — write <code>:::main-article slug</code>)</em></p>\n`;
    }
    return `<p class="opctx-main-article">Main article: <a href="/${escape(slug)}.html">${escape(humanTitleFromSlug(slug))}</a></p>\n`;
  },
};
