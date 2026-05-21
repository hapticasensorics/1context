import AppKit
@preconcurrency import ApplicationServices
import AVFoundation
import CoreGraphics
import CoreMedia
import CoreServices
import CryptoKit
import Darwin
import Foundation
import IOKit.hid
import ScreenCaptureKit
import Security
import OneContextPlatform

public enum OneContextRequiredPermissionStatus: String, Codable, Sendable {
  case granted
  case required
  case needsRelaunch
  case unavailable
}

public struct OneContextRequiredPermissionSnapshot: Codable, Equatable, Sendable {
  public let title: String
  public let status: OneContextRequiredPermissionStatus
  public let detail: String

  public var ready: Bool {
    status == .granted
  }

  public var displayStatus: String {
    switch status {
    case .granted:
      return "Granted"
    case .required:
      return "Required"
    case .needsRelaunch:
      return "Needs Relaunch"
    case .unavailable:
      return "Unavailable"
    }
  }

  public init(
    title: String,
    status: OneContextRequiredPermissionStatus,
    detail: String
  ) {
    self.title = title
    self.status = status
    self.detail = detail
  }
}

public struct OneContextRememberingPermissionsSnapshot: Codable, Equatable, Sendable {
  public let screenRecording: OneContextRequiredPermissionSnapshot
  public let accessibility: OneContextRequiredPermissionSnapshot
  public let inputMonitoring: OneContextRequiredPermissionSnapshot
  public let browserExtension: OneContextRequiredPermissionSnapshot
  public let microphone: OneContextRequiredPermissionSnapshot
  public let automation: OneContextRequiredPermissionSnapshot
  public let fullDiskAccess: OneContextRequiredPermissionSnapshot

  public var requiredReady: Bool {
    requiredPermissions.allSatisfy(\.ready)
  }

  public var requiredSummary: String {
    let missing = requiredPermissions.filter { !$0.ready }.map(\.title)
    if missing.isEmpty {
      return "Remembering permissions are granted."
    }
    return "Remembering requires \(missing.joined(separator: ", "))."
  }

  public var requiredPermissions: [OneContextRequiredPermissionSnapshot] {
    [
      screenRecording,
      accessibility,
      inputMonitoring,
      browserExtension,
      microphone,
      automation,
      fullDiskAccess
    ]
  }

  public init(
    screenRecording: OneContextRequiredPermissionSnapshot,
    accessibility: OneContextRequiredPermissionSnapshot,
    inputMonitoring: OneContextRequiredPermissionSnapshot,
    browserExtension: OneContextRequiredPermissionSnapshot,
    microphone: OneContextRequiredPermissionSnapshot,
    automation: OneContextRequiredPermissionSnapshot,
    fullDiskAccess: OneContextRequiredPermissionSnapshot
  ) {
    self.screenRecording = screenRecording
    self.accessibility = accessibility
    self.inputMonitoring = inputMonitoring
    self.browserExtension = browserExtension
    self.microphone = microphone
    self.automation = automation
    self.fullDiskAccess = fullDiskAccess
  }

  public static func snapshot(
    screenRecordingGranted: Bool,
    accessibilityGranted: Bool,
    inputMonitoringGranted: Bool,
    browserExtensionGranted: Bool,
    microphoneGranted: Bool,
    automationGranted: Bool,
    fullDiskAccessGranted: Bool
  ) -> OneContextRememberingPermissionsSnapshot {
    OneContextRememberingPermissionsSnapshot(
      screenRecording: OneContextRequiredPermissionSnapshot(
        title: "Screen & System Audio Recording",
        status: screenRecordingGranted ? .granted : .required,
        detail: "Required so 1Context can see your desktop work and attach pixel and system-audio evidence."
      ),
      accessibility: OneContextRequiredPermissionSnapshot(
        title: "Accessibility",
        status: accessibilityGranted ? .granted : .required,
        detail: "Required so 1Context can understand the active app, window, and focus."
      ),
      inputMonitoring: OneContextRequiredPermissionSnapshot(
        title: "Input Monitoring",
        status: inputMonitoringGranted ? .granted : .required,
        detail: "Required so 1Context can anchor focus, click, scroll, and command timing."
      ),
      browserExtension: OneContextRequiredPermissionSnapshot(
        title: "Browser Extension Permissions",
        status: browserExtensionGranted ? .granted : .required,
        detail: "Required so 1Context can see URLs, tabs, page text, selection, and scroll state."
      ),
      microphone: OneContextRequiredPermissionSnapshot(
        title: "Microphone",
        status: microphoneGranted ? .granted : .required,
        detail: "Required so 1Context can capture spoken context when audio remembering is enabled."
      ),
      automation: OneContextRequiredPermissionSnapshot(
        title: "Automation",
        status: automationGranted ? .granted : .required,
        detail: "Required so 1Context can ask apps for high-signal context when pixels are not enough."
      ),
      fullDiskAccess: OneContextRequiredPermissionSnapshot(
        title: "Full Disk Access",
        status: fullDiskAccessGranted ? .granted : .required,
        detail: "Required so 1Context can read session logs, Messages, browser history, and app data across this Mac."
      )
    )
  }
}

public struct OneContextPermissionSubject: Codable, Equatable, Sendable {
  public let bundleIdentifier: String
  public let designatedRequirementSHA256: String
  public let designatedRequirement: String
  public let executablePath: String
  public let appVersion: String

  public init(
    bundleIdentifier: String,
    designatedRequirementSHA256: String,
    designatedRequirement: String,
    executablePath: String,
    appVersion: String
  ) {
    self.bundleIdentifier = bundleIdentifier
    self.designatedRequirementSHA256 = designatedRequirementSHA256
    self.designatedRequirement = designatedRequirement
    self.executablePath = executablePath
    self.appVersion = appVersion
  }
}

public struct OneContextPermissionPlatform: Sendable {
  public let screenPreflight: @Sendable () -> Bool
  public let screenRequest: @Sendable () -> Bool
  public let accessibilityTrusted: @Sendable () -> Bool
  public let accessibilityRequest: @Sendable () -> Bool
  public let inputPreflight: @Sendable () -> Bool
  public let inputRequest: @Sendable () -> Bool
  public let microphoneStatus: @Sendable () -> AVAuthorizationStatus

