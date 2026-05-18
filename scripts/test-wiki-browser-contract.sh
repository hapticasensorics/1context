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

ONECONTEXT_RENDER_CONTRACT_RUNTIME="$RUNTIME_TEST" \
ONECONTEXT_RENDER_CONTRACT_OUT_DIR="$OUT_DIR" \
ONECONTEXT_RENDER_CONTRACT_KEEP=1 \
  "$ROOT/scripts/test-wiki-render-contract.sh" >"$ARTIFACT_DIR/render-contract.out"

SITE_DIR="$OUT_DIR/site"
PORT_FILE="$PORT_FILE" node "$ROOT/scripts/serve-wiki-site.mjs" "$SITE_DIR" >"$SERVER_LOG" 2>&1 &
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

const baseURL = process.env.BASE_URL;
const artifactDir = process.env.ARTIFACT_DIR;
const pages = [
  { route: '/for-you', talkRoute: '/for-you/talk', mdPath: '/for-you.md', talkMdPath: '/for-you.talk.md', label: 'For You' },
  { route: '/your-context', talkRoute: '/your-context/talk', mdPath: '/your-context.md', talkMdPath: '/your-context.talk.md' },
  { route: '/projects', talkRoute: '/projects/talk', mdPath: '/projects.md', talkMdPath: '/projects.talk.md' },
  { route: '/topics', talkRoute: '/topics/talk', mdPath: '/topics.md', talkMdPath: '/topics.talk.md' },
];

async function internalLinks(page) {
  return await page.evaluate(() => Array.from(document.links)
    .map((link) => link.href)
    .filter((href) => href.startsWith(location.origin))
    .filter((href) => !href.includes('#')));
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
  for (const wikiPage of pages) {
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

    const resourceLinks = await page.evaluate(() => Array.from(document.querySelectorAll('link[href], script[src], img[src]'))
      .map((el) => el.href || el.src)
      .filter((href) => href.startsWith(location.origin)));
    for (const href of resourceLinks) {
      const resourceResponse = await request.get(href);
      if (resourceResponse.status() >= 400) {
        fail('missing-resource', `${route} has missing resource ${new URL(href).pathname} (${resourceResponse.status()})`);
      }
    }

    const markdownLinks = await page.evaluate(() => [
      ...Array.from(document.links).map((link) => link.href),
      ...Array.from(document.querySelectorAll('link[rel="alternate"][type="text/markdown"]')).map((link) => link.href),
    ]
      .filter((href) => href.startsWith(location.origin))
      .filter((href) => href.endsWith('.md')));
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
  bash -c 'cd "$0" && printf "%s\n" "{\"name\":\"onecontext-wiki-browser-contract\",\"private\":true,\"type\":\"commonjs\"}" > package.json && npm install --silent --no-save @playwright/test >/dev/null && npx playwright test "$(basename "$1")" --reporter=line' \
  "$WORK_DIR" "$TEST_FILE"

echo "browser_contract_base_url=$BASE_URL"
echo "browser_contract_artifacts=$ARTIFACT_DIR"
