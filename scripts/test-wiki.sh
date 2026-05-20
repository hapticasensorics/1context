#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUNTIME_TEST="$(mktemp -d /tmp/1ctx-wiki-browser-runtime-XXXXXX)"
OUT_DIR="$(mktemp -d /tmp/1ctx-wiki-browser-output-XXXXXX)"
ARTIFACT_DIR="${ONECONTEXT_BROWSER_ARTIFACT_DIR:-$(mktemp -d /tmp/1ctx-wiki-browser-artifacts-XXXXXX)}"
WORK_DIR="$(mktemp -d "$ROOT/.tmp-wiki-browser-XXXXXX")"
TEST_FILE="$WORK_DIR/wiki-browser-contract.spec.js"
SERVER_LOG="$ARTIFACT_DIR/server.log"
PORT_FILE="$ARTIFACT_DIR/port"

resolve_wiki_core_bin() {
  if [[ -n "${ONECONTEXT_WIKI_CORE_BIN:-}" ]]; then
    printf '%s\n' "$ONECONTEXT_WIKI_CORE_BIN"
    return
  fi
  local debug_bin="$ROOT/target/debug/onecontext-wiki"
  if [[ ! -x "$debug_bin" ]] || find "$ROOT/crates" -name '*.rs' -newer "$debug_bin" -print -quit | grep -q .; then
    cargo build --package onecontext-wiki-daemon >/dev/null
  fi
  printf '%s\n' "$debug_bin"
}

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "$SERVER_PID" >/dev/null 2>&1 || true
  fi
  rm -rf "$RUNTIME_TEST"
  rm -rf "$OUT_DIR"
  rm -rf "$WORK_DIR"
  if [[ -z "${ONECONTEXT_BROWSER_ARTIFACT_DIR:-}" ]]; then
    rm -rf "$ARTIFACT_DIR"
  fi
}
trap cleanup EXIT

mkdir -p "$ARTIFACT_DIR"

"$ROOT/scripts/init-dev-wiki-runtime.sh" "$RUNTIME_TEST" >"$ARTIFACT_DIR/init-runtime.out"

cat >>"$RUNTIME_TEST/1Context/user-wiki/wiki.toml" <<'TOML'

[[pages]]
id = "dummy-custom"
enabled = true
title = "Dummy Custom"
slug = "dummy-custom"
route = "/dummy-custom"
family_group = "custom"
family_group_title = "Custom"
family_id = "dummy-custom"
family_title = "Dummy Custom"
type = "context-page"
template = "pages/context-page.md"
talk_conventions_template = "talk/conventions.md"
summary = "Fixture custom page generated from the generic fallback template."
nav_order = 900
TOML

WIKI_CORE_BIN="$(resolve_wiki_core_bin)"
"$WIKI_CORE_BIN" --root "$RUNTIME_TEST/1Context" page-create dummy-custom >"$ARTIFACT_DIR/create-custom-page.json"

CUSTOM_SOURCE="$RUNTIME_TEST/1Context/user-wiki/source/families/custom/dummy-custom/source/dummy-custom.md"
CUSTOM_TALK="$RUNTIME_TEST/1Context/user-wiki/source/families/custom/dummy-custom/talk/dummy-custom.talk"
test -f "$CUSTOM_SOURCE"
test -f "$CUSTOM_TALK/_meta.yaml"
grep -q 'title: "Dummy Custom"' "$CUSTOM_SOURCE"
grep -q 'talk_route: "/dummy-custom/talk"' "$CUSTOM_TALK/_meta.yaml"
if grep -R '{{' "$CUSTOM_SOURCE" "$CUSTOM_TALK" >/dev/null; then
  echo "custom page creation left unresolved template placeholders" >&2
  exit 1
fi

SITE_DIR="$OUT_DIR/site"
RESULT_JSON="$OUT_DIR/render-result.json"
node "$ROOT/wiki-engine/tools/render-site.mjs" \
  --source-root "$RUNTIME_TEST/1Context/user-wiki/source" \
  --output "$SITE_DIR" \
  --result-json "$RESULT_JSON" \
  >"$ARTIFACT_DIR/render-site.out"

python3 - "$RESULT_JSON" "$SITE_DIR/.1context/route-manifest.json" <<'PY'
import json
import sys
from pathlib import Path

result = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
manifest = json.loads(Path(sys.argv[2]).read_text(encoding="utf-8"))
if result.get("status") != "published":
    raise SystemExit("wiki render did not publish")
routes = {entry.get("route") for entry in manifest.get("routes", [])}
for route in [
    "/for-you",
    "/for-you/talk",
    "/your-context",
    "/your-context/talk",
    "/projects",
    "/projects/talk",
    "/topics",
    "/topics/talk",
    "/dummy-custom",
    "/dummy-custom/talk",
]:
    if route not in routes:
        raise SystemExit(f"missing rendered wiki route: {route}")
