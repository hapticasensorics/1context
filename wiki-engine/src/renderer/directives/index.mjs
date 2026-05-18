// Aggregator for the four wiki-engine markdown directives. The
// renderer (`../index.mjs`) imports `directives` from here and
// passes them all to `marked.use({ extensions: directives })` in
// one call. Adding a new directive: drop a file alongside, import
// it here, push into the array.

import infobox     from './infobox.mjs';
import mainArticle from './main-article.mjs';
import seeAlso     from './see-also.mjs';
import audience    from './audience.mjs';

export const directives = [
  infobox,
  mainArticle,
  seeAlso,
  audience,
];
