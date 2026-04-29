import Foundation
import Darwin

public let oneContextVersion = "0.1.18"
public let oneContextGitHubURL = URL(string: "https://github.com/hapticasensorics/1context")!
public let oneContextLatestReleaseURL = URL(string: "https://api.github.com/repos/hapticasensorics/1context/releases/latest")!
public let oneContextHomebrewUpdateCommand = "brew update && brew upgrade --cask hapticasensorics/tap/1context"
public let oneContextUpdateCheckInterval: TimeInterval = 24 * 60 * 60

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
      pidPath: runDirectory.appendingPathComponent("onecontextd.pid").path,
      logDirectory: logDirectory,
      logPath: environment["ONECONTEXT_LOG_PATH"]
        ?? logDirectory.appendingPathComponent("onecontextd.log").path,
      cacheDirectory: cacheDirectory,
      renderCacheDirectory: cacheDirectory.appendingPathComponent("render-cache", isDirectory: true),
      downloadCacheDirectory: cacheDirectory.appendingPathComponent("download-cache", isDirectory: true),
      preferencesPath: environment["ONECONTEXT_PREFERENCES_PATH"]
        ?? home.appendingPathComponent("Library/Preferences/com.haptica.1context.plist").path
    )
  }
}

public struct UpdateStatePaths {
  public let directory: URL
  public let file: URL

  public static func current(environment: [String: String] = ProcessInfo.processInfo.environment) -> UpdateStatePaths {
    let runtimePaths = RuntimePaths.current(environment: environment)
    let directory = URL(
      fileURLWithPath: environment["ONECONTEXT_UPDATE_STATE_DIR"]
        ?? runtimePaths.appSupportDirectory.appendingPathComponent("update").path,
      isDirectory: true
    )
    return UpdateStatePaths(directory: directory, file: directory.appendingPathComponent("update-check.json"))
  }
}

public struct RuntimeHealth: Codable {
  public let status: String
  public let version: String
  public let uptimeSeconds: Int
  public let pid: Int32
}

public enum UnixSocketError: Error, LocalizedError {
  case pathTooLong(String)
  case socketFailed
  case connectFailed(String)
  case writeFailed
  case emptyResponse
  case invalidResponse
  case rpcError(String)
  case socketPathExists(String)

  public var errorDescription: String? {
    switch self {
    case .pathTooLong(let path):
      return "Socket path is too long: \(path)"
    case .socketFailed:
      return "Could not create socket"
    case .connectFailed(let path):
      return "Could not connect to \(path)"
    case .writeFailed:
      return "Could not write request"
    case .emptyResponse:
      return "1Context did not return a response"
    case .invalidResponse:
      return "1Context returned an invalid response"
    case .rpcError(let message):
      return message
    case .socketPathExists(let path):
      return "Socket path already exists and is not a socket: \(path)"
    }
  }
}

public func withUnixSocketAddress<T>(
  path: String,
  _ body: (UnsafePointer<sockaddr>, socklen_t) throws -> T
) throws -> T {
  var address = sockaddr_un()
  address.sun_family = sa_family_t(AF_UNIX)

  let pathBytes = path.utf8CString.map { UInt8(bitPattern: $0) }
  let maxPathBytes = MemoryLayout.size(ofValue: address.sun_path)
  guard pathBytes.count <= maxPathBytes else {
    throw UnixSocketError.pathTooLong(path)
  }

  withUnsafeMutableBytes(of: &address.sun_path) { buffer in
    buffer.initializeMemory(as: UInt8.self, repeating: 0)
    buffer.copyBytes(from: pathBytes)
  }

  return try withUnsafePointer(to: &address) { pointer in
    try pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
      try body(sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.size))
    }
  }
}

public final class UnixJSONRPCClient {
  private let socketPath: String

  public init(socketPath: String = RuntimePaths.current().socketPath) {
    self.socketPath = socketPath
  }

