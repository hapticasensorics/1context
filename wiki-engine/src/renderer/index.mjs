// Wiki engine renderer — entry point.
//
// Takes a markdown source string + slug and returns:
//   { html, md, frontmatter, sections }
//
//   html       — themed HTML page (full document, ready to write to dist/)
//   md         — clean markdown twin (frontmatter + body, normalized)
//   frontmatter — parsed frontmatter object
//   sections   — array of section sub-page renders (may be empty);
//                each entry is { slug, html, md, talkMd?, frontmatter }
//                and represents a self-contained agent-friendly view
//                of a single H2 section. See sections.mjs for the
//                opt-in marker syntax.
//
// Pipeline:
//   1. parse frontmatter (validates required fields, allowed enums)
//   2. extract section markers from the body
//   3. configure marked with custom renderer for stable heading
//      anchors matching the 1Context convention
//   4. render the (clean) markdown body to HTML for the parent page
//   5. extract TOC nav from the rendered HTML
//   6. assemble the parent page shell via the template
//   7. for each declared section, render a sub-page (and optional
//      talk-page stub) using the section's body slice
//   8. produce the .md twin (frontmatter preserved, body normalized)
//
// Pages without section markers render exactly as before — sections
// is an empty array. Existing concept pages, biography, life-story
// etc. produce identical output (modulo new chrome in template.mjs).

import { Marked } from 'marked';
import { parseFrontmatter, FrontmatterError } from './frontmatter.mjs';
import { buildToc, slugifyHeading } from './toc.mjs';
import { renderShell } from './template.mjs';
import { directives } from './directives/index.mjs';
import { stableCodeBlockId, slugToken } from './references.mjs';
import {
  extractSections,
  deriveSectionFrontmatter,
  stringifyFrontmatter,
  buildTalkStub,
} from './sections.mjs';

export { FrontmatterError } from './frontmatter.mjs';

function codeLanguageClass(language) {
  const firstToken = String(language || '').trim().split(/\s+/)[0] || '';
  return firstToken.replace(/[^A-Za-z0-9_-]/g, '');
}

