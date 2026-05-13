import Foundation
import Darwin

public struct RuntimePaths {
  public let userContentDirectory: URL
  public let appSupportDirectory: URL
  public let configPath: String
  public let runDirectory: URL
  public let socketPath: String
  public let pidPath: String
  public let logDirectory: URL
  public let logPath: String
  public let cacheDirectory: URL
  public let renderCacheDirectory: URL
  public let downloadCacheDirectory: URL
  public let preferencesPath: String

  public init(
    userContentDirectory: URL,
    appSupportDirectory: URL,
    logDirectory: URL,
    cacheDirectory: URL,
    socketPath: String? = nil,
    logPath: String? = nil,
    preferencesPath: String? = nil
  ) {
    let runDirectory = appSupportDirectory.appendingPathComponent("run", isDirectory: true)
    self.userContentDirectory = userContentDirectory
    self.appSupportDirectory = appSupportDirectory
    self.configPath = appSupportDirectory.appendingPathComponent("config.json").path
    self.runDirectory = runDirectory
    self.socketPath = socketPath ?? runDirectory.appendingPathComponent("1context.sock").path
    self.pidPath = runDirectory.appendingPathComponent("1contextd.pid").path
    self.logDirectory = logDirectory
    self.logPath = logPath ?? logDirectory.appendingPathComponent("1contextd.log").path
    self.cacheDirectory = cacheDirectory
    self.renderCacheDirectory = cacheDirectory.appendingPathComponent("render-cache", isDirectory: true)
    self.downloadCacheDirectory = cacheDirectory.appendingPathComponent("download-cache", isDirectory: true)
    self.preferencesPath = preferencesPath
      ?? FileManager.default.homeDirectoryForCurrentUser
        .appendingPathComponent("Library/Preferences/com.haptica.1context.plist").path
  }

  public static func current() -> RuntimePaths {
    let home = FileManager.default.homeDirectoryForCurrentUser
    return RuntimePaths(
      userContentDirectory: home.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: home.appendingPathComponent("Library/Application Support/1Context", isDirectory: true),
      logDirectory: home.appendingPathComponent("Library/Logs/1Context", isDirectory: true),
      cacheDirectory: home.appendingPathComponent("Library/Caches/1Context", isDirectory: true)
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
