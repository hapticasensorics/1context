import Foundation
import XCTest
@testable import OneContextInstall

final class AppInstallTests: XCTestCase {
  func testPlannerContinuesWhenPromptIsSkipped() {
    let planner = AppInstallPlanner(environment: [
      AppInstallEnvironment.skipPromptKey: "1"
    ])

    let recommendation = planner.recommendation(
      currentBundleURL: URL(fileURLWithPath: "/tmp/1Context.app"),
      currentVersion: "1.2.3"
    )

    XCTAssertEqual(recommendation, .continueInPlace("Install prompt disabled by environment."))
  }

  func testPlannerContinuesWhenAlreadyInApplicationsDestination() {
    let destination = URL(fileURLWithPath: "/Applications/1Context.app", isDirectory: true)
    let planner = AppInstallPlanner(environment: [:])

    let recommendation = planner.recommendation(
      currentBundleURL: destination,
      currentVersion: "1.2.3"
    )

    XCTAssertEqual(recommendation, .continueInPlace("1Context is already running from Applications."))
  }

  func testPlannerOffersMoveWhenRunningFromDownloads() {
    let temporaryDestination = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1Context-tests-\(UUID().uuidString)")
      .appendingPathComponent("Applications/1Context.app", isDirectory: true)
    let planner = AppInstallPlanner(environment: [
      AppInstallEnvironment.destinationKey: temporaryDestination.path
    ])
    let current = URL(fileURLWithPath: "/Users/example/Downloads/1Context.app", isDirectory: true)

    let recommendation = planner.recommendation(
      currentBundleURL: current,
      currentVersion: "1.2.3"
    )

    guard case .moveToApplications(let request) = recommendation else {
      return XCTFail("Expected move recommendation, got \(recommendation)")
    }
    XCTAssertEqual(request.currentBundleURL, current)
    XCTAssertEqual(request.destinationBundleURL, temporaryDestination)
    XCTAssertEqual(request.currentVersion, "1.2.3")
    XCTAssertNil(request.existingVersion)
    XCTAssertEqual(request.existingRelation, .none)
  }

  func testPlannerOffersMoveWhenRunningFromMountedDMGOrTranslocatedPath() {
    let temporaryDestination = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1Context-tests-\(UUID().uuidString)")
      .appendingPathComponent("Applications/1Context.app", isDirectory: true)
    let planner = AppInstallPlanner(environment: [
      AppInstallEnvironment.destinationKey: temporaryDestination.path
    ])

    for current in [
      URL(fileURLWithPath: "/Volumes/1Context/1Context.app", isDirectory: true),
      URL(fileURLWithPath: "/private/var/folders/xx/AppTranslocation/1Context.app", isDirectory: true)
    ] {
      let recommendation = planner.recommendation(
        currentBundleURL: current,
        currentVersion: "1.2.3"
      )
      guard case .moveToApplications(let request) = recommendation else {
        return XCTFail("Expected move recommendation for \(current.path), got \(recommendation)")
      }
      XCTAssertEqual(request.currentBundleURL, current)
      XCTAssertEqual(request.destinationBundleURL, temporaryDestination)
    }
  }

  func testPlannerClassifiesExistingVersions() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1Context-install-tests-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let destination = root.appendingPathComponent("Applications/1Context.app", isDirectory: true)
    try writeBundleVersion("1.2.2", to: destination)

    let planner = AppInstallPlanner(environment: [
      AppInstallEnvironment.destinationKey: destination.path
    ])
    let current = root.appendingPathComponent("Downloads/1Context.app", isDirectory: true)

    var recommendation = planner.recommendation(
      currentBundleURL: current,
      currentVersion: "1.2.3"
    )
    XCTAssertEqual(moveRequest(from: recommendation)?.existingVersion, "1.2.2")
    XCTAssertEqual(moveRequest(from: recommendation)?.existingRelation, .olderVersion)

    try writeBundleVersion("1.2.3", to: destination)
    recommendation = planner.recommendation(currentBundleURL: current, currentVersion: "1.2.3")
    XCTAssertEqual(moveRequest(from: recommendation)?.existingRelation, .sameVersion)

