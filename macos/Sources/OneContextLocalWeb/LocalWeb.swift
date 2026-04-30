import Darwin
import Foundation
import OneContextPlatform

public struct LocalWebSnapshot: Codable, Equatable, Sendable {
  public var running: Bool
  public var url: String
  public var pid: Int32?
  public var route: String
  public var health: String
  public var lastError: String?

  public init(
    running: Bool,
    url: String = LocalWebDefaults.defaultWikiURL,
    pid: Int32? = nil,
    route: String = LocalWebDefaults.wikiRoute,
    health: String,
    lastError: String? = nil
  ) {
    self.running = running
    self.url = url
    self.pid = pid
    self.route = route
    self.health = health
    self.lastError = lastError
  }
}

public struct LocalWebDiagnostics: Codable, Equatable, Sendable {
  public var snapshot: LocalWebSnapshot
  public var apiURL: String
  public var apiHealth: String
  public var apiPort: Int
  public var apiStatePath: String
  public var caddyExecutable: String
  public var caddyExecutableExists: Bool
  public var caddyExecutableIsExecutable: Bool
  public var caddyExecutableIsBundled: Bool
  public var bundledCaddyPath: String
  public var bundledCaddyVersionPath: String
  public var bundledCaddyVersion: String
  public var caddyfilePath: String
  public var statePath: String
  public var pidPath: String
  public var logPath: String
  public var currentSitePath: String
  public var nextSitePath: String
  public var previousSitePath: String
  public var currentSiteHasIndex: Bool
  public var currentSiteHasTheme: Bool
  public var currentSiteHasEnhanceJS: Bool
  public var currentSiteHasHealth: Bool
}

public enum LocalWebDefaults {
  public static let wikiHost = "wiki.1context.localhost"
  public static let bindHost = "127.0.0.1"
  public static let wikiPort = 17319
  public static let wikiAPIPort = 17320
  public static let wikiRoute = "/your-context"
  public static let defaultWikiURL = "http://\(wikiHost):\(wikiPort)\(wikiRoute)"
}

public struct LocalWebPaths: Sendable {
  public let directory: URL
  public let caddyDirectory: URL
  public let caddyfile: URL
  public let stateFile: URL
  public let wikiBrowserStateFile: URL
  public let pidFile: URL
  public let logFile: URL
  public let wikiSiteDirectory: URL
  public let wikiCurrent: URL
  public let wikiNext: URL
  public let wikiPrevious: URL

  public init(runtimePaths: RuntimePaths = .current()) {
    self.directory = runtimePaths.appSupportDirectory.appendingPathComponent("local-web", isDirectory: true)
    self.caddyDirectory = directory.appendingPathComponent("caddy", isDirectory: true)
    self.caddyfile = caddyDirectory.appendingPathComponent("Caddyfile")
    self.stateFile = caddyDirectory.appendingPathComponent("state.json")
    self.wikiBrowserStateFile = directory.appendingPathComponent("wiki-browser-state.json")
    self.pidFile = runtimePaths.runDirectory.appendingPathComponent("local-web-caddy.pid")
    self.logFile = runtimePaths.logDirectory.appendingPathComponent("local-web-caddy.log")
    self.wikiSiteDirectory = runtimePaths.appSupportDirectory.appendingPathComponent("wiki-site", isDirectory: true)
    self.wikiCurrent = wikiSiteDirectory.appendingPathComponent("current", isDirectory: true)
    self.wikiNext = wikiSiteDirectory.appendingPathComponent("next", isDirectory: true)
    self.wikiPrevious = wikiSiteDirectory.appendingPathComponent("previous", isDirectory: true)
  }
}

public struct CaddyConfig: Equatable, Sendable {
  public var siteRoot: URL
  public var logFile: URL
  public var host: String
  public var bindHost: String
  public var port: Int
  public var apiBindHost: String
  public var apiPort: Int

  public init(
    siteRoot: URL,
    logFile: URL,
    host: String = LocalWebDefaults.wikiHost,
    bindHost: String = LocalWebDefaults.bindHost,
    port: Int = LocalWebDefaults.wikiPort,
    apiBindHost: String = LocalWebDefaults.bindHost,
    apiPort: Int = LocalWebDefaults.wikiAPIPort
  ) {
    self.siteRoot = siteRoot
    self.logFile = logFile
    self.host = host
    self.bindHost = bindHost
    self.port = port
    self.apiBindHost = apiBindHost
    self.apiPort = apiPort
  }

