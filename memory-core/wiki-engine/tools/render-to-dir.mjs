#!/usr/bin/env node
// Minimal CLI: render one .md file to HTML in a specified output dir.
//
//   node wiki-engine/tools/render-to-dir.mjs <input.md> <output-dir>
//
// Writes:
//   <output-dir>/<slug>.html         — the parent page (always)
//   <output-dir>/<slug>.md           — the markdown twin (always)
//
// And if the source declares any sections (via inline `<!-- section: ... -->`
// markers or the frontmatter `sections:` list):
//   <output-dir>/<slug>/<section-slug>.html      — agent-friendly sub-page
//   <output-dir>/<slug>/<section-slug>.md         — markdown twin
//   <output-dir>/<slug>/<section-slug>.talk.md   — talk-page stub (when talk: true)
//
// Time-based URL versioning side-effect: any slug of the form
// `<family>-<YYYY-MM-DD>` updates `<output-dir>/latest_for_family.json`
// so that `/<owner>/<family>` can be redirected to the latest dated
// child. The JSON is read+merged each run so multiple invocations
// (one per .md file) accumulate into one file a redirect step can
// consume.

import { readFileSync, writeFileSync, mkdirSync, existsSync, statSync } from 'node:fs';
import { dirname, resolve, basename, extname, join, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import matter from 'gray-matter';
import { Marked } from 'marked';
import { renderPage, FrontmatterError } from '../src/renderer/index.mjs';
import { renderTalkFolder } from '../src/renderer/talk-folder.mjs';
import { renderShell } from '../src/renderer/template.mjs';
import { stringifyFrontmatter } from '../src/renderer/sections.mjs';
import { buildToc } from '../src/renderer/toc.mjs';

// Repo root for resolving talk-conventions paths. This file lives at
// <repo>/wiki-engine/tools/render-to-dir.mjs, so two levels up is the
// repo root.
const __filename = fileURLToPath(import.meta.url);
const REPO_ROOT = resolve(dirname(__filename), '../..');

const ESCAPE_MAP = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const escapeHtml = (value) => String(value).replace(/[&<>"']/g, (character) => ESCAPE_MAP[character]);

function makeSafeMarked() {
  const marked = new Marked();
  marked.use({
    renderer: {
      html({ text, raw }) {
        return escapeHtml(text || raw || '');
      },
    },
  });
  return marked;
}

function sourceBaseDir(sourcePath) {
  try {
    return statSync(sourcePath).isDirectory() ? sourcePath : dirname(sourcePath);
  } catch {
    return dirname(sourcePath);
  }
}

function looksLikePath(value) {
  return (
    value.startsWith('.') ||
    value.includes('/') ||
    value.includes('\\') ||
    /\.[A-Za-z0-9]+$/.test(value)
  );
}

function humanizeTalkConventionKey(key) {
  if (!key || typeof key !== 'string') return 'Talk';
  return basename(key, extname(key))
    .replace(/[-_]+/g, ' ')
    .replace(/\b\w/g, (ch) => ch.toUpperCase());
}

function resolveTalkConventionsPath(frontmatter, sourcePath) {
  const explicitPath = frontmatter.talk_conventions_path || frontmatter.talk_conventions_file;
  const key = frontmatter.talk_conventions;
  const declaredPath = explicitPath || (
    typeof key === 'string' && looksLikePath(key) ? key : null
  );
  if (!declaredPath) {
    if (key) {
      console.warn(
        `talk_conventions "${key}" declared without talk_conventions_path; skipping conventions banner`
      );
    }
    return null;
  }
  if (typeof declaredPath !== 'string') {
    console.warn(`talk_conventions path must be a string: ${JSON.stringify(declaredPath)}`);
    return null;
  }
  if (isAbsolute(declaredPath)) return declaredPath;

  const fromSource = resolve(sourceBaseDir(sourcePath), declaredPath);
  if (existsSync(fromSource)) return fromSource;
  return resolve(REPO_ROOT, declaredPath);
}

// Load the talk-conventions banner for a given frontmatter, returning
// { html, label } or null if no conventions are declared / found.
//
// The conventions doc is a regular markdown file (with possible
// frontmatter); we strip frontmatter, render the body to HTML with a
// minimal Marked instance, and produce a one-line label suitable for
// the collapsed <summary> banner.
function loadTalkConventions(frontmatter, sourcePath) {
  if (
    !frontmatter.talk_conventions &&
    !frontmatter.talk_conventions_path &&
    !frontmatter.talk_conventions_file
  ) {
    return null;
  }
  const fullPath = resolveTalkConventionsPath(frontmatter, sourcePath);
  if (!fullPath) return null;
  if (!existsSync(fullPath)) {
    console.warn(`talk_conventions file not found: ${fullPath}`);
    return null;
  }
  const raw = readFileSync(fullPath, 'utf8');
  const { content } = matter(raw);
  const marked = makeSafeMarked();
  const html = marked.parse(content);
  const audience = frontmatter.talk_audience || 'private';
  // Capitalize first letter for label
  const audienceLabel = audience.charAt(0).toUpperCase() + audience.slice(1);
  const kindLabel = humanizeTalkConventionKey(frontmatter.talk_conventions || fullPath);
  const label = `${kindLabel} ${audienceLabel.toLowerCase()} talk page · Wikipedia + LKML conventions, append-only`;
  return { html, label };
}

// Detect when the input path points at a talk-folder (a directory
// whose name ends in `.talk` and which contains a `_meta.yaml`).
// These are rendered through renderTalkFolder rather than renderPage.
function inputIsTalkFolder(inputPath) {
  let st;
  try { st = statSync(inputPath); } catch { return false; }
  if (!st.isDirectory()) return false;
  const name = basename(inputPath.replace(/\/$/, ''));
  if (!/\.talk$/.test(name)) return false;
  return existsSync(join(inputPath, '_meta.yaml'));
}

// Match `<family>-<YYYY-MM-DD>` where the family is everything before
// the trailing date. Family must be non-empty. The date portion is
// pattern-only (digit shape); we revalidate semantics below before
// emitting a family entry, because `2026-13-42` matches the regex
// shape but isn't a real date.
const DATED_SLUG_RE = /^(.+)-(\d{4})-(\d{2})-(\d{2})$/;
const AUDIENCE_ORDER = ['private', 'internal', 'public'];

function isValidYmd(y, m, d) {
  const yi = Number(y), mi = Number(m), di = Number(d);
  if (mi < 1 || mi > 12 || di < 1 || di > 31) return false;
  // Cheap calendar sanity check via Date round-trip. Catches
  // 2026-04-31 (Apr has 30 days), 2026-02-30, etc.
  const dt = new Date(`${y}-${m}-${d}T00:00:00Z`);
  if (isNaN(dt.getTime())) return false;
  return (
    dt.getUTCFullYear() === yi &&
    dt.getUTCMonth() + 1 === mi &&
    dt.getUTCDate() === di
  );
}

/**
 * Maintain `latest_for_family.json` so /<owner>/<family> can redirect
 * to the latest dated parent.
 *
 * A page is considered a family member when EITHER:
 *   - frontmatter declares `family: <name>` (explicit opt-in), or
 *   - the slug matches `<family>-YYYY-MM-DD` AND the date is real
 *     AND the frontmatter does NOT carry `family: false` (implicit
 *     opt-out).
 *
 * The opt-in/opt-out path keeps a concept page accidentally named
 * `meeting-2026-01-01` from being silently promoted into a "meeting"
 * family. Authors who want the slug-pattern fallback don't need to
 * change anything; authors who don't want it set `family: false`.
 */
function detectFamily(slug, frontmatter) {
  // Explicit declaration wins. `family: false` opts out. `family: <s>`
  // opts in (and overrides the slug-pattern family name).
  if (frontmatter && Object.prototype.hasOwnProperty.call(frontmatter, 'family')) {
    if (frontmatter.family === false) return null;
    if (typeof frontmatter.family === 'string' && frontmatter.family) {
      // Need a date too — either from frontmatter (`family_date`) or
      // from the slug suffix.
      const dateFromFm = typeof frontmatter.family_date === 'string'
        ? frontmatter.family_date
        : null;
      const m = DATED_SLUG_RE.exec(slug);
      const dateFromSlug = m && isValidYmd(m[2], m[3], m[4])
        ? `${m[2]}-${m[3]}-${m[4]}`
        : null;
      const date = dateFromFm || dateFromSlug;
      if (!date) {
        throw new Error(
          `frontmatter declares family="${frontmatter.family}" but provides no date. Add family_date: YYYY-MM-DD or use a slug of the form <family>-YYYY-MM-DD.`
        );
      }
      return { family: frontmatter.family, date };
    }
  }
  // Slug-pattern fallback.
  const m = DATED_SLUG_RE.exec(slug);
  if (!m) return null;
  const [, family, y, mo, d] = m;
  if (!isValidYmd(y, mo, d)) return null;
  return { family, date: `${y}-${mo}-${d}` };
}

function updateLatestForFamily(outDir, slug, frontmatter) {
  const detected = detectFamily(slug, frontmatter);
  if (!detected) return null;
  const { family, date } = detected;

  const path = join(outDir, 'latest_for_family.json');
  let data = {};
  if (existsSync(path)) {
    try {
      data = JSON.parse(readFileSync(path, 'utf8'));
    } catch {
      data = {};
    }
  }
  // Compare ISO date strings — lexicographic equals chronological.
  const prev = data[family];
  if (!prev || (typeof prev.date === 'string' && prev.date < date)) {
    data[family] = { slug, date };
  }
  writeFileSync(path, JSON.stringify(data, null, 2) + '\n');

  // Also maintain a per-family index — `<family>-index.json` —
  // enumerating every known dated snapshot in this family, most-recent
  // first. The version-dropdown chrome reads this at runtime to
  // populate its menu; the infinite-scroll behavior reads it to know
  // what to lazy-load when the reader scrolls past the bottom of the
  // current snapshot. Same merge-on-disk pattern as
  // latest_for_family.json so multiple render-to-dir.mjs invocations
  // (one per .md file) accumulate into a single index.
  updateFamilyIndex(outDir, family, slug, date, frontmatter);
  return { family, date };
}

/**
 * Maintain `<family>-index.json` — a list of every dated snapshot
 * in this family, most-recent first.
 *
 * Schema:
 *   {
 *     "family": "for-you",
 *     "members": [
 *       {
 *         "slug": "for-you-2026-04-26",
 *         "date": "2026-04-26",
 *         "title": "For You · Paul · Sunday, April 26, 2026",
 *         "url":   "for-you-2026-04-26.html"
 *       },
 *       ...
 *     ]
 *   }
 *
 * `url` is relative — the chrome resolves it against
 * `location.pathname`'s owner segment so the same JSON works for
 * any owner. The chrome can fetch this file from the same directory
 * the parent page lives in.
 *
 * Re-render of an existing slug replaces its entry rather than
 * duplicating it, so the index converges to truth-on-disk.
 */
function updateFamilyIndex(outDir, family, slug, date, frontmatter) {
  const path = join(outDir, `${family}-index.json`);
  let data = { family, members: [] };
  if (existsSync(path)) {
    try {
      const parsed = JSON.parse(readFileSync(path, 'utf8'));
      if (parsed && typeof parsed === 'object'
          && parsed.family === family
          && Array.isArray(parsed.members)) {
        data = parsed;
      }
    } catch {
      // fall through with fresh data
    }
  }
  // Replace any existing entry for this slug; preserve all others.
  const others = data.members.filter((m) => m && m.slug !== slug);
  others.push({
    slug,
    date,
    title: typeof frontmatter.title === 'string' ? frontmatter.title : slug,
    url: `${slug}.html`,
  });
  // Sort most-recent first; ISO date strings sort lexicographically
  // = chronologically. Stable secondary sort by slug to keep
  // determinism when two members share a date (shouldn't happen but
  // belt-and-suspenders).
  others.sort((a, b) => {
    const d = (b.date || '').localeCompare(a.date || '');
    if (d !== 0) return d;
    return (a.slug || '').localeCompare(b.slug || '');
  });
  data.members = others;
  writeFileSync(path, JSON.stringify(data, null, 2) + '\n');
}

function titleCaseAudience(key) {
  return key.charAt(0).toUpperCase() + key.slice(1);
}

function resolveAudienceVariants(inputPath, frontmatter) {
  const cfg = frontmatter && frontmatter.audiences;
  if (cfg && (typeof cfg !== 'object' || Array.isArray(cfg))) {
    throw new Error(`frontmatter audiences must be an object when present`);
  }

  const stem = basename(inputPath, extname(inputPath));
  const baseDir = dirname(inputPath);
  const siblingPublic = resolve(baseDir, `${stem}.public.md`);
  const siblingInternal = resolve(baseDir, `${stem}.internal.md`);
  const hasSiblingTiers = existsSync(siblingPublic) || existsSync(siblingInternal);

  if (!cfg && !hasSiblingTiers) return null;

  if (!cfg && hasSiblingTiers) {
    if (existsSync(siblingInternal) && !existsSync(siblingPublic)) {
      throw new Error(
        `Tier source ${siblingInternal} exists but ${siblingPublic} is missing. Refusing to render public output from the private canonical source.`
      );
    }
    const streams = {};
    if (existsSync(siblingPublic)) {
      streams.public = {
        key: 'public',
        label: 'Public',
        url: `${frontmatter.slug}.html`,
        sourcePath: siblingPublic,
      };
    }
    if (existsSync(siblingInternal)) {
      streams.internal = {
        key: 'internal',
        label: 'Internal',
        url: `${frontmatter.slug}.internal.html`,
        sourcePath: siblingInternal,
      };
    }
    streams.private = {
      key: 'private',
      label: 'Private',
      url: `${frontmatter.slug}.private.html`,
      sourcePath: inputPath,
    };
    return { order: AUDIENCE_ORDER.slice(), streams };
  }

  const streams = {
    public: {
      key: 'public',
      label: 'Public',
      url: `${frontmatter.slug}.html`,
      sourcePath: inputPath,
    },
  };
  let hasAlternate = false;

  for (const key of AUDIENCE_ORDER) {
    const entry = cfg[key];
    if (entry === undefined || entry === false || entry === null) continue;
    if (key !== 'public') hasAlternate = true;
    if (entry !== true && typeof entry !== 'string') {
      throw new Error(
        `frontmatter audiences.${key} must be true or a relative path (got ${JSON.stringify(entry)})`
      );
    }
    const sourcePath = entry === true
      ? resolve(baseDir, `${stem}.${key}.md`)
      : resolve(baseDir, entry);
    streams[key] = {
      key,
      label: titleCaseAudience(key),
      url: key === 'public'
        ? `${frontmatter.slug}.html`
        : `${frontmatter.slug}.${key}.html`,
      sourcePath,
    };
  }

  if (hasAlternate && !cfg.public) {
    throw new Error(
      `frontmatter audiences declares non-public streams for ${inputPath} but no audiences.public source. Refusing to render public output from the private canonical source.`
    );
  }
  if (!hasAlternate) return null;
  return { order: AUDIENCE_ORDER.slice(), streams };
}

function buildAudienceSource(baseFrontmatter, rawSource) {
  const { data, content } = matter(rawSource);
  const merged = { ...baseFrontmatter, ...data };
  return stringifyFrontmatter(merged) + '\n' + content;
}

function cleanGeneratedText(value) {
  return String(value).replace(/[ \t]+$/gm, '');
}

function main() {
  const [,, inputPath, outDir] = process.argv;
  if (!inputPath || !outDir) {
    console.error('Usage: render-to-dir.mjs <input.md> <output-dir>');
    process.exit(2);
  }

  // Two input modes:
  //   1. Single .md file → existing renderPage pipeline.
  //   2. .talk/ directory containing `_meta.yaml` and per-entry .md
  //      files → folder-assembly pipeline (renderTalkFolder).
  // Decided up front so both branches share the output-emit code below.
  const isTalkFolder = inputIsTalkFolder(inputPath);

  let result;
  let slug;

  if (isTalkFolder) {
    // Folder mode — assemble from individual entry files.
    const folderName = basename(inputPath.replace(/\/$/, ''));
    // Folder is "<slug>.talk" → slug ends in `.talk`.
    slug = folderName;

    const folder = renderTalkFolder(inputPath);
    const fm = folder.frontmatter;
    if (!fm.slug) fm.slug = slug;
    if (!fm.access && fm.talk_audience) fm.access = fm.talk_audience;

    const talkConventions = loadTalkConventions(fm, inputPath);
    let tocHtml = buildToc(folder.bodyHtml);
    // Empty buildToc returns ''. The layout grid reserves the
    // sidebar column unconditionally, so an empty tocHtml leaves
    // the column slotted with zero content and `<main>` falls
    // into the narrow first column. Emit a minimal placeholder
    // nav so the article gets the second (wide) column.
    if (!tocHtml || !/<nav/.test(tocHtml)) {
      tocHtml = `<nav class="opctx-toc opctx-toc--talk-empty" aria-label="Talk page navigation"></nav>`;
    }
    const html = renderShell({
      frontmatter: fm,
      bodyHtml: folder.bodyHtml,
      tocHtml,
      talkConventionsHtml: talkConventions ? talkConventions.html : null,
      talkConventionsLabel: talkConventions ? talkConventions.label : null,
    });
    // Build the .md twin: frontmatter (re-stringified) + assembled body.
    const md = stringifyFrontmatter(fm) + '\n' + folder.mdAssembled;
    result = { html, md, frontmatter: fm, sections: [] };
  } else {
    // Single-file mode — existing pipeline.
    const source = readFileSync(inputPath, 'utf8');
    slug = basename(inputPath, extname(inputPath));

    // If this is a talk page declaring `talk_conventions: <name>` in
    // its frontmatter, load the corresponding conventions doc and pass
    // it through to renderShell as a collapsed banner.
    const initialFrontmatter = matter(source).data;
    const talkConventions = loadTalkConventions(initialFrontmatter, inputPath);
    const baseShellOptions = talkConventions
      ? {
          talkConventionsHtml: talkConventions.html,
          talkConventionsLabel: talkConventions.label,
        }
      : {};

    try {
      result = renderPage(source, { slug, shellOptions: baseShellOptions });
    } catch (err) {
      if (err instanceof FrontmatterError) {
        console.error(`Frontmatter error in ${inputPath}: ${err.message}`);
        process.exit(1);
      }
      throw err;
    }
  }

  // Talk folders don't have audience-variant rendering — each
  // audience tier is its own folder, rendered as its own invocation.
  const audienceVariants = isTalkFolder ? null : resolveAudienceVariants(inputPath, result.frontmatter);
  let renderedStreams = null;
  if (audienceVariants) {
    renderedStreams = {};
    const shellOptions = { audienceStreams: audienceVariants };

    for (const key of AUDIENCE_ORDER) {
      const stream = audienceVariants.streams[key];
      if (!stream) continue;
      if (!existsSync(stream.sourcePath)) {
        throw new Error(
          `Audience stream "${key}" is enabled for ${inputPath} but ${stream.sourcePath} does not exist`
        );
      }
      const variantRaw = readFileSync(stream.sourcePath, 'utf8');
      const variantSource = stream.sourcePath === inputPath
        ? variantRaw
        : buildAudienceSource(result.frontmatter, variantRaw);
      renderedStreams[key] = renderPage(variantSource, {
        slug,
        shellOptions: {
          ...shellOptions,
          activeAudienceStream: key,
        },
      });
    }

    if (renderedStreams.public) {
      result = renderedStreams.public;
    }
  }

  mkdirSync(outDir, { recursive: true });
  const parentSlug = result.frontmatter.slug;
  const htmlPath = resolve(outDir, `${parentSlug}.html`);
  const mdPath = resolve(outDir, `${parentSlug}.md`);
  writeFileSync(htmlPath, cleanGeneratedText(result.html));
  writeFileSync(mdPath, cleanGeneratedText(result.md));

  let summary = `✓ ${slug}: ${result.html.length} bytes → ${htmlPath}`;

  // Section sub-pages — one HTML + one MD per declared section,
  // plus a talk-page stub when the section opts into talk: true.
  if (result.sections && result.sections.length) {
    const subDir = resolve(outDir, parentSlug);
    mkdirSync(subDir, { recursive: true });
    for (const sec of result.sections) {
      const subHtml = resolve(subDir, `${sec.slug}.html`);
      const subMd = resolve(subDir, `${sec.slug}.md`);
      writeFileSync(subHtml, cleanGeneratedText(sec.html));
      writeFileSync(subMd, cleanGeneratedText(sec.md));
      if (sec.talkMd) {
        const talkMd = resolve(subDir, `${sec.slug}.talk.md`);
        writeFileSync(talkMd, cleanGeneratedText(sec.talkMd));
      }
    }
    summary += `\n  + ${result.sections.length} section sub-page(s) → ${subDir}/`;
  }

  if (renderedStreams) {
    for (const key of AUDIENCE_ORDER) {
      if (key === 'public') continue;
      const stream = renderedStreams[key];
      if (!stream) continue;
      writeFileSync(resolve(outDir, `${parentSlug}.${key}.html`), cleanGeneratedText(stream.html));
      writeFileSync(resolve(outDir, `${parentSlug}.${key}.md`), cleanGeneratedText(stream.md));
    }
    summary += `\n  + ${Object.keys(renderedStreams).length} audience stream render(s)`;
  }

  // Time-based URL versioning: maintain latest_for_family.json so a
  // separate Caddy/redirect step knows that e.g. `for-you` →
  // `for-you-2026-04-26`. Also maintains <family>-index.json with the
  // full list of dated members for the in-page version-dropdown +
  // infinite-scroll chrome to read at runtime.
  const updated = updateLatestForFamily(outDir, parentSlug, result.frontmatter);
  if (updated) {
    summary += `\n  family/latest: ${updated.family} → ${parentSlug} (${updated.date})`;
    summary += `\n  family/index:  ${updated.family}-index.json (member registered)`;
  }

  console.log(summary);
}

main();
