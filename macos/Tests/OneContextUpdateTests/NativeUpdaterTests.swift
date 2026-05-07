import XCTest
@testable import OneContextUpdate

final class NativeUpdaterTests: XCTestCase {
  func testSparkleConfigurationRequiresFeedAndPublicKey() {
    let empty = SparkleUpdaterConfiguration(infoDictionary: [:])
    XCTAssertFalse(empty.isConfigured)
    XCTAssertEqual(empty.missingConfigurationSummary, "SUFeedURL, SUPublicEDKey")

    let missingKey = SparkleUpdaterConfiguration(infoDictionary: [
      "SUFeedURL": "https://updates.1context.localhost/appcast.xml",
      "SUEnableAutomaticChecks": true,
      "SUAutomaticallyUpdate": true,
      "SUScheduledCheckInterval": 3600
    ])
    XCTAssertFalse(missingKey.isConfigured)
    XCTAssertEqual(missingKey.feedURL?.absoluteString, "https://updates.1context.localhost/appcast.xml")
    XCTAssertTrue(missingKey.automaticChecksEnabled)
    XCTAssertTrue(missingKey.automaticDownloadsEnabled)
    XCTAssertEqual(missingKey.scheduledCheckInterval, 3600)
    XCTAssertEqual(missingKey.missingConfigurationSummary, "SUPublicEDKey")

    let configured = SparkleUpdaterConfiguration(infoDictionary: [
      "SUFeedURL": " https://updates.1context.localhost/appcast.xml ",
      "SUPublicEDKey": "ed25519-public-key",
      "SUEnableAutomaticChecks": "yes",
      "SUAutomaticallyUpdate": "yes",
      "SUScheduledCheckInterval": "1800"
    ])
    XCTAssertTrue(configured.isConfigured)
    XCTAssertEqual(configured.feedURL?.absoluteString, "https://updates.1context.localhost/appcast.xml")
    XCTAssertTrue(configured.automaticChecksEnabled)
    XCTAssertTrue(configured.automaticDownloadsEnabled)
    XCTAssertEqual(configured.scheduledCheckInterval, 1800)
    XCTAssertNil(configured.missingConfigurationSummary)
  }

  func testSparkleConfigurationRejectsInvalidFeedURL() {
    let configuration = SparkleUpdaterConfiguration(infoDictionary: [
      "SUFeedURL": "updates.xml",
      "SUPublicEDKey": "ed25519-public-key"
    ])

    XCTAssertFalse(configuration.isConfigured)
    XCTAssertNil(configuration.feedURL)
    XCTAssertEqual(configuration.missingConfigurationSummary, "SUFeedURL")
  }

  func testSparkleConfigurationReadsAppBundleInfoPlist() throws {
    let bundleURL = FileManager.default.temporaryDirectory
      .appendingPathComponent(UUID().uuidString)
      .appendingPathComponent("1Context.app", isDirectory: true)
    let contentsURL = bundleURL.appendingPathComponent("Contents", isDirectory: true)
    try FileManager.default.createDirectory(at: contentsURL, withIntermediateDirectories: true)
    let infoPlist = contentsURL.appendingPathComponent("Info.plist")
    let plist: [String: Any] = [
      "SUFeedURL": "https://updates.example.com/appcast.xml",
      "SUPublicEDKey": "ed25519-public-key",
      "SUEnableAutomaticChecks": true,
      "SUAutomaticallyUpdate": true,
      "SUScheduledCheckInterval": 3600
    ]
    let data = try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
    try data.write(to: infoPlist)
    defer {
      try? FileManager.default.removeItem(at: bundleURL.deletingLastPathComponent())
    }

    let configuration = SparkleUpdaterConfiguration(appBundleURL: bundleURL)

    XCTAssertTrue(configuration.isConfigured)
    XCTAssertEqual(configuration.feedURL?.absoluteString, "https://updates.example.com/appcast.xml")
    XCTAssertEqual(configuration.publicEdKey, "ed25519-public-key")
    XCTAssertTrue(configuration.automaticChecksEnabled)
    XCTAssertTrue(configuration.automaticDownloadsEnabled)
    XCTAssertEqual(configuration.scheduledCheckInterval, 3600)
  }

