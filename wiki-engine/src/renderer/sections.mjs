// Sections extractor. Reads the post-frontmatter markdown body for
// HTML-comment markers that opt sub-page rendering into the
// pipeline. The marker syntax is intentionally invisible in the
// rendered HTML and harmless if the renderer ignores it:
//
//   <!-- section: { slug: "2026-04-26", talk: true, date: "2026-04-26" } -->
//   ## Today · 2026-04-26 · Sunday
//   [content...]
//
// The marker MUST sit immediately before an H2 (with at most one
// blank line between). It applies to that H2 + everything until the
// next H2 (whether marked or not) or end-of-body — so authors don't
// have to mark every section to opt one section in.
//
// Why HTML comments and not a fenced-div directive (`:::section`)?
//   - The H2 stays the canonical heading; the TOC builder finds it
//     unchanged. A fenced-div would need to wrap the H2 + content
//     and then re-emit them, which complicates marked's pipeline.
//   - Comments are invisible to the markdown rendering. Pages that
//     also feed downstream tools (LLMs reading the .md twin) get
//     a quiet metadata channel rather than visible directive syntax.
//   - Authors can drop a marker on existing H2s without restructuring.
//
// Returns: { sections: [{ slug, talk, date, title, anchor, body }],
//            cleanBody: string }
//
// `body` is the slice of the parent body that belongs to that section
// (starting at the H2 line). `cleanBody` is the parent's body with
// the marker comments stripped (so the parent renders without them
// showing up as raw HTML in a literal-comment-stripper).

// Strict marker form: payload must be a `{...}` block. Catches the
// happy case where the author wrote `<!-- section: { slug: "x" } -->`.
const SECTION_MARKER_RE =
  /^[ \t]*<!--[ \t]*section[ \t]*:?[ \t]*(\{[\s\S]*?\})[ \t]*-->[ \t]*$/;

// Loose detector — catches lines that LOOK like an attempted section
// marker (start with `<!-- section`) but didn't match the strict form
// above. Used to fail loudly on malformed markers (missing brace,
// stray text) instead of silently dropping them on the floor.
const SECTION_MARKER_LOOSE_RE = /^[ \t]*<!--[ \t]*section\b/;

// Match an H2 line: `## ...`. Captures the heading text (sans `## `).
const H2_RE = /^##[ \t]+(.+?)[ \t]*$/;

// Slug format used for section sub-page filenames AND URL segments.
// Matches the frontmatter validator's SLUG_RE so a section slug is
// the same flavour as a page slug. Catches `../etc`, `UPPERCASE`,
// empty strings, embedded spaces, etc. — all of which would either
// produce a confusing filename or escape the output dir.
const SECTION_SLUG_RE = /^[a-z0-9][a-z0-9-]{0,59}$/;

// 1Context-convention slug for an anchor — must mirror toc.mjs's
// slugifyHeading exactly so the TOC and the section anchors agree.
import { slugifyHeading } from './toc.mjs';

/**
 * Parse the section JSON-ish payload from a marker comment. The
 * payload is JSON-with-relaxed-keys (single-quoted strings allowed,
 * unquoted keys allowed). We do a forgiving normalization rather
 * than require strict JSON because authors will hand-write these.
 *
 * After parsing, validate semantic invariants:
 *   - `slug` is required and must match SECTION_SLUG_RE
 *   - `talk` if present must be boolean
 *   - `date` if present must be a YYYY-MM-DD string
 * Anything wrong → throw, so the renderer fails loudly. Silent
 * drops are how authors discover their sections didn't render
 * three deploys later.
 */
