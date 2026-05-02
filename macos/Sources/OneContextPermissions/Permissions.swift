import ApplicationServices
import CoreGraphics
import Foundation

public enum PermissionKind: String, Codable, CaseIterable, Sendable {
  case screenRecording = "screen_recording"
  case accessibility

  public var displayName: String {
    switch self {
    case .screenRecording:
      return "Screen Recording"
    case .accessibility:
      return "Accessibility"
    }
  }

  public var settingsLocation: String {
    switch self {
    case .screenRecording:
      return "System Settings > Privacy & Security > Screen & System Audio Recording"
    case .accessibility:
      return "System Settings > Privacy & Security > Accessibility"
    }
  }
}

public enum PermissionStatus: String, Codable, Sendable {
  case granted
  case notGranted = "not_granted"
  case notChecked = "not_checked"
  case unavailable
}

public struct PermissionOwner: Codable, Equatable, Sendable {
  public let displayName: String
  public let bundleIdentifier: String

  public init(displayName: String, bundleIdentifier: String) {
    self.displayName = displayName
    self.bundleIdentifier = bundleIdentifier
  }

  public static let mainApp = PermissionOwner(
    displayName: "1Context.app",
    bundleIdentifier: "com.haptica.1context"
  )
}

public struct PermissionSnapshot: Codable, Equatable, Sendable {
  public let kind: PermissionKind
  public let status: PermissionStatus
  public let owner: PermissionOwner
  public let checkedByCurrentProcess: Bool
  public let canPromptFromCurrentProcess: Bool
  public let requiresRelaunchAfterGrant: Bool
  public let reason: String
  public let repairHint: String

  public init(
    kind: PermissionKind,
    status: PermissionStatus,
    owner: PermissionOwner,
    checkedByCurrentProcess: Bool,
    canPromptFromCurrentProcess: Bool,
    requiresRelaunchAfterGrant: Bool,
    reason: String,
    repairHint: String
  ) {
    self.kind = kind
    self.status = status
    self.owner = owner
    self.checkedByCurrentProcess = checkedByCurrentProcess
    self.canPromptFromCurrentProcess = canPromptFromCurrentProcess
    self.requiresRelaunchAfterGrant = requiresRelaunchAfterGrant
    self.reason = reason
    self.repairHint = repairHint
  }
}

public protocol PermissionChecking: Sendable {
  func snapshot(for kind: PermissionKind) -> PermissionSnapshot
}

public struct PermissionReporter: Sendable {
  private let checker: any PermissionChecking

  public init(checker: any PermissionChecking = MacOSPermissionChecker()) {
    self.checker = checker
  }

  public func snapshots(kinds: [PermissionKind] = PermissionKind.allCases) -> [PermissionSnapshot] {
    kinds.map { checker.snapshot(for: $0) }
  }
}

public struct MacOSPermissionChecker: PermissionChecking, Sendable {
  private let owner: PermissionOwner
  private let currentBundleIdentifier: String?
  private let checkCurrentProcess: Bool

  public init(
    owner: PermissionOwner = .mainApp,
    currentBundleIdentifier: String? = Bundle.main.bundleIdentifier,
    checkCurrentProcess: Bool = false
  ) {
    self.owner = owner
    self.currentBundleIdentifier = currentBundleIdentifier
    self.checkCurrentProcess = checkCurrentProcess
  }

  public func snapshot(for kind: PermissionKind) -> PermissionSnapshot {
    let isOwnerProcess = currentBundleIdentifier == owner.bundleIdentifier
    let canCheck = checkCurrentProcess && isOwnerProcess

    guard canCheck else {
      return PermissionSnapshot(
        kind: kind,
        status: .notChecked,
        owner: owner,
        checkedByCurrentProcess: false,
        canPromptFromCurrentProcess: false,
        requiresRelaunchAfterGrant: true,
        reason: "\(owner.displayName) owns \(kind.displayName) consent. The CLI does not check or request it so macOS does not attach sensitive permissions to the wrong process.",
        repairHint: "Open \(owner.displayName), then enable \(kind.displayName) in \(kind.settingsLocation) when you choose to turn on the related feature."
      )
    }

    return PermissionSnapshot(
      kind: kind,
      status: currentProcessStatus(for: kind),
      owner: owner,
      checkedByCurrentProcess: true,
      canPromptFromCurrentProcess: true,
      requiresRelaunchAfterGrant: true,
      reason: reason(for: kind),
      repairHint: "Enable \(kind.displayName) for \(owner.displayName) in \(kind.settingsLocation), then relaunch 1Context if macOS asks."
    )
  }

  private func currentProcessStatus(for kind: PermissionKind) -> PermissionStatus {
    switch kind {
    case .accessibility:
      let options = ["AXTrustedCheckOptionPrompt": false] as CFDictionary
      return AXIsProcessTrustedWithOptions(options) ? .granted : .notGranted
    case .screenRecording:
      if #available(macOS 11.0, *) {
        return CGPreflightScreenCaptureAccess() ? .granted : .notGranted
      }
      return .unavailable
    }
  }

  private func reason(for kind: PermissionKind) -> String {
    switch kind {
    case .screenRecording:
      return "Allows a future passive context feature to understand visible work only after explicit user consent."
    case .accessibility:
      return "Allows future automation/control features to inspect or act on UI only after explicit user consent."
    }
  }
}

public enum PermissionDiagnostics {
  public static func render(_ snapshots: [PermissionSnapshot]) -> [String] {
    snapshots.flatMap(render)
  }

  public static func render(_ snapshot: PermissionSnapshot) -> [String] {
    [
      "  \(snapshot.kind.displayName): \(display(snapshot.status))",
      "    Owner: \(snapshot.owner.displayName) (\(snapshot.owner.bundleIdentifier))",
      "    Checked Here: \(snapshot.checkedByCurrentProcess ? "yes" : "no")",
      "    Can Prompt Here: \(snapshot.canPromptFromCurrentProcess ? "yes" : "no")",
      "    Relaunch After Grant: \(snapshot.requiresRelaunchAfterGrant ? "yes" : "no")",
      "    Reason: \(snapshot.reason)",
      "    Next Action: \(snapshot.repairHint)"
    ]
  }

  private static func display(_ status: PermissionStatus) -> String {
    switch status {
    case .granted:
      return "granted"
    case .notGranted:
      return "not granted"
    case .notChecked:
      return "not checked"
    case .unavailable:
      return "unavailable"
    }
  }
}
