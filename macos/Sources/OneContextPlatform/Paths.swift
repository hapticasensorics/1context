import Foundation
import Darwin

public struct RuntimePaths {
  public let identity: OneContextAppIdentity
  public let userContentDirectory: URL
  public let userWikiDirectory: URL
  public let userWikiSourceDirectory: URL
  public let userWikiSiteDirectory: URL
  public let contextEngineDirectory: URL
  public let contextEngineIndexesDirectory: URL
  public let appSupportDirectory: URL
  public let appSupportIndexesDirectory: URL
  public let appSupportSetupDirectory: URL
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
    self.init(
      userContentDirectory: userContentDirectory,
      appSupportDirectory: appSupportDirectory,
      logDirectory: logDirectory,
      cacheDirectory: cacheDirectory,
      socketPath: socketPath,
      logPath: logPath,
      preferencesPath: preferencesPath,
      identity: .official
    )
  }

  public init(
    userContentDirectory: URL,
    appSupportDirectory: URL,
    logDirectory: URL,
    cacheDirectory: URL,
    socketPath: String? = nil,
    logPath: String? = nil,
    preferencesPath: String? = nil,
    identity: OneContextAppIdentity
  ) {
    self.identity = identity
    let runDirectory = appSupportDirectory.appendingPathComponent("run", isDirectory: true)
    self.userContentDirectory = userContentDirectory
    self.userWikiDirectory = userContentDirectory.appendingPathComponent("user-wiki", isDirectory: true)
    self.userWikiSourceDirectory = self.userWikiDirectory.appendingPathComponent("source", isDirectory: true)
    self.userWikiSiteDirectory = self.userWikiDirectory.appendingPathComponent("site", isDirectory: true)
    self.contextEngineDirectory = userContentDirectory.appendingPathComponent("context-engine", isDirectory: true)
    self.contextEngineIndexesDirectory = self.contextEngineDirectory.appendingPathComponent("indexes", isDirectory: true)
    self.appSupportDirectory = appSupportDirectory
    self.appSupportIndexesDirectory = appSupportDirectory.appendingPathComponent("indexes", isDirectory: true)
    self.appSupportSetupDirectory = appSupportDirectory.appendingPathComponent("setup", isDirectory: true)
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
        .appendingPathComponent("Library/Preferences/\(identity.preferencesFileName)").path
  }

  public static func current() -> RuntimePaths {
    let identity = OneContextAppIdentity.current()
    #if DEBUG
    if let runtimeHome = ProcessInfo.processInfo.environment["ONECONTEXT_DEV_RUNTIME_HOME"],
       !runtimeHome.isEmpty {
      let root = URL(fileURLWithPath: runtimeHome, isDirectory: true)
      let socketPath = ProcessInfo.processInfo.environment["ONECONTEXT_DEV_SOCKET_PATH"]
        .flatMap { $0.isEmpty ? nil : $0 }
      return RuntimePaths(
        userContentDirectory: root.appendingPathComponent(identity.userContentDirectoryName, isDirectory: true),
        appSupportDirectory: root.appendingPathComponent("Library/Application Support/\(identity.appSupportDirectoryName)", isDirectory: true),
        logDirectory: root.appendingPathComponent("Library/Logs/\(identity.logDirectoryName)", isDirectory: true),
        cacheDirectory: root.appendingPathComponent("Library/Caches/\(identity.cacheDirectoryName)", isDirectory: true),
        socketPath: socketPath,
        preferencesPath: root.appendingPathComponent("Library/Preferences/\(identity.preferencesFileName)").path,
        identity: identity
      )
    }
    #endif

    return identity.runtimePaths(homeDirectory: FileManager.default.homeDirectoryForCurrentUser)
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
      paths.userWikiDirectory,
      paths.userWikiSourceDirectory,
      paths.userWikiSiteDirectory,
      paths.contextEngineDirectory,
      paths.contextEngineIndexesDirectory,
      paths.appSupportDirectory,
      paths.appSupportIndexesDirectory,
      paths.appSupportSetupDirectory,
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