  public var url: String {
    "http://\(host):\(port)\(LocalWebDefaults.wikiRoute)"
  }

  public var healthURL: URL {
    URL(string: "http://\(bindHost):\(port)/__1context/health")!
  }

  public func caddyfileText() -> String {
    """
    {
      admin off
      auto_https off
    }

    http://\(host):\(port), http://\(bindHost):\(port) {
      bind \(bindHost)
      root * "\(escape(siteRoot.path))"

      log {
        output file "\(escape(logFile.path))" {
          roll_size 1MiB
          roll_keep 2
        }
      }

      encode zstd gzip

      header {
        X-Content-Type-Options nosniff
        Referrer-Policy no-referrer
        Cache-Control no-store
      }

      route {
        @wikiDynamicApi path /api/wiki/health /api/wiki/search /api/wiki/state /api/wiki/bookmarks /api/wiki/chat/config /api/wiki/chat/provider /api/wiki/chat/reset /api/wiki/chat
        handle @wikiDynamicApi {
          reverse_proxy \(apiBindHost):\(apiPort)
        }

        rewrite /api/wiki/site /api/wiki/site.json
        rewrite /api/wiki/pages /api/wiki/pages.json
        rewrite /api/wiki/stats /api/wiki/stats.json

        try_files {path} {path}.html {path}/index.html /index.html
        file_server
      }
    }
    """
  }

  private func escape(_ value: String) -> String {
    value
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
  }
}

private struct CaddyState: Codable {
  var schemaVersion: Int = 1
  var pid: Int32
  var url: String
  var caddyExecutable: String
  var startedAt: Date

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case pid
    case url
    case caddyExecutable = "caddy_executable"
    case startedAt = "started_at"
  }
}

public final class CaddyManager: @unchecked Sendable {
  private let runtimePaths: RuntimePaths
  private let paths: LocalWebPaths
  private let environment: [String: String]
  private let fileManager: FileManager
  private let host: String
  private let port: Int
  private let apiBindHost: String
  private let apiPort: Int
  private let lifecycleLock = NSLock()

  public init(
    runtimePaths: RuntimePaths = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default
  ) {
    self.runtimePaths = runtimePaths
    self.paths = LocalWebPaths(runtimePaths: runtimePaths)
    self.environment = environment
    self.fileManager = fileManager
    self.host = environment["ONECONTEXT_WIKI_HOST"] ?? LocalWebDefaults.wikiHost
    self.port = Int(environment["ONECONTEXT_WIKI_PORT"] ?? "") ?? LocalWebDefaults.wikiPort
    self.apiBindHost = environment["ONECONTEXT_WIKI_API_BIND_HOST"] ?? LocalWebDefaults.bindHost
    self.apiPort = Int(environment["ONECONTEXT_WIKI_API_PORT"] ?? "") ?? LocalWebDefaults.wikiAPIPort
  }

  public var pidPath: String {
    paths.pidFile.path
  }

  public var wikiCurrentDirectory: URL {
    paths.wikiCurrent
  }