function parseSectionPayload(raw) {
  // Normalize: convert single-quoted strings to double-quoted, quote
  // bare keys. This is a tiny ad-hoc parser, not a full JSON5.
  const normalized = raw
    // Quote bare keys: { slug: "foo" } → { "slug": "foo" }
    .replace(/([{,]\s*)([A-Za-z_][A-Za-z0-9_]*)\s*:/g, '$1"$2":')
    // Single-quoted string values → double-quoted
    .replace(/:\s*'([^']*)'/g, ': "$1"')
    // Trailing commas before }
    .replace(/,\s*\}/g, '}');
  let parsed;
  try {
    parsed = JSON.parse(normalized);
  } catch (e) {
    throw new Error(
      `section marker payload is not parseable JSON: ${raw}\n  (after normalization: ${normalized})\n  ${e.message}`
    );
  }
  validateSectionPayload(parsed, raw);
  return parsed;
}

function validateSectionPayload(p, raw) {
  if (p === null || typeof p !== 'object' || Array.isArray(p)) {
    throw new Error(`section marker payload must be an object: ${raw}`);
  }
  if (typeof p.slug !== 'string' || p.slug === '') {
    throw new Error(
      `section marker requires a non-empty "slug" string: ${raw}`
    );
  }
  if (!SECTION_SLUG_RE.test(p.slug)) {
    throw new Error(
      `section "slug" "${p.slug}" must match ${SECTION_SLUG_RE} (lowercase, hyphens, alphanumeric start, ≤60 chars). Slugs become filenames + URL segments — paths like "../etc" or uppercase letters break that contract.`
    );
  }
  if (p.talk !== undefined && typeof p.talk !== 'boolean') {
    throw new Error(
      `section "talk" must be a boolean (true/false), got ${JSON.stringify(p.talk)}: ${raw}`
    );
  }
  if (p.date !== undefined) {
    if (typeof p.date !== 'string' || !/^\d{4}-\d{2}-\d{2}$/.test(p.date)) {
      throw new Error(
        `section "date" must be a YYYY-MM-DD string, got ${JSON.stringify(p.date)}: ${raw}`
      );
    }
  }
}

/**
 * Extract sections from a markdown body. Mutates nothing; returns a
 * fresh structure. The caller chooses what to do with the sections
 * (for-you wants per-section sub-page renders; concept pages will
 * have zero sections and pass straight through).
 *
 * If the frontmatter declares a `sections:` list, that list ALSO
 * contributes section configurations (matching by `slug` to a
 * heading anchor); the marker syntax is the primary mechanism but
 * frontmatter declarations work as a fallback.
 *
 * @param {string} body
 * @param {object} [frontmatter]
 * @returns {{ sections: object[], cleanBody: string }}
 */
