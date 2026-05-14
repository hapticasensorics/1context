import Darwin
import Foundation
import OneContextInstall
import OneContextLocalWeb
import OneContextCore
import OneContextPlatform
import OneContextSetup
import OneContextSupervisor
import OneContextUpdate

@main
struct OneContextCLI {
  static let args = Array(CommandLine.arguments.dropFirst())
  static let command = args.first

  static func main() async {
    do {
      switch command {
      case "--version", "-v", "version":
        print(oneContextVersion)
      case "--help", "-h":
        printHelp()
      case nil:
        await printMain()
      case "diagnose":
        try rejectUnknownArguments()
        await diagnose()
      case "uninstall":
        try rejectUnknownArguments(allowed: ["--delete-data", "--keep-app", "--menu-process"])
        try await uninstall()
      case "wiki":
        try await wiki()
      default:
        FileHandle.standardError.write(Data("Unknown command: \(command ?? "")\n".utf8))
        printHelp()
        Foundation.exit(1)
      }
    } catch {
      FileHandle.standardError.write(Data("1Context needs attention: \(error.localizedDescription)\n".utf8))
      Foundation.exit(1)
    }
  }

  static func printMain() async {
    print("""
    1Context \(oneContextVersion)
    Public macOS preview.
    https://github.com/hapticasensorics/1context
    """)
  }

  static func printHelp() {
    print("""
    1Context

    Usage:
      1context
      1context --version
      1context --help
      1context diagnose
      1context uninstall [--delete-data] [--keep-app]
      1context wiki local-url
    """)
  }

  static func rejectUnknownArguments(allowed: Set<String> = []) throws {
    let unknown = args.dropFirst().filter { !allowed.contains($0) }
    if let first = unknown.first {
      throw CLIError.unknownArgument(first)
    }
  }

  static func diagnose() async {
    let redact = true
    let paths = RuntimePaths.current()
    let controller = RuntimeController()
    let health = controller.status()

    print("1Context Diagnose\n")
    print("CLI:")
    print("  Version: \(oneContextVersion)")
    print("  Executable: \(displayPath(currentExecutablePath() ?? CommandLine.arguments[0], redact: redact))")
    print("  App Bundle: /Applications/1Context.app")
    print("  App Version: \(appVersion() ?? "not installed")")

    let readiness = OneContextAppReadiness.current()
    print("\nApp Readiness:")
    for line in OneContextAppReadinessDiagnostics.render(readiness) {
      print("  \(line)")
    }

    print("\nRuntime:")
    switch health {
    case .success(let runtime):
      print("  Health: OK")
      print("  Runtime Version: \(runtime.version)")
      print("  PID: \(runtime.pid)")
      print("  Uptime Seconds: \(runtime.uptimeSeconds)")
    case .failure(let error):
      print("  Health: no response")
      print("  Error: \(error.localizedDescription)")
    }
    print("  User Content: \(displayPath(paths.userContentDirectory.path, redact: redact))")
    print("  App Support: \(displayPath(paths.appSupportDirectory.path, redact: redact))")
    print("  Socket: \(displayPath(paths.socketPath, redact: redact))")

    printLocalWebDiagnostics(redact: redact)

    print("\nSetup:")
    for line in OneContextAppSetupDiagnostics.render(
      readiness.setup,
      redact: { displayPath($0, redact: redact) }
    ) {
      print("  \(line)")
    }

    print("\nLaunchAgents:")
    printLaunchAgent(label: LaunchAgentManager.runtimeLabel, redact: redact)
    printLaunchAgent(label: LaunchAgentManager.menuLabel, redact: redact)

    print("\nUpdate:")
    let updateSnapshot = await appUpdateSnapshot(currentVersion: oneContextVersion)
    for line in AppUpdateDiagnostics.render(updateSnapshot) {
      print(line)
    }

    print("\nLogs:")
    printLogTail(title: "Runtime", path: paths.logPath, redact: redact)
    printLogTail(title: "Menu", path: paths.logDirectory.appendingPathComponent("menu.log").path, redact: redact)
  }

