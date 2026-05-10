import Foundation
import OneContextCore

public enum NativeUpdaterImplementation: String, Codable, Sendable {
  case sparkle
}

public enum NativeUpdaterAvailability: String, Codable, Sendable {
  case available
  case notConfigured = "not_configured"
  case unavailable
}

public struct NativeUpdateSnapshot: Codable, Equatable, Sendable {
  public let implementation: NativeUpdaterImplementation
  public let availability: NativeUpdaterAvailability
  public let currentVersion: String
  public let latestVersion: String?
  public let feedURL: String?
  public let configurationComplete: Bool?
  public let automaticChecksEnabled: Bool?
  public let automaticDownloadsEnabled: Bool?
  public let scheduledCheckInterval: TimeInterval?
  public let appLocation: NativeUpdaterAppLocation?
  public let updateAvailable: Bool
  public let mandatoryUpdateAvailable: Bool
  public let minimumUpdateVersion: String?
  public let minimumAutoupdateVersion: String?
  public let canInstallFromCurrentProcess: Bool
  public let userFacingStatus: String
  public let nextAction: String

  public init(
    implementation: NativeUpdaterImplementation,
    availability: NativeUpdaterAvailability,
    currentVersion: String,
    latestVersion: String?,
    feedURL: String? = nil,
    configurationComplete: Bool? = nil,
    automaticChecksEnabled: Bool? = nil,
    automaticDownloadsEnabled: Bool? = nil,
    scheduledCheckInterval: TimeInterval? = nil,
    appLocation: NativeUpdaterAppLocation? = nil,
    updateAvailable: Bool,
    mandatoryUpdateAvailable: Bool = false,
    minimumUpdateVersion: String? = nil,
    minimumAutoupdateVersion: String? = nil,
    canInstallFromCurrentProcess: Bool,
    userFacingStatus: String,
    nextAction: String
  ) {
    self.implementation = implementation
    self.availability = availability
    self.currentVersion = currentVersion
    self.latestVersion = latestVersion
    self.feedURL = feedURL
    self.configurationComplete = configurationComplete
    self.automaticChecksEnabled = automaticChecksEnabled
    self.automaticDownloadsEnabled = automaticDownloadsEnabled
    self.scheduledCheckInterval = scheduledCheckInterval
    self.appLocation = appLocation
    self.updateAvailable = updateAvailable
    self.mandatoryUpdateAvailable = mandatoryUpdateAvailable
    self.minimumUpdateVersion = minimumUpdateVersion
    self.minimumAutoupdateVersion = minimumAutoupdateVersion
    self.canInstallFromCurrentProcess = canInstallFromCurrentProcess
    self.userFacingStatus = userFacingStatus
    self.nextAction = nextAction
  }
}

public protocol NativeUpdater {
  func snapshot(currentVersion: String) async -> NativeUpdateSnapshot
}

public enum MandatoryUpdateRuntimePolicy {
  public static func shouldPausePassiveRemembering(_ snapshot: NativeUpdateSnapshot) -> Bool {
    false
  }

  public static func startBlockedMessage(_ snapshot: NativeUpdateSnapshot) -> String {
    "1Context will keep remembering while the updater retries."
  }
}

public enum NativeUpdaterAppLocation: String, Codable, Equatable, Sendable {
  case applications
  case appBundleOutsideApplications = "app_bundle_outside_applications"
  case commandLineTool = "command_line_tool"
  case unknown

  public var canInstallAppUpdates: Bool {
    self == .applications
  }

  public var userFacingDescription: String {
    switch self {
    case .applications:
      return "Applications"
    case .appBundleOutsideApplications:
      return "outside Applications"
    case .commandLineTool:
      return "command line tool"
    case .unknown:
      return "unknown"
    }
  }
}

public struct NativeUpdaterAppContext: Codable, Equatable, Sendable {
  public let bundleURL: URL?
  public let executableURL: URL?
  public let location: NativeUpdaterAppLocation

  public init(
    bundleURL: URL?,
    executableURL: URL?,
    location: NativeUpdaterAppLocation? = nil
  ) {
    self.bundleURL = bundleURL
    self.executableURL = executableURL
    self.location = location ?? Self.classify(bundleURL: bundleURL, executableURL: executableURL)
  }

  public static func current(bundle: Bundle = .main) -> NativeUpdaterAppContext {
    NativeUpdaterAppContext(bundleURL: bundle.bundleURL, executableURL: bundle.executableURL)
  }