if int(result.get("markdown_twin_count") or 0) < 10:
    raise SystemExit("render result did not include expected markdown twins")
PY

PORT_FILE="$PORT_FILE" node "$ROOT/wiki-engine/tools/serve-site.mjs" "$SITE_DIR" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!

for _ in {1..100}; do
  [[ -f "$PORT_FILE" ]] && break
  sleep 0.05
done

if [[ ! -f "$PORT_FILE" ]]; then
  echo "browser contract server did not start" >&2
  cat "$SERVER_LOG" >&2 || true
  exit 1
fi

BASE_URL="http://127.0.0.1:$(cat "$PORT_FILE")"

cat >"$TEST_FILE" <<'NODE'
const { test, expect } = require('@playwright/test');
const fs = require('node:fs');

test.use({ channel: process.env.PLAYWRIGHT_BROWSER_CHANNEL || 'chrome' });
test.setTimeout(Number(process.env.ONECONTEXT_WIKI_BROWSER_TIMEOUT_MS || '180000'));

const baseURL = process.env.BASE_URL;
const artifactDir = process.env.ARTIFACT_DIR;
const pages = [
  { route: '/for-you', talkRoute: '/for-you/talk', mdPath: '/for-you.md', talkMdPath: '/for-you.talk.md', label: 'For You' },
  { route: '/your-context', talkRoute: '/your-context/talk', mdPath: '/your-context.md', talkMdPath: '/your-context.talk.md' },
  { route: '/projects', talkRoute: '/projects/talk', mdPath: '/projects.md', talkMdPath: '/projects.talk.md' },
  { route: '/topics', talkRoute: '/topics/talk', mdPath: '/topics.md', talkMdPath: '/topics.talk.md' },
  { route: '/dummy-custom', talkRoute: '/dummy-custom/talk', mdPath: '/dummy-custom.md', talkMdPath: '/dummy-custom.talk.md', nav: false },
];

async function internalLinks(page) {
  return await page.evaluate(() => Array.from(new Set(Array.from(document.links)
    .map((link) => link.href)
    .filter((href) => href.startsWith(location.origin))
    .filter((href) => !href.includes('#')))));
}

async function assertTocTargets(page, route, fail) {
  const toc = await page.evaluate(() => {
    const links = Array.from(document.querySelectorAll('.opctx-toc a[href^="#"]'))
      .map((link) => ({
        id: link.getAttribute('href').slice(1),
        text: link.textContent.trim(),
      }))
      .filter((link) => link.id);
    const headingIds = Array.from(document.querySelectorAll('article h2[id], article h3[id]'))
      .map((heading) => ({
        id: heading.id,
        text: heading.textContent.trim(),
      }))
      .filter((heading) => heading.id);
    const linkIds = new Set(links.map((link) => link.id));
    return {
      links,
      headingIds,
      missingTargets: links.filter((link) => !document.getElementById(link.id)),
      missingHeadings: headingIds.filter((heading) => !linkIds.has(heading.id)),
    };
  });
  if (toc.headingIds.length > 0 && toc.links.length === 0) {
    fail('missing-toc', `${route} has ${toc.headingIds.length} heading(s) but no TOC links`);
  }
  if (toc.missingTargets.length > 0) {
    fail('toc-broken-anchor', `${route} TOC links target missing heading ids: ${toc.missingTargets.map((item) => item.id).join(', ')}`);
  }
  if (toc.missingHeadings.length > 0) {
    fail('toc-missing-heading', `${route} headings missing from TOC: ${toc.missingHeadings.map((item) => item.id).join(', ')}`);
  }
}

async function assertBrandMenuNavigation(page, fail) {
  for (const wikiPage of pages.filter((candidate) => candidate.nav !== false)) {
    const startRoute = wikiPage.route === '/your-context' ? '/topics' : '/your-context';
    await page.goto(`${baseURL}${startRoute}`);
    const toggle = page.locator('.opctx-brand-menu-toggle');
    if (await toggle.count() !== 1) {
      fail('brand-menu-toggle', `${startRoute} should expose exactly one brand menu toggle`);
      continue;
    }
    await toggle.click();
    const item = page.locator(`#opctx-brand-menu a[href="${wikiPage.route}"]`);
    if (await item.count() !== 1) {
      fail('brand-menu-link', `brand menu should expose exactly one ${wikiPage.route} link`);
      continue;
    }
    await item.click();
    await page.waitForLoadState('load');
    if (page.url() !== `${baseURL}${wikiPage.route}`) {
      fail('brand-menu-navigation', `brand menu link ${wikiPage.route} landed on ${page.url()}`);
    }
  }
}