  func testAppLocationClassifiesApplicationsBundleAsInstallable() {
    let context = NativeUpdaterAppContext(
      bundleURL: URL(fileURLWithPath: "/Applications/1Context.app", isDirectory: true),
      executableURL: URL(fileURLWithPath: "/Applications/1Context.app/Contents/MacOS/1Context")
    )

    XCTAssertEqual(context.location, .applications)
    XCTAssertTrue(context.location.canInstallAppUpdates)
  }

  func testAppLocationClassifiesDownloadsBundleAsOutsideApplications() {
    let context = NativeUpdaterAppContext(
      bundleURL: URL(fileURLWithPath: "/Users/paul/Downloads/1Context.app", isDirectory: true),
      executableURL: URL(fileURLWithPath: "/Users/paul/Downloads/1Context.app/Contents/MacOS/1Context")
    )

    XCTAssertEqual(context.location, .appBundleOutsideApplications)
    XCTAssertFalse(context.location.canInstallAppUpdates)
  }

  func testAppLocationClassifiesSwiftPMExecutableAsCommandLineTool() {
    let context = NativeUpdaterAppContext(
      bundleURL: URL(fileURLWithPath: "/Users/paul/dev/1context-public-launch/.build/debug/1context"),
      executableURL: URL(fileURLWithPath: "/Users/paul/dev/1context-public-launch/.build/debug/1context")
    )

    XCTAssertEqual(context.location, .commandLineTool)
    XCTAssertFalse(context.location.canInstallAppUpdates)
  }

  func testSparkleUpdaterReportsNotConfiguredState() async {
    let updater = SparkleNativeUpdater(
      configuration: SparkleUpdaterConfiguration(feedURL: nil, publicEdKey: nil),
      appContext: NativeUpdaterAppContext(
        bundleURL: URL(fileURLWithPath: "/Applications/1Context.app", isDirectory: true),
        executableURL: nil
      ),
      driver: FakeSparkleDriver.availableUpdate
    )

    let snapshot = await updater.snapshot(currentVersion: "0.1.49")

    XCTAssertEqual(snapshot.implementation, .sparkle)
    XCTAssertEqual(snapshot.availability, .notConfigured)
    XCTAssertEqual(snapshot.appLocation, .applications)
    XCTAssertEqual(snapshot.configurationComplete, false)
    XCTAssertFalse(snapshot.updateAvailable)
    XCTAssertFalse(snapshot.canInstallFromCurrentProcess)
    XCTAssertTrue(snapshot.nextAction.contains("SUFeedURL"))
  }

  func testSparkleUpdaterRequiresApplicationsLocationBeforeCallingDriver() async {
    let updater = SparkleNativeUpdater(
      configuration: .testConfigured,
      appContext: NativeUpdaterAppContext(
        bundleURL: URL(fileURLWithPath: "/Users/paul/Downloads/1Context.app", isDirectory: true),
        executableURL: nil
      ),
      driver: FakeSparkleDriver.availableUpdate
    )

    let snapshot = await updater.snapshot(currentVersion: "0.1.49")

    XCTAssertEqual(snapshot.availability, .unavailable)
    XCTAssertEqual(snapshot.appLocation, .appBundleOutsideApplications)
    XCTAssertEqual(snapshot.configurationComplete, true)
    XCTAssertFalse(snapshot.updateAvailable)
    XCTAssertFalse(snapshot.canInstallFromCurrentProcess)
    XCTAssertTrue(snapshot.userFacingStatus.contains("Applications"))
  }