function makeMarked({ codeContextSlug = 'page' } = {}) {
  const m = new Marked();
  let codeSequence = 0;
  // Override the default heading renderer so H2/H3 get stable
  // 1Context-convention slugs as their id attributes. Text is
  // preserved verbatim (marked already escapes inline HTML).
  m.use({
    renderer: {
      code({ text, lang }) {
        codeSequence += 1;
        const language = codeLanguageClass(lang);
        const id = stableCodeBlockId(codeContextSlug, codeSequence, language, text);
        const languageAttr = language ? ` data-1context-language="${escapeHtml(language)}"` : '';
        const codeClass = language ? ` class="language-${escapeHtml(language)}"` : '';
        return `<pre id="${id}" class="opctx-code-block" data-1context-code-id="${id}"${languageAttr}><code${codeClass}>${escapeHtml(text)}</code></pre>\n`;
      },
      html({ text, raw }) {
        return escapeHtml(text || raw || '');
      },
      heading({ tokens, depth }) {
        const text = this.parser.parseInline(tokens);
        if (depth === 2 || depth === 3) {
          // Use raw text for the slug, not the rendered HTML.
          const raw = tokens.map((t) => t.raw || t.text || '').join('');
          const id = slugifyHeading(raw);
          return `<h${depth} id="${id}">${text}</h${depth}>\n`;
        }
        return `<h${depth}>${text}</h${depth}>\n`;
      },
    },
  });
  // Custom pandoc-style fenced-div directives:
  //   :::infobox / :::main-article slug / :::see-also / :::audience name
  // Each directive lives in its own file under ./directives/ for
  // discoverability; the index module collects them so the renderer
  // doesn't have to know the names.
  m.use({ extensions: directives });
  return m;
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function normalizeFootnoteLabel(value) {
  return String(value || '').trim().replace(/\s+/g, ' ').toLowerCase();
}

function extractFootnoteDefinitions(markdown) {
  const lines = String(markdown || '').split(/\r?\n/);
  const definitions = new Map();
  const bodyLines = [];
  let inCode = null;

  for (const line of lines) {
    if (inCode) {
      const closeMatch = /^ {0,3}(`{3,}|~{3,})\s*$/.exec(line);
      if (closeMatch && closeMatch[1][0] === inCode.fenceChar && closeMatch[1].length >= inCode.fenceLength) {
        inCode = null;
      }
      bodyLines.push(line);
      continue;
    }

    const openMatch = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (openMatch) {
      inCode = {
        fenceChar: openMatch[1][0],
        fenceLength: openMatch[1].length,
      };
      bodyLines.push(line);
      continue;
    }

    const definitionMatch = /^ {0,3}\[\^([^\]\n]+)\]:[ \t]*(.*)$/.exec(line);
    if (definitionMatch) {
      const label = normalizeFootnoteLabel(definitionMatch[1]);
      if (label && !definitions.has(label)) {
        definitions.set(label, definitionMatch[2] || '');
      }
      continue;
    }

    bodyLines.push(line);
  }

  return {
    body: bodyLines.join('\n'),
    definitions,
  };
}

function prepareFootnoteBody(markdown, pageSlug) {
  const { body, definitions } = extractFootnoteDefinitions(markdown);
  if (!definitions.size) return { body, footnotes: [] };

  const lines = body.split(/\r?\n/);
  const footnotes = [];
  const labelToFootnote = new Map();
  const slug = slugToken(pageSlug);
  let inCode = null;

  const renderedLines = lines.map((line) => {
    if (inCode) {
      const closeMatch = /^ {0,3}(`{3,}|~{3,})\s*$/.exec(line);
      if (closeMatch && closeMatch[1][0] === inCode.fenceChar && closeMatch[1].length >= inCode.fenceLength) {
        inCode = null;
      }
      return line;
    }

    const openMatch = /^ {0,3}(`{3,}|~{3,})(.*)$/.exec(line);
    if (openMatch) {
      inCode = {
        fenceChar: openMatch[1][0],
        fenceLength: openMatch[1].length,
      };
      return line;
    }

    return line.replace(/\[\^([^\]\n]+)\]/g, (match, rawLabel) => {
      const label = normalizeFootnoteLabel(rawLabel);
      if (!definitions.has(label)) return match;
      let footnote = labelToFootnote.get(label);
      if (!footnote) {
        const number = footnotes.length + 1;
        footnote = {
          label,
          number,
          noteId: `cite-note-${slug}-${number}`,
          refs: [],
          markdown: definitions.get(label),
        };
        labelToFootnote.set(label, footnote);
        footnotes.push(footnote);
      }
      const refOrdinal = footnote.refs.length + 1;
      const ref = {
        id: `cite-ref-${slug}-${footnote.number}-${refOrdinal}`,
        token: `OPCTX_FOOTNOTE_REF_${slug}_${footnote.number}_${refOrdinal}_TOKEN`,
      };
      footnote.refs.push(ref);
      return ref.token;
    });
  });

  return {
    body: renderedLines.join('\n'),
    footnotes,
  };
}

function renderMarkdownWithFootnotes(marked, markdown, { pageSlug = 'page' } = {}) {
  const { body, footnotes } = prepareFootnoteBody(markdown, pageSlug);
  let html = marked.parse(body);
  if (!footnotes.length) return html;

  for (const footnote of footnotes) {
    for (const ref of footnote.refs) {
      const refHtml = `<sup id="${ref.id}" class="opctx-footnote-ref" data-1context-citation-ref="${footnote.noteId}"><a href="#${footnote.noteId}" aria-label="Reference ${footnote.number}">${footnote.number}</a></sup>`;
      html = html.split(ref.token).join(refHtml);
    }
  }

  const items = footnotes.map((footnote) => {
    const noteHtml = marked.parseInline(footnote.markdown || '');
    const firstRef = footnote.refs[0]?.id || '';
    return `<li id="${footnote.noteId}" class="opctx-reference" data-1context-citation-id="${footnote.noteId}"><span class="opctx-reference-body">${noteHtml}</span> <a class="opctx-reference-back" href="#${firstRef}" aria-label="Back to reference ${footnote.number}">back</a></li>`;
  }).join('\n');

  return `${html}\n<section class="opctx-references" id="opctx-references" data-1context-references="footnotes">\n<h2 id="references">References</h2>\n<ol>\n${items}\n</ol>\n</section>\n`;
}

/**
 * Render a single page from its markdown source.
 *
 * @param {string} source  - the raw .md file contents (including frontmatter)
 * @param {object} [opts]
 * @param {string} [opts.slug]  - the page's slug (used in error messages
 *                                if frontmatter validation fails before
 *                                the slug field can be read)
 * @param {object} [opts.shellOptions] - extra page-shell options for the
 *                                       parent page only (used by
 *                                       render-to-dir.mjs for opt-in
 *                                       chrome like audience streams)
 * @returns {{ html: string, md: string, frontmatter: object, sections: object[] }}
 */
export function renderPage(source, { slug, shellOptions = {} } = {}) {
  const { data: frontmatter, content: body } = parseFrontmatter(source, { slug });
  const { sections: sectionDefs, cleanBody } = extractSections(body, frontmatter);

  const pageSlug = frontmatter.slug || slug || 'page';
  const marked = makeMarked({ codeContextSlug: pageSlug });
  // Strip the leading H1 from the parent body before rendering. The
  // shell prints `frontmatter.title` as the page <h1>; many .md
  // sources also begin with a `# Title` line by Markdown convention,
  // which would produce a duplicate <h1>. This mirrors the
  // stripLeadingH2 pass we already do for section sub-pages (where
  // the section's H2 becomes the sub-page's H1, and the source H2
  // would otherwise be duplicated).
  const bodyHtml = stripLeadingH1(renderMarkdownWithFootnotes(marked, cleanBody, { pageSlug }));
  const tocHtml = buildToc(bodyHtml);
  const html = renderShell({ frontmatter, bodyHtml, tocHtml, ...shellOptions });

  // Markdown twin: preserve the source verbatim. (cleanBody is only
  // for the HTML render; the .md twin keeps marker comments so the
  // file remains the single source of truth — re-rendering it gives
  // the same sub-pages.)
  const md = source;

  // Render each declared section as its own sub-page. The section's
  // body is just the slice from its H2 to the next H2; we wrap it
  // with the section's own derived frontmatter and render through
  // the same shell so the sub-page gets the same chrome.
  const sections = sectionDefs.map((sec) => {
    const subFm = deriveSectionFrontmatter(frontmatter, sec, frontmatter.slug);
    const sectionSlug = subFm.slug || sec.slug || 'section';
    const subMarked = makeMarked({ codeContextSlug: sectionSlug });
    const subBodyHtml = renderMarkdownWithFootnotes(subMarked, sec.body, { pageSlug: sectionSlug });
    // Strip the leading H2 from the body HTML — the shell renders
    // the section title as the page H1. Otherwise the sub-page would
    // show "Today · 2026-04-26 · Sunday" twice.
    const strippedBodyHtml = stripLeadingH2(subBodyHtml);
    const subTocHtml = buildToc(strippedBodyHtml);
    const subHtml = renderShell({
      frontmatter: subFm,
      bodyHtml: strippedBodyHtml,
      tocHtml: subTocHtml,
      siteNavigation: shellOptions.siteNavigation || null,
    });
    const subMdBody = stripLeadingH2Md(sec.body);
    const subMd = stringifyFrontmatter(subFm) + '\n' + subMdBody;
    const out = {
      slug: sec.slug,
      html: subHtml,
      md: subMd,
      frontmatter: subFm,
      anchor: sec.anchor,
      title: sec.title,
      date: sec.date,
    };
    if (sec.talk) {
      out.talkMd = buildTalkStub(frontmatter, sec, frontmatter.slug);
    }
    return out;
  });

  return { html, md, frontmatter, sections };
}

// Drop the first H2 from a rendered HTML block. The shell already
// prints the page title as <h1>, so leaving the source H2 produces
// duplicated titles.
function stripLeadingH2(html) {
  return html.replace(/^[ \t]*<h2\s+[^>]*>[\s\S]*?<\/h2>\s*/, '');
}

// Drop the first H1 from a rendered HTML block. The shell prints
// `frontmatter.title` as the page <h1>; most authored .md files
// begin with a `# Title` line by Markdown convention which would
// otherwise produce a duplicate <h1> in the body. We tolerate
// leading whitespace and an optional id attribute (marked emits
// h1s without an id, but be defensive).
function stripLeadingH1(html) {
  return html.replace(/^\s*<h1(?:\s+[^>]*)?>[\s\S]*?<\/h1>\s*/, '');
}

// Same idea for the markdown twin: drop the first `## ...` line.
function stripLeadingH2Md(md) {
  const lines = md.split('\n');
  let i = 0;
  // Skip blank lines.
  while (i < lines.length && lines[i].trim() === '') i++;
  if (i < lines.length && /^##[ \t]+/.test(lines[i])) {
    lines.splice(i, 1);
    // Also drop one trailing blank if present.
    if (i < lines.length && lines[i].trim() === '') lines.splice(i, 1);
  }
  return lines.join('\n');
}