  public func call(method: String, params: [String: Any] = [:]) throws -> [String: Any] {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { throw UnixSocketError.socketFailed }
    defer { close(fd) }

    let connected = try withUnixSocketAddress(path: socketPath) { pointer, length in
      connect(fd, pointer, length)
    }
    guard connected == 0 else { throw UnixSocketError.connectFailed(socketPath) }

    let payload: [String: Any] = [
      "jsonrpc": "2.0",
      "id": 1,
      "method": method,
      "params": params
    ]
    let requestData = try JSONSerialization.data(withJSONObject: payload)
      + Data([UInt8(ascii: "\n")])

    let written = requestData.withUnsafeBytes { buffer in
      write(fd, buffer.baseAddress, requestData.count)
    }
    guard written == requestData.count else { throw UnixSocketError.writeFailed }

    var response = Data()
    var buffer = [UInt8](repeating: 0, count: 4096)
    while true {
      let count = read(fd, &buffer, buffer.count)
      if count <= 0 { break }
      response.append(buffer, count: count)
      if response.contains(UInt8(ascii: "\n")) { break }
    }

    guard !response.isEmpty else { throw UnixSocketError.emptyResponse }
    let line = response.split(separator: UInt8(ascii: "\n"), maxSplits: 1).first ?? response[...]
    let object = try JSONSerialization.jsonObject(with: Data(line))
    guard let dictionary = object as? [String: Any] else {
      throw UnixSocketError.invalidResponse
    }

    if let error = dictionary["error"] as? [String: Any] {
      throw UnixSocketError.rpcError(error["message"] as? String ?? "1Context returned an error")
    }

    return dictionary["result"] as? [String: Any] ?? [:]
  }

  public func health() throws -> RuntimeHealth {
    let result = try call(method: "health")
    let data = try JSONSerialization.data(withJSONObject: result)
    return try JSONDecoder().decode(RuntimeHealth.self, from: data)
  }
}

public struct ReleaseInfo: Sendable {
  public let version: String
  public let notesURL: URL?

  public init(version: String, notesURL: URL?) {
    self.version = version
    self.notesURL = notesURL
  }
}

public struct UpdateCheckResult: Sendable {
  public let latest: ReleaseInfo?
  public let updateAvailable: Bool
  public let checked: Bool
}

public final class UpdateChecker {
  private let environment: [String: String]
  private let session: URLSession

  public init(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    session: URLSession = .shared
  ) {
    self.environment = environment
    self.session = session
  }

  public func check(force: Bool = false, currentVersion: String = oneContextVersion) async throws -> UpdateCheckResult {
    if !force && environment["ONECONTEXT_NO_UPDATE_CHECK"] == "1" {
      return UpdateCheckResult(latest: nil, updateAvailable: false, checked: false)
    }

    let statePaths = UpdateStatePaths.current(environment: environment)
    if !force, let cached = readState(at: statePaths.file) {
      let lastChecked = (cached["last_checked_at"] as? String).flatMap {
        ISO8601DateFormatter().date(from: $0)
      }
      if let lastChecked, Date().timeIntervalSince(lastChecked) < oneContextUpdateCheckInterval {
        let release = releaseInfo(fromState: cached)
        return UpdateCheckResult(
          latest: release,
          updateAvailable: release.map { compareVersions($0.version, currentVersion) > 0 } ?? false,
          checked: false
        )
      }
    }

    let release = try await fetchLatestRelease(currentVersion: currentVersion)
    writeState(release, at: statePaths)
    return UpdateCheckResult(
      latest: release,
      updateAvailable: compareVersions(release.version, currentVersion) > 0,
      checked: true
    )
  }

  public func cached(currentVersion: String = oneContextVersion) -> UpdateCheckResult? {
    let state = readState(at: UpdateStatePaths.current(environment: environment).file)
    guard let release = state.flatMap(releaseInfo(fromState:)) else { return nil }
    return UpdateCheckResult(
      latest: release,
      updateAvailable: compareVersions(release.version, currentVersion) > 0,
      checked: false
    )
  }