  public init(
    screenPreflight: @escaping @Sendable () -> Bool,
    screenRequest: @escaping @Sendable () -> Bool,
    accessibilityTrusted: @escaping @Sendable () -> Bool,
    accessibilityRequest: @escaping @Sendable () -> Bool,
    inputPreflight: @escaping @Sendable () -> Bool,
    inputRequest: @escaping @Sendable () -> Bool,
    microphoneStatus: @escaping @Sendable () -> AVAuthorizationStatus
  ) {
    self.screenPreflight = screenPreflight
    self.screenRequest = screenRequest
    self.accessibilityTrusted = accessibilityTrusted
    self.accessibilityRequest = accessibilityRequest
    self.inputPreflight = inputPreflight
    self.inputRequest = inputRequest
    self.microphoneStatus = microphoneStatus
  }

  public static let live = OneContextPermissionPlatform(
    screenPreflight: { CGPreflightScreenCaptureAccess() },
    screenRequest: { CGRequestScreenCaptureAccess() },
    accessibilityTrusted: { AXIsProcessTrusted() },
    accessibilityRequest: {
      let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue() as String: true] as CFDictionary
      return AXIsProcessTrustedWithOptions(options)
    },
    inputPreflight: { IOHIDCheckAccess(kIOHIDRequestTypeListenEvent) == kIOHIDAccessTypeGranted },
    inputRequest: { IOHIDRequestAccess(kIOHIDRequestTypeListenEvent) },
    microphoneStatus: { AVCaptureDevice.authorizationStatus(for: .audio) }
  )
}

public struct OneContextBrowserExtensionPermissionProof: Codable, Equatable, Sendable {
  public let extensionIdentifier: String
  public let extensionVersion: String
  public let grantedPermissions: [String]
  public let grantedHostPermissions: [String]

  public init(
    extensionIdentifier: String,
    extensionVersion: String,
    grantedPermissions: [String],
    grantedHostPermissions: [String]
  ) {
    self.extensionIdentifier = extensionIdentifier
    self.extensionVersion = extensionVersion
    self.grantedPermissions = grantedPermissions
    self.grantedHostPermissions = grantedHostPermissions
  }
}

public struct OneContextAutomationTarget: Codable, Equatable, Sendable {
  public let name: String
  public let bundleIdentifier: String

  public init(name: String, bundleIdentifier: String) {
    self.name = name
    self.bundleIdentifier = bundleIdentifier
  }
}

public struct OneContextFullDiskAccessProbe: Codable, Equatable, Sendable {
  public let label: String
  public let url: URL

  public init(label: String, url: URL) {
    self.label = label
    self.url = url
  }
}

public struct OneContextFullDiskAccessProbeReport: Codable, Equatable, Sendable {
  public let existingProbeCount: Int
  public let failedProbeLabels: [String]

  public var granted: Bool {
    existingProbeCount > 0 && failedProbeLabels.isEmpty
  }
}

public enum OneContextPermissionGrantOutcome: Equatable, Sendable {
  case granted
  case required
  case needsRelaunch
  case unavailable(String)
}

public enum OneContextSystemPermissions {
  public static let browserExtensionPermissionsGrantedKey = "RememberingBrowserExtensionPermissionsGranted"
  public static let automationPermissionsGrantedKey = "RememberingAutomationPermissionsGranted"
  public static let inputMonitoringProofKey = "RememberingInputMonitoringProof"
  public static let browserExtensionProofKey = "RememberingBrowserExtensionProof"
  public static let automationProofKey = "RememberingAutomationProof"
  public static let automationProofsKey = "RememberingAutomationProofs"
  public static let automationTargetBundleIdentifiersKey = "RememberingAutomationTargetBundleIdentifiers"
  public static let screenCaptureProofKey = "RememberingScreenCaptureProof"
  public static let systemAudioProofKey = "RememberingSystemAudioProof"
  public static let screenCaptureRelaunchProofKey = "RememberingScreenCaptureRelaunchProof"
  public static let microphoneProofKey = "RememberingMicrophoneProof"

  public static let requiredBrowserExtensionPermissions = [
    "nativeMessaging",
    "scripting",
    "storage",
    "tabs"
  ]
  public static let requiredBrowserExtensionHostPermissions = ["<all_urls>"]

  public static let fullDiskAccessSettingsURL = "x-apple.systempreferences:com.apple.settings.PrivacySecurity.extension?Privacy_AllFiles"
  public static let inputMonitoringSettingsURL = "x-apple.systempreferences:com.apple.preference.security?Privacy_ListenEvent"
  public static let screenRecordingSettingsURL = "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
  public static let accessibilitySettingsURL = "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
  public static let microphoneSettingsURL = "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
  public static let automationSettingsURL = "x-apple.systempreferences:com.apple.preference.security?Privacy_Automation"

  private static let proofSchemaVersion = "1context.permission-proof.v2"
  private static let processStartIdentifier = "\(getpid())-\(Date().timeIntervalSince1970)"

  public static let defaultAutomationTargets: [OneContextAutomationTarget] = [
    OneContextAutomationTarget(name: "Finder", bundleIdentifier: "com.apple.finder"),
    OneContextAutomationTarget(name: "Safari", bundleIdentifier: "com.apple.Safari"),
    OneContextAutomationTarget(name: "Google Chrome", bundleIdentifier: "com.google.Chrome"),
    OneContextAutomationTarget(name: "Visual Studio Code", bundleIdentifier: "com.microsoft.VSCode"),
    OneContextAutomationTarget(name: "Slack", bundleIdentifier: "com.tinyspeck.slackmacgap"),
    OneContextAutomationTarget(name: "Mail", bundleIdentifier: "com.apple.mail"),
    OneContextAutomationTarget(name: "Messages", bundleIdentifier: "com.apple.MobileSMS")
  ]
  public static let defaultAutomationTargetBundleIdentifiers = [
    "com.apple.finder",
    "com.google.Chrome",
    "com.microsoft.VSCode"
  ]

