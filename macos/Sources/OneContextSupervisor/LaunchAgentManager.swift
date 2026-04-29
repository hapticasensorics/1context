import Foundation
import Darwin
import OneContextCore
import OneContextPlatform

public struct LaunchAgentState {
  public let configured: Bool
  public let loaded: Bool
}

private typealias ProcessResult = (status: Int32, stdout: String, stderr: String)

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
    try ensureNormalUserLifecycle()
    try install(daemonPath: daemonPath)
    _ = await launchctl(["bootout", guiDomain(), launchAgentPath.path])
    let boot = await launchctl(["bootstrap", guiDomain(), launchAgentPath.path])
    if boot.status != 0 {
      throw RuntimeControlError.launchAgentFailed((boot.stderr + boot.stdout).trimmingCharacters(in: .whitespacesAndNewlines))
    }
  }

  public func startMenu(appPath: String) async throws {
    guard !isDisabled else { return }
    try ensureNormalUserLifecycle()
    try installMenu(appPath: appPath)
    let path = launchAgentPath(label: Self.menuLabel)
    let target = "\(guiDomain())/\(Self.menuLabel)"

    let current = await launchctl(["print", target])
    if current.status == 0 {
      if launchAgentHasPID(current.stdout) {
        return
      }
      _ = await launchctl(["kickstart", "-k", target])
      return
    }

    let boot = await launchctl(["bootstrap", guiDomain(), path.path])
    if boot.status != 0 {
      throw RuntimeControlError.launchAgentFailed((boot.stderr + boot.stdout).trimmingCharacters(in: .whitespacesAndNewlines))
    }
  }

  public func restart(daemonPath: String) async throws {
    try ensureNormalUserLifecycle()
    try install(daemonPath: daemonPath)
    let target = agentTarget()
    let current = await launchctl(["print", target])
    if current.status != 0 {
      try await start(daemonPath: daemonPath)
      return
    }

    _ = await launchctl(["bootout", target])
    _ = await launchctl(["bootout", guiDomain(), launchAgentPath.path])
    let boot = await launchctl(["bootstrap", guiDomain(), launchAgentPath.path])
    if boot.status != 0 {
      throw RuntimeControlError.launchAgentFailed(
        (boot.stderr + boot.stdout).trimmingCharacters(in: .whitespacesAndNewlines)
      )
    }
  }

  public func stop() async {
    let path = launchAgentPath
    let byTarget = await launchctl(["bootout", agentTarget()])
    if byTarget.status != 0 {
      _ = await launchctl(["bootout", guiDomain(), path.path])
    }
    try? FileManager.default.removeItem(at: path)
  }

  public func stopMenu() async {
    await quitMenuApp()
    let path = launchAgentPath(label: Self.menuLabel)
    _ = await launchctl(["bootout", "\(guiDomain())/\(Self.menuLabel)"])
    _ = await launchctl(["bootout", guiDomain(), path.path])
    try? FileManager.default.removeItem(at: path)
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
    let paths = launchAgentRuntimePaths()
    try RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.runDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
    RuntimePermissions.repairRuntimePaths(paths)
    try FileManager.default.createDirectory(at: launchAgentPath.deletingLastPathComponent(), withIntermediateDirectories: true)
    try plist(daemonPath: daemonPath, paths: paths).write(to: launchAgentPath, atomically: true, encoding: .utf8)
  }

  private func installMenu(appPath: String) throws {
    let paths = launchAgentRuntimePaths()
    let menuLogPath = paths.logDirectory.appendingPathComponent("menu.log").path
    try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
    _ = FileManager.default.createFile(atPath: menuLogPath, contents: nil)
    RuntimePermissions.ensurePrivateFile(menuLogPath)
    try FileManager.default.createDirectory(at: launchAgentPath(label: Self.menuLabel).deletingLastPathComponent(), withIntermediateDirectories: true)
    try menuPlist(appPath: appPath, paths: paths).write(
      to: launchAgentPath(label: Self.menuLabel),
      atomically: true,
      encoding: .utf8
    )
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
      <key>ThrottleInterval</key>
      <integer>1</integer>
      <key>StandardOutPath</key>
      <string>\(plistEscape(paths.logPath))</string>
      <key>StandardErrorPath</key>
      <string>\(plistEscape(paths.logPath))</string>
      <key>EnvironmentVariables</key>
      \(environmentPlist(paths: paths))
    </dict>
    </plist>
    """
  }

  private func menuPlist(appPath: String, paths: RuntimePaths) -> String {
    """
    <?xml version="1.0" encoding="UTF-8"?>
    <!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
    <plist version="1.0">
    <dict>
      <key>Label</key>
      <string>\(Self.menuLabel)</string>
      <key>ProgramArguments</key>
      <array>
        <string>\(plistEscape(appPath))</string>
      </array>
      <key>RunAtLoad</key>
      <true/>
      <key>KeepAlive</key>
      <false/>
      <key>ThrottleInterval</key>
      <integer>1</integer>
      <key>StandardOutPath</key>
      <string>\(plistEscape(paths.logDirectory.appendingPathComponent("menu.log").path))</string>
      <key>StandardErrorPath</key>
      <string>\(plistEscape(paths.logDirectory.appendingPathComponent("menu.log").path))</string>
      <key>EnvironmentVariables</key>
      \(environmentPlist(paths: paths))
    </dict>
    </plist>
    """
  }

  private func environmentPlist(paths: RuntimePaths) -> String {
    """
      <dict>
        <key>ONECONTEXT_APP_SUPPORT_DIR</key>
        <string>\(plistEscape(paths.appSupportDirectory.path))</string>
        <key>ONECONTEXT_USER_CONTENT_DIR</key>
        <string>\(plistEscape(paths.userContentDirectory.path))</string>
        <key>ONECONTEXT_LOG_DIR</key>
        <string>\(plistEscape(paths.logDirectory.path))</string>
        <key>ONECONTEXT_LOG_PATH</key>
        <string>\(plistEscape(paths.logPath))</string>
        <key>ONECONTEXT_CACHE_DIR</key>
        <string>\(plistEscape(paths.cacheDirectory.path))</string>
        <key>ONECONTEXT_SOCKET_PATH</key>
        <string>\(plistEscape(paths.socketPath))</string>
        <key>ONECONTEXT_PREFERENCES_PATH</key>
        <string>\(plistEscape(paths.preferencesPath))</string>
        <key>ONECONTEXT_UPDATE_STATE_DIR</key>
        <string>\(plistEscape(UpdateStatePaths.current(environment: launchAgentPathEnvironment()).directory.path))</string>
      </dict>
    """
  }

  private func launchAgentRuntimePaths() -> RuntimePaths {
    RuntimePaths.current(environment: launchAgentPathEnvironment())
  }

  private func launchAgentPathEnvironment() -> [String: String] {
    environment["ONECONTEXT_PERSIST_ENV_PATH_OVERRIDES"] == "1" ? environment : [:]
  }

  private func ensureNormalUserLifecycle() throws {
    if geteuid() == 0 || environment["SUDO_USER"] != nil {
      throw RuntimeControlError.rootUserUnsupported
    }
  }

  private func guiDomain() -> String {
    "gui/\(getuid())"
  }

  private func agentTarget() -> String {
    "\(guiDomain())/\(Self.runtimeLabel)"
  }

  private func launchctl(_ args: [String]) async -> ProcessResult {
    await runProcess(executable: "/bin/launchctl", arguments: args, timeout: 2)
  }

  private func launchAgentHasPID(_ output: String) -> Bool {
    output.split(separator: "\n").contains { line in
      line.trimmingCharacters(in: .whitespaces).hasPrefix("pid =")
    }
  }

  private func quitMenuApp() async {
    _ = await runProcess(
      executable: "/usr/bin/osascript",
      arguments: ["-e", "tell application id \"com.haptica.1context.menu\" to quit"],
      timeout: 2
    )
  }

  private func runProcess(executable: String, arguments: [String], timeout: TimeInterval) async -> ProcessResult {
    await withCheckedContinuation { continuation in
      let process = Process()
      let processBox = ProcessBox(process)
      process.executableURL = URL(fileURLWithPath: executable)
      process.arguments = arguments
      let result = ProcessResultState(continuation: continuation, executable: executable)

      do {
        process.standardOutput = try result.stdoutWriteHandle()
        process.standardError = try result.stderrWriteHandle()
      } catch {
        result.finish(status: 1, stderrOverride: error.localizedDescription)
        return
      }

      process.terminationHandler = { process in
        result.finish(status: process.terminationStatus)
      }

      do {
        try process.run()
      } catch {
        result.finish(status: 1, stderrOverride: error.localizedDescription)
        return
      }

      DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + timeout) {
        guard result.markTimedOut() else { return }
        processBox.terminate()
        result.finish(status: 124, stderrOverride: "\(executable) timed out")
      }
    }
  }
}

private final class ProcessResultState: @unchecked Sendable {
  private let continuation: CheckedContinuation<ProcessResult, Never>
  private let lock = NSLock()
  private let stdoutURL: URL
  private let stderrURL: URL
  private var completed = false
  private var stdoutHandle: FileHandle?
  private var stderrHandle: FileHandle?
  private let outputLimit = 64 * 1024

  init(continuation: CheckedContinuation<ProcessResult, Never>, executable: String) {
    self.continuation = continuation
    let base = FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-process-\(UUID().uuidString)")
    self.stdoutURL = base.appendingPathExtension("out")
    self.stderrURL = base.appendingPathExtension("err")
  }

  func stdoutWriteHandle() throws -> FileHandle {
    try makeHandle(url: stdoutURL, assign: { stdoutHandle = $0 })
  }

  func stderrWriteHandle() throws -> FileHandle {
    try makeHandle(url: stderrURL, assign: { stderrHandle = $0 })
  }

  private func makeHandle(url: URL, assign: (FileHandle) -> Void) throws -> FileHandle {
    FileManager.default.createFile(atPath: url.path, contents: nil)
    let handle = try FileHandle(forWritingTo: url)
    assign(handle)
    return handle
  }

  func markTimedOut() -> Bool {
    lock.lock()
    defer { lock.unlock() }
    return !completed
  }

  func finish(status: Int32, stderrOverride: String? = nil) {
    lock.lock()
    guard !completed else {
      lock.unlock()
      return
    }
    completed = true
    stdoutHandle?.closeFile()
    stderrHandle?.closeFile()
    let stdoutString = readOutput(stdoutURL)
    let stderrString = stderrOverride ?? readOutput(stderrURL)
    try? FileManager.default.removeItem(at: stdoutURL)
    try? FileManager.default.removeItem(at: stderrURL)
    lock.unlock()
    continuation.resume(returning: (status, stdoutString, stderrString))
  }

  private func readOutput(_ url: URL) -> String {
    guard let handle = try? FileHandle(forReadingFrom: url) else { return "" }
    defer { try? handle.close() }
    let data = (try? handle.read(upToCount: outputLimit)) ?? Data()
    return String(data: data, encoding: .utf8) ?? ""
  }
}

private final class ProcessBox: @unchecked Sendable {
  private let process: Process

  init(_ process: Process) {
    self.process = process
  }

  func terminate() {
    if process.isRunning {
      process.terminate()
      usleep(100_000)
      if process.isRunning {
        kill(process.processIdentifier, SIGKILL)
      }
    }
  }
}