    try writeBundleVersion("1.2.4", to: destination)
    recommendation = planner.recommendation(currentBundleURL: current, currentVersion: "1.2.3")
    XCTAssertEqual(moveRequest(from: recommendation)?.existingRelation, .newerVersion)
  }

  func testPlannerDoesNotTreatSameVersionLegacyInstallAsCurrent() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1Context-install-tests-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let current = root.appendingPathComponent("Downloads/1Context.app", isDirectory: true)
    let destination = root.appendingPathComponent("Applications/1Context.app", isDirectory: true)
    try writeBundleVersion("1.2.3", bundleIdentifier: "com.haptica.1context", to: current)
    try writeExecutable("new-proxy", to: current.appendingPathComponent("Contents/Resources/1context-local-web-proxy"))
    try writeBundleVersion("1.2.3", bundleIdentifier: "com.haptica.1context.menu", to: destination)

    let planner = AppInstallPlanner(environment: [
      AppInstallEnvironment.destinationKey: destination.path
    ])
    let request = try XCTUnwrap(moveRequest(from: planner.recommendation(
      currentBundleURL: current,
      currentVersion: "1.2.3"
    )))

    XCTAssertEqual(request.existingRelation, .sameVersion)
    XCTAssertFalse(request.existingInstallMatchesCurrent)
  }

  func testPlannerRecognizesSameInstalledBuild() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1Context-install-tests-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }
    let current = root.appendingPathComponent("Downloads/1Context.app", isDirectory: true)
    let destination = root.appendingPathComponent("Applications/1Context.app", isDirectory: true)
    try writeBundleVersion("1.2.3", bundleIdentifier: "com.haptica.1context", to: current)
    try writeExecutable("proxy", to: current.appendingPathComponent("Contents/Resources/1context-local-web-proxy"))
    try writeBundleVersion("1.2.3", bundleIdentifier: "com.haptica.1context", to: destination)
    try writeExecutable("proxy", to: destination.appendingPathComponent("Contents/Resources/1context-local-web-proxy"))

    let planner = AppInstallPlanner(environment: [
      AppInstallEnvironment.destinationKey: destination.path
    ])
    let request = try XCTUnwrap(moveRequest(from: planner.recommendation(
      currentBundleURL: current,
      currentVersion: "1.2.3"
    )))

    XCTAssertEqual(request.existingRelation, .sameVersion)
    XCTAssertTrue(request.existingInstallMatchesCurrent)
  }

  func testMoverCopiesBundleAndReplacesExistingApp() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1Context-install-move-tests-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }

    let source = root.appendingPathComponent("Downloads/1Context.app", isDirectory: true)
    let destination = root.appendingPathComponent("Applications/1Context.app", isDirectory: true)
    try writeBundleVersion("1.2.3", to: source)
    try writeExecutable("new", to: source.appendingPathComponent("Contents/MacOS/1Context"))
    try writeBundleVersion("1.2.2", to: destination)
    try writeExecutable("old", to: destination.appendingPathComponent("Contents/MacOS/1Context"))

    let request = AppInstallRequest(
      currentBundleURL: source,
      destinationBundleURL: destination,
      currentVersion: "1.2.3",
      existingVersion: "1.2.2",
      existingRelation: .olderVersion
    )

    try AppInstallMover().install(request)

    XCTAssertEqual(AppInstallPlanner.bundleVersion(at: destination), "1.2.3")
    let installedExecutable = destination.appendingPathComponent("Contents/MacOS/1Context")
    XCTAssertEqual(try String(contentsOf: installedExecutable, encoding: .utf8), "new")
    XCTAssertTrue(FileManager.default.isExecutableFile(atPath: installedExecutable.path))
  }

  func testTrasherMoves1ContextBundleToConfiguredTrashDestination() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1Context-trash-tests-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }

    let bundle = root.appendingPathComponent("Applications/1Context.app", isDirectory: true)
    let trash = root.appendingPathComponent("Trash", isDirectory: true)
    try writeBundleVersion("1.2.3", bundleIdentifier: "com.haptica.1context", to: bundle)

    let trashed = try AppBundleTrasher(environment: [
      AppBundleTrashEnvironment.allowNonApplicationsKey: "1",
      AppBundleTrashEnvironment.trashDestinationKey: trash.path
    ]).trash(bundle)

    XCTAssertFalse(FileManager.default.fileExists(atPath: bundle.path))
    XCTAssertEqual(trashed?.deletingLastPathComponent(), trash)
    XCTAssertTrue(FileManager.default.fileExists(atPath: trash.appendingPathComponent("1Context.app").path))
  }

  func testTrasherRefusesWrongBundleIdentifier() throws {
    let root = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("1Context-trash-tests-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }

    let bundle = root.appendingPathComponent("Applications/Other.app", isDirectory: true)
    try writeBundleVersion("1.2.3", bundleIdentifier: "com.example.other", to: bundle)

    XCTAssertThrowsError(try AppBundleTrasher(environment: [
      AppBundleTrashEnvironment.allowNonApplicationsKey: "1",
      AppBundleTrashEnvironment.trashDestinationKey: root.appendingPathComponent("Trash").path
    ]).trash(bundle)) { error in
      XCTAssertEqual(error as? AppBundleTrashError, .wrongBundleIdentifier("com.example.other"))
    }
  }

  func testTrasherIsIdempotentWhenBundleIsAlreadyMissing() throws {
    let missing = URL(fileURLWithPath: NSTemporaryDirectory())
      .appendingPathComponent("missing-1Context.app", isDirectory: true)

    let trashed = try AppBundleTrasher(environment: [
      AppBundleTrashEnvironment.allowNonApplicationsKey: "1"
    ]).trash(missing)

    XCTAssertNil(trashed)
  }

  private func moveRequest(from recommendation: AppInstallRecommendation) -> AppInstallRequest? {
    guard case .moveToApplications(let request) = recommendation else { return nil }
    return request
  }

  private func writeBundleVersion(
    _ version: String,
    bundleIdentifier: String? = nil,
    to bundle: URL
  ) throws {
    let contents = bundle.appendingPathComponent("Contents", isDirectory: true)
    try FileManager.default.createDirectory(at: contents, withIntermediateDirectories: true)
    var plist: [String: Any] = ["CFBundleShortVersionString": version]
    if let bundleIdentifier {
      plist["CFBundleIdentifier"] = bundleIdentifier
    }
    let data = try PropertyListSerialization.data(fromPropertyList: plist, format: .xml, options: 0)
    try data.write(to: contents.appendingPathComponent("Info.plist"))
  }

  private func writeExecutable(_ text: String, to url: URL) throws {
    try FileManager.default.createDirectory(
      at: url.deletingLastPathComponent(),
      withIntermediateDirectories: true
    )
    try text.write(to: url, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: url.path)
  }

}