  public static func current(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    platform: OneContextPermissionPlatform = .live
  ) -> OneContextRememberingPermissionsSnapshot {
    clearMismatchedProofs(preferencesPath: preferencesPath, subject: subject)
    let screenStatus = screenAndSystemAudioStatus(
      preferencesPath: preferencesPath,
      subject: subject,
      platform: platform
    )
    let microphoneStatus = microphonePermissionStatus(
      preferencesPath: preferencesPath,
      subject: subject,
      platform: platform
    )
    let fullDiskReport = fullDiskAccessProbeReport()

    return OneContextRememberingPermissionsSnapshot(
      screenRecording: OneContextRequiredPermissionSnapshot(
        title: "Screen & System Audio Recording",
        status: screenStatus,
        detail: "Required so 1Context can prove pixels and system audio from the signed capture process."
      ),
      accessibility: OneContextRequiredPermissionSnapshot(
        title: "Accessibility",
        status: platform.accessibilityTrusted() ? .granted : .required,
        detail: "Required so 1Context can understand the active app, window, and focus."
      ),
      inputMonitoring: OneContextRequiredPermissionSnapshot(
        title: "Input Monitoring",
        status: inputMonitoringGranted(preferencesPath: preferencesPath, subject: subject, platform: platform) ? .granted : .required,
        detail: "Required so 1Context can anchor focus, click, scroll, and command timing through macOS Input Monitoring."
      ),
      browserExtension: OneContextRequiredPermissionSnapshot(
        title: "Browser Extension Permissions",
        status: browserExtensionGranted(preferencesPath: preferencesPath, subject: subject) ? .granted : .required,
        detail: browserExtensionRequiredDetail(preferencesPath: preferencesPath, subject: subject)
      ),
      microphone: OneContextRequiredPermissionSnapshot(
        title: "Microphone",
        status: microphoneStatus,
        detail: "Required so 1Context can prove the signed app can open the audio capture path."
      ),
      automation: OneContextRequiredPermissionSnapshot(
        title: "Automation",
        status: automationGranted(preferencesPath: preferencesPath, subject: subject) ? .granted : .required,
        detail: automationRequiredDetail(preferencesPath: preferencesPath, subject: subject)
      ),
      fullDiskAccess: OneContextRequiredPermissionSnapshot(
        title: "Full Disk Access",
        status: fullDiskReport.granted ? .granted : .required,
        detail: fullDiskAccessDetail(report: fullDiskReport)
      )
    )
  }

  public static func currentPermissionSubject(
    bundleIdentifier: String = permissionSubjectBundleIdentifier()
  ) -> OneContextPermissionSubject {
    let appBundle = OneContextAppIdentity.containingAppBundle()
    let appInfo = appBundle.flatMap(infoDictionary)
    let executablePath = appBundle?
      .appendingPathComponent("Contents/MacOS/1Context")
      .path
      ?? Bundle.main.executableURL?.path
      ?? currentExecutablePath()
      ?? "unknown"
    let appVersion = appInfo?["CFBundleVersion"] as? String
      ?? appInfo?["CFBundleShortVersionString"] as? String
      ?? Bundle.main.object(forInfoDictionaryKey: "CFBundleVersion") as? String
      ?? Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String
      ?? "unknown"
    let requirement = appBundle.flatMap(designatedRequirementString)
      ?? currentDesignatedRequirementString()
      ?? "unsigned bundle_identifier=\(bundleIdentifier) executable=\(executablePath)"
    return OneContextPermissionSubject(
      bundleIdentifier: bundleIdentifier,
      designatedRequirementSHA256: sha256(requirement),
      designatedRequirement: requirement,
      executablePath: executablePath,
      appVersion: appVersion
    )
  }

  public static func screenRecordingGranted(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    platform: OneContextPermissionPlatform = .live
  ) -> Bool {
    screenAndSystemAudioStatus(
      preferencesPath: preferencesPath,
      subject: subject,
      platform: platform
    ) == .granted
  }

  public static func requestScreenRecording() -> Bool {
    OneContextPermissionPlatform.live.screenRequest()
  }

  public static func requestScreenAndSystemAudio(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    platform: OneContextPermissionPlatform = .live
  ) async -> OneContextPermissionGrantOutcome {
    if !platform.screenPreflight() {
      let granted = platform.screenRequest()
      if granted {
        try? recordProof(
          screenCaptureRelaunchProofKey,
          method: "coregraphics-screen-requested-relaunch-required",
          subject: subject,
          preferencesPath: preferencesPath,
          details: ["process_start": processStartIdentifier]
        )
        return .needsRelaunch
      }
      clearScreenAndSystemAudioProofs(preferencesPath: preferencesPath)
      return .required
    }

    do {
      try await proveScreenAndSystemAudio(preferencesPath: preferencesPath, subject: subject)
      return .granted
    } catch {
      clearScreenAndSystemAudioProofs(preferencesPath: preferencesPath)
      return .unavailable(error.localizedDescription)
    }
  }

  @discardableResult
  public static func refreshRuntimeProofsIfAuthorized(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    platform: OneContextPermissionPlatform = .live
  ) async -> Bool {
    var changed = false
    clearMismatchedProofs(preferencesPath: preferencesPath, subject: subject)

    if platform.inputPreflight(),
      !proofMatches(inputMonitoringProofKey, subject: subject, preferencesPath: preferencesPath)
    {
      try? recordProof(
        inputMonitoringProofKey,
        method: "iohid-listen-event-preflight",
        subject: subject,
        preferencesPath: preferencesPath
      )
      changed = true
    }

    if platform.microphoneStatus() == .authorized,
      !proofMatches(microphoneProofKey, subject: subject, preferencesPath: preferencesPath)
    {
      changed = proveMicrophoneCapture(preferencesPath: preferencesPath, subject: subject) || changed
    }

    let screenProofMissing =
      !proofMatches(screenCaptureProofKey, subject: subject, preferencesPath: preferencesPath)
      || !proofMatches(systemAudioProofKey, subject: subject, preferencesPath: preferencesPath)
    if platform.screenPreflight(),
      !relaunchRequiredInCurrentProcess(preferencesPath: preferencesPath, subject: subject),
      screenProofMissing
    {
      do {
        try await proveScreenAndSystemAudio(preferencesPath: preferencesPath, subject: subject)
        changed = true
      } catch {
        clearScreenAndSystemAudioProofs(preferencesPath: preferencesPath)
      }
    }

    changed = refreshAutomationProofsIfAuthorized(
      preferencesPath: preferencesPath,
      subject: subject
    ) || changed

    return changed
  }

  public static func accessibilityGranted(platform: OneContextPermissionPlatform = .live) -> Bool {
    platform.accessibilityTrusted()
  }

  @discardableResult
  public static func requestAccessibility(platform: OneContextPermissionPlatform = .live) -> Bool {
    platform.accessibilityRequest()
  }

