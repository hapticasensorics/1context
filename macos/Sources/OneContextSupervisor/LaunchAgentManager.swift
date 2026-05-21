import Foundation
import Darwin
import OneContextCore
import OneContextPlatform

public struct LaunchAgentState {
  public let configured: Bool
  public let loaded: Bool
}

typealias ProcessResult = (status: Int32, stdout: String, stderr: String)

public final class LaunchAgentManager {
  public static var runtimeLabel: String {
    OneContextAppIdentity.current().runtimeLaunchAgentLabel
  }
  public static var menuLabel: String {
    OneContextAppIdentity.current().menuLaunchAgentLabel
  }

  private let homeDirectory: URL
  private let runtimePaths: RuntimePaths
  private let identity: OneContextAppIdentity
  private let uid: uid_t
  private let isRootLifecycleRejected: @Sendable () -> Bool
  private let processRunner: @Sendable (String, [String], TimeInterval) async -> ProcessResult

  public convenience init() {
    self.init(
      homeDirectory: FileManager.default.homeDirectoryForCurrentUser,
      runtimePaths: RuntimePaths.current(),
      uid: getuid(),
      isRootLifecycleRejected: { ProcessPrivilegePolicy.rejectsAppOwnedUserLifecycle() },
      processRunner: { executable, arguments, timeout in
        await LaunchAgentManager.runProcess(executable: executable, arguments: arguments, timeout: timeout)
      }
    )
  }

  init(
    homeDirectory: URL,
    runtimePaths: RuntimePaths,
    uid: uid_t,
    isRootLifecycleRejected: @escaping @Sendable () -> Bool,
    processRunner: @escaping @Sendable (String, [String], TimeInterval) async -> ProcessResult
  ) {
    self.homeDirectory = homeDirectory
    self.runtimePaths = runtimePaths
    self.identity = runtimePaths.identity
    self.uid = uid
    self.isRootLifecycleRejected = isRootLifecycleRejected
    self.processRunner = processRunner
  }

  public func status() async -> LaunchAgentState {
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
    try ensureNormalUserLifecycle()
    try installMenu(appPath: appPath)
    let path = launchAgentPath(label: identity.menuLaunchAgentLabel)
    let target = "\(guiDomain())/\(identity.menuLaunchAgentLabel)"

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
    guard launchAgentLifecycleIsSafe() else { return }
    let path = launchAgentPath
    let byTarget = await launchctl(["bootout", agentTarget()])
    if byTarget.status != 0 {
      _ = await launchctl(["bootout", guiDomain(), path.path])
    }
    try? FileManager.default.removeItem(at: path)
  }

  public func stopMenu() async {
    guard launchAgentLifecycleIsSafe() else { return }
    await quitMenuApp()
    let path = launchAgentPath(label: identity.menuLaunchAgentLabel)
    _ = await launchctl(["bootout", "\(guiDomain())/\(identity.menuLaunchAgentLabel)"])
    _ = await launchctl(["bootout", guiDomain(), path.path])
    try? FileManager.default.removeItem(at: path)
  }

  public func uninstallManagedLaunchAgents() async {
    guard launchAgentLifecycleIsSafe() else { return }
    await quitMenuApp()
    for label in [identity.menuLaunchAgentLabel, identity.runtimeLaunchAgentLabel] {
      let path = launchAgentPath(label: label)
      _ = await launchctl(["bootout", "\(guiDomain())/\(label)"])
      _ = await launchctl(["bootout", guiDomain(), path.path])
      try? FileManager.default.removeItem(at: path)
    }
  }

  private var launchAgentPath: URL {
    launchAgentPath(label: identity.runtimeLaunchAgentLabel)
  }

  private func launchAgentPath(label: String) -> URL {
    homeDirectory
      .appendingPathComponent("Library/LaunchAgents/\(label).plist")
  }

  private func install(daemonPath: String) throws {
    let paths = runtimePaths
    try RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.runDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
    RuntimePermissions.repairRuntimePaths(paths)
    try FileManager.default.createDirectory(at: launchAgentPath.deletingLastPathComponent(), withIntermediateDirectories: true)
    try plist(daemonPath: daemonPath, paths: paths).write(to: launchAgentPath, atomically: true, encoding: .utf8)
  }

