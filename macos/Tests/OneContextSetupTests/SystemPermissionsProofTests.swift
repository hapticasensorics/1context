import AVFoundation
import XCTest
import OneContextSetup

final class SystemPermissionsProofTests: XCTestCase {
  func testInputMonitoringPreflightAloneDoesNotGrantUntilSubjectProofIsRefreshed() async {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    let platform = permissionPlatform(inputPreflight: { true })

    XCTAssertTrue(OneContextSystemPermissions.inputMonitoringRuntimeGranted(platform: platform))
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
      platform: permissionPlatform(
        screenPreflight: { false },
        inputPreflight: { true },
        microphoneStatus: { .denied }
      )
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

  func testInputMonitoringRequestResultCannotBypassPostRequestPreflight() {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    let inputState = PermissionProofInputState()
    let platform = permissionPlatform(
      inputPreflight: { inputState.preflight },
      inputRequest: {
        inputState.requested = true
        return true
      }
    )

    XCTAssertFalse(
      OneContextSystemPermissions.requestInputMonitoring(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: platform
      )
    )
    XCTAssertTrue(inputState.requested)
    XCTAssertFalse(
      OneContextSystemPermissions.inputMonitoringGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: permissionPlatform(inputPreflight: { true })
      )
    )
  }

  func testMicrophoneRequiresAuthorizedStatusAndMatchingRuntimeProof() throws {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    let authorizedPlatform = permissionPlatform(microphoneStatus: { .authorized })

    XCTAssertFalse(
      OneContextSystemPermissions.microphoneGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: authorizedPlatform
      )
    )

    try writeProof(
      key: OneContextSystemPermissions.microphoneProofKey,
      subject: subject,
      preferencesPath: preferencesPath,
      method: "avcapture-audio-session",
      details: ["device_unique_id": "deterministic-test-device"]
    )

