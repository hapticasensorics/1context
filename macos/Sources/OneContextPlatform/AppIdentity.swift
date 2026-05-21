import Darwin
import Foundation

public enum OneContextAppIdentityKind: String, Codable, Sendable {
  case official
  case dev
}

public struct OneContextAppIdentity: Equatable, Sendable {
  public static let environmentKey = "ONECONTEXT_APP_IDENTITY"
  public static let infoPlistKey = "OneContextAppIdentity"

  public let kind: OneContextAppIdentityKind
  public let displayName: String
  public let appBundleName: String
  public let bundleIdentifier: String
  public let userContentDirectoryName: String
  public let appSupportDirectoryName: String
  public let logDirectoryName: String
  public let cacheDirectoryName: String
  public let preferencesDomain: String
  public let runtimeLaunchAgentLabel: String
  public let menuLaunchAgentLabel: String
  public let localWebProxyLaunchDaemonLabel: String
  public let localWebURLModeRawValue: String
  public let localWebPort: Int
  public let localWebAPIPort: Int

  public var appBundleURL: URL {
    URL(fileURLWithPath: "/Applications/\(appBundleName).app", isDirectory: true)
  }

  public var preferencesFileName: String {
    "\(preferencesDomain).plist"
  }

  public var requiresPrivilegedLocalWebSetup: Bool {
    kind == .official
  }

  public static let official = OneContextAppIdentity(
    kind: .official,
    displayName: "1Context",
    appBundleName: "1Context",
    bundleIdentifier: "com.haptica.1context",
    userContentDirectoryName: "1Context",
    appSupportDirectoryName: "1Context",
    logDirectoryName: "1Context",
    cacheDirectoryName: "1Context",
    preferencesDomain: "com.haptica.1context",
    runtimeLaunchAgentLabel: "com.haptica.1context",
    menuLaunchAgentLabel: "com.haptica.1context.menu",
    localWebProxyLaunchDaemonLabel: "com.haptica.1context.local-web-proxy",
    localWebURLModeRawValue: "local-https-portless",
    localWebPort: 39191,
    localWebAPIPort: 39192
  )

  public static let dev = OneContextAppIdentity(
    kind: .dev,
    displayName: "1Context Dev",
    appBundleName: "1Context Dev",
    bundleIdentifier: "com.haptica.1context.dev",
    userContentDirectoryName: "1Context-Dev",
    appSupportDirectoryName: "1Context Dev",
    logDirectoryName: "1Context Dev",
    cacheDirectoryName: "1Context Dev",
    preferencesDomain: "com.haptica.1context.dev",
    runtimeLaunchAgentLabel: "com.haptica.1context.dev",
    menuLaunchAgentLabel: "com.haptica.1context.dev.menu",
    localWebProxyLaunchDaemonLabel: "com.haptica.1context.dev.local-web-proxy",
    localWebURLModeRawValue: "local-http-ported",
    localWebPort: 39291,
    localWebAPIPort: 39292
  )

  public static func from(_ rawValue: String?) -> OneContextAppIdentity? {
    guard let rawValue else { return nil }
    switch rawValue.trimmingCharacters(in: .whitespacesAndNewlines).lowercased() {
    case "official", "release", "prod", "production", "public":
      return .official
    case "dev", "development", "debug", "local":
      return .dev
    default:
      return nil
    }
  }

  public static func current(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    mainBundle: Bundle = .main,
    executablePath: String? = CommandLine.arguments.first
  ) -> OneContextAppIdentity {
    if let identity = from(environment[environmentKey]) {
      return identity
    }

    if let identity = from(mainBundle.object(forInfoDictionaryKey: infoPlistKey) as? String) {
      return identity
    }

    if let identity = fromBundleIdentifier(mainBundle.bundleIdentifier) {
      return identity
    }

    if let appBundle = containingAppBundle(executablePath: executablePath),
      let identity = fromInfoPlist(at: appBundle)
    {
      return identity
    }

    return .official
  }

  public static func runtimePaths(homeDirectory: URL = FileManager.default.homeDirectoryForCurrentUser) -> RuntimePaths {
    current().runtimePaths(homeDirectory: homeDirectory)
  }

  public func runtimePaths(homeDirectory: URL) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: homeDirectory.appendingPathComponent(userContentDirectoryName, isDirectory: true),
      appSupportDirectory: homeDirectory.appendingPathComponent("Library/Application Support/\(appSupportDirectoryName)", isDirectory: true),
      logDirectory: homeDirectory.appendingPathComponent("Library/Logs/\(logDirectoryName)", isDirectory: true),
      cacheDirectory: homeDirectory.appendingPathComponent("Library/Caches/\(cacheDirectoryName)", isDirectory: true),
      preferencesPath: homeDirectory.appendingPathComponent("Library/Preferences/\(preferencesFileName)").path,
      identity: self
    )
  }

  public static func currentExecutableURL() -> URL? {
    var size = UInt32(0)
    _NSGetExecutablePath(nil, &size)
    var buffer = [CChar](repeating: 0, count: Int(size))
    guard _NSGetExecutablePath(&buffer, &size) == 0 else { return nil }
    let pathBytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    let path = String(decoding: pathBytes, as: UTF8.self)
    return URL(fileURLWithPath: path).resolvingSymlinksInPath()
  }

  public static func containingAppBundle(executablePath: String? = CommandLine.arguments.first) -> URL? {
    guard let executablePath, !executablePath.isEmpty else { return nil }
    var candidate = URL(fileURLWithPath: executablePath)
      .standardizedFileURL
      .resolvingSymlinksInPath()
    while candidate.path != "/" {
      if candidate.pathExtension == "app" {
        return candidate
      }
      candidate.deleteLastPathComponent()
    }
    return nil
  }

  private static func fromInfoPlist(at appBundle: URL) -> OneContextAppIdentity? {
    let plist = appBundle.appendingPathComponent("Contents/Info.plist")
    guard let data = try? Data(contentsOf: plist),
      let info = try? PropertyListSerialization.propertyList(from: data, options: [], format: nil) as? [String: Any]
    else {
      return nil
    }
    if let identity = from(info[infoPlistKey] as? String) {
      return identity
    }
    return fromBundleIdentifier(info["CFBundleIdentifier"] as? String)
  }

  private static func fromBundleIdentifier(_ bundleIdentifier: String?) -> OneContextAppIdentity? {
    switch bundleIdentifier {
    case dev.bundleIdentifier:
      return .dev
    case official.bundleIdentifier:
      return .official
    default:
      return nil
    }
  }
}
