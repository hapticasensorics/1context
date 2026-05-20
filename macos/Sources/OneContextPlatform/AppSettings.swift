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

public enum OneContextAppSettings {
  public static let wikiAutomaticPublishCadenceKey = "WikiAutomaticPublishCadence"

  public static func wikiAutomaticPublishCadence(preferencesPath: String? = nil) -> WikiAutomaticPublishCadence {
    if let preferencesPath,
      let preferences = NSDictionary(contentsOfFile: preferencesPath) as? [String: Any],
      let rawValue = preferences[wikiAutomaticPublishCadenceKey] as? String,
      let cadence = WikiAutomaticPublishCadence.parse(rawValue)
    {
      return cadence
    }
    return WikiAutomaticPublishCadence.parse(UserDefaults.standard.string(forKey: wikiAutomaticPublishCadenceKey))
      ?? .defaultValue
  }

  public static func setWikiAutomaticPublishCadence(
    _ cadence: WikiAutomaticPublishCadence,
    preferencesPath: String? = nil
  ) throws {
    UserDefaults.standard.set(cadence.rawValue, forKey: wikiAutomaticPublishCadenceKey)
    guard let preferencesPath else { return }

    let preferences = NSMutableDictionary(contentsOfFile: preferencesPath) ?? NSMutableDictionary()
    preferences[wikiAutomaticPublishCadenceKey] = cadence.rawValue
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
