// `:::see-also` — Wikipedia-style "See also" section. Adds an H2
// with the canonical id="See_also" anchor and renders the inner
// markdown (typically a list of links).
//
// Authored:
//   :::see-also
//   - [Quartz](https://quartz.jzhao.xyz)
//   - [Hugo](https://gohugo.io)
//   :::
//
// Renders to:
//   <h2 id="See_also">See also</h2>
//   <ul>
//     <li><a href="https://quartz.jzhao.xyz">Quartz</a></li>
//     <li><a href="https://gohugo.io">Hugo</a></li>
//   </ul>

import { matchFence, startFence } from './_shared.mjs';

const NAME = 'see-also';

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
    return `<h2 id="See_also">See also</h2>\n${this.parser.parse(token.tokens)}`;
  },
};
