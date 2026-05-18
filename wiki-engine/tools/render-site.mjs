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
import { dirname, join, relative, resolve } from 'node:path';
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

function routeForMarkdown(markdownPath) {
  if (markdownPath.endsWith('.talk.md')) {
    const base = markdownPath.slice(0, -'.talk.md'.length);
    return `/${base}/talk`;
  }
  return `/${markdownPath.slice(0, -'.md'.length)}`;
}

function htmlForMarkdown(markdownPath) {
  if (markdownPath.endsWith('.talk.md')) {
    return `${markdownPath.slice(0, -'.talk.md'.length)}.talk.html`;
  }
  return `${markdownPath.slice(0, -'.md'.length)}.html`;
}

function routeIndexForMarkdown(markdownPath) {
  if (markdownPath.includes('/')) return null;
  if (markdownPath.endsWith('.talk.md')) {
    const base = markdownPath.slice(0, -'.talk.md'.length);
    return `${base}/talk/index.html`;
  }
  return `${markdownPath.slice(0, -'.md'.length)}/index.html`;
}

function readFrontmatter(path) {
  try {
    return matter(readFileSync(path, 'utf8')).data || {};
  } catch {
    return {};
  }
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
    const route = hasHtml ? routeForMarkdown(markdownPath) : null;

    const twin = {
      ...fileRecord(output, markdownFile, `${kind}-markdown`, 'text/markdown; charset=utf-8'),
      kind,
      route,
      html_path: hasHtml ? htmlPath : null,
      route_index_path: hasRouteIndex ? routeIndexPath : null,
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
        slug: twin.slug,
        title: twin.title,
        access: twin.access,
        status: twin.status,
        html_path: htmlPath,
        route_index_path: hasRouteIndex ? routeIndexPath : null,
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

function renderInput(input, output) {
  const result = spawnSync(process.execPath, [RENDER_TOOL, input, output], {
    cwd: ENGINE_ROOT,
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
  const sourceInputs = walkFiles(families)
    .filter((path) => /\/source\/families\/[^/]+\/[^/]+\/source\/[^/]+\.md$/.test(path))
    .filter((path) => !path.endsWith('.tombstone.md'))
    .filter((path) => !sourceIsTombstoned(path))
    .sort();
  const talkInputs = walkDirs(families)
    .filter((path) => path.endsWith('.talk') && existsSync(join(path, '_meta.yaml')))
    .filter((path) => !talkFolderIsTombstoned(path))
    .sort();

  try {
    if (sourceInputs.length === 0 && talkInputs.length === 0) {
      throw new Error(`No wiki source pages or talk folders found under ${families}`);
    }
    rmSync(output, { recursive: true, force: true });
    mkdirSync(output, { recursive: true });
    const logs = [];
    for (const input of [...sourceInputs, ...talkInputs]) {
      logs.push({ input, summary: renderInput(input, output) });
    }
    const assets = copyThemeAssets(output);
    const metadata = buildSiteMetadata(output, startedAt, assets);
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
      talk_inputs: talkInputs,
      source_input_count: sourceInputs.length,
      talk_input_count: talkInputs.length,
      assets,
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
      talk_inputs: talkInputs,
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
