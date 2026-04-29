import Foundation
import Darwin
import OneContextCore
import OneContextPlatform
import OneContextProtocol

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

  public func snapshot() -> RuntimeSnapshot {
    switch status() {
    case .success(let health):
      if health.version == oneContextVersion {
        return RuntimeSnapshot(state: .running, health: health)
      }
      return RuntimeSnapshot(
        state: .needsAttention,
        health: health,
        lastErrorDescription: "1Context runtime version \(health.version) does not match app version \(oneContextVersion)",
        recommendedAction: "Restart 1Context"
      )
    case .failure(let error):
      return RuntimeSnapshot(
        state: .stopped,
        lastErrorDescription: error.localizedDescription,
        recommendedAction: "Start 1Context"
      )
    }
  }

  public func launchAgentState() async -> LaunchAgentState {
    await launchAgent.status()
  }

  public func start() async throws -> (alreadyRunning: Bool, health: RuntimeHealth) {
    try await start(startMenu: true)
  }

  public func start(startMenu: Bool) async throws -> (alreadyRunning: Bool, health: RuntimeHealth) {
    setStartDesired(true)
    if case .success(let health) = status() {
      if health.version == oneContextVersion {
        if startMenu { try await startMenuIfAvailable() }
        return (true, health)
      }

      let restarted = try await restartRuntimeForVersionMismatch(existingHealth: health)
      if startMenu { try await startMenuIfAvailable() }
      return (false, restarted)
    }
    guard let daemon = findDaemonPath() else { throw RuntimeControlError.daemonNotFound }

    if launchAgent.isDisabled {
      try startDetached(daemonPath: daemon)
    } else {
      try await launchAgent.start(daemonPath: daemon)
    }

    let health = try await waitForRunning()
    if startMenu { try await startMenuIfAvailable() }
    return (false, health)
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

  public func quit() async throws -> Bool {
    try await quit(stopMenu: true)
  }

  public func quit(stopMenu: Bool) async throws -> Bool {
    let stopped = try await stop()
    if stopMenu, !launchAgent.isDisabled {
      await launchAgent.stopMenu()
    }
    return stopped
  }

  private func restartRuntimeForVersionMismatch(existingHealth health: RuntimeHealth) async throws -> RuntimeHealth {
    guard let daemon = findDaemonPath() else { throw RuntimeControlError.daemonNotFound }

    if launchAgent.isDisabled {
      guard health.pid > 0 else { throw RuntimeControlError.missingPID }
      kill(health.pid, SIGTERM)
      try await waitForStopped()
      try startDetached(daemonPath: daemon)
      return try await waitForRunning()
    }

    try await launchAgent.restart(daemonPath: daemon)
    return try await waitForRunning()
  }

  public func restart() async throws -> RuntimeHealth {
    try await restart(startMenu: true)
  }

  public func restart(startMenu: Bool) async throws -> RuntimeHealth {
    setStartDesired(true)
    guard let daemon = findDaemonPath() else { throw RuntimeControlError.daemonNotFound }

    if launchAgent.isDisabled {
      _ = try await stop()
      setStartDesired(true)
      return try await start(startMenu: startMenu).health
    }

    try await launchAgent.restart(daemonPath: daemon)
    let health = try await waitForRunning()
    if startMenu { try await startMenuIfAvailable() }
    return health
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
      if case .success(let health) = status(), health.version == oneContextVersion { return health }
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
    try RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.runDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.cacheDirectory)
    RuntimePermissions.repairRuntimePaths(paths)

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
      try RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
      try RuntimePermissions.writePrivateString(desired ? "running\n" : "stopped\n", toFile: paths.desiredStatePath)
    } catch {
      // Desired runtime state is advisory. Lifecycle commands should still proceed.
    }
  }

  private func findDaemonPath() -> String? {
    let fm = FileManager.default
    if environment["ONECONTEXT_ALLOW_DAEMON_OVERRIDE"] == "1",
      let override = environment["ONECONTEXT_DAEMON_PATH"]
    {
      let resolved = URL(fileURLWithPath: override).resolvingSymlinksInPath().path
      return fm.isExecutableFile(atPath: resolved) ? resolved : nil
    }

    guard let executableDirectory = currentExecutableURL()?.deletingLastPathComponent() else {
      return nil
    }

    let bundled = executableDirectory.appendingPathComponent("1contextd").resolvingSymlinksInPath()
    if isBundledMacOSDirectory(executableDirectory), fm.isExecutableFile(atPath: bundled.path) {
      return bundled.path
    }

    if environment["ONECONTEXT_LAUNCH_AGENT_DISABLED"] == "1", fm.isExecutableFile(atPath: bundled.path) {
      return bundled.path
    }

    return nil
  }

  private func startMenuIfAvailable() async throws {
    guard let menuApp = findMenuAppPath() else { return }
    try await launchAgent.startMenu(appPath: menuApp)
  }

  private func findMenuAppPath() -> String? {
    let fm = FileManager.default
    let executableDirectory = currentExecutableURL()?.deletingLastPathComponent()
    let candidates: [String?] = [
      executableDirectory?.appendingPathComponent("OneContextMenuBar").path,
      executableDirectory?.appendingPathComponent("1Context").path,
      URL(fileURLWithPath: "/Applications/1Context.app/Contents/MacOS/1Context").path
    ]
    return candidates.compactMap { $0 }.first { fm.isExecutableFile(atPath: $0) }
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

  private func isBundledMacOSDirectory(_ directory: URL) -> Bool {
    guard directory.lastPathComponent == "MacOS",
      directory.deletingLastPathComponent().lastPathComponent == "Contents"
    else {
      return false
    }
    return FileManager.default.fileExists(
      atPath: directory.deletingLastPathComponent().appendingPathComponent("Info.plist").path
    )
  }
}
