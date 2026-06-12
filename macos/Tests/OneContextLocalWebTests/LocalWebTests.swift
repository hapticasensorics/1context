import XCTest
import Darwin
@testable import OneContextLocalWeb
import OneContextPlatform

final class LocalWebTests: XCTestCase {
  func testCaddyConfigSupportsProfessionalLocalHTTPSMode() {
    let config = CaddyConfig(
      mode: .localHTTPSPortless,
      siteRoot: URL(fileURLWithPath: "/tmp/1Context Wiki/current", isDirectory: true),
      logFile: URL(fileURLWithPath: "/tmp/1Context Logs/caddy.log")
    )

    let text = config.caddyfileText()
    XCTAssertTrue(text.contains("admin off"))
    XCTAssertTrue(text.contains("skip_install_trust"))
    XCTAssertTrue(text.contains("auto_https disable_redirects"))
    XCTAssertTrue(text.contains("https://127.0.0.1:39191"))
    XCTAssertTrue(text.contains("https://localhost:39191"))
    XCTAssertTrue(text.contains("https://wiki.1context.localhost:39191"))
    XCTAssertTrue(text.contains("bind 127.0.0.1"))
    XCTAssertTrue(text.contains("tls internal"))
    XCTAssertTrue(text.contains("@memoryApi path /memory /api/memory/*"))
    XCTAssertFalse(text.contains("auto_https off"))
    XCTAssertTrue(text.contains(":39191"))
    XCTAssertEqual(config.url, "https://localhost/your-context")
    XCTAssertEqual(config.healthURL.absoluteString, "https://127.0.0.1:39191/__1context/health")
    XCTAssertEqual(config.privilegedProxyHealthURL.absoluteString, "https://localhost/__1context/health")
    XCTAssertEqual(config.brandedHealthURL.absoluteString, "https://wiki.1context.localhost/__1context/health")
  }

  func testCaddyConfigSupportsSideBySideDevHTTPMode() {
    let config = CaddyConfig(
      mode: .localHTTPPorted,
      siteRoot: URL(fileURLWithPath: "/tmp/1Context Dev Wiki/current", isDirectory: true),
      logFile: URL(fileURLWithPath: "/tmp/1Context Dev Logs/caddy.log"),
      host: "wiki-dev.1context.localhost",
      port: 39291,
      apiPort: 39292
    )

    let text = config.caddyfileText()
    XCTAssertTrue(text.contains("admin off"))
    XCTAssertTrue(text.contains("auto_https off"))
    XCTAssertTrue(text.contains("http://127.0.0.1:39291"))
    XCTAssertTrue(text.contains("http://localhost:39291"))
    XCTAssertTrue(text.contains("http://wiki-dev.1context.localhost:39291"))
    XCTAssertTrue(text.contains("bind 127.0.0.1"))
    XCTAssertTrue(text.contains("@memoryApi path /memory /api/memory/*"))
    XCTAssertFalse(text.contains("tls internal"))
    XCTAssertEqual(config.url, "http://localhost:39291/your-context")
    XCTAssertEqual(config.healthURL.absoluteString, "http://127.0.0.1:39291/__1context/health")
    XCTAssertEqual(config.privilegedProxyHealthURL.absoluteString, "http://127.0.0.1:39291/__1context/health")
    XCTAssertEqual(config.brandedHealthURL.absoluteString, "http://wiki-dev.1context.localhost:39291/__1context/health")
  }

  func testDefaultURLModeRequiresProfessionalLocalHTTPSSetup() {
    let mode = LocalWebURLMode.localHTTPSPortless
    let config = CaddyConfig(
      mode: mode,
      siteRoot: URL(fileURLWithPath: "/tmp/1Context Wiki/current", isDirectory: true),
      logFile: URL(fileURLWithPath: "/tmp/1Context Logs/caddy.log")
    )

    XCTAssertEqual(mode, .localHTTPSPortless)
    XCTAssertEqual(LocalWebDefaults.defaultWikiURL, "https://localhost/your-context")
    XCTAssertEqual(LocalWebDefaults.brandedWikiURL, "https://wiki.1context.localhost/your-context")
    XCTAssertEqual(config.url, LocalWebDefaults.defaultWikiURL)
  }

  func testReadinessProbeUsesLiteralLoopbackInsteadOfBrandedLocalhostName() {
    let config = CaddyConfig(
      mode: .localHTTPSPortless,
      siteRoot: URL(fileURLWithPath: "/tmp/1Context Wiki/current", isDirectory: true),
      logFile: URL(fileURLWithPath: "/tmp/1Context Logs/caddy.log")
    )

    XCTAssertEqual(config.url, "https://localhost/your-context")
    XCTAssertEqual(config.healthURL.host, "127.0.0.1")
    XCTAssertEqual(config.brandedHealthURL.host, "wiki.1context.localhost")
  }