  func testSparkleUpdaterUsesDriverSnapshotWhenConfiguredAndInstalled() async {
    let updater = SparkleNativeUpdater(
      configuration: .testConfigured,
      appContext: .testApplicationsApp,
      driver: FakeSparkleDriver.availableUpdate
    )

    let snapshot = await updater.snapshot(currentVersion: "0.1.49")

    XCTAssertEqual(snapshot.availability, .available)
    XCTAssertEqual(snapshot.currentVersion, "0.1.49")
    XCTAssertEqual(snapshot.latestVersion, "0.1.50")
    XCTAssertTrue(snapshot.updateAvailable)
    XCTAssertFalse(snapshot.mandatoryUpdateAvailable)
    XCTAssertTrue(snapshot.canInstallFromCurrentProcess)
    XCTAssertEqual(snapshot.userFacingStatus, "1Context 0.1.50 is available.")
    XCTAssertEqual(snapshot.nextAction, "Install from the app.")
  }

  func testSparkleUpdaterSurfacesMandatoryUpdateMetadata() async {
    let updater = SparkleNativeUpdater(
      configuration: .testConfigured,
      appContext: .testApplicationsApp,
      driver: FakeSparkleDriver.mandatoryUpdate
    )

    let snapshot = await updater.snapshot(currentVersion: "0.1.49")

    XCTAssertTrue(snapshot.updateAvailable)
    XCTAssertTrue(snapshot.mandatoryUpdateAvailable)
    XCTAssertEqual(snapshot.minimumUpdateVersion, "0.1.49")
    XCTAssertEqual(snapshot.minimumAutoupdateVersion, "0.1.49")
    XCTAssertEqual(snapshot.userFacingStatus, "1Context 0.1.51 is a mandatory update.")
  }

  func testMandatoryUpdateRuntimePolicyOnlyPausesForAvailableMandatoryUpdates() {
    let mandatory = NativeUpdateSnapshot(
      implementation: .sparkle,
      availability: .available,
      currentVersion: "0.1.50",
      latestVersion: "0.1.51",
      updateAvailable: true,
      mandatoryUpdateAvailable: true,
      canInstallFromCurrentProcess: true,
      userFacingStatus: "1Context 0.1.51 is a mandatory update.",
      nextAction: "Install from the app."
    )
    let normal = NativeUpdateSnapshot(
      implementation: .sparkle,
      availability: .available,
      currentVersion: "0.1.50",
      latestVersion: "0.1.51",
      updateAvailable: true,
      mandatoryUpdateAvailable: false,
      canInstallFromCurrentProcess: true,
      userFacingStatus: "1Context 0.1.51 is available.",
      nextAction: "Install from the app."
    )
    let unavailable = NativeUpdateSnapshot(
      implementation: .sparkle,
      availability: .unavailable,
      currentVersion: "0.1.50",
      latestVersion: nil,
      updateAvailable: false,
      mandatoryUpdateAvailable: true,
      canInstallFromCurrentProcess: false,
      userFacingStatus: "Move 1Context to Applications to install app updates.",
      nextAction: "Open 1Context from /Applications/1Context.app."
    )

    XCTAssertTrue(MandatoryUpdateRuntimePolicy.shouldPausePassiveRemembering(mandatory))
    XCTAssertEqual(
      MandatoryUpdateRuntimePolicy.startBlockedMessage(mandatory),
      "1Context 0.1.51 is mandatory. Update before starting passive remembering."
    )
    XCTAssertFalse(MandatoryUpdateRuntimePolicy.shouldPausePassiveRemembering(normal))
    XCTAssertFalse(MandatoryUpdateRuntimePolicy.shouldPausePassiveRemembering(unavailable))
  }

