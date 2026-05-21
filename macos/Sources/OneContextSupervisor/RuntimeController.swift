import Foundation
import Darwin
import OneContextCore
import OneContextPlatform
import OneContextProtocol

public final class RuntimeController {
  private let client: UnixJSONRPCClient
  private let launchAgent: LaunchAgentManager

  public init() {
    self.client = UnixJSONRPCClient(socketPath: RuntimePaths.current().socketPath)
    self.launchAgent = LaunchAgentManager()
  }

  public func status() -> Result<RuntimeHealth, Error> {
    Result { try client.health() }
  }

  public func snapshot() -> RuntimeSnapshot {
    switch status() {
    case .success(let health):
      if health.version == oneContextVersion {
        if health.requiredSetupReady == false {
          return RuntimeSnapshot(
            state: .needsSetup,
            health: health,
            lastErrorDescription: health.requiredSetupSummary ?? "1Context setup is incomplete",
            recommendedAction: "Open 1Context Setup"
          )
        }
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
        state: .needsAttention,
        lastErrorDescription: error.localizedDescription,
        recommendedAction: "Open 1Context"
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
    try ensureNormalUserLifecycle()
    if case .success(let health) = status() {
      if health.version == oneContextVersion {
        if startMenu { try await startMenuIfAvailable() }
        return (true, health)
      }

      let restarted = try await restartRuntimeForVersionMismatch()
      if startMenu { try await startMenuIfAvailable() }
      return (false, restarted)
    }
    guard let daemon = findDaemonPath() else { throw RuntimeControlError.daemonNotFound }

    try await launchAgent.start(daemonPath: daemon)

    let health = try await waitForRunning()
    if startMenu { try await startMenuIfAvailable() }
    return (false, health)
  }

  public func requestStart(startMenu: Bool = true) async throws {
    try ensureNormalUserLifecycle()
    let current = status()
    if case .success(let health) = current, health.version == oneContextVersion {
      if startMenu { try await startMenuIfAvailable() }
      return
    }
    guard let daemon = findDaemonPath() else { throw RuntimeControlError.daemonNotFound }

    if case .success(let health) = current, health.version != oneContextVersion {
      try await launchAgent.restart(daemonPath: daemon)
    } else {
      try await launchAgent.start(daemonPath: daemon)
    }
    if startMenu { try await startMenuIfAvailable() }
  }

  public func stop() async throws -> Bool {
    try await stopRuntime()
  }

  public func stopForAppQuit() async throws -> Bool {
    try await stopRuntime()
  }

  private func stopRuntime() async throws -> Bool {
    try ensureNormalUserLifecycle()
    let current = status()
    await launchAgent.stop()
    if case .failure = current { return false }
    try await waitForStopped()
    return true
  }

  public func quit() async throws -> Bool {
    try await quit(stopMenu: true)
  }

  public func quit(stopMenu: Bool) async throws -> Bool {
    try ensureNormalUserLifecycle()
    let stopped = try await stop()
    if stopMenu {
      await launchAgent.stopMenu()
    }
    return stopped
  }

  private func restartRuntimeForVersionMismatch() async throws -> RuntimeHealth {
    guard let daemon = findDaemonPath() else { throw RuntimeControlError.daemonNotFound }

    try await launchAgent.restart(daemonPath: daemon)
    return try await waitForRunning()
  }

  public func restart() async throws -> RuntimeHealth {
    try await restart(startMenu: true)
  }

  public func restart(startMenu: Bool) async throws -> RuntimeHealth {
    try ensureNormalUserLifecycle()
    guard let daemon = findDaemonPath() else { throw RuntimeControlError.daemonNotFound }

    try await launchAgent.restart(daemonPath: daemon)
    let health = try await waitForRunning()
    if startMenu { try await startMenuIfAvailable() }
    return health
  }

  public func uninstall(deleteData: Bool = false) async throws {
    try ensureNormalUserLifecycle()
    _ = try? await stop()
    await launchAgent.uninstallManagedLaunchAgents()
    if deleteData {
      try removeLocalData()
    }
  }

  private func removeLocalData() throws {
    let fileManager = FileManager.default
    let runtimePaths = RuntimePaths.current()
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

  private func ensureNormalUserLifecycle() throws {
    if ProcessPrivilegePolicy.rejectsAppOwnedUserLifecycle() {
      throw RuntimeControlError.rootUserUnsupported
    }
  }

  private func findDaemonPath() -> String? {
    let fm = FileManager.default
    guard let executableDirectory = currentExecutableURL()?.deletingLastPathComponent() else {
      return nil
    }

    let bundled = executableDirectory.appendingPathComponent("1contextd").resolvingSymlinksInPath()
    if isBundledMacOSDirectory(executableDirectory), fm.isExecutableFile(atPath: bundled.path) {
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
      RuntimePaths.current().identity.appBundleURL.appendingPathComponent("Contents/MacOS/1Context").path
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