  static func printLocalWebDiagnostics(redact: Bool) {
    let diagnostics = CaddyManager().diagnostics()
    let snapshot = diagnostics.snapshot

    print("\nLocal Web:")
    print("  Health: \(snapshot.running ? "OK" : snapshot.health)")
    print("  URL: \(snapshot.url)")
    print("  URL Mode: \(diagnostics.urlMode)")
    print("  Trust Mode: \(diagnostics.trustMode)")
    print("  Privileged Bind Required: \(yesNo(diagnostics.privilegedBindRequired))")
    for line in LocalWebSetupDiagnostics.render(diagnostics.setup, redact: { displayPath($0, redact: redact) }) {
      print(line)
    }
    print("  API Health: \(diagnostics.apiHealth)")
    print("  API URL: \(diagnostics.apiURL)")
    print("  API Port: \(diagnostics.apiPort)")
    print("  API State: \(displayPath(diagnostics.apiStatePath, redact: redact))")
    if let pid = snapshot.pid {
      print("  PID: \(pid)")
    }
    print("  Caddy: \(diagnostics.caddyExecutableIsExecutable ? "executable" : diagnostics.caddyExecutableExists ? "not executable" : "missing")")
    print("  Caddy Path: \(displayPath(diagnostics.caddyExecutable, redact: redact))")
    print("  Bundled Caddy: \(diagnostics.caddyExecutableIsBundled ? "yes" : "no")")
    print("  Bundled Caddy Path: \(displayPath(diagnostics.bundledCaddyPath, redact: redact))")
    print("  Bundled Caddy Version: \(diagnostics.bundledCaddyVersion)")
    print("  Caddyfile: \(displayPath(diagnostics.caddyfilePath, redact: redact))")
    print("  State: \(displayPath(diagnostics.statePath, redact: redact))")
    print("  PID File: \(displayPath(diagnostics.pidPath, redact: redact))")
    print("  Log: \(displayPath(diagnostics.logPath, redact: redact))")
    print("  Current Site: \(displayPath(diagnostics.currentSitePath, redact: redact))")
    print("  Previous Site: \(displayPath(diagnostics.previousSitePath, redact: redact))")
    print("  Next Site: \(displayPath(diagnostics.nextSitePath, redact: redact))")
    print("  Current Has Index: \(yesNo(diagnostics.currentSiteHasIndex))")
    print("  Current Has Health: \(yesNo(diagnostics.currentSiteHasHealth))")
  }

  static func uninstall() async throws {
    try rejectRootInvocationForAppCleanup()

    let deleteData = args.contains("--delete-data")
    let keepApp = args.contains("--keep-app")
    let menuProcess = args.contains("--menu-process")
    var warnings: [String] = []

    print("Uninstalling 1Context app-owned setup...")

    _ = try? await RuntimeController().quit(stopMenu: !menuProcess)
    CaddyManager().stop()
    print("Removed: Runtime")

    recordCleanupStep("Local Wiki Access", warnings: &warnings) {
      _ = try LocalWebSetupInstaller().uninstall()
    }

    for label in [LaunchAgentManager.menuLabel, LaunchAgentManager.runtimeLabel] {
      recordCleanupStep("LaunchAgent \(label)", warnings: &warnings) {
        try uninstallLaunchAgent(label: label)
      }
    }

    if deleteData {
      recordCleanupStep("User data", warnings: &warnings) {
        try deleteApprovedUserData()
      }
    } else {
      print("Preserved user data. Re-run with --delete-data to remove approved 1Context data paths.")
    }

    if keepApp {
      print("Preserved application bundle.")
    } else {
      recordCleanupStep("Application bundle", warnings: &warnings) {
        _ = try AppBundleTrasher().trash(installedAppBundleURL())
      }
    }

    if warnings.isEmpty {
      print("1Context uninstalled.")
      return
    }

    print("\nUninstall needs attention:")
    for warning in warnings {
      print("- \(warning)")
    }
    throw CLIError.commandFailed("Uninstall completed with cleanup warnings.")
  }

  static func recordCleanupStep(_ title: String, warnings: inout [String], action: () throws -> Void) {
    do {
      try action()
      print("Removed: \(title)")
    } catch {
      warnings.append("\(title): \(error.localizedDescription)")
    }
  }

  static func wiki() async throws {
    guard args.count >= 2 else {
      throw CLIError.commandFailed("wiki requires a subcommand")
    }

    switch args[1] {
    case "local-url":
      try rejectUnknownWikiArguments(allowed: [])
      let diagnostics = CaddyManager().diagnostics()
      guard diagnostics.setup.ready else {
        throw CLIError.commandFailed("""
        Local wiki access is not set up.

        Open 1Context and choose Settings > Setup...
        """)
      }
      print(diagnostics.snapshot.url)
    default:
      throw CLIError.commandFailed("Unknown wiki subcommand: \(args[1])")
    }
  }

  static func rejectUnknownWikiArguments(allowed: Set<String>) throws {
    let unknown = args.dropFirst(2).filter { !allowed.contains($0) }
    if let first = unknown.first {
      throw CLIError.unknownArgument(first)
    }
  }