test('wiki source and talk routes work in a real browser', async ({ page, request }) => {
  const failures = [];
  const consoleErrors = [];
  const responseErrors = [];
  page.on('console', (message) => {
    if (message.type() === 'error') consoleErrors.push(message.text());
  });
  page.on('response', (response) => {
    const url = response.url();
    if (url.startsWith(baseURL) && response.status() >= 400) {
      responseErrors.push(`${new URL(url).pathname} ${response.status()}`);
    }
  });

  function fail(kind, detail) {
    failures.push({ kind, detail });
  }

  async function assertRoute(route, expectedMdPath) {
    const response = await page.goto(`${baseURL}${route}`);
    if (!response || response.status() !== 200) {
      fail('route-status', `${route} returned ${response ? response.status() : 'no response'}`);
      return;
    }
    if (page.url() !== `${baseURL}${route}`) {
      fail('wrong-redirect', `${route} landed on ${page.url()}`);
    }
    const h1Text = await page.locator('h1').first().innerText().catch(() => '');
    if (h1Text.includes('Operator')) {
      fail('placeholder-title', `${route} rendered placeholder title "${h1Text}"`);
    }
    await assertTocTargets(page, route, fail);
    await expect(page.locator('body')).not.toContainText('undefined');
    await expect(page.locator('body')).not.toContainText('/Users/');
    await page.screenshot({
      path: `${artifactDir}${route.replaceAll('/', '_') || '_home'}.png`,
      fullPage: false,
    });

    for (const href of await internalLinks(page)) {
      const url = new URL(href);
      if (url.pathname === '/') continue;
      const linkResponse = await request.get(href);
      if (linkResponse.status() >= 400) {
        fail('broken-link', `${route} has broken link ${url.pathname} (${linkResponse.status()})`);
      }
    }

    const resourceLinks = await page.evaluate(() => Array.from(new Set(Array.from(document.querySelectorAll('link[href], script[src], img[src]'))
      .map((el) => el.href || el.src)
      .filter((href) => href.startsWith(location.origin)))));
    for (const href of resourceLinks) {
      const resourceResponse = await request.get(href);
      if (resourceResponse.status() >= 400) {
        fail('missing-resource', `${route} has missing resource ${new URL(href).pathname} (${resourceResponse.status()})`);
      }
    }

    const markdownLinks = await page.evaluate(() => Array.from(new Set([
      ...Array.from(document.links).map((link) => link.href),
      ...Array.from(document.querySelectorAll('link[rel="alternate"][type="text/markdown"]')).map((link) => link.href),
    ]
      .filter((href) => href.startsWith(location.origin))
      .filter((href) => href.endsWith('.md')))));
    if (markdownLinks.length === 0) {
      fail('missing-markdown-twin', `${route} exposes no markdown twin link`);
    }
    if (!markdownLinks.some((href) => new URL(href).pathname === expectedMdPath)) {
      fail('wrong-markdown-twin', `${route} does not expose ${expectedMdPath}`);
    }
    for (const href of markdownLinks) {
      const mdResponse = await request.get(href);
      if (mdResponse.status() !== 200) {
        fail('markdown-twin-status', `${route} markdown twin ${new URL(href).pathname} returned ${mdResponse.status()}`);
      }
    }
  }

  for (const wikiPage of pages) {
    await assertRoute(wikiPage.route, wikiPage.mdPath);

    const talkToggle = page.locator('[data-talk-toggle]');
    if (await talkToggle.count() !== 1) {
      fail('talk-toggle', `${wikiPage.route} should expose exactly one Talk button`);
    } else {
      await talkToggle.click();
      await page.waitForLoadState('load');
      if (page.url() !== `${baseURL}${wikiPage.talkRoute}`) {
        fail('talk-navigation', `${wikiPage.route} Talk button landed on ${page.url()}, expected ${baseURL}${wikiPage.talkRoute}`);
      }
      await assertRoute(wikiPage.talkRoute, wikiPage.talkMdPath);
      const backToggle = page.locator('[data-talk-toggle]');
      if (await backToggle.count() !== 1) {
        fail('talk-back-toggle', `${wikiPage.talkRoute} should expose exactly one Talk button back to source`);
      } else {
        await backToggle.click();
        await page.waitForLoadState('load');
        if (page.url() !== `${baseURL}${wikiPage.route}`) {
          fail('talk-back-navigation', `${wikiPage.talkRoute} Talk button landed on ${page.url()}, expected ${baseURL}${wikiPage.route}`);
        }
      }
    }

    await assertRoute(wikiPage.route, wikiPage.mdPath);
    const agentButton = page.locator('[data-view-set="agent"]');
    if (await agentButton.count() !== 1) {
      fail('agent-toggle', `${wikiPage.route} should expose exactly one Agent view button`);
    } else {
      await agentButton.click();
      await expect(page.locator('#agent-frontmatter')).toBeVisible();
      await expect(page.locator('#agent-body')).toBeVisible();
      await expect(page.locator('body')).not.toContainText('Failed to load markdown alternate');
      for (const href of await internalLinks(page)) {
        const url = new URL(href);
        if (url.pathname === '/') continue;
        const linkResponse = await request.get(href);
        if (linkResponse.status() >= 400) {
          fail('agent-view-broken-link', `${wikiPage.route} Agent view has broken link ${url.pathname} (${linkResponse.status()})`);
        }
      }
    }

    await assertRoute(wikiPage.talkRoute, wikiPage.talkMdPath);
    const talkAgentButton = page.locator('[data-view-set="agent"]');
    if (await talkAgentButton.count() !== 1) {
      fail('talk-agent-toggle', `${wikiPage.talkRoute} should expose exactly one Agent view button`);
    } else {
      await talkAgentButton.click();
      await expect(page.locator('#agent-frontmatter')).toBeVisible();
      await expect(page.locator('#agent-body')).toBeVisible();
      await expect(page.locator('body')).not.toContainText('Failed to load markdown alternate');
      for (const href of await internalLinks(page)) {
        const url = new URL(href);
        if (url.pathname === '/') continue;
        const linkResponse = await request.get(href);
        if (linkResponse.status() >= 400) {
          fail('talk-agent-view-broken-link', `${wikiPage.talkRoute} Agent view has broken link ${url.pathname} (${linkResponse.status()})`);
        }
      }
    }
  }

  const routeIndex = await page.goto(`${baseURL}/dummy-custom/`);
  if (!routeIndex || routeIndex.status() !== 200) {
    fail('route-index-status', `/dummy-custom/ returned ${routeIndex ? routeIndex.status() : 'no response'}`);
  }
  const routeIndexState = await page.evaluate(() => ({
    base: document.querySelector('base')?.getAttribute('href') || '',
    hasRootAsset: Array.from(document.querySelectorAll('link[href], script[src]'))
      .some((el) => (el.getAttribute('href') || el.getAttribute('src') || '').startsWith('/assets/')),
  }));
  if (routeIndexState.base !== '/dummy-custom') {
    fail('route-index-base', `/dummy-custom/ should serve the route-index page with base /dummy-custom, got ${routeIndexState.base || '<none>'}`);
  }
  if (!routeIndexState.hasRootAsset) {
    fail('route-index-assets', `/dummy-custom/ should keep root-anchored asset links`);
  }

  await assertBrandMenuNavigation(page, fail);

  const consoleBeforeMissing = consoleErrors.length;
  const responsesBeforeMissing = responseErrors.length;
  const missing = await page.goto(`${baseURL}/definitely-missing`);
  if (!missing || missing.status() !== 404) {
    fail('missing-route-status', `/definitely-missing should return 404 diagnostic, got ${missing ? missing.status() : 'no response'}`);
  }
  if (page.url() !== `${baseURL}/definitely-missing`) {
    fail('missing-route-redirect', `/definitely-missing redirected to ${page.url()}`);
  }
  consoleErrors.length = consoleBeforeMissing;
  responseErrors.length = responsesBeforeMissing;
  if (consoleErrors.length > 0) {
    fail('console-error', `browser console errors:\n${consoleErrors.join('\n')}`);
  }
  if (responseErrors.length > 0) {
    fail('response-error', `browser response errors:\n${[...new Set(responseErrors)].join('\n')}`);
  }

  fs.writeFileSync(`${artifactDir}failure-cases.json`, JSON.stringify(failures, null, 2));
  expect(failures, failures.map((failure) => `${failure.kind}: ${failure.detail}`).join('\n')).toEqual([]);
});
NODE

BASE_URL="$BASE_URL" ARTIFACT_DIR="$ARTIFACT_DIR/" \
  NPM_CONFIG_CACHE="$ARTIFACT_DIR/npm-cache" \
  bash -c 'cd "$0" && printf "%s\n" "{\"name\":\"onecontext-wiki-browser-contract\",\"private\":true,\"type\":\"commonjs\"}" > package.json && npm install --silent --no-save @playwright/test >/dev/null && npx playwright test "$(basename "$1")" --reporter=line --timeout="${ONECONTEXT_WIKI_BROWSER_TIMEOUT_MS:-180000}"' \
  "$WORK_DIR" "$TEST_FILE"

echo "browser_contract_base_url=$BASE_URL"
echo "browser_contract_artifacts=$ARTIFACT_DIR"
