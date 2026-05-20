#!/usr/bin/env node
import { createServer } from 'node:http';
import { createReadStream, existsSync, readFileSync, statSync, writeFileSync } from 'node:fs';
import { extname, resolve, join, normalize } from 'node:path';

const siteDir = process.argv[2] ? resolve(process.argv[2]) : null;
if (!siteDir) {
  console.error('Usage: serve-site.mjs <site-dir>');
  process.exit(2);
}
if (!existsSync(siteDir) || !statSync(siteDir).isDirectory()) {
  console.error(`Site directory does not exist: ${siteDir}`);
  process.exit(1);
}

const host = process.env.HOST || '127.0.0.1';
const port = Number(process.env.PORT || 0);
const portFile = process.env.PORT_FILE || '';

const types = {
  '.html': 'text/html; charset=utf-8',
  '.md': 'text/markdown; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.txt': 'text/plain; charset=utf-8',
  '.eml': 'text/plain; charset=utf-8',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.jpeg': 'image/jpeg',
  '.gif': 'image/gif',
  '.webp': 'image/webp',
  '.avif': 'image/avif',
  '.svg': 'image/svg+xml',
};

function isFile(relPath) {
  const fullPath = join(siteDir, relPath);
  return existsSync(fullPath) && statSync(fullPath).isFile();
}

function readJson(relPath) {
  const fullPath = join(siteDir, relPath);
  if (!existsSync(fullPath)) return null;
  try {
    return JSON.parse(readFileSync(fullPath, 'utf8'));
  } catch {
    return null;
  }
}

function siteFileFor(relPath) {
  const pathParts = String(relPath || '').split('/').filter(Boolean);
  if (!pathParts.length || pathParts.includes('..')) return null;
  const fullPath = normalize(join(siteDir, ...pathParts));
  const normalizedSite = normalize(siteDir);
  if (fullPath !== normalizedSite && !fullPath.startsWith(`${normalizedSite}/`)) return null;
  return fullPath;
}

