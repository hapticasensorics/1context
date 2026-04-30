import XCTest
@testable import OneContextLocalWeb
import OneContextPlatform

final class LocalWebTests: XCTestCase {
  func testCaddyConfigIsOpinionatedLocalWebEdge() {
    let config = CaddyConfig(
      siteRoot: URL(fileURLWithPath: "/tmp/1Context Wiki/current", isDirectory: true),
      logFile: URL(fileURLWithPath: "/tmp/1Context Logs/caddy.log")
    )

    let text = config.caddyfileText()
    XCTAssertTrue(text.contains("admin off"))
    XCTAssertTrue(text.contains("auto_https off"))
    XCTAssertTrue(text.contains("http://wiki.1context.localhost:17319, http://127.0.0.1:17319"))
    XCTAssertTrue(text.contains("bind 127.0.0.1"))
    XCTAssertTrue(text.contains("root * \"/tmp/1Context Wiki/current\""))
    XCTAssertTrue(text.contains("route {"))
    XCTAssertTrue(text.contains("@wikiDynamicApi path /api/wiki/health /api/wiki/search /api/wiki/state /api/wiki/bookmarks /api/wiki/chat/config /api/wiki/chat/provider /api/wiki/chat/reset /api/wiki/chat"))
    XCTAssertLessThan(
      try XCTUnwrap(text.range(of: "@wikiDynamicApi path")?.lowerBound),
      try XCTUnwrap(text.range(of: "try_files {path}")?.lowerBound)
    )
    XCTAssertTrue(text.contains("try_files {path} {path}.html {path}/index.html /index.html"))
    XCTAssertTrue(text.contains("file_server"))
    XCTAssertTrue(text.contains("reverse_proxy 127.0.0.1:17320"))
    XCTAssertTrue(text.contains("rewrite /api/wiki/pages /api/wiki/pages.json"))
    XCTAssertFalse(text.contains("rewrite /api/wiki/search /api/wiki/search.json"))
    XCTAssertFalse(text.contains("respond `"))
    XCTAssertEqual(config.url, "http://wiki.1context.localhost:17319/your-context")
    XCTAssertEqual(config.healthURL.absoluteString, "http://127.0.0.1:17319/__1context/health")
  }

  func testLocalWebPathsUseDedicatedInfrastructureFolders() {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-tests-\(UUID().uuidString)", isDirectory: true)
    let paths = RuntimePaths.current(environment: [
      "ONECONTEXT_APP_SUPPORT_DIR": root.appendingPathComponent("Application Support/1Context").path,
      "ONECONTEXT_LOG_DIR": root.appendingPathComponent("Logs/1Context").path,
      "ONECONTEXT_CACHE_DIR": root.appendingPathComponent("Caches/1Context").path
    ])
    let web = LocalWebPaths(runtimePaths: paths)

    XCTAssertTrue(web.caddyDirectory.path.hasSuffix("Application Support/1Context/local-web/caddy"))
    XCTAssertTrue(web.wikiCurrent.path.hasSuffix("Application Support/1Context/wiki-site/current"))
    XCTAssertTrue(web.wikiNext.path.hasSuffix("Application Support/1Context/wiki-site/next"))
    XCTAssertTrue(web.wikiPrevious.path.hasSuffix("Application Support/1Context/wiki-site/previous"))
    XCTAssertTrue(web.pidFile.path.hasSuffix("Application Support/1Context/run/local-web-caddy.pid"))
  }

  func testDiagnosticsReportsConfiguredCaddyAndSitePaths() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-diagnostics-\(UUID().uuidString)", isDirectory: true)
    let caddy = root.appendingPathComponent("bin/caddy")
    try FileManager.default.createDirectory(at: caddy.deletingLastPathComponent(), withIntermediateDirectories: true)
    FileManager.default.createFile(atPath: caddy.path, contents: Data("#!/bin/sh\n".utf8))
    chmod(caddy.path, 0o755)

    let paths = RuntimePaths.current(environment: [
      "ONECONTEXT_APP_SUPPORT_DIR": root.appendingPathComponent("Application Support/1Context").path,
      "ONECONTEXT_LOG_DIR": root.appendingPathComponent("Logs/1Context").path,
      "ONECONTEXT_CACHE_DIR": root.appendingPathComponent("Caches/1Context").path
    ])
    let manager = CaddyManager(runtimePaths: paths, environment: ["ONECONTEXT_CADDY_PATH": caddy.path])
    let diagnostics = manager.diagnostics()

