import XCTest
import OneContextLocalWeb
import OneContextSetup

final class AppSetupTests: XCTestCase {
  func testReadinessNeedsSetupBeforeLocalWebAttention() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: false),
        caddyExecutableExists: false,
        caddyExecutableIsExecutable: false
      )
    )

    XCTAssertEqual(readiness.state, .needsSetup)
    XCTAssertFalse(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.menuTitle, "1Context Needs Setup")
  }

  func testReadinessNeedsAttentionWhenCaddyIsMissingAfterSetup() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: LocalWebSetupSnapshot.highPortHTTP(targetURL: "http://127.0.0.1:39191/your-context"),
        caddyExecutableExists: false,
        caddyExecutableIsExecutable: false
      )
    )

    XCTAssertEqual(readiness.state, .needsAttention)
    XCTAssertTrue(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.menuTitle, "1Context Needs Attention")
  }

  func testReadinessReadyWhenSetupAndCaddyAreReady() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: LocalWebSetupSnapshot.highPortHTTP(targetURL: "http://127.0.0.1:39191/your-context"),
        caddyExecutableExists: true,
        caddyExecutableIsExecutable: true
      )
    )

    XCTAssertEqual(readiness.state, .ready)
    XCTAssertTrue(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.menuTitle, "1Context Ready")
  }

  func testReadinessNeedsSetupWhenProxyBinaryIsStaleAfterAppUpdate() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: true, installedProxySHA256: "OLD"),
        caddyExecutableExists: true,
        caddyExecutableIsExecutable: true
      )
    )

    XCTAssertEqual(readiness.state, .needsSetup)
    XCTAssertFalse(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.menuTitle, "1Context Needs Setup")
  }

  private func diagnostics(
    setup: LocalWebSetupSnapshot,
    caddyExecutableExists: Bool,
    caddyExecutableIsExecutable: Bool
  ) -> LocalWebDiagnostics {
    LocalWebDiagnostics(
      snapshot: LocalWebSnapshot(
        running: false,
        url: setup.targetURL,
        health: setup.ready ? "not running" : "setup required",
        lastError: setup.ready ? nil : setup.blockingSummary
      ),
      urlMode: setup.urlMode,
      trustMode: setup.urlMode == LocalWebURLMode.localHTTPSPortless.rawValue ? "local-ca-required" : "none",
      privilegedBindRequired: setup.urlMode == LocalWebURLMode.localHTTPSPortless.rawValue,
      setup: setup,
      apiURL: "http://127.0.0.1:39192/__1context/api/health",
      apiHealth: "not running",
      apiPort: 39192,
      apiStatePath: "/tmp/1context/wiki-browser-state.json",
      caddyExecutable: caddyExecutableExists ? "/tmp/1context/caddy" : "",
      caddyExecutableExists: caddyExecutableExists,
      caddyExecutableIsExecutable: caddyExecutableIsExecutable,
      caddyExecutableIsBundled: caddyExecutableExists,
      bundledCaddyPath: "/Applications/1Context.app/Contents/Resources/caddy",
      bundledCaddyVersionPath: "/Applications/1Context.app/Contents/Resources/caddy.version",
      bundledCaddyVersion: "test",
      caddyfilePath: "/tmp/1context/Caddyfile",
      statePath: "/tmp/1context/state.json",
      pidPath: "/tmp/1context/local-web-caddy.pid",
      logPath: "/tmp/1context/local-web-caddy.log",
      currentSitePath: "/tmp/1context/wiki/current",
      nextSitePath: "/tmp/1context/wiki/next",
      previousSitePath: "/tmp/1context/wiki/previous",
      currentSiteHasIndex: true,
      currentSiteHasTheme: true,
      currentSiteHasEnhanceJS: true,
      currentSiteHasHealth: true
    )
  }

  private func localHTTPSSetup(ready: Bool, installedProxySHA256: String? = nil) -> LocalWebSetupSnapshot {
    LocalWebSetupSnapshot.localHTTPSPortless(
      targetURL: "https://wiki.1context.localhost/your-context",
      state: LocalWebSetupState(
        label: LocalWebSetupConstants.proxyLabel,
        targetHost: LocalWebDefaults.wikiHost,
        targetURL: "https://wiki.1context.localhost/your-context",
        backendHost: LocalWebDefaults.bindHost,
        backendPort: LocalWebDefaults.wikiPort,
        privilegedPort: LocalWebSetupConstants.privilegedHTTPSPort,
        sourceProxyExecutablePath: "/Applications/1Context.app/Contents/Resources/1context-local-web-proxy",
        sourceProxyExecutableSHA256: "ABC",
        installedProxyExecutableSHA256: installedProxySHA256 ?? (ready ? "ABC" : nil),
        userRootCertificatePath: "/tmp/1context/root.crt",
        userRootCertificateExists: ready,
        userRootCertificateSHA1: ready ? "SHA1" : nil,
        userRootCertificateSHA256: ready ? "SHA256" : nil,
        systemPaths: LocalWebSetupSystemPaths(environment: [
          "ONECONTEXT_APP_BUNDLE_PATH": "/Applications/1Context.app",
          "ONECONTEXT_LOCAL_WEB_SYSTEM_SUPPORT_DIR": "/tmp/1context/setup",
          "ONECONTEXT_LOCAL_WEB_SYSTEM_LOG_DIR": "/tmp/1context/logs"
        ]),
        proxyPlistInstalled: ready,
        proxyExecutableInstalled: ready,
        proxyServiceStatus: ready ? "enabled" : "notFound",
        proxyLaunchDaemonLoaded: ready,
        proxyPortReachable: ready,
        trustedRootCertificateInstalled: ready,
        trustedRootSHA1: ready ? "SHA1" : nil,
        trustedRootSHA256: ready ? "SHA256" : nil
      )
    )
  }
}
