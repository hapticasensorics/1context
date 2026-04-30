#!/usr/bin/env node
// Smallest possible CLI for the in-progress P1 renderer.
//
// Usage:
//   node wiki-engine/tools/render-test.mjs <slug>
//
// Reads `preview/public/<slug>.md`, renders both surfaces (HTML + the
// .md twin), writes them to `_render-test/<slug>.{html,md}` so you
// can diff against the hand-authored version in `preview/<slug>.html`
// without overwriting anything.
//
// This is the proof-of-life for P1 — once the renderer can produce
// HTML close enough to the hand-authored version, we'll wire it into
// the actual build pipeline (replacing the hand-authored HTML files).
// Until then, this script is the way to iterate.

import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { renderPage, FrontmatterError } from '../src/renderer/index.mjs';

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO = resolve(__dirname, '../..');

function main() {
  const slug = process.argv[2];
  if (!slug) {
    console.error('Usage: node wiki-engine/tools/render-test.mjs <slug>');
    console.error('Example: node wiki-engine/tools/render-test.mjs wiki-engine');
    process.exit(2);
  }

  const sourcePath = resolve(REPO, 'preview/public', `${slug}.md`);
  let source;
  try {
    source = readFileSync(sourcePath, 'utf8');
  } catch (err) {
    console.error(`Could not read ${sourcePath}: ${err.message}`);
    process.exit(1);
  }

  let result;
  try {
    result = renderPage(source, { slug });
  } catch (err) {
    if (err instanceof FrontmatterError) {
      console.error(`Frontmatter validation failed: ${err.message}`);
      process.exit(1);
    }
    throw err;
  }

  const outDir = resolve(REPO, '_render-test');
  mkdirSync(outDir, { recursive: true });
  const htmlPath = resolve(outDir, `${slug}.html`);
  const mdPath = resolve(outDir, `${slug}.md`);
  writeFileSync(htmlPath, result.html);
  writeFileSync(mdPath, result.md);

  console.log(`✓ rendered ${slug}`);
  console.log(`  ${htmlPath}  (${result.html.length} bytes)`);
  console.log(`  ${mdPath}  (${result.md.length} bytes)`);
  console.log();
  console.log('Compare HTML against the hand-authored version:');
  console.log(`  diff ${htmlPath} preview/${slug}.html`);
  console.log();
  console.log('Custom directives are not yet implemented — `:::infobox`,');
  console.log('`:::main-article`, `:::see-also` blocks render as raw');
  console.log('paragraphs starting with ":::". See the talk page TODO.');
}

main();
