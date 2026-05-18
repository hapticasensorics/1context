// Talk-folder renderer — assembles a talk page from a directory of
// per-entry markdown files.
//
// Layout (Maildir / mailing-list inspired):
//
//   <base>.<audience>.talk/
//     _meta.yaml                              ← page-level frontmatter
//     2026-04-19T22-00Z.conversation.md       ← one entry per file
//     2026-04-19T23-00Z.conversation.md
//     2026-04-21T14-30Z.proposal.split-quality.md
//     2026-04-21T17-45Z.reply.split-quality.md  ← `parent:` in frontmatter
//     ...
//
// Each entry file has its own frontmatter (kind, author, ts, parent,
// trailers) plus a body. The folder renderer scans the directory,
// sorts entries by filename (ISO-timestamp-prefixed), groups replies
// under parents recursively, and composes one assembled HTML body
// that the existing renderShell wraps with the page chrome.
//
// Filename convention:
//
//   <ISO-UTC-timestamp>.<kind>[.<short-slug>].md
//
// where <ISO-UTC-timestamp> is `YYYY-MM-DDTHH-MMZ` (colons replaced with
// hyphens for filesystem-safety; second/sub-second precision optional).
// Kind is one of: conversation, proposal, concern, question, decided,
// rfc, synthesis, verify, merge, split, move, cleanup, reply.
// Short slug is required for topic-driven kinds, omitted for
// conversation entries (timestamp identifies them).
//
// Per-entry frontmatter shape:
//
//   ---
//   kind: conversation | proposal | concern | reply | …
//   author: <model-id> | <username@domain>
//   ts: 2026-04-19T22:00:00Z      ← canonical timestamp (display)
//   parent: <filename>            ← only for replies
//   closes: PR #14                ← LKML-style trailers, optional
//   decided-by: paul@example.com
//   ---
//   [entry body markdown]
//
// The returned `frontmatter` object is what renderShell expects as the
// page-level frontmatter (read from _meta.yaml). The returned
// `bodyHtml` is the assembled, threaded, HTML-rendered content body
// ready to drop into the article shell.

import { readFileSync, readdirSync, statSync } from 'node:fs';
import { resolve, basename, join } from 'node:path';
import matter from 'gray-matter';
import yaml from 'js-yaml';
import { Marked } from 'marked';

