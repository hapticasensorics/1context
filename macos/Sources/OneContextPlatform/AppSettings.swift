import Darwin
import Foundation

public enum WikiAutomaticPublishCadence: String, CaseIterable, Sendable {
  case noLimit = "no_limit"
  case oneMinute = "1_minute"
  case thirtyMinutes = "30_minute"

  public static let defaultValue: WikiAutomaticPublishCadence = .oneMinute

  public var title: String {
    switch self {
    case .noLimit:
      return "No Limit"
    case .oneMinute:
      return "1 Min"
    case .thirtyMinutes:
      return "30 Min"
    }
  }

  public var minimumAutomaticInterval: TimeInterval? {
    switch self {
    case .noLimit:
      return nil
    case .oneMinute:
      return 60
    case .thirtyMinutes:
      return 30 * 60
    }
  }

  public static func parse(_ rawValue: String?) -> WikiAutomaticPublishCadence? {
    guard let rawValue else { return nil }
    switch rawValue.trimmingCharacters(in: .whitespacesAndNewlines) {
    case "no_limit", "none", "unlimited":
      return .noLimit
    case "1_minute", "1_min", "one_minute":
      return .oneMinute
    case "30_minute", "30_minutes", "30_min", "thirty_minutes":
      return .thirtyMinutes
    default:
      return nil
    }
  }
}

public enum WikiAutomaticUpdateCadence: String, CaseIterable, Sendable {
  case disabled
  case twelveHours = "12_hours"
  case oneHour = "1_hour"
  case constant

  public static let defaultValue: WikiAutomaticUpdateCadence = .twelveHours

  public var title: String {
    switch self {
    case .disabled:
      return "Disabled"
    case .twelveHours:
      return "12 Hrs"
    case .oneHour:
      return "1 Hr"
    case .constant:
      return "Constant"
    }
  }

  public var isEnabled: Bool {
    self != .disabled
  }

  public var minimumAutomaticInterval: TimeInterval? {
    switch self {
    case .disabled:
      return nil
    case .twelveHours:
      return 12 * 60 * 60
    case .oneHour:
      return 60 * 60
    case .constant:
      return 0
    }
  }

  public static func parse(_ rawValue: String?) -> WikiAutomaticUpdateCadence? {
    guard let rawValue else { return nil }
    switch rawValue.trimmingCharacters(in: .whitespacesAndNewlines) {
    case "disabled", "disable", "off", "none":
      return .disabled
    case "12_hours", "12_hour", "12hrs", "12hr", "twelve_hours":
      return .twelveHours
    case "1_hour", "1_hr", "1hr", "one_hour", "hourly":
      return .oneHour
    case "constant", "always", "continuous", "no_limit":
      return .constant
    default:
      return nil
    }
  }
}

public enum WikiAgentConcurrencyLimit: String, CaseIterable, Sendable {
  case three = "3"
  case five = "5"
  case twelve = "12"
  case noLimit = "no_limit"

  public static let defaultValue: WikiAgentConcurrencyLimit = .five
  public static let noLimitArgumentValue = 10_000

  public var title: String {
    switch self {
    case .three:
      return "3"
    case .five:
      return "5"
    case .twelve:
      return "12"
    case .noLimit:
      return "No Limit"
    }
  }

  public var contextEngineArgumentValue: Int {
    switch self {
    case .three:
      return 3
    case .five:
      return 5
    case .twelve:
      return 12
    case .noLimit:
      return Self.noLimitArgumentValue
    }
  }

  public static func parse(_ rawValue: String?) -> WikiAgentConcurrencyLimit? {
    guard let rawValue else { return nil }
    switch rawValue.trimmingCharacters(in: .whitespacesAndNewlines) {
    case "3", "three":
      return .three
    case "5", "five":
      return .five
    case "12", "twelve":
      return .twelve
    case "no_limit", "none", "unlimited", "all":
      return .noLimit
    default:
      return nil
    }
  }
}

public enum OneContextAppSettings {
  public static let wikiAutomaticPublishCadenceKey = "WikiAutomaticPublishCadence"
  public static let wikiAutomaticUpdateCadenceKey = "WikiAutomaticUpdateCadence"
  public static let wikiAgentConcurrencyLimitKey = "WikiAgentConcurrencyLimit"

