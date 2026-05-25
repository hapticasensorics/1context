import XCTest
@testable import OneContextCapture

final class CaptureStatusMetadataTests: XCTestCase {
  func testPermissionDerivedMetadataSerializesDashboardFieldsWithoutSensitiveInput() throws {
    let daemon = CaptureProcessIdentity(
      role: "daemon_process",
      pid: 42,
      executablePath: "/Applications/1Context Dev.app/Contents/MacOS/1contextd",
      bundleIdentifier: "com.haptica.1context.dev"
    )
    let subject = CaptureProcessIdentity(
      role: "permission_subject",
      executablePath: "/Applications/1Context Dev.app/Contents/MacOS/1Context",
      bundleIdentifier: "com.haptica.1context.dev",
      appVersion: "99",
      designatedRequirementSHA256: "abc123"
    )
    let metadata = CapturePermissionDerivedMetadata(
      generatedAt: "2026-05-24T12:00:00Z",
      processIdentities: [daemon, subject],
      capturePaths: CaptureStatusPathMetadata(
        rootDirectory: "/tmp/capture",
        eventsDirectory: "/tmp/capture/events",
        windowsDirectory: "/tmp/capture/windows",
        mediaDirectory: "/tmp/capture/media"
      ),
      signals: [
        "input_monitoring": CapturePermissionSignalMetadata(
          ready: true,
          status: "granted",
          source: "persistent listen-only CGEventTap owned by 1contextd",
          ownerRole: daemon.role,
          permissionSubjectRole: subject.role,
          eventTap: CaptureInputEventTapMetadata(
            active: true,
            lifecycleState: "running",
            eventTap: "cgSessionEventTap",
            tapOptions: "listenOnly",
            eventMask: ["key_down", "scroll_wheel"],
            observedEventCount: 7,
            queueDepth: 0,
            droppedCount: 0,
            coalescedCount: 3
          ),
          proof: CapturePermissionProofSummary(
            proofKey: "RememberingInputMonitoringProof",
            recorded: true,
            matchesCurrentSubject: true,
            method: "iohid-preflight-listen-event"
          )
        )
      ]
    )

    let object = try XCTUnwrap(CaptureJSON.dictionary(metadata) as [String: Any]?)
    XCTAssertEqual(object["schema_version"] as? Int, 1)

    let privacy = try XCTUnwrap(object["privacy"] as? [String: Any])
    XCTAssertEqual(privacy["raw_keystrokes_included"] as? Bool, false)
    XCTAssertEqual(privacy["raw_text_included"] as? Bool, false)
    XCTAssertEqual(privacy["coordinates_included"] as? Bool, false)
    XCTAssertEqual(privacy["aggregates_and_counts_only"] as? Bool, true)

    let signals = try XCTUnwrap(object["signals"] as? [String: Any])
    let inputMonitoring = try XCTUnwrap(signals["input_monitoring"] as? [String: Any])
    XCTAssertEqual(inputMonitoring["owner_role"] as? String, "daemon_process")
    XCTAssertEqual(inputMonitoring["permission_subject_role"] as? String, "permission_subject")
    let eventTap = try XCTUnwrap(inputMonitoring["event_tap"] as? [String: Any])
    XCTAssertEqual(eventTap["observed_event_count"] as? Int, 7)
    XCTAssertEqual(eventTap["queue_depth"] as? Int, 0)

    let json = String(decoding: try JSONEncoder().encode(metadata), as: UTF8.self)
    XCTAssertFalse(json.contains("keyCode"))
    XCTAssertFalse(json.contains("key_code"))
    XCTAssertFalse(json.contains("characters"))
    XCTAssertFalse(json.contains("\"text\""))
    XCTAssertFalse(json.contains("locationX"))
    XCTAssertFalse(json.contains("locationY"))
  }
}