  func testLocalHealthProbeSessionBypassesSystemProxyAndCache() {
    let configuration = CaddyManager.localHealthProbeSessionConfiguration()

    XCTAssertNotNil(configuration.connectionProxyDictionary)
    XCTAssertEqual(configuration.connectionProxyDictionary?.isEmpty, true)
    XCTAssertEqual(configuration.requestCachePolicy, .reloadIgnoringLocalCacheData)
    XCTAssertNil(configuration.urlCache)
  }

  func testLocalWebPathsUseDedicatedInfrastructureFolders() {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-tests-\(UUID().uuidString)", isDirectory: true)
    let paths = testRuntimePaths(root: root)
    let web = LocalWebPaths(runtimePaths: paths)

    XCTAssertTrue(web.caddyDirectory.path.hasSuffix("Application Support/1Context/local-web/caddy"))
    XCTAssertTrue(web.wikiCurrent.path.hasSuffix("Application Support/1Context/wiki-site/current"))
    XCTAssertTrue(web.wikiNext.path.hasSuffix("Application Support/1Context/wiki-site/next"))
    XCTAssertTrue(web.wikiPrevious.path.hasSuffix("Application Support/1Context/wiki-site/previous"))
    XCTAssertTrue(web.pidFile.path.hasSuffix("Application Support/1Context/run/local-web-caddy.pid"))
  }

  func testLocalWebSetupResolvesCLISymlinkToInstalledApp() throws {
    let root = temporaryRoot()
    let app = root.appendingPathComponent("Applications/1Context.app", isDirectory: true)
    let cli = app.appendingPathComponent("Contents/MacOS/1context-cli")
    let link = root.appendingPathComponent("linked-bin/1context")
    try FileManager.default.createDirectory(at: cli.deletingLastPathComponent(), withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: link.deletingLastPathComponent(), withIntermediateDirectories: true)
    FileManager.default.createFile(atPath: cli.path, contents: Data("#!/bin/sh\n".utf8))
    try FileManager.default.createSymbolicLink(at: link, withDestinationURL: cli)

    let resolved = LocalWebSetupSystemPaths.appBundleURL(
      commandLineExecutable: link.path,
      mainBundleURL: root.appendingPathComponent("linked-bin", isDirectory: true)
    )

    XCTAssertEqual(resolved.path, app.path)
  }

  func testDiagnosticsReportsConfiguredCaddyAndSitePaths() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-diagnostics-\(UUID().uuidString)", isDirectory: true)
    let caddy = root.appendingPathComponent("bin/caddy")
    try FileManager.default.createDirectory(at: caddy.deletingLastPathComponent(), withIntermediateDirectories: true)
    FileManager.default.createFile(atPath: caddy.path, contents: Data("#!/bin/sh\n".utf8))
    chmod(caddy.path, 0o755)

    let paths = testRuntimePaths(root: root)
    let manager = CaddyManager(
      runtimePaths: paths,
      setupSystemPaths: testSetupSystemPaths(root: root),
      caddyExecutable: caddy
    )
    let diagnostics = manager.diagnostics()