  private func installMenu(appPath: String) throws {
    let paths = runtimePaths
    let menuLogPath = paths.logDirectory.appendingPathComponent("menu.log").path
    try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
    _ = FileManager.default.createFile(atPath: menuLogPath, contents: nil)
    RuntimePermissions.ensurePrivateFile(menuLogPath)
    try FileManager.default.createDirectory(at: launchAgentPath(label: identity.menuLaunchAgentLabel).deletingLastPathComponent(), withIntermediateDirectories: true)
    try menuPlist(appPath: appPath, paths: paths).write(
      to: launchAgentPath(label: identity.menuLaunchAgentLabel),
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
      <string>\(identity.runtimeLaunchAgentLabel)</string>
      <key>EnvironmentVariables</key>
      <dict>
        <key>\(OneContextAppIdentity.environmentKey)</key>
        <string>\(identity.environmentValue)</string>
      </dict>
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
      <string>\(identity.menuLaunchAgentLabel)</string>
      <key>EnvironmentVariables</key>
      <dict>
        <key>\(OneContextAppIdentity.environmentKey)</key>
        <string>\(identity.environmentValue)</string>
      </dict>
      <key>ProgramArguments</key>
      <array>
        <string>\(plistEscape(appPath))</string>
      </array>
      <key>RunAtLoad</key>
      <true/>
      <key>KeepAlive</key>
      <dict>
        <key>SuccessfulExit</key>
        <false/>
        <key>Crashed</key>
        <true/>
      </dict>
      <key>ThrottleInterval</key>
      <integer>1</integer>
      <key>StandardOutPath</key>
      <string>\(plistEscape(paths.logDirectory.appendingPathComponent("menu.log").path))</string>
      <key>StandardErrorPath</key>
      <string>\(plistEscape(paths.logDirectory.appendingPathComponent("menu.log").path))</string>
    </dict>
    </plist>
    """
  }

  private func ensureNormalUserLifecycle() throws {
    if isRootLifecycleRejected() {
      throw RuntimeControlError.rootUserUnsupported
    }
    try ensureStandardRuntimePathsForLaunchAgentLifecycle()
  }

  private func launchAgentLifecycleIsSafe() -> Bool {
    (try? ensureNormalUserLifecycle()) != nil
  }

  private func ensureStandardRuntimePathsForLaunchAgentLifecycle() throws {
    let home = homeDirectory.standardizedFileURL
    let expected = identity.runtimePaths(homeDirectory: home)
    let checks: [(label: String, actual: String, expected: String)] = [
      ("userContentDirectory", runtimePaths.userContentDirectory.path, expected.userContentDirectory.path),
      ("appSupportDirectory", runtimePaths.appSupportDirectory.path, expected.appSupportDirectory.path),
      ("runDirectory", runtimePaths.runDirectory.path, expected.runDirectory.path),
      ("socketPath", runtimePaths.socketPath, expected.socketPath),
      ("pidPath", runtimePaths.pidPath, expected.pidPath),
      ("logDirectory", runtimePaths.logDirectory.path, expected.logDirectory.path),
      ("logPath", runtimePaths.logPath, expected.logPath),
      ("cacheDirectory", runtimePaths.cacheDirectory.path, expected.cacheDirectory.path),
      ("preferencesPath", runtimePaths.preferencesPath, expected.preferencesPath)
    ]

    for check in checks where standardizedPath(check.actual) != standardizedPath(check.expected) {
      throw RuntimeControlError.unsafeLaunchAgentRuntimePaths(
        "\(check.label)=\(check.actual) expected=\(check.expected)"
      )
    }
  }

  private func standardizedPath(_ path: String) -> String {
    URL(fileURLWithPath: path).standardizedFileURL.path
  }

  private func guiDomain() -> String {
    "gui/\(uid)"
  }

  private func agentTarget() -> String {
    "\(guiDomain())/\(identity.runtimeLaunchAgentLabel)"
  }

  private func launchctl(_ args: [String]) async -> ProcessResult {
    await processRunner("/bin/launchctl", args, 2)
  }

  private func launchAgentHasPID(_ output: String) -> Bool {
    output.split(separator: "\n").contains { line in
      line.trimmingCharacters(in: .whitespaces).hasPrefix("pid =")
    }
  }

  private func quitMenuApp() async {
    _ = await launchctl(["kill", "TERM", "\(guiDomain())/\(Self.menuLabel)"])
  }

  private static func runProcess(executable: String, arguments: [String], timeout: TimeInterval) async -> ProcessResult {
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