function stripMarkdown(value) {
  return String(value || '')
    .replace(/^---\n[\s\S]*?\n---\n*/m, '')
    .replace(/`{3}[\s\S]*?`{3}/g, ' ')
    .replace(/!\[[^\]]*]\([^)]*\)/g, ' ')
    .replace(/\[([^\]]+)]\([^)]*\)/g, '$1')
    .replace(/[#>*_`-]+/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
}

function normalizeSearchText(value) {
  return stripMarkdown(value).toLowerCase();
}

function excerptFor(text, query) {
  const clean = stripMarkdown(text);
  const lower = clean.toLowerCase();
  const needle = query.toLowerCase();
  const index = lower.indexOf(needle);
  if (index < 0) return clean.slice(0, 180);
  const start = Math.max(0, index - 70);
  const end = Math.min(clean.length, index + needle.length + 110);
  return `${start > 0 ? '...' : ''}${clean.slice(start, end)}${end < clean.length ? '...' : ''}`;
}

function searchWiki(query) {
  const q = String(query || '').trim();
  if (!q) return { query: q, matches: [], pages: [] };
  const index = readJson('.1context/content-index.json');
  const entries = Array.isArray(index?.pages) ? index.pages : [];
  const terms = normalizeSearchText(q).split(/\s+/).filter(Boolean);
  if (!terms.length) return { query: q, matches: [], pages: [] };
  const matches = [];
  for (const entry of entries) {
    if (!entry?.route || !entry.markdown_path) continue;
    if (!routeCandidate(entry.route)) continue;
    const markdownPath = siteFileFor(entry.markdown_path);
    if (!markdownPath || !existsSync(markdownPath) || !statSync(markdownPath).isFile()) continue;
    const markdown = readFileSync(markdownPath, 'utf8');
    const haystack = normalizeSearchText([
      entry.title,
      entry.slug,
      entry.route,
      entry.kind,
      stripMarkdown(markdown),
    ].join(' '));
    if (!terms.every((term) => haystack.includes(term))) continue;
    matches.push({
      title: entry.title || entry.slug || entry.route,
      route: entry.route,
      url: entry.route,
      summary: excerptFor(markdown, q),
      excerpt: excerptFor(markdown, q),
      family_label: entry.kind === 'talk' ? 'Talk' : 'Page',
      kind: entry.kind || 'page',
    });
  }
  matches.sort((left, right) => {
    const leftTitle = String(left.title || '').toLowerCase();
    const rightTitle = String(right.title || '').toLowerCase();
    const leftScore = leftTitle.includes(q.toLowerCase()) ? 0 : 1;
    const rightScore = rightTitle.includes(q.toLowerCase()) ? 0 : 1;
    if (leftScore !== rightScore) return leftScore - rightScore;
    return left.route.localeCompare(right.route);
  });
  return { query: q, matches, pages: matches };
}

function routeCandidate(pathname) {
  const clean = decodeURIComponent(pathname.split('?')[0]);
  if (clean.includes('\0')) return null;
  if (clean === '/') return isFile('index.html') ? 'index.html' : null;
  if (clean === '/favicon.ico' && isFile('assets/favicon-32.png')) {
    return 'assets/favicon-32.png';
  }

  const wantsRouteIndex = clean.endsWith('/');
  const trimmed = clean.replace(/^\/+/, '').replace(/\/+$/, '');
  if (!trimmed || trimmed.split('/').includes('..')) return null;

  const legacyTalkRoute = extname(trimmed) === '.talk';
  const candidates = legacyTalkRoute
    ? wantsRouteIndex
      ? [`${trimmed}/index.html`, `${trimmed}.html`, trimmed]
      : [trimmed, `${trimmed}.html`, `${trimmed}/index.html`]
    : extname(trimmed)
    ? [trimmed]
    : wantsRouteIndex
      ? [`${trimmed}/index.html`, trimmed, `${trimmed}.html`]
      : [trimmed, `${trimmed}.html`, `${trimmed}/index.html`];
  return candidates.find(isFile) || null;
}

function writeJson(res, payload, statusCode = 200, headers = {}) {
  res.writeHead(statusCode, { 'content-type': 'application/json; charset=utf-8', ...headers });
  res.end(JSON.stringify(payload));
}

function writeNoContent(res, headers = {}) {
  res.writeHead(204, headers);
  res.end();
}

function staticWikiStatePayload() {
  return {
    settings: {},
    bookmarks: [],
    _storage: {
      exists: false,
      writable: false,
      mode: 'static',
      uri: 'static://serve-site/wiki-browser-state.json',
    },
  };
}

const server = createServer((req, res) => {
  const url = new URL(req.url || '/', `http://${host}`);
  if (url.pathname === '/api/wiki/state') {
    if (req.method === 'OPTIONS') {
      writeNoContent(res, {
        allow: 'GET, HEAD, OPTIONS',
        'access-control-allow-methods': 'GET, HEAD, OPTIONS',
      });
      return;
    }
    if (req.method !== 'GET' && req.method !== 'HEAD') {
      writeJson(
        res,
        {
          error: 'static_state_read_only',
          message: 'Disposable static serving exposes wiki state for read-only hydration only.',
        },
        405,
        { allow: 'GET, HEAD, OPTIONS' }
      );
      return;
    }
    if (req.method === 'HEAD') {
      res.writeHead(200, { 'content-type': 'application/json; charset=utf-8' });
      res.end();
      return;
    }
    writeJson(res, staticWikiStatePayload());
    return;
  }
  if (url.pathname === '/api/wiki/search') {
    if (req.method === 'OPTIONS') {
      writeNoContent(res, {
        allow: 'GET, HEAD, OPTIONS',
        'access-control-allow-methods': 'GET, HEAD, OPTIONS',
      });
      return;
    }
    if (req.method === 'HEAD') {
      res.writeHead(200, { 'content-type': 'application/json; charset=utf-8' });
      res.end();
      return;
    }
    if (req.method !== 'GET') {
      writeJson(res, { error: 'method_not_allowed' }, 405, { allow: 'GET, HEAD, OPTIONS' });
      return;
    }
    writeJson(res, searchWiki(url.searchParams.get('q')));
    return;
  }
  const relPath = routeCandidate(url.pathname);
  const resolved = relPath ? normalize(join(siteDir, relPath)) : null;
  if (!relPath || !resolved.startsWith(normalize(siteDir)) || !existsSync(resolved)) {
    res.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
    res.end(`missing route: ${url.pathname}\n`);
    return;
  }

  res.writeHead(200, { 'content-type': types[extname(resolved).toLowerCase()] || 'application/octet-stream' });
  createReadStream(resolved).pipe(res);
});

server.listen(port, host, () => {
  const address = server.address();
  const url = `http://${address.address}:${address.port}`;
  if (portFile) writeFileSync(portFile, String(address.port));
  console.log(`serving ${siteDir} on ${url}`);
});
