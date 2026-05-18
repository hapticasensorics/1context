#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="${ONECONTEXT_RENDER_CONTRACT_RUNTIME:-$(mktemp -d /tmp/1ctx-wiki-render-contract-XXXXXX)}"
OUT_DIR="${ONECONTEXT_RENDER_CONTRACT_OUT_DIR:-$(mktemp -d /tmp/1ctx-wiki-render-output-XXXXXX)}"
ENGINE_DIR="$ROOT/wiki-engine"
RENDER_CLI="$ENGINE_DIR/tools/render-to-dir.mjs"
KEEP_OUTPUT="${ONECONTEXT_RENDER_CONTRACT_KEEP:-0}"
CREATED_RUNTIME=0
CREATED_OUTPUT=0

if [[ -z "${ONECONTEXT_RENDER_CONTRACT_RUNTIME:-}" ]]; then
  CREATED_RUNTIME=1
fi

if [[ -z "${ONECONTEXT_RENDER_CONTRACT_OUT_DIR:-}" ]]; then
  CREATED_OUTPUT=1
fi

cleanup() {
  if [[ "$KEEP_OUTPUT" != "1" && "$CREATED_RUNTIME" == "1" ]]; then
    rm -rf "$RUNTIME_TEST"
  fi
  if [[ "$KEEP_OUTPUT" != "1" && "$CREATED_OUTPUT" == "1" ]]; then
    rm -rf "$OUT_DIR"
  fi
}
trap cleanup EXIT

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" >/tmp/1ctx-wiki-render-contract-init.out

(
  cd "$ENGINE_DIR"
  RUNTIME_TEST="$RUNTIME_TEST" OUT_DIR="$OUT_DIR" RENDER_CLI="$RENDER_CLI" node --input-type=module <<'NODE'
import { copyFileSync, existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { spawnSync } from 'node:child_process';
import matter from 'gray-matter';
import yaml from 'js-yaml';

const runtimeTest = process.env.RUNTIME_TEST;
const outDir = process.env.OUT_DIR;
const renderCli = process.env.RENDER_CLI;
const userWiki = join(runtimeTest, '1Context/user-wiki');
const families = join(userWiki, 'source/families');
const siteOut = join(outDir, 'site');
const failures = [];

mkdirSync(siteOut, { recursive: true });

function copyThemeAssets() {
  const assetOut = join(siteOut, 'assets');
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
    copyFileSync(join(process.cwd(), source), join(assetOut, dest));
  }
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

function rel(path) {
  return path.replace(`${runtimeTest}/`, '');
}

function renderInput(input) {
  const result = spawnSync(process.execPath, [renderCli, input, siteOut], {
    cwd: process.cwd(),
    encoding: 'utf8',
  });
  if (result.status !== 0) {
    failures.push([
      `render failed: ${rel(input)}`,
      result.stdout.trim(),
      result.stderr.trim(),
    ].filter(Boolean).join('\n'));
    return false;
  }
  return true;
}

function resolveHref(htmlPath, href) {
  if (!href) return null;
  if (/^https?:\/\//.test(href)) return null;
  if (href.startsWith('/')) return join(siteOut, href.slice(1));
  return join(dirname(htmlPath), href);
}

const sourceFiles = walkFiles(families)
  .filter((path) => /\/source\/families\/[^/]+\/[^/]+\/source\/[^/]+\.md$/.test(path))
  .filter((path) => !path.endsWith('.tombstone.md'))
  .sort();

const talkFolders = walkDirs(families)
  .filter((path) => path.endsWith('.talk') && existsSync(join(path, '_meta.yaml')))
  .sort();

for (const source of sourceFiles) {
  if (!renderInput(source)) continue;
  const frontmatter = matter.read(source).data;
  const slug = frontmatter.slug;
  const htmlPath = join(siteOut, `${slug}.html`);
  const mdPath = join(siteOut, `${slug}.md`);
  const routeIndexPath = join(siteOut, slug, 'index.html');
  if (!existsSync(htmlPath)) failures.push(`missing rendered html for ${rel(source)}: ${htmlPath}`);
  if (!existsSync(mdPath)) failures.push(`missing rendered markdown twin for ${rel(source)}: ${mdPath}`);
  if (!existsSync(routeIndexPath)) failures.push(`missing rendered route index for ${rel(source)}: ${routeIndexPath}`);
  const mdHrefPath = resolveHref(htmlPath, frontmatter.md_url || `/${slug}.md`);
  if (mdHrefPath && !existsSync(mdHrefPath)) {
    failures.push(`md_url does not resolve for ${rel(source)}: ${frontmatter.md_url} -> ${mdHrefPath}`);
  }
}

for (const folder of talkFolders) {
  if (!renderInput(folder)) continue;
  const meta = yaml.load(readdirSync(folder).includes('_meta.yaml')
    ? await import('node:fs').then((fs) => fs.readFileSync(join(folder, '_meta.yaml'), 'utf8'))
    : '') || {};
  const slug = meta.slug || folder.split('/').pop();
  const htmlPath = join(siteOut, `${slug}.html`);
  const mdPath = join(siteOut, `${slug}.md`);
  const routeSlug = slug.endsWith('.talk') ? `${slug.slice(0, -'.talk'.length)}/talk` : slug;
  const routeIndexPath = join(siteOut, routeSlug, 'index.html');
  if (!existsSync(htmlPath)) failures.push(`missing rendered talk html for ${rel(folder)}: ${htmlPath}`);
  if (!existsSync(mdPath)) failures.push(`missing rendered talk markdown twin for ${rel(folder)}: ${mdPath}`);
  if (!existsSync(routeIndexPath)) failures.push(`missing rendered talk route index for ${rel(folder)}: ${routeIndexPath}`);
  const rendered = existsSync(mdPath) ? matter.read(mdPath).data : {};
  if (rendered.access !== 'private') {
    failures.push(`talk access should inherit private default for ${rel(folder)}; got ${JSON.stringify(rendered.access)}`);
  }
}

copyThemeAssets();

if (failures.length > 0) {
  console.error(`wiki render contract failed with ${failures.length} failure(s):`);
  for (const failure of failures) console.error(`\n- ${failure}`);
  process.exit(1);
}

console.log(`render_contract_pages=${sourceFiles.length}`);
console.log(`render_contract_talk_folders=${talkFolders.length}`);
console.log(`render_contract_output=${siteOut}`);
NODE
)
