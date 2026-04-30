// Frontmatter parser + validator for wiki-engine.
//
// Wraps `gray-matter` (the standard YAML-frontmatter parser for the
// markdown ecosystem) and adds 1Context-specific validation: required
// fields, allowed enum values, slug-format rules, type-checking on
// known optional fields. Unknown fields pass through silently
// (forward-compat) — the schema can grow without breaking pages
// authored against an older engine.
//
// The validator is the source of truth for the "Engine spec ›
// Frontmatter schema" subsection of /wiki-engine. If you add a
// field here, document it there.
//
// Throws FrontmatterError on validation failure so the caller
// (renderer, build pipeline) can fail loudly with a clear message.

import matter from 'gray-matter';

// Required fields. Every authored page must have these or the engine
// can't decide how to render / classify / link to it.
const REQUIRED_FIELDS = ['title', 'slug', 'section', 'access'];

// Allowed values per enum field. Strict allowlists — fail-closed on
// typos. `audience` ships as P5; declared here so frontmatter can
// already carry it.
const ENUMS = {
  access:           ['public', 'shared', 'private'],
  source_type:      ['authored', 'imported'],
  section:          ['project', 'reference', 'ops', 'api', 'product'],
  status:           ['draft', 'published', 'archived', 'superseded'],
  audience:         ['internal', 'public', 'both'],
  theme_default:    ['light', 'dark', 'auto'],
  article_width:    ['s', 'm', 'l'],
  font_size:        ['s', 'm', 'l'],
  border_radius:    ['rounded', 'square'],
  links_style:      ['underline', 'color'],
  cover_image:      ['show', 'hide'],
  article_style:    ['full', 'pics', 'text'],
};

// Boolean fields — page-level toggles. The engine reads these to
// suppress chrome features (talk button, footer, agent view, etc.).
const BOOLEAN_FIELDS = [
  'toc_enabled',
  'talk_enabled',
  'agent_view_enabled',
  'copy_buttons_enabled',
  'footer_enabled',
  'search_indexed',
  'noindex',
  'requires_auth',
];

// List-of-string fields — frontmatter arrays of plain strings.
const STRING_LIST_FIELDS = ['tags', 'keywords', 'shared_with', 'related'];

// List-of-object fields — frontmatter arrays of objects with their
// own internal schema. Validation here is shallow (must be array of
// objects with at least the required keys); deeper validation is
// the consumer's responsibility.
const OBJECT_LIST_FIELDS = {
  // sections: opt-in per-section sub-page rendering (see sections.mjs).
  // Each entry: { slug: <string>, anchor: <string>, talk?: <bool>, date?: <string> }
  // This is the frontmatter alternative to inline `<!-- section: ... -->` markers.
  sections: ['slug', 'anchor'],
};

// Slugs allow lowercase alphanumeric + hyphens, and dots in the
// middle for sibling-suffix conventions like `<base>.talk`,
// `<base>.internal`, `<base>.private`, and combinations such as
// `<base>.internal.talk`. Dots are not permitted at the start or
// end of the slug. Length cap stays at 60 characters.
const SLUG_RE = /^[a-z0-9](?:[a-z0-9.-]{0,58}[a-z0-9])?$/;

// ISO-8601 date or datetime. Permissive (allows YYYY-MM-DD too).
const DATE_RE = /^\d{4}-\d{2}-\d{2}(T\d{2}:\d{2}(:\d{2})?(\.\d+)?(Z|[+-]\d{2}:?\d{2})?)?$/;

export class FrontmatterError extends Error {
  constructor(slug, message) {
    super(`[${slug || 'unknown'}] frontmatter: ${message}`);
    this.name = 'FrontmatterError';
    this.slug = slug;
  }
}

