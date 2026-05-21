import XCTest
import AVFoundation
import OneContextLocalWeb
import OneContextSetup

final class AppSetupTests: XCTestCase {
  func testBlockedSetupActionsUseSpecificSetupMessages() {
    XCTAssertEqual(OneContextBlockedSetupAction.openWiki.setupMessage, "Finish setup to open your wiki.")
    XCTAssertEqual(OneContextBlockedSetupAction.refreshWiki.setupMessage, "Finish setup to refresh your wiki.")
  }

  func testSetupContinuationResumesOpenWikiOnceAfterSetup() {
    var continuation = OneContextSetupContinuation()

    let message = continuation.block(.openWiki)
    let action = continuation.consumeResumableActionAfterSetup()
    let secondAction = continuation.consumeResumableActionAfterSetup()

    XCTAssertEqual(message, "Finish setup to open your wiki.")
    XCTAssertEqual(action, .openWiki)
    XCTAssertNil(secondAction)
    XCTAssertNil(continuation.pendingAction)
  }

  func testNewBlockedActionReplacesOlderPendingAction() {
    var continuation = OneContextSetupContinuation()

    continuation.block(.openWiki)
    continuation.block(.refreshWiki)

    XCTAssertEqual(continuation.consumeResumableActionAfterSetup(), .refreshWiki)
  }