export function extractSections(body, frontmatter = {}) {
  const lines = body.split('\n');
  const markers = []; // {line, payload}
  // First pass: find marker comments and remember them with their
  // line index so we can match them to the next H2. Lines that
  // *attempt* to be section markers (start with `<!-- section`) but
  // don't match the strict form (e.g. missing close brace, missing
  // `}-->` end) are reported as errors — silent drops are how authors
  // discover their sections didn't render three deploys later.
  for (let i = 0; i < lines.length; i++) {
    const m = SECTION_MARKER_RE.exec(lines[i]);
    if (m) {
      const payload = parseSectionPayload(m[1]);
      markers.push({ line: i, payload });
      continue;
    }
    if (SECTION_MARKER_LOOSE_RE.test(lines[i])) {
      throw new Error(
        `line ${i + 1}: looks like a section marker but doesn't parse. Expected the form\n  <!-- section: { slug: "...", talk: true, date: "..." } -->\nGot:\n  ${lines[i]}`
      );
    }
  }

  // Second pass: walk the body line-by-line tracking H2 boundaries.
  // For each H2, record its line index, heading text, anchor, and
  // any marker that appears in the ≤2 lines immediately above it.
  const headings = []; // {line, text, anchor, marker}
  for (let i = 0; i < lines.length; i++) {
    const m = H2_RE.exec(lines[i]);
    if (!m) continue;
    const text = m[1].trim();
    const anchor = slugifyHeading(text);
    // Look at the previous 1-2 lines for an associated marker.
    let marker = null;
    for (let j = i - 1; j >= Math.max(0, i - 3); j--) {
      const trimmed = lines[j].trim();
      if (trimmed === '') continue;
      const mm = markers.find((x) => x.line === j);
      if (mm) marker = mm.payload;
      break; // first non-blank line above the H2 is the only candidate
    }
    headings.push({ line: i, text, anchor, marker });
  }

  // Third pass: any frontmatter-declared sections that haven't been
  // matched by an inline marker. fm.sections = [{ slug, anchor,
  // talk?, date? }] — anchor matches a heading's slugified text.
  // If an entry references an anchor that doesn't exist in the body,
  // throw — silent drops mean the author thinks they declared a
  // section that never renders.
  const fmSections = Array.isArray(frontmatter.sections) ? frontmatter.sections : [];
  for (const fm of fmSections) {
    if (!fm.anchor) {
      throw new Error(
        `frontmatter sections[] entry missing required "anchor" field: ${JSON.stringify(fm)}`
      );
    }
    const h = headings.find((x) => x.anchor === fm.anchor);
    if (!h) {
      const known = headings.map((x) => x.anchor).join(', ') || '(none)';
      throw new Error(
        `frontmatter sections[] anchor "${fm.anchor}" doesn't match any H2 in the body. Known H2 anchors: ${known}`
      );
    }
    if (h.marker) continue; // inline marker wins
    if (typeof fm.slug !== 'string' || !SECTION_SLUG_RE.test(fm.slug)) {
      throw new Error(
        `frontmatter sections[] entry slug "${fm.slug}" must match ${SECTION_SLUG_RE}`
      );
    }
    h.marker = { ...fm };
  }

  // Build section records. Each section's body starts at its H2 line
  // and ends at the next H2 (marked or not) or end-of-body. If the
  // next H2 has its own marker comment immediately above, scoot the
  // end up so the marker doesn't bleed into this section's body.
  const sections = [];
  const seenSlugs = new Set();
  const parentSlug = typeof frontmatter.slug === 'string' ? frontmatter.slug : null;
  for (let i = 0; i < headings.length; i++) {
    const h = headings[i];
    if (!h.marker) continue;
    const startLine = h.line;
    let endLine = i + 1 < headings.length ? headings[i + 1].line : lines.length;
    // Trim trailing marker / blank lines that belong to the next H2.
    while (endLine > startLine + 1) {
      const candidate = endLine - 1;
      const trimmed = lines[candidate].trim();
      if (trimmed === '') { endLine = candidate; continue; }
      if (SECTION_MARKER_RE.test(lines[candidate])) { endLine = candidate; continue; }
      break;
    }
    const sectionBody = lines.slice(startLine, endLine).join('\n').replace(/\s+$/, '\n');
    const slug = h.marker.slug || h.anchor;
    // Section slug uniqueness within the parent. Two sections sharing
    // a slug would have one render overwrite the other on disk.
    if (seenSlugs.has(slug)) {
      throw new Error(
        `duplicate section slug "${slug}" in this page. Section slugs become filenames; collisions overwrite siblings on disk.`
      );
    }
    seenSlugs.add(slug);
    // A section slug equal to the parent's slug doesn't collide on
    // disk (parent lives at <slug>.html; this section would live at
    // <parent-slug>/<slug>.html), but it's almost always an authoring
    // mistake — surface it loudly.
    if (parentSlug && slug === parentSlug) {
      throw new Error(
        `section slug "${slug}" equals the parent's slug. Sub-page would render at <parent>/<parent>.html, which is confusing — pick a distinct section slug.`
      );
    }
    sections.push({
      slug,
      anchor: h.anchor,
      title: h.text,
      date: h.marker.date || null,
      talk: !!h.marker.talk,
      body: sectionBody,
    });
  }

  // cleanBody: drop marker comment lines from the parent so the
  // rendered HTML doesn't include them. Markdown comments wouldn't
  // render anyway (marked treats `<!-- ... -->` as a comment), but
  // dropping them keeps the .md twin tidy.
  const markerLines = new Set(markers.map((m) => m.line));
  const cleanBody = lines.filter((_, i) => !markerLines.has(i)).join('\n');

  return { sections, cleanBody };
}

