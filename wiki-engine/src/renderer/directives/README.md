# Custom Directives

Each `.mjs` file in this directory is a `marked` extension that
implements one wiki-engine custom directive. Loaded by
`wiki-engine/src/renderer/index.mjs` via `marked.use({ extensions: [...] })`.

Directive syntax follows the pandoc fenced-div convention:

```md
:::infobox
Content
:::
```

## Implemented Directives

- `infobox.mjs`: `:::infobox ... :::` to a right-rail facts box.
- `main-article.mjs`: `:::main-article slug :::` to a main-article hatnote.
- `see-also.mjs`: `:::see-also ... :::` to a related-links section.
- `audience.mjs`: `:::audience tier ... :::` to a tier wrapper used by future
  audience filtering.

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
2. Add or update `*.test.mjs` coverage under `wiki-engine/src/renderer/`.
3. Keep emitted markup deterministic so render manifests remain stable.