  private func fetchLatestRelease(currentVersion: String) async throws -> ReleaseInfo {
    let url = environment["ONECONTEXT_UPDATE_URL"].flatMap(URL.init(string:)) ?? oneContextLatestReleaseURL
    var request = URLRequest(url: url)
    request.setValue("application/json", forHTTPHeaderField: "accept")
    request.setValue("1context/\(currentVersion)", forHTTPHeaderField: "user-agent")
    request.timeoutInterval = 5

    let (data, response) = try await session.data(for: request)
    guard let httpResponse = response as? HTTPURLResponse,
      (200..<300).contains(httpResponse.statusCode)
    else {
      throw URLError(.badServerResponse)
    }

    let object = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    let release = (object?["stable"] as? [String: Any]) ?? object
    let rawVersion = release?["version"] as? String
      ?? release?["tag_name"] as? String
      ?? release?["name"] as? String
      ?? ""
    let version = rawVersion.replacingOccurrences(of: "^v", with: "", options: .regularExpression)
    let notesURL = (release?["notes_url"] as? String ?? release?["html_url"] as? String).flatMap(URL.init(string:))
    return ReleaseInfo(version: version, notesURL: notesURL)
  }

  private func readState(at url: URL) -> [String: Any]? {
    guard let data = try? Data(contentsOf: url) else { return nil }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
  }

  private func releaseInfo(fromState state: [String: Any]) -> ReleaseInfo? {
    guard let version = state["last_seen_latest"] as? String else { return nil }
    return ReleaseInfo(
      version: version,
      notesURL: (state["notes_url"] as? String).flatMap(URL.init(string:))
    )
  }

  private func writeState(_ release: ReleaseInfo, at paths: UpdateStatePaths) {
    try? FileManager.default.createDirectory(at: paths.directory, withIntermediateDirectories: true)
    var state: [String: Any] = [
      "last_checked_at": ISO8601DateFormatter().string(from: Date()),
      "last_seen_latest": release.version
    ]
    if let notesURL = release.notesURL {
      state["notes_url"] = notesURL.absoluteString
    }
    if let data = try? JSONSerialization.data(withJSONObject: state, options: [.prettyPrinted, .sortedKeys]) {
      try? (data + Data([UInt8(ascii: "\n")])).write(to: paths.file, options: .atomic)
    }
  }
}

public struct LaunchAgentState {
  public let configured: Bool
  public let loaded: Bool
}

public final class LaunchAgentManager {
  public static let runtimeLabel = "com.haptica.1context"
  public static let menuLabel = "com.haptica.1context.menu"

  private let environment: [String: String]

  public init(environment: [String: String] = ProcessInfo.processInfo.environment) {
    self.environment = environment
  }

  public var isDisabled: Bool {
    ProcessInfo.processInfo.operatingSystemVersionString.isEmpty
      || environment["ONECONTEXT_LAUNCH_AGENT_DISABLED"] == "1"
  }

  public func status() async -> LaunchAgentState {
    if isDisabled { return LaunchAgentState(configured: false, loaded: false) }
    let result = await launchctl(["print", agentTarget()])
    return LaunchAgentState(
      configured: FileManager.default.fileExists(atPath: launchAgentPath.path),
      loaded: result.status == 0
    )
  }

  public func start(daemonPath: String) async throws {
    try install(daemonPath: daemonPath)
    _ = await launchctl(["bootout", guiDomain(), launchAgentPath.path])
    let boot = await launchctl(["bootstrap", guiDomain(), launchAgentPath.path])
    if boot.status != 0 {
      throw RuntimeControlError.launchAgentFailed((boot.stderr + boot.stdout).trimmingCharacters(in: .whitespacesAndNewlines))
    }
    _ = await launchctl(["kickstart", "-k", agentTarget()])
  }

  public func stop() async {
    let byTarget = await launchctl(["bootout", agentTarget()])
    if byTarget.status != 0 {
      _ = await launchctl(["bootout", guiDomain(), launchAgentPath.path])
    }
  }

