// Shared parser logic for pandoc-fenced-div directives.
//
//   :::name [args...]
//   <body>
//   :::
//
// Each directive (infobox, main-article, see-also, audience) imports
// these helpers and exports its own marked extension. Keeping the
// parsing in one place means the whole family stays consistent if we
// later need to support nested directives, attribute syntax, etc.

// Matches the opening fence line: `:::name` or `::: name [args...]`.
// Captures the name (group 1) and any trailing argument string (group 2).
const OPENING_FENCE = /^:::\s*(\w[\w-]*)(?:[ \t]+(.*))?\n/;

// Returns the byte offset where a fenced div *might* start. Marked
// calls this to skip ahead until a directive is seen — letting the
// rest of the doc parse via the default rules.
export function startFence(src) {
  return src.search(/(^|\n):::/);
}

// Try to extract a fenced-div block whose opening fence has the given
// `name`. Returns `{ raw, args, body }` on match, `undefined` otherwise.
//
// Body content stops at the first closing fence: either `\n:::` (the
// usual case where there's preceding body content) OR `^:::` (an empty
// body — `:::main-article slug\n:::\n` is a single-line directive
// whose closing fence sits at position 0 of `rest`). The `(^|\n)`
// alternation handles both. Nested fenced divs are not supported;
// a nested `:::nested` would close the outer block.
const CLOSE_RE = /(^|\n):::([ \t]*\n|[ \t]*$)/;
export function matchFence(src, name) {
  const open = OPENING_FENCE.exec(src);
  if (!open || open[1] !== name) return undefined;
  const headerLen = open[0].length;
  const rest = src.slice(headerLen);
  const closeMatch = CLOSE_RE.exec(rest);
  if (!closeMatch) return undefined;
  const body = rest.slice(0, closeMatch.index);
  // Consume up to AND INCLUDING the closing fence + its trailing
  // whitespace/newline so the next iteration starts cleanly.
  const consumed = headerLen + closeMatch.index + closeMatch[0].length;
  return {
    raw: src.slice(0, consumed),
    args: (open[2] || '').trim(),
    body,
  };
}

// Title-case a slug for human display. "wiki-engine" → "Wiki engine"
// (sentence case, not full title case — matches the wiki's
// "first word capitalized" convention used in article titles).
export function humanTitleFromSlug(slug) {
  if (!slug) return '';
  const words = slug.replace(/[-_]+/g, ' ').trim();
  return words.charAt(0).toUpperCase() + words.slice(1);
}