  public static func classify(bundleURL: URL?, executableURL: URL? = nil) -> NativeUpdaterAppLocation {
    guard let bundleURL else {
      return executableURL == nil ? .unknown : .commandLineTool
    }

    let standardizedBundle = bundleURL.standardizedFileURL.resolvingSymlinksInPath()
    guard standardizedBundle.pathExtension == "app" else {
      return .commandLineTool
    }

    let components = standardizedBundle.pathComponents
    if components.count >= 3, components[0] == "/", components[1] == "Applications" {
      return .applications
    }
    return .appBundleOutsideApplications
  }
}

public struct SparkleUpdaterConfiguration: Codable, Equatable, Sendable {
  public static let feedURLInfoKey = "SUFeedURL"
  public static let publicEdKeyInfoKey = "SUPublicEDKey"
  public static let automaticChecksInfoKey = "SUEnableAutomaticChecks"
  public static let automaticDownloadsInfoKey = "SUAutomaticallyUpdate"
  public static let scheduledCheckIntervalInfoKey = "SUScheduledCheckInterval"

  public let feedURL: URL?
  public let publicEdKey: String?
  public let automaticChecksEnabled: Bool
  public let automaticDownloadsEnabled: Bool
  public let scheduledCheckInterval: TimeInterval?
  public let userFacingPolicy: UpdateUserFacingPolicy

  public var isConfigured: Bool {
    feedURL != nil && trimmedPublicEdKey != nil
  }

  public var missingConfigurationSummary: String? {
    var missing: [String] = []
    if feedURL == nil {
      missing.append(Self.feedURLInfoKey)
    }
    if trimmedPublicEdKey == nil {
      missing.append(Self.publicEdKeyInfoKey)
    }
    return missing.isEmpty ? nil : missing.joined(separator: ", ")
  }

  public init(
    feedURL: URL?,
    publicEdKey: String?,
    automaticChecksEnabled: Bool = false,
    automaticDownloadsEnabled: Bool = false,
    scheduledCheckInterval: TimeInterval? = nil,
    userFacingPolicy: UpdateUserFacingPolicy = .default
  ) {
    self.feedURL = feedURL
    self.publicEdKey = publicEdKey
    self.automaticChecksEnabled = automaticChecksEnabled
    self.automaticDownloadsEnabled = automaticDownloadsEnabled
    self.scheduledCheckInterval = scheduledCheckInterval
    self.userFacingPolicy = userFacingPolicy
  }

  public init(infoDictionary: [String: Any]) {
    self.init(
      feedURL: Self.parseURL(infoDictionary[Self.feedURLInfoKey]),
      publicEdKey: infoDictionary[Self.publicEdKeyInfoKey] as? String,
      automaticChecksEnabled: Self.parseBool(infoDictionary[Self.automaticChecksInfoKey]),
      automaticDownloadsEnabled: Self.parseBool(infoDictionary[Self.automaticDownloadsInfoKey]),
      scheduledCheckInterval: Self.parseTimeInterval(infoDictionary[Self.scheduledCheckIntervalInfoKey]),
      userFacingPolicy: UpdateUserFacingPolicy(infoDictionary: infoDictionary)
    )
  }

  public init(appBundleURL: URL) {
    let infoPlistURL = appBundleURL.appendingPathComponent("Contents/Info.plist")
    let infoDictionary = NSDictionary(contentsOf: infoPlistURL) as? [String: Any] ?? [:]
    self.init(infoDictionary: infoDictionary)
  }

  public static func current(bundle: Bundle = .main) -> SparkleUpdaterConfiguration {
    SparkleUpdaterConfiguration(infoDictionary: bundle.infoDictionary ?? [:])
  }

  private var trimmedPublicEdKey: String? {
    guard let publicEdKey else { return nil }
    let trimmed = publicEdKey.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? nil : trimmed
  }

  private static func parseURL(_ value: Any?) -> URL? {
    if let url = value as? URL, url.scheme != nil {
      return url
    }
    guard let string = value as? String else { return nil }
    let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !trimmed.isEmpty, let url = URL(string: trimmed), url.scheme != nil else {
      return nil
    }
    return url
  }