  public func uninstallManagedLaunchAgents() async {
    await quitMenuApp()
    for label in [Self.menuLabel, Self.runtimeLabel] {
      let path = launchAgentPath(label: label)
      _ = await launchctl(["bootout", "\(guiDomain())/\(label)"])
      _ = await launchctl(["bootout", guiDomain(), path.path])
      try? FileManager.default.removeItem(at: path)
    }
  }

  private var launchAgentPath: URL {
    launchAgentPath(label: Self.runtimeLabel)
  }

  private func launchAgentPath(label: String) -> URL {
    FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent("Library/LaunchAgents/\(label).plist")
  }

  private func install(daemonPath: String) throws {
    let paths = RuntimePaths.current(environment: environment)
    try FileManager.default.createDirectory(at: paths.runDirectory, withIntermediateDirectories: true)
    chmod(paths.runDirectory.path, 0o700)
    try FileManager.default.createDirectory(at: paths.logDirectory, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: launchAgentPath.deletingLastPathComponent(), withIntermediateDirectories: true)
    try plist(daemonPath: daemonPath, paths: paths).write(to: launchAgentPath, atomically: true, encoding: .utf8)
  }

  private func plist(daemonPath: String, paths: RuntimePaths) -> String {
    """
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
      <key>Label</key>
      <string>\(Self.runtimeLabel)</string>
      <key>ProgramArguments</key>
      <array>
        <string>\(plistEscape(daemonPath))</string>
      </array>
      <key>RunAtLoad</key>
      <true/>
      <key>KeepAlive</key>
      <true/>
      <key>StandardOutPath</key>
      <string>\(plistEscape(paths.logPath))</string>
      <key>StandardErrorPath</key>
      <string>\(plistEscape(paths.logPath))</string>
      <key>EnvironmentVariables</key>
      <dict>
        <key>ONECONTEXT_APP_SUPPORT_DIR</key>
        <string>\(plistEscape(paths.appSupportDirectory.path))</string>
        <key>ONECONTEXT_USER_CONTENT_DIR</key>
        <string>\(plistEscape(paths.userContentDirectory.path))</string>
        <key>ONECONTEXT_LOG_DIR</key>
        <string>\(plistEscape(paths.logDirectory.path))</string>
        <key>ONECONTEXT_CACHE_DIR</key>
        <string>\(plistEscape(paths.cacheDirectory.path))</string>
      </dict>
    </dict>
    </plist>
    """
  }

  private func guiDomain() -> String {
    "gui/\(getuid())"
  }

  private func agentTarget() -> String {
    "\(guiDomain())/\(Self.runtimeLabel)"
  }

  private func launchctl(_ args: [String]) async -> (status: Int32, stdout: String, stderr: String) {
    await runProcess(executable: "/bin/launchctl", arguments: args)
  }

  private func quitMenuApp() async {
    _ = await runProcess(
      executable: "/usr/bin/osascript",
      arguments: ["-e", "tell application id \"com.haptica.1context.menu\" to quit"]
    )
  }

  private func runProcess(executable: String, arguments: [String]) async -> (status: Int32, stdout: String, stderr: String) {
    await withCheckedContinuation { continuation in
      let process = Process()
      process.executableURL = URL(fileURLWithPath: executable)
      process.arguments = arguments
      let stdout = Pipe()
      let stderr = Pipe()
      process.standardOutput = stdout
      process.standardError = stderr

      do {
        try process.run()
      } catch {
        continuation.resume(returning: (1, "", error.localizedDescription))
        return
      }

      process.terminationHandler = { process in
        let stdoutData = stdout.fileHandleForReading.readDataToEndOfFile()
        let stderrData = stderr.fileHandleForReading.readDataToEndOfFile()
        continuation.resume(returning: (
          process.terminationStatus,
          String(data: stdoutData, encoding: .utf8) ?? "",
          String(data: stderrData, encoding: .utf8) ?? ""
        ))
      }
    }
  }
}

public enum RuntimeControlError: Error, LocalizedError {
  case daemonNotFound
  case missingPID
  case launchAgentFailed(String)
  case timedOut(String)
  case unsafeDeletionPath(String)

