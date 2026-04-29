import XCTest
@testable import OneContextUpdate

final class UpdateCheckerTests: XCTestCase {
  func testCachedUpdateStateReportsAvailableUpdate() throws {
    let root = FileManager.default.temporaryDirectory
      .appendingPathComponent("1ctx-update-\(UUID().uuidString)", isDirectory: true)
    let updateDir = root.appendingPathComponent("update", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: updateDir, withIntermediateDirectories: true)
    try Data("""
    {
      "last_checked_at": "2026-04-29T00:00:00Z",
      "last_seen_latest": "0.1.32",
      "notes_url": "https://github.com/hapticasensorics/1context/releases/tag/v0.1.32"
    }
    """.utf8).write(to: updateDir.appendingPathComponent("update-check.json"))

    let checker = UpdateChecker(environment: [
      "ONECONTEXT_APP_SUPPORT_DIR": root.appendingPathComponent("support").path,
      "ONECONTEXT_UPDATE_STATE_DIR": updateDir.path
    ])

    let cached = checker.cached(currentVersion: "0.1.26")

    XCTAssertEqual(cached?.latest?.version, "0.1.32")
    XCTAssertEqual(cached?.latest?.notesURL?.absoluteString, "https://github.com/hapticasensorics/1context/releases/tag/v0.1.32")
    XCTAssertEqual(cached?.updateAvailable, true)
    XCTAssertEqual(cached?.checked, false)
  }
}