  public static func inputMonitoringGranted(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    platform: OneContextPermissionPlatform = .live
  ) -> Bool {
    platform.inputPreflight()
      && proofMatches(inputMonitoringProofKey, subject: subject, preferencesPath: preferencesPath)
  }

  public static func inputMonitoringRuntimeGranted(platform: OneContextPermissionPlatform = .live) -> Bool {
    platform.inputPreflight()
  }

  @discardableResult
  public static func requestInputMonitoring(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    platform: OneContextPermissionPlatform = .live
  ) -> Bool {
    _ = platform.inputRequest()
    let granted = platform.inputPreflight()
    if granted {
      try? recordProof(
        inputMonitoringProofKey,
        method: "iohid-listen-event",
        subject: subject,
        preferencesPath: preferencesPath
      )
    } else {
      try? removePreferenceValue(inputMonitoringProofKey, preferencesPath: preferencesPath)
    }
    return granted
  }

  public static func microphoneGranted(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    platform: OneContextPermissionPlatform = .live
  ) -> Bool {
    microphonePermissionStatus(
      preferencesPath: preferencesPath,
      subject: subject,
      platform: platform
    ) == .granted
  }

  public static func requestMicrophone(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    _ completion: @escaping @Sendable (Bool) -> Void
  ) {
    AVCaptureDevice.requestAccess(for: .audio) { granted in
      guard granted else {
        try? removePreferenceValue(microphoneProofKey, preferencesPath: preferencesPath)
        completion(false)
        return
      }
      let proved = proveMicrophoneCapture(preferencesPath: preferencesPath, subject: subject)
      completion(proved)
    }
  }

  public static func browserExtensionGranted(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    expectedExtensionIdentifier: String? = nil
  ) -> Bool {
    guard let proof = preferenceDictionary(browserExtensionProofKey, preferencesPath: preferencesPath),
      proofMatches(proof, subject: subject),
      let details = proof["details"] as? [String: Any],
      let extensionIdentifier = details["extension_identifier"] as? String,
      let extensionVersion = details["extension_version"] as? String,
      !extensionIdentifier.isEmpty,
      !extensionVersion.isEmpty
    else {
      return false
    }

    let expectedExtensionIdentifier = expectedExtensionIdentifier ?? self.expectedBrowserExtensionIdentifier()
    if let expectedExtensionIdentifier, !expectedExtensionIdentifier.isEmpty,
      extensionIdentifier != expectedExtensionIdentifier
    {
      return false
    }

    let permissions = Set(details["granted_permissions"] as? [String] ?? [])
    let hostPermissions = Set(details["granted_host_permissions"] as? [String] ?? [])
    return Set(requiredBrowserExtensionPermissions).isSubset(of: permissions)
      && Set(requiredBrowserExtensionHostPermissions).isSubset(of: hostPermissions)
  }

  public static func recordBrowserExtensionProof(
    _ proof: OneContextBrowserExtensionPermissionProof,
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject()
  ) throws {
    guard !proof.extensionIdentifier.isEmpty else {
      throw permissionError("Browser extension proof is missing an extension identifier.")
    }
    guard !proof.extensionVersion.isEmpty else {
      throw permissionError("Browser extension proof is missing an extension version.")
    }
    try recordProof(
      browserExtensionProofKey,
      method: "browser-extension-native-handshake",
      subject: subject,
      preferencesPath: preferencesPath,
      details: [
        "extension_identifier": proof.extensionIdentifier,
        "extension_version": proof.extensionVersion,
        "granted_permissions": proof.grantedPermissions.sorted(),
        "granted_host_permissions": proof.grantedHostPermissions.sorted()
      ]
    )
  }

  public static func recordBrowserExtensionGranted(
    _ granted: Bool = true,
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    bundleIdentifier: String = permissionSubjectBundleIdentifier()
  ) throws {
    if !granted {
      try removePreferenceValue(browserExtensionProofKey, preferencesPath: preferencesPath)
      return
    }
    throw permissionError("Browser Extension Permissions require an extension native-messaging proof; self-recorded grants are not accepted.")
  }

  public static func automationGranted(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    targets: [OneContextAutomationTarget]? = nil
  ) -> Bool {
    let installedTargets = installedAutomationTargets(targets ?? selectedAutomationTargets(preferencesPath: preferencesPath))
    guard !installedTargets.isEmpty else { return false }
    return installedTargets.allSatisfy { target in
      automationTargetUsable(target, askUserIfNeeded: false)
        && automationProofMatches(target: target, subject: subject, preferencesPath: preferencesPath)
    }
  }

  @discardableResult
  public static func requestAutomation(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    targets: [OneContextAutomationTarget]? = nil
  ) -> Bool {
    let installedTargets = installedAutomationTargets(targets ?? selectedAutomationTargets(preferencesPath: preferencesPath))
    guard !installedTargets.isEmpty else { return false }

    var allGranted = true
    for target in installedTargets {
      if automationTargetUsable(target, askUserIfNeeded: true) {
        try? recordAutomationProof(target: target, subject: subject, preferencesPath: preferencesPath)
      } else {
        try? removeAutomationProof(target: target, preferencesPath: preferencesPath)
        allGranted = false
      }
    }
    return allGranted
  }

  @discardableResult
  public static func refreshAutomationProofsIfAuthorized(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    subject: OneContextPermissionSubject = currentPermissionSubject(),
    targets: [OneContextAutomationTarget]? = nil
  ) -> Bool {
    let installedTargets = installedAutomationTargets(targets ?? selectedAutomationTargets(preferencesPath: preferencesPath))
    var changed = false
    for target in installedTargets {
      let authorized = automationTargetUsable(target, askUserIfNeeded: false)
      if authorized {
        if !automationProofMatches(target: target, subject: subject, preferencesPath: preferencesPath) {
          try? recordAutomationProof(target: target, subject: subject, preferencesPath: preferencesPath)
          changed = true
        }
      } else if automationProofMatches(target: target, subject: subject, preferencesPath: preferencesPath) {
        try? removeAutomationProof(target: target, preferencesPath: preferencesPath)
        changed = true
      }
    }
    return changed
  }

  public static func proveAutomationTarget(_ target: OneContextAutomationTarget) throws -> String {
    let source = """
    tell application id "\(target.bundleIdentifier)"
      get name
    end tell
    """
    guard let script = NSAppleScript(source: source) else {
      throw permissionError("Could not compile AppleScript probe for \(target.name).")
    }

    var errorInfo: NSDictionary?
    let result = script.executeAndReturnError(&errorInfo)
    guard errorInfo == nil else {
      throw permissionError(errorInfo?.description ?? "Unknown AppleScript error for \(target.name).")
    }

    return result.stringValue ?? ""
  }

