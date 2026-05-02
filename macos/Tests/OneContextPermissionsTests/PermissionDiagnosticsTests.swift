import XCTest
@testable import OneContextPermissions

final class PermissionDiagnosticsTests: XCTestCase {
  func testDefaultCLIReporterDoesNotCheckSensitivePermissions() {
    let snapshots = PermissionReporter(
      checker: MacOSPermissionChecker(
        owner: .mainApp,
        currentBundleIdentifier: nil,
        checkCurrentProcess: false
      )
    ).snapshots()

    XCTAssertEqual(snapshots.map(\.kind), [.screenRecording, .accessibility])
    XCTAssertTrue(snapshots.allSatisfy { $0.status == .notChecked })
    XCTAssertTrue(snapshots.allSatisfy { !$0.checkedByCurrentProcess })
    XCTAssertTrue(snapshots.allSatisfy { !$0.canPromptFromCurrentProcess })
    XCTAssertTrue(snapshots.allSatisfy { $0.owner.bundleIdentifier == "com.haptica.1context" })
  }

  func testPermissionDiagnosticsRenderStableOwnerAndNextAction() {
    let snapshot = PermissionSnapshot(
      kind: .screenRecording,
      status: .notChecked,
      owner: .mainApp,
      checkedByCurrentProcess: false,
      canPromptFromCurrentProcess: false,
      requiresRelaunchAfterGrant: true,
      reason: "1Context.app owns Screen Recording consent.",
      repairHint: "Open 1Context.app, then enable Screen Recording."
    )

    let lines = PermissionDiagnostics.render(snapshot)

    XCTAssertEqual(lines[0], "  Screen Recording: not checked")
    XCTAssertEqual(lines[1], "    Owner: 1Context.app (com.haptica.1context)")
    XCTAssertEqual(lines[2], "    Checked Here: no")
    XCTAssertEqual(lines[3], "    Can Prompt Here: no")
    XCTAssertEqual(lines[4], "    Relaunch After Grant: yes")
    XCTAssertEqual(lines[5], "    Reason: 1Context.app owns Screen Recording consent.")
    XCTAssertEqual(lines[6], "    Next Action: Open 1Context.app, then enable Screen Recording.")
  }

  func testPermissionSnapshotIsCodable() throws {
    let snapshot = PermissionSnapshot(
      kind: .accessibility,
      status: .granted,
      owner: .mainApp,
      checkedByCurrentProcess: true,
      canPromptFromCurrentProcess: true,
      requiresRelaunchAfterGrant: true,
      reason: "Needed for future automation.",
      repairHint: "No action needed."
    )

    let data = try JSONEncoder().encode(snapshot)
    let decoded = try JSONDecoder().decode(PermissionSnapshot.self, from: data)

    XCTAssertEqual(decoded, snapshot)
  }
}