/**
 * Build the frontmatter object for a section sub-page. The parent
 * frontmatter is the base; section-specific overrides (title, slug,
 * date, talk_url) layer on top.
 *
 * @param {object} parentFm
 * @param {object} section  { slug, title, date, talk, anchor }
 * @param {string} parentSlug
 */
export function deriveSectionFrontmatter(parentFm, section, parentSlug) {
  const fm = { ...parentFm };
  fm.title = section.title;
  fm.slug = section.slug;
  fm.parent_slug = parentSlug;
  fm.parent_anchor = section.anchor;
  // The section's own md_url points to its own .md sibling.
  fm.md_url = `./${section.slug}.md`;
  if (section.date) fm.section_date = section.date;
  if (section.talk) {
    fm.talk_enabled = true;
    fm.talk_url = `./${section.slug}.talk.md`;
  } else {
    fm.talk_enabled = false;
    delete fm.talk_url;
  }
  // Sub-pages don't carry the parent's `sections` list (avoid recursion).
  delete fm.sections;
  return fm;
}

/**
 * Stringify a frontmatter object back to YAML for the .md twin. We
 * keep this minimal — quote strings, write lists/booleans/dates raw.
 * Anything more complex is out of scope (gray-matter has a stringify
 * but pulls a heavy dep tree).
 */
export function stringifyFrontmatter(fm) {
  const lines = ['---'];
  for (const [k, v] of Object.entries(fm)) {
    if (v === undefined || v === null) continue;
    if (Array.isArray(v)) {
      const inner = v.map((x) => (typeof x === 'string' ? x : String(x))).join(', ');
      lines.push(`${k}: [${inner}]`);
    } else if (typeof v === 'boolean' || typeof v === 'number') {
      lines.push(`${k}: ${v}`);
    } else if (v instanceof Date) {
      lines.push(`${k}: ${v.toISOString().slice(0, 10)}`);
    } else {
      const s = String(v);
      // Quote if contains characters that YAML treats specially.
      if (/[:#&*!|>'"%@`\n]/.test(s) || s.trim() !== s) {
        lines.push(`${k}: ${JSON.stringify(s)}`);
      } else {
        lines.push(`${k}: ${s}`);
      }
    }
  }
  lines.push('---');
  return lines.join('\n') + '\n';
}

/**
 * Build an initial talk-page markdown surface for a section. Just
 * enough so the agent endpoint exists and downstream tools can find it.
 */
export function buildTalkStub(parentFm, section, parentSlug) {
  const talkFm = {
    title: `Talk · ${section.title}`,
    slug: `${section.slug}-talk`,
    section: parentFm.section || 'product',
    access: parentFm.access || 'public',
    summary: `Talk page for ${section.title}.`,
    parent_slug: parentSlug,
    parent_anchor: section.anchor,
    status: 'draft',
    talk_enabled: false,
    footer_enabled: parentFm.footer_enabled !== false,
  };
  const fmYaml = stringifyFrontmatter(talkFm);
  const bodyLines = [
    '',
    `# Talk · ${section.title}`,
    '',
    `> Talk-page sibling for the [${section.title}](./${section.slug}.md)`,
    `> section. Hourly conversations and revision proposals will`,
    `> arrive here once the daemon publishes them.`,
    '',
    '*No discussion yet. Add a timestamped entry when this section has a proposal, question, decision, or memory note worth preserving.*',
    '',
  ];
  return fmYaml + bodyLines.join('\n');
}