    XCTAssertEqual(diagnostics.caddyExecutable, caddy.path)
    XCTAssertTrue(diagnostics.caddyExecutableExists)
    XCTAssertTrue(diagnostics.caddyExecutableIsExecutable)
    XCTAssertFalse(diagnostics.caddyExecutableIsBundled)
    XCTAssertTrue(diagnostics.currentSitePath.hasSuffix("Application Support/1Context/wiki-site/current"))
    XCTAssertTrue(diagnostics.caddyfilePath.hasSuffix("Application Support/1Context/local-web/caddy/Caddyfile"))
    XCTAssertEqual(diagnostics.apiPort, LocalWebDefaults.wikiAPIPort)
    XCTAssertTrue(diagnostics.apiStatePath.hasSuffix("Application Support/1Context/local-web/wiki-browser-state.json"))
  }

  func testLegacyPythonWikiServerMigrationRemovesRetiredServerArtifacts() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let paths = testRuntimePaths(root: root)
    let legacyRoot = paths.appSupportDirectory.appendingPathComponent("memory-core", isDirectory: true)
    let legacySource = legacyRoot.appendingPathComponent("core/src/onectx/wiki", isDirectory: true)
    try FileManager.default.createDirectory(at: legacySource, withIntermediateDirectories: true)
    try Data(#"{"pid":999999}"#.utf8).write(to: legacyRoot.appendingPathComponent("wiki-server.json"))
    try Data("old server log\n".utf8).write(to: legacyRoot.appendingPathComponent("wiki-server.log"))
    try Data("old server\n".utf8).write(to: legacySource.appendingPathComponent("server.py"))
    try Data("old serve main\n".utf8).write(to: legacySource.appendingPathComponent("serve_main.py"))

    let result = LegacyPythonWikiServerMigration.run(runtimePaths: paths)

    XCTAssertNil(result.stoppedPID)
    XCTAssertEqual(result.removedPaths.count, 4)
    XCTAssertFalse(FileManager.default.fileExists(atPath: legacyRoot.appendingPathComponent("wiki-server.json").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: legacyRoot.appendingPathComponent("wiki-server.log").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: legacySource.appendingPathComponent("server.py").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: legacySource.appendingPathComponent("serve_main.py").path))
  }

  func testWikiLocalAPISearchesPublishedContentIndex() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let web = LocalWebPaths(runtimePaths: paths)
    try RuntimePermissions.ensurePrivateDirectory(web.wikiCurrent)
    let index: [String: Any] = [
      "pages": [
        ["title": "Channel Tunnel Notes", "route": "/for-you", "description": "English and French engineering alignment"],
        ["title": "Release Bones", "route": "/projects", "description": "Packaging and daemon work"]
      ]
    ]
    try writeJSON(index, to: web.wikiCurrent.appendingPathComponent("content-index.json"))

    let handler = WikiLocalAPIHandler(paths: web)
    let response = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/search", query: ["q": "french"]))
    let payload = try XCTUnwrap(json(response))
    let matches = try XCTUnwrap(payload["matches"] as? [[String: Any]])

    XCTAssertEqual(response.statusCode, 200)
    XCTAssertEqual(matches.count, 1)
    XCTAssertEqual(matches.first?["route"] as? String, "/for-you")
  }

  func testWikiLocalAPIStatePersistsAndRejectsOversizedPayloads() throws {
    let root = temporaryRoot()
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: root))
    let handler = WikiLocalAPIHandler(paths: web)
    let body = Data(#"{"settings":{"theme":"dark"},"bookmarks":[{"title":"For You","url":"/for-you"}]}"#.utf8)

    let saved = handler.handle(WikiLocalAPIRequest(method: "PATCH", path: "/api/wiki/state", body: body))
    let loaded = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/state"))
    let loadedPayload = try XCTUnwrap(json(loaded))
    let settings = try XCTUnwrap(loadedPayload["settings"] as? [String: Any])
    let bookmarks = try XCTUnwrap(loadedPayload["bookmarks"] as? [[String: Any]])

    XCTAssertEqual(saved.statusCode, 200)
    XCTAssertEqual(settings["theme"] as? String, "dark")
    XCTAssertEqual(bookmarks.first?["url"] as? String, "/for-you")

    let oversized = Data(repeating: UInt8(ascii: "x"), count: WikiLocalAPIHandler.maxStateBodyBytes + 1)
    let rejected = handler.handle(WikiLocalAPIRequest(method: "POST", path: "/api/wiki/state", body: oversized))
    XCTAssertEqual(rejected.statusCode, 413)
  }

  func testWikiLocalAPIChatConfigIsGracefulUnavailableShell() throws {
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: temporaryRoot()))
    let handler = WikiLocalAPIHandler(paths: web)

    let config = try XCTUnwrap(json(handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/chat/config"))))
    let chat = try XCTUnwrap(json(handler.handle(WikiLocalAPIRequest(method: "POST", path: "/api/wiki/chat", body: Data(#"{"message":"hello"}"#.utf8)))))

    XCTAssertEqual(config["enabled"] as? Bool, false)
    XCTAssertEqual(config["chat_available"] as? Bool, false)
    XCTAssertEqual(chat["enabled"] as? Bool, false)
    XCTAssertTrue((chat["text"] as? String ?? "").contains("not enabled"))
  }

  private func temporaryRoot() -> URL {
    URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-tests-\(UUID().uuidString)", isDirectory: true)
  }

  private func testRuntimePaths(root: URL) -> RuntimePaths {
    RuntimePaths.current(environment: [
      "ONECONTEXT_APP_SUPPORT_DIR": root.appendingPathComponent("Application Support/1Context").path,
      "ONECONTEXT_LOG_DIR": root.appendingPathComponent("Logs/1Context").path,
      "ONECONTEXT_CACHE_DIR": root.appendingPathComponent("Caches/1Context").path
    ])
  }

  private func writeJSON(_ payload: [String: Any], to url: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    let data = try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys])
    try RuntimePermissions.writePrivateData(data, to: url)
  }

  private func json(_ response: WikiLocalAPIResponse) throws -> [String: Any]? {
    try JSONSerialization.jsonObject(with: response.body) as? [String: Any]
  }
}
