import CryptoKit
import Foundation
import XCTest
@testable import OneContextWikiRuntime
import OneContextPlatform

final class WikiRenderCoordinatorTests: XCTestCase {
  func testPublishesExistingUserWikiSiteAndWritesRenderState() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let sourceSite = paths.userWikiSiteDirectory
    try writeValidatedSite(at: sourceSite)

    let coordinator = WikiRenderCoordinator(runtimePaths: paths, now: { Date(timeIntervalSince1970: 1_777_777_777) })
    let result = coordinator.publishExistingSite(trigger: "unit-test")
    let current = paths.appSupportDirectory.appendingPathComponent("wiki-site/current", isDirectory: true)
    let sourceState = try renderState(at: sourceSite)
    let currentState = try renderState(at: current)
    let events = try String(contentsOf: sourceSite.appendingPathComponent(".1context/render-events.jsonl"), encoding: .utf8)

    XCTAssertEqual(result.status, WikiRenderStatus.published)
    XCTAssertTrue(FileManager.default.fileExists(atPath: current.appendingPathComponent("your-context.html").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: current.appendingPathComponent("your-context.md").path))
    XCTAssertEqual(sourceState.status, .published)
    XCTAssertEqual(currentState.status, .published)
    XCTAssertEqual(sourceState.sourceSite, "user-wiki://site")
    XCTAssertEqual(sourceState.publishedSite, "app-support://wiki-site/current")
    XCTAssertTrue(events.contains(#""status":"published""#))
    XCTAssertTrue(result.message.contains("Validated 1 route(s) and 1 markdown twin(s)"))
  }

  func testInvalidPublishPreservesLastGoodCurrentSite() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    try RuntimePermissions.ensurePrivateDirectory(paths.userWikiSiteDirectory)
    try writeString(
      "<!doctype html><span data-tier=\"private\">Invalid</span>",
      to: paths.userWikiSiteDirectory.appendingPathComponent("your-context.html")
    )
    let current = paths.appSupportDirectory.appendingPathComponent("wiki-site/current", isDirectory: true)
    try RuntimePermissions.ensurePrivateDirectory(current)
    try writeString("last-good", to: current.appendingPathComponent("your-context.html"))

    let coordinator = WikiRenderCoordinator(runtimePaths: paths, now: { Date(timeIntervalSince1970: 1_777_777_777) })
    let result = coordinator.publishExistingSite(trigger: "unit-test")
    let preserved = try String(contentsOf: current.appendingPathComponent("your-context.html"), encoding: .utf8)
    let sourceState = try renderState(at: paths.userWikiSiteDirectory)

    XCTAssertEqual(result.status, WikiRenderStatus.failed)
    XCTAssertEqual(preserved, "last-good")
    XCTAssertEqual(sourceState.status, WikiRenderStatus.failed)
    XCTAssertTrue(sourceState.message.contains("Missing wiki site metadata"))
  }

  func testRenderAndPublishInvokesRendererBeforePromoting() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let engine = try fakeEngine(root: root, exitsWithFailure: false)
    let source = paths.userWikiSourceDirectory
      .appendingPathComponent("families/context/your-context/source", isDirectory: true)
      .appendingPathComponent("your-context.md")
    try writeString("---\ntitle: Your Context\nslug: your-context\nsection: context\naccess: private\n---\n# Your Context\n", to: source)

    let coordinator = WikiRenderCoordinator(
      runtimePaths: paths,
      rendererConfig: WikiEngineRendererConfig(
        nodeExecutable: URL(fileURLWithPath: "/usr/bin/env"),
        engineDirectory: engine,
        renderTool: engine.appendingPathComponent("tools/render-site.mjs")
      ),
      now: { Date(timeIntervalSince1970: 1_777_777_777) }
    )

    let result = coordinator.renderAndPublish(trigger: "unit-test")
    let current = paths.appSupportDirectory.appendingPathComponent("wiki-site/current", isDirectory: true)
    let html = try String(contentsOf: current.appendingPathComponent("your-context.html"), encoding: .utf8)
    let state = try renderState(at: paths.userWikiSiteDirectory)