  static func printLaunchAgent(label: String, redact: Bool = false) {
    let home = FileManager.default.homeDirectoryForCurrentUser
    let plist = home.appendingPathComponent("Library/LaunchAgents/\(label).plist")
    let loaded = launchctlPrint(label: label)
    let loadedFields = loaded.map(launchctlFields) ?? [:]

    print("  \(label):")
    print("    Plist: \(displayPath(plist.path, redact: redact))")
    print("    Plist Exists: \(FileManager.default.fileExists(atPath: plist.path) ? "yes" : "no")")
    print("    Plist Program: \(displayPath(plistProgram(path: plist.path) ?? "missing", redact: redact))")
    print("    Loaded: \(loaded == nil ? "no" : "yes")")
    print("    State: \(loadedFields["state"] ?? "missing")")
    print("    Loaded Program: \(displayPath(loadedFields["program"] ?? "missing", redact: redact))")
    print("    PID: \(loadedFields["pid"] ?? "missing")")
    print("    Minimum Runtime: \(loadedFields["minimum runtime"] ?? "missing")")
    print("    Last Exit Code: \(loadedFields["last exit code"] ?? "missing")")
    print("    Last Signal: \(loadedFields["last terminating signal"] ?? "missing")")
  }

  static func launchctlPrint(label: String) -> String? {
    let result = runCapture("/bin/launchctl", ["print", "gui/\(getuid())/\(label)"])
    guard result.status == 0 else { return nil }
    return result.stdout
  }

  static func launchctlFields(_ output: String) -> [String: String] {
    var fields: [String: String] = [:]
    let wanted = [
      "state",
      "program",
      "pid",
      "minimum runtime",
      "last exit code",
      "last terminating signal"
    ]

    for line in output.split(separator: "\n").map(String.init) {
      let trimmed = line.trimmingCharacters(in: .whitespaces)
      for key in wanted where trimmed.hasPrefix("\(key) =") {
        fields[key] = trimmed.replacingOccurrences(of: "\(key) =", with: "")
          .trimmingCharacters(in: .whitespaces)
      }
    }
    return fields
  }

  static func plistProgram(path: String) -> String? {
    guard let dictionary = NSDictionary(contentsOfFile: path),
      let arguments = dictionary["ProgramArguments"] as? [String],
      let first = arguments.first
    else {
      return nil
    }
    return first
  }

  static func printLogTail(title: String, path: String, lineCount: Int = 5, redact: Bool = false) {
    print("  \(title): \(displayPath(path, redact: redact))")
    guard let text = try? String(contentsOfFile: path, encoding: .utf8) else {
      print("    missing")
      return
    }
    let lines = text.split(separator: "\n").suffix(lineCount)
    if lines.isEmpty {
      print("    empty")
    } else {
      for line in lines {
        print("    \(displayPath(String(line), redact: redact))")
      }
    }
  }

  static func displayPath(_ value: String, redact: Bool) -> String {
    guard redact else { return value }
    let home = FileManager.default.homeDirectoryForCurrentUser.path
    return value
      .replacingOccurrences(of: home, with: "~")
      .replacingOccurrences(of: NSTemporaryDirectory(), with: "$TMPDIR/")
  }

  static func yesNo(_ value: Bool) -> String {
    value ? "yes" : "no"
  }

  static func readTrimmed(_ path: String) -> String? {
    try? String(contentsOfFile: path, encoding: .utf8)
      .trimmingCharacters(in: .whitespacesAndNewlines)
  }

  static func appUpdateSnapshot(currentVersion: String) async -> AppUpdateSnapshot {
    let appBundleURL = installedAppBundleURL()
    return await SparkleUpdateSnapshotProvider(
      configuration: SparkleUpdaterConfiguration(appBundleURL: appBundleURL),
      appContext: AppUpdateContext(
        bundleURL: appBundleURL,
        executableURL: appBundleURL.appendingPathComponent("Contents/MacOS/1Context")
      ),
      driver: CLIAppManagedSparkleDriver()
    ).snapshot(currentVersion: currentVersion)
  }

  static func installedAppBundleURL() -> URL {
    URL(fileURLWithPath: "/Applications/1Context.app", isDirectory: true)
  }

  static func appVersion() -> String? {
    let infoPlist = installedAppBundleURL()
      .appendingPathComponent("Contents/Info.plist")
      .path
    return NSDictionary(contentsOfFile: infoPlist)?["CFBundleShortVersionString"] as? String
  }

  static func rejectRootInvocationForAppCleanup() throws {
    if ProcessPrivilegePolicy.rejectsAppOwnedUserLifecycle() {
      throw CLIError.commandFailed("Run 1Context uninstall as your normal macOS user, not with sudo or as root.")
    }
  }

