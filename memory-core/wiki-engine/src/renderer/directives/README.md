# Custom directives (P1, in progress)

Each `.mjs` file in this directory is a `marked` extension that
implements one wiki-engine custom directive. Loaded by
`wiki-engine/src/renderer/index.mjs` via `marked.use({ extensions: [...] })`.

Directive syntax follows the **pandoc fenced div** convention
(`Acked-by` decision pending on the talk page; see the
`[PROPOSAL] P1 renderer: library = marked, directives = pandoc
fenced divs` topic at `/wiki-engine.talk.md`).

## Directives to implement

- [ ] `infobox.mjs` — `:::infobox … :::` → `<aside class="infobox">`
      Used for the right-rail fast-facts boxes on long-form pages.
- [ ] `main-article.mjs` — `:::main-article slug :::` → 
      `<p class="opctx-main-article">Main article: <a href="/slug">…</a></p>`
- [ ] `see-also.mjs` — `:::see-also … :::` → `<h2 id="See_also">See also</h2><ul>…</ul>`
- [ ] `audience.mjs` — `:::audience internal … :::` (P5 territory; the
      directive emits a wrapper div the build can strip)

## Pattern

Each directive exports an object marked-compatible:

```js
export default {
  name: 'infobox',
  level: 'block',
  start(src) { /* return the index where infobox starts in src, or undefined */ },
  tokenizer(src) {
    const match = /^:::infobox\n([\s\S]*?)\n:::/.exec(src);
    if (!match) return false;
    return {
      type: 'infobox',
      raw: match[0],
      tokens: this.lexer.blockTokens(match[1]),
    };
  },
  renderer(token) {
    const inner = this.parser.parse(token.tokens);
    return `<aside class="infobox" role="complementary">${inner}</aside>`;
  },
};
```

When you implement one, also:

1. Import it in `../index.mjs` and register it in `makeMarked()`.
2. Add a fixture under `wiki-engine/src/renderer/__tests__/`
   (when we set up tests) showing a sample input and expected output.
3. Reply to the `[PROPOSAL]` thread on the talk page with
   `Acked-by: <your-id>` once the syntax is confirmed.