  public static func recordAutomationGranted(
    _ granted: Bool = true,
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    bundleIdentifier: String = permissionSubjectBundleIdentifier()
  ) throws {
    if !granted {
      try removePreferenceValue(automationProofsKey, preferencesPath: preferencesPath)
      try removePreferenceValue(automationProofKey, preferencesPath: preferencesPath)
      return
    }
    throw permissionError("Automation requires per-target Apple Events proof; Finder-only or self-recorded grants are not accepted.")
  }

  public static func fullDiskAccessGranted() -> Bool {
    fullDiskAccessProbeReport().granted
  }

  public static func fullDiskAccessGranted(probeURLs: [URL]) -> Bool {
    fullDiskAccessProbeReport(
      probes: probeURLs.map { OneContextFullDiskAccessProbe(label: $0.lastPathComponent, url: $0) }
    ).granted
  }

  public static func fullDiskAccessProbeReport() -> OneContextFullDiskAccessProbeReport {
    fullDiskAccessProbeReport(probes: protectedFullDiskAccessProbes())
  }

  public static func fullDiskAccessProbeReport(probes: [OneContextFullDiskAccessProbe]) -> OneContextFullDiskAccessProbeReport {
    let existingProbes = probes.filter { FileManager.default.fileExists(atPath: $0.url.path) }
    let failed = existingProbes.filter { !canReadProtectedPath($0.url) }.map(\.label)
    return OneContextFullDiskAccessProbeReport(
      existingProbeCount: existingProbes.count,
      failedProbeLabels: failed.sorted()
    )
  }

  public static func permissionSubjectBundleIdentifier() -> String {
    if let bundleIdentifier = Bundle.main.bundleIdentifier, !bundleIdentifier.isEmpty {
      return bundleIdentifier
    }
    if let appBundle = OneContextAppIdentity.containingAppBundle(),
      let bundleIdentifier = bundleIdentifier(at: appBundle)
    {
      return bundleIdentifier
    }
    return RuntimePaths.current().identity.bundleIdentifier
  }

  public static func installedAutomationTargets(
    _ targets: [OneContextAutomationTarget] = defaultAutomationTargets
  ) -> [OneContextAutomationTarget] {
    targets.filter { target in
      NSWorkspace.shared.urlForApplication(withBundleIdentifier: target.bundleIdentifier) != nil
    }
  }

  public static func selectedAutomationTargets(
    preferencesPath: String = RuntimePaths.current().preferencesPath,
    targets: [OneContextAutomationTarget] = defaultAutomationTargets
  ) -> [OneContextAutomationTarget] {
    let selected = selectedAutomationTargetBundleIdentifiers(preferencesPath: preferencesPath)
    var byIdentifier: [String: OneContextAutomationTarget] = [:]
    for target in targets {
      byIdentifier[target.bundleIdentifier] = target
    }
    return selected.compactMap { byIdentifier[$0] }
  }

  public static func selectedAutomationTargetBundleIdentifiers(
    preferencesPath: String = RuntimePaths.current().preferencesPath
  ) -> [String] {
    let known = Set(defaultAutomationTargets.map(\.bundleIdentifier))
    let stored = preferenceStringArray(automationTargetBundleIdentifiersKey, preferencesPath: preferencesPath)
    let raw = stored?.isEmpty == false ? stored! : defaultAutomationTargetBundleIdentifiers
    let selected = deduplicated(raw.filter { known.contains($0) })
    return selected.isEmpty ? defaultAutomationTargetBundleIdentifiers : selected
  }

  public static func setAutomationTargetBundleIdentifiers(
    _ bundleIdentifiers: [String],
    preferencesPath: String = RuntimePaths.current().preferencesPath
  ) throws {
    let known = Set(defaultAutomationTargets.map(\.bundleIdentifier))
    let selected = deduplicated(bundleIdentifiers.filter { known.contains($0) })
    guard !selected.isEmpty else {
      throw permissionError("Choose at least one app for Automation.")
    }

    try setPreferenceValue(selected, for: automationTargetBundleIdentifiersKey, preferencesPath: preferencesPath)
    try removeAutomationProofsNotIn(Set(selected), preferencesPath: preferencesPath)
  }

  public static func fullDiskAccessDiagnostics(redact: (String) -> String = { $0 }) -> [String] {
    let report = fullDiskAccessProbeReport()
    guard !report.granted else { return [] }
    if report.existingProbeCount == 0 {
      return ["Full Disk Access Probes: no protected probe paths exist on this Mac"]
    }
    return [
      "Full Disk Access Failed Probes: \(report.failedProbeLabels.map(redact).joined(separator: ", "))"
    ]
  }

  private static func screenAndSystemAudioStatus(
    preferencesPath: String,
    subject: OneContextPermissionSubject,
    platform: OneContextPermissionPlatform
  ) -> OneContextRequiredPermissionStatus {
    guard platform.screenPreflight() else { return .required }
    if relaunchRequiredInCurrentProcess(preferencesPath: preferencesPath, subject: subject) {
      return .needsRelaunch
    }
    return proofMatches(screenCaptureProofKey, subject: subject, preferencesPath: preferencesPath)
      && proofMatches(systemAudioProofKey, subject: subject, preferencesPath: preferencesPath)
      ? .granted
      : .required
  }

  private static func microphonePermissionStatus(
    preferencesPath: String,
    subject: OneContextPermissionSubject,
    platform: OneContextPermissionPlatform
  ) -> OneContextRequiredPermissionStatus {
    switch platform.microphoneStatus() {
    case .authorized:
      return proofMatches(microphoneProofKey, subject: subject, preferencesPath: preferencesPath)
        ? .granted
        : .required
    case .denied, .notDetermined:
      return .required
    case .restricted:
      return .unavailable
    @unknown default:
      return .required
    }
  }