    XCTAssertEqual(result.status, WikiRenderStatus.published)
    XCTAssertTrue(html.contains("rendered your-context"))
    XCTAssertTrue(FileManager.default.fileExists(atPath: current.appendingPathComponent("assets/theme.css").path))
    XCTAssertTrue(state.message.contains("Rendered 1 source page(s), 0 talk folder(s), 1 route(s), and 1 markdown twin(s)"))
  }

  func testRenderFailurePreservesLastGoodCurrentSite() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let engine = try fakeEngine(root: root, exitsWithFailure: true)
    let current = paths.appSupportDirectory.appendingPathComponent("wiki-site/current", isDirectory: true)
    try RuntimePermissions.ensurePrivateDirectory(current)
    try writeString("last-good", to: current.appendingPathComponent("your-context.html"))
    let source = paths.userWikiSourceDirectory
      .appendingPathComponent("families/context/your-context/source", isDirectory: true)
      .appendingPathComponent("your-context.md")
    try writeString("---\ntitle: Your Context\nslug: your-context\nsection: context\naccess: private\n---\n# Your Context\n", to: source)

    let coordinator = WikiRenderCoordinator(
      runtimePaths: paths,
      rendererConfig: WikiEngineRendererConfig(
        nodeExecutable: URL(fileURLWithPath: "/usr/bin/env"),
        engineDirectory: engine,
        renderTool: engine.appendingPathComponent("tools/render-site.mjs")
      ),
      now: { Date(timeIntervalSince1970: 1_777_777_777) }
    )

    let result = coordinator.renderAndPublish(trigger: "unit-test")
    let preserved = try String(contentsOf: current.appendingPathComponent("your-context.html"), encoding: .utf8)

    XCTAssertEqual(result.status, WikiRenderStatus.failed)
    XCTAssertEqual(preserved, "last-good")
    XCTAssertTrue(result.message.contains("Wiki render failed"))
  }

  func testNoOpRefreshSkipsRendererWhenInputsAreUnchanged() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let engine = try fakeEngine(root: root, exitsWithFailure: false)
    let source = paths.userWikiSourceDirectory
      .appendingPathComponent("families/context/your-context/source", isDirectory: true)
      .appendingPathComponent("your-context.md")
    try writeString("---\ntitle: Your Context\nslug: your-context\nsection: context\naccess: private\n---\n# Your Context\n", to: source)

    let coordinator = WikiRenderCoordinator(
      runtimePaths: paths,
      rendererConfig: WikiEngineRendererConfig(
        nodeExecutable: URL(fileURLWithPath: "/usr/bin/env"),
        engineDirectory: engine,
        renderTool: engine.appendingPathComponent("tools/render-site.mjs")
      ),
      now: { Date(timeIntervalSince1970: 1_777_777_777) }
    )

    let first = coordinator.renderAndPublish(trigger: "wiki.refresh")
    let second = coordinator.renderAndPublish(trigger: "wiki.refresh")
    let renderCount = try String(contentsOf: engine.appendingPathComponent("render-count.txt"), encoding: .utf8)
      .trimmingCharacters(in: .whitespacesAndNewlines)

    XCTAssertEqual(first.status, .published)
    XCTAssertEqual(second.status, .published)
    XCTAssertEqual(renderCount, "1")
    XCTAssertTrue(second.message.contains("Skipped renderer; accepted inputs unchanged"))
  }

  func testRenderAndPublishCanUseExtractedWikiEnginePackage() throws {
    let engine = repoRoot().appendingPathComponent("wiki-engine", isDirectory: true)
    guard FileManager.default.fileExists(atPath: engine.appendingPathComponent("tools/render-site.mjs").path) else {
      throw XCTSkip("Root wiki-engine package is not present in this checkout.")
    }
    guard FileManager.default.fileExists(atPath: engine.appendingPathComponent("node_modules/marked").path) else {
      throw XCTSkip("Root wiki-engine dependencies are not installed.")
    }

    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let source = paths.userWikiSourceDirectory
      .appendingPathComponent("families/context/your-context/source", isDirectory: true)
      .appendingPathComponent("your-context.md")
    try writeString(
      """
      ---
      title: Your Context
      slug: your-context
      section: context
      access: private
      summary: Renderer integration fixture.
      md_url: /your-context.md
      talk_enabled: true
      ---

      # Your Context

      Swift should render this through the extracted wiki-engine package.
      """,
      to: source
    )

    let coordinator = WikiRenderCoordinator(
      runtimePaths: paths,
      rendererConfig: WikiEngineRendererConfig(
        nodeExecutable: URL(fileURLWithPath: "/usr/bin/env"),
        engineDirectory: engine,
        renderTool: engine.appendingPathComponent("tools/render-site.mjs")
      ),
      now: { Date(timeIntervalSince1970: 1_777_777_777) }
    )

    let result = coordinator.renderAndPublish(trigger: "unit-test")
    let current = paths.appSupportDirectory.appendingPathComponent("wiki-site/current", isDirectory: true)
    let html = try String(contentsOf: current.appendingPathComponent("your-context.html"), encoding: .utf8)

    XCTAssertEqual(result.status, WikiRenderStatus.published)
    XCTAssertTrue(html.contains("Swift should render this through the extracted wiki-engine package."))
    XCTAssertTrue(FileManager.default.fileExists(atPath: current.appendingPathComponent("your-context.md").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: current.appendingPathComponent("assets/enhance.js").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: current.appendingPathComponent(".1context/route-manifest.json").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: current.appendingPathComponent(".1context/reference-index.json").path))
  }

  func testValidatorRejectsBrokenMarkdownTwinHash() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let sourceSite = paths.userWikiSiteDirectory
    try writeValidatedSite(at: sourceSite)
    try writeString("---\nslug: your-context\n---\n# Edited\n", to: sourceSite.appendingPathComponent("your-context.md"))

    XCTAssertThrowsError(try WikiSiteValidator().validate(site: sourceSite)) { error in
      guard case WikiSiteValidationError.failed(let message) = error else {
        return XCTFail("Unexpected error: \(error)")
      }
      XCTAssertTrue(message.contains("mismatch"))
    }
  }

  private func temporaryRoot() -> URL {
    URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-wiki-runtime-tests-\(UUID().uuidString)", isDirectory: true)
  }

  private func testRuntimePaths(root: URL) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true)
    )
  }

  private func writeString(_ value: String, to url: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    try RuntimePermissions.writePrivateData(Data(value.utf8), to: url)
  }

  private func fakeEngine(root: URL, exitsWithFailure: Bool) throws -> URL {
    let engine = root.appendingPathComponent("wiki-engine", isDirectory: true)
    let tool = engine.appendingPathComponent("tools/render-site.mjs")
    try writeString(
      exitsWithFailure
        ? "console.error('fixture renderer failed'); process.exit(7);\n"
        : """
          import { createHash } from 'node:crypto';
          import { existsSync, mkdirSync, readdirSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
          import { dirname, join } from 'node:path';
          const argv = process.argv.slice(2);
          const args = {};
          for (let i = 0; i < argv.length; i += 2) args[argv[i].slice(2)] = argv[i + 1];
          const sourceRoot = args['source-root'];
          const out = args.output;
          const resultJson = args['result-json'];
          const countPath = 'render-count.txt';
          const count = existsSync(countPath) ? Number(readFileSync(countPath, 'utf8').trim() || '0') : 0;
          writeFileSync(countPath, String(count + 1));
          function walk(root) {
            const found = [];
            for (const entry of readdirSync(root)) {
              const full = join(root, entry);
              if (statSync(full).isDirectory()) found.push(...walk(full));
              else found.push(full);
            }
            return found;
          }
          const input = walk(sourceRoot).find((path) => path.endsWith('.md') && path.includes('/source/'));
          const raw = readFileSync(input, 'utf8');
          const slugLine = raw.split('\\n').find((line) => line.startsWith('slug:'));
          const slug = slugLine ? slugLine.replace(/^slug:[ \\t]*/, '').replaceAll('"', '').trim() : 'your-context';
          const html = `<!doctype html><title>${slug}</title><span data-tier="private">Private</span><p>rendered ${slug}</p>`;
          rmSync(out, { recursive: true, force: true });
          mkdirSync(join(out, slug), { recursive: true });
          mkdirSync(join(out, 'assets'), { recursive: true });
          mkdirSync(join(out, '.1context'), { recursive: true });
          writeFileSync(join(out, `${slug}.html`), html);
          writeFileSync(join(out, slug, 'index.html'), html);
          writeFileSync(join(out, `${slug}.md`), raw);
          writeFileSync(join(out, 'assets', 'theme.css'), 'asset');
          writeFileSync(join(out, 'assets', 'enhance.js'), 'asset');
          const hash = createHash('sha256').update(raw).digest('hex');
          const route = {
            route: `/${slug}`,
            kind: 'page',
            slug,
            title: 'Your Context',
            access: 'private',
            status: 'published',
            html_path: `${slug}.html`,
            route_index_path: `${slug}/index.html`,
            markdown_path: `${slug}.md`
          };
          const twin = {
            role: 'page-markdown',
            path: `${slug}.md`,
            sha256: hash,
            bytes: Buffer.byteLength(raw),
            content_type: 'text/markdown; charset=utf-8',
            kind: 'page',
            route: `/${slug}`,
            html_path: `${slug}.html`,
            route_index_path: `${slug}/index.html`,
            slug,
            title: 'Your Context',
            access: 'private',
            status: 'published',
            md_url: `/${slug}.md`,
            talk_enabled: true,
            talk_url: null
          };
          const generated_at = '2026-05-14T00:00:00.000Z';
          writeFileSync(join(out, '.1context', 'route-manifest.json'), JSON.stringify({
            schema_version: 'wiki.route-manifest.v1',
            generated_at,
            output: out,
            route_count: 1,
            routes: [route],
            assets: ['assets/theme.css', 'assets/enhance.js']
          }, null, 2));
          writeFileSync(join(out, '.1context', 'content-index.json'), JSON.stringify({
            schema_version: 'wiki.content-index.v1',
            generated_at,
            output: out,
            page_count: 1,
            talk_count: 0,
            markdown_twin_count: 1,
            pages: [route],
            markdown_twins: [twin],
            export_allowlist: ['*.html', '*.md', '*/index.html', 'assets/*', '.1context/route-manifest.json', '.1context/content-index.json']
          }, null, 2));
          mkdirSync(dirname(resultJson), { recursive: true });
          writeFileSync(resultJson, JSON.stringify({
            schema_version: 1,
            status: 'published',
            rendered_at: generated_at,
            source_root: sourceRoot,
            output: out,
            route_manifest: '.1context/route-manifest.json',
            content_index: '.1context/content-index.json',
            route_count: 1,
            markdown_twin_count: 1,
            source_inputs: [input],
            talk_inputs: [],
            source_input_count: 1,
            talk_input_count: 0,
            assets: ['assets/theme.css', 'assets/enhance.js'],
            logs: [{ input, summary: 'fixture render' }]
          }, null, 2));
          """,
      to: tool
    )
    return engine
  }

  private func repoRoot() -> URL {
    URL(fileURLWithPath: #filePath)
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .deletingLastPathComponent()
  }

  private func renderState(at site: URL) throws -> WikiRenderResult {
    let url = site.appendingPathComponent(".1context/current-render.json")
    let data = try Data(contentsOf: url)
    return try JSONDecoder().decode(WikiRenderResult.self, from: data)
  }

  private func writeValidatedSite(at site: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(site)
    let slug = "your-context"
    let access = "private"
    let markdown = "---\ntitle: Your Context\nslug: \(slug)\naccess: \(access)\nstatus: published\nmd_url: /\(slug).md\ntalk_enabled: true\n---\n# Your Context\n"
    let html = "<!doctype html><title>Your Context</title><span data-tier=\"\(access)\">Private</span>"
    try writeString(html, to: site.appendingPathComponent("\(slug).html"))
    try writeString(html, to: site.appendingPathComponent("\(slug)/index.html"))
    try writeString(markdown, to: site.appendingPathComponent("\(slug).md"))
    try RuntimePermissions.ensurePrivateDirectory(site.appendingPathComponent(".1context", isDirectory: true))

    let hash = sha256Hex(Data(markdown.utf8))
    let bytes = Data(markdown.utf8).count
    let routeJSON = """
    {
      "route": "/\(slug)",
      "kind": "page",
      "slug": "\(slug)",
      "title": "Your Context",
      "access": "\(access)",
      "status": "published",
      "html_path": "\(slug).html",
      "route_index_path": "\(slug)/index.html",
      "markdown_path": "\(slug).md"
    }
    """
    let routeManifest = """
    {
      "schema_version": "wiki.route-manifest.v1",
      "generated_at": "2026-05-14T00:00:00.000Z",
      "output": "\(jsonEscape(site.path))",
      "route_count": 1,
      "routes": [\(routeJSON)],
      "assets": [],
      "reference_index": {
        "path": ".1context/reference-index.json",
        "reference_count": 0,
        "asset_count": 0,
        "link_count": 0,
        "code_block_count": 0,
        "citation_count": 0
      }
    }
    """
    let contentIndex = """
    {
      "schema_version": "wiki.content-index.v1",
      "generated_at": "2026-05-14T00:00:00.000Z",
      "output": "\(jsonEscape(site.path))",
      "page_count": 1,
      "talk_count": 0,
      "markdown_twin_count": 1,
      "pages": [\(routeJSON)],
      "markdown_twins": [{
        "role": "page-markdown",
        "path": "\(slug).md",
        "sha256": "\(hash)",
        "bytes": \(bytes),
        "content_type": "text/markdown; charset=utf-8",
        "kind": "page",
        "route": "/\(slug)",
        "html_path": "\(slug).html",
        "route_index_path": "\(slug)/index.html",
        "slug": "\(slug)",
        "title": "Your Context",
        "access": "\(access)",
        "status": "published",
        "md_url": "/\(slug).md",
        "talk_enabled": true,
        "talk_url": null
      }],
      "reference_index": {
        "path": ".1context/reference-index.json",
        "reference_count": 0,
        "asset_count": 0,
        "link_count": 0,
        "code_block_count": 0,
        "citation_count": 0
      },
      "export_allowlist": ["*.html", "*.md", "*/index.html", "assets/*", ".1context/route-manifest.json", ".1context/content-index.json", ".1context/reference-index.json"]
    }
    """
    let referenceIndex = """
    {
      "schema_version": "wiki.reference-index.v1",
      "generated_at": "2026-05-14T00:00:00.000Z",
      "output": "\(jsonEscape(site.path))",
      "reference_count": 0,
      "asset_count": 0,
      "link_count": 0,
      "code_block_count": 0,
      "citation_count": 0,
      "assets": [],
      "links": [],
      "code_blocks": [],
      "citations": []
    }
    """
    try writeString(routeManifest, to: site.appendingPathComponent(".1context/route-manifest.json"))
    try writeString(contentIndex, to: site.appendingPathComponent(".1context/content-index.json"))
    try writeString(referenceIndex, to: site.appendingPathComponent(".1context/reference-index.json"))
  }

  private func sha256Hex(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
  }

  private func jsonEscape(_ value: String) -> String {
    value
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
  }
}