  public func diagnostics() -> LocalWebDiagnostics {
    let caddy = (try? caddyExecutable()) ?? URL(fileURLWithPath: "")
    let bundled = bundledCaddyURL()
    return LocalWebDiagnostics(
      snapshot: status(),
      apiURL: WikiLocalAPIConfig(environment: environment).healthURL.absoluteString,
      apiHealth: WikiLocalAPIProbe.health(environment: environment),
      apiPort: WikiLocalAPIConfig(environment: environment).port,
      apiStatePath: paths.wikiBrowserStateFile.path,
      caddyExecutable: caddy.path,
      caddyExecutableExists: !caddy.path.isEmpty && fileManager.fileExists(atPath: caddy.path),
      caddyExecutableIsExecutable: !caddy.path.isEmpty && fileManager.isExecutableFile(atPath: caddy.path),
      caddyExecutableIsBundled: !caddy.path.isEmpty && caddy.standardizedFileURL == bundled.standardizedFileURL,
      bundledCaddyPath: bundled.path,
      bundledCaddyVersionPath: bundledCaddyVersionURL().path,
      bundledCaddyVersion: readTrimmed(bundledCaddyVersionURL()) ?? "missing",
      caddyfilePath: paths.caddyfile.path,
      statePath: paths.stateFile.path,
      pidPath: paths.pidFile.path,
      logPath: paths.logFile.path,
      currentSitePath: paths.wikiCurrent.path,
      nextSitePath: paths.wikiNext.path,
      previousSitePath: paths.wikiPrevious.path,
      currentSiteHasIndex: fileManager.fileExists(atPath: paths.wikiCurrent.appendingPathComponent("index.html").path),
      currentSiteHasTheme: fileManager.fileExists(atPath: paths.wikiCurrent.appendingPathComponent("assets/theme.css").path),
      currentSiteHasEnhanceJS: fileManager.fileExists(atPath: paths.wikiCurrent.appendingPathComponent("assets/enhance.js").path),
      currentSiteHasHealth: fileManager.fileExists(atPath: paths.wikiCurrent.appendingPathComponent("__1context/health").path)
    )
  }

  public func ensurePlaceholderSite() throws {
    try RuntimePermissions.ensurePrivateDirectory(paths.wikiCurrent)
    try RuntimePermissions.ensurePrivateDirectory(paths.wikiCurrent.appendingPathComponent("__1context", isDirectory: true))
    try RuntimePermissions.ensurePrivateDirectory(paths.wikiCurrent.appendingPathComponent("api/wiki/chat", isDirectory: true))
    try copyBundledThemeAssetsIfAvailable(to: paths.wikiCurrent)
    try writeString(staticJSON(["status": "ok", "service": "1context-local-web"]), to: paths.wikiCurrent.appendingPathComponent("__1context/health"))
    try writeString(staticJSON(["query": "", "matches": [], "pages": []]), to: paths.wikiCurrent.appendingPathComponent("api/wiki/search.json"))
    try writeString(staticJSON(["bookmarks": []]), to: paths.wikiCurrent.appendingPathComponent("api/wiki/bookmarks.json"))
    try writeString(staticJSON([:]), to: paths.wikiCurrent.appendingPathComponent("api/wiki/state.json"))
    try writeString(staticJSON([:]), to: paths.wikiCurrent.appendingPathComponent("api/wiki/chat/config.json"))
    guard !fileManager.fileExists(atPath: paths.wikiCurrent.appendingPathComponent("index.html").path) else { return }
    if try copyBundledSeedWikiIfAvailable(to: paths.wikiCurrent) {
      return
    }
    try RuntimePermissions.ensurePrivateDirectory(paths.wikiCurrent.appendingPathComponent("your-context", isDirectory: true))
    try RuntimePermissions.ensurePrivateDirectory(paths.wikiCurrent.appendingPathComponent("for-you", isDirectory: true))
    try writeString(placeholderHTML(), to: paths.wikiCurrent.appendingPathComponent("index.html"))
    try writeString(placeholderHTML(), to: paths.wikiCurrent.appendingPathComponent("your-context.html"))
    try writeString(placeholderHTML(), to: paths.wikiCurrent.appendingPathComponent("your-context/index.html"))
    try writeString(placeholderHTML(), to: paths.wikiCurrent.appendingPathComponent("for-you.html"))
    try writeString(placeholderHTML(), to: paths.wikiCurrent.appendingPathComponent("for-you/index.html"))
  }