  public static func wikiAutomaticPublishCadence(preferencesPath: String? = nil) -> WikiAutomaticPublishCadence {
    if preferencesPath != nil {
      guard let rawValue = preferenceString(forKey: wikiAutomaticPublishCadenceKey, preferencesPath: preferencesPath) else {
        return .defaultValue
      }
      return WikiAutomaticPublishCadence.parse(rawValue) ?? .defaultValue
    }
    return WikiAutomaticPublishCadence.parse(UserDefaults.standard.string(forKey: wikiAutomaticPublishCadenceKey))
      ?? .defaultValue
  }

  public static func setWikiAutomaticPublishCadence(
    _ cadence: WikiAutomaticPublishCadence,
    preferencesPath: String? = nil
  ) throws {
    UserDefaults.standard.set(cadence.rawValue, forKey: wikiAutomaticPublishCadenceKey)
    try setPreferenceValue(cadence.rawValue, forKey: wikiAutomaticPublishCadenceKey, preferencesPath: preferencesPath)
  }

  public static func wikiAutomaticUpdateCadence(preferencesPath: String? = nil) -> WikiAutomaticUpdateCadence {
    if preferencesPath != nil {
      guard let rawValue = preferenceString(forKey: wikiAutomaticUpdateCadenceKey, preferencesPath: preferencesPath) else {
        return .defaultValue
      }
      return WikiAutomaticUpdateCadence.parse(rawValue) ?? .defaultValue
    }
    return WikiAutomaticUpdateCadence.parse(UserDefaults.standard.string(forKey: wikiAutomaticUpdateCadenceKey))
      ?? .defaultValue
  }

  public static func setWikiAutomaticUpdateCadence(
    _ cadence: WikiAutomaticUpdateCadence,
    preferencesPath: String? = nil
  ) throws {
    UserDefaults.standard.set(cadence.rawValue, forKey: wikiAutomaticUpdateCadenceKey)
    try setPreferenceValue(cadence.rawValue, forKey: wikiAutomaticUpdateCadenceKey, preferencesPath: preferencesPath)
  }

  public static func wikiAgentConcurrencyLimit(preferencesPath: String? = nil) -> WikiAgentConcurrencyLimit {
    if preferencesPath != nil {
      guard let rawValue = preferenceString(forKey: wikiAgentConcurrencyLimitKey, preferencesPath: preferencesPath) else {
        return .defaultValue
      }
      return WikiAgentConcurrencyLimit.parse(rawValue) ?? .defaultValue
    }
    return WikiAgentConcurrencyLimit.parse(UserDefaults.standard.string(forKey: wikiAgentConcurrencyLimitKey))
      ?? .defaultValue
  }

  public static func setWikiAgentConcurrencyLimit(
    _ limit: WikiAgentConcurrencyLimit,
    preferencesPath: String? = nil
  ) throws {
    UserDefaults.standard.set(limit.rawValue, forKey: wikiAgentConcurrencyLimitKey)
    try setPreferenceValue(limit.rawValue, forKey: wikiAgentConcurrencyLimitKey, preferencesPath: preferencesPath)
  }

  private static func preferenceString(forKey key: String, preferencesPath: String?) -> String? {
    guard let preferencesPath,
      let preferences = NSDictionary(contentsOfFile: preferencesPath) as? [String: Any]
    else {
      return nil
    }
    return preferences[key] as? String
  }

  private static func setPreferenceValue(_ value: String, forKey key: String, preferencesPath: String?) throws {
    guard let preferencesPath else { return }

    let preferences = NSMutableDictionary(contentsOfFile: preferencesPath) ?? NSMutableDictionary()
    preferences[key] = value
    let directory = URL(fileURLWithPath: preferencesPath).deletingLastPathComponent()
    try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
    guard preferences.write(toFile: preferencesPath, atomically: true) else {
      throw NSError(
        domain: "OneContextAppSettings",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Failed to write preferences at \(preferencesPath)"]
      )
    }
    chmod(preferencesPath, RuntimePermissions.privateFileMode)
  }
}
