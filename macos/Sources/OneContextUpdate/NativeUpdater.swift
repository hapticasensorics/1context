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
  public let appLocation: NativeUpdaterAppLocation?
  public let updateAvailable: Bool
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
    appLocation: NativeUpdaterAppLocation? = nil,
    updateAvailable: Bool,
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
    self.appLocation = appLocation
    self.updateAvailable = updateAvailable
    self.canInstallFromCurrentProcess = canInstallFromCurrentProcess
    self.userFacingStatus = userFacingStatus
    self.nextAction = nextAction
  }
}

public protocol NativeUpdater {
  func snapshot(currentVersion: String) async -> NativeUpdateSnapshot
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

  public let feedURL: URL?
  public let publicEdKey: String?
  public let automaticChecksEnabled: Bool

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
    automaticChecksEnabled: Bool = false
  ) {
    self.feedURL = feedURL
    self.publicEdKey = publicEdKey
    self.automaticChecksEnabled = automaticChecksEnabled
  }

  public init(infoDictionary: [String: Any]) {
    self.init(
      feedURL: Self.parseURL(infoDictionary[Self.feedURLInfoKey]),
      publicEdKey: infoDictionary[Self.publicEdKeyInfoKey] as? String,
      automaticChecksEnabled: Self.parseBool(infoDictionary[Self.automaticChecksInfoKey])
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
}

public struct SparkleUpdateDriverSnapshot: Codable, Equatable, Sendable {
  public let availability: NativeUpdaterAvailability
  public let latestVersion: String?
  public let updateAvailable: Bool
  public let canInstallUpdates: Bool
  public let userFacingStatus: String
  public let nextAction: String

  public init(
    availability: NativeUpdaterAvailability,
    latestVersion: String?,
    updateAvailable: Bool,
    canInstallUpdates: Bool,
    userFacingStatus: String,
    nextAction: String
  ) {
    self.availability = availability
    self.latestVersion = latestVersion
    self.updateAvailable = updateAvailable
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
        appLocation: appContext.location,
        updateAvailable: false,
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
        appLocation: appContext.location,
        updateAvailable: false,
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
      appLocation: appContext.location,
      updateAvailable: driverSnapshot.updateAvailable,
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