  @discardableResult
  public func start() throws -> LocalWebSnapshot {
    lifecycleLock.lock()
    defer { lifecycleLock.unlock() }

    _ = LegacyPythonWikiServerMigration.run(runtimePaths: runtimePaths, fileManager: fileManager)
    try ensurePlaceholderSite()
    let caddy = try caddyExecutable()
    let config = caddyConfig()
    try RuntimePermissions.ensurePrivateDirectory(paths.directory)
    try RuntimePermissions.ensurePrivateDirectory(paths.caddyDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.caddyDirectory.appendingPathComponent("home", isDirectory: true))
    try RuntimePermissions.ensurePrivateDirectory(paths.caddyDirectory.appendingPathComponent("config", isDirectory: true))
    try RuntimePermissions.ensurePrivateDirectory(paths.caddyDirectory.appendingPathComponent("data", isDirectory: true))
    try RuntimePermissions.ensurePrivateDirectory(runtimePaths.runDirectory)
    try RuntimePermissions.ensurePrivateDirectory(runtimePaths.logDirectory)
    try RuntimePermissions.writePrivateString(config.caddyfileText() + "\n", toFile: paths.caddyfile.path)

    let current = status()
    if current.running {
      return current
    }
    if let pid = current.pid, processIsAlive(pid) {
      stopUnlocked()
    }

    let process = Process()
    process.executableURL = caddy
    process.arguments = ["run", "--config", paths.caddyfile.path, "--adapter", "caddyfile"]
    process.currentDirectoryURL = paths.caddyDirectory
    process.environment = caddyProcessEnvironment()
    process.standardInput = FileHandle.nullDevice
    let logHandle = try appendLogHandle()
    process.standardOutput = logHandle
    process.standardError = logHandle
    try process.run()

    let state = CaddyState(
      pid: process.processIdentifier,
      url: config.url,
      caddyExecutable: caddy.path,
      startedAt: Date()
    )
    try writeState(state)
    try RuntimePermissions.writePrivateString("\(process.processIdentifier)\n", toFile: paths.pidFile.path)
    return try waitForHealthy(state: state)
  }

  public func status() -> LocalWebSnapshot {
    let config = caddyConfig()
    if let state = readState() {
      if processIsAlive(state.pid), healthOK(config.healthURL) {
        return LocalWebSnapshot(running: true, url: state.url, pid: state.pid, health: "OK")
      }
      if !processIsAlive(state.pid) {
        return LocalWebSnapshot(running: false, url: state.url, pid: state.pid, health: "stale")
      }
      return LocalWebSnapshot(running: false, url: state.url, pid: state.pid, health: "no response")
    }
    if healthOK(config.healthURL) {
      return LocalWebSnapshot(running: true, url: config.url, health: "OK")
    }
    return LocalWebSnapshot(running: false, url: config.url, health: "not running")
  }

  public func stop() {
    lifecycleLock.lock()
    defer { lifecycleLock.unlock() }

    stopUnlocked()
  }

  private func stopUnlocked() {
    if let state = readState(), processIsAlive(state.pid) {
      kill(state.pid, SIGTERM)
    } else if let text = try? String(contentsOf: paths.pidFile, encoding: .utf8),
      let pid = Int32(text.trimmingCharacters(in: .whitespacesAndNewlines)),
      processIsAlive(pid)
    {
      kill(pid, SIGTERM)
    }
    try? fileManager.removeItem(at: paths.stateFile)
    try? fileManager.removeItem(at: paths.pidFile)
  }

  private func waitForHealthy(state: CaddyState, timeout: TimeInterval = 10) throws -> LocalWebSnapshot {
    let config = caddyConfig()
    let deadline = Date().addingTimeInterval(timeout)
    repeat {
      if processIsAlive(state.pid), healthOK(config.healthURL) {
        return LocalWebSnapshot(running: true, url: state.url, pid: state.pid, health: "OK")
      }
      Thread.sleep(forTimeInterval: 0.15)
    } while Date() < deadline
    throw LocalWebError.timedOut("Caddy did not become healthy")
  }

  private func caddyExecutable() throws -> URL {
    for candidate in caddyCandidates() where fileManager.isExecutableFile(atPath: candidate.path) {
      return candidate
    }
    throw LocalWebError.caddyMissing
  }

  private func caddyConfig() -> CaddyConfig {
    CaddyConfig(
      siteRoot: paths.wikiCurrent,
      logFile: paths.logFile,
      host: host,
      port: port,
      apiBindHost: apiBindHost,
      apiPort: apiPort
    )
  }