  private static func parseBool(_ value: Any?) -> Bool {
    if let bool = value as? Bool {
      return bool
    }
    if let number = value as? NSNumber {
      return number.boolValue
    }
    if let string = value as? String {
      switch string.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
      case "1", "true", "yes":
        return true
      default:
        return false
      }
    }
    return false
  }

  private static func parseTimeInterval(_ value: Any?) -> TimeInterval? {
    if let number = value as? NSNumber {
      return number.doubleValue > 0 ? number.doubleValue : nil
    }
    if let string = value as? String {
      let trimmed = string.trimmingCharacters(in: .whitespacesAndNewlines)
      guard let interval = TimeInterval(trimmed), interval > 0 else { return nil }
      return interval
    }
    return nil
  }
}

public struct UpdateUserFacingPolicy: Codable, Equatable, Sendable {
  public static let optionalPromptTitleInfoKey = "OneContextUpdateOptionalPromptTitle"
  public static let optionalPromptBodyInfoKey = "OneContextUpdateOptionalPromptBody"
  public static let failureTitleInfoKey = "OneContextUpdateFailureTitle"
  public static let failureBodyInfoKey = "OneContextUpdateFailureBody"
  public static let postInstallMessageEnabledInfoKey = "OneContextUpdatePostInstallMessageEnabled"
  public static let postInstallTitleInfoKey = "OneContextUpdatePostInstallTitle"
  public static let postInstallBodyInfoKey = "OneContextUpdatePostInstallBody"
  public static let showReleaseNotesInUpdateWindowInfoKey = "OneContextUpdateShowReleaseNotesInUpdateWindow"

  public static let `default` = UpdateUserFacingPolicy()

  public let optionalPromptTitle: String
  public let optionalPromptBody: String
  public let failureTitle: String
  public let failureBody: String
  public let postInstallMessageEnabled: Bool
  public let postInstallTitle: String
  public let postInstallBody: String
  public let showReleaseNotesInUpdateWindow: Bool

  public init(
    optionalPromptTitle: String = "Update 1Context?",
    optionalPromptBody: String = "A 1Context update is ready.",
    failureTitle: String = "Update failed.",
    failureBody: String = "Please contact support at paul@haptica.ai.",
    postInstallMessageEnabled: Bool = false,
    postInstallTitle: String = "1Context Improved!",
    postInstallBody: String = "",
    showReleaseNotesInUpdateWindow: Bool = false
  ) {
    self.optionalPromptTitle = Self.nonEmpty(optionalPromptTitle, fallback: "Update 1Context?")
    self.optionalPromptBody = Self.nonEmpty(optionalPromptBody, fallback: "A 1Context update is ready.")
    self.failureTitle = Self.nonEmpty(failureTitle, fallback: "Update failed.")
    self.failureBody = Self.nonEmpty(failureBody, fallback: "Please contact support at paul@haptica.ai.")
    self.postInstallMessageEnabled = postInstallMessageEnabled
    self.postInstallTitle = Self.nonEmpty(postInstallTitle, fallback: "1Context Improved!")
    self.postInstallBody = postInstallBody
    self.showReleaseNotesInUpdateWindow = showReleaseNotesInUpdateWindow
  }

  public init(infoDictionary: [String: Any]) {
    self.init(
      optionalPromptTitle: Self.string(infoDictionary[Self.optionalPromptTitleInfoKey], fallback: "Update 1Context?"),
      optionalPromptBody: Self.string(infoDictionary[Self.optionalPromptBodyInfoKey], fallback: "A 1Context update is ready."),
      failureTitle: Self.string(infoDictionary[Self.failureTitleInfoKey], fallback: "Update failed."),
      failureBody: Self.string(infoDictionary[Self.failureBodyInfoKey], fallback: "Please contact support at paul@haptica.ai."),
      postInstallMessageEnabled: Self.bool(infoDictionary[Self.postInstallMessageEnabledInfoKey]),
      postInstallTitle: Self.string(infoDictionary[Self.postInstallTitleInfoKey], fallback: "1Context Improved!"),
      postInstallBody: Self.string(infoDictionary[Self.postInstallBodyInfoKey], fallback: ""),
      showReleaseNotesInUpdateWindow: Self.bool(infoDictionary[Self.showReleaseNotesInUpdateWindowInfoKey])
    )
  }

  public func optionalPromptBody(displayVersion: String) -> String {
    optionalPromptBody.replacingOccurrences(of: "{version}", with: displayVersion)
  }

  public func postInstallBody(displayVersion: String) -> String {
    postInstallBody.replacingOccurrences(of: "{version}", with: displayVersion)
  }