  static func uninstallLaunchAgent(label: String) throws {
    let fileManager = FileManager.default
    let home = try uninstallHomeDirectory()
    let plist = home
      .appendingPathComponent("Library/LaunchAgents/\(label).plist")
    _ = runCapture("/bin/launchctl", ["bootout", "gui/\(getuid())/\(label)"])
    _ = runCapture("/bin/launchctl", ["bootout", "gui/\(getuid())", plist.path])
    if fileManager.fileExists(atPath: plist.path) {
      try fileManager.removeItem(at: plist)
    }
  }

  static func deleteApprovedUserData() throws {
    let fileManager = FileManager.default
    let home = try uninstallHomeDirectory()
    let relativePaths = [
      "1Context",
      "Library/Application Support/1Context",
      "Library/Logs/1Context",
      "Library/Caches/1Context",
      "Library/Caches/com.haptica.1context",
      "Library/Caches/com.haptica.1context.menu",
      "Library/HTTPStorages/com.haptica.1context",
      "Library/HTTPStorages/com.haptica.1context.binarycookies",
      "Library/HTTPStorages/1context",
      "Library/HTTPStorages/1context.binarycookies",
      "Library/HTTPStorages/com.haptica.1context.menu",
      "Library/HTTPStorages/com.haptica.1context.menu.binarycookies",
      "Library/Preferences/com.haptica.1context.plist",
      "Library/Saved Application State/com.haptica.1context.savedState",
      "Library/Saved Application State/com.haptica.1context.menu.savedState",
      "Library/WebKit/com.haptica.1context",
      "Library/WebKit/com.haptica.1context.menu"
    ]

    for relativePath in relativePaths {
      try removeApprovedUserPath(home.appendingPathComponent(relativePath), home: home, fileManager: fileManager)
    }
    try removeApprovedTemporaryFiles(fileManager: fileManager)
  }

  static func uninstallHomeDirectory() throws -> URL {
    FileManager.default.homeDirectoryForCurrentUser.standardizedFileURL
  }

  static func removeApprovedUserPath(_ url: URL, home: URL, fileManager: FileManager) throws {
    let target = url.standardizedFileURL.path
    let homePath = home.standardizedFileURL.path
    guard target.hasPrefix(homePath + "/"), target != homePath, target != "/" else {
      throw CLIError.commandFailed("Refusing to delete unsafe path: \(target)")
    }
    guard fileManager.fileExists(atPath: target) else { return }
    try fileManager.removeItem(atPath: target)
  }

  static func removeApprovedTemporaryFiles(fileManager: FileManager) throws {
    let temporaryDirectory = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
    guard let children = try? fileManager.contentsOfDirectory(at: temporaryDirectory, includingPropertiesForKeys: nil) else {
      return
    }
    for child in children {
      let name = child.lastPathComponent
      guard (name.hasPrefix("1context-") && name.hasSuffix(".command"))
        || name.hasPrefix("1context-update-")
      else {
        continue
      }
      try fileManager.removeItem(at: child)
    }
  }

  static func currentExecutablePath() -> String? {
    var size = UInt32(0)
    _NSGetExecutablePath(nil, &size)
    var buffer = [CChar](repeating: 0, count: Int(size))
    guard _NSGetExecutablePath(&buffer, &size) == 0 else { return nil }
    let pathBytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    let path = String(decoding: pathBytes, as: UTF8.self)
    return URL(fileURLWithPath: path).resolvingSymlinksInPath().path
  }

  static func runCapture(_ executable: String, _ arguments: [String]) -> (status: Int32, stdout: String, stderr: String) {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr

    do {
      try process.run()
      process.waitUntilExit()
    } catch {
      return (1, "", error.localizedDescription)
    }

    let stdoutData = stdout.fileHandleForReading.readDataToEndOfFile()
    let stderrData = stderr.fileHandleForReading.readDataToEndOfFile()
    return (
      process.terminationStatus,
      String(data: stdoutData, encoding: .utf8) ?? "",
      String(data: stderrData, encoding: .utf8) ?? ""
    )
  }

}

enum CLIError: Error, LocalizedError {
  case commandFailed(String)
  case unknownArgument(String)

  var errorDescription: String? {
    switch self {
    case .commandFailed(let command):
      return "Command failed: \(command)"
    case .unknownArgument(let argument):
      return "Unknown argument: \(argument)"
    }
  }
}

private struct CLIAppManagedSparkleDriver: SparkleUpdateDriver, Sendable {
  func snapshot(
    currentVersion: String,
    configuration: SparkleUpdaterConfiguration
  ) async -> SparkleUpdateDriverSnapshot {
    SparkleUpdateDriverSnapshot(
      availability: .available,
      latestVersion: nil,
      updateAvailable: false,
      canInstallUpdates: false,
      userFacingStatus: "1Context app updates are managed by the installed app.",
      nextAction: "Open 1Context from /Applications and choose Check for Updates."
    )
  }
}