  private func caddyCandidates() -> [URL] {
    var candidates: [URL] = []
    if let override = environment["ONECONTEXT_CADDY_PATH"], !override.isEmpty {
      candidates.append(URL(fileURLWithPath: override))
    }
    if let executableDirectory = currentExecutableURL()?.deletingLastPathComponent() {
      candidates.append(bundledCaddyURL(executableDirectory: executableDirectory))
    }
    candidates.append(URL(fileURLWithPath: "/opt/homebrew/bin/caddy"))
    candidates.append(URL(fileURLWithPath: "/usr/local/bin/caddy"))
    return candidates
  }

  private func bundledCaddyURL(executableDirectory: URL? = nil) -> URL {
    let directory = executableDirectory ?? currentExecutableURL()?.deletingLastPathComponent()
    return (directory ?? URL(fileURLWithPath: ""))
      .deletingLastPathComponent()
      .appendingPathComponent("Resources/local-web/caddy/caddy")
  }

  private func bundledCaddyVersionURL() -> URL {
    bundledCaddyURL().deletingLastPathComponent().appendingPathComponent("caddy.version")
  }

  private func caddyProcessEnvironment() -> [String: String] {
    var env = ProcessInfo.processInfo.environment.merging(environment) { _, new in new }
    env["HOME"] = paths.caddyDirectory.appendingPathComponent("home", isDirectory: true).path
    env["XDG_CONFIG_HOME"] = paths.caddyDirectory.appendingPathComponent("config", isDirectory: true).path
    env["XDG_DATA_HOME"] = paths.caddyDirectory.appendingPathComponent("data", isDirectory: true).path
    return env
  }

  private func healthOK(_ url: URL) -> Bool {
    var request = URLRequest(url: url)
    request.timeoutInterval = 0.75
    let semaphore = DispatchSemaphore(value: 0)
    nonisolated(unsafe) var ok = false
    let task = URLSession.shared.dataTask(with: request) { data, _, _ in
      defer { semaphore.signal() }
      guard let data,
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
      else {
        return
      }
      ok = object["status"] as? String == "ok"
    }
    task.resume()
    if semaphore.wait(timeout: .now() + 1) == .timedOut {
      task.cancel()
      return false
    }
    return ok
  }

  private func readState() -> CaddyState? {
    guard let data = try? Data(contentsOf: paths.stateFile) else { return nil }
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return try? decoder.decode(CaddyState.self, from: data)
  }

  private func writeState(_ state: CaddyState) throws {
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try RuntimePermissions.writePrivateData(try encoder.encode(state), to: paths.stateFile)
  }

  private func appendLogHandle() throws -> FileHandle {
    try RuntimePermissions.ensurePrivateDirectory(paths.logFile.deletingLastPathComponent())
    if !fileManager.fileExists(atPath: paths.logFile.path) {
      fileManager.createFile(atPath: paths.logFile.path, contents: nil)
      chmod(paths.logFile.path, RuntimePermissions.privateFileMode)
    }
    let handle = try FileHandle(forWritingTo: paths.logFile)
    try handle.seekToEnd()
    return handle
  }

