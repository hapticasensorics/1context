import Foundation
import Darwin
import OneContextCore
import OneContextPlatform

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

  public func startMenu(appPath: String) async throws {
    guard !isDisabled else { return }
    try installMenu(appPath: appPath)
    let path = launchAgentPath(label: Self.menuLabel)
    let target = "\(guiDomain())/\(Self.menuLabel)"
    _ = await launchctl(["bootout", target])
    _ = await launchctl(["bootout", guiDomain(), path.path])
    let boot = await launchctl(["bootstrap", guiDomain(), path.path])
    if boot.status != 0 {
      throw RuntimeControlError.launchAgentFailed((boot.stderr + boot.stdout).trimmingCharacters(in: .whitespacesAndNewlines))
    }
    _ = await launchctl(["kickstart", "-k", target])
  }

  public func restart(daemonPath: String) async throws {
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
    _ = await launchctl(["kickstart", "-k", target])
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
    let paths = RuntimePaths.current(environment: environment)
    try RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.runDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
    RuntimePermissions.repairRuntimePaths(paths)
    try FileManager.default.createDirectory(at: launchAgentPath.deletingLastPathComponent(), withIntermediateDirectories: true)
    try plist(daemonPath: daemonPath, paths: paths).write(to: launchAgentPath, atomically: true, encoding: .utf8)
  }

  private func installMenu(appPath: String) throws {
    let paths = RuntimePaths.current(environment: environment)
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
        <string>\(plistEscape(UpdateStatePaths.current(environment: environment).directory.path))</string>
      </dict>
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