  @available(macOS 13.0, *)
  private static func proveScreenAndSystemAudio(
    preferencesPath: String,
    subject: OneContextPermissionSubject
  ) async throws {
    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    guard let display = content.displays.first else {
      throw permissionError("No captureable display was available.")
    }

    let filter = SCContentFilter(display: display, excludingWindows: [])
    let configuration = SCStreamConfiguration()
    configuration.width = 64
    configuration.height = 64
    configuration.minimumFrameInterval = CMTime(value: 1, timescale: 5)
    configuration.queueDepth = 3
    configuration.showsCursor = false
    configuration.capturesAudio = true
    configuration.excludesCurrentProcessAudio = false

    let output = ScreenAndSystemAudioProofOutput()
    let queue = DispatchQueue(label: "com.haptica.1context.permissions.screencapture-proof")
    let stream = SCStream(filter: filter, configuration: configuration, delegate: nil)
    try stream.addStreamOutput(output, type: .screen, sampleHandlerQueue: queue)
    try stream.addStreamOutput(output, type: .audio, sampleHandlerQueue: queue)

    try await startCapture(stream)
    let screenFrameArrived = output.waitForScreenFrame(timeout: 3)
    try await stopCapture(stream)

    guard screenFrameArrived else {
      throw permissionError("ScreenCaptureKit started but did not produce a screen frame.")
    }

    try recordProof(
      screenCaptureProofKey,
      method: "screencapturekit-screen-frame",
      subject: subject,
      preferencesPath: preferencesPath,
      details: ["display_count": "\(content.displays.count)"]
    )
    try recordProof(
      systemAudioProofKey,
      method: "screencapturekit-system-audio-stream",
      subject: subject,
      preferencesPath: preferencesPath,
      details: ["captures_audio": "true"]
    )
    try removePreferenceValue(screenCaptureRelaunchProofKey, preferencesPath: preferencesPath)
  }

