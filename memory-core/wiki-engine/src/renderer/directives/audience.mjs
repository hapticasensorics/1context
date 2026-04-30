// `:::audience internal` — wraps a section in a `<div data-audience="…">`
// for the build-time audience-tier filter (P5 in ROADMAP).
//
// Authored:
//   :::audience internal
//   This paragraph only renders for internal viewers.
//   :::
//
// Renders to:
//   <div class="opctx-audience" data-audience="internal">
//     <p>This paragraph only renders for internal viewers.</p>
//   </div>
//
// The renderer emits the wrapper unconditionally — the build-time
// audience filter (P5) is responsible for stripping wrappers whose
// data-audience doesn't match the build target. Until P5 lands,
// internal sections render in BOTH builds. That's a known gap, not
// a bug; intentional during scaffolding so we can author with the
// markers in place and turn on enforcement later.

import { matchFence, startFence } from './_shared.mjs';

const NAME = 'audience';

const ALLOWED = new Set(['internal', 'public', 'shared']);

export default {
  name: NAME,
  level: 'block',
  start: startFence,
  tokenizer(src) {
    const m = matchFence(src, NAME);
    if (!m) return undefined;
    const audience = m.args || 'internal';
    return {
      type: NAME,
      raw: m.raw,
      audience,
      valid: ALLOWED.has(audience),
      tokens: this.lexer.blockTokens(m.body),
    };
  },
  renderer(token) {
    const klass = token.valid ? 'opctx-audience' : 'opctx-audience opctx-directive-error';
    const inner = this.parser.parse(token.tokens);
    return `<div class="${klass}" data-audience="${token.audience}">${inner}</div>\n`;
  },
};
