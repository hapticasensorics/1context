// `:::infobox` — wraps content in <aside class="infobox" role="complementary">.
// Used for right-rail "fast facts" sidebars on long-form pages.
//
// Authored:
//   :::infobox
//   ### Fast facts
//   - **Category:** Static wiki engine
//   - **Source format:** Markdown + frontmatter
//   :::
//
// Renders to:
//   <aside class="infobox" role="complementary">
//     <h3>Fast facts</h3>
//     <ul>
//       <li><strong>Category:</strong> Static wiki engine</li>
//       <li><strong>Source format:</strong> Markdown + frontmatter</li>
//     </ul>
//   </aside>

import { matchFence, startFence } from './_shared.mjs';

const NAME = 'infobox';

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
      tokens: this.lexer.blockTokens(m.body),
    };
  },
  renderer(token) {
    return `<aside class="infobox" role="complementary">${this.parser.parse(token.tokens)}</aside>\n`;
  },
};