  @available(macOS 13.0, *)
  private static func startCapture(_ stream: SCStream) async throws {
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      stream.startCapture { error in
        if let error {
          continuation.resume(throwing: error)
        } else {
          continuation.resume()
        }
      }
    }
  }

  @available(macOS 13.0, *)
  private static func stopCapture(_ stream: SCStream) async throws {
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      stream.stopCapture { error in
        if let error {
          continuation.resume(throwing: error)
        } else {
          continuation.resume()
        }
      }
    }
  }

  private static func proveMicrophoneCapture(
    preferencesPath: String,
    subject: OneContextPermissionSubject
  ) -> Bool {
    guard let device = AVCaptureDevice.default(for: .audio) else {
      try? removePreferenceValue(microphoneProofKey, preferencesPath: preferencesPath)
      return false
    }

    let session = AVCaptureSession()
    do {
      let input = try AVCaptureDeviceInput(device: device)
      guard session.canAddInput(input) else {
        try? removePreferenceValue(microphoneProofKey, preferencesPath: preferencesPath)
        return false
      }
      session.addInput(input)
      session.startRunning()
      let running = session.isRunning
      session.stopRunning()
      if running {
        try recordProof(
          microphoneProofKey,
          method: "avcapture-audio-session",
          subject: subject,
          preferencesPath: preferencesPath,
          details: ["device_unique_id": device.uniqueID]
        )
      } else {
        try? removePreferenceValue(microphoneProofKey, preferencesPath: preferencesPath)
      }
      return running
    } catch {
      try? removePreferenceValue(microphoneProofKey, preferencesPath: preferencesPath)
      return false
    }
  }

  private static func automationRequiredDetail(
    preferencesPath: String,
    subject: OneContextPermissionSubject
  ) -> String {
    let installedConfiguredTargets = installedAutomationTargets(
      selectedAutomationTargets(preferencesPath: preferencesPath)
    )
    if installedConfiguredTargets.isEmpty {
      return "Choose at least one installed app for 1Context to automate."
    }
    let missing = installedConfiguredTargets.filter {
      !automationProofMatches(target: $0, subject: subject, preferencesPath: preferencesPath)
        || !automationTargetUsable($0, askUserIfNeeded: false)
    }
    if missing.isEmpty {
      return "Configured apps: \(installedConfiguredTargets.map(\.name).joined(separator: ", "))."
    }
    return "Required for app-specific Apple Events proof: \(missing.map(\.name).joined(separator: ", "))."
  }

  private static func browserExtensionRequiredDetail(
    preferencesPath: String,
    subject: OneContextPermissionSubject
  ) -> String {
    if browserExtensionGranted(preferencesPath: preferencesPath, subject: subject) {
      return "Required so 1Context can prove the installed browser extension id, version, and granted URL/tab/DOM permissions."
    }
    return "Required; no browser extension native-message proof has been received from this signed app yet."
  }

  private static func fullDiskAccessDetail(report: OneContextFullDiskAccessProbeReport) -> String {
    if report.granted {
      return "Required so 1Context can read session logs, Messages, browser history, and app data across this Mac."
    }
    if report.existingProbeCount == 0 {
      return "No protected probe paths exist on this Mac yet, so Full Disk Access cannot be proved."
    }
    return "Full Disk Access still cannot read: \(report.failedProbeLabels.joined(separator: ", "))."
  }

  private static func automationPermissionStatus(
    targetBundleIdentifier: String,
    askUserIfNeeded: Bool
  ) -> OSStatus {
    var descriptor = AEAddressDesc()
    let createStatus = targetBundleIdentifier.withCString { pointer in
      AECreateDesc(
        DescType(typeApplicationBundleID),
        pointer,
        targetBundleIdentifier.utf8.count,
        &descriptor
      )
    }
    guard createStatus == noErr else { return OSStatus(createStatus) }
    defer { AEDisposeDesc(&descriptor) }

    return AEDeterminePermissionToAutomateTarget(
      &descriptor,
      AEEventClass(typeWildCard),
      AEEventID(typeWildCard),
      askUserIfNeeded
    )
  }

  private static func automationTargetUsable(
    _ target: OneContextAutomationTarget,
    askUserIfNeeded: Bool
  ) -> Bool {
    if automationPermissionStatus(
      targetBundleIdentifier: target.bundleIdentifier,
      askUserIfNeeded: askUserIfNeeded
    ) == noErr {
      return true
    }

    guard NSWorkspace.shared.runningApplications.contains(where: { $0.bundleIdentifier == target.bundleIdentifier }) else {
      return false
    }
    return (try? proveAutomationTarget(target)) != nil
  }

  private static func automationProofMatches(
    target: OneContextAutomationTarget,
    subject: OneContextPermissionSubject,
    preferencesPath: String
  ) -> Bool {
    guard let proofs = preferenceDictionary(automationProofsKey, preferencesPath: preferencesPath),
      let proof = proofs[target.bundleIdentifier] as? [String: Any]
    else {
      return false
    }
    return proofMatches(proof, subject: subject)
      && (proof["details"] as? [String: Any])?["target_bundle_identifier"] as? String == target.bundleIdentifier
  }

  private static func recordAutomationProof(
    target: OneContextAutomationTarget,
    subject: OneContextPermissionSubject,
    preferencesPath: String
  ) throws {
    var proofs = preferenceDictionary(automationProofsKey, preferencesPath: preferencesPath) ?? [:]
    proofs[target.bundleIdentifier] = proofDictionary(
      method: "apple-events-target",
      subject: subject,
      details: [
        "target_name": target.name,
        "target_bundle_identifier": target.bundleIdentifier,
        "event_class": "wildcard",
        "event_id": "wildcard"
      ]
    )
    try setPreferenceValue(proofs, for: automationProofsKey, preferencesPath: preferencesPath)
    try removePreferenceValue(automationProofKey, preferencesPath: preferencesPath)
  }

  private static func removeAutomationProof(
    target: OneContextAutomationTarget,
    preferencesPath: String
  ) throws {
    var proofs = preferenceDictionary(automationProofsKey, preferencesPath: preferencesPath) ?? [:]
    proofs.removeValue(forKey: target.bundleIdentifier)
    try setPreferenceValue(proofs, for: automationProofsKey, preferencesPath: preferencesPath)
  }

  private static func removeAutomationProofsNotIn(
    _ selectedBundleIdentifiers: Set<String>,
    preferencesPath: String
  ) throws {
    var proofs = preferenceDictionary(automationProofsKey, preferencesPath: preferencesPath) ?? [:]
    for key in proofs.keys where !selectedBundleIdentifiers.contains(key) {
      proofs.removeValue(forKey: key)
    }
    try setPreferenceValue(proofs, for: automationProofsKey, preferencesPath: preferencesPath)
  }

  private static func relaunchRequiredInCurrentProcess(
    preferencesPath: String,
    subject: OneContextPermissionSubject
  ) -> Bool {
    guard let proof = preferenceDictionary(screenCaptureRelaunchProofKey, preferencesPath: preferencesPath),
      proofMatches(proof, subject: subject),
      let details = proof["details"] as? [String: Any]
    else {
      return false
    }
    return details["process_start"] as? String == processStartIdentifier
  }

  private static func clearScreenAndSystemAudioProofs(preferencesPath: String) {
    try? removePreferenceValue(screenCaptureProofKey, preferencesPath: preferencesPath)
    try? removePreferenceValue(systemAudioProofKey, preferencesPath: preferencesPath)
  }

  private static func clearMismatchedProofs(
    preferencesPath: String,
    subject: OneContextPermissionSubject
  ) {
    for key in [
      inputMonitoringProofKey,
      browserExtensionProofKey,
      screenCaptureProofKey,
      systemAudioProofKey,
      screenCaptureRelaunchProofKey,
      microphoneProofKey
    ] {
      if let proof = preferenceDictionary(key, preferencesPath: preferencesPath),
        !proofMatches(proof, subject: subject)
      {
        try? removePreferenceValue(key, preferencesPath: preferencesPath)
      }
    }

    guard var automationProofs = preferenceDictionary(automationProofsKey, preferencesPath: preferencesPath) else {
      return
    }
    for (target, value) in automationProofs {
      guard let proof = value as? [String: Any], proofMatches(proof, subject: subject) else {
        automationProofs.removeValue(forKey: target)
        continue
      }
    }
    try? setPreferenceValue(automationProofs, for: automationProofsKey, preferencesPath: preferencesPath)
  }

  private static func proofMatches(
    _ key: String,
    subject: OneContextPermissionSubject,
    preferencesPath: String
  ) -> Bool {
    guard let proof = preferenceDictionary(key, preferencesPath: preferencesPath) else {
      return false
    }
    return proofMatches(proof, subject: subject)
  }

  private static func proofMatches(_ proof: [String: Any], subject: OneContextPermissionSubject) -> Bool {
    guard proof["schema_version"] as? String == proofSchemaVersion,
      let proofSubject = proof["subject"] as? [String: Any]
    else {
      return false
    }
    return proofSubject["bundle_identifier"] as? String == subject.bundleIdentifier
      && proofSubject["designated_requirement_sha256"] as? String == subject.designatedRequirementSHA256
      && proofSubject["app_version"] as? String == subject.appVersion
  }

  private static func recordProof(
    _ key: String,
    method: String,
    subject: OneContextPermissionSubject,
    preferencesPath: String,
    details: [String: Any] = [:]
  ) throws {
    try setPreferenceValue(
      proofDictionary(method: method, subject: subject, details: details),
      for: key,
      preferencesPath: preferencesPath
    )
  }

  private static func proofDictionary(
    method: String,
    subject: OneContextPermissionSubject,
    details: [String: Any]
  ) -> [String: Any] {
    [
      "schema_version": proofSchemaVersion,
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

  private static func preferenceDictionary(_ key: String, preferencesPath: String) -> [String: Any]? {
    guard let preferences = NSDictionary(contentsOfFile: preferencesPath) as? [String: Any] else {
      return nil
    }
    return preferences[key] as? [String: Any]
  }

  private static func preferenceStringArray(_ key: String, preferencesPath: String) -> [String]? {
    guard let preferences = NSDictionary(contentsOfFile: preferencesPath) as? [String: Any] else {
      return nil
    }
    return preferences[key] as? [String]
  }

  private static func deduplicated(_ values: [String]) -> [String] {
    var seen = Set<String>()
    var result: [String] = []
    for value in values where !seen.contains(value) {
      seen.insert(value)
      result.append(value)
    }
    return result
  }

  private static func expectedBrowserExtensionIdentifier() -> String? {
    if let appBundle = OneContextAppIdentity.containingAppBundle(),
      let extensionIdentifier = infoDictionary(at: appBundle)?["OneContextBrowserExtensionID"] as? String
    {
      return extensionIdentifier
    }
    return Bundle.main.object(forInfoDictionaryKey: "OneContextBrowserExtensionID") as? String
  }

  private static func infoDictionary(at appBundle: URL) -> [String: Any]? {
    let plist = appBundle.appendingPathComponent("Contents/Info.plist")
    guard let data = try? Data(contentsOf: plist),
      let info = try? PropertyListSerialization.propertyList(from: data, options: [], format: nil) as? [String: Any]
    else {
      return nil
    }
    return info
  }

  private static func bundleIdentifier(at appBundle: URL) -> String? {
    infoDictionary(at: appBundle)?["CFBundleIdentifier"] as? String
  }

  private static func setPreferenceValue(_ value: Any, for key: String, preferencesPath: String) throws {
    try setPreferenceValues([key: value], preferencesPath: preferencesPath)
  }

  private static func removePreferenceValue(_ key: String, preferencesPath: String) throws {
    let preferences = NSMutableDictionary(contentsOfFile: preferencesPath) ?? NSMutableDictionary()
    preferences.removeObject(forKey: key)
    try writePreferences(preferences, preferencesPath: preferencesPath)
  }

  private static func setPreferenceValues(_ values: [String: Any], preferencesPath: String) throws {
    let preferences = NSMutableDictionary(contentsOfFile: preferencesPath) ?? NSMutableDictionary()
    for (key, value) in values {
      preferences[key] = value
    }
    try writePreferences(preferences, preferencesPath: preferencesPath)
  }

  private static func writePreferences(_ preferences: NSMutableDictionary, preferencesPath: String) throws {
    let directory = URL(fileURLWithPath: preferencesPath).deletingLastPathComponent()
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    guard preferences.write(toFile: preferencesPath, atomically: true) else {
      throw permissionError("Failed to write permissions setup state at \(preferencesPath)")
    }
    chmod(preferencesPath, RuntimePermissions.privateFileMode)
  }

  private static func protectedFullDiskAccessProbes() -> [OneContextFullDiskAccessProbe] {
    let home = FileManager.default.homeDirectoryForCurrentUser
    return [
      OneContextFullDiskAccessProbe(label: "Messages chat database", url: home.appendingPathComponent("Library/Messages/chat.db")),
      OneContextFullDiskAccessProbe(label: "Safari history database", url: home.appendingPathComponent("Library/Safari/History.db")),
      OneContextFullDiskAccessProbe(label: "Mail library", url: home.appendingPathComponent("Library/Mail", isDirectory: true)),
      OneContextFullDiskAccessProbe(label: "AddressBook database", url: home.appendingPathComponent("Library/Application Support/AddressBook", isDirectory: true)),
      OneContextFullDiskAccessProbe(label: "Chrome profile data", url: home.appendingPathComponent("Library/Application Support/Google/Chrome", isDirectory: true)),
      OneContextFullDiskAccessProbe(label: "Brave profile data", url: home.appendingPathComponent("Library/Application Support/BraveSoftware/Brave-Browser", isDirectory: true))
    ]
  }

  private static func canReadProtectedPath(_ url: URL) -> Bool {
    var isDirectory: ObjCBool = false
    guard FileManager.default.fileExists(atPath: url.path, isDirectory: &isDirectory) else {
      return false
    }
    if isDirectory.boolValue {
      return (try? FileManager.default.contentsOfDirectory(atPath: url.path)) != nil
    }
    guard let handle = FileHandle(forReadingAtPath: url.path) else {
      return false
    }
    try? handle.close()
    return true
  }

  private static func currentDesignatedRequirementString() -> String? {
    var code: SecCode?
    guard SecCodeCopySelf(SecCSFlags(), &code) == errSecSuccess,
      let code
    else {
      return nil
    }
    var staticCode: SecStaticCode?
    guard SecCodeCopyStaticCode(code, SecCSFlags(), &staticCode) == errSecSuccess,
      let staticCode
    else {
      return nil
    }
    var requirement: SecRequirement?
    guard SecCodeCopyDesignatedRequirement(staticCode, SecCSFlags(), &requirement) == errSecSuccess,
      let requirement
    else {
      return nil
    }
    var requirementString: CFString?
    guard SecRequirementCopyString(requirement, SecCSFlags(), &requirementString) == errSecSuccess,
      let requirementString
    else {
      return nil
    }
    return requirementString as String
  }

  private static func designatedRequirementString(for staticCodeURL: URL) -> String? {
    var staticCode: SecStaticCode?
    guard SecStaticCodeCreateWithPath(staticCodeURL as CFURL, SecCSFlags(), &staticCode) == errSecSuccess,
      let staticCode
    else {
      return nil
    }
    var requirement: SecRequirement?
    guard SecCodeCopyDesignatedRequirement(staticCode, SecCSFlags(), &requirement) == errSecSuccess,
      let requirement
    else {
      return nil
    }
    var requirementString: CFString?
    guard SecRequirementCopyString(requirement, SecCSFlags(), &requirementString) == errSecSuccess,
      let requirementString
    else {
      return nil
    }
    return requirementString as String
  }

  private static func currentExecutablePath() -> String? {
    var size = UInt32(0)
    _NSGetExecutablePath(nil, &size)
    var buffer = [CChar](repeating: 0, count: Int(size))
    guard _NSGetExecutablePath(&buffer, &size) == 0 else { return nil }
    let pathBytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    return String(decoding: pathBytes, as: UTF8.self)
  }

  private static func sha256(_ value: String) -> String {
    let digest = SHA256.hash(data: Data(value.utf8))
    return digest.map { String(format: "%02x", $0) }.joined()
  }

  private static func permissionError(_ message: String) -> NSError {
    NSError(
      domain: "OneContextSystemPermissions",
      code: 1,
      userInfo: [NSLocalizedDescriptionKey: message]
    )
  }
}

@available(macOS 13.0, *)
private final class ScreenAndSystemAudioProofOutput: NSObject, SCStreamOutput {
  private let semaphore = DispatchSemaphore(value: 0)
  private let stateQueue = DispatchQueue(label: "com.haptica.1context.permissions.screencapture-proof-output")
  private var screenFrameArrived = false

  func waitForScreenFrame(timeout: TimeInterval) -> Bool {
    if stateQueue.sync(execute: { screenFrameArrived }) {
      return true
    }
    return semaphore.wait(timeout: .now() + timeout) == .success
  }

  func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
    guard type == .screen, CMSampleBufferIsValid(sampleBuffer) else { return }
    let shouldSignal = stateQueue.sync { () -> Bool in
      if screenFrameArrived {
        return false
      }
      screenFrameArrived = true
      return true
    }
    if shouldSignal {
      semaphore.signal()
    }
  }
}