export function parseFrontmatter(source, { slug } = {}) {
  const { data, content } = matter(source);

  // 1. Required fields present and non-empty.
  for (const field of REQUIRED_FIELDS) {
    if (data[field] === undefined || data[field] === null || data[field] === '') {
      throw new FrontmatterError(slug, `missing required field "${field}"`);
    }
  }

  // 2. Enum values in their allowlists.
  for (const [field, allowed] of Object.entries(ENUMS)) {
    const value = data[field];
    if (value === undefined) continue;
    if (!allowed.includes(value)) {
      throw new FrontmatterError(
        slug,
        `field "${field}" has value "${value}"; allowed: ${allowed.join(', ')}`
      );
    }
  }

  // 3. Boolean fields actually boolean.
  for (const field of BOOLEAN_FIELDS) {
    if (data[field] === undefined) continue;
    if (typeof data[field] !== 'boolean') {
      throw new FrontmatterError(
        slug,
        `field "${field}" must be a boolean (true/false); got ${JSON.stringify(data[field])}`
      );
    }
  }

  // 4. String-list fields are arrays of strings.
  for (const field of STRING_LIST_FIELDS) {
    if (data[field] === undefined) continue;
    if (!Array.isArray(data[field]) || !data[field].every(s => typeof s === 'string')) {
      throw new FrontmatterError(
        slug,
        `field "${field}" must be a list of strings; got ${JSON.stringify(data[field])}`
      );
    }
  }

  // 4b. Object-list fields are arrays of objects with required keys.
  for (const [field, requiredKeys] of Object.entries(OBJECT_LIST_FIELDS)) {
    if (data[field] === undefined) continue;
    if (!Array.isArray(data[field])) {
      throw new FrontmatterError(
        slug,
        `field "${field}" must be a list of objects; got ${JSON.stringify(data[field])}`
      );
    }
    for (const entry of data[field]) {
      if (entry === null || typeof entry !== 'object' || Array.isArray(entry)) {
        throw new FrontmatterError(
          slug,
          `field "${field}" entries must be objects; got ${JSON.stringify(entry)}`
        );
      }
      for (const key of requiredKeys) {
        if (typeof entry[key] !== 'string' || entry[key] === '') {
          throw new FrontmatterError(
            slug,
            `field "${field}" entry missing required string key "${key}": ${JSON.stringify(entry)}`
          );
        }
      }
    }
  }

  // 5. Slug format.
  if (typeof data.slug === 'string' && !SLUG_RE.test(data.slug)) {
    throw new FrontmatterError(
      slug,
      `slug "${data.slug}" must match ${SLUG_RE} (lowercase, hyphens, ≤60 chars, alphanumeric start)`
    );
  }

  // 6. Date fields. last_updated must parse; expires_at if present too.
  for (const field of ['last_updated', 'expires_at']) {
    const v = data[field];
    if (v === undefined) continue;
    // gray-matter may parse YYYY-MM-DD into a Date object; accept both.
    if (v instanceof Date) continue;
    if (typeof v !== 'string' || !DATE_RE.test(v)) {
      throw new FrontmatterError(
        slug,
        `field "${field}" must be an ISO date (YYYY-MM-DD or YYYY-MM-DDThh:mm:ssZ); got ${JSON.stringify(v)}`
      );
    }
  }

  // 7. Cross-field constraint: if status: superseded, superseded_by required.
  if (data.status === 'superseded' && (!data.superseded_by || typeof data.superseded_by !== 'string')) {
    throw new FrontmatterError(
      slug,
      `status: superseded requires superseded_by: <slug>`
    );
  }

  // 8. Cross-field constraint: if access: shared, shared_with should be present.
  if (data.access === 'shared' && (!Array.isArray(data.shared_with) || data.shared_with.length === 0)) {
    throw new FrontmatterError(
      slug,
      `access: shared requires shared_with: [identity, ...] (got empty or missing)`
    );
  }

  // Unknown fields pass through silently — forward-compat for fields
  // the engine doesn't yet read but plug-ins or future versions might.
  return { data, content };
}
