import Darwin
import Foundation
import OneContextRuntimeSupport

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
      case "start":
        try rejectUnknownArguments(allowed: ["--debug"])
        try await start()
      case "stop":
        try rejectUnknownArguments(allowed: ["--debug"])
        try await stop()
      case "quit":
        try rejectUnknownArguments(allowed: ["--debug"])
        try await quit()
      case "restart":
        try rejectUnknownArguments(allowed: ["--debug"])
        try await restart()
      case "status":
        try rejectUnknownArguments(allowed: ["--debug"])
        await status()
      case "diagnose", "debug":
        try rejectUnknownArguments(allowed: ["--no-redact"])
        await diagnose()
      case "logs":
        try rejectUnknownArguments(allowed: ["--follow"])
        try logs()
      case "update":
        try rejectUnknownArguments()
        try await update()
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
    await maybeCheckForUpdate()
  }

  static func printHelp() {
    print("""
    1Context

    Usage:
      1context
      1context --version
      1context --help
      1context start [--debug]
      1context stop [--debug]
      1context quit [--debug]
      1context restart [--debug]
      1context status [--debug]
      1context diagnose [--no-redact]
      1context debug [--no-redact]
      1context logs [--follow]
      1context update
    """)
  }

  static func rejectUnknownArguments(allowed: Set<String> = []) throws {
    let unknown = args.dropFirst().filter { !allowed.contains($0) }
    if let first = unknown.first {
      throw CLIError.unknownArgument(first)
    }
  }

  static func maybeCheckForUpdate() async {
    do {
      let result = try await UpdateChecker().check(currentVersion: oneContextVersion)
      if result.updateAvailable, let latest = result.latest {
        FileHandle.standardError.write(Data("""
        1Context \(latest.version) is available. You have \(oneContextVersion).
        Update: \(oneContextHomebrewUpdateCommand)

        """.utf8))
      }
    } catch {
      // Network and parsing failures stay silent.
    }
  }

  static func printDebug(controller: RuntimeController, error: Error?) async {
    let paths = RuntimePaths.current()
    let launchAgent = await controller.launchAgentState()
    print("""

    Runtime:
      LaunchAgent: \(launchAgent.loaded ? "loaded" : launchAgent.configured ? "installed" : "not installed")
      Process: \(error == nil ? "running" : "not confirmed")
      Socket: \(error == nil ? "responding" : "no response")
      User Content: \(paths.userContentDirectory.path)
      App Support: \(paths.appSupportDirectory.path)
      Socket Path: \(paths.socketPath)
      Log: \(paths.logPath)
      Cache: \(paths.cacheDirectory.path)
    """)
  }

  static func start() async throws {
    let debug = args.contains("--debug")
    let startedAt = Date()
    let controller = RuntimeController()
    do {
      let result = try await controller.start()
      print(result.alreadyRunning ? "1Context is already running." : "1Context is running.")
      if debug { await printLifecycleDebug(controller: controller, startedAt: startedAt, error: nil) }
    } catch {
      if debug { await printLifecycleDebug(controller: controller, startedAt: startedAt, error: error) }
      throw error
    }
  }

  static func stop() async throws {
    let debug = args.contains("--debug")
    let startedAt = Date()
    let controller = RuntimeController()
    do {
      let stopped = try await controller.stop()
      print(stopped ? "1Context is stopped." : "1Context is not running.")
      if debug { await printLifecycleDebug(controller: controller, startedAt: startedAt, error: CLIError.runtimeStopped) }
    } catch {
      if debug { await printLifecycleDebug(controller: controller, startedAt: startedAt, error: error) }
      throw error
    }
  }

  static func quit() async throws {
    let debug = args.contains("--debug")
    let startedAt = Date()
    let controller = RuntimeController()
    do {
      _ = try await controller.quit()
      print("1Context quit.")
      if debug { await printLifecycleDebug(controller: controller, startedAt: startedAt, error: CLIError.runtimeStopped) }
    } catch {
      if debug { await printLifecycleDebug(controller: controller, startedAt: startedAt, error: error) }
      throw error
    }
  }

  static func restart() async throws {
    let debug = args.contains("--debug")
    let startedAt = Date()
    let controller = RuntimeController()
    do {
      _ = try await controller.restart()
      print("1Context is running.")
      if debug { await printLifecycleDebug(controller: controller, startedAt: startedAt, error: nil) }
    } catch {
      if debug { await printLifecycleDebug(controller: controller, startedAt: startedAt, error: error) }
      throw error
    }
  }

  static func printLifecycleDebug(controller: RuntimeController, startedAt: Date, error: Error?) async {
    let elapsed = Date().timeIntervalSince(startedAt)
    print("\nCompleted in \(String(format: "%.2f", elapsed))s.")
    await printDebug(controller: controller, error: error)
  }

  static func update() async throws {
    let result = try await UpdateChecker().check(force: true, currentVersion: oneContextVersion)
    guard result.updateAvailable else {
      print("1Context up to date.")
      return
    }

    if let latest = result.latest {
      print("1Context \(latest.version) is available. You have \(oneContextVersion).")
    }
    print("Updating 1Context...")
    try updateWithHomebrew()
  }

  static func status() async {
    let debug = args.contains("--debug")
    let controller = RuntimeController()
    switch controller.status() {
    case .success(let health):
      let menuStatus = launchAgentSummary(label: LaunchAgentManager.menuLabel)
      print("""
      1Context is running.

      Version: \(health.version)
      Health: OK
      Menu Bar: \(menuStatus.userFacingStatus)
      """)
      if !menuStatus.running {
        print("""

        Start the menu bar with:
          1context start
        """)
      }
      if debug { await printDebug(controller: controller, error: nil) }
    case .failure(let error):
      FileHandle.standardError.write(Data("""
      1Context is not running.

      Start it with:
        1context start
      """.utf8))
      if debug { await printDebug(controller: controller, error: error) }
      Foundation.exit(1)
    }
  }

  static func diagnose() async {
    let redact = !args.contains("--no-redact")
    let paths = RuntimePaths.current()
    let controller = RuntimeController()
    let health = controller.status()
    let updateState = readJSON(paths: UpdateStatePaths.current().file)

    print("1Context Diagnose\n")
    print("CLI:")
    print("  Version: \(oneContextVersion)")
    print("  Executable: \(displayPath(currentExecutablePath() ?? CommandLine.arguments[0], redact: redact))")
    print("  App Bundle: /Applications/1Context.app")
    print("  App Version: \(appVersion() ?? "not installed")")

    print("\nRuntime:")
    print("  Desired State: \(readTrimmed(paths.desiredStatePath) ?? "missing")")
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

    print("\nLaunchAgents:")
    printLaunchAgent(label: LaunchAgentManager.runtimeLabel, redact: redact)
    printLaunchAgent(label: LaunchAgentManager.menuLabel, redact: redact)

    print("\nUpdate:")
    print("  Cache: \(displayPath(UpdateStatePaths.current().file.path, redact: redact))")
    print("  Last Checked: \(updateState?["last_checked_at"] as? String ?? "missing")")
    print("  Latest Seen: \(updateState?["last_seen_latest"] as? String ?? "missing")")
    print("  Notes URL: \(updateState?["notes_url"] as? String ?? "missing")")

    print("\nLogs:")
    printLogTail(title: "Runtime", path: paths.logPath, redact: redact)
    printLogTail(title: "Menu", path: paths.logDirectory.appendingPathComponent("menu.log").path, redact: redact)
  }

  static func logs() throws {
    let paths = RuntimePaths.current()
    let runtimeLog = paths.logPath
    let menuLog = paths.logDirectory.appendingPathComponent("menu.log").path

    if args.contains("--follow") {
      try runProcess("/usr/bin/tail", ["-n", "80", "-F", runtimeLog, menuLog])
      return
    }

    print("1Context Logs\n")
    printLogTail(title: "Runtime", path: runtimeLog, lineCount: 80)
    printLogTail(title: "Menu", path: menuLog, lineCount: 80)
  }

  static func updateWithHomebrew() throws {
    guard let brew = firstExecutable(["/opt/homebrew/bin/brew", "/usr/local/bin/brew"]) else {
      throw CLIError.commandFailed("Homebrew is required to update 1Context")
    }

    print("Checking Homebrew...")
    print("Checking 1Context tap...")
    var tap = runCapture(brew, ["--repo", "hapticasensorics/tap"]).stdout
      .trimmingCharacters(in: .whitespacesAndNewlines)
    if tap.isEmpty {
      try runProcess(brew, ["tap", "hapticasensorics/tap"])
      tap = runCapture(brew, ["--repo", "hapticasensorics/tap"]).stdout
        .trimmingCharacters(in: .whitespacesAndNewlines)
    }
    guard !tap.isEmpty else {
      throw CLIError.commandFailed("brew --repo hapticasensorics/tap")
    }

    print("Refreshing 1Context cask metadata...")
    try runProcess("/usr/bin/git", [
      "-C", tap,
      "fetch", "--quiet", "--no-tags", "origin", "main:refs/remotes/origin/main"
    ])
    try runProcess("/usr/bin/git", [
      "-C", tap,
      "merge", "--quiet", "--ff-only", "refs/remotes/origin/main"
    ])

    print("Installing 1Context...")
    try runProcess(
      brew,
      ["upgrade", "--cask", "hapticasensorics/tap/1context"],
      environment: [
        "HOMEBREW_NO_AUTO_UPDATE": "1",
        "HOMEBREW_NO_INSTALL_CLEANUP": "1"
      ]
    )

    if RuntimeController().shouldAutoStartRuntime(),
      let cli = firstExecutable(["/opt/homebrew/bin/1context", "/usr/local/bin/1context"])
    {
      _ = runCapture(cli, ["restart"])
      try runProcess(cli, ["--version"])
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

  static func launchAgentSummary(label: String) -> (loaded: Bool, running: Bool, userFacingStatus: String) {
    guard let output = launchctlPrint(label: label) else {
      return (false, false, "not loaded")
    }
    let fields = launchctlFields(output)
    if let pid = fields["pid"], !pid.isEmpty {
      return (true, true, "running")
    }
    let state = fields["state"] ?? "loaded"
    return (true, false, "\(state), no process")
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
    return value.replacingOccurrences(of: home, with: "~")
  }

  static func readTrimmed(_ path: String) -> String? {
    try? String(contentsOfFile: path, encoding: .utf8)
      .trimmingCharacters(in: .whitespacesAndNewlines)
  }

  static func readJSON(paths path: URL) -> [String: Any]? {
    guard let data = try? Data(contentsOf: path) else { return nil }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
  }

  static func appVersion() -> String? {
    NSDictionary(contentsOfFile: "/Applications/1Context.app/Contents/Info.plist")?["CFBundleShortVersionString"] as? String
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

  static func firstExecutable(_ candidates: [String]) -> String? {
    candidates.first { FileManager.default.isExecutableFile(atPath: $0) }
  }

  static func runProcess(
    _ executable: String,
    _ arguments: [String],
    environment: [String: String] = [:]
  ) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    if !environment.isEmpty {
      process.environment = ProcessInfo.processInfo.environment.merging(environment) { _, new in new }
    }
    process.standardOutput = FileHandle.standardOutput
    process.standardError = FileHandle.standardError
    try process.run()
    process.waitUntilExit()
    guard process.terminationStatus == 0 else {
      throw CLIError.commandFailed(([executable] + arguments).joined(separator: " "))
    }
  }
}

enum CLIError: Error, LocalizedError {
  case commandFailed(String)
  case runtimeStopped
  case unknownArgument(String)

  var errorDescription: String? {
    switch self {
    case .commandFailed(let command):
      return "Command failed: \(command)"
    case .runtimeStopped:
      return "1Context is stopped"
    case .unknownArgument(let argument):
      return "Unknown argument: \(argument)"
    }
  }
}
