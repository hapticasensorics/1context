import XCTest
@testable import OneContextCore

final class VersionTests: XCTestCase {
  func testVersionResolutionIgnoresEnvironmentOverrides() {
    let version = OneContextVersion.resolve(
      environment: ["ONECONTEXT_VERSION_OVERRIDE": "9.9.9"],
      bundleVersion: "1.0.0",
      executableURL: nil,
      appBundleVersion: { _ in nil }
    )

    XCTAssertEqual(version, "1.0.0")
  }

  func testVersionResolutionUsesMainBundleVersionBeforeFallback() {
    let version = OneContextVersion.resolve(
      environment: [:],
      bundleVersion: "1.2.3",
      executableURL: nil,
      appBundleVersion: { _ in nil }
    )

    XCTAssertEqual(version, "1.2.3")
  }

  func testVersionResolutionFindsContainingAppBundleForEmbeddedTools() {
    let executableURL = URL(fileURLWithPath: "/Applications/1Context.app/Contents/MacOS/1context-cli")
    let version = OneContextVersion.resolve(
      environment: [:],
      bundleVersion: nil,
      executableURL: executableURL,
      appBundleVersion: { appURL in
        appURL.path == "/Applications/1Context.app" ? "2.3.4" : nil
      }
    )

    XCTAssertEqual(version, "2.3.4")
  }

  func testVersionResolutionFallsBackForSwiftPMExecutables() {
    let version = OneContextVersion.resolve(
      environment: [:],
      bundleVersion: nil,
      executableURL: URL(fileURLWithPath: "/tmp/1context"),
      appBundleVersion: { _ in nil }
    )

    XCTAssertEqual(version, OneContextVersion.fallback)
  }

  func testCompareVersionsHandlesBasicSemver() {
    XCTAssertEqual(compareVersions("0.1.33", "0.1.33"), 0)
    XCTAssertGreaterThan(compareVersions("0.1.34", "0.1.33"), 0)
    XCTAssertLessThan(compareVersions("0.1.9", "0.1.10"), 0)
    XCTAssertEqual(compareVersions("v0.1.33", "0.1.33"), 0)
  }

  func testRuntimeHealthDecodesFromRPCPayload() throws {
    let data = Data("""
    {"status":"ok","version":"0.1.33","currentTime":"2026-04-29T11:30:00Z","uptimeSeconds":12,"pid":42}
    """.utf8)
    let health = try JSONDecoder().decode(RuntimeHealth.self, from: data)

    XCTAssertEqual(health.status, "ok")
    XCTAssertEqual(health.version, "0.1.33")
    XCTAssertEqual(health.currentTime, "2026-04-29T11:30:00Z")
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