  func testNativeUpdateDiagnosticsRenderStableLines() {
    let snapshot = NativeUpdateSnapshot(
      implementation: .sparkle,
      availability: .available,
      currentVersion: "0.1.49",
      latestVersion: "0.1.50",
      updateAvailable: true,
      canInstallFromCurrentProcess: true,
      userFacingStatus: "1Context 0.1.50 is available.",
      nextAction: "Install from the app."
    )

    let lines = NativeUpdateDiagnostics.render(snapshot)

    XCTAssertEqual(lines[0], "  Native Updater: available")
    XCTAssertEqual(lines[1], "  Implementation: sparkle")
    XCTAssertEqual(lines[2], "  Current Version: 0.1.49")
    XCTAssertEqual(lines[3], "  Latest Version: 0.1.50")
    XCTAssertEqual(lines[4], "  Update Available: yes")
    XCTAssertEqual(lines[5], "  Mandatory Update: no")
    XCTAssertEqual(lines[6], "  Can Install Here: yes")
    XCTAssertEqual(lines[7], "  Status: 1Context 0.1.50 is available.")
    XCTAssertEqual(lines[8], "  Next Action: Install from the app.")
  }

  func testNativeUpdateDiagnosticsIncludeSparkleContextWhenPresent() async {
    let snapshot = await SparkleNativeUpdater(
      configuration: .testConfigured,
      appContext: .testApplicationsApp,
      driver: FakeSparkleDriver.noUpdate
    ).snapshot(currentVersion: "0.1.49")

    let lines = NativeUpdateDiagnostics.render(snapshot)

    XCTAssertTrue(lines.contains("  App Location: Applications"))
    XCTAssertTrue(lines.contains("  Configuration: complete"))
    XCTAssertTrue(lines.contains("  Feed URL: https://updates.1context.localhost/appcast.xml"))
    XCTAssertTrue(lines.contains("  Automatic Checks: yes"))
    XCTAssertTrue(lines.contains("  Automatic Downloads: yes"))
    XCTAssertTrue(lines.contains("  Scheduled Check Interval: 3600s"))
  }
}

private struct FakeSparkleDriver: SparkleUpdateDriver {
  static let availableUpdate = FakeSparkleDriver(snapshot: SparkleUpdateDriverSnapshot(
    availability: .available,
    latestVersion: "0.1.50",
    updateAvailable: true,
    canInstallUpdates: true,
    userFacingStatus: "1Context 0.1.50 is available.",
    nextAction: "Install from the app."
  ))

  static let noUpdate = FakeSparkleDriver(snapshot: SparkleUpdateDriverSnapshot(
    availability: .available,
    latestVersion: "0.1.49",
    updateAvailable: false,
    canInstallUpdates: true,
    userFacingStatus: "1Context is up to date.",
    nextAction: "No action needed."
  ))

  static let mandatoryUpdate = FakeSparkleDriver(snapshot: SparkleUpdateDriverSnapshot(
    availability: .available,
    latestVersion: "0.1.51",
    updateAvailable: true,
    mandatoryUpdateAvailable: true,
    minimumUpdateVersion: "0.1.49",
    minimumAutoupdateVersion: "0.1.49",
    canInstallUpdates: true,
    userFacingStatus: "1Context 0.1.51 is a mandatory update.",
    nextAction: "Install from the app."
  ))

  let snapshot: SparkleUpdateDriverSnapshot

  func snapshot(
    currentVersion: String,
    configuration: SparkleUpdaterConfiguration
  ) async -> SparkleUpdateDriverSnapshot {
    snapshot
  }
}

private extension SparkleUpdaterConfiguration {
  static let testConfigured = SparkleUpdaterConfiguration(
    feedURL: URL(string: "https://updates.1context.localhost/appcast.xml"),
    publicEdKey: "ed25519-public-key",
    automaticChecksEnabled: true,
    automaticDownloadsEnabled: true,
    scheduledCheckInterval: 3600
  )
}

private extension NativeUpdaterAppContext {
  static let testApplicationsApp = NativeUpdaterAppContext(
    bundleURL: URL(fileURLWithPath: "/Applications/1Context.app", isDirectory: true),
    executableURL: URL(fileURLWithPath: "/Applications/1Context.app/Contents/MacOS/1Context")
  )
}