    XCTAssertEqual(diagnostics.urlMode, "local-https-portless")
    XCTAssertEqual(diagnostics.trustMode, "local-ca-required")
    XCTAssertTrue(diagnostics.privilegedBindRequired)
    XCTAssertEqual(diagnostics.caddyExecutable, caddy.path)
    XCTAssertTrue(diagnostics.caddyExecutableExists)
    XCTAssertTrue(diagnostics.caddyExecutableIsExecutable)
    XCTAssertFalse(diagnostics.caddyExecutableIsBundled)
    XCTAssertNil(diagnostics.runningCaddyExecutable)
    XCTAssertTrue(diagnostics.currentSitePath.hasSuffix("Application Support/1Context/wiki-site/current"))
    XCTAssertTrue(diagnostics.caddyfilePath.hasSuffix("Application Support/1Context/local-web/caddy/Caddyfile"))
    XCTAssertEqual(diagnostics.apiPort, LocalWebDefaults.wikiAPIPort)
    XCTAssertTrue(diagnostics.apiStatePath.hasSuffix("Application Support/1Context/local-web/wiki-browser-state.json"))
    XCTAssertEqual(diagnostics.readinessProbeURL, "https://127.0.0.1:39191/__1context/health")
    XCTAssertEqual(diagnostics.readinessProbeHealth, "setup required")
    XCTAssertEqual(diagnostics.privilegedProxyProbeURL, "https://localhost/__1context/health")
    XCTAssertEqual(diagnostics.privilegedProxyProbeHealth, "setup required")
    XCTAssertEqual(diagnostics.brandedProbeURL, "https://wiki.1context.localhost/__1context/health")
    XCTAssertEqual(diagnostics.brandedProbeHealth, "setup required")
  }

  func testDiagnosticsDoesNotTreatExecutableDirectoryAsCaddy() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-caddy-dir-\(UUID().uuidString)", isDirectory: true)
    let caddyDirectory = root.appendingPathComponent("bin/caddy", isDirectory: true)
    try FileManager.default.createDirectory(at: caddyDirectory, withIntermediateDirectories: true)
    chmod(caddyDirectory.path, 0o755)

    let manager = CaddyManager(
      runtimePaths: testRuntimePaths(root: root),
      setupSystemPaths: testSetupSystemPaths(root: root),
      caddyExecutable: caddyDirectory
    )
    let diagnostics = manager.diagnostics()

    XCTAssertEqual(diagnostics.caddyExecutable, "")
    XCTAssertFalse(diagnostics.caddyExecutableExists)
    XCTAssertFalse(diagnostics.caddyExecutableIsExecutable)
  }

  func testDiagnosticsReportsLocalHTTPSURLMode() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-url-mode-\(UUID().uuidString)", isDirectory: true)
    let caddy = root.appendingPathComponent("bin/caddy")
    try FileManager.default.createDirectory(at: caddy.deletingLastPathComponent(), withIntermediateDirectories: true)
    FileManager.default.createFile(atPath: caddy.path, contents: Data("#!/bin/sh\n".utf8))
    chmod(caddy.path, 0o755)

    let paths = testRuntimePaths(root: root)
    let manager = CaddyManager(
      runtimePaths: paths,
      setupSystemPaths: testSetupSystemPaths(root: root),
      caddyExecutable: caddy
    )

    let diagnostics = manager.diagnostics()

    XCTAssertEqual(diagnostics.snapshot.url, "https://localhost/your-context")
    XCTAssertEqual(diagnostics.urlMode, "local-https-portless")
    XCTAssertEqual(diagnostics.trustMode, "local-ca-required")
    XCTAssertTrue(diagnostics.privilegedBindRequired)
  }

  func testStatusReportsTargetURLWhenStoredStateUsesDifferentMode() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let web = LocalWebPaths(runtimePaths: paths)
    try RuntimePermissions.ensurePrivateDirectory(web.caddyDirectory)
    try writeJSON([
      "schema_version": 1,
      "pid": 999999,
      "url": "http://wiki.1context.localhost:39191/your-context",
      "caddy_executable": "/tmp/caddy",
      "started_at": "2026-04-30T00:00:00Z"
    ], to: web.stateFile)

    let manager = CaddyManager(runtimePaths: paths, setupSystemPaths: testSetupSystemPaths(root: root))
    let snapshot = manager.status()

    XCTAssertFalse(snapshot.running)
    XCTAssertEqual(snapshot.health, "setup required")
    XCTAssertEqual(snapshot.url, "https://localhost/your-context")
    XCTAssertEqual(snapshot.lastError, "Local web setup required: Local HTTPS helper, Local certificate trust")
  }

  func testStartRequiresSetupBeforeProfessionalLocalHTTPSModeRuns() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let manager = CaddyManager(runtimePaths: paths, setupSystemPaths: testSetupSystemPaths(root: root))

    XCTAssertThrowsError(try manager.start()) { error in
      XCTAssertEqual(
        error as? LocalWebError,
        .setupRequired("Local web setup required: Local HTTPS helper, Local certificate trust")
      )
    }
  }

  func testStaticSupportFilesDoNotPublishPlaceholderPages() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let web = LocalWebPaths(runtimePaths: paths)
    let manager = CaddyManager(runtimePaths: paths)

    try manager.ensureStaticSupportFiles()

    XCTAssertTrue(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("__1context/health").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("api/wiki/search.json").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("api/wiki/bookmarks.json").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("index.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("your-context.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("for-you.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("projects.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("topics.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("goal.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("goal.md").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("your-context.md").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("render-manifest.json").path))
  }

  func testStaticSupportFilesPreserveLastGoodRenderedSite() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let web = LocalWebPaths(runtimePaths: paths)
    let manager = CaddyManager(runtimePaths: paths)
    try writeString("last-good", to: web.wikiCurrent.appendingPathComponent("your-context.html"))
    try writeString("last-good-md", to: web.wikiCurrent.appendingPathComponent("your-context.md"))

    try manager.ensureStaticSupportFiles()

    XCTAssertEqual(
      try String(contentsOf: web.wikiCurrent.appendingPathComponent("your-context.html"), encoding: .utf8),
      "last-good"
    )
    XCTAssertEqual(
      try String(contentsOf: web.wikiCurrent.appendingPathComponent("your-context.md"), encoding: .utf8),
      "last-good-md"
    )
    XCTAssertTrue(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("__1context/health").path))
  }

  func testLocalHTTPSSetupSnapshotReflectsInstalledProxyAndTrust() {
    let systemPaths = testSetupSystemPaths(root: URL(fileURLWithPath: "/tmp/1Context", isDirectory: true))
    let state = LocalWebSetupState(
      label: LocalWebSetupConstants.proxyLabel,
      targetHost: LocalWebDefaults.wikiHost,
      targetURL: LocalWebDefaults.defaultWikiURL,
      backendHost: LocalWebDefaults.bindHost,
      backendPort: LocalWebDefaults.wikiPort,
      privilegedPort: LocalWebSetupConstants.privilegedHTTPSPort,
      sourceProxyExecutablePath: "/tmp/source-proxy",
      sourceProxyExecutableSHA256: "PROXY123",
      installedProxyExecutableSHA256: "PROXY123",
      userRootCertificatePath: "/tmp/root.crt",
      userRootCertificateExists: true,
      userRootCertificateSHA1: "ABC123",
      userRootCertificateSHA256: "DEF456",
      systemPaths: systemPaths,
      proxyPlistInstalled: true,
      proxyExecutableInstalled: true,
      proxyServiceStatus: "enabled",
      proxyLaunchDaemonLoaded: true,
      proxyPortReachable: true,
      trustedRootCertificateInstalled: true,
      trustedRootSHA1: "ABC123",
      trustedRootSHA256: "DEF456"
    )

    let snapshot = LocalWebSetupSnapshot.localHTTPSPortless(
      targetURL: LocalWebDefaults.defaultWikiURL,
      state: state
    )

    XCTAssertTrue(snapshot.ready)
    XCTAssertEqual(snapshot.blockingSummary, "Local web setup is complete.")
    XCTAssertTrue(snapshot.requirements.allSatisfy { $0.status == .satisfied })
  }

  func testLocalHTTPSSetupRequiresRepairWhenInstalledProxyIsStale() {
    let systemPaths = testSetupSystemPaths(root: URL(fileURLWithPath: "/tmp/1Context", isDirectory: true))
    let state = LocalWebSetupState(
      label: LocalWebSetupConstants.proxyLabel,
      targetHost: LocalWebDefaults.wikiHost,
      targetURL: LocalWebDefaults.defaultWikiURL,
      backendHost: LocalWebDefaults.bindHost,
      backendPort: LocalWebDefaults.wikiPort,
      privilegedPort: LocalWebSetupConstants.privilegedHTTPSPort,
      sourceProxyExecutablePath: "/tmp/source-proxy",
      sourceProxyExecutableSHA256: "NEW",
      installedProxyExecutableSHA256: "OLD",
      userRootCertificatePath: "/tmp/root.crt",
      userRootCertificateExists: true,
      userRootCertificateSHA1: "ABC123",
      userRootCertificateSHA256: "DEF456",
      systemPaths: systemPaths,
      proxyPlistInstalled: true,
      proxyExecutableInstalled: true,
      proxyServiceStatus: "enabled",
      proxyLaunchDaemonLoaded: true,
      proxyPortReachable: true,
      trustedRootCertificateInstalled: true,
      trustedRootSHA1: "ABC123",
      trustedRootSHA256: "DEF456"
    )

    let snapshot = LocalWebSetupSnapshot.localHTTPSPortless(
      targetURL: LocalWebDefaults.defaultWikiURL,
      state: state
    )

    XCTAssertFalse(snapshot.ready)
    XCTAssertEqual(snapshot.blockingSummary, "Local web setup required: Local HTTPS helper")
    XCTAssertTrue(snapshot.requirements.first?.details.contains("Proxy current: no") == true)
  }

  func testLocalHTTPSSetupDiagnosticsRedactEmbeddedPaths() {
    let snapshot = localHTTPSSetupSnapshot(
      sourceProxySHA256: "NEW",
      installedProxySHA256: "OLD"
    )

    let text = LocalWebSetupDiagnostics.render(snapshot) {
      $0.replacingOccurrences(of: "/tmp/1Context", with: "$ONECONTEXT_SYSTEM")
    }.joined(separator: "\n")

    XCTAssertFalse(text.contains("/tmp/1Context"))
    XCTAssertTrue(text.contains("$ONECONTEXT_SYSTEM/LaunchDaemons/proxy.plist"))
    XCTAssertTrue(text.contains("$ONECONTEXT_SYSTEM/System/local-web-root.sha256"))
  }

  func testLocalHTTPSSetupAppReplacementDetectsStaleProxyAndRepairRestoresReadiness() {
    let staleAfterReplacement = localHTTPSSetupSnapshot(
      sourceProxySHA256: "NEW_PROXY_FROM_REPLACED_APP",
      installedProxySHA256: "OLD_PROXY_FROM_PREVIOUS_APP"
    )

    XCTAssertFalse(staleAfterReplacement.ready)
    XCTAssertEqual(staleAfterReplacement.blockingSummary, "Local web setup required: Local HTTPS helper")
    XCTAssertTrue(staleAfterReplacement.requirements.first?.details.contains("Proxy current: no") == true)
    XCTAssertEqual(
      staleAfterReplacement.requirements.first?.nextAction,
      "Open 1Context and choose Settings > Setup... If macOS opens System Settings, allow 1Context."
    )

    let repairedAfterReplacement = localHTTPSSetupSnapshot(
      sourceProxySHA256: "NEW_PROXY_FROM_REPLACED_APP",
      installedProxySHA256: "NEW_PROXY_FROM_REPLACED_APP"
    )

    XCTAssertTrue(repairedAfterReplacement.ready)
    XCTAssertEqual(repairedAfterReplacement.blockingSummary, "Local web setup is complete.")
    XCTAssertTrue(repairedAfterReplacement.requirements.first?.details.contains("Proxy current: yes") == true)
  }

  func testWikiLocalAPISearchesPublishedContentIndex() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let web = LocalWebPaths(runtimePaths: paths)
    try RuntimePermissions.ensurePrivateDirectory(web.wikiCurrent)
    let index: [String: Any] = [
      "pages": [
        [
          "title": "Channel Tunnel Notes",
          "route": "/for-you",
          "description": "English and French engineering alignment",
          "markdown_path": "for-you.md"
        ],
        ["title": "Release Bones", "route": "/projects", "description": "Packaging and daemon work"]
      ]
    ]
    try writeJSON(index, to: web.wikiCurrent.appendingPathComponent(".1context/content-index.json"))
    try writeString(
      """
      ---
      title: Channel Tunnel Notes
      ---

      # Channel Tunnel Notes

      A quiet insight about alignment only appears in the markdown body.
      """,
      to: web.wikiCurrent.appendingPathComponent("for-you.md")
    )

    let handler = WikiLocalAPIHandler(paths: web)
    let response = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/search", query: ["q": "french"]))
    let payload = try XCTUnwrap(json(response))
    let matches = try XCTUnwrap(payload["matches"] as? [[String: Any]])

    XCTAssertEqual(response.statusCode, 200)
    XCTAssertEqual(matches.count, 1)
    XCTAssertEqual(matches.first?["route"] as? String, "/for-you")

    let bodyResponse = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/search", query: ["q": "quiet insight"]))
    let bodyPayload = try XCTUnwrap(json(bodyResponse))
    let bodyMatches = try XCTUnwrap(bodyPayload["matches"] as? [[String: Any]])
    XCTAssertEqual(bodyMatches.count, 1)
    XCTAssertEqual(bodyMatches.first?["route"] as? String, "/for-you")
    XCTAssertTrue((bodyMatches.first?["excerpt"] as? String ?? "").contains("quiet insight"))
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

  func testMemoryViewerAndAPIRoutesUseInjectedProvider() throws {
    let root = temporaryRoot()
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: root))
    let handler = WikiLocalAPIHandler(
      paths: web,
      memoryStatus: {
        [
          "surface": "memory_daemon_status",
          "running": true,
          "viewport_path": root.appendingPathComponent("1Context/context-engine/memory-db/viewport/local-source-records.jsonl").path,
          "status": ["elapsed_ms": 12]
        ]
      },
      memoryViewport: { query in
        [
          "schema_version": 1,
          "surface": "memory_viewport",
          "protocol": "memory.queryViewport.v1",
          "source": query["source"] ?? "all",
          "record_count": 1,
          "sources": ["codex.local_sessions"],
          "records": [
            [
              "source": "codex.local_sessions",
              "source_uri": root.appendingPathComponent(".codex/sessions/rollout.jsonl").path,
              "record": [
                "connector_key": "codex.local_sessions",
                "source_uri": root.appendingPathComponent(".codex/sessions/rollout.jsonl").path,
                "envelope": [
                  "kind": "codex_message",
                  "event_start": "2026-05-24T12:00:00Z",
                  "event_end": "2026-05-24T12:00:01Z",
                  "display_title": "Codex message",
                  "display_text": "hello"
                ],
                "blob_uri": "file://\(root.appendingPathComponent("1Context/context-engine/blobs/blob-1.txt").path)"
              ]
            ]
          ]
        ]
      },
      memoryObject: { query in
        [
          "schema_version": 1,
          "surface": "memory_object_hydration",
          "protocol": "memory.hydrateObjects.v1",
          "object_id": query["object_id"] ?? "",
          "object": [
            "object_id": query["object_id"] ?? "",
            "source_uri": root.appendingPathComponent(".codex/sessions/rollout.jsonl").path,
            "display_title": "Codex message detail"
          ]
        ]
      },
      memoryDensity: { _ in
        [
          "schema_version": 1,
          "surface": "memory_density",
          "protocol": "memory.queryDensity.v1",
          "buckets": [
            ["bucket_start": "2026-05-24T12:00:00Z", "object_count": 1]
          ]
        ]
      },
      memoryEdges: { query in
        [
          "schema_version": 1,
          "surface": "memory_edges",
          "protocol": "memory.queryEdges.v1",
          "object_id": query["object_id"] ?? "",
          "edges": []
        ]
      },
      memorySearch: { query in
        [
          "schema_version": 1,
          "surface": "memory_search",
          "protocol": "memory.searchText.v1",
          "query": query["q"] ?? query["query"] ?? "",
          "objects": []
        ]
      }
    )

    let status = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/memory/status"))
    let viewport = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/memory/viewport", query: ["source": "codex.local_sessions"]))
    let object = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/memory/object", query: ["object_id": "object-123"]))
    let density = handler.handle(WikiLocalAPIRequest(method: "POST", path: "/api/memory/density", body: Data(#"{"source":"codex.local_sessions"}"#.utf8)))
    let edges = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/memory/edges", query: ["object_id": "object-123"]))
    let memorySearch = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/memory/search", query: ["q": "hello"]))
    let removedRoute = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/memory/traces"))
    let viewer = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/memory"))
    let statusPayload = try XCTUnwrap(json(status))
    let viewportPayload = try XCTUnwrap(json(viewport))
    let objectPayload = try XCTUnwrap(json(object))
    let densityPayload = try XCTUnwrap(json(density))
    let edgesPayload = try XCTUnwrap(json(edges))
    let memorySearchPayload = try XCTUnwrap(json(memorySearch))
    let html = String(data: viewer.body, encoding: .utf8) ?? ""

    XCTAssertEqual(status.statusCode, 200)
    XCTAssertEqual(statusPayload["surface"] as? String, "memory_daemon_status")
    XCTAssertEqual(viewport.statusCode, 200)
    XCTAssertEqual(viewportPayload["surface"] as? String, "memory_viewport")
    XCTAssertEqual(viewportPayload["protocol"] as? String, "memory.queryViewport.v1")
    XCTAssertEqual(viewportPayload["source"] as? String, "codex.local_sessions")
    XCTAssertEqual(viewportPayload["record_count"] as? Int, 1)
    XCTAssertEqual(object.statusCode, 200)
    XCTAssertEqual(objectPayload["protocol"] as? String, "memory.hydrateObjects.v1")
    XCTAssertEqual(density.statusCode, 200)
    XCTAssertEqual(densityPayload["protocol"] as? String, "memory.queryDensity.v1")
    XCTAssertEqual(edges.statusCode, 200)
    XCTAssertEqual(edgesPayload["protocol"] as? String, "memory.queryEdges.v1")
    XCTAssertEqual(memorySearch.statusCode, 200)
    XCTAssertEqual(memorySearchPayload["protocol"] as? String, "memory.searchText.v1")
    XCTAssertEqual(removedRoute.statusCode, 404)
    XCTAssertFalse((String(data: status.body + viewport.body + object.body + density.body + edges.body + memorySearch.body, encoding: .utf8) ?? "").contains(root.path))
    XCTAssertFalse((String(data: viewport.body, encoding: .utf8) ?? "").contains("file://"))
    XCTAssertEqual(viewer.statusCode, 200)
    XCTAssertTrue(viewer.headers["Content-Type"]?.contains("text/html") == true)
    XCTAssertTrue(html.contains("1Context Memory"))
    XCTAssertTrue(html.contains("/api/memory/viewport"))
    XCTAssertTrue(html.contains("/api/memory/object"))
    XCTAssertTrue(html.contains("/api/memory/density"))
    XCTAssertTrue(html.contains("protocolProblem"))
    XCTAssertFalse(html.contains("/api/memory/traces"))
    XCTAssertFalse(html.localizedCaseInsensitiveContains("trace"))
  }

  func testMemoryAPIDoesNotFabricateUnavailableResultsWithoutProvider() throws {
    let root = temporaryRoot()
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: root))
    let handler = WikiLocalAPIHandler(paths: web)

    let response = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/memory/viewport"))
    let payload = try XCTUnwrap(json(response))
    let error = try XCTUnwrap(payload["error"] as? [String: Any])

    XCTAssertEqual(response.statusCode, 503)
    XCTAssertEqual(payload["surface"] as? String, "memory_viewport")
    XCTAssertEqual(payload["status"] as? String, "error")
    XCTAssertEqual(error["code"] as? String, "memory_protocol_error")
  }

  func testWikiLocalAPIRedactsBrowserVisibleLocalPaths() throws {
    let root = temporaryRoot()
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: root))
    let handler = WikiLocalAPIHandler(paths: web)

    let health = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/health"))
    let state = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/state"))
    let healthPayload = try XCTUnwrap(json(health))
    let statePayload = try XCTUnwrap(json(state))
    let storage = try XCTUnwrap(statePayload["_storage"] as? [String: Any])
    let visibleAPIText = [
      String(data: health.body, encoding: .utf8) ?? "",
      String(data: state.body, encoding: .utf8) ?? ""
    ].joined(separator: "\n")

    XCTAssertEqual(healthPayload["current_site"] as? String, "app-support://wiki-site/current")
    XCTAssertEqual(storage["uri"] as? String, "app-support://local-web/wiki-browser-state.json")
    XCTAssertNil(storage["path"])
    XCTAssertFalse(visibleAPIText.contains(root.path))
    XCTAssertFalse(visibleAPIText.contains(NSHomeDirectory()))
  }

  func testWikiLocalAPIHealthReadsCurrentRenderStateFromAppMirror() throws {
    let root = temporaryRoot()
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: root))
    let renderState = web.wikiCurrent
      .appendingPathComponent(".1context", isDirectory: true)
      .appendingPathComponent("current-render.json")
    try writeString("<!doctype html><title>1Context</title>", to: web.wikiCurrent.appendingPathComponent("index.html"))
    try writeJSON([
      "schemaVersion": 1,
      "status": "published",
      "trigger": "unit-test",
      "publishedAt": "2026-05-20T13:36:00Z",
      "sourceSite": "user-wiki://site",
      "publishedSite": "app-support://wiki-site/current",
      "message": "Published from unit test."
    ], to: renderState)

    let handler = WikiLocalAPIHandler(paths: web)
    let response = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/health"))
    let payload = try XCTUnwrap(json(response))

    XCTAssertEqual(response.statusCode, 200)
    XCTAssertEqual(payload["current_site"] as? String, "app-support://wiki-site/current")
    XCTAssertEqual(payload["current_site_exists"] as? Bool, true)
    XCTAssertEqual(payload["published_at"] as? String, "2026-05-20T13:36:00Z")
  }

  func testWikiLocalAPIHealthIgnoresLegacyPublishManifest() throws {
    let root = temporaryRoot()
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: root))
    try writeJSON([
      "published_at": "2020-01-01T00:00:00Z"
    ], to: web.wikiCurrent.appendingPathComponent("publish-manifest.json"))

    let handler = WikiLocalAPIHandler(paths: web)
    let response = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/health"))
    let payload = try XCTUnwrap(json(response))

    XCTAssertEqual(response.statusCode, 200)
    XCTAssertTrue(payload["published_at"] is NSNull)
  }

  func testWikiLocalAPIDoesNotExposeUnshippedChatRoutes() throws {
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: temporaryRoot()))
    let handler = WikiLocalAPIHandler(paths: web)
    let removedPrefix = "/api/wiki/" + "chat"

    let config = handler.handle(WikiLocalAPIRequest(method: "GET", path: removedPrefix + "/config"))
    let provider = handler.handle(WikiLocalAPIRequest(method: "POST", path: removedPrefix + "/provider", body: Data(#"{"provider":"auto"}"#.utf8)))
    let reset = handler.handle(WikiLocalAPIRequest(method: "POST", path: removedPrefix + "/reset"))
    let chat = handler.handle(WikiLocalAPIRequest(method: "POST", path: removedPrefix, body: Data(#"{"message":"hello"}"#.utf8)))

    XCTAssertEqual(config.statusCode, 404)
    XCTAssertEqual(provider.statusCode, 404)
    XCTAssertEqual(reset.statusCode, 404)
    XCTAssertEqual(chat.statusCode, 404)
  }

  func testWikiLocalAPIDefaultPortCollisionFallsBackToAvailableLoopbackPort() throws {
    WikiLocalAPIConfig.resetPreferredPortForTesting()
    defer { WikiLocalAPIConfig.resetPreferredPortForTesting() }
    let occupiedFD = try bindLoopbackPortIfAvailable(LocalWebDefaults.wikiAPIPort)
    defer {
      if let occupiedFD {
        close(occupiedFD)
      }
    }
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: temporaryRoot()))
    let server = WikiLocalAPIServer(config: WikiLocalAPIConfig(), handler: WikiLocalAPIHandler(paths: web))

    let snapshot = try server.start()
    defer { server.stop() }

    XCTAssertTrue(snapshot.running)
    XCTAssertNotEqual(snapshot.port, LocalWebDefaults.wikiAPIPort)
    XCTAssertEqual(snapshot.health, "OK")
    XCTAssertTrue(snapshot.lastError?.contains("Default 1Context wiki API port") == true)
    XCTAssertTrue(snapshot.url.contains(":\(snapshot.port)/api/wiki/health"))
    XCTAssertEqual(WikiLocalAPIProbe.health(config: WikiLocalAPIConfig(port: snapshot.port)), "OK")

    let diagnostics = CaddyManager(runtimePaths: testRuntimePaths(root: temporaryRoot())).diagnostics()
    XCTAssertEqual(diagnostics.apiPort, snapshot.port)
    XCTAssertEqual(diagnostics.apiHealth, "OK")
    XCTAssertTrue(diagnostics.apiURL.contains(":\(snapshot.port)/api/wiki/health"))
  }

  private func temporaryRoot() -> URL {
    URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-tests-\(UUID().uuidString)", isDirectory: true)
  }

  private func testRuntimePaths(root: URL) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true)
    )
  }

  private func testSetupSystemPaths(root: URL) -> LocalWebSetupSystemPaths {
    LocalWebSetupSystemPaths(
      appBundle: root.appendingPathComponent("Applications/1Context.app", isDirectory: true),
      supportDirectory: root.appendingPathComponent("System", isDirectory: true).path,
      logDirectory: root.appendingPathComponent("Logs", isDirectory: true).path,
      launchDaemonPlist: root.appendingPathComponent("LaunchDaemons/proxy.plist").path
    )
  }

  private func localHTTPSSetupSnapshot(
    sourceProxySHA256: String,
    installedProxySHA256: String
  ) -> LocalWebSetupSnapshot {
    LocalWebSetupSnapshot.localHTTPSPortless(
      targetURL: LocalWebDefaults.defaultWikiURL,
      state: LocalWebSetupState(
        label: LocalWebSetupConstants.proxyLabel,
        targetHost: LocalWebDefaults.wikiHost,
        targetURL: LocalWebDefaults.defaultWikiURL,
        backendHost: LocalWebDefaults.bindHost,
        backendPort: LocalWebDefaults.wikiPort,
        privilegedPort: LocalWebSetupConstants.privilegedHTTPSPort,
        sourceProxyExecutablePath: "/Applications/1Context.app/Contents/Resources/1context-local-web-proxy",
        sourceProxyExecutableSHA256: sourceProxySHA256,
        installedProxyExecutableSHA256: installedProxySHA256,
        userRootCertificatePath: "/tmp/root.crt",
        userRootCertificateExists: true,
        userRootCertificateSHA1: "ABC123",
        userRootCertificateSHA256: "DEF456",
        systemPaths: testSetupSystemPaths(root: URL(fileURLWithPath: "/tmp/1Context", isDirectory: true)),
        proxyPlistInstalled: true,
        proxyExecutableInstalled: true,
        proxyServiceStatus: "enabled",
        proxyLaunchDaemonLoaded: true,
        proxyPortReachable: true,
        trustedRootCertificateInstalled: true,
        trustedRootSHA1: "ABC123",
        trustedRootSHA256: "DEF456"
      )
    )
  }

  private func writeJSON(_ payload: [String: Any], to url: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    let data = try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys])
    try RuntimePermissions.writePrivateData(data, to: url)
  }

  private func writeString(_ value: String, to url: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    try RuntimePermissions.writePrivateData(Data(value.utf8), to: url)
  }

  private func json(_ response: WikiLocalAPIResponse) throws -> [String: Any]? {
    try JSONSerialization.jsonObject(with: response.body) as? [String: Any]
  }

  private func bindLoopbackPortIfAvailable(_ port: Int) throws -> Int32? {
    let fd = socket(AF_INET, SOCK_STREAM, 0)
    XCTAssertGreaterThanOrEqual(fd, 0)
    guard fd >= 0 else {
      throw WikiLocalAPIError.socketFailed(String(cString: strerror(errno)))
    }
    var reuse: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))

    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_port = UInt16(port).bigEndian
    address.sin_addr = in_addr(s_addr: inet_addr(LocalWebDefaults.bindHost))

    let bindResult = withUnsafePointer(to: &address) {
      $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
        Darwin.bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
      }
    }
    guard bindResult == 0 else {
      let code = errno
      let message = String(cString: strerror(code))
      close(fd)
      if code == EADDRINUSE {
        return nil
      }
      throw WikiLocalAPIError.bindFailed(LocalWebDefaults.bindHost, port, message)
    }
    guard listen(fd, 1) == 0 else {
      let message = String(cString: strerror(errno))
      close(fd)
      throw WikiLocalAPIError.socketFailed(message)
    }
    return fd
  }
}