  private static func string(_ value: Any?, fallback: String) -> String {
    guard let string = value as? String else { return fallback }
    return nonEmpty(string, fallback: fallback)
  }

  private static func nonEmpty(_ value: String, fallback: String) -> String {
    let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? fallback : trimmed
  }

  private static func bool(_ value: Any?) -> Bool {
    if let bool = value as? Bool {
      return bool
    }
    if let number = value as? NSNumber {
      return number.boolValue
    }
    if let string = value as? String {
      switch string.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
      case "1", "true", "yes":
        return true
      default:
        return false
      }
    }
    return false
  }
}

public struct PostInstallUpdateMessage: Equatable, Sendable {
  public let title: String
  public let body: String
}

public enum PostInstallUpdateMessageGate {
  public static func message(
    currentVersion: String,
    previousVersion: String?,
    lastShownVersion: String?,
    policy: UpdateUserFacingPolicy
  ) -> PostInstallUpdateMessage? {
    guard policy.postInstallMessageEnabled else { return nil }
    guard let previousVersion,
      !previousVersion.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
      previousVersion != currentVersion
    else {
      return nil
    }
    guard lastShownVersion != currentVersion else { return nil }
    return PostInstallUpdateMessage(
      title: policy.postInstallTitle,
      body: policy.postInstallBody(displayVersion: currentVersion)
    )
  }
}

public struct SparkleUpdateDriverSnapshot: Codable, Equatable, Sendable {
  public let availability: NativeUpdaterAvailability
  public let latestVersion: String?
  public let updateAvailable: Bool
  public let mandatoryUpdateAvailable: Bool
  public let minimumUpdateVersion: String?
  public let minimumAutoupdateVersion: String?
  public let canInstallUpdates: Bool
  public let userFacingStatus: String
  public let nextAction: String

  public init(
    availability: NativeUpdaterAvailability,
    latestVersion: String?,
    updateAvailable: Bool,
    mandatoryUpdateAvailable: Bool = false,
    minimumUpdateVersion: String? = nil,
    minimumAutoupdateVersion: String? = nil,
    canInstallUpdates: Bool,
    userFacingStatus: String,
    nextAction: String
  ) {
    self.availability = availability
    self.latestVersion = latestVersion
    self.updateAvailable = updateAvailable
    self.mandatoryUpdateAvailable = mandatoryUpdateAvailable
    self.minimumUpdateVersion = minimumUpdateVersion
    self.minimumAutoupdateVersion = minimumAutoupdateVersion
    self.canInstallUpdates = canInstallUpdates
    self.userFacingStatus = userFacingStatus
    self.nextAction = nextAction
  }
}

public protocol SparkleUpdateDriver: Sendable {
  func snapshot(
    currentVersion: String,
    configuration: SparkleUpdaterConfiguration
  ) async -> SparkleUpdateDriverSnapshot
}

public struct SparkleNativeUpdater: NativeUpdater, Sendable {
  public let configuration: SparkleUpdaterConfiguration
  public let appContext: NativeUpdaterAppContext
  private let driver: any SparkleUpdateDriver

  public init(
    configuration: SparkleUpdaterConfiguration = .current(),
    appContext: NativeUpdaterAppContext = .current(),
    driver: any SparkleUpdateDriver
  ) {
    self.configuration = configuration
    self.appContext = appContext
    self.driver = driver
  }

