// HTML page-shell assembler. Wraps the rendered article body in the
// canonical 1Context chrome — header, TOC nav, main, article — with
// metadata (title, OG tags, link rel=alternate) populated from the
// page's frontmatter.
//
// This is intentionally simple: no template engine, just a tagged
// template literal. The shape mirrors hand-authored pages like
// `agent-ux.html`.

const ESCAPE_MAP = { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' };
const escape = (s) => String(s).replace(/[&<>"']/g, (c) => ESCAPE_MAP[c]);
const AUDIENCE_ORDER = ['private', 'internal', 'public'];

// Family detection — same logic as render-to-dir.mjs. Kept duplicated
// here intentionally: this is a render-time concern (the shell has to
// know whether to emit the version-dropdown chrome), separate from the
// post-render bookkeeping render-to-dir.mjs does for the index file.
// If the two ever diverge they need to drift toward the strictest
// interpretation, but for now they're identical.
const DATED_SLUG_RE = /^(.+)-(\d{4})-(\d{2})-(\d{2})$/;
function isValidYmdShell(y, m, d) {
  const yi = Number(y), mi = Number(m), di = Number(d);
  if (mi < 1 || mi > 12 || di < 1 || di > 31) return false;
  const dt = new Date(`${y}-${m}-${d}T00:00:00Z`);
  if (isNaN(dt.getTime())) return false;
  return dt.getUTCFullYear() === yi
    && dt.getUTCMonth() + 1 === mi
    && dt.getUTCDate() === di;
}
function detectFamilyForShell(slug, frontmatter) {
  if (frontmatter && Object.prototype.hasOwnProperty.call(frontmatter, 'family')) {
    if (frontmatter.family === false) return null;
    if (typeof frontmatter.family === 'string' && frontmatter.family) {
      const dateFromFm = typeof frontmatter.family_date === 'string'
        ? frontmatter.family_date
        : null;
      const m = DATED_SLUG_RE.exec(slug);
      const dateFromSlug = m && isValidYmdShell(m[2], m[3], m[4])
        ? `${m[2]}-${m[3]}-${m[4]}`
        : null;
      const date = dateFromFm || dateFromSlug;
      if (!date) return null;
      return { family: frontmatter.family, date };
    }
  }
  const m = DATED_SLUG_RE.exec(slug);
  if (!m) return null;
  const [, family, y, mo, d] = m;
  if (!isValidYmdShell(y, mo, d)) return null;
  return { family, date: `${y}-${mo}-${d}` };
}

// Title dropdown menu. Static list of per-user surfaces for the
// owner of the demo (paul-demo2). Anonymous viewers see the same
// list — once auth wires up, the menu will be derived from the
// page's `owner` frontmatter and the viewer's identity.
//
// As of round 3 (mobile-cleanup), the entire `1Context ▼` element
// is a single button trigger — clicking the wordmark or the chevron
// both open the menu. Home navigation lives as the first item inside
// the menu (Group: "Project") rather than as a separate link on the
// wordmark itself. Rationale: the previous split-target (link + chevron)
// made the menu feel like an afterthought next to the home link, and
// the non-uniform tap target was easy to miss on mobile. With a
// single trigger the menu becomes the primary affordance and home
// becomes "menu → first item" — one extra tap, but consistent.
//
// Each group has a label + list of links. Order is deliberate:
// rolling/personal feeds first (For You, Weekly), then long-form
// memory (Biography, Life story, Concept pages — all owner-scoped),
// then universal project surfaces.
//
// Personalization seam: when auth/owner wiring arrives, swap this
// for a derivation `(viewer, page.owner) → menu`. The "For You"
// label becomes "For <Viewer Name>" or "Your stuff" and the items
// within re-point to the viewer's own surfaces. The "Project" group
// is universal across viewers and stays put. Concept pages belong in
// the personal group because they are a per-owner subgraph, not a
// universal project surface.
function renderBrandMenu(opts) {
  // Owner segment is empty by default → root-relative URLs (`/for-you`, `/your-context`).
  // Pass an explicit owner to scope into a subfolder deploy.
  const owner = opts.owner === undefined ? '' : opts.owner;
  const homeHref = opts.homeHref || (owner ? `/${owner}/` : '/');
  const ownerPrefix = owner ? `/${owner}` : '';
  const groups = [
    {
      label: 'For You',
      items: [
        { href: `${ownerPrefix}/for-you`, label: 'For You', sub: 'Rolling 14-day feed + Monday biography rewrite' },
        { href: `${ownerPrefix}/your-context`, label: 'Your Context', sub: 'Working style, preferences, taste, life story' },
      ],
    },
    {
      label: 'Project',
      items: [
        { href: `${ownerPrefix}/projects`, label: 'Projects', sub: 'Active, paused, completed, archived' },
      ],
    },
    {
      label: 'Topics',
      items: [
        { href: `${ownerPrefix}/topics`, label: 'Topics', sub: 'Named subjects, categorized' },
      ],
    },
  ];

  const groupHtml = groups.map((g) => {
    const items = g.items.map((it) => {
      const sub = it.sub ? `<span class="opctx-brand-menu-sub">${escape(it.sub)}</span>` : '';
      return `<li><a href="${escape(it.href)}" role="menuitem"><span class="opctx-brand-menu-label">${escape(it.label)}</span>${sub}</a></li>`;
    }).join('\n            ');
    return `<div class="opctx-brand-menu-group" role="group" aria-label="${escape(g.label)}">
          <div class="opctx-brand-menu-heading">${escape(g.label)}</div>
          <ul class="opctx-brand-menu-list">
            ${items}
          </ul>
        </div>`;
  }).join('\n        ');

  // Single-button brand element. Round 7: chevron removed — none of
  // the chrome dropdowns (brand, Era, Audience) carry a visible
  // chevron now, since pill/wordmark + hover/focus + aria-haspopup
  // are sufficient discoverability and the bare wordmark reads
  // calmer in the header. The entire `1Context` button is one tap
  // target. The wordmark span keeps `.opctx-header-logo` so the
  // existing typography + ::first-letter accent color still apply.
  return `<div class="opctx-header-brand">
      <button type="button"
              class="opctx-brand-menu-toggle"
              aria-haspopup="menu"
              aria-expanded="false"
              aria-controls="opctx-brand-menu"
              aria-label="Open 1Context navigation menu"
              data-home-href="${escape(homeHref)}">
        <span class="opctx-header-logo">1Context</span>
      </button>
      <div id="opctx-brand-menu" class="opctx-brand-menu" role="menu" hidden>
        ${groupHtml}
      </div>
    </div>`;
}

// Version picker — version selector for the current page-family.
// Round 6 promotes it to the same pill+menu pattern the brand chrome
// uses, so the TOC control feels first-class rather than like a raw
// form widget. Hydration still fetches `<family>-index.json` at
// runtime; the server renders a one-item skeleton containing the
// current snapshot so the pill is truthful before the menu hydrates.
//
// Returns an empty string for pages that don't belong to a family
// (concept pages, the static project pages 1context / agent-ux, etc.).
//
// As of round 5, the TOC copy is intentionally sparse: the page title
// already tells the reader what surface they are on, so the picker
// itself only shows ISO dates (`2026-04-26`, `2026-04-25`, …) with a
// `(today)` suffix on the newest member. The surrounding TOC row has
// no extra "Snapshot" label or long-date subtitle.
function renderVersionMenu({ family, date, slug }) {
  const currentUrl = `${slug}.html`;

  return `<div class="opctx-header-version"
         data-family="${escape(family)}"
         data-current-slug="${escape(slug)}"
         data-current-date="${escape(date)}">
      <button type="button"
              id="opctx-version-toggle"
              class="opctx-pill-menu-toggle opctx-version-toggle"
              aria-haspopup="menu"
              aria-expanded="false"
              aria-controls="opctx-version-menu"
              aria-label="Snapshot version"
              data-current-url="${escape(currentUrl)}">
        <span class="opctx-pill-menu-toggle-label opctx-version-toggle-label">${escape(date)}</span>
      </button>
      <div id="opctx-version-menu"
           class="opctx-pill-menu opctx-version-menu"
           role="menu"
           aria-label="Snapshot versions"
           hidden>
        <button type="button"
                class="opctx-pill-menu-item opctx-version-menu-item"
                role="menuitemradio"
                aria-checked="true"
                data-version-url="${escape(currentUrl)}"
                data-version-slug="${escape(slug)}"
                data-version-date="${escape(date)}">
          ${escape(date)}
        </button>
      </div>
    </div>`;
}

function titleCaseAudience(audience) {
  return audience.charAt(0).toUpperCase() + audience.slice(1);
}

function renderTierBadge({ access, tierTitle, tierLabel }) {
  return `<span class="opctx-tier-badge" data-tier="${escape(access)}" title="${escape(tierTitle)}">${escape(tierLabel)}</span>`;
}

function renderAudienceSwitcher({ audienceStreams, activeAudienceStream = 'public' }) {
  if (!audienceStreams || !audienceStreams.streams) return '';

  const items = (audienceStreams.order || AUDIENCE_ORDER)
    .filter((key) => audienceStreams.streams[key])
    .map((key) => {
      const checked = key === activeAudienceStream;
      return `<button type="button"
              class="opctx-pill-menu-item opctx-audience-menu-item"
              role="menuitemradio"
              aria-checked="${checked ? 'true' : 'false'}"
              data-audience-target="${escape(key)}">
        ${escape(titleCaseAudience(key))}
      </button>`;
    })
    .join('');

  // Share is disabled when the current audience is Private. Private
  // pages can't be shared by definition — the link wouldn't expose
  // anything to the recipient that they don't already have, and the
  // intent of "Private" is "no one else." Initial render carries the
  // right state; JS keeps it in sync when the user swaps audiences.
  const shareDisabled = activeAudienceStream === 'private';
  const shareAttrs = shareDisabled
    ? 'disabled aria-disabled="true" data-audience-share-disabled-reason="Private pages can\'t be shared"'
    : '';

  return `<div class="opctx-audience-switcher"
          aria-label="Audience stream"
          data-active-audience="${escape(activeAudienceStream)}">
      <button type="button"
              class="opctx-pill-menu-toggle opctx-audience-switcher-toggle"
              aria-haspopup="menu"
              aria-expanded="false"
              aria-controls="opctx-audience-menu"
              aria-label="Audience stream">
        <span class="opctx-pill-menu-toggle-label opctx-audience-switcher-current">${escape(titleCaseAudience(activeAudienceStream))}</span>
      </button>
      <div id="opctx-audience-menu"
           class="opctx-pill-menu opctx-audience-menu"
           role="menu"
           aria-label="Audience stream"
           hidden>
        ${items}
        <div class="opctx-pill-menu-divider" role="separator"></div>
        <button type="button"
                class="opctx-pill-menu-item opctx-audience-menu-action"
                role="menuitem"
                data-audience-action="share"
                ${shareAttrs}>
          Share
        </button>
      </div>
    </div>`;
}

function renderShareModal({ title, activeAudienceStream = 'public' }) {
  // Title row makes the audience version explicit. Sharing the
  // Internal version sends a different URL than sharing the Public
  // version — the modal must say which one. JS updates the badge
  // when the user opens Share after swapping audience streams.
  const initialAudienceLabel = titleCaseAudience(activeAudienceStream);
  return `<div class="opctx-modal-scrim opctx-share-modal-scrim" aria-hidden="true">
      <div class="opctx-modal opctx-share-modal"
           role="dialog"
           aria-modal="true"
           aria-labelledby="opctx-share-title"
           aria-describedby="opctx-share-general-description"
           tabindex="-1">
        <header class="opctx-share-modal-head">
          <div class="opctx-share-modal-title-row">
            <h2 id="opctx-share-title" class="opctx-share-modal-title">Share <span class="opctx-share-modal-title-name">${escape(title)}</span></h2>
            <button type="button" class="opctx-share-modal-close" aria-label="Close share dialog">
              <svg width="18" height="18" viewBox="0 0 24 24" aria-hidden="true" focusable="false"><path d="m18 6-12 12M6 6l12 12" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"/></svg>
            </button>
          </div>
          <p class="opctx-share-modal-stream"
             data-share-stream-row>
            Sharing the
            <span class="opctx-share-stream-badge"
                  data-share-stream-badge
                  data-stream="${escape(activeAudienceStream)}">${escape(initialAudienceLabel)}</span>
            version of this page.
          </p>
        </header>
        <div class="opctx-share-modal-body">
          <section class="opctx-share-section" aria-labelledby="opctx-share-people-label">
            <div id="opctx-share-people-label" class="opctx-share-section-label">People with access</div>
            <label class="opctx-share-invite">
              <span class="opctx-share-invite-label">Invite</span>
              <input id="opctx-share-invite-input"
                     class="opctx-share-invite-input"
                     type="text"
                     placeholder="Add people, groups, or emails"
                     aria-label="Add people, groups, or emails"
                     autocomplete="off"
                     spellcheck="false">
            </label>
            <div class="opctx-share-access-list" role="list" aria-label="People with access">
              <div class="opctx-share-access-row" role="listitem">
                <span class="opctx-share-access-avatar" aria-hidden="true">P</span>
                <div class="opctx-share-access-meta">
                  <span class="opctx-share-access-primary">paul@haptica.ai</span>
                  <span class="opctx-share-access-secondary">Page owner</span>
                </div>
                <span class="opctx-share-access-role">Owner</span>
              </div>
              <div class="opctx-share-access-row opctx-share-access-row--hint" role="listitem">
                Add people, groups, or emails
              </div>
            </div>
          </section>
          <section class="opctx-share-section" aria-labelledby="opctx-share-general-access-label">
            <div id="opctx-share-general-access-label" class="opctx-share-section-label">General access</div>
            <div class="opctx-share-general-access">
              <div class="opctx-share-general-copy">
                <div class="opctx-share-general-title" data-share-general-label>Restricted</div>
                <p id="opctx-share-general-description"
                   class="opctx-share-general-description"
                   data-share-general-description>Only people with access can open this link.</p>
              </div>
              <div class="opctx-share-general-controls">
                <div class="opctx-share-control opctx-share-control--access">
                  <button type="button"
                          class="opctx-share-control-toggle opctx-share-access-toggle"
                          aria-haspopup="menu"
                          aria-expanded="false"
                          aria-controls="opctx-share-access-menu"
                          aria-label="General access">
                    <span data-share-access-current>Restricted</span>
                    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" focusable="false" class="opctx-share-control-chevron"><path d="M2 4l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
                  </button>
                  <div id="opctx-share-access-menu"
                       class="opctx-pill-menu opctx-share-access-menu"
                       role="menu"
                       aria-label="General access"
                       hidden>
                    <button type="button"
                            class="opctx-pill-menu-item"
                            role="menuitemradio"
                            aria-checked="true"
                            data-share-access="restricted"
                            data-share-access-label="Restricted"
                            data-share-access-description="Only people with access can open this link.">
                      Restricted
                    </button>
                    <button type="button"
                            class="opctx-pill-menu-item"
                            role="menuitemradio"
                            aria-checked="false"
                            data-share-access="anyone"
                            data-share-access-label="Anyone with the link"
                            data-share-access-description="Anyone with the link can open this link.">
                      Anyone with the link
                    </button>
                  </div>
                </div>
                <div class="opctx-share-control opctx-share-control--role">
                  <button type="button"
                          class="opctx-share-control-toggle opctx-share-role-toggle"
                          aria-haspopup="menu"
                          aria-expanded="false"
                          aria-controls="opctx-share-role-menu"
                          aria-label="Share role">
                    <span data-share-role-current>Viewer</span>
                    <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true" focusable="false" class="opctx-share-control-chevron"><path d="M2 4l4 4 4-4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/></svg>
                  </button>
                  <div id="opctx-share-role-menu"
                       class="opctx-pill-menu opctx-share-role-menu"
                       role="menu"
                       aria-label="Share role"
                       hidden>
                    <button type="button" class="opctx-pill-menu-item" role="menuitemradio" aria-checked="true" data-share-role="viewer">Viewer</button>
                    <button type="button" class="opctx-pill-menu-item" role="menuitemradio" aria-checked="false" data-share-role="commenter">Commenter</button>
                    <button type="button" class="opctx-pill-menu-item" role="menuitemradio" aria-checked="false" data-share-role="editor">Editor</button>
                  </div>
                </div>
              </div>
            </div>
          </section>
        </div>
        <footer class="opctx-share-modal-foot">
          <button type="button" class="opctx-share-copy-link">Copy link</button>
          <button type="button" class="opctx-share-done">Done</button>
        </footer>
      </div>
    </div>`;
}

// Inject the version-selector block into the TOC HTML (round 3 —
// the version chrome lives inside the TOC, not the header). Three
// cases:
//
//   1. Page has a TOC and a family — splice the family block in
//      right after `<nav class="opctx-toc" ...>` and before the
//      "CONTENTS" label so the version row is visually first.
//   2. Page has NO TOC but has a family (rare — a family parent
//      with no H2/H3 headings) — emit a minimal <nav> wrapper so
//      the version chrome still appears in the sidebar.
//   3. Page has a TOC but no family — pass-through.
//
// Pages with neither TOC nor family return their original tocHtml
// (which may be ''). Empty string is fine — the layout grid hides
// the column when there's no content.
function injectVersionIntoToc(tocHtml, familyTocBlock) {
  if (!familyTocBlock) return tocHtml;
  const navOpen = /<nav\s+class="opctx-toc"[^>]*>/;
  if (navOpen.test(tocHtml)) {
    return tocHtml.replace(navOpen, (match) => `${match}\n      ${familyTocBlock}`);
  }
  // No existing TOC nav — wrap the family block in a minimal nav
  // shell so the layout still places it in the sidebar column.
  return `<nav class="opctx-toc opctx-toc--version-only" aria-label="Snapshot picker">
      ${familyTocBlock}
    </nav>`;
}

export function renderShell({
  frontmatter,
  bodyHtml,
  tocHtml,
  audienceStreams = null,
  activeAudienceStream = 'public',
  talkConventionsHtml = null,
  talkConventionsLabel = null,
}) {
  const {
    title,
    summary = '',
    slug,
    access = 'public',
    md_url,
    language = 'en',
    status = 'published',
    superseded_by,
    deprecation_notice,
    keywords,
    noindex = false,
    // Owner namespace for the brand-menu links. Empty by default →
    // root-relative URLs. Pages can override per-deploy.
    owner = '',
    home_href,
    // Display defaults — page-level recommended values for the
    // customizer settings. Reader's localStorage wins if set.
    theme_default = 'auto',
    article_width = 's',
    font_size = 'm',
    border_radius = 'rounded',
    links_style = 'color',
    cover_image = 'show',
    article_style = 'full',
    // Structural toggles — page-level features the agent enables/disables.
    talk_enabled = true,
    footer_enabled = true,
  } = frontmatter;

  const mdHref = md_url || `/${slug}.md`;
  const tierTitle = {
    public: 'Anyone with the URL can read',
    shared: 'Shared with specific users',
    private: 'Only you',
  }[access] || access;
  const tierLabel = access.charAt(0).toUpperCase() + access.slice(1);
  const visibilityLabel = {
    public: 'Public — internet-readable',
    shared: 'Shared — accessible to specified users',
    private: 'Private — only you',
  }[access] || tierLabel;
  const headerActions = audienceStreams
    ? renderAudienceSwitcher({ audienceStreams, activeAudienceStream })
    : renderTierBadge({ access, tierTitle, tierLabel });

  // Page defaults emitted as data-* attributes on <html>. The
  // no-flash bootstrap script + enhance.js's loadSetting() prefer
  // localStorage, then fall back to these. So an agent can declare
  // "this page is best read in dark mode" without overriding a
  // reader who has chosen light system-wide.
  const dataAttrs = [
    `data-theme="${escape(theme_default)}"`,
    `data-article-width="${escape(article_width)}"`,
    `data-font-size="${escape(font_size)}"`,
    `data-border-radius="${escape(border_radius)}"`,
    `data-links-style="${escape(links_style)}"`,
    `data-cover-image="${escape(cover_image)}"`,
    `data-article-style="${escape(article_style)}"`,
    talk_enabled ? '' : 'data-talk-enabled="false"',
  ].filter(Boolean).join(' ');

  const robotsMeta = noindex
    ? '\n  <meta name="robots" content="noindex, nofollow">'
    : '';
  const keywordsMeta = Array.isArray(keywords) && keywords.length
    ? `\n  <meta name="keywords" content="${escape(keywords.join(', '))}">`
    : '';

  // Status banner — surfaces lifecycle state to the reader. Draft and
  // archived pages get a banner; superseded pages link to the
  // replacement; deprecation_notice overrides the auto-generated copy.
  let statusBanner = '';
  if (deprecation_notice) {
    statusBanner = `\n  <aside class="opctx-status-banner" data-status="${escape(status)}" role="note">${escape(deprecation_notice)}</aside>`;
  } else if (status === 'draft') {
    statusBanner = `\n  <aside class="opctx-status-banner" data-status="draft" role="note">Draft — work in progress, not yet considered authoritative.</aside>`;
  } else if (status === 'archived') {
    statusBanner = `\n  <aside class="opctx-status-banner" data-status="archived" role="note">Archived — kept for history; no longer maintained.</aside>`;
  } else if (status === 'superseded' && superseded_by) {
    statusBanner = `\n  <aside class="opctx-status-banner" data-status="superseded" role="note">Superseded by <a href="/${escape(superseded_by)}.html">${escape(superseded_by)}</a>.</aside>`;
  }

  const footer = footer_enabled
    ? `\n  <footer class="opctx-site-footer">
    <img src="/assets/onecontext-icon-64.png" alt="" width="22" height="22" class="opctx-site-footer-logo">
    <span class="opctx-site-footer-text"><strong>1Context</strong> by <a href="https://haptica.ai" class="opctx-site-footer-haptica">Haptica</a></span>
  </footer>`
    : '';

  // Family / version-dropdown chrome. Detect whether this page belongs
  // to a versioned family (slug pattern `<family>-YYYY-MM-DD` or
  // explicit `family:` frontmatter); if so, emit the version-menu
  // chrome and a JSON data island the chrome JS can read. Pages with
  // no family (concept/, 1context, agent-ux, biography snapshots that
  // opt out) get an empty string and no chrome appears — no design
  // language for "this page has no family" to dispute, since the
  // visual absence is the right answer.
  //
  // Section sub-pages inherit the parent's slug (overridden to the
  // section slug like `2026-04-26`), which does NOT match the dated
  // family regex (requires `<word>-YYYY-MM-DD`). So sub-pages don't
  // get the version-dropdown — by design. The user navigates back
  // to the parent first, then uses the dropdown.
  const family = detectFamilyForShell(slug, frontmatter);
  let familyTocBlock = '';
  let familyDataScript = '';
  let familyInfiniteScrollScript = '';
  if (family) {
    familyTocBlock = `<div class="opctx-toc-version">
      ${renderVersionMenu({
        family: family.family,
        date: family.date,
        slug,
      })}
    </div>
    `;
    // Data island. The chrome JS reads this once on load to know:
    //   - which family-index.json to fetch
    //   - what the current page's slug + date are (so it can mark
    //     the current entry in the dropdown and seed the
    //     infinite-scroll state)
    //
    // JSON.stringify is pre-escaped enough — but we still need to
    // close-tag-escape against the </script> sequence in case any
    // family/title contains it (defense in depth, no real-world
    // family does today). The standard trick: replace `</` with
    // `<\/` inside the JSON, which is invalid in HTML script
    // contexts but valid JS.
    const payload = {
      family: family.family,
      date: family.date,
      slug,
      title,
      indexUrl: `${family.family}-index.json`,
    };
    const safeJson = JSON.stringify(payload).replace(/<\//g, '<\\/');
    familyDataScript = `\n  <script id="opctx-family-data" type="application/json">${safeJson}</script>`;

    // Infinite scroll into older snapshots. This script is only
    // emitted on family parent pages; section sub-pages are
    // agent-focused atomic units and don't get the chain. The
    // mechanism:
    //   1. Wait for the version-menu hydration (so we know the full
    //      member list).
    //   2. Insert a sentinel + status node just above the See-also
    //      block (or at end of article body if no See-also).
    //   3. Watch the sentinel with IntersectionObserver, root
    //      margin 600px so we start the fetch before the reader
    //      hits the bottom.
    //   4. On hit, fetch the next-older snapshot's HTML, extract
    //      the article body (without H1, subtitle, or its own
    //      See-also), wrap with a divider, append before the
    //      sentinel, advance the cursor.
    //   5. As the reader scrolls, update aria-current in the
    //      version-menu and history.replaceState to the snapshot
    //      whose body is centered in the viewport.
    //   6. When no older snapshots remain, swap the sentinel for
    //      a clean "End of history" footer.
    //   7. On fetch failure, show an inline retry button.
    //
    // CSS lives at the bottom of theme.css (.opctx-snapshot-divider,
    // .opctx-snapshot-end-of-history, .opctx-snapshot-retry).
    familyInfiniteScrollScript = `
  <script>
    /* Family infinite scroll. Lazy-loads older snapshots as the
     * reader approaches the bottom. Only runs on family parent
     * pages (the data island is the gate). */
    (function() {
      var dataEl = document.getElementById('opctx-family-data');
      if (!dataEl) return;
      var familyData;
      try { familyData = JSON.parse(dataEl.textContent); } catch (e) { return; }
      if (!familyData || !familyData.indexUrl) return;

      var article = document.querySelector('article.opctx-article');
      if (!article) return;
      // Tag the original (server-rendered) snapshot so the
      // bookkeeping below treats it the same as appended ones.
      var originalSection = document.createElement('section');
      originalSection.className = 'opctx-snapshot';
      originalSection.setAttribute('data-snapshot-slug', familyData.slug);
      originalSection.setAttribute('data-snapshot-date', familyData.date);
      originalSection.setAttribute('data-snapshot-original', 'true');
      // Move all current article children into the wrapper EXCEPT
      // the H1 and its immediate subtitle, which should stay as
      // the page's own heading. Easier: leave H1/subtitle in place
      // and wrap from the first H2 (or the See-also) onward.
      // Simpler: we don't need to wrap the original. We just need
      // a "sentinel" node placed before the See-also footer, and
      // we need to know the original snapshot's bounds for the
      // scroll-position-tracking logic. So tag the article itself
      // with the current snapshot id, and emit appended siblings
      // as <section> children of <article>.
      article.setAttribute('data-current-snapshot-slug', familyData.slug);
      article.setAttribute('data-current-snapshot-date', familyData.date);

      // Find See-also in the current article. Two cases:
      //   - source-rendered: an <h2> whose id slugifies to "See_also"
      //     (slugifyHeading uppercases first letters, joins with _)
      //   - post-enhance.js: a <details class="opctx-appendix"> with
      //     the same id, because enhance.js wraps appendices late.
      // Either way, treat that element as the boundary: append the
      // sentinel before it so newly-loaded snapshots slide in
      // between body content and the See-also footer.
      //
      // We also handle the racey case where enhance.js wraps AFTER
      // we install — the sentinel ends up inside the <details> if
      // we put it after the H2. To be safe, we look for both
      // (case-insensitive id selector) and re-check after a tick.
      function findSeeAlso() {
        return (
          article.querySelector('[id="See_also"]') ||
          article.querySelector('[id="see-also"]') ||
          article.querySelector('details.opctx-appendix') ||
          // Fall back: any h2 with text matching the appendix regex
          // (mirrors enhance.js's APPENDIX_REGEX).
          (function() {
            var h2s = article.querySelectorAll('h2');
            var re = /^(notes?|references|citations?|footnotes?|further reading|external links?|see also|bibliography|sources|selected bibliography|notes and references)$/i;
            for (var i = 0; i < h2s.length; i++) {
              if (re.test(h2s[i].textContent.trim())) return h2s[i];
            }
            return null;
          })()
        );
      }
      var seeAlsoEl = findSeeAlso();

      // Sentinel + status node.
      var sentinel = document.createElement('div');
      sentinel.className = 'opctx-snapshot-sentinel';
      sentinel.setAttribute('aria-hidden', 'true');
      var status = document.createElement('div');
      status.className = 'opctx-snapshot-status';
      status.setAttribute('role', 'status');
      status.setAttribute('aria-live', 'polite');
      status.textContent = '';

      function placeSentinels() {
        // Re-locate See-also; it may have been wrapped into a
        // details by enhance.js between the initial install and now.
        seeAlsoEl = findSeeAlso();
        if (sentinel.parentNode) sentinel.parentNode.removeChild(sentinel);
        if (status.parentNode) status.parentNode.removeChild(status);
        if (seeAlsoEl && seeAlsoEl.parentNode === article) {
          article.insertBefore(sentinel, seeAlsoEl);
          article.insertBefore(status, seeAlsoEl);
        } else {
          article.appendChild(sentinel);
          article.appendChild(status);
        }
      }
      placeSentinels();
      // Re-place after enhance.js gets a chance to run (it wraps
      // appendices on DOMContentLoaded). Two ticks: one for any
      // sync mutations, one after a frame for defer-loaded enhance.
      setTimeout(placeSentinels, 0);
      requestAnimationFrame(function() {
        requestAnimationFrame(placeSentinels);
      });

      // State.
      var members = []; // populated when version menu hydrates.
      // Append cursor: index of the next member to fetch. Members
      // are sorted most-recent-first; current snapshot is at index
      // 0; we append starting from the next one.
      var nextIndex = -1; // -1 = unknown until hydrated
      var loading = false;
      var done = false;
      var failed = false;
      var lastReplacedSlug = familyData.slug;

      function announceLive(msg) {
        // Brief flash so screen readers re-announce.
        status.textContent = '';
        // Force a tick.
        setTimeout(function() { status.textContent = msg; }, 30);
      }

      function strip(html, tagSelector) {
        // Server-side parsing in the renderer is impractical for
        // arbitrary appended HTML; use the browser's parser via
        // DOMParser to extract only what we want.
        var doc = new DOMParser().parseFromString(html, 'text/html');
        var article = doc.querySelector('article.opctx-article');
        if (!article) return null;
        // Remove ALL H1s (the page shell adds one, and the source
        // .md typically begins with a "# Title" line which produces
        // a second one in bodyHtml), plus subtitle, status banner,
        // and the leading rolling-window blockquote which describes
        // the parent and would only be confusing inside a divided
        // older snapshot.
        var dropAll = ['h1', 'p.opctx-subtitle', '.opctx-status-banner'];
        dropAll.forEach(function(sel) {
          var nodes = article.querySelectorAll(sel);
          for (var i = 0; i < nodes.length; i++) {
            if (nodes[i].parentNode) nodes[i].parentNode.removeChild(nodes[i]);
          }
        });
        // Remove the FIRST blockquote if it appears at the very
        // top of the body (post-removal-of-h1) — that's the
        // "Rolling 14-day view…" intro which makes no sense as
        // older-snapshot content.
        var firstChild = article.firstElementChild;
        while (firstChild && firstChild.tagName === 'P' && !firstChild.textContent.trim()) {
          var next = firstChild.nextElementSibling;
          if (firstChild.parentNode) firstChild.parentNode.removeChild(firstChild);
          firstChild = next;
        }
        if (firstChild && firstChild.tagName === 'BLOCKQUOTE') {
          firstChild.parentNode.removeChild(firstChild);
        }
        // Remove See-also (h2 with id See_also/see-also or text
        // matching the appendix regex) and everything after it.
        var seeAlsoRe = /^(notes?|references|citations?|footnotes?|further reading|external links?|see also|bibliography|sources|selected bibliography|notes and references)$/i;
        var seeAlso = article.querySelector('[id="See_also"]')
          || article.querySelector('[id="see-also"]')
          || (function() {
            var h2s = article.querySelectorAll('h2');
            for (var i = 0; i < h2s.length; i++) {
              if (seeAlsoRe.test(h2s[i].textContent.trim())) return h2s[i];
            }
            return null;
          })();
        if (seeAlso) {
          var n = seeAlso;
          var doomed = [];
          while (n) { doomed.push(n); n = n.nextSibling; }
          doomed.forEach(function(d) { if (d.parentNode) d.parentNode.removeChild(d); });
        }
        // Audience-stream pages wrap the body in an animation stage.
        // Infinite-scroll wants the raw article body children, not the
        // switcher shell, so unwrap down to ".opctx-article-body" when
        // present.
        var audienceBody = article.querySelector('.opctx-article-body');
        if (audienceBody) {
          while (article.firstChild) article.removeChild(article.firstChild);
          while (audienceBody.firstChild) article.appendChild(audienceBody.firstChild);
        }
        return article;
      }

      function appendSnapshot(member) {
        loading = true;
        announceLive('Loading ' + member.date + ' snapshot…');
        clearRetry();
        var requestedStream = (
          window.__opctxAudienceStream &&
          window.__opctxAudienceStream.active &&
          window.__opctxAudienceStream.active !== 'public'
        ) ? window.__opctxAudienceStream.active : 'public';
        var targetUrl = member.url;
        if (requestedStream !== 'public') {
          targetUrl = member.url.replace(/\\.html$/, '.' + requestedStream + '.html');
        }
        return fetch(targetUrl, { credentials: 'same-origin' })
          .then(function(r) {
            if (!r.ok && requestedStream !== 'public') {
              return fetch(member.url, { credentials: 'same-origin' });
            }
            return r;
          })
          .then(function(r) {
            if (!r.ok) throw new Error('HTTP ' + r.status);
            return r.text();
          })
          .then(function(html) {
            var bodyArticle = strip(html);
            if (!bodyArticle) throw new Error('no article element in fetched snapshot');

            var wrap = document.createElement('section');
            wrap.className = 'opctx-snapshot opctx-snapshot-appended';
            wrap.setAttribute('data-snapshot-slug', member.slug);
            wrap.setAttribute('data-snapshot-date', member.date);
            // Divider header.
            var divider = document.createElement('div');
            divider.className = 'opctx-snapshot-divider';
            divider.setAttribute('role', 'separator');
            divider.setAttribute('aria-label', 'Older snapshot from ' + member.date);
            // Render with ISO + pretty date for readers + agents.
            var pretty = member.date;
            try {
              var p = (member.date || '').split('-').map(Number);
              if (p.length === 3 && p[0] && p[1] && p[2]) {
                pretty = new Date(p[0], p[1] - 1, p[2]).toLocaleDateString(undefined, {
                  weekday: 'long', month: 'long', day: 'numeric', year: 'numeric',
                });
              }
            } catch (e) { /* keep ISO */ }
            divider.innerHTML =
              '<span class="opctx-snapshot-divider-line" aria-hidden="true"></span>' +
              '<span class="opctx-snapshot-divider-label">' +
                '<span class="opctx-snapshot-divider-date">' + pretty + '</span>' +
                '<span class="opctx-snapshot-divider-iso">' + member.date + '</span>' +
              '</span>' +
              '<span class="opctx-snapshot-divider-line" aria-hidden="true"></span>';
            wrap.appendChild(divider);
            // Move the fetched article's children into the wrapper.
            while (bodyArticle.firstChild) {
              wrap.appendChild(bodyArticle.firstChild);
            }
            // Insert before the sentinel.
            article.insertBefore(wrap, sentinel);
            loading = false;
            failed = false;
            announceLive('Loaded ' + member.date + ' snapshot.');
          })
          .catch(function(err) {
            loading = false;
            failed = true;
            showRetry(member, err);
            announceLive('Could not load ' + member.date + '. Retry available.');
          });
      }

      function clearRetry() {
        var existing = sentinel.querySelector('.opctx-snapshot-retry');
        if (existing && existing.parentNode) existing.parentNode.removeChild(existing);
      }
      function showRetry(member, err) {
        clearRetry();
        var btn = document.createElement('button');
        btn.type = 'button';
        btn.className = 'opctx-snapshot-retry';
        btn.textContent = 'Retry loading ' + member.date;
        btn.addEventListener('click', function() {
          btn.disabled = true;
          btn.textContent = 'Retrying…';
          appendSnapshot(member).then(function() { tryFetchNext(); });
        });
        sentinel.appendChild(btn);
      }

      function showEndOfHistory() {
        clearRetry();
        sentinel.classList.add('opctx-snapshot-end-of-history');
        sentinel.removeAttribute('aria-hidden');
        sentinel.setAttribute('role', 'note');
        sentinel.innerHTML =
          '<span class="opctx-snapshot-end-line" aria-hidden="true"></span>' +
          '<span class="opctx-snapshot-end-label">End of history — no older snapshots</span>' +
          '<span class="opctx-snapshot-end-line" aria-hidden="true"></span>';
        announceLive('End of history.');
      }

      function tryFetchNext() {
        if (loading || done || failed) return;
        if (nextIndex < 0 || nextIndex >= members.length) {
          done = true;
          showEndOfHistory();
          return;
        }
        var member = members[nextIndex];
        nextIndex += 1;
        appendSnapshot(member).then(function() {
          // After the append, schedule another check; the new
          // content may or may not push the sentinel out of the
          // viewport. The IntersectionObserver fires again
          // automatically if it's still in view.
        });
      }

      // Wait for version-menu hydration to know the member list.
      function setMembers(list, currentSlug) {
        members = list.slice();
        // Find current slug in the (sorted) list.
        var idx = members.findIndex(function(m) { return m.slug === currentSlug; });
        if (idx === -1) idx = 0;
        // Older snapshots come AFTER the current one in
        // most-recent-first order.
        nextIndex = idx + 1;
        if (nextIndex >= members.length) {
          // No older snapshots at all — show end-of-history right
          // away rather than waiting for a scroll.
          done = true;
          showEndOfHistory();
        }
      }

      document.addEventListener('opctx:version-menu-hydrated', function(e) {
        if (!e.detail || !Array.isArray(e.detail.members)) return;
        setMembers(e.detail.members, e.detail.currentSlug);
        // If user has already scrolled near the bottom by the
        // time hydration finishes, kick a fetch.
        if (sentinelInView) tryFetchNext();
      });
      // Also handle the case where hydration happened before this
      // listener was registered.
      if (window.__opctxVersionMenu && Array.isArray(window.__opctxVersionMenu.members)) {
        setMembers(window.__opctxVersionMenu.members, window.__opctxVersionMenu.currentSlug);
      }

      // IntersectionObserver to drive fetches.
      var sentinelInView = false;
      var io = new IntersectionObserver(function(entries) {
        entries.forEach(function(entry) {
          sentinelInView = entry.isIntersecting;
          if (sentinelInView) tryFetchNext();
        });
      }, {
        // Fire when the sentinel is within 600px of the viewport
        // bottom. rootMargin bottom expands the trigger area.
        rootMargin: '0px 0px 600px 0px',
        threshold: 0,
      });
      io.observe(sentinel);

      // Rebuild the TOC's content list so it reflects the
      // currently-visible era. Two cases:
      //   - Original snapshot: the article itself carries
      //     data-current-snapshot-slug; the body content is the
      //     article's direct H2/H3 descendants OUTSIDE any
      //     appended-snapshot section.
      //   - Appended snapshot: a section with data-snapshot-slug
      //     attribute was inserted between body and See-also.
      // Falls back silently if neither match.
      function rebuildTocForSnapshot(slug) {
        window.__opctxRebuildTrace = window.__opctxRebuildTrace || [];
        var trace = { slug: slug, t: Date.now() };
        var toc = document.querySelector('.opctx-toc');
        if (!toc) { trace.bail = 'no toc'; window.__opctxRebuildTrace.push(trace); return; }
        var list = toc.querySelector('#opctx-toc-list');
        if (!list) {
          // List may have been re-located by injectTocHead. Fall back
          // to any direct ol/ul descendant.
          list = toc.querySelector(':scope ol, :scope ul');
        }
        if (!list) { trace.bail = 'no list'; window.__opctxRebuildTrace.push(trace); return; }
        var headings;
        var snapshot = document.querySelector('[data-snapshot-slug="' + slug + '"]');
        if (snapshot) {
          headings = snapshot.querySelectorAll('h2[id], h3[id]');
          trace.path = 'wrapped';
        } else if (article && article.getAttribute('data-current-snapshot-slug') === slug) {
          var all = Array.from(article.querySelectorAll('h2[id], h3[id]'));
          headings = all.filter(function(h) {
            return !h.closest('[data-snapshot-slug]');
          });
          trace.path = 'original';
        } else {
          trace.bail = 'no match (article slug=' + (article ? article.getAttribute('data-current-snapshot-slug') : 'no-article') + ')';
          window.__opctxRebuildTrace.push(trace);
          return;
        }
        trace.headingsCount = headings.length;
        if (!headings.length) { trace.bail = 'no headings'; window.__opctxRebuildTrace.push(trace); return; }

        var html = '';
        var inSubList = false;

        function escText(s) {
          return String(s == null ? '' : s)
            .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
        }
        function escAttr(s) {
          return String(s == null ? '' : s)
            .replace(/&/g, '&amp;').replace(/</g, '&lt;')
            .replace(/>/g, '&gt;').replace(/"/g, '&quot;');
        }

        for (var i = 0; i < headings.length; i++) {
          var h = headings[i];
          var id = h.id;
          var text = (h.textContent || '').trim();
          var level = h.tagName === 'H2' ? 2 : 3;
          if (level === 2) {
            if (inSubList) { html += '</ul></li>'; inSubList = false; }
            // Look ahead — if the next heading is an H3, open a
            // sublist for it.
            var nextIsSub = false;
            for (var j = i + 1; j < headings.length; j++) {
              if (headings[j].tagName === 'H2') break;
              if (headings[j].tagName === 'H3') { nextIsSub = true; break; }
            }
            html += '<li><a href="#' + escAttr(id) + '">' + escText(text) + '</a>';
            if (nextIsSub) { html += '<ul>'; inSubList = true; }
            else { html += '</li>'; }
          } else if (level === 3) {
            html += '<li class="is-sub"><a href="#' + escAttr(id) + '">' + escText(text) + '</a></li>';
          }
        }
        if (inSubList) html += '</ul></li>';

        list.innerHTML = html;
        trace.applied = true;
        window.__opctxRebuildTrace.push(trace);
      }

      // Scroll-position tracking. As the reader crosses snapshot
      // boundaries (i.e., the snapshot whose top is within the
      // upper half of the viewport changes), update the
      // version-dropdown's aria-current and the URL via
      // history.replaceState.
      var rafScheduled = false;
      function onScroll() {
        if (rafScheduled) return;
        rafScheduled = true;
        requestAnimationFrame(function() {
          rafScheduled = false;
          // Find the snapshot section closest to the top quarter
          // of the viewport.
          var snapshots = article.querySelectorAll('[data-snapshot-slug]');
          // Plus the article itself (which represents the
          // original snapshot when nothing has been appended).
          var threshold = window.innerHeight * 0.25;
          var bestSlug = familyData.slug;
          var bestDate = familyData.date;
          // Walk in order; pick the last one whose top is above
          // the threshold (we're inside it).
          for (var i = 0; i < snapshots.length; i++) {
            var rect = snapshots[i].getBoundingClientRect();
            if (rect.top <= threshold) {
              bestSlug = snapshots[i].getAttribute('data-snapshot-slug');
              bestDate = snapshots[i].getAttribute('data-snapshot-date');
            } else {
              break;
            }
          }
          // Original snapshot edge case — when the article hasn't
          // scrolled out yet, treat the article wrapper as the
          // current snapshot.
          var articleRect = article.getBoundingClientRect();
          if (articleRect.top > threshold) {
            // Above the article entirely — keep the original.
            bestSlug = familyData.slug;
            bestDate = familyData.date;
          }
          if (bestSlug !== lastReplacedSlug) {
            lastReplacedSlug = bestSlug;
            window.__opctxLastEraChange = { slug: bestSlug, t: Date.now() };
            // Era pill follows the scroll — visual cue of which
            // era's content is currently centered. The Era pill
            // is a navigation control to OTHER eras; the active
            // marker on it just reflects the current view.
            if (window.__opctxVersionMenu && typeof window.__opctxVersionMenu.setActive === 'function') {
              window.__opctxVersionMenu.setActive(bestSlug);
            }
            // Rebuild the TOC contents to reflect the active era.
            // Each era has its own isolated TOC. Concatenating
            // across eras would balloon the list and create the
            // mid-scroll "weirdness" the user flagged.
            rebuildTocForSnapshot(bestSlug);
            // Note: we deliberately do NOT history.replaceState the
            // URL or change document.title on scroll. Either would
            // make a refresh land the user mid-scroll on a deeper
            // era's page (which would then auto-trigger its own
            // older-snapshot fetch, double-loading content). The
            // URL is stable to whatever the user picked from the
            // Era dropdown (or arrived at via direct link); scroll
            // updates only the on-page chrome.
          }
        });
      }
      window.addEventListener('scroll', onScroll, { passive: true });
    })();
  </script>`;
  }

  let audienceDataScript = '';
  if (audienceStreams && audienceStreams.streams) {
    const payload = {
      active: activeAudienceStream,
      order: audienceStreams.order || AUDIENCE_ORDER,
      streams: Object.fromEntries(
        Object.entries(audienceStreams.streams).map(([key, stream]) => [key, {
          label: stream.label || titleCaseAudience(key),
          url: stream.url,
        }])
      ),
    };
    const safeJson = JSON.stringify(payload).replace(/<\//g, '<\\/');
    audienceDataScript = `\n  <script id="opctx-audience-data" type="application/json">${safeJson}</script>`;
  }

  // Wikipedia-style talk-page conventions banner. Collapsed by default;
  // the summary line carries the visible-by-default one-liner so the
  // page doesn't get dominated by rules. The full conventions render
  // inside the <details> when expanded — same affordance as
  // talk-conventions on Wikipedia (Talk header banner + WP:TPG link).
  const conventionsBanner = talkConventionsHtml
    ? `<details class="opctx-talk-conventions" data-collapsed-by-default="true">
        <summary><strong>Talk-page conventions</strong>${talkConventionsLabel ? ` — ${escape(talkConventionsLabel)}` : ''} <span class="opctx-talk-conventions-hint">click to expand</span></summary>
        <div class="opctx-talk-conventions-body">${talkConventionsHtml}</div>
      </details>`
    : '';

  const articleBody = audienceStreams
    ? `<div class="opctx-audience-stage" data-active-audience="${escape(activeAudienceStream)}">
        <div class="opctx-article-body opctx-audience-stream-panel is-current" data-audience-stream="${escape(activeAudienceStream)}">
          ${conventionsBanner}${bodyHtml}
        </div>
      </div>`
    : `<div class="opctx-article-body">${conventionsBanner}${bodyHtml}</div>`;
  const shareModalHtml = audienceStreams ? renderShareModal({ title, activeAudienceStream }) : '';

  return `<!doctype html>
<html lang="${escape(language)}" ${dataAttrs}>
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
  <title>${escape(title)} — 1Context</title>

  <link rel="alternate" type="text/markdown" href="${escape(mdHref)}">
  <link rel="icon" type="image/png" sizes="32x32" href="/assets/favicon-32.png">
  <link rel="icon" type="image/png" sizes="16x16" href="/assets/favicon-16.png">
  <link rel="apple-touch-icon" sizes="180x180" href="/assets/apple-touch-icon.png">

  <meta name="generator" content="1Context wiki-engine">${robotsMeta}${keywordsMeta}

  <meta property="og:title" content="${escape(title)}">
  <meta property="og:type"  content="article">
  <meta property="og:description" content="${escape(summary)}">
  <meta name="description" content="${escape(summary)}">

  <script>
    /* No-flash theme bootstrap. enhance.js reads opctx-theme from
     * localStorage and applies it as data-theme on <html>, but that
     * runs after CSS paints. If the user picked "light" but their
     * system is dark, the page would paint dark for ~50ms before JS
     * flipped it. Reading localStorage here (synchronous, blocking,
     * pre-paint) avoids the flash. The HTML-emitted data-theme is the
     * page-recommended default; localStorage wins if set. */
    (function() {
      try {
        var t = localStorage.getItem('opctx-theme');
        if (t === 'light' || t === 'dark') {
          document.documentElement.dataset.theme = t;
        }
      } catch (e) { /* localStorage blocked — keep page-default */ }
    })();
  </script>
  <link rel="stylesheet" href="/assets/theme.css">
  <script type="module" src="/assets/enhance.js" defer></script>
</head>
<body>
  <div class="opctx-visibility-bar" data-tier="${escape(access)}" aria-label="${escape(visibilityLabel)}"></div>
  <div class="opctx-progress-bar" aria-hidden="true"></div>

  <header class="opctx-header">
    ${renderBrandMenu({ owner, homeHref: home_href })}
    <div class="opctx-header-search">
      <input type="search" placeholder="Search pages, books, tags…" aria-label="Search">
    </div>
    <div class="opctx-header-actions">
      ${headerActions}
    </div>
  </header>${familyDataScript}${audienceDataScript}

  <div class="opctx-layout">
    ${injectVersionIntoToc(tocHtml, familyTocBlock)}

    <main class="opctx-main">
      <article class="opctx-article">
        <h1>${escape(title)}</h1>
        ${summary ? `<p class="opctx-subtitle">${escape(summary)}</p>` : ''}${statusBanner}

        ${articleBody}
      </article>
    </main>
  </div>${footer}${shareModalHtml}
  <script>
    /* Reusable menu wiring. Brand, version, audience, and the share
     * modal's internal pickers all use the same disclosure pattern:
     * button toggle, list of buttons/links, keyboard navigation,
     * Escape/outside-click dismiss. This stays local to the shell so
     * section sub-pages inherit it without a separate runtime module. */
    (function() {
      function closeOtherMenus(currentToggle) {
        var openToggles = document.querySelectorAll('[aria-expanded="true"][aria-haspopup="menu"]');
        for (var i = 0; i < openToggles.length; i++) {
          if (openToggles[i] === currentToggle) continue;
          openToggles[i].setAttribute('aria-expanded', 'false');
          var otherId = openToggles[i].getAttribute('aria-controls');
          if (!otherId) continue;
          var otherMenu = document.getElementById(otherId);
          if (otherMenu) otherMenu.setAttribute('hidden', '');
        }
      }

      function wireMenu(root, toggleSelector, menuSelector, opts) {
        if (!root) return null;
        opts = opts || {};
        var toggle = typeof toggleSelector === 'string'
          ? root.querySelector(toggleSelector)
          : toggleSelector;
        var menu = typeof menuSelector === 'string'
          ? root.querySelector(menuSelector)
          : menuSelector;
        if (!toggle || !menu) return null;

        function items() {
          return Array.prototype.slice.call(
            menu.querySelectorAll(
              'a[role="menuitem"], button[role="menuitem"], button[role="menuitemradio"], button[role="menuitemcheckbox"]'
            )
          );
        }
        function checkedItem() {
          return menu.querySelector('[aria-checked="true"]');
        }
        function setOpen(open, state) {
          toggle.setAttribute('aria-expanded', open ? 'true' : 'false');
          if (open) {
            closeOtherMenus(toggle);
            menu.removeAttribute('hidden');
            if (state && state.focusItems) {
              var list = items();
              var focusTarget = state.focusLast
                ? list[list.length - 1]
                : (checkedItem() || list[0]);
              if (focusTarget) focusTarget.focus();
            }
          } else {
            menu.setAttribute('hidden', '');
          }
        }
        function focusItem(delta) {
          var list = items();
          if (list.length === 0) return;
          var idx = list.indexOf(document.activeElement);
          if (idx === -1) idx = delta > 0 ? -1 : list.length;
          var next = (idx + delta + list.length) % list.length;
          list[next].focus();
        }
        toggle.addEventListener('click', function(e) {
          e.preventDefault();
          var open = toggle.getAttribute('aria-expanded') === 'true';
          setOpen(!open);
        });
        toggle.addEventListener('keydown', function(e) {
          if (e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            setOpen(true, { focusItems: true });
          } else if (e.key === 'ArrowUp') {
            e.preventDefault();
            setOpen(true, { focusItems: true, focusLast: true });
          }
        });
        document.addEventListener('click', function(e) {
          if (!root.contains(e.target)) setOpen(false);
        });
        document.addEventListener('keydown', function(e) {
          if (toggle.getAttribute('aria-expanded') !== 'true') return;
          if (e.key === 'Escape') {
            setOpen(false);
            toggle.focus();
            return;
          }
          if (!menu.contains(document.activeElement)) return;
          if (e.key === 'ArrowDown') { e.preventDefault(); focusItem(1); }
          else if (e.key === 'ArrowUp') { e.preventDefault(); focusItem(-1); }
          else if (e.key === 'Home') {
            e.preventDefault();
            var list = items(); if (list[0]) list[0].focus();
          } else if (e.key === 'End') {
            e.preventDefault();
            var list = items(); if (list.length) list[list.length - 1].focus();
          } else if ((e.key === 'Enter' || e.key === ' ') && menu.contains(document.activeElement)) {
            e.preventDefault();
            if (typeof document.activeElement.click === 'function') {
              document.activeElement.click();
            }
          }
        });
        menu.addEventListener('focusout', function(e) {
          setTimeout(function() {
            if (!root.contains(document.activeElement)) setOpen(false);
          }, 0);
        });
        menu.addEventListener('click', function(e) {
          var item = e.target.closest(
            'a[role="menuitem"], button[role="menuitem"], button[role="menuitemradio"], button[role="menuitemcheckbox"]'
          );
          if (!item) return;
          if (typeof opts.onSelect === 'function') opts.onSelect(item, e);
          if (opts.closeOnSelect !== false) setOpen(false);
        });
        return { setOpen: setOpen, toggle: toggle, menu: menu };
      }

      // Wire the brand menu. Always present on every page.
      wireMenu(
        document.querySelector('.opctx-header-brand'),
        '.opctx-brand-menu-toggle',
        '.opctx-brand-menu'
      );

      // Wire the version picker. Only present on family pages.
      var versionRoot = document.querySelector('.opctx-header-version');
      var versionMenu = versionRoot
        ? wireMenu(versionRoot, '.opctx-version-toggle', '.opctx-version-menu', {
            onSelect: function(item) {
              var url = item.getAttribute('data-version-url');
              if (url) location.href = url;
            },
          })
        : null;

      function setVersionSelection(slug) {
        if (!versionRoot) return;
        var items = versionRoot.querySelectorAll('[data-version-slug]');
        var label = versionRoot.querySelector('.opctx-version-toggle-label');
        for (var i = 0; i < items.length; i++) {
          var match = items[i].getAttribute('data-version-slug') === slug;
          items[i].setAttribute('aria-checked', match ? 'true' : 'false');
          if (match && label) {
            label.textContent = items[i].getAttribute('data-version-date') || items[i].textContent.trim();
          }
        }
      }

      // Hydrate the version picker from <family>-index.json. The
      // server emits a one-option skeleton containing just the
      // current snapshot; here we fetch the full index and replace
      // the options list. If the
      // fetch fails, we leave the skeleton alone — graceful
      // degradation: at minimum the user sees what page they're on.
      var familyData = null;
      var dataEl = document.getElementById('opctx-family-data');
      if (dataEl) {
        try { familyData = JSON.parse(dataEl.textContent); } catch (e) { familyData = null; }
      }
      if (versionRoot && familyData && familyData.indexUrl) {
        // Resolve relative to the directory of the current page so
        // both /paul-demo2/for-you-... and any other owner work
        // without a hard-coded prefix.
        var indexUrl = new URL(familyData.indexUrl, location.href).toString();
        fetch(indexUrl, { credentials: 'same-origin' })
          .then(function(r) { return r.ok ? r.json() : null; })
          .then(function(data) {
            if (!data || !Array.isArray(data.members)) return;
            // Build a row per member, most-recent first. The data
            // file is already sorted, but resort defensively.
            var members = data.members.slice().sort(function(a, b) {
              return (b.date || '').localeCompare(a.date || '');
            });
            // Each For You page is a Monday-anchored week (era). The
            // dropdown shows just the short M/D/YY for each era — no
            // "Week of" prefix and no "(this week)" suffix. The user
            // already knows they're picking a week (the date is the
            // anchor), and the topmost option is the latest by sort
            // order, which is sufficient. The overlap between adjacent
            // eras is intentional: the same days get re-narrated from
            // a later vantage, giving the system a chance to correct
            // history with hindsight.
            function formatWeekShort(isoDate) {
              // 2026-04-20 → 4/20/26 (American MM/DD/YY).
              var parts = isoDate.split('-');
              if (parts.length !== 3) return isoDate;
              var year2 = parts[0].slice(2);
              var month = parseInt(parts[1], 10);
              var day = parseInt(parts[2], 10);
              return month + '/' + day + '/' + year2;
            }
            // Update the closed-pill label too — server rendered the
            // raw ISO date as a fallback before this hydration ran.
            var pillLabel = versionRoot.querySelector('.opctx-version-toggle-label');
            if (pillLabel && familyData.date) pillLabel.textContent = formatWeekShort(familyData.date);
            // Patch setVersionSelection so it rewrites the pill in
            // the same short week-shape when the active era changes
            // via dropdown click or scroll-into-era.
            var origSetSel = setVersionSelection;
            setVersionSelection = function(slug) {
              origSetSel(slug);
              var match = members.find(function(m) { return m.slug === slug; });
              if (match && pillLabel) pillLabel.textContent = formatWeekShort(match.date);
            };
            var currentSlug = familyData.slug;
            versionMenu.menu.innerHTML = members.map(function(m) {
              var label = formatWeekShort(m.date);
              return '<button type="button" class="opctx-pill-menu-item opctx-version-menu-item" ' +
                     'role="menuitemradio" aria-checked="' + (m.slug === currentSlug ? 'true' : 'false') + '" ' +
                     'data-version-url="' + escapeAttr(m.url) + '" ' +
                     'data-version-slug="' + escapeAttr(m.slug) + '" ' +
                     'data-version-date="' + escapeAttr(m.date) + '">' +
                     escapeText(label) + '</button>';
            }).join('');
            setVersionSelection(currentSlug);
            window.__opctxVersionMenu = {
              root: versionRoot,
              toggle: versionMenu ? versionMenu.toggle : null,
              menu: versionMenu ? versionMenu.menu : null,
              members: members,
              currentSlug: currentSlug,
              setActive: function(slug) {
                this.currentSlug = slug;
                for (var i = 0; i < members.length; i++) {
                  if (members[i].slug === slug) {
                    setVersionSelection(slug);
                    break;
                  }
                }
              },
            };
            // Dispatch an event so any other chrome (infinite-scroll)
            // can wait for hydration to complete.
            document.dispatchEvent(new CustomEvent('opctx:version-menu-hydrated', {
              detail: { members: members, currentSlug: currentSlug },
            }));
          })
          .catch(function() { /* keep skeleton — graceful */ });
      }

      var audienceData = null;
      var audienceDataEl = document.getElementById('opctx-audience-data');
      if (audienceDataEl) {
        try { audienceData = JSON.parse(audienceDataEl.textContent); } catch (e) { audienceData = null; }
      }

      var audienceRoot = document.querySelector('.opctx-audience-switcher');
      var audienceStage = document.querySelector('.opctx-audience-stage');
      var audienceHeading = document.querySelector('.opctx-article > h1');
      var audienceSubtitle = document.querySelector('.opctx-article > .opctx-subtitle');
      var audienceTocCurrent = document.querySelector('.opctx-toc-current');
      if (audienceRoot && audienceStage && audienceData && audienceData.streams) {
        var activeAudience = audienceData.active || 'public';
        var audienceCache = Object.create(null);
        var audienceBusy = false;
        var reduceMotion = false;
        var audienceMenu = null;
        var shareModal = document.querySelector('.opctx-share-modal-scrim');
        var shareDialog = shareModal ? shareModal.querySelector('.opctx-share-modal') : null;
        var shareInviteInput = shareModal ? shareModal.querySelector('#opctx-share-invite-input') : null;
        var shareCloseButton = shareModal ? shareModal.querySelector('.opctx-share-modal-close') : null;
        var shareDoneButton = shareModal ? shareModal.querySelector('.opctx-share-done') : null;
        var shareCopyButton = shareModal ? shareModal.querySelector('.opctx-share-copy-link') : null;
        var shareGeneralLabel = shareModal ? shareModal.querySelector('[data-share-general-label]') : null;
        var shareGeneralDescription = shareModal ? shareModal.querySelector('[data-share-general-description]') : null;
        var shareAccessCurrent = shareModal ? shareModal.querySelector('[data-share-access-current]') : null;
        var shareRoleCurrent = shareModal ? shareModal.querySelector('[data-share-role-current]') : null;
        var shareCopyDefaultLabel = shareCopyButton ? shareCopyButton.textContent : 'Copy link';
        var shareCopyTimer = 0;
        var shareReturnFocus = null;
        var sharePreviousBodyOverflow = '';
        var shareAccessState = 'restricted';
        var shareRoleState = 'viewer';
        var shareAccessMenu = shareModal
          ? wireMenu(shareModal.querySelector('.opctx-share-control--access'),
              '.opctx-share-access-toggle',
              '.opctx-share-access-menu',
              {
                onSelect: function(item) {
                  setShareAccess(item.getAttribute('data-share-access'));
                  setTimeout(function() {
                    if (shareAccessMenu && shareAccessMenu.toggle) shareAccessMenu.toggle.focus();
                  }, 0);
                },
              })
          : null;
        var shareRoleMenu = shareModal
          ? wireMenu(shareModal.querySelector('.opctx-share-control--role'),
              '.opctx-share-role-toggle',
              '.opctx-share-role-menu',
              {
                onSelect: function(item) {
                  setShareRole(item.getAttribute('data-share-role'));
                  setTimeout(function() {
                    if (shareRoleMenu && shareRoleMenu.toggle) shareRoleMenu.toggle.focus();
                  }, 0);
                },
              })
          : null;
        try {
          reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
        } catch (e) { reduceMotion = false; }

        function audienceIndex(key) {
          var order = Array.isArray(audienceData.order) ? audienceData.order : [];
          var idx = order.indexOf(key);
          return idx === -1 ? 0 : idx;
        }

        function readPreferredAudience() {
          try { return sessionStorage.getItem('opctx-audience-stream'); } catch (e) { return null; }
        }

        function persistAudience(key) {
          try { sessionStorage.setItem('opctx-audience-stream', key); } catch (e) { /* storage blocked */ }
        }

        function titleCase(s) { return s.charAt(0).toUpperCase() + s.slice(1); }

        function setAudienceSelection(key) {
          audienceRoot.dataset.activeAudience = key;
          var current = audienceRoot.querySelector('.opctx-audience-switcher-current');
          if (current) current.textContent = titleCase(key);
          var items = audienceRoot.querySelectorAll('[data-audience-target]');
          for (var i = 0; i < items.length; i++) {
            var match = items[i].getAttribute('data-audience-target') === key;
            items[i].setAttribute('aria-checked', match ? 'true' : 'false');
          }
          // Disable Share when the active audience is Private.
          var shareItem = audienceRoot.querySelector('[data-audience-action="share"]');
          if (shareItem) {
            if (key === 'private') {
              shareItem.setAttribute('disabled', '');
              shareItem.setAttribute('aria-disabled', 'true');
            } else {
              shareItem.removeAttribute('disabled');
              shareItem.setAttribute('aria-disabled', 'false');
            }
          }
          // Update the share modal's audience badge in place so a
          // user who opens Share later sees the right version label.
          var streamBadge = shareModal ? shareModal.querySelector('[data-share-stream-badge]') : null;
          if (streamBadge) {
            streamBadge.textContent = titleCase(key);
            streamBadge.setAttribute('data-stream', key);
          }
        }

        function getFocusable(rootEl) {
          return Array.prototype.slice.call(
            rootEl.querySelectorAll('input, button, a[href], [tabindex]:not([tabindex="-1"])')
          );
        }

        function trapFocus(rootEl, e) {
          if (e.key !== 'Tab') return;
          var focusable = getFocusable(rootEl);
          if (!focusable.length) return;
          var first = focusable[0];
          var last = focusable[focusable.length - 1];
          if (e.shiftKey && document.activeElement === first) {
            e.preventDefault();
            last.focus();
          } else if (!e.shiftKey && document.activeElement === last) {
            e.preventDefault();
            first.focus();
          }
        }

        function buildShareLink() {
          // Copy-link URL is audience-specific. Sharing the Internal
          // version produces a link to the .internal sibling so the
          // recipient lands on the same audience the user was on.
          // Falls back to the bare current URL if audience data is
          // unavailable (defensive — shouldn't happen on family pages).
          if (audienceData && audienceData.streams && audienceData.streams[activeAudience]) {
            var streamUrl = audienceData.streams[activeAudience].url;
            if (streamUrl) {
              return new URL(streamUrl, location.href).toString();
            }
          }
          var url = new URL(location.href);
          url.hash = '';
          return url.toString();
        }

        function fallbackCopyText(text) {
          var field = document.createElement('textarea');
          field.value = text;
          field.setAttribute('readonly', '');
          field.style.position = 'fixed';
          field.style.opacity = '0';
          document.body.appendChild(field);
          field.select();
          try { document.execCommand('copy'); } catch (e) { /* no-op */ }
          document.body.removeChild(field);
        }

        function setShareAccess(key) {
          if (!shareModal) return;
          shareAccessState = key === 'anyone' ? 'anyone' : 'restricted';
          var items = shareModal.querySelectorAll('[data-share-access]');
          for (var i = 0; i < items.length; i++) {
            var match = items[i].getAttribute('data-share-access') === shareAccessState;
            items[i].setAttribute('aria-checked', match ? 'true' : 'false');
            if (match) {
              var label = items[i].getAttribute('data-share-access-label') || items[i].textContent.trim();
              var description = items[i].getAttribute('data-share-access-description') || '';
              if (shareAccessCurrent) shareAccessCurrent.textContent = label;
              if (shareGeneralLabel) shareGeneralLabel.textContent = label;
              if (shareGeneralDescription) shareGeneralDescription.textContent = description;
            }
          }
        }

        function setShareRole(key) {
          if (!shareModal) return;
          shareRoleState = key || 'viewer';
          var items = shareModal.querySelectorAll('[data-share-role]');
          for (var i = 0; i < items.length; i++) {
            var match = items[i].getAttribute('data-share-role') === shareRoleState;
            items[i].setAttribute('aria-checked', match ? 'true' : 'false');
            if (match && shareRoleCurrent) {
              shareRoleCurrent.textContent = items[i].textContent.trim();
            }
          }
        }

        function openShareModal(returnFocusEl) {
          if (!shareModal || !shareDialog) return;
          if (shareAccessMenu) shareAccessMenu.setOpen(false);
          if (shareRoleMenu) shareRoleMenu.setOpen(false);
          shareReturnFocus = returnFocusEl || (audienceMenu ? audienceMenu.toggle : document.activeElement);
          sharePreviousBodyOverflow = document.body.style.overflow;
          document.body.style.overflow = 'hidden';
          shareModal.classList.add('is-open');
          shareModal.setAttribute('aria-hidden', 'false');
          if (shareInviteInput) shareInviteInput.value = '';
          window.clearTimeout(shareCopyTimer);
          if (shareCopyButton) shareCopyButton.textContent = shareCopyDefaultLabel;
          window.setTimeout(function() {
            if (shareInviteInput) shareInviteInput.focus();
            else shareDialog.focus();
          }, reduceMotion ? 0 : 24);
        }

        function closeShareModal() {
          if (!shareModal || !shareModal.classList.contains('is-open')) return;
          if (shareAccessMenu) shareAccessMenu.setOpen(false);
          if (shareRoleMenu) shareRoleMenu.setOpen(false);
          shareModal.classList.remove('is-open');
          shareModal.setAttribute('aria-hidden', 'true');
          document.body.style.overflow = sharePreviousBodyOverflow;
          if (shareReturnFocus && typeof shareReturnFocus.focus === 'function') {
            shareReturnFocus.focus();
          }
          shareReturnFocus = null;
        }

        function copyShareLink() {
          var text = buildShareLink();
          var didCopy = function() {
            if (!shareCopyButton) return;
            shareCopyButton.textContent = 'Copied';
            window.clearTimeout(shareCopyTimer);
            shareCopyTimer = window.setTimeout(function() {
              shareCopyButton.textContent = shareCopyDefaultLabel;
            }, 1400);
          };
          if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
            navigator.clipboard.writeText(text).then(didCopy).catch(function() {
              fallbackCopyText(text);
              didCopy();
            });
          } else {
            fallbackCopyText(text);
            didCopy();
          }
        }

        if (shareModal && shareDialog) {
          setShareAccess(shareAccessState);
          setShareRole(shareRoleState);
          if (shareCloseButton) shareCloseButton.addEventListener('click', closeShareModal);
          if (shareDoneButton) shareDoneButton.addEventListener('click', closeShareModal);
          if (shareCopyButton) shareCopyButton.addEventListener('click', copyShareLink);
          shareModal.addEventListener('click', function(e) {
            if (e.target === shareModal) closeShareModal();
          });
          shareModal.addEventListener('keydown', function(e) {
            var accessOpen = shareAccessMenu && shareAccessMenu.toggle.getAttribute('aria-expanded') === 'true';
            var roleOpen = shareRoleMenu && shareRoleMenu.toggle.getAttribute('aria-expanded') === 'true';
            if (e.key === 'Escape') {
              if (accessOpen || roleOpen) return;
              e.preventDefault();
              closeShareModal();
              return;
            }
            trapFocus(shareDialog, e);
          });
        }

        function cacheCurrentAudience() {
          var current = audienceStage.querySelector('.opctx-audience-stream-panel.is-current');
          if (!current) return;
          audienceCache[activeAudience] = {
            bodyHtml: current.innerHTML,
            titleHtml: audienceHeading ? audienceHeading.innerHTML : '',
            titleText: audienceHeading ? audienceHeading.textContent.trim() : '',
            subtitleHtml: audienceSubtitle ? audienceSubtitle.outerHTML : '',
            documentTitle: document.title,
          };
        }

        function applyAudienceMeta(snapshot) {
          if (audienceHeading && snapshot.titleHtml) {
            audienceHeading.innerHTML = snapshot.titleHtml;
          }
          if (audienceTocCurrent && audienceHeading) {
            audienceTocCurrent.textContent = audienceHeading.textContent.trim();
          }
          if (snapshot.documentTitle) {
            document.title = snapshot.documentTitle;
          }
          var existingSubtitle = document.querySelector('.opctx-article > .opctx-subtitle');
          if (snapshot.subtitleHtml) {
            if (existingSubtitle) existingSubtitle.outerHTML = snapshot.subtitleHtml;
            else if (audienceHeading) audienceHeading.insertAdjacentHTML('afterend', snapshot.subtitleHtml);
          } else if (existingSubtitle) {
            existingSubtitle.parentNode.removeChild(existingSubtitle);
          }
          audienceSubtitle = document.querySelector('.opctx-article > .opctx-subtitle');
        }

        function extractAudienceSnapshot(html) {
          var doc = new DOMParser().parseFromString(html, 'text/html');
          var article = doc.querySelector('article.opctx-article');
          if (!article) return null;
          var body = article.querySelector('.opctx-article-body');
          if (!body) return null;
          var heading = article.querySelector('h1');
          var subtitle = article.querySelector('.opctx-subtitle');
          return {
            bodyHtml: body.innerHTML,
            titleHtml: heading ? heading.innerHTML : '',
            titleText: heading ? heading.textContent.trim() : '',
            subtitleHtml: subtitle ? subtitle.outerHTML : '',
            documentTitle: doc.title || '',
          };
        }

        function loadAudience(key) {
          if (!audienceData.streams[key]) {
            return Promise.reject(new Error('unknown audience ' + key));
          }
          if (audienceCache[key]) {
            return Promise.resolve(audienceCache[key]);
          }
          return fetch(audienceData.streams[key].url, { credentials: 'same-origin' })
            .then(function(r) {
              if (!r.ok) throw new Error('HTTP ' + r.status);
              return r.text();
            })
            .then(function(html) {
              var snapshot = extractAudienceSnapshot(html);
              if (!snapshot) throw new Error('missing audience article body');
              audienceCache[key] = snapshot;
              return snapshot;
            });
        }

        function finishAudienceSwap(next, key) {
          var panels = audienceStage.querySelectorAll('.opctx-audience-stream-panel');
          for (var i = 0; i < panels.length; i++) {
            if (panels[i] !== next && panels[i].parentNode === audienceStage) {
              audienceStage.removeChild(panels[i]);
            }
          }
          next.classList.add('is-current');
          next.style.position = 'relative';
          next.style.inset = 'auto';
          next.style.pointerEvents = 'auto';
          next.style.zIndex = '1';
          audienceStage.style.minHeight = '';
          audienceStage.removeAttribute('data-transitioning');
          activeAudience = key;
          audienceBusy = false;
          window.__opctxAudienceStream.active = key;
        }

        function swapAudience(key, opts) {
          opts = opts || {};
          if (!audienceData.streams[key]) return;
          if (audienceBusy || key === activeAudience) {
            setAudienceSelection(activeAudience);
            return;
          }

          cacheCurrentAudience();
          audienceBusy = true;
          setAudienceSelection(key);
          persistAudience(key);

          var direction = audienceIndex(key) > audienceIndex(activeAudience) ? 1 : -1;
          loadAudience(key)
            .then(function(snapshot) {
              applyAudienceMeta(snapshot);
              var current = audienceStage.querySelector('.opctx-audience-stream-panel.is-current');
              if (!current || reduceMotion || opts.immediate || typeof current.animate !== 'function') {
                if (current) {
                  current.innerHTML = snapshot.bodyHtml;
                  current.setAttribute('data-audience-stream', key);
                }
                activeAudience = key;
                audienceBusy = false;
                audienceStage.dataset.activeAudience = key;
                window.__opctxAudienceStream.active = key;
                return;
              }

              audienceStage.dataset.activeAudience = key;
              audienceStage.setAttribute('data-transitioning', 'true');
              var next = document.createElement('div');
              next.className = 'opctx-article-body opctx-audience-stream-panel';
              next.setAttribute('data-audience-stream', key);
              next.innerHTML = snapshot.bodyHtml;
              next.style.position = 'absolute';
              next.style.inset = '0';
              next.style.pointerEvents = 'none';
              next.style.zIndex = '2';
              audienceStage.appendChild(next);
              audienceStage.style.minHeight =
                Math.max(current.offsetHeight, next.offsetHeight) + 'px';

              var enterFrom = direction > 0 ? 32 : -32;
              var exitTo = direction > 0 ? -32 : 32;
              var enterAnim = next.animate([
                { transform: 'translateX(' + enterFrom + 'px)', opacity: 0.2 },
                { transform: 'translateX(0)', opacity: 1 },
              ], {
                duration: 280,
                easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
                fill: 'forwards',
              });
              var exitAnim = current.animate([
                { transform: 'translateX(0)', opacity: 1 },
                { transform: 'translateX(' + exitTo + 'px)', opacity: 0.18 },
              ], {
                duration: 280,
                easing: 'cubic-bezier(0.22, 1, 0.36, 1)',
                fill: 'forwards',
              });

              var settled = false;
              function settle() {
                if (settled) return;
                settled = true;
                finishAudienceSwap(next, key);
              }
              Promise.allSettled([enterAnim.finished, exitAnim.finished]).then(settle);
              setTimeout(settle, 320);
            })
            .catch(function() {
              audienceBusy = false;
              setAudienceSelection(activeAudience);
            });
        }

        audienceMenu = wireMenu(audienceRoot, '.opctx-audience-switcher-toggle', '.opctx-audience-menu', {
          onSelect: function(item) {
            var targetAudience = item.getAttribute('data-audience-target');
            if (targetAudience) {
              swapAudience(targetAudience);
              window.setTimeout(function() {
                if (audienceMenu && audienceMenu.toggle) audienceMenu.toggle.focus();
              }, 0);
              return;
            }
            if (item.getAttribute('data-audience-action') === 'share') {
              openShareModal(audienceMenu ? audienceMenu.toggle : null);
            }
          },
        });

        window.__opctxAudienceStream = { active: activeAudience };
        cacheCurrentAudience();
        setAudienceSelection(activeAudience);
        var preferredAudience = readPreferredAudience();
        if (preferredAudience && preferredAudience !== activeAudience && audienceData.streams[preferredAudience]) {
          swapAudience(preferredAudience, { immediate: true });
        }
      }

      function escapeAttr(s) {
        return String(s == null ? '' : s)
          .replace(/&/g, '&amp;').replace(/</g, '&lt;')
          .replace(/>/g, '&gt;').replace(/"/g, '&quot;')
          .replace(/'/g, '&#39;');
      }
      function escapeText(s) {
        return String(s == null ? '' : s)
          .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
      }
    })();
  </script>${familyInfiniteScrollScript}
</body>
</html>
`;
}
