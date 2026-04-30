// TOC generator. Walks the rendered HTML for h2/h3 with id attrs
// and produces the `<nav class="opctx-toc">` block to inject into
// the page shell. Slug convention matches the existing 1Context
// anchors (capitalize-words, replace-spaces-with-underscores) so
// inbound links to existing pages don't break.

const HEADING_RE = /<h([23])\s+id="([^"]+)">([\s\S]*?)<\/h\1>/g;

// Strip inline tags (em, code, strong, etc.) from heading text so
// the TOC link reads as plain prose.
const STRIP_INLINE = /<[^>]+>/g;

export function buildToc(html) {
  const items = [];
  let match;
  while ((match = HEADING_RE.exec(html)) !== null) {
    const [, level, id, inner] = match;
    items.push({
      level: Number(level),
      id,
      text: inner.replace(STRIP_INLINE, '').trim(),
    });
  }
  if (items.length === 0) return '';

  // Build nested OL structure. H2 = top-level, H3 = nested under
  // the most recent H2.
  let html_ = '<ol>\n';
  let inSubList = false;
  items.forEach((item, i) => {
    const next = items[i + 1];
    if (item.level === 2) {
      if (inSubList) {
        html_ += '          </ul>\n        </li>\n';
        inSubList = false;
      }
      const opensSub = next && next.level === 3;
      html_ += `        <li><a href="#${item.id}">${escapeText(item.text)}</a>`;
      if (opensSub) {
        html_ += '\n          <ul>\n';
        inSubList = true;
      } else {
        html_ += '</li>\n';
      }
    } else if (item.level === 3) {
      html_ += `            <li class="is-sub"><a href="#${item.id}">${escapeText(item.text)}</a></li>\n`;
    }
  });
  if (inSubList) html_ += '          </ul>\n        </li>\n';
  html_ += '      </ol>';

  return `<nav class="opctx-toc" aria-label="Table of contents">
      <span class="opctx-toc-label">Contents</span>
      ${html_}
    </nav>`;
}

// Generate a slug from heading text matching the 1Context convention:
// preserve case, preserve dots (so "Status — v0.2.0" → "Status_v0.2.0"
// not "Status_v020"), replace any whitespace run with one underscore,
// drop other punctuation. Used by the markdown renderer when
// assigning ids to h2/h3.
export function slugifyHeading(text) {
  return text
    // Keep word chars + whitespace + hyphens + dots (legal in URL anchors,
    // useful for version numbers like v0.2.0 and slug-like phrases).
    .replace(/[^\w\s.-]/g, '')
    .trim()
    .replace(/\s+/g, '_')
    // Collapse the em-dash artifact: "Status_—_v0.2.0" → "Status_v0.2.0".
    // After we strip the em-dash above we get adjacent underscores; clean.
    .replace(/_+/g, '_')
    .replace(/^_+|_+$/g, '');
}

// Decode common HTML entities (marked already escapes apostrophes
// → `&#39;`, ampersands → `&amp;`, etc. when it renders heading
// text). We need the raw string to safely re-escape into the TOC
// link text without producing `&amp;#39;` doubled-up garbage.
function decodeEntities(text) {
  return text
    .replace(/&#(\d+);/g, (_, code) => String.fromCharCode(parseInt(code, 10)))
    .replace(/&#x([0-9a-fA-F]+);/g, (_, code) => String.fromCharCode(parseInt(code, 16)))
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&apos;/g, "'");
}

function escapeText(text) {
  return decodeEntities(text).replace(/[&<>]/g, (c) =>
    ({ '&': '&amp;', '<': '&lt;', '>': '&gt;' }[c])
  );
}