  private func copyBundledThemeAssetsIfAvailable(to siteRoot: URL) throws {
    guard let theme = bundledThemeDirectory() else { return }
    let assets = siteRoot.appendingPathComponent("assets", isDirectory: true)
    try RuntimePermissions.ensurePrivateDirectory(assets)
    try copyIfPresent(theme.appendingPathComponent("css/theme.css"), to: assets.appendingPathComponent("theme.css"))
    try copyIfPresent(theme.appendingPathComponent("js/enhance.js"), to: assets.appendingPathComponent("enhance.js"))

    let assetSource = theme.appendingPathComponent("assets", isDirectory: true)
    guard let enumerator = fileManager.enumerator(at: assetSource, includingPropertiesForKeys: [.isRegularFileKey]) else {
      return
    }
    for case let source as URL in enumerator {
      guard (try? source.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true else { continue }
      let relative = relativePath(source, from: assetSource)
      try copyIfPresent(source, to: assets.appendingPathComponent(relative))
    }
  }

  private func copyBundledSeedWikiIfAvailable(to siteRoot: URL) throws -> Bool {
    guard let core = bundledMemoryCoreDirectory() else { return false }
    let seedGeneratedDirectories = [
      core.appendingPathComponent("wiki/menu/10-for-you/10-for-you/generated", isDirectory: true),
      core.appendingPathComponent("wiki/menu/10-for-you/20-your-context/generated", isDirectory: true),
      core.appendingPathComponent("wiki/menu/20-project/10-projects/generated", isDirectory: true),
      core.appendingPathComponent("wiki/menu/30-topics/10-topics/generated", isDirectory: true)
    ]
    guard seedGeneratedDirectories.contains(where: { fileManager.fileExists(atPath: $0.appendingPathComponent("your-context.html").path) }) else {
      return false
    }

    for generated in seedGeneratedDirectories where fileManager.fileExists(atPath: generated.path) {
      try copyGeneratedSeedFiles(from: generated, to: siteRoot)
    }
    try copyBundledThemeAssetsIfAvailable(to: siteRoot)
    try copySeedSiteJSON(from: core, to: siteRoot)
    try RuntimePermissions.ensurePrivateDirectory(siteRoot.appendingPathComponent("your-context", isDirectory: true))
    try RuntimePermissions.ensurePrivateDirectory(siteRoot.appendingPathComponent("for-you", isDirectory: true))
    try copyIfPresent(siteRoot.appendingPathComponent("your-context.html"), to: siteRoot.appendingPathComponent("index.html"))
    try copyIfPresent(siteRoot.appendingPathComponent("your-context.html"), to: siteRoot.appendingPathComponent("your-context/index.html"))
    if let forYou = forYouSeedHTML(in: siteRoot) {
      try copyIfPresent(forYou, to: siteRoot.appendingPathComponent("for-you.html"))
      try copyIfPresent(forYou, to: siteRoot.appendingPathComponent("for-you/index.html"))
      if let forYouTalk = forYouTalkSeedHTML(forYou: forYou, in: siteRoot) {
        try copyIfPresent(forYouTalk, to: siteRoot.appendingPathComponent("for-you.talk.html"))
      }
    }
    try writeString(staticJSON(["status": "ok", "service": "1context-local-web", "seed": true]), to: siteRoot.appendingPathComponent("__1context/health"))
    return fileManager.fileExists(atPath: siteRoot.appendingPathComponent("index.html").path)
  }

  private func forYouSeedHTML(in siteRoot: URL) -> URL? {
    guard let children = try? fileManager.contentsOfDirectory(at: siteRoot, includingPropertiesForKeys: [.isRegularFileKey]) else {
      return nil
    }
    return children
      .filter {
        $0.lastPathComponent.hasPrefix("for-you-")
          && $0.pathExtension == "html"
          && !$0.lastPathComponent.contains(".talk.")
      }
      .sorted { $0.lastPathComponent > $1.lastPathComponent }
      .first
  }

  private func forYouTalkSeedHTML(forYou: URL, in siteRoot: URL) -> URL? {
    let baseName = forYou.deletingPathExtension().lastPathComponent
    let talk = siteRoot.appendingPathComponent("\(baseName).talk.html")
    return fileManager.fileExists(atPath: talk.path) ? talk : nil
  }

  private func copyGeneratedSeedFiles(from generated: URL, to siteRoot: URL) throws {
    guard let enumerator = fileManager.enumerator(at: generated, includingPropertiesForKeys: [.isRegularFileKey]) else {
      return
    }
    for case let source as URL in enumerator {
      guard (try? source.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true else { continue }
      let name = source.lastPathComponent.lowercased()
      guard name != ".gitignore", name != "render-manifest.json", !name.contains(".private."), !name.contains(".internal.") else {
        continue
      }
      let relative = relativePath(source, from: generated)
      try copyIfPresent(source, to: siteRoot.appendingPathComponent(relative))
    }
  }

  private func copySeedSiteJSON(from core: URL, to siteRoot: URL) throws {
    let source = core.appendingPathComponent("wiki/generated", isDirectory: true)
    let api = siteRoot.appendingPathComponent("api/wiki", isDirectory: true)
    try RuntimePermissions.ensurePrivateDirectory(api)
    try copyIfPresent(source.appendingPathComponent("site-manifest.json"), to: siteRoot.appendingPathComponent("site-manifest.json"))
    try copyIfPresent(source.appendingPathComponent("content-index.json"), to: siteRoot.appendingPathComponent("content-index.json"))
    try copyIfPresent(source.appendingPathComponent("wiki-stats.json"), to: siteRoot.appendingPathComponent("wiki-stats.json"))
    try copyIfPresent(source.appendingPathComponent("site-manifest.json"), to: api.appendingPathComponent("site.json"))
    try copyIfPresent(source.appendingPathComponent("content-index.json"), to: api.appendingPathComponent("pages.json"))
    try copyIfPresent(source.appendingPathComponent("wiki-stats.json"), to: api.appendingPathComponent("stats.json"))
  }

  private func relativePath(_ url: URL, from root: URL) -> String {
    let rootPath = root.standardizedFileURL.path
    let path = url.standardizedFileURL.path
    guard path.hasPrefix(rootPath + "/") else {
      return url.lastPathComponent
    }
    return String(path.dropFirst(rootPath.count + 1))
  }

  private func copyIfPresent(_ source: URL, to destination: URL) throws {
    guard fileManager.fileExists(atPath: source.path) else { return }
    try RuntimePermissions.ensurePrivateDirectory(destination.deletingLastPathComponent())
    if source.standardizedFileURL == destination.standardizedFileURL {
      return
    }
    if fileManager.fileExists(atPath: destination.path) {
      try fileManager.removeItem(at: destination)
    }
    do {
      try fileManager.copyItem(at: source, to: destination)
    } catch {
      if fileManager.fileExists(atPath: destination.path) {
        try fileManager.removeItem(at: destination)
        try fileManager.copyItem(at: source, to: destination)
      } else {
        throw error
      }
    }
    chmod(destination.path, RuntimePermissions.privateFileMode)
  }

  private func bundledThemeDirectory() -> URL? {
    bundledMemoryCoreDirectory()?.appendingPathComponent("wiki-engine/theme", isDirectory: true)
  }

  private func bundledMemoryCoreDirectory() -> URL? {
    if let executableDirectory = currentExecutableURL()?.deletingLastPathComponent() {
      let releaseCore = executableDirectory
        .deletingLastPathComponent()
        .appendingPathComponent("Resources/memory-core", isDirectory: true)
      if fileManager.fileExists(atPath: releaseCore.appendingPathComponent("wiki-engine/theme/css/theme.css").path) {
        return releaseCore
      }

      var directory = executableDirectory
      for _ in 0..<8 {
        let candidate = directory.appendingPathComponent("memory-core", isDirectory: true)
        if fileManager.fileExists(atPath: candidate.appendingPathComponent("wiki-engine/theme/css/theme.css").path) {
          return candidate
        }
        directory.deleteLastPathComponent()
      }
    }
    return nil
  }

  private func writeString(_ value: String, to url: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    try RuntimePermissions.writePrivateString(value, toFile: url.path)
  }

  private func staticJSON(_ payload: [String: Any]) throws -> String {
    let data = try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys])
    return String(decoding: data, as: UTF8.self) + "\n"
  }

