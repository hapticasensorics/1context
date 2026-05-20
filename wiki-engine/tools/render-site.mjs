// Render a complete user-wiki source tree into a staging site directory.
//
//   node tools/render-site.mjs \
//     --source-root <runtime/1Context/user-wiki/source> \
//     --output <staging/site> \
//     [--result-json <path>]
//
// This is the structured, whole-site wrapper around render-to-dir.mjs. It owns
// explicit roots and result JSON; render-to-dir.mjs remains the per-input
// renderer used by tests and older harnesses.

import {
  copyFileSync,
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  rmSync,
  statSync,
  writeFileSync,
} from 'node:fs';
import { basename, dirname, join, relative, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { fileURLToPath } from 'node:url';
import matter from 'gray-matter';

const __filename = fileURLToPath(import.meta.url);
const ENGINE_ROOT = resolve(dirname(__filename), '..');
const RENDER_TOOL = resolve(dirname(__filename), 'render-to-dir.mjs');

function usage() {
  console.error('Usage: render-site.mjs --source-root <user-wiki/source> --output <staging/site> [--result-json <path>]');
}

function parseArgs(argv) {
  const args = {};
  for (let i = 0; i < argv.length; i += 1) {
    const arg = argv[i];
    if (!arg.startsWith('--')) {
      throw new Error(`Unexpected argument: ${arg}`);
    }
    const key = arg.slice(2);
    const value = argv[i + 1];
    if (!value || value.startsWith('--')) {
      throw new Error(`Missing value for ${arg}`);
    }
    args[key] = value;
    i += 1;
  }
  return args;
}

function walkFiles(root) {
  if (!existsSync(root)) return [];
  const out = [];
  for (const entry of readdirSync(root)) {
    const full = join(root, entry);
    const stat = statSync(full);
    if (stat.isDirectory()) out.push(...walkFiles(full));
    else out.push(full);
  }
  return out;
}

function walkDirs(root) {
  if (!existsSync(root)) return [];
  const out = [root];
  for (const entry of readdirSync(root)) {
    const full = join(root, entry);
    if (statSync(full).isDirectory()) out.push(...walkDirs(full));
  }
  return out;
}

function sourceIsTombstoned(path) {
  return existsSync(path.replace(/\.md$/, '.tombstone.toml'));
}

function talkFolderIsTombstoned(path) {
  const folderName = path.split(/[\\/]/).pop() || '';
  const slug = folderName.endsWith('.talk') ? folderName.slice(0, -'.talk'.length) : folderName;
  const familyRoot = dirname(dirname(path));
  return existsSync(join(familyRoot, 'source', `${slug}.tombstone.toml`));
}

function copyThemeAssets(output) {
  const assetOut = join(output, 'assets');
  mkdirSync(assetOut, { recursive: true });
  const files = [
    ['theme/css/theme.css', 'theme.css'],
    ['theme/js/enhance.js', 'enhance.js'],
    ['theme/assets/favicon-32.png', 'favicon-32.png'],
    ['theme/assets/favicon-16.png', 'favicon-16.png'],
    ['theme/assets/apple-touch-icon.png', 'apple-touch-icon.png'],
    ['theme/assets/onecontext-icon-64.png', 'onecontext-icon-64.png'],
  ];
  for (const [source, dest] of files) {
    const sourcePath = join(ENGINE_ROOT, source);
    if (!existsSync(sourcePath)) {
      throw new Error(`Missing wiki theme asset: ${sourcePath}`);
    }
    copyFileSync(sourcePath, join(assetOut, dest));
  }
  return files.map(([, dest]) => `assets/${dest}`);
}

function collectPublishedAssets(output, seededAssets = []) {
  const assets = new Set(seededAssets);
  for (const file of walkFiles(output)) {
    const rel = posixRelative(output, file);
    if (rel.startsWith('.1context/')) continue;
    if (
      rel.startsWith('assets/')
      || rel.includes('.assets/')
      || rel.includes('/assets/')
    ) {
      assets.add(rel);
    }
  }
  return [...assets].sort();
}

function posixRelative(root, path) {
  return relative(root, path).split(/[\\/]/).join('/');
}

function sha256File(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function fileRecord(root, path, role, contentType) {
  const stat = statSync(path);
  return {
    role,
    path: posixRelative(root, path),
    sha256: sha256File(path),
    bytes: stat.size,
    content_type: contentType,
  };
}

function markdownTwinKind(markdownPath) {
  if (markdownPath.endsWith('.talk.md')) return 'talk';
  return 'page';
}

function pageIdFromTalkFor(talkFor) {
  if (typeof talkFor !== 'string') return null;
  const match = talkFor.match(/^page:\/\/(.+)$/);
  return match ? match[1] : null;
}

function routeForMarkdown(markdownPath) {
  if (markdownPath.endsWith('.talk.md')) {
    const base = markdownPath.slice(0, -'.talk.md'.length);
    return `/${base}/talk`;
  }
  return `/${markdownPath.slice(0, -'.md'.length)}`;
}

function normalizeManifestRoute(route) {
  const value = typeof route === 'string' && route.trim() ? route.trim() : '/';
  if (value === '/') return '/';
  return `/${value.replace(/^\/+/, '').replace(/\/+$/, '')}`;
}

function htmlForMarkdown(markdownPath) {
  if (markdownPath.endsWith('.talk.md')) {
    return `${markdownPath.slice(0, -'.talk.md'.length)}.talk.html`;
  }
  return `${markdownPath.slice(0, -'.md'.length)}.html`;
}

function routeIndexForMarkdown(markdownPath) {
  if (markdownPath.endsWith('.talk.md')) {
    const base = markdownPath.slice(0, -'.talk.md'.length);
    return `${base}/talk/index.html`;
  }
  return `${markdownPath.slice(0, -'.md'.length)}/index.html`;
}

function routeIndexForManifestRoute(route) {
  const clean = String(route || '').split(/[?#]/)[0].replace(/^\/+/, '').replace(/\/+$/, '');
  return clean ? `${clean}/index.html` : 'index.html';
}

function readFrontmatter(path) {
  try {
    return matter(readFileSync(path, 'utf8')).data || {};
  } catch {
    return {};
  }
}

function parseTomlStringArray(value) {
  const match = /^\[(.*)\]$/.exec(value.trim());
  if (!match) return [];
  return [...match[1].matchAll(/"([^"]+)"/g)].map((item) => item[1]);
}

function parseTomlValue(value) {
  const trimmed = value.trim();
  const stringMatch = /^"([^"]*)"$/.exec(trimmed);
  if (stringMatch) return stringMatch[1];
  if (trimmed === 'false') return false;
  if (trimmed === 'true') return true;
  if (/^-?\d+$/.test(trimmed)) return Number(trimmed);
  if (/^\[.*\]$/.test(trimmed)) return parseTomlStringArray(trimmed);
  return trimmed;
}

function parseWikiConfig(text) {
  const config = {
    title: null,
    defaults: {},
    site: { home_feed: {} },
    navigation: [],
    primary_navigation: [],
    utility_navigation: [],
    site_pages: [],
    pages: [],
  };
  let currentTarget = config;
  for (const rawLine of text.split(/\r?\n/)) {
    const line = rawLine.replace(/\s+#.*$/, '').trim();
    if (!line) continue;
    if (line === '[defaults]') {
      currentTarget = config.defaults;
      continue;
    }
    if (line === '[site]') {
      currentTarget = config.site;
      continue;
    }
    if (line === '[site.home_feed]') {
      config.site.home_feed ||= {};
      currentTarget = config.site.home_feed;
      continue;
    }
    if (line === '[[site_pages]]') {
      currentTarget = {};
      config.site_pages.push(currentTarget);
      continue;
    }
    if (line === '[[pages]]') {
      currentTarget = {};
      config.pages.push(currentTarget);
      continue;
    }
    if (/^\[.+\]$/.test(line)) {
      currentTarget = null;
      continue;
    }
    const match = /^([A-Za-z0-9_]+)\s*=\s*(.+)$/.exec(line);
    if (!match) continue;
    const [, key, rawValue] = match;
    if (!currentTarget) continue;
    if (
      (currentTarget === config || currentTarget === config.site)
      && ['navigation', 'primary_navigation', 'utility_navigation'].includes(key)
    ) {
      config[key] = parseTomlStringArray(rawValue);
      if (currentTarget === config.site) currentTarget[key] = config[key];
      continue;
    }
    currentTarget[key] = parseTomlValue(rawValue);
  }
  return config;
}

function readWikiConfig(sourceRoot) {
  const wikiConfig = resolve(sourceRoot, '..', 'wiki.toml');
  if (!existsSync(wikiConfig)) {
    return {
      title: null,
      defaults: {},
      site: { home_feed: {} },
      navigation: [],
      primary_navigation: [],
      utility_navigation: [],
      site_pages: [],
      pages: [],
    };
  }
  return parseWikiConfig(readFileSync(wikiConfig, 'utf8'));
}

function titleCase(value) {
  return String(value || '')
    .split(/[-_\s]+/)
    .filter(Boolean)
    .map((part) => part.charAt(0).toUpperCase() + part.slice(1))
    .join(' ');
}

function buildSiteNavigation(sourceRoot, config) {
  const byId = new Map();
  for (const page of config.pages) {
    if (page.id && pageRenderableForMenu(sourceRoot, page)) {
      byId.set(page.id, page);
    }
  }
  for (const page of config.site_pages || []) {
    if (!page.id || page.enabled === false || byId.has(page.id)) continue;
    byId.set(page.id, { ...page, __site_page: true });
  }
  const baseNavigation = config.primary_navigation.length ? config.primary_navigation : config.navigation;
  const orderedPagesFor = (ids, defaultGroupLabel) => {
    const orderedIds = [...new Set(ids)].filter((id) => byId.has(id));
    const indexById = new Map(orderedIds.map((id, index) => [id, index]));
    return orderedIds
      .map((id) => ({
        ...byId.get(id),
        __menu_group_label: byId.get(id).__site_page ? defaultGroupLabel : null,
      }))
      .sort((left, right) => {
        const leftOrder = Number.isFinite(left.nav_order) ? left.nav_order : indexById.get(left.id);
        const rightOrder = Number.isFinite(right.nav_order) ? right.nav_order : indexById.get(right.id);
        if (leftOrder !== rightOrder) return leftOrder - rightOrder;
        return (indexById.get(left.id) ?? 0) - (indexById.get(right.id) ?? 0);
      });
  };
  const primaryPages = orderedPagesFor(baseNavigation, 'Site');
  const utilityPages = orderedPagesFor(config.utility_navigation, 'Utility');
  const groups = [];
  const appendPages = (pages) => {
    let currentGroup = null;
    let currentGroupLabel = null;
    for (const page of pages) {
      const label = page.__menu_group_label || page.family_group_title || titleCase(page.family_group || 'Pages');
      if (!currentGroup || currentGroupLabel !== label) {
        currentGroup = { label, items: [] };
        currentGroupLabel = label;
        groups.push(currentGroup);
      }
      currentGroup.items.push({
        href: page.route || `/${page.slug || page.id}`,
        label: page.title || titleCase(page.id),
        sub: page.summary || '',
      });
    }
  };
  appendPages(primaryPages);
  appendPages(utilityPages);
  return { groups };
}

function pageSourcePath(sourceRoot, page) {
  const slug = page.slug || page.id;
  const familyGroup = page.family_group || 'pages';
  const familyId = page.family_id || slug;
  return join(sourceRoot, 'families', familyGroup, familyId, 'source', `${slug}.md`);
}

function pageTalkPath(sourceRoot, page) {
  const slug = page.slug || page.id;
  const familyGroup = page.family_group || 'pages';
  const familyId = page.family_id || slug;
  return join(sourceRoot, 'families', familyGroup, familyId, 'talk', `${slug}.talk`);
}

function pageRenderableForMenu(sourceRoot, page) {
  if (page.enabled === false) return false;
  const source = pageSourcePath(sourceRoot, page);
  return existsSync(source) && !existsSync(source.replace(/\.md$/, '.tombstone.toml'));
}

function routeStem(route, fallback) {
  const value = typeof route === 'string' && route.trim() ? route.trim() : fallback;
  const clean = String(value || '').split(/[?#]/)[0].replace(/^\/+/, '').replace(/\/+$/, '');
  return clean || 'index';
}

function generatedSitePageSlug(page, route) {
  const declared = typeof page.slug === 'string' && page.slug.trim()
    ? page.slug.trim()
    : typeof page.id === 'string' && page.id.trim()
      ? page.id.trim()
      : '';
  const fallback = basename(routeStem(route, 'index')) || 'index';
  return routeStem(declared || fallback, fallback).split('/').pop() || fallback;
}

function replaceTemplateVars(text, vars) {
  return text.replace(/\{\{\s*([A-Za-z0-9_]+)\s*\}\}/g, (_match, key) => {
    const value = vars[key];
    return value === undefined || value === null ? '' : String(value);
  });
}

function markGeneratedSitePageInput(text) {
  if (!text.startsWith('---')) return text;
  const frontmatterEnd = text.indexOf('\n---', 3);
  if (frontmatterEnd < 0 || /^source_kind:/m.test(text.slice(0, frontmatterEnd))) {
    return text;
  }
  return `${text.slice(0, frontmatterEnd)}\nsource_kind: generated_site_page${text.slice(frontmatterEnd)}`;
}

function readJsonLines(path) {
  if (!existsSync(path)) return [];
  return readFileSync(path, 'utf8')
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      try {
        return JSON.parse(line);
      } catch {
        return null;
      }
    })
    .filter(Boolean);
}

function pageRouteMap(config) {
  const routes = new Map();
  for (const page of config.pages || []) {
    if (!page.id) continue;
    routes.set(page.id, page.route || `/${page.slug || page.id}`);
  }
  for (const page of config.site_pages || []) {
    if (!page.id || routes.has(page.id)) continue;
    routes.set(page.id, page.route || `/${page.slug || page.id}`);
  }
  return routes;
}

function humanEvent(value) {
  return String(value || 'changed')
    .replace(/^page\./, '')
    .replace(/\./g, ' ')
    .replace(/_/g, ' ');
}

function activityLine({ at, label, href, detail }) {
  const when = at ? `**${String(at).slice(0, 19).replace('T', ' ')}** - ` : '';
  const target = href ? `[${label}](${href})` : label;
  return `- ${when}${target}${detail ? ` - ${detail}` : ''}`;
}

function collectPageLedgerActivity(userWikiRoot, config) {
  const routes = pageRouteMap(config);
  return readJsonLines(join(userWikiRoot, '.1context', 'page-ledger.jsonl')).map((event) => ({
    at: event.at,
    label: event.page || 'wiki',
    href: routes.get(event.page),
    detail: humanEvent(event.event),
  }));
}

function collectRenderActivity(userWikiRoot) {
  return readJsonLines(join(userWikiRoot, 'site', '.1context', 'render-events.jsonl')).map((event) => ({
    at: event.published_at || event.rendered_at || event.at,
    label: 'render',
    href: '/',
    detail: [event.status, event.trigger].filter(Boolean).join(' via '),
  }));
}

function collectLinkDiagnosticActivity(userWikiRoot) {
  const path = join(userWikiRoot, 'site', '.1context', 'link-diagnostics.json');
  if (!existsSync(path)) return [];
  let diagnostics;
  try {
    diagnostics = JSON.parse(readFileSync(path, 'utf8'));
  } catch {
    return [];
  }
  const issueCount = diagnostics.issue_count || diagnostics.issues?.length || 0;
  if (!issueCount) return [];
  return [{
    at: diagnostics.generated_at || diagnostics.checked_at,
    label: 'link diagnostics',
    href: '/',
    detail: `${issueCount} issue${issueCount === 1 ? '' : 's'} need repair`,
  }];
}

function buildActivityFeed(sourceRoot, config) {
  const homeFeed = config.site?.home_feed || {};
  if (homeFeed.enabled === false) {
    return '- Feed disabled by site configuration.';
  }
  const userWikiRoot = resolve(sourceRoot, '..');
  const sources = Array.isArray(homeFeed.sources)
    ? homeFeed.sources
    : ['page_ledger', 'render_events', 'link_diagnostics'];
  const maxItems = Number.isFinite(homeFeed.max_items) ? homeFeed.max_items : 12;
  let items = [];
  if (sources.includes('page_ledger')) {
    items.push(...collectPageLedgerActivity(userWikiRoot, config));
  }
  if (sources.includes('render_events')) {
    items.push(...collectRenderActivity(userWikiRoot));
  }
  if (sources.includes('link_diagnostics')) {
    items.push(...collectLinkDiagnosticActivity(userWikiRoot));
  }
  items = items
    .filter((item) => item.label || item.detail)
    .sort((left, right) => String(right.at || '').localeCompare(String(left.at || '')))
    .slice(0, Math.max(1, maxItems));
  if (!items.length) {
    return '- No recorded wiki changes yet.';
  }
  return items.map(activityLine).join('\n');
}

function generateSitePageInputs(output, sourceRoot, config, generatedAt) {
  const templatesRoot = resolve(sourceRoot, '..', 'templates');
  const generatedDir = join(output, '.1context', 'generated-site-inputs');
  const defaults = config.defaults || {};
  const activityFeed = buildActivityFeed(sourceRoot, config);
  const inputs = [];
  for (const page of config.site_pages || []) {
    if (page.enabled === false || !page.template) continue;
    const templatePath = join(templatesRoot, ...String(page.template).split('/'));
    if (!existsSync(templatePath)) {
      throw new Error(`Configured site page template does not exist: ${templatePath}`);
    }
    const route = page.route || `/${page.id}`;
    const routeOutputStem = routeStem(route, page.id || 'index');
    const slug = generatedSitePageSlug(page, route);
    const vars = {
      ...defaults,
      page_id: page.id || slug,
      slug,
      route_stem: routeOutputStem,
      route,
      title: page.title || titleCase(page.id || slug),
      wiki_title: config.title || defaults.wiki_title || page.title || '1Context',
      wiki_summary: page.summary || defaults.wiki_summary || '',
      wiki_tagline: defaults.wiki_tagline || page.summary || '',
      activity_feed: activityFeed,
      created_date: generatedAt.slice(0, 10),
      access_tier: defaults.access_tier || 'private',
      asset_base: defaults.asset_base || '.',
      home_href: defaults.home_href || '/',
    };
    mkdirSync(generatedDir, { recursive: true });
    const input = join(generatedDir, `${slug}.md`);
    writeFileSync(
      input,
      markGeneratedSitePageInput(replaceTemplateVars(readFileSync(templatePath, 'utf8'), vars))
    );
    inputs.push({
      input,
      routeOverride: { route, slug: routeOutputStem },
    });
  }
  return inputs;
}

function talkRouteForPage(page) {
  const route = page.route || `/${page.slug || page.id}`;
  if (route === '/') return '/talk';
  return `${route.replace(/\/+$/, '')}/talk`;
}

function routeOverrideForInput(sourceRoot, config, input) {
  const absoluteInput = resolve(input);
  for (const page of config.pages) {
    if (resolve(pageSourcePath(sourceRoot, page)) === absoluteInput) {
      return {
        route: page.route || `/${page.slug || page.id}`,
        slug: page.slug || page.id,
      };
    }
    if (resolve(pageTalkPath(sourceRoot, page)) === absoluteInput) {
      return {
        route: page.route || `/${page.slug || page.id}`,
        talk_route: talkRouteForPage(page),
        slug: `${page.slug || page.id}.talk`,
      };
    }
  }
  return null;
}

function buildSiteMetadata(output, generatedAt, assets) {
  const metadataDir = join(output, '.1context');
  mkdirSync(metadataDir, { recursive: true });

  const markdownFiles = walkFiles(output)
    .filter((path) => path.endsWith('.md'))
    .filter((path) => !posixRelative(output, path).startsWith('.1context/'))
    .sort();

  const markdownTwins = [];
  const routes = [];

  for (const markdownFile of markdownFiles) {
    const markdownPath = posixRelative(output, markdownFile);
    const frontmatter = readFrontmatter(markdownFile);
    const htmlPath = htmlForMarkdown(markdownPath);
    const htmlFile = join(output, ...htmlPath.split('/'));
    const routeIndexPath = routeIndexForMarkdown(markdownPath);
    const routeIndexFile = routeIndexPath ? join(output, ...routeIndexPath.split('/')) : null;
    const hasHtml = existsSync(htmlFile);
    const hasRouteIndex = routeIndexFile ? existsSync(routeIndexFile) : false;
    const kind = markdownTwinKind(markdownPath);
    const frontmatterRoute = kind === 'talk' ? (frontmatter.talk_route || frontmatter.route) : frontmatter.route;
    const route = hasHtml ? normalizeManifestRoute(frontmatterRoute || routeForMarkdown(markdownPath)) : null;
    const manifestRouteIndexPath = route
      ? (() => {
          const routeDerivedPath = routeIndexForManifestRoute(route);
          const routeDerivedFile = join(output, ...routeDerivedPath.split('/'));
          if (existsSync(routeDerivedFile)) return routeDerivedPath;
          return route === '/' ? htmlPath : (hasRouteIndex ? routeIndexPath : null);
        })()
      : null;
    const sourceKind = frontmatter.source_kind || (kind === 'talk' ? 'talk_page' : 'source_page');
    const pageId = frontmatter.page_id || pageIdFromTalkFor(frontmatter.talk_for);

    const twin = {
      ...fileRecord(output, markdownFile, `${kind}-markdown`, 'text/markdown; charset=utf-8'),
      kind,
      source_kind: sourceKind,
      page_id: pageId || null,
      talk_for: frontmatter.talk_for || null,
      route,
      html_path: hasHtml ? htmlPath : null,
      route_index_path: hasHtml ? manifestRouteIndexPath : null,
      slug: frontmatter.slug || markdownPath.replace(/\.md$/, ''),
      title: frontmatter.title || null,
      access: frontmatter.access || frontmatter.talk_audience || 'public',
      status: frontmatter.status || 'published',
      md_url: frontmatter.md_url || `/${markdownPath}`,
      talk_enabled: frontmatter.talk_enabled !== false,
      talk_url: frontmatter.talk_url || null,
    };
    markdownTwins.push(twin);

    if (hasHtml) {
      routes.push({
        route,
        kind,
        source_kind: sourceKind,
        page_id: pageId || null,
        talk_for: frontmatter.talk_for || null,
        slug: twin.slug,
        title: twin.title,
        access: twin.access,
        status: twin.status,
        html_path: htmlPath,
        route_index_path: manifestRouteIndexPath,
        markdown_path: markdownPath,
      });
    }
  }

  const routeManifest = {
    schema_version: 'wiki.route-manifest.v1',
    generated_at: generatedAt,
    output: 'site://.',
    route_count: routes.length,
    routes,
    assets,
  };
  const contentIndex = {
    schema_version: 'wiki.content-index.v1',
    generated_at: generatedAt,
    output: 'site://.',
    page_count: routes.filter((entry) => entry.kind === 'page').length,
    talk_count: routes.filter((entry) => entry.kind === 'talk').length,
    markdown_twin_count: markdownTwins.length,
    pages: routes,
    markdown_twins: markdownTwins,
    export_allowlist: [
      '*.html',
      '*.md',
      '*/index.html',
      'assets/*',
      '*.assets/*',
      '*/*.assets/*',
      '.1context/current-render.json',
      '.1context/render-events.jsonl',
      '.1context/route-manifest.json',
      '.1context/content-index.json',
    ],
  };

  writeFileSync(join(metadataDir, 'route-manifest.json'), JSON.stringify(routeManifest, null, 2) + '\n');
  writeFileSync(join(metadataDir, 'content-index.json'), JSON.stringify(contentIndex, null, 2) + '\n');

  return {
    routeManifestPath: '.1context/route-manifest.json',
    contentIndexPath: '.1context/content-index.json',
    routeCount: routes.length,
    markdownTwinCount: markdownTwins.length,
  };
}

function addValidTarget(targets, value) {
  if (!value) return;
  let target = value.startsWith('/') ? value : `/${value}`;
  target = target.replace(/\/+$/, '') || '/';
  targets.add(target);
  if (target !== '/') targets.add(`${target}/`);
  if (target.endsWith('/index.html')) {
    const route = target.slice(0, -'/index.html'.length) || '/';
    targets.add(route);
    if (route !== '/') targets.add(`${route}/`);
  }
}

function validInternalTargets(routeManifest) {
  const targets = new Set(['/']);
  for (const route of routeManifest.routes || []) {
    for (const key of ['route', 'html_path', 'route_index_path', 'markdown_path']) {
      if (typeof route[key] === 'string') addValidTarget(targets, route[key]);
    }
  }
  return targets;
}

function linkSourceLookup(routeManifest) {
  const lookup = new Map();
  for (const route of routeManifest.routes || []) {
    const info = {
      page_id: route.page_id || route.slug || null,
      route: route.route || null,
      markdown_path: route.markdown_path || null,
      route_index_path: route.route_index_path || null,
    };
    for (const key of ['html_path', 'route_index_path']) {
      if (typeof route[key] === 'string' && route[key]) {
        lookup.set(route[key], info);
      }
    }
  }
  return lookup;
}

function decodeBasicHtmlEntities(value) {
  return value
    .replace(/&amp;/g, '&')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'");
}

function extractHtmlAttrValues(text, attr) {
  const values = [];
  const needle = `${attr}=`;
  let rest = text;
  while (true) {
    const index = rest.indexOf(needle);
    if (index < 0) break;
    rest = rest.slice(index + needle.length);
    const quote = rest[0];
    if (quote !== '"' && quote !== "'") continue;
    rest = rest.slice(1);
    const end = rest.indexOf(quote);
    if (end < 0) break;
    values.push(decodeBasicHtmlEntities(rest.slice(0, end)));
    rest = rest.slice(end + 1);
  }
  return values;
}

function baseHrefFromHtml(text) {
  const match = /<base\b[^>]*\bhref=(["'])(.*?)\1/i.exec(text);
  if (!match) return null;
  const value = decodeBasicHtmlEntities(match[2]).trim();
  if (!value || /^https?:\/\//i.test(value)) return null;
  return value.startsWith('/') ? value : `/${value}`;
}

function normalizePosixSegments(path) {
  const parts = [];
  for (const part of path.split('/')) {
    if (!part || part === '.') continue;
    if (part === '..') {
      if (!parts.length) return null;
      parts.pop();
      continue;
    }
    parts.push(part);
  }
  return `/${parts.join('/')}`;
}

function normalizeRelativeRoute(sourcePath, href) {
  const slash = sourcePath.lastIndexOf('/');
  const base = slash >= 0 ? sourcePath.slice(0, slash) : '';
  return normalizePosixSegments(base ? `${base}/${href}` : href);
}

function normalizeRelativeHref(baseHref, href) {
  try {
    const resolved = new URL(href, `https://wiki.local${baseHref}`);
    return normalizePosixSegments(decodeURIComponent(resolved.pathname).replace(/^\/+/, ''));
  } catch {
    return null;
  }
}

function normalizeInternalHref(href, sourcePath, baseHref = null) {
  const trimmed = String(href || '').trim();
  if (
    !trimmed ||
    trimmed.startsWith('#') ||
    /^https?:\/\//i.test(trimmed) ||
    /^(mailto|tel|data|javascript):/i.test(trimmed)
  ) {
    return null;
  }
  const withoutFragment = trimmed.split('#')[0] || trimmed;
  const withoutQuery = withoutFragment.split('?')[0] || withoutFragment;
  const normalized = withoutQuery.startsWith('/')
    ? normalizePosixSegments(withoutQuery.replace(/^\/+/, ''))
    : baseHref
      ? normalizeRelativeHref(baseHref, withoutQuery)
      : normalizeRelativeRoute(sourcePath, withoutQuery);
  if (!normalized) return null;
  return normalized === '/' ? '/' : normalized.replace(/\/+$/, '');
}

function ignoredInternalHref(target) {
  return (
    target.startsWith('/assets/') ||
    target.startsWith('/api/') ||
    target.startsWith('/.1context/') ||
    target === '/favicon.ico'
  );
}

function outputFileTargetExists(output, target) {
  const relativeTarget = target.replace(/^\/+/, '');
  if (
    !relativeTarget ||
    relativeTarget.split('/').some((segment) => !segment || segment === '.' || segment === '..')
  ) {
    return false;
  }
  return existsSync(join(output, ...relativeTarget.split('/')));
}

function collectCanonicalHtmlFiles(output) {
  return walkFiles(output)
    .filter((path) => path.endsWith('.html'))
    .filter((path) => basename(path) !== 'index.html' || dirname(path) === output)
    .sort();
}

function internalLinkDiagnostics(output) {
  const manifestPath = join(output, '.1context', 'route-manifest.json');
  if (!existsSync(manifestPath)) {
    return {
      status: 'warning',
      issue_count: 1,
      broken_internal_count: 0,
      issues: [{
        code: 'route_manifest_missing',
        severity: 'warning',
        message: 'route manifest missing; internal links were not validated',
      }],
    };
  }

  const routeManifest = JSON.parse(readFileSync(manifestPath, 'utf8'));
  const validTargets = validInternalTargets(routeManifest);
  const sourceLookup = linkSourceLookup(routeManifest);
  const manifestDisplay = posixRelative(output, manifestPath);
  const seen = new Set();
  const issues = [];

  for (const htmlFile of collectCanonicalHtmlFiles(output)) {
    const text = readFileSync(htmlFile, 'utf8');
    const sourcePath = posixRelative(output, htmlFile);
    const baseHref = baseHrefFromHtml(text);
    for (const href of extractHtmlAttrValues(text, 'href')) {
      const target = normalizeInternalHref(href, sourcePath, baseHref);
      if (
        !target ||
        validTargets.has(target) ||
        outputFileTargetExists(output, target) ||
        ignoredInternalHref(target)
      ) {
        continue;
      }
      const key = `${sourcePath}\0${href}\0${target}`;
      if (seen.has(key)) continue;
      seen.add(key);
      const source = sourceLookup.get(sourcePath) || {};
      issues.push({
        code: 'broken_internal_link',
        severity: 'warning',
        phase: 'post_render_link_check',
        source_path: sourcePath,
        page_id: source.page_id || null,
        route: source.route || null,
        markdown_path: source.markdown_path || null,
        route_index_path: source.route_index_path || null,
        href,
        target,
        manifest_path: manifestDisplay,
        suggested_actions: ['edit_source', 'replace_link', 'publish'],
        message: 'internal link target is not present in the route manifest',
      });
    }
  }

  const brokenInternalCount = issues.filter((issue) => issue.code === 'broken_internal_link').length;
  return {
    status: issues.length ? 'warning' : 'ok',
    issue_count: issues.length,
    broken_internal_count: brokenInternalCount,
    issues,
  };
}

function escapeHtml(value) {
  return String(value)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;');
}

function annotateAnchorTag(tag) {
  let annotated;
  if (tag.includes('class="')) {
    annotated = tag.replace('class="', 'class="opctx-broken-link ');
  } else if (tag.includes("class='")) {
    annotated = tag.replace("class='", "class='opctx-broken-link ");
  } else {
    annotated = `${tag} class="opctx-broken-link"`;
  }
  if (!annotated.includes('data-1context-link-state=')) {
    annotated += ' data-1context-link-state="broken"';
  }
  if (!annotated.includes('title=')) {
    annotated += ' title="Broken internal link"';
  }
  if (!annotated.includes('aria-label=')) {
    annotated += ' aria-label="Broken internal link"';
  }
  return annotated;
}

function anchorTagHasHref(tag, href) {
  return tag.includes(`href="${href}"`) || tag.includes(`href='${href}'`);
}

function annotateAnchorHref(html, href) {
  let out = '';
  let rest = html;
  while (true) {
    const start = rest.indexOf('<a ');
    if (start < 0) break;
    out += rest.slice(0, start);
    const anchor = rest.slice(start);
    const end = anchor.indexOf('>');
    if (end < 0) {
      out += anchor;
      rest = '';
      break;
    }
    const tag = anchor.slice(0, end);
    out += anchorTagHasHref(tag, href) && !tag.includes('data-1context-link-state="broken"')
      ? annotateAnchorTag(tag)
      : tag;
    out += '>';
    rest = anchor.slice(end + 1);
  }
  return out + rest;
}

function annotateBrokenAnchor(html, href) {
  const hrefs = new Set([href, escapeHtml(href)]);
  let out = html;
  for (const candidate of hrefs) {
    out = annotateAnchorHref(out, candidate);
  }
  return out;
}

function insertLinkWarningBanner(html, issues) {
  if (html.includes('data-1context-link-diagnostics="broken-internal-links"')) {
    return html;
  }
  const targets = [...new Set(
    issues.map((issue) => issue.target).filter((target) => typeof target === 'string' && target)
  )].sort();
  const targetList = targets.length
    ? `<ul>${targets.map((target) => `<li><code>${escapeHtml(target)}</code></li>`).join('')}</ul>`
    : '';
  const noun = issues.length === 1 ? 'link' : 'links';
  const verb = issues.length === 1 ? 'points' : 'point';
  const object = issues.length === 1 ? 'a missing page' : 'missing pages';
  const banner = `
        <aside class="opctx-link-warning" role="note" data-1context-link-diagnostics="broken-internal-links">
          <strong>Broken internal ${noun}</strong>
          <p>${issues.length} internal ${noun} ${verb} to ${object}.</p>
          ${targetList}
        </aside>`;
  const bodyIndex = html.indexOf('<div class="opctx-article-body');
  if (bodyIndex >= 0) {
    const close = html.indexOf('>', bodyIndex);
    if (close >= 0) {
      return `${html.slice(0, close + 1)}${banner}${html.slice(close + 1)}`;
    }
  }
  return html.replace('<article class="opctx-article">', `<article class="opctx-article">${banner}`);
}

function annotateLinkDiagnostics(output, diagnostics) {
  const metadata = join(output, '.1context');
  mkdirSync(metadata, { recursive: true });
  writeFileSync(
    join(metadata, 'link-diagnostics.json'),
    JSON.stringify(diagnostics, null, 2) + '\n'
  );

  const bySource = new Map();
  for (const issue of diagnostics.issues || []) {
    if (issue.code !== 'broken_internal_link') continue;
    for (const key of ['source_path', 'route_index_path']) {
      if (!issue[key]) continue;
      if (!bySource.has(issue[key])) bySource.set(issue[key], []);
      bySource.get(issue[key]).push(issue);
    }
  }

  for (const [sourcePath, sourceIssues] of bySource) {
    const htmlPath = join(output, ...sourcePath.split('/'));
    if (!existsSync(htmlPath) || !statSync(htmlPath).isFile()) continue;
    let html = readFileSync(htmlPath, 'utf8');
    for (const issue of sourceIssues) {
      if (issue.href) html = annotateBrokenAnchor(html, issue.href);
    }
    html = insertLinkWarningBanner(html, sourceIssues);
    writeFileSync(htmlPath, html);
  }
}

function linkHealthFromDiagnostics(diagnostics) {
  const brokenIssues = (diagnostics.issues || [])
    .filter((issue) => issue.code === 'broken_internal_link');
  const pages = [...new Set(
    brokenIssues.map((issue) => issue.page_id || issue.route || issue.source_path).filter(Boolean)
  )].sort();
  const targets = [...new Set(
    brokenIssues.map((issue) => issue.target).filter(Boolean)
  )].sort();
  return {
    status: diagnostics.status || 'unknown',
    broken_internal_count: brokenIssues.length,
    pages_with_broken_links: pages,
    broken_internal_targets: targets,
    next_action: brokenIssues.length ? 'repair_links' : 'none',
  };
}

function annotateRouteManifestLinkDiagnostics(output, diagnostics) {
  const path = join(output, '.1context', 'route-manifest.json');
  if (!existsSync(path)) return;
  const manifest = JSON.parse(readFileSync(path, 'utf8'));
  manifest.link_diagnostics = {
    path: '.1context/link-diagnostics.json',
    status: diagnostics.status || 'unknown',
    issue_count: diagnostics.issue_count || 0,
    health: linkHealthFromDiagnostics(diagnostics),
  };
  writeFileSync(path, JSON.stringify(manifest, null, 2) + '\n');
}

function annotateRenderedLinks(output) {
  const diagnostics = internalLinkDiagnostics(output);
  annotateLinkDiagnostics(output, diagnostics);
  annotateRouteManifestLinkDiagnostics(output, diagnostics);
  return diagnostics;
}

function renderInput(input, output, siteNavigation, routeOverride) {
  const env = {
    ...process.env,
    ONECONTEXT_WIKI_SITE_NAV_JSON: siteNavigation ? JSON.stringify(siteNavigation) : '',
    ONECONTEXT_WIKI_ROUTE_JSON: routeOverride ? JSON.stringify(routeOverride) : '',
  };
  const result = spawnSync(process.execPath, [RENDER_TOOL, input, output], {
    cwd: ENGINE_ROOT,
    env,
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    const detail = [result.stderr, result.stdout]
      .map((part) => String(part || '').trim())
      .filter(Boolean)
      .join('\n');
    throw new Error(`Render failed for ${input}${detail ? `\n${detail}` : ''}`);
  }
  return String(result.stdout || '').trim();
}

function writeResult(path, result) {
  if (!path) return;
  mkdirSync(dirname(path), { recursive: true });
  writeFileSync(path, JSON.stringify(result, null, 2) + '\n');
}

function isoNow() {
  return new Date().toISOString();
}

function main() {
  let args;
  try {
    args = parseArgs(process.argv.slice(2));
  } catch (error) {
    usage();
    console.error(error.message);
    process.exit(2);
  }

  const sourceRoot = args['source-root'] ? resolve(args['source-root']) : null;
  const output = args.output ? resolve(args.output) : null;
  const resultJson = args['result-json'] ? resolve(args['result-json']) : null;
  if (!sourceRoot || !output) {
    usage();
    process.exit(2);
  }
  if (!existsSync(sourceRoot) || !statSync(sourceRoot).isDirectory()) {
    const result = {
      schema_version: 1,
      status: 'failed',
      rendered_at: isoNow(),
      source_root: sourceRoot,
      output,
      error: `Source root does not exist: ${sourceRoot}`,
    };
    writeResult(resultJson, result);
    console.error(result.error);
    process.exit(1);
  }

  const startedAt = isoNow();
  const families = join(sourceRoot, 'families');
  const wikiConfig = readWikiConfig(sourceRoot);
  const siteNavigation = buildSiteNavigation(sourceRoot, wikiConfig);
  const sourceInputs = walkFiles(families)
    .filter((path) => /\/source\/families\/[^/]+\/[^/]+\/source\/[^/]+\.md$/.test(path))
    .filter((path) => !path.endsWith('.tombstone.md'))
    .filter((path) => !sourceIsTombstoned(path))
    .sort();
  const talkInputs = walkDirs(families)
    .filter((path) => path.endsWith('.talk') && existsSync(join(path, '_meta.yaml')))
    .filter((path) => !talkFolderIsTombstoned(path))
    .sort();
  let siteInputs = [];

  try {
    if (sourceInputs.length === 0 && talkInputs.length === 0) {
      throw new Error(`No wiki source pages or talk folders found under ${families}`);
    }
    rmSync(output, { recursive: true, force: true });
    mkdirSync(output, { recursive: true });
    const logs = [];
    siteInputs = generateSitePageInputs(output, sourceRoot, wikiConfig, startedAt);
    for (const siteInput of siteInputs) {
      logs.push({
        input: siteInput.input,
        route_override: siteInput.routeOverride,
        summary: renderInput(siteInput.input, output, siteNavigation, siteInput.routeOverride),
      });
    }
    for (const input of [...sourceInputs, ...talkInputs]) {
      const routeOverride = routeOverrideForInput(sourceRoot, wikiConfig, input);
      logs.push({
        input,
        route_override: routeOverride,
        summary: renderInput(input, output, siteNavigation, routeOverride),
      });
    }
    const themeAssets = copyThemeAssets(output);
    const assets = collectPublishedAssets(output, themeAssets);
    const metadata = buildSiteMetadata(output, startedAt, assets);
    const linkDiagnostics = annotateRenderedLinks(output);
    const result = {
      schema_version: 1,
      status: 'published',
      rendered_at: startedAt,
      source_root: sourceRoot,
      output,
      route_manifest: metadata.routeManifestPath,
      content_index: metadata.contentIndexPath,
      route_count: metadata.routeCount,
      markdown_twin_count: metadata.markdownTwinCount,
      source_inputs: sourceInputs,
      site_inputs: siteInputs.map((entry) => entry.input),
      talk_inputs: talkInputs,
      site_input_count: siteInputs.length,
      source_input_count: sourceInputs.length,
      talk_input_count: talkInputs.length,
      assets,
      link_diagnostics: linkDiagnostics,
      logs,
    };
    writeResult(resultJson, result);
    console.log(JSON.stringify(result, null, 2));
  } catch (error) {
    const result = {
      schema_version: 1,
      status: 'failed',
      rendered_at: startedAt,
      source_root: sourceRoot,
      output,
      source_inputs: sourceInputs,
      site_inputs: siteInputs.map((entry) => entry.input),
      talk_inputs: talkInputs,
      site_input_count: siteInputs.length,
      source_input_count: sourceInputs.length,
      talk_input_count: talkInputs.length,
      error: error.message,
    };
    writeResult(resultJson, result);
    console.error(error.message);
    process.exit(1);
  }
}

main();