  public func snapshot(currentVersion: String = oneContextVersion) async -> NativeUpdateSnapshot {
    guard configuration.isConfigured else {
      return NativeUpdateSnapshot(
        implementation: .sparkle,
        availability: .notConfigured,
        currentVersion: currentVersion,
        latestVersion: nil,
        feedURL: configuration.feedURL?.absoluteString,
        configurationComplete: false,
        automaticChecksEnabled: configuration.automaticChecksEnabled,
        automaticDownloadsEnabled: configuration.automaticDownloadsEnabled,
        scheduledCheckInterval: configuration.scheduledCheckInterval,
        appLocation: appContext.location,
        updateAvailable: false,
        mandatoryUpdateAvailable: false,
        canInstallFromCurrentProcess: false,
        userFacingStatus: "Sparkle updates are not configured in this build.",
        nextAction: "Set \(configuration.missingConfigurationSummary ?? "Sparkle Info.plist keys") before release."
      )
    }

    guard appContext.location.canInstallAppUpdates else {
      return NativeUpdateSnapshot(
        implementation: .sparkle,
        availability: .unavailable,
        currentVersion: currentVersion,
        latestVersion: nil,
        feedURL: configuration.feedURL?.absoluteString,
        configurationComplete: true,
        automaticChecksEnabled: configuration.automaticChecksEnabled,
        automaticDownloadsEnabled: configuration.automaticDownloadsEnabled,
        scheduledCheckInterval: configuration.scheduledCheckInterval,
        appLocation: appContext.location,
        updateAvailable: false,
        mandatoryUpdateAvailable: false,
        canInstallFromCurrentProcess: false,
        userFacingStatus: "Move 1Context to Applications to install app updates.",
        nextAction: "Open 1Context from /Applications/1Context.app."
      )
    }

    let driverSnapshot = await driver.snapshot(
      currentVersion: currentVersion,
      configuration: configuration
    )
    return NativeUpdateSnapshot(
      implementation: .sparkle,
      availability: driverSnapshot.availability,
      currentVersion: currentVersion,
      latestVersion: driverSnapshot.latestVersion,
      feedURL: configuration.feedURL?.absoluteString,
      configurationComplete: true,
      automaticChecksEnabled: configuration.automaticChecksEnabled,
      automaticDownloadsEnabled: configuration.automaticDownloadsEnabled,
      scheduledCheckInterval: configuration.scheduledCheckInterval,
      appLocation: appContext.location,
      updateAvailable: driverSnapshot.updateAvailable,
      mandatoryUpdateAvailable: driverSnapshot.mandatoryUpdateAvailable,
      minimumUpdateVersion: driverSnapshot.minimumUpdateVersion,
      minimumAutoupdateVersion: driverSnapshot.minimumAutoupdateVersion,
      canInstallFromCurrentProcess: driverSnapshot.canInstallUpdates,
      userFacingStatus: driverSnapshot.userFacingStatus,
      nextAction: driverSnapshot.nextAction
    )
  }
}

public enum NativeUpdateDiagnostics {
  public static func render(_ snapshot: NativeUpdateSnapshot) -> [String] {
    var lines = [
      "  Native Updater: \(display(snapshot.availability))",
      "  Implementation: \(snapshot.implementation.rawValue)",
      "  Current Version: \(snapshot.currentVersion)",
      "  Latest Version: \(snapshot.latestVersion ?? "unknown")",
      "  Update Available: \(snapshot.updateAvailable ? "yes" : "no")",
      "  Mandatory Update: \(snapshot.mandatoryUpdateAvailable ? "yes" : "no")",
      "  Can Install Here: \(snapshot.canInstallFromCurrentProcess ? "yes" : "no")",
      "  Status: \(snapshot.userFacingStatus)",
      "  Next Action: \(snapshot.nextAction)"
    ]
    if let appLocation = snapshot.appLocation {
      lines.append("  App Location: \(appLocation.userFacingDescription)")
    }
    if let configurationComplete = snapshot.configurationComplete {
      lines.append("  Configuration: \(configurationComplete ? "complete" : "incomplete")")
    }
    if let feedURL = snapshot.feedURL {
      lines.append("  Feed URL: \(feedURL)")
    }
    if let automaticChecksEnabled = snapshot.automaticChecksEnabled {
      lines.append("  Automatic Checks: \(automaticChecksEnabled ? "yes" : "no")")
    }
    if let automaticDownloadsEnabled = snapshot.automaticDownloadsEnabled {
      lines.append("  Automatic Downloads: \(automaticDownloadsEnabled ? "yes" : "no")")
    }
    if let scheduledCheckInterval = snapshot.scheduledCheckInterval {
      lines.append("  Scheduled Check Interval: \(Int(scheduledCheckInterval))s")
    }
    if let minimumUpdateVersion = snapshot.minimumUpdateVersion {
      lines.append("  Minimum Update Version: \(minimumUpdateVersion)")
    }
    if let minimumAutoupdateVersion = snapshot.minimumAutoupdateVersion {
      lines.append("  Minimum Autoupdate Version: \(minimumAutoupdateVersion)")
    }
    return lines
  }

  private static func display(_ availability: NativeUpdaterAvailability) -> String {
    switch availability {
    case .available:
      return "available"
    case .notConfigured:
      return "not configured"
    case .unavailable:
      return "unavailable"
    }
  }
}
