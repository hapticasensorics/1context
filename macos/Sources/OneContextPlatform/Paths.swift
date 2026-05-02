import Foundation
import Darwin

public struct RuntimePaths {
  public let userContentDirectory: URL
  public let appSupportDirectory: URL
  public let configPath: String
  public let runDirectory: URL
  public let desiredStatePath: String
  public let socketPath: String
  public let pidPath: String
  public let logDirectory: URL
  public let logPath: String
  public let cacheDirectory: URL
  public let renderCacheDirectory: URL
  public let downloadCacheDirectory: URL
  public let preferencesPath: String

  public static func current(environment: [String: String] = ProcessInfo.processInfo.environment) -> RuntimePaths {
    let home = FileManager.default.homeDirectoryForCurrentUser
    let userContentDirectory = URL(
      fileURLWithPath: environment["ONECONTEXT_USER_CONTENT_DIR"]
        ?? home.appendingPathComponent("1Context").path,
      isDirectory: true
    )
    let appSupport = URL(
      fileURLWithPath: environment["ONECONTEXT_APP_SUPPORT_DIR"]
        ?? home.appendingPathComponent("Library/Application Support/1Context").path,
      isDirectory: true
    )
    let cacheDirectory = URL(
      fileURLWithPath: environment["ONECONTEXT_CACHE_DIR"]
        ?? home.appendingPathComponent("Library/Caches/1Context").path,
      isDirectory: true
    )
    let runDirectory = appSupport.appendingPathComponent("run", isDirectory: true)
    let logDirectory = URL(
      fileURLWithPath: environment["ONECONTEXT_LOG_DIR"]
        ?? home.appendingPathComponent("Library/Logs/1Context").path,
      isDirectory: true
    )

    return RuntimePaths(
      userContentDirectory: userContentDirectory,
      appSupportDirectory: appSupport,
      configPath: appSupport.appendingPathComponent("config.json").path,
      runDirectory: runDirectory,
      desiredStatePath: appSupport.appendingPathComponent("desired-state").path,
      socketPath: environment["ONECONTEXT_SOCKET_PATH"]
        ?? runDirectory.appendingPathComponent("1context.sock").path,
      pidPath: runDirectory.appendingPathComponent("1contextd.pid").path,
      logDirectory: logDirectory,
      logPath: environment["ONECONTEXT_LOG_PATH"]
        ?? logDirectory.appendingPathComponent("1contextd.log").path,
      cacheDirectory: cacheDirectory,
      renderCacheDirectory: cacheDirectory.appendingPathComponent("render-cache", isDirectory: true),
      downloadCacheDirectory: cacheDirectory.appendingPathComponent("download-cache", isDirectory: true),
      preferencesPath: environment["ONECONTEXT_PREFERENCES_PATH"]
        ?? home.appendingPathComponent("Library/Preferences/com.haptica.1context.plist").path
    )
  }
}

public enum RuntimePermissions {
  public static let privateDirectoryMode: mode_t = 0o700
  public static let privateFileMode: mode_t = 0o600

  public static func ensurePrivateDirectory(_ url: URL) throws {
    try FileManager.default.createDirectory(at: url, withIntermediateDirectories: true)
    chmod(url.path, privateDirectoryMode)
  }

  public static func ensurePrivateFile(_ path: String) {
    if FileManager.default.fileExists(atPath: path) {
      chmod(path, privateFileMode)
    }
  }

  public static func writePrivateString(_ string: String, toFile path: String) throws {
    try string.write(toFile: path, atomically: true, encoding: .utf8)
    chmod(path, privateFileMode)
  }

  public static func writePrivateData(_ data: Data, to url: URL) throws {
    try data.write(to: url, options: .atomic)
    chmod(url.path, privateFileMode)
  }

  public static func repairRuntimePaths(_ paths: RuntimePaths) {
    for directory in [
      paths.userContentDirectory,
      paths.appSupportDirectory,
      paths.runDirectory,
      paths.logDirectory,
      paths.cacheDirectory,
      paths.renderCacheDirectory,
      paths.downloadCacheDirectory
    ] {
      if FileManager.default.fileExists(atPath: directory.path) {
        chmod(directory.path, privateDirectoryMode)
      }
    }

    for file in [
      paths.configPath,
      paths.desiredStatePath,
      paths.socketPath,
      paths.pidPath,
      paths.logPath,
      paths.preferencesPath
    ] {
      ensurePrivateFile(file)
    }
  }
}

public func plistEscape(_ value: String) -> String {
  value
    .replacingOccurrences(of: "&", with: "&amp;")
    .replacingOccurrences(of: "<", with: "&lt;")
    .replacingOccurrences(of: ">", with: "&gt;")
    .replacingOccurrences(of: "\"", with: "&quot;")
    .replacingOccurrences(of: "'", with: "&apos;")
}