    XCTAssertTrue(
      OneContextSystemPermissions.microphoneGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: authorizedPlatform
      )
    )
    XCTAssertFalse(
      OneContextSystemPermissions.microphoneGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: permissionPlatform(microphoneStatus: { .denied })
      )
    )
    XCTAssertFalse(
      OneContextSystemPermissions.microphoneGranted(
        preferencesPath: preferencesPath,
        subject: permissionSubject(designatedRequirementSHA256: "different-requirement"),
        platform: authorizedPlatform
      )
    )
  }

  func testScreenAndSystemAudioRequiresPreflightAndBothRuntimeProofs() throws {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    let authorizedPlatform = permissionPlatform(screenPreflight: { true })

    XCTAssertFalse(
      OneContextSystemPermissions.screenRecordingGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: authorizedPlatform
      )
    )

    try writeProof(
      key: OneContextSystemPermissions.screenCaptureProofKey,
      subject: subject,
      preferencesPath: preferencesPath,
      method: "screencapturekit-screen-frame"
    )
    XCTAssertFalse(
      OneContextSystemPermissions.screenRecordingGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: authorizedPlatform
      )
    )

    try writeProof(
      key: OneContextSystemPermissions.systemAudioProofKey,
      subject: subject,
      preferencesPath: preferencesPath,
      method: "screencapturekit-system-audio-stream"
    )
    XCTAssertTrue(
      OneContextSystemPermissions.screenRecordingGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: authorizedPlatform
      )
    )
    XCTAssertFalse(
      OneContextSystemPermissions.screenRecordingGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: permissionPlatform(screenPreflight: { false })
      )
    )
  }

  func testScreenAndSystemAudioRequestCanRequireRelaunchWithoutTurningReady() async {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()
    let requestPlatform = permissionPlatform(
      screenPreflight: { false },
      screenRequest: { true },
      inputPreflight: { false },
      microphoneStatus: { .denied }
    )

    let outcome = await OneContextSystemPermissions.requestScreenAndSystemAudio(
      preferencesPath: preferencesPath,
      subject: subject,
      platform: requestPlatform
    )

    XCTAssertEqual(outcome, .needsRelaunch)
    let postGrantPlatform = permissionPlatform(
      screenPreflight: { true },
      inputPreflight: { false },
      microphoneStatus: { .denied }
    )
    XCTAssertFalse(
      OneContextSystemPermissions.screenRecordingGranted(
        preferencesPath: preferencesPath,
        subject: subject,
        platform: postGrantPlatform
      )
    )

    let snapshot = OneContextSystemPermissions.current(
      preferencesPath: preferencesPath,
      subject: subject,
      platform: postGrantPlatform
    )
    XCTAssertEqual(snapshot.screenRecording.status, .needsRelaunch)
  }

  func testPreviousProcessRelaunchProofDoesNotKeepScreenCaptureInRelaunchState() throws {
    let preferencesPath = temporaryPreferencesPath()
    let subject = permissionSubject()

    try writeProof(
      key: OneContextSystemPermissions.screenCaptureRelaunchProofKey,
      subject: subject,
      preferencesPath: preferencesPath,
      method: "coregraphics-screen-requested-relaunch-required",
      details: ["process_start": "previous-process"]
    )

    let snapshot = OneContextSystemPermissions.current(
      preferencesPath: preferencesPath,
      subject: subject,
      platform: permissionPlatform(
        screenPreflight: { true },
        inputPreflight: { false },
        microphoneStatus: { .denied }
      )
    )

    XCTAssertEqual(snapshot.screenRecording.status, .required)
  }

  func testCurrentInvalidatesStaleProofsForChangedSignedSubject() throws {
    let preferencesPath = temporaryPreferencesPath()
    let oldSubject = permissionSubject(
      bundleIdentifier: "com.example.1context.old",
      designatedRequirementSHA256: "old-requirement",
      appVersion: "1.0.0"
    )
    let newSubject = permissionSubject(
      bundleIdentifier: "com.example.1context.new",
      designatedRequirementSHA256: "new-requirement",
      appVersion: "1.0.1"
    )

    for key in [
      OneContextSystemPermissions.inputMonitoringProofKey,
      OneContextSystemPermissions.browserExtensionProofKey,
      OneContextSystemPermissions.screenCaptureProofKey,
      OneContextSystemPermissions.systemAudioProofKey,
      OneContextSystemPermissions.screenCaptureRelaunchProofKey,
      OneContextSystemPermissions.microphoneProofKey
    ] {
      try writeProof(
        key: key,
        subject: oldSubject,
        preferencesPath: preferencesPath,
        method: "stale-test-proof"
      )
    }
    try writePreferenceValue(
      [
        "com.apple.finder": proofDictionary(
          method: "apple-events-target",
          subject: oldSubject,
          details: ["target_bundle_identifier": "com.apple.finder"]
        )
      ],
      for: OneContextSystemPermissions.automationProofsKey,
      preferencesPath: preferencesPath
    )

    let snapshot = OneContextSystemPermissions.current(
      preferencesPath: preferencesPath,
      subject: newSubject,
      platform: permissionPlatform(
        screenPreflight: { true },
        inputPreflight: { true },
        microphoneStatus: { .authorized }
      )
    )

    XCTAssertEqual(snapshot.screenRecording.status, .required)
    XCTAssertEqual(snapshot.inputMonitoring.status, .required)
    XCTAssertEqual(snapshot.browserExtension.status, .notRequired)
    XCTAssertEqual(snapshot.microphone.status, .required)

    let preferences = try XCTUnwrap(NSDictionary(contentsOfFile: preferencesPath) as? [String: Any])
    XCTAssertNil(preferences[OneContextSystemPermissions.inputMonitoringProofKey])
    XCTAssertNil(preferences[OneContextSystemPermissions.browserExtensionProofKey])
    XCTAssertNil(preferences[OneContextSystemPermissions.screenCaptureProofKey])
    XCTAssertNil(preferences[OneContextSystemPermissions.systemAudioProofKey])
    XCTAssertNil(preferences[OneContextSystemPermissions.screenCaptureRelaunchProofKey])
    XCTAssertNil(preferences[OneContextSystemPermissions.microphoneProofKey])

    let automationProofs = try XCTUnwrap(preferences[OneContextSystemPermissions.automationProofsKey] as? [String: Any])
    XCTAssertTrue(automationProofs.isEmpty)
  }

  private func temporaryPreferencesPath() -> String {
    URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
      .appendingPathComponent("1context-permissions-\(UUID().uuidString).plist")
      .path
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

  private func writeProof(
    key: String,
    subject: OneContextPermissionSubject,
    preferencesPath: String,
    method: String,
    details: [String: Any] = [:]
  ) throws {
    try writePreferenceValue(
      proofDictionary(method: method, subject: subject, details: details),
      for: key,
      preferencesPath: preferencesPath
    )
  }

  private func proofDictionary(
    method: String,
    subject: OneContextPermissionSubject,
    details: [String: Any]
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
      "method": method,
      "proved_at": ISO8601DateFormatter().string(from: Date()),
      "details": details
    ]
  }

  private func writePreferenceValue(
    _ value: Any,
    for key: String,
    preferencesPath: String
  ) throws {
    let directory = URL(fileURLWithPath: preferencesPath).deletingLastPathComponent()
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)

    let preferences = NSMutableDictionary(contentsOfFile: preferencesPath) ?? NSMutableDictionary()
    preferences[key] = value
    XCTAssertTrue(preferences.write(toFile: preferencesPath, atomically: true))
  }
}

private final class PermissionProofInputState: @unchecked Sendable {
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
