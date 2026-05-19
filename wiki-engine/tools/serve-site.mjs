#!/usr/bin/env node
import { createServer } from 'node:http';
import { createReadStream, existsSync, statSync, writeFileSync } from 'node:fs';
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
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
};

function isFile(relPath) {
  const fullPath = join(siteDir, relPath);
  return existsSync(fullPath) && statSync(fullPath).isFile();
}

function routeCandidate(pathname) {
  const clean = decodeURIComponent(pathname.split('?')[0]);
  if (clean.includes('\0')) return null;
  if (clean === '/') return isFile('index.html') ? 'index.html' : null;

  const trimmed = clean.replace(/^\/+/, '').replace(/\/+$/, '');
  if (!trimmed || trimmed.split('/').includes('..')) return null;

  const candidates = extname(trimmed)
    ? [trimmed]
    : [trimmed, `${trimmed}.html`, `${trimmed}/index.html`];
  return candidates.find(isFile) || null;
}

function writeJson(res, payload) {
  res.writeHead(200, { 'content-type': 'application/json; charset=utf-8' });
  res.end(JSON.stringify(payload));
}

const server = createServer((req, res) => {
  const url = new URL(req.url || '/', `http://${host}`);
  if (url.pathname === '/api/wiki/state') {
    writeJson(res, { ok: true, state: {} });
    return;
  }
  const relPath = routeCandidate(url.pathname);
  const resolved = relPath ? normalize(join(siteDir, relPath)) : null;
  if (!relPath || !resolved.startsWith(normalize(siteDir)) || !existsSync(resolved)) {
    res.writeHead(404, { 'content-type': 'text/plain; charset=utf-8' });
    res.end(`missing route: ${url.pathname}\n`);
    return;
  }

  res.writeHead(200, { 'content-type': types[extname(resolved)] || 'application/octet-stream' });
  createReadStream(resolved).pipe(res);
});

server.listen(port, host, () => {
  const address = server.address();
  const url = `http://${address.address}:${address.port}`;
  if (portFile) writeFileSync(portFile, String(address.port));
  console.log(`serving ${siteDir} on ${url}`);
});