  public var errorDescription: String? {
    switch self {
    case .daemonNotFound:
      return "1Context runtime is not installed"
    case .missingPID:
      return "1Context is running, but did not report a process id"
    case .launchAgentFailed(let message):
      return message.isEmpty ? "Could not start 1Context" : message
    case .timedOut(let message):
      return message
    case .unsafeDeletionPath(let path):
      return "Refusing to delete unsafe path: \(path)"
    }
  }
}

public final class RuntimeController {
  private let environment: [String: String]
  private let client: UnixJSONRPCClient
  private let launchAgent: LaunchAgentManager

  public init(environment: [String: String] = ProcessInfo.processInfo.environment) {
    self.environment = environment
    self.client = UnixJSONRPCClient(socketPath: RuntimePaths.current(environment: environment).socketPath)
    self.launchAgent = LaunchAgentManager(environment: environment)
  }

  public func status() -> Result<RuntimeHealth, Error> {
    Result { try client.health() }
  }

  public func launchAgentState() async -> LaunchAgentState {
    await launchAgent.status()
  }

  public func start() async throws -> (alreadyRunning: Bool, health: RuntimeHealth) {
    setStartDesired(true)
    if case .success(let health) = status() {
      return (true, health)
    }
    guard let daemon = findDaemonPath() else { throw RuntimeControlError.daemonNotFound }

    if launchAgent.isDisabled {
      try startDetached(daemonPath: daemon)
    } else {
      try await launchAgent.start(daemonPath: daemon)
    }

    return (false, try await waitForRunning())
  }

  public func stop() async throws -> Bool {
    setStartDesired(false)
    let current = status()
    if !launchAgent.isDisabled {
      await launchAgent.stop()
      if case .failure = current { return false }
      try await waitForStopped()
      return true
    }

    guard case .success(let health) = current else { return false }
    guard health.pid > 0 else { throw RuntimeControlError.missingPID }
    kill(health.pid, SIGTERM)
    try await waitForStopped()
    return true
  }

  public func restart() async throws -> RuntimeHealth {
    setStartDesired(true)
    _ = try await stop()
    setStartDesired(true)
    return try await start().health
  }

  public func shouldAutoStartRuntime() -> Bool {
    let paths = RuntimePaths.current(environment: environment)
    guard let state = try? String(contentsOfFile: paths.desiredStatePath, encoding: .utf8) else {
      return true
    }
    return state.trimmingCharacters(in: .whitespacesAndNewlines) != "stopped"
  }

  public func uninstall(deleteData: Bool = false) async throws {
    _ = try? await stop()
    await launchAgent.uninstallManagedLaunchAgents()
    if deleteData {
      try removeLocalData()
    }
  }

  private func removeLocalData() throws {
    let fileManager = FileManager.default
    let runtimePaths = RuntimePaths.current(environment: environment)
    for url in [
      runtimePaths.userContentDirectory,
      runtimePaths.appSupportDirectory,
      runtimePaths.logDirectory,
      runtimePaths.cacheDirectory,
      URL(fileURLWithPath: runtimePaths.preferencesPath)
    ] {
      try removeLocalDataItem(url, fileManager: fileManager)
    }
  }

  private func removeLocalDataItem(_ url: URL, fileManager: FileManager) throws {
    let standardized = url.standardizedFileURL
    guard isSafeLocalDataDirectory(standardized) else {
      throw RuntimeControlError.unsafeDeletionPath(standardized.path)
    }
    try? fileManager.removeItem(at: standardized)
  }

  private func isSafeLocalDataDirectory(_ url: URL) -> Bool {
    let home = FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL.path
    let temporaryDirectory = FileManager.default.temporaryDirectory.standardizedFileURL.path
    let path = url.path
    let lastComponent = url.lastPathComponent.lowercased()

    guard path != "/" && path != home else {
      return false
    }

    let isUnderAllowedRoot = path.hasPrefix(home + "/")
      || path.hasPrefix(temporaryDirectory)
      || path.hasPrefix("/tmp/")
      || path.hasPrefix("/private/tmp/")
    return isUnderAllowedRoot && lastComponent.contains("1context")
  }

