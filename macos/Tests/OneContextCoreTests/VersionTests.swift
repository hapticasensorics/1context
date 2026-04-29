import XCTest
@testable import OneContextCore

final class VersionTests: XCTestCase {
  func testCompareVersionsHandlesBasicSemver() {
    XCTAssertEqual(compareVersions("0.1.26", "0.1.26"), 0)
    XCTAssertGreaterThan(compareVersions("0.1.27", "0.1.26"), 0)
    XCTAssertLessThan(compareVersions("0.1.9", "0.1.10"), 0)
    XCTAssertEqual(compareVersions("v0.1.26", "0.1.26"), 0)
  }

  func testRuntimeHealthDecodesFromRPCPayload() throws {
    let data = Data("""
    {"status":"ok","version":"0.1.26","uptimeSeconds":12,"pid":42}
    """.utf8)
    let health = try JSONDecoder().decode(RuntimeHealth.self, from: data)

    XCTAssertEqual(health.status, "ok")
    XCTAssertEqual(health.version, "0.1.26")
    XCTAssertEqual(health.uptimeSeconds, 12)
    XCTAssertEqual(health.pid, 42)
  }

  func testRuntimeSnapshotEncodesCanonicalState() throws {
    let snapshot = RuntimeSnapshot(
      state: .needsAttention,
      health: RuntimeHealth(status: "ok", version: "0.1.25", uptimeSeconds: 1, pid: 99),
      lastErrorDescription: "Wrong runtime version",
      recommendedAction: "Restart 1Context"
    )

    let data = try JSONEncoder().encode(snapshot)
    let decoded = try JSONDecoder().decode(RuntimeSnapshot.self, from: data)

    XCTAssertEqual(decoded.state, .needsAttention)
    XCTAssertEqual(decoded.health?.version, "0.1.25")
    XCTAssertEqual(decoded.recommendedAction, "Restart 1Context")
  }
}