const ESCAPE_MAP = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const escape = (s) => String(s).replace(/[&<>"']/g, (c) => ESCAPE_MAP[c]);

function makeSafeMarked() {
  const marked = new Marked();
  marked.use({
    renderer: {
      html({ text, raw }) {
        return escape(text || raw || '');
      },
    },
  });
  return marked;
}

// Kinds we know how to group into top-level sections in the
// assembled page. Anything else falls into a generic "Other" bucket.
const SECTION_ORDER = [
  { kind: 'conversation', heading: 'Conversations' },
  { kind: 'proposal',     heading: 'Proposals' },
  { kind: 'rfc',          heading: 'RFCs' },
  { kind: 'concern',      heading: 'Concerns' },
  { kind: 'question',     heading: 'Questions' },
  { kind: 'synthesis',    heading: 'Synthesis questions' },
  { kind: 'verify',       heading: 'Verification asks' },
  { kind: 'decided',      heading: 'Decided' },
  { kind: 'cleanup',      heading: 'Cleanup' },
  { kind: 'merge',        heading: 'Merge proposals' },
  { kind: 'split',        heading: 'Split proposals' },
  { kind: 'move',         heading: 'Move proposals' },
];

function isEntryFile(name) {
  if (name.startsWith('_') || name.startsWith('.')) return false;
  if (!name.endsWith('.md')) return false;
  return true;
}

// Parse `<filename-without-ext>` to extract the kind and (optional)
// short slug. Filename convention:
//   <timestamp>.<kind>[.<slug>].md
// Returns { kind, slug } where slug is null for two-segment names
// (e.g., conversations).
function parseEntryFilename(name) {
  // Strip extension
  const stem = name.endsWith('.md') ? name.slice(0, -3) : name;
  // Find first '.' AFTER the timestamp segment. The timestamp itself
  // looks like `2026-04-19T22-00Z` and contains hyphens but no dots.
  const firstDot = stem.indexOf('.');
  if (firstDot < 0) return { kind: 'conversation', slug: null };
  const after = stem.slice(firstDot + 1);
  const secondDot = after.indexOf('.');
  if (secondDot < 0) return { kind: after, slug: null };
  return { kind: after.slice(0, secondDot), slug: after.slice(secondDot + 1) };
}

// Read all entry files. Returns an array of entry objects sorted by
// filename (which is timestamp-prefixed → chronological).
function readEntries(folderPath) {
  const all = readdirSync(folderPath);
  const entries = [];
  for (const name of all) {
    if (!isEntryFile(name)) continue;
    const fullPath = join(folderPath, name);
    const stat = statSync(fullPath);
    if (!stat.isFile()) continue;
    const raw = readFileSync(fullPath, 'utf8');
    const { data, content } = matter(raw);
    const filenameMeta = parseEntryFilename(name);
    // YAML auto-parses ISO timestamps as Date objects. Normalize to
    // an ISO-8601 string so downstream rendering and the .md twin
    // produce stable text.
    let ts = data.ts;
    if (ts instanceof Date) ts = ts.toISOString();
    if (ts != null && typeof ts !== 'string') ts = String(ts);
    entries.push({
      filename: name,
      stem: name.endsWith('.md') ? name.slice(0, -3) : name,
      kind: data.kind || filenameMeta.kind,
      slug: filenameMeta.slug,
      author: data.author || 'unknown',
      ts: ts || null,
      parent: data.parent || null,
      trailers: pickTrailers(data),
      body: content.trim(),
    });
  }
  entries.sort((a, b) => a.filename.localeCompare(b.filename));
  return entries;
}

// Split known LKML-style trailer keys out of the frontmatter object.
// Renderer formats them as a small trailer list at the bottom of each
// entry. New keys are accepted; the renderer just lists what's there.
const TRAILER_KEYS = [
  'closes', 'fixes', 'resolves', 'decided-by', 'reported-by',
  'acked-by', 'reviewed-by', 'tested-by', 'suggested-by',
  'co-developed-by', 'superseded-by', 'blocked-on',
];
function pickTrailers(fm) {
  const out = {};
  for (const k of TRAILER_KEYS) {
    if (fm[k] != null) out[k] = fm[k];
    // Frontmatter parsers normalize hyphens differently across yaml
    // dialects; accept underscored keys too for forgiveness.
    const underscored = k.replace(/-/g, '_');
    if (fm[underscored] != null) out[k] = fm[underscored];
  }
  return out;
}

// Build a parent → [reply, …] map for threading.
function buildReplyMap(entries) {
  const m = new Map();
  for (const e of entries) {
    if (!e.parent) continue;
    if (!m.has(e.parent)) m.set(e.parent, []);
    m.get(e.parent).push(e);
  }
  return m;
}

// Render one entry to HTML (heading + body + signature + trailers +
// nested replies recursively). `depth` controls blockquote nesting.
function renderEntry(entry, replyMap, marked, depth = 0) {
  const headingId = slugify(entry.stem);
  let heading;
  if (entry.kind === 'conversation') {
    // Hourly Conversations: bracketless timestamp heading.
    heading = `<h2 id="${escape(headingId)}">${escape(formatTimestamp(entry.ts))}</h2>`;
  } else {
    // Topic-driven: bracketed kind + descriptive subject.
    const subject = entry.slug
      ? entry.slug.replace(/-/g, ' ').replace(/^./, (c) => c.toUpperCase())
      : entry.kind;
    heading = `<h2 id="${escape(headingId)}">[${escape(entry.kind.toUpperCase())}] ${escape(subject)}</h2>`;
  }

  const bodyHtml = marked.parse(entry.body);
  const sigHtml = `<p class="opctx-talk-entry-sig"><em>— ${escape(entry.author)} · ${escape(entry.ts || '')}</em></p>`;
  const trailersHtml = renderTrailers(entry.trailers);

  // Replies — render under this entry, blockquote-nested.
  const replies = (replyMap.get(entry.filename) || replyMap.get(entry.stem) || []);
  const repliesHtml = replies
    .map((r) => `<blockquote class="opctx-talk-reply">${renderEntry(r, replyMap, marked, depth + 1)}</blockquote>`)
    .join('\n');

  return `<article class="opctx-talk-entry" data-kind="${escape(entry.kind)}">${heading}\n${bodyHtml}\n${sigHtml}${trailersHtml}${repliesHtml}</article>`;
}

function renderTrailers(trailers) {
  const keys = Object.keys(trailers);
  if (keys.length === 0) return '';
  const lines = keys.map((k) => {
    const label = k.split('-').map((p) => p.charAt(0).toUpperCase() + p.slice(1)).join('-');
    return `<dt>${escape(label)}:</dt><dd>${escape(String(trailers[k]))}</dd>`;
  }).join('');
  return `<dl class="opctx-talk-trailers">${lines}</dl>`;
}

function formatTimestamp(ts) {
  if (!ts) return '(no timestamp)';
  // Display as YYYY-MM-DD · HH:MM UTC
  const m = /^(\d{4}-\d{2}-\d{2})T(\d{2}):(\d{2})/.exec(ts);
  if (!m) return ts;
  return `${m[1]} · ${m[2]}:${m[3]} UTC`;
}

function slugify(s) {
  return s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '');
}

// Compose the full body HTML by section. Replies are nested under
// their parents (recursively); top-level entries are grouped by kind
// in SECTION_ORDER.
function composeBody(entries, replyMap, marked, lede, seeAlso, curatorMd) {
  const topLevel = entries.filter((e) => !e.parent);
  const byKind = new Map();
  for (const e of topLevel) {
    if (!byKind.has(e.kind)) byKind.set(e.kind, []);
    byKind.get(e.kind).push(e);
  }

  const parts = [];

  if (lede) {
    parts.push(`<blockquote class="opctx-talk-lede">${marked.parse(lede)}</blockquote>`);
  }

  // Curator prompt — if the talk folder has a `_curator.md` file,
  // surface it as a collapsed <details> block. The operator can
  // edit `_curator.md` directly to change how the curator agent
  // behaves; the rendered banner makes it discoverable.
  if (curatorMd) {
    const curatorHtml = marked.parse(curatorMd);
    parts.push(`<details class="opctx-talk-curator-prompt"><summary><strong>Curator prompt for this page</strong> <span class="opctx-talk-curator-hint">click to expand · edit <code>_curator.md</code> in this talk folder to change how the curator agent for this page behaves</span></summary><div class="opctx-talk-curator-prompt-body">${curatorHtml}</div></details>`);
  }

  for (const { kind, heading } of SECTION_ORDER) {
    const list = byKind.get(kind);
    if (!list || list.length === 0) continue;
    parts.push(`<section class="opctx-talk-section" data-kind="${escape(kind)}"><h2 id="${escape(slugify(kind))}">${escape(heading)}</h2>`);
    for (const e of list) {
      parts.push(renderEntry(e, replyMap, marked));
    }
    parts.push(`</section>`);
    byKind.delete(kind);
  }

  // Anything left over (unknown kinds): one "Other" section.
  if (byKind.size > 0) {
    parts.push(`<section class="opctx-talk-section" data-kind="other"><h2 id="other">Other</h2>`);
    for (const [, list] of byKind) {
      for (const e of list) parts.push(renderEntry(e, replyMap, marked));
    }
    parts.push(`</section>`);
  }

  if (topLevel.length === 0) {
    parts.push(`<p class="opctx-talk-empty"><em>No discussion yet. Add a timestamped entry here when there is a proposal, question, decision, or memory note worth preserving.</em></p>`);
  }

  if (seeAlso && seeAlso.length > 0) {
    parts.push(`<section class="opctx-talk-section opctx-talk-see-also"><h2 id="see-also">See also</h2><ul>`);
    for (const item of seeAlso) {
      parts.push(`<li><a href="${escape(item.url)}">${escape(item.text)}</a></li>`);
    }
    parts.push(`</ul></section>`);
  }

  return parts.join('\n');
}

/**
 * Render a talk-folder into the same shape that renderPage produces
 * for a single .md file: { html, md, frontmatter, sections }.
 *
 * @param {string} folderPath - absolute path to the .talk/ directory
 * @returns {{ frontmatter: object, bodyHtml: string, mdAssembled: string, entries: object[] }}
 */
export function renderTalkFolder(folderPath) {
  const metaPath = join(folderPath, '_meta.yaml');
  const metaRaw = readFileSync(metaPath, 'utf8');
  const frontmatter = yaml.load(metaRaw) || {};
  const lede = frontmatter.lede || null;
  const seeAlso = Array.isArray(frontmatter.see_also) ? frontmatter.see_also : null;

  // Curator prompt — if a `_curator.md` file lives in the talk
  // folder, surface it on the rendered page as a collapsed block.
  // The curator agent for this page reads this file at runtime;
  // the operator edits it in place to change curator behavior.
  let curatorMd = null;
  const curatorPath = join(folderPath, '_curator.md');
  try {
    const raw = readFileSync(curatorPath, 'utf8');
    const { content } = matter(raw);
    curatorMd = content.trim();
  } catch {
    // No curator prompt for this talk folder — that's fine; not
    // every talk page has a curator agent.
  }

  const entries = readEntries(folderPath);
  const replyMap = buildReplyMap(entries);
  const marked = makeSafeMarked();
  const bodyHtml = composeBody(entries, replyMap, marked, lede, seeAlso, curatorMd);

  // Markdown twin — the assembled .md a reader/agent gets when they
  // request the URL with a trailing .md. We re-emit the entries in
  // chronological order, separated by blank lines, with their
  // headings + bodies. Threading isn't represented in the .md twin
  // (replies appear in their natural chronological order with
  // `parent:` frontmatter intact when the agent reads the folder
  // directly). Lede and see-also are preserved.
  const mdLines = [];
  if (lede) mdLines.push(`> ${lede.replace(/\n/g, '\n> ')}\n`);
  for (const e of entries) {
    if (e.kind === 'conversation') {
      mdLines.push(`## ${formatTimestamp(e.ts)}\n`);
    } else {
      const subject = e.slug
        ? e.slug.replace(/-/g, ' ').replace(/^./, (c) => c.toUpperCase())
        : e.kind;
      mdLines.push(`## [${e.kind.toUpperCase()}] ${subject}\n`);
    }
    mdLines.push(e.body + '\n');
    mdLines.push(`— *${e.author} · ${e.ts || ''}*\n`);
    const trailerKeys = Object.keys(e.trailers);
    if (trailerKeys.length) {
      for (const k of trailerKeys) {
        const label = k.split('-').map((p) => p.charAt(0).toUpperCase() + p.slice(1)).join('-');
        mdLines.push(`${label}: ${e.trailers[k]}`);
      }
      mdLines.push('');
    }
  }
  if (entries.length === 0) {
    mdLines.push('*No discussion yet. Add a timestamped entry here when there is a proposal, question, decision, or memory note worth preserving.*\n');
  }
  if (seeAlso && seeAlso.length) {
    mdLines.push('## See also\n');
    for (const item of seeAlso) mdLines.push(`- [${item.text}](${item.url})`);
    mdLines.push('');
  }
  const mdAssembled = mdLines.join('\n');

  return { frontmatter, bodyHtml, mdAssembled, entries };
}
