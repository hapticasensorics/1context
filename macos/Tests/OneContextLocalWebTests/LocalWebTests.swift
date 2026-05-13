import XCTest
@testable import OneContextLocalWeb
import OneContextPlatform

final class LocalWebTests: XCTestCase {
  func testCaddyConfigIsOpinionatedLocalWebEdge() {
    let config = CaddyConfig(
      mode: .highPortHTTP,
      siteRoot: URL(fileURLWithPath: "/tmp/1Context Wiki/current", isDirectory: true),
      logFile: URL(fileURLWithPath: "/tmp/1Context Logs/caddy.log")
    )

    let text = config.caddyfileText()
    XCTAssertTrue(text.contains("admin off"))
    XCTAssertTrue(text.contains("auto_https off"))
    XCTAssertTrue(text.contains("http://wiki.1context.localhost:39191, http://127.0.0.1:39191"))
    XCTAssertTrue(text.contains("bind 127.0.0.1"))
    XCTAssertTrue(text.contains("root * \"/tmp/1Context Wiki/current\""))
    XCTAssertTrue(text.contains("route {"))
    XCTAssertTrue(text.contains("@wikiStaticApi path /api/wiki/site /api/wiki/pages /api/wiki/stats"))
    XCTAssertTrue(text.contains("rewrite * {path}.json"))
    XCTAssertTrue(text.contains("@wikiDynamicApi path /api/wiki/*"))
    XCTAssertLessThan(
      try XCTUnwrap(text.range(of: "@wikiStaticApi path")?.lowerBound),
      try XCTUnwrap(text.range(of: "@wikiDynamicApi path")?.lowerBound)
    )
    XCTAssertLessThan(
      try XCTUnwrap(text.range(of: "@wikiDynamicApi path")?.lowerBound),
      try XCTUnwrap(text.range(of: "try_files {path}")?.lowerBound)
    )
    XCTAssertTrue(text.contains("try_files {path} {path}.html {path}/index.html"))
    XCTAssertFalse(text.contains("try_files {path} {path}.html {path}/index.html /index.html"))
    XCTAssertTrue(text.contains("file_server"))
    XCTAssertTrue(text.contains("reverse_proxy 127.0.0.1:39192"))
    XCTAssertFalse(text.contains("rewrite /api/wiki/search /api/wiki/search.json"))
    XCTAssertFalse(text.contains("respond `"))
    XCTAssertEqual(config.url, "http://wiki.1context.localhost:39191/your-context")
    XCTAssertEqual(config.healthURL.absoluteString, "http://127.0.0.1:39191/__1context/health")
  }

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
    XCTAssertTrue(text.contains("https://wiki.1context.localhost:39191 {"))
    XCTAssertTrue(text.contains("bind 127.0.0.1"))
    XCTAssertTrue(text.contains("tls internal"))
    XCTAssertFalse(text.contains("auto_https off"))
    XCTAssertTrue(text.contains(":39191"))
    XCTAssertEqual(config.url, "https://wiki.1context.localhost/your-context")
    XCTAssertEqual(config.healthURL.absoluteString, "https://wiki.1context.localhost/__1context/health")
  }

  func testDefaultURLModeRequiresProfessionalLocalHTTPSSetup() {
    let mode = LocalWebURLMode(environmentValue: nil)
    let config = CaddyConfig(
      mode: mode,
      siteRoot: URL(fileURLWithPath: "/tmp/1Context Wiki/current", isDirectory: true),
      logFile: URL(fileURLWithPath: "/tmp/1Context Logs/caddy.log")
    )

    XCTAssertEqual(mode, .localHTTPSPortless)
    XCTAssertEqual(LocalWebDefaults.defaultWikiURL, "https://wiki.1context.localhost/your-context")
    XCTAssertEqual(config.url, LocalWebDefaults.defaultWikiURL)
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

  func testLocalWebSetupResolvesHomebrewCLISymlinkToInstalledApp() throws {
    let root = temporaryRoot()
    let app = root.appendingPathComponent("Applications/1Context.app", isDirectory: true)
    let cli = app.appendingPathComponent("Contents/MacOS/1context-cli")
    let link = root.appendingPathComponent("homebrew/bin/1context")
    try FileManager.default.createDirectory(at: cli.deletingLastPathComponent(), withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: link.deletingLastPathComponent(), withIntermediateDirectories: true)
    FileManager.default.createFile(atPath: cli.path, contents: Data("#!/bin/sh\n".utf8))
    try FileManager.default.createSymbolicLink(at: link, withDestinationURL: cli)

    let resolved = LocalWebSetupSystemPaths.appBundleURL(
      environment: [:],
      commandLineExecutable: link.path,
      mainBundleURL: root.appendingPathComponent("homebrew/bin", isDirectory: true)
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

    let paths = RuntimePaths.current(environment: [
      "ONECONTEXT_APP_SUPPORT_DIR": root.appendingPathComponent("Application Support/1Context").path,
      "ONECONTEXT_LOG_DIR": root.appendingPathComponent("Logs/1Context").path,
      "ONECONTEXT_CACHE_DIR": root.appendingPathComponent("Caches/1Context").path
    ])
    let manager = CaddyManager(runtimePaths: paths, environment: [
      "ONECONTEXT_CADDY_PATH": caddy.path,
      "ONECONTEXT_WIKI_URL_MODE": "high-port-http"
    ])
    let diagnostics = manager.diagnostics()

    XCTAssertEqual(diagnostics.urlMode, "high-port-http")
    XCTAssertEqual(diagnostics.trustMode, "none")
    XCTAssertFalse(diagnostics.privilegedBindRequired)
    XCTAssertEqual(diagnostics.caddyExecutable, caddy.path)
    XCTAssertTrue(diagnostics.caddyExecutableExists)
    XCTAssertTrue(diagnostics.caddyExecutableIsExecutable)
    XCTAssertFalse(diagnostics.caddyExecutableIsBundled)
    XCTAssertTrue(diagnostics.currentSitePath.hasSuffix("Application Support/1Context/wiki-site/current"))
    XCTAssertTrue(diagnostics.caddyfilePath.hasSuffix("Application Support/1Context/local-web/caddy/Caddyfile"))
    XCTAssertEqual(diagnostics.apiPort, LocalWebDefaults.wikiAPIPort)
    XCTAssertTrue(diagnostics.apiStatePath.hasSuffix("Application Support/1Context/local-web/wiki-browser-state.json"))
  }

  func testDiagnosticsReportsLocalHTTPSURLMode() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1context-local-web-url-mode-\(UUID().uuidString)", isDirectory: true)
    let caddy = root.appendingPathComponent("bin/caddy")
    try FileManager.default.createDirectory(at: caddy.deletingLastPathComponent(), withIntermediateDirectories: true)
    FileManager.default.createFile(atPath: caddy.path, contents: Data("#!/bin/sh\n".utf8))
    chmod(caddy.path, 0o755)

    let paths = RuntimePaths.current(environment: [
      "ONECONTEXT_APP_SUPPORT_DIR": root.appendingPathComponent("Application Support/1Context").path,
      "ONECONTEXT_LOG_DIR": root.appendingPathComponent("Logs/1Context").path,
      "ONECONTEXT_CACHE_DIR": root.appendingPathComponent("Caches/1Context").path
    ])
    let manager = CaddyManager(runtimePaths: paths, environment: [
      "ONECONTEXT_CADDY_PATH": caddy.path,
      "ONECONTEXT_WIKI_URL_MODE": "local-https-portless"
    ].merging(localWebSetupTestEnvironment(root: root)) { _, new in new })

    let diagnostics = manager.diagnostics()

    XCTAssertEqual(diagnostics.snapshot.url, "https://wiki.1context.localhost/your-context")
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

    let manager = CaddyManager(runtimePaths: paths, environment: [
      "ONECONTEXT_WIKI_URL_MODE": "local-https-portless"
    ].merging(localWebSetupTestEnvironment(root: root)) { _, new in new })
    let snapshot = manager.status()

    XCTAssertFalse(snapshot.running)
    XCTAssertEqual(snapshot.health, "setup required")
    XCTAssertEqual(snapshot.url, "https://wiki.1context.localhost/your-context")
    XCTAssertEqual(snapshot.lastError, "Local web setup required: Local HTTPS helper, Local certificate trust")
  }

  func testStartRequiresSetupBeforeProfessionalLocalHTTPSModeRuns() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let manager = CaddyManager(runtimePaths: paths, environment: [
      "ONECONTEXT_WIKI_URL_MODE": "local-https-portless"
    ].merging(localWebSetupTestEnvironment(root: root)) { _, new in new })

    XCTAssertThrowsError(try manager.start()) { error in
      XCTAssertEqual(
        error as? LocalWebError,
        .setupRequired("Local web setup required: Local HTTPS helper, Local certificate trust")
      )
    }
  }

  func testPlaceholderSiteUsesPlaceholderPagesInsteadOfGeneratedSourceWiki() throws {
    let root = temporaryRoot()
    let paths = testRuntimePaths(root: root)
    let web = LocalWebPaths(runtimePaths: paths)
    let manager = CaddyManager(runtimePaths: paths, environment: [
      "ONECONTEXT_WIKI_URL_MODE": "high-port-http"
    ])

    try manager.ensurePlaceholderSite()

    XCTAssertTrue(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("your-context.html").path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("for-you.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("projects.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("topics.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("goal.html").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("goal.md").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("your-context.md").path))
    XCTAssertFalse(FileManager.default.fileExists(atPath: web.wikiCurrent.appendingPathComponent("render-manifest.json").path))
  }

  func testLocalHTTPSSetupSnapshotReflectsInstalledProxyAndTrust() {
    let systemPaths = LocalWebSetupSystemPaths(environment: [
      "ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR": "/tmp/1Context/System",
      "ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR": "/tmp/1Context/Logs",
      "ONECONTEXT_LOCAL_WEB_LAUNCH_DAEMON_PATH": "/tmp/1Context/LaunchDaemons/proxy.plist"
    ])
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
    let systemPaths = LocalWebSetupSystemPaths(environment: [
      "ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR": "/tmp/1Context/System",
      "ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR": "/tmp/1Context/Logs",
      "ONECONTEXT_LOCAL_WEB_LAUNCH_DAEMON_PATH": "/tmp/1Context/LaunchDaemons/proxy.plist"
    ])
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

  func testWikiLocalAPIRejectsUnshippedChatRoutes() throws {
    let web = LocalWebPaths(runtimePaths: testRuntimePaths(root: temporaryRoot()))
    let handler = WikiLocalAPIHandler(paths: web)

    let config = handler.handle(WikiLocalAPIRequest(method: "GET", path: "/api/wiki/chat/config"))
    let chat = handler.handle(WikiLocalAPIRequest(method: "POST", path: "/api/wiki/chat", body: Data(#"{"message":"hello"}"#.utf8)))

    XCTAssertEqual(config.statusCode, 404)
    XCTAssertEqual(chat.statusCode, 404)
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

  private func localWebSetupTestEnvironment(root: URL) -> [String: String] {
    [
      "ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR": root.appendingPathComponent("System/Application Support/1Context").path,
      "ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR": root.appendingPathComponent("System/Logs/1Context").path,
      "ONECONTEXT_LOCAL_WEB_LAUNCH_DAEMON_PATH": root.appendingPathComponent("System/LaunchDaemons/com.haptica.1context.local-web-proxy.plist").path
    ]
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
        systemPaths: LocalWebSetupSystemPaths(environment: [
          "ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR": "/tmp/1Context/System",
          "ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR": "/tmp/1Context/Logs",
          "ONECONTEXT_LOCAL_WEB_LAUNCH_DAEMON_PATH": "/tmp/1Context/LaunchDaemons/proxy.plist"
        ]),
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

  private func json(_ response: WikiLocalAPIResponse) throws -> [String: Any]? {
    try JSONSerialization.jsonObject(with: response.body) as? [String: Any]
  }
}