  func testReadinessNeedsSetupBeforeLocalWebAttention() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: false),
        caddyExecutableExists: false,
        caddyExecutableIsExecutable: false
      ),
      rememberingPermissions: rememberingPermissions(ready: true)
    )

    XCTAssertEqual(readiness.state, .needsSetup)
    XCTAssertFalse(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.menuTitle, "1Context Needs Setup")
  }

  func testReadinessNeedsAttentionWhenCaddyIsMissingAfterSetup() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: true),
        caddyExecutableExists: false,
        caddyExecutableIsExecutable: false
      ),
      rememberingPermissions: rememberingPermissions(ready: true)
    )

    XCTAssertEqual(readiness.state, .needsAttention)
    XCTAssertTrue(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.menuTitle, "1Context Needs Attention")
  }

  func testReadinessReadyWhenSetupAndCaddyAreReady() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: true),
        caddyExecutableExists: true,
        caddyExecutableIsExecutable: true
      ),
      rememberingPermissions: rememberingPermissions(ready: true)
    )

    XCTAssertEqual(readiness.state, .ready)
    XCTAssertTrue(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.menuTitle, "1Context Ready")
  }

  func testRememberingPermissionsAreRequiredSetup() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: true),
        caddyExecutableExists: true,
        caddyExecutableIsExecutable: true
      ),
      rememberingPermissions: rememberingPermissions(
        screenRecording: false,
        accessibility: true,
        inputMonitoring: true,
        microphone: true
      )
    )

    XCTAssertEqual(readiness.state, .needsSetup)
    XCTAssertFalse(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.requiredSetupSummary, "Remembering requires Screen & System Audio Recording.")
  }

  func testInputMonitoringIsRequiredSetup() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: true),
        caddyExecutableExists: true,
        caddyExecutableIsExecutable: true
      ),
      rememberingPermissions: rememberingPermissions(
        screenRecording: true,
        accessibility: true,
        inputMonitoring: false,
        microphone: true
      )
    )

    XCTAssertEqual(readiness.state, .needsSetup)
    XCTAssertFalse(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.requiredSetupSummary, "Remembering requires Input Monitoring.")
  }

  func testMicrophoneIsRequiredSetup() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: true),
        caddyExecutableExists: true,
        caddyExecutableIsExecutable: true
      ),
      rememberingPermissions: rememberingPermissions(
        screenRecording: true,
        accessibility: true,
        inputMonitoring: true,
        microphone: false
      )
    )

    XCTAssertEqual(readiness.state, .needsSetup)
    XCTAssertFalse(readiness.requiredSetupReady)
    XCTAssertEqual(readiness.requiredSetupSummary, "Remembering requires Microphone.")
  }

  func testBrowserAutomationAndFullDiskAccessAreRequiredSetup() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: true),
        caddyExecutableExists: true,
        caddyExecutableIsExecutable: true
      ),
      rememberingPermissions: rememberingPermissions(
        browserExtension: false,
        automation: false,
        fullDiskAccess: false
      )
    )

    XCTAssertEqual(readiness.state, .needsSetup)
    XCTAssertFalse(readiness.requiredSetupReady)
    XCTAssertEqual(
      readiness.requiredSetupSummary,
      "Remembering requires Browser Extension Permissions, Automation, Full Disk Access."
    )
  }

  func testSetupDiagnosticsContainRememberingPermissionRequirements() {
    let setup = OneContextAppSetup.snapshot(
      localWikiAccess: localHTTPSSetup(ready: true),
      rememberingPermissions: rememberingPermissions(
        screenRecording: true,
        accessibility: false,
        inputMonitoring: true,
        microphone: true
      )
    )

    let text = OneContextAppSetupDiagnostics.render(setup).joined(separator: "\n")

    XCTAssertTrue(text.contains("Local Wiki Access: Granted"))
    XCTAssertTrue(text.contains("Screen & System Audio Recording: Granted"))
    XCTAssertTrue(text.contains("Accessibility: Required"))
    XCTAssertTrue(text.contains("Input Monitoring: Granted"))
    XCTAssertTrue(text.contains("Browser Extension Permissions: Granted"))
    XCTAssertTrue(text.contains("Microphone: Granted"))
    XCTAssertTrue(text.contains("Automation: Granted"))
    XCTAssertTrue(text.contains("Full Disk Access: Granted"))
    XCTAssertFalse(text.contains("Sensitive Permissions:"))
  }

  func testBrowserExtensionProofIsScopedToSignedSubjectAndPermissionSet() throws {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject(
      bundleIdentifier: "com.example.one",
      designatedRequirementSHA256: "dr-one",
      appVersion: "1"
    )

    try OneContextSystemPermissions.recordBrowserExtensionProof(
      OneContextBrowserExtensionPermissionProof(
        extensionIdentifier: "abcdefghijklmnoabcdefghijklmnoab",
        extensionVersion: "1.0.0",
        grantedPermissions: OneContextSystemPermissions.requiredBrowserExtensionPermissions,
        grantedHostPermissions: OneContextSystemPermissions.requiredBrowserExtensionHostPermissions
      ),
      preferencesPath: preferencesPath,
      subject: subject
    )

    XCTAssertTrue(
      OneContextSystemPermissions.browserExtensionGranted(
        preferencesPath: preferencesPath,
        subject: subject
      )
    )
    XCTAssertFalse(
      OneContextSystemPermissions.browserExtensionGranted(
        preferencesPath: preferencesPath,
        subject: permissionSubject(
          bundleIdentifier: "com.example.one",
          designatedRequirementSHA256: "dr-two",
          appVersion: "1"
        )
      )
    )
    XCTAssertFalse(
      OneContextSystemPermissions.browserExtensionGranted(
        preferencesPath: preferencesPath,
        subject: permissionSubject(
          bundleIdentifier: "com.example.one",
          designatedRequirementSHA256: "dr-one",
          appVersion: "2"
        )
      )
    )
  }

  func testBrowserExtensionProofRequiresCompleteNativeHandshakeDetails() throws {
    let subject = permissionSubject()
    let completeProof = browserExtensionProof()

    for proof in [
      browserExtensionProof(grantedPermissions: OneContextSystemPermissions.requiredBrowserExtensionPermissions.filter { $0 != "tabs" }),
      browserExtensionProof(grantedHostPermissions: [])
    ] {
      let preferencesPath = temporaryPreferencesPath()

      try OneContextSystemPermissions.recordBrowserExtensionProof(
        proof,
        preferencesPath: preferencesPath,
        subject: subject
      )

      XCTAssertFalse(
        OneContextSystemPermissions.browserExtensionGranted(
          preferencesPath: preferencesPath,
          subject: subject,
          expectedExtensionIdentifier: proof.extensionIdentifier
        )
      )
    }

    XCTAssertThrowsError(
      try OneContextSystemPermissions.recordBrowserExtensionProof(
        browserExtensionProof(extensionIdentifier: ""),
        preferencesPath: temporaryPreferencesPath(),
        subject: subject
      )
    )
    XCTAssertThrowsError(
      try OneContextSystemPermissions.recordBrowserExtensionProof(
        browserExtensionProof(extensionVersion: ""),
        preferencesPath: temporaryPreferencesPath(),
        subject: subject
      )
    )

    let preferencesPath = temporaryPreferencesPath()
    try OneContextSystemPermissions.recordBrowserExtensionProof(
      completeProof,
      preferencesPath: preferencesPath,
      subject: subject
    )
    XCTAssertTrue(
      OneContextSystemPermissions.browserExtensionGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        expectedExtensionIdentifier: completeProof.extensionIdentifier
      )
    )
  }

  func testBrowserExtensionProofMustMatchExpectedExtensionIdentifier() throws {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()

    try OneContextSystemPermissions.recordBrowserExtensionProof(
      browserExtensionProof(extensionIdentifier: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"),
      preferencesPath: preferencesPath,
      subject: subject
    )

    XCTAssertTrue(
      OneContextSystemPermissions.browserExtensionGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        expectedExtensionIdentifier: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
      )
    )
    XCTAssertFalse(
      OneContextSystemPermissions.browserExtensionGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        expectedExtensionIdentifier: "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
      )
    )
  }

  func testLegacyBrowserExtensionBooleanDoesNotGrantCurrentBundle() {
    let preferencesPath = temporaryPreferencesPath()
    let preferences = NSMutableDictionary()
    preferences[OneContextSystemPermissions.browserExtensionPermissionsGrantedKey] = true
    XCTAssertTrue(preferences.write(toFile: preferencesPath, atomically: true))

    XCTAssertFalse(
      OneContextSystemPermissions.browserExtensionGranted(
        preferencesPath: preferencesPath,
        subject: permissionSubject()
      )
    )
  }

  func testBrowserExtensionSelfRecordedGrantIsRejected() {
    let preferencesPath = temporaryPreferencesPath()

    XCTAssertThrowsError(
      try OneContextSystemPermissions.recordBrowserExtensionGranted(preferencesPath: preferencesPath)
    )
    XCTAssertFalse(
      OneContextSystemPermissions.browserExtensionGranted(
        preferencesPath: preferencesPath,
        subject: permissionSubject()
      )
    )
  }

  func testInputMonitoringUsesInputMonitoringGrantAndSubjectProof() {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    let inputState = PermissionTestInputState()
    let platform = permissionPlatform(
      inputPreflight: { inputState.preflight },
      inputRequest: {
        inputState.requested = true
        inputState.preflight = true
        return true
      }
    )

    XCTAssertFalse(
      OneContextSystemPermissions.inputMonitoringGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: platform
      )
    )
    XCTAssertTrue(
      OneContextSystemPermissions.requestInputMonitoring(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: platform
      )
    )
    XCTAssertTrue(inputState.requested)
    XCTAssertTrue(
      OneContextSystemPermissions.inputMonitoringGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: platform
      )
    )
    XCTAssertFalse(
      OneContextSystemPermissions.inputMonitoringGranted(
        preferencesPath: preferencesPath,
        subject: permissionSubject(designatedRequirementSHA256: "different"),
        platform: platform
      )
    )
  }

  func testInputMonitoringDeniedRequestDoesNotPersistProof() {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    let platform = permissionPlatform(
      inputPreflight: { false },
      inputRequest: { false }
    )

    XCTAssertFalse(
      OneContextSystemPermissions.requestInputMonitoring(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: platform
      )
    )
    XCTAssertFalse(
      OneContextSystemPermissions.inputMonitoringGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: permissionPlatform(inputPreflight: { true })
      )
    )
  }

  func testRefreshRuntimeProofsRecordsAlreadyAuthorizedInputMonitoring() async {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    let platform = permissionPlatform(
      screenPreflight: { false },
      inputPreflight: { true },
      microphoneStatus: { .denied }
    )

    XCTAssertFalse(
      OneContextSystemPermissions.inputMonitoringGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: platform
      )
    )

    let changed = await OneContextSystemPermissions.refreshRuntimeProofsIfAuthorized(
      preferencesPath: preferencesPath,
      subject: subject,
      platform: platform
    )
    XCTAssertTrue(changed)
    XCTAssertTrue(
      OneContextSystemPermissions.inputMonitoringGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: platform
      )
    )
  }

  func testAutomationTargetsDefaultToVisibleHighSignalApps() {
    let preferencesPath = temporaryPreferencesPath()

    XCTAssertEqual(
      OneContextSystemPermissions.selectedAutomationTargetBundleIdentifiers(preferencesPath: preferencesPath),
      [
        "com.apple.finder",
        "com.google.Chrome",
        "com.microsoft.VSCode"
      ]
    )
  }

  func testAutomationTargetsExposeConfiguredDefaultTargetObjects() {
    let preferencesPath = temporaryPreferencesPath()

    XCTAssertEqual(
      OneContextSystemPermissions.selectedAutomationTargets(preferencesPath: preferencesPath),
      [
        OneContextAutomationTarget(name: "Finder", bundleIdentifier: "com.apple.finder"),
        OneContextAutomationTarget(name: "Google Chrome", bundleIdentifier: "com.google.Chrome"),
        OneContextAutomationTarget(name: "Visual Studio Code", bundleIdentifier: "com.microsoft.VSCode")
      ]
    )
  }

  func testAutomationTargetsCanBeConfiguredAndSanitized() throws {
    let preferencesPath = temporaryPreferencesPath()

    try OneContextSystemPermissions.setAutomationTargetBundleIdentifiers(
      [
        "com.google.Chrome",
        "com.google.Chrome",
        "com.not-supported.App",
        "com.apple.finder"
      ],
      preferencesPath: preferencesPath
    )

    XCTAssertEqual(
      OneContextSystemPermissions.selectedAutomationTargetBundleIdentifiers(preferencesPath: preferencesPath),
      [
        "com.google.Chrome",
        "com.apple.finder"
      ]
    )
  }

  func testAutomationSelectionIgnoresUnconfiguredCandidateTargets() throws {
    let preferencesPath = temporaryPreferencesPath()
    try OneContextSystemPermissions.setAutomationTargetBundleIdentifiers(
      ["com.google.Chrome"],
      preferencesPath: preferencesPath
    )

    let candidateTargets = OneContextSystemPermissions.defaultAutomationTargets + [
      OneContextAutomationTarget(name: "Hidden Helper", bundleIdentifier: "com.example.hidden-helper")
    ]

    XCTAssertEqual(
      OneContextSystemPermissions.selectedAutomationTargets(
        preferencesPath: preferencesPath,
        targets: candidateTargets
      ),
      [
        OneContextAutomationTarget(name: "Google Chrome", bundleIdentifier: "com.google.Chrome")
      ]
    )
  }

  func testAutomationConfigurationRejectsUnknownTargetsWithoutReplacingSelection() throws {
    let preferencesPath = temporaryPreferencesPath()
    try OneContextSystemPermissions.setAutomationTargetBundleIdentifiers(
      ["com.google.Chrome"],
      preferencesPath: preferencesPath
    )

    XCTAssertThrowsError(
      try OneContextSystemPermissions.setAutomationTargetBundleIdentifiers(
        ["com.example.hidden-helper"],
        preferencesPath: preferencesPath
      )
    )
    XCTAssertEqual(
      OneContextSystemPermissions.selectedAutomationTargetBundleIdentifiers(preferencesPath: preferencesPath),
      ["com.google.Chrome"]
    )
  }

  func testAutomationProofsForDeselectedTargetsAreCleared() throws {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    try writePreferences(
      [
        OneContextSystemPermissions.automationProofsKey: [
          "com.apple.finder": automationProof(
            targetName: "Finder",
            targetBundleIdentifier: "com.apple.finder",
            subject: subject
          ),
          "com.google.Chrome": automationProof(
            targetName: "Google Chrome",
            targetBundleIdentifier: "com.google.Chrome",
            subject: subject
          ),
          "com.microsoft.VSCode": automationProof(
            targetName: "Visual Studio Code",
            targetBundleIdentifier: "com.microsoft.VSCode",
            subject: subject
          )
        ]
      ],
      to: preferencesPath
    )

    try OneContextSystemPermissions.setAutomationTargetBundleIdentifiers(
      ["com.google.Chrome"],
      preferencesPath: preferencesPath
    )

    let preferences = try preferences(at: preferencesPath)
    let proofs = try XCTUnwrap(preferences[OneContextSystemPermissions.automationProofsKey] as? [String: Any])
    XCTAssertEqual(Set(proofs.keys), ["com.google.Chrome"])
    XCTAssertEqual(
      preferences[OneContextSystemPermissions.automationTargetBundleIdentifiersKey] as? [String],
      ["com.google.Chrome"]
    )
  }

  func testAutomationSelfRecordedGrantIsRejected() {
    XCTAssertThrowsError(try OneContextSystemPermissions.recordAutomationGranted())
  }

  func testFullDiskAccessRequiresAtLeastOneExistingProbe() {
    let missingProbe = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("missing-\(UUID().uuidString)")

    XCTAssertFalse(OneContextSystemPermissions.fullDiskAccessGranted(probeURLs: [missingProbe]))
  }

  func testFullDiskAccessRequiresAllExistingProbesReadable() throws {
    let directory = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
      .appendingPathComponent("1context-fda-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let first = directory.appendingPathComponent("first.txt")
    let second = directory.appendingPathComponent("second.txt")
    try "first".write(to: first, atomically: true, encoding: .utf8)
    try "second".write(to: second, atomically: true, encoding: .utf8)

    XCTAssertTrue(OneContextSystemPermissions.fullDiskAccessGranted(probeURLs: [first, second]))
  }

  func testReadinessNeedsSetupWhenProxyBinaryIsStaleAfterAppUpdate() {
    let readiness = OneContextAppReadiness.snapshot(
      localWebDiagnostics: diagnostics(
        setup: localHTTPSSetup(ready: true, installedProxySHA256: "OLD"),
        caddyExecutableExists: true,
        caddyExecutableIsExecutable: true
      ),
      rememberingPermissions: rememberingPermissions(ready: true)
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
      readinessProbeURL: "https://127.0.0.1:39191/__1context/health",
      readinessProbeHealth: setup.ready ? "not running" : "setup required",
      privilegedProxyProbeURL: "https://localhost/__1context/health",
      privilegedProxyProbeHealth: setup.ready ? "not running" : "setup required",
      brandedProbeURL: "https://wiki.1context.localhost/__1context/health",
      brandedProbeHealth: setup.ready ? "not running" : "setup required",
      apiURL: "http://127.0.0.1:39192/__1context/api/health",
      apiHealth: "not running",
      apiPort: 39192,
      apiStatePath: "/tmp/1context/wiki-browser-state.json",
      caddyExecutable: caddyExecutableExists ? "/tmp/1context/caddy" : "",
      caddyExecutableExists: caddyExecutableExists,
      caddyExecutableIsExecutable: caddyExecutableIsExecutable,
      caddyExecutableIsBundled: caddyExecutableExists,
      runningCaddyExecutable: nil,
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
      currentSiteHasHealth: true
    )
  }

  private func localHTTPSSetup(ready: Bool, installedProxySHA256: String? = nil) -> LocalWebSetupSnapshot {
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
        sourceProxyExecutableSHA256: "ABC",
        installedProxyExecutableSHA256: installedProxySHA256 ?? (ready ? "ABC" : nil),
        userRootCertificatePath: "/tmp/1context/root.crt",
        userRootCertificateExists: ready,
        userRootCertificateSHA1: ready ? "SHA1" : nil,
        userRootCertificateSHA256: ready ? "SHA256" : nil,
        systemPaths: LocalWebSetupSystemPaths(
          appBundle: URL(fileURLWithPath: "/Applications/1Context.app", isDirectory: true),
          supportDirectory: "/tmp/1context/setup",
          logDirectory: "/tmp/1context/logs"
        ),
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

  private func rememberingPermissions(ready: Bool) -> OneContextRememberingPermissionsSnapshot {
    rememberingPermissions(
      screenRecording: ready,
      accessibility: ready,
      inputMonitoring: ready,
      browserExtension: ready,
      microphone: ready,
      automation: ready,
      fullDiskAccess: ready
    )
  }

  private func rememberingPermissions(
    screenRecording: Bool = true,
    accessibility: Bool = true,
    inputMonitoring: Bool = true,
    browserExtension: Bool = true,
    microphone: Bool = true,
    automation: Bool = true,
    fullDiskAccess: Bool = true
  ) -> OneContextRememberingPermissionsSnapshot {
    OneContextRememberingPermissionsSnapshot.snapshot(
      screenRecordingGranted: screenRecording,
      accessibilityGranted: accessibility,
      inputMonitoringGranted: inputMonitoring,
      browserExtensionGranted: browserExtension,
      microphoneGranted: microphone,
      automationGranted: automation,
      fullDiskAccessGranted: fullDiskAccess
    )
  }

  private func temporaryPreferencesPath() -> String {
    URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
      .appendingPathComponent("1context-permissions-\(UUID().uuidString).plist")
      .path
  }

  private func browserExtensionProof(
    extensionIdentifier: String = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
    extensionVersion: String = "1.0.0",
    grantedPermissions: [String] = OneContextSystemPermissions.requiredBrowserExtensionPermissions,
    grantedHostPermissions: [String] = OneContextSystemPermissions.requiredBrowserExtensionHostPermissions
  ) -> OneContextBrowserExtensionPermissionProof {
    OneContextBrowserExtensionPermissionProof(
      extensionIdentifier: extensionIdentifier,
      extensionVersion: extensionVersion,
      grantedPermissions: grantedPermissions,
      grantedHostPermissions: grantedHostPermissions
    )
  }

  private func automationProof(
    targetName: String,
    targetBundleIdentifier: String,
    subject: OneContextPermissionSubject
  ) -> [String: Any] {
    [
      "schema_version": "1context.permission-proof.v2",
      "subject": [
        "bundle_identifier": subject.bundleIdentifier,
        "designated_requirement_sha256": subject.designatedRequirementSHA256,
        "designated_requirement": subject.designatedRequirement,
        "executable_path": subject.executablePath,
        "app_version": subject.appVersion
      ],
      "method": "apple-events-target",
      "proved_at": "2026-05-21T00:00:00Z",
      "details": [
        "target_name": targetName,
        "target_bundle_identifier": targetBundleIdentifier,
        "event_class": "wildcard",
        "event_id": "wildcard"
      ]
    ]
  }

  private func writePreferences(_ values: [String: Any], to preferencesPath: String) throws {
    let directory = URL(fileURLWithPath: preferencesPath).deletingLastPathComponent()
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    let preferences = NSMutableDictionary(dictionary: values)
    XCTAssertTrue(preferences.write(toFile: preferencesPath, atomically: true))
  }

  private func preferences(at preferencesPath: String) throws -> [String: Any] {
    let preferences = NSDictionary(contentsOfFile: preferencesPath) as? [String: Any]
    return try XCTUnwrap(preferences)
  }

  private func permissionSubject(
    bundleIdentifier: String = "com.example.1context",
    designatedRequirementSHA256: String = "requirement-digest",
    appVersion: String = "1.0.0"
  ) -> OneContextPermissionSubject {
    OneContextPermissionSubject(
      bundleIdentifier: bundleIdentifier,
      designatedRequirementSHA256: designatedRequirementSHA256,
      designatedRequirement: "identifier \"\(bundleIdentifier)\"",
      executablePath: "/Applications/1Context.app/Contents/MacOS/1Context",
      appVersion: appVersion
    )
  }

  private func permissionPlatform(
    screenPreflight: @escaping @Sendable () -> Bool = { true },
    screenRequest: @escaping @Sendable () -> Bool = { true },
    accessibilityTrusted: @escaping @Sendable () -> Bool = { true },
    accessibilityRequest: @escaping @Sendable () -> Bool = { true },
    inputPreflight: @escaping @Sendable () -> Bool = { true },
    inputRequest: @escaping @Sendable () -> Bool = { true },
    microphoneStatus: @escaping @Sendable () -> AVAuthorizationStatus = { .authorized }
  ) -> OneContextPermissionPlatform {
    OneContextPermissionPlatform(
      screenPreflight: screenPreflight,
      screenRequest: screenRequest,
      accessibilityTrusted: accessibilityTrusted,
      accessibilityRequest: accessibilityRequest,
      inputPreflight: inputPreflight,
      inputRequest: inputRequest,
      microphoneStatus: microphoneStatus
    )
  }
}

private final class PermissionTestInputState: @unchecked Sendable {
  private let lock = NSLock()
  private var storedPreflight = false
  private var storedRequested = false

  var preflight: Bool {
    get {
      lock.withLock { storedPreflight }
    }
    set {
      lock.withLock { storedPreflight = newValue }
    }
  }

  var requested: Bool {
    get {
      lock.withLock { storedRequested }
    }
    set {
      lock.withLock { storedRequested = newValue }
    }
  }
}