  private func waitForRunning(timeout: TimeInterval = 5) async throws -> RuntimeHealth {
    let deadline = Date().addingTimeInterval(timeout)
    repeat {
      if case .success(let health) = status() { return health }
      try await Task.sleep(nanoseconds: 150_000_000)
    } while Date() < deadline
    throw RuntimeControlError.timedOut("1Context did not start in time")
  }

  private func waitForStopped(timeout: TimeInterval = 5) async throws {
    let deadline = Date().addingTimeInterval(timeout)
    repeat {
      if case .failure = status() { return }
      try await Task.sleep(nanoseconds: 150_000_000)
    } while Date() < deadline
    throw RuntimeControlError.timedOut("1Context did not stop in time")
  }

  private func startDetached(daemonPath: String) throws {
    let paths = RuntimePaths.current(environment: environment)
    try FileManager.default.createDirectory(at: paths.runDirectory, withIntermediateDirectories: true)
    chmod(paths.runDirectory.path, 0o700)
    try FileManager.default.createDirectory(at: paths.logDirectory, withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: paths.cacheDirectory, withIntermediateDirectories: true)

    let process = Process()
    process.executableURL = URL(fileURLWithPath: daemonPath)
    process.environment = ProcessInfo.processInfo.environment.merging(environment) { _, new in new }
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    try process.run()
  }

  private func setStartDesired(_ desired: Bool) {
    let paths = RuntimePaths.current(environment: environment)
    do {
      try FileManager.default.createDirectory(at: paths.appSupportDirectory, withIntermediateDirectories: true)
      try (desired ? "running\n" : "stopped\n").write(
        toFile: paths.desiredStatePath,
        atomically: true,
        encoding: .utf8
      )
    } catch {
      // Desired runtime state is advisory. Lifecycle commands should still proceed.
    }
  }

  private func findDaemonPath() -> String? {
    let fm = FileManager.default
    let executableDirectory = currentExecutableURL()?.deletingLastPathComponent()
    var candidates: [String] = [
      Bundle.main.bundleURL.deletingLastPathComponent().appendingPathComponent("onecontextd").path,
      executableDirectory?.appendingPathComponent("onecontextd").path,
      URL(fileURLWithPath: "/Applications/1Context.app/Contents/MacOS/onecontextd").path
    ].compactMap { $0 }

    if environment["ONECONTEXT_ALLOW_DAEMON_OVERRIDE"] == "1",
      let override = environment["ONECONTEXT_DAEMON_PATH"]
    {
      candidates.insert(override, at: 0)
    }

    return candidates.first { fm.isExecutableFile(atPath: $0) }
  }

  private func currentExecutableURL() -> URL? {
    var size = UInt32(0)
    _NSGetExecutablePath(nil, &size)
    var buffer = [CChar](repeating: 0, count: Int(size))
    guard _NSGetExecutablePath(&buffer, &size) == 0 else { return nil }
    let pathBytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    let path = String(decoding: pathBytes, as: UTF8.self)
    return URL(fileURLWithPath: path).resolvingSymlinksInPath()
  }
}

public func compareVersions(_ lhs: String, _ rhs: String) -> Int {
  let left = versionComponents(lhs)
  let right = versionComponents(rhs)
  for index in 0..<max(left.count, right.count) {
    let difference = (index < left.count ? left[index] : 0) - (index < right.count ? right[index] : 0)
    if difference != 0 { return difference }
  }
  return 0
}

private func versionComponents(_ version: String) -> [Int] {
  version
    .replacingOccurrences(of: "^v", with: "", options: .regularExpression)
    .split(separator: ".")
    .map { Int($0) ?? 0 }
}

private func plistEscape(_ value: String) -> String {
  value
    .replacingOccurrences(of: "&", with: "&amp;")
    .replacingOccurrences(of: "<", with: "&lt;")
    .replacingOccurrences(of: ">", with: "&gt;")
    .replacingOccurrences(of: "\"", with: "&quot;")
    .replacingOccurrences(of: "'", with: "&apos;")
}