  private func readTrimmed(_ url: URL) -> String? {
    (try? String(contentsOf: url, encoding: .utf8))?.trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private func placeholderHTML() -> String {
    """
    <!doctype html>
    <html lang="en" data-theme="auto" data-article-width="s" data-font-size="m" data-border-radius="rounded" data-links-style="color" data-cover-image="show" data-article-style="full">
    <head>
      <meta charset="utf-8">
      <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover">
      <title>1Context Wiki</title>
      <link rel="icon" type="image/png" sizes="32x32" href="/assets/favicon-32.png">
      <link rel="icon" type="image/png" sizes="16x16" href="/assets/favicon-16.png">
      <link rel="apple-touch-icon" sizes="180x180" href="/assets/apple-touch-icon.png">
      <meta name="generator" content="1Context local web">
      <meta name="description" content="1Context local wiki">
      <script>
        (function() {
          try {
            var t = localStorage.getItem('opctx-theme');
            if (t === 'light' || t === 'dark') {
              document.documentElement.dataset.theme = t;
            }
          } catch (e) {}
        })();
      </script>
      <link rel="stylesheet" href="/assets/theme.css">
      <script type="module" src="/assets/enhance.js" defer></script>
    </head>
    <body>
      <div class="opctx-visibility-bar" data-tier="private" aria-label="Private - only you"></div>
      <div class="opctx-progress-bar" aria-hidden="true"></div>

      <header class="opctx-header">
        <div class="opctx-header-brand">
          <button type="button"
                  class="opctx-brand-menu-toggle"
                  aria-haspopup="menu"
                  aria-expanded="false"
                  aria-controls="opctx-brand-menu"
                  aria-label="Open 1Context navigation menu"
                  data-home-href="/">
            <span class="opctx-header-logo">1Context</span>
          </button>
          <div id="opctx-brand-menu" class="opctx-brand-menu" role="menu" hidden>
            <div class="opctx-brand-menu-group" role="group" aria-label="For You">
              <div class="opctx-brand-menu-heading">For You</div>
              <ul class="opctx-brand-menu-list">
                <li><a href="/for-you" role="menuitem"><span class="opctx-brand-menu-label">For You</span><span class="opctx-brand-menu-sub">Rolling local memory surface</span></a></li>
                <li><a href="/your-context" role="menuitem"><span class="opctx-brand-menu-label">Your Context</span><span class="opctx-brand-menu-sub">Working style and durable preferences</span></a></li>
              </ul>
            </div>
            <div class="opctx-brand-menu-group" role="group" aria-label="Project">
              <div class="opctx-brand-menu-heading">Project</div>
              <ul class="opctx-brand-menu-list">
                <li><a href="/projects" role="menuitem"><span class="opctx-brand-menu-label">Projects</span><span class="opctx-brand-menu-sub">Active, paused, completed, archived</span></a></li>
              </ul>
            </div>
            <div class="opctx-brand-menu-group" role="group" aria-label="Topics">
              <div class="opctx-brand-menu-heading">Topics</div>
              <ul class="opctx-brand-menu-list">
                <li><a href="/topics" role="menuitem"><span class="opctx-brand-menu-label">Topics</span><span class="opctx-brand-menu-sub">Named subjects and concept pages</span></a></li>
              </ul>
            </div>
          </div>
        </div>
        <div class="opctx-header-search">
          <input type="search" placeholder="Search pages, books, tags..." aria-label="Search">
        </div>
        <div class="opctx-header-actions">
          <span class="opctx-tier-badge" data-tier="private" title="Only you">Private</span>
        </div>
      </header>

      <div class="opctx-layout">
        <nav class="opctx-toc" aria-label="Table of contents">
          <ol>
            <li><a href="#wiki">Wiki</a></li>
            <li><a href="#refresh">Refresh</a></li>
          </ol>
        </nav>
        <main class="opctx-main">
          <article class="opctx-article">
            <header class="opctx-article-header">
              <p class="opctx-kicker">Local web</p>
              <h1 id="wiki">1Context Wiki</h1>
              <p class="opctx-subtitle">The local web shell is ready. The first refresh publishes the latest rendered memory pages into this same 1Context interface.</p>
            </header>
            <section>
              <h2 id="refresh">Refresh</h2>
              <p>Run <code>1context wiki refresh</code> to publish the latest rendered wiki artifacts.</p>
            </section>
          </article>
        </main>
      </div>
    </body>
    </html>
    """
  }

  private func processIsAlive(_ pid: Int32) -> Bool {
    pid > 0 && kill(pid, 0) == 0
  }

  private func currentExecutableURL() -> URL? {
    var size = UInt32(0)
    _NSGetExecutablePath(nil, &size)
    var buffer = [CChar](repeating: 0, count: Int(size))
    guard _NSGetExecutablePath(&buffer, &size) == 0 else { return nil }
    let pathBytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    return URL(fileURLWithPath: String(decoding: pathBytes, as: UTF8.self)).resolvingSymlinksInPath()
  }
}

public enum LocalWebError: Error, LocalizedError, Equatable {
  case caddyMissing
  case timedOut(String)

  public var errorDescription: String? {
    switch self {
    case .caddyMissing:
      return "Bundled Caddy web server was not found"
    case .timedOut(let message):
      return message
    }
  }
}
