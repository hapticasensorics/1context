import Darwin
import Foundation
import OneContextAgent
import OneContextInstall
import OneContextLocalWeb
import OneContextMemoryCore
import OneContextRuntimeSupport
import OneContextSetup
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
      case "permissions":
        try rejectUnknownArguments()
        printRequiredSetup()
      case "update":
        try rejectUnknownArguments()
        try await update()
      case "uninstall":
        try rejectUnknownArguments(allowed: ["--delete-data", "--keep-app", "--menu-process"])
        try await uninstall()
      case "setup":
        try setup()
      case "agent":
        try agent()
      case "memory-core":
        try memoryCore()
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
      1context permissions
      1context update
      1context uninstall [--delete-data] [--keep-app]
      1context setup local-web <status|install|repair|uninstall>
      1context agent hook --provider <claude|codex> --event <event>
      1context agent statusline --provider <claude|codex>
      1context agent integrations <status|install|repair|uninstall>
      1context memory-core <status|doctor|configure|run>
      1context wiki <local-url|refresh>
    """)
  }

  static func rejectUnknownArguments(allowed: Set<String> = []) throws {
    let unknown = args.dropFirst().filter { !allowed.contains($0) }
    if let first = unknown.first {
      throw CLIError.unknownArgument(first)
    }
  }

  static func maybeCheckForUpdate() async {
    // Native update checks are app-owned. The CLI stays quiet until that
    // adapter is configured.
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
    printLocalWebDiagnostics(redact: false)
  }

  static func start() async throws {
    let debug = args.contains("--debug")
    let startedAt = Date()
    let controller = RuntimeController()
    do {
      try ensureRequiredSetupForUse()
      if debug {
        let result = try await controller.start()
        recordCurrentWikiURL()
        print(result.alreadyRunning ? "1Context is already running." : "1Context is running.")
      } else {
        try await controller.requestStart()
        recordWikiURL(LocalWebDefaults.defaultWikiURL)
        print("1Context Remembering.")
      }
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
      if debug {
        let stopped = try await controller.stop()
        print(stopped ? "1Context is stopped." : "1Context is not running.")
      } else {
        try await controller.requestStop()
        print("1Context Stopped.")
      }
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
      CaddyManager().stop()
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
      try ensureRequiredSetupForUse()
      _ = try await controller.restart()
      recordCurrentWikiURL()
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
    let snapshot = await nativeUpdateSnapshot(currentVersion: effectiveCurrentVersion())
    for line in NativeUpdateDiagnostics.render(snapshot) {
      print(line.trimmingCharacters(in: .whitespaces))
    }
    guard snapshot.availability == .available else {
      throw CLIError.commandFailed(snapshot.nextAction)
    }
    print(snapshot.userFacingStatus)
  }

  static func status() async {
    let debug = args.contains("--debug")
    let controller = RuntimeController()
    switch controller.status() {
    case .success(let health):
      let menuStatus = launchAgentSummary(label: LaunchAgentManager.menuLabel)
      guard health.version == oneContextVersion else {
        FileHandle.standardError.write(Data("""
        1Context needs attention.

        Runtime version \(health.version) does not match CLI version \(oneContextVersion).
        Restart it with:
          1context restart
        """.utf8))
        if debug { await printDebug(controller: controller, error: RuntimeControlError.launchAgentFailed("runtime version mismatch")) }
        Foundation.exit(1)
      }
      let setupReady = health.requiredSetupReady ?? true
      print("""
      1Context is running.

      Version: \(health.version)
      Health: \(setupReady ? "OK" : "Needs Setup")
      Menu Bar: \(menuStatus.userFacingStatus)
      """)
      if !setupReady {
        print("""

        Required Setup: \(health.requiredSetupSummary ?? "1Context setup is incomplete")
        Open 1Context and choose Settings > Setup...
        """)
      }
      recordCurrentWikiURL()
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

    print("1Context Diagnose\n")
    print("CLI:")
    print("  Version: \(oneContextVersion)")
    print("  Executable: \(displayPath(currentExecutablePath() ?? CommandLine.arguments[0], redact: redact))")
    print("  App Bundle: /Applications/1Context.app")
    print("  App Version: \(appVersion() ?? "not installed")")

    let readiness = OneContextAppReadiness.current(
      nativeUpdate: await nativeUpdateSnapshot(currentVersion: oneContextVersion)
    )
    print("\nApp Readiness:")
    for line in OneContextAppReadinessDiagnostics.render(readiness) {
      print("  \(line)")
    }

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

    printLocalWebDiagnostics(redact: redact)

    print("\nPermissions:")
    printPermissionDiagnostics()

    print("\nLaunchAgents:")
    printLaunchAgent(label: LaunchAgentManager.runtimeLabel, redact: redact)
    printLaunchAgent(label: LaunchAgentManager.menuLabel, redact: redact)

    print("\nUpdate:")
    let updateSnapshot: NativeUpdateSnapshot
    if let nativeUpdate = readiness.nativeUpdate {
      updateSnapshot = nativeUpdate
    } else {
      updateSnapshot = await nativeUpdateSnapshot(currentVersion: oneContextVersion)
    }
    for line in NativeUpdateDiagnostics.render(updateSnapshot) {
      print(line)
    }

    print("\nMemory Core:")
    printMemoryCoreStatus(MemoryCoreAdapter().status(forceCheck: false), redact: redact)

    print("\nLogs:")
    printLogTail(title: "Runtime", path: paths.logPath, redact: redact)
    printLogTail(title: "Menu", path: paths.logDirectory.appendingPathComponent("menu.log").path, redact: redact)
  }

  static func printLocalWebDiagnostics(redact: Bool) {
    let diagnostics = CaddyManager().diagnostics()
    let snapshot = diagnostics.snapshot
    recordWikiURL(snapshot.url)

    print("\nLocal Web:")
    print("  Health: \(snapshot.running ? "OK" : snapshot.health)")
    print("  URL: \(snapshot.url)")
    print("  URL Mode: \(diagnostics.urlMode)")
    print("  Trust Mode: \(diagnostics.trustMode)")
    print("  Privileged Bind Required: \(yesNo(diagnostics.privilegedBindRequired))")
    for line in LocalWebSetupDiagnostics.render(diagnostics.setup) {
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
    print("  Current Has Theme: \(yesNo(diagnostics.currentSiteHasTheme))")
    print("  Current Has Enhance JS: \(yesNo(diagnostics.currentSiteHasEnhanceJS))")
    print("  Current Has Health: \(yesNo(diagnostics.currentSiteHasHealth))")
  }

  static func printRequiredSetup() {
    print("1Context Setup\n")
    let readiness = OneContextAppReadiness.current()
    for line in OneContextAppReadinessDiagnostics.render(readiness) {
      print(line)
    }
    print("")
    for line in OneContextAppSetupDiagnostics.render(readiness.setup) {
      print(line)
    }
  }

  static func ensureRequiredSetupForUse() throws {
    let ready: Bool
    if LocalWebURLMode(environmentValue: ProcessInfo.processInfo.environment["ONECONTEXT_WIKI_URL_MODE"]) == .highPortHTTP {
      ready = OneContextAppReadiness.current().requiredSetupReady
    } else {
      ready = OneContextAppSetup.current().requiredReady
    }
    guard ready else {
      throw CLIError.commandFailed("""
      Local wiki access is not set up.

      Open 1Context and choose Settings > Setup..., or run:
        1context setup local-web install
      """)
    }
  }

  @discardableResult
  static func recordCurrentWikiURL() -> String {
    let url = CaddyManager().status().url
    recordWikiURL(url)
    return url
  }

  static func recordWikiURL(_ url: String) {
    try? AgentConfigStore.writeWikiURL(url)
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

  static func agent() throws {
    guard args.count >= 2 else {
      throw CLIError.commandFailed("agent requires a subcommand")
    }

    switch args[1] {
    case "hook":
      try agentHook()
    case "integrations":
      try agentIntegrations()
    case "statusline":
      try agentStatusLine()
    default:
      throw CLIError.commandFailed("Unknown agent subcommand: \(args[1])")
    }
  }

  static func setup() throws {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("Usage: 1context setup local-web <status|install|repair|uninstall>")
    }

    switch args[1] {
    case "local-web":
      try localWebSetup()
    default:
      throw CLIError.commandFailed("Unknown setup domain: \(args[1])")
    }
  }

  static func localWebSetup() throws {
    guard args.count == 3 else {
      throw CLIError.commandFailed("Usage: 1context setup local-web <status|install|repair|uninstall>")
    }
    let installer = LocalWebSetupInstaller()
    switch args[2] {
    case "status":
      print("1Context Local HTTPS Setup\n")
      for line in LocalWebSetupDiagnostics.render(installer.status()) {
        print(line.trimmingCharacters(in: .whitespaces))
      }
    case "install":
      print("Preparing 1Context local HTTPS setup...")
      print("macOS may ask you to grant 1Context network permissions.")
      let result = try installer.install()
      print("\nLocal HTTPS setup complete.")
      for line in LocalWebSetupDiagnostics.render(result.setup) {
        print(line.trimmingCharacters(in: .whitespaces))
      }
      if let localWeb = result.localWeb {
        print("URL: \(localWeb.url)")
      }
    case "repair":
      print("Repairing 1Context local HTTPS setup...")
      print("macOS may ask you to grant 1Context network permissions.")
      let result = try installer.install()
      print("\nLocal HTTPS setup repaired.")
      for line in LocalWebSetupDiagnostics.render(result.setup) {
        print(line.trimmingCharacters(in: .whitespaces))
      }
      if let localWeb = result.localWeb {
        print("URL: \(localWeb.url)")
      }
    case "uninstall":
      print("Removing 1Context local HTTPS setup...")
      let result = try installer.uninstall()
      print("\nLocal HTTPS setup removed.")
      for line in LocalWebSetupDiagnostics.render(result.setup) {
        print(line.trimmingCharacters(in: .whitespaces))
      }
    default:
      throw CLIError.commandFailed("Unknown local-web setup command: \(args[2])")
    }
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

    recordCleanupStep("Agent integrations", warnings: &warnings) {
      _ = try agentIntegrationManager().uninstall()
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
        _ = try AppBundleTrasher(environment: ProcessInfo.processInfo.environment).trash(installedAppBundleURL())
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

  static func agentHook() throws {
    let values = Array(args.dropFirst(2))
    let providerValue = try optionValue("--provider", in: values)
    let eventValue = try optionValue("--event", in: values)
    try rejectUnknownAgentOptions(values, allowed: ["--provider", "--event"])

    guard let provider = AgentProvider(rawValue: providerValue) else {
      throw CLIError.commandFailed("Unsupported agent provider: \(providerValue)")
    }
    guard let event = AgentHookEvent(rawValue: eventValue) else {
      throw CLIError.commandFailed("Unsupported agent hook event: \(eventValue)")
    }

    let input = FileHandle.standardInput.readDataToEndOfFile()
    let agentEnvironment = oneContextAgentRuntimeEnvironment()
    let paths = RuntimePaths.current(environment: agentEnvironment)
    let executor = AgentHookExecutor(
      paths: AgentPaths.current(environment: agentEnvironment),
      userContentDirectory: paths.userContentDirectory,
      environment: agentEnvironment
    )
    let output = executor.execute(provider: provider, event: event, inputData: input)
    let encoder = JSONEncoder()
    let data = try encoder.encode(output)
    FileHandle.standardOutput.write(data)
    FileHandle.standardOutput.write(Data("\n".utf8))
  }

  static func agentStatusLine() throws {
    let values = Array(args.dropFirst(2))
    let providerValue = try optionValue("--provider", in: values)
    try rejectUnknownAgentOptions(values, allowed: ["--provider"])

    guard let provider = AgentProvider(rawValue: providerValue) else {
      throw CLIError.commandFailed("Unsupported agent provider: \(providerValue)")
    }

    let input = FileHandle.standardInput.readDataToEndOfFile()
    let agentEnvironment = oneContextAgentRuntimeEnvironment()
    let output = AgentStatusLineRenderer(
      paths: AgentPaths.current(environment: agentEnvironment),
      environment: agentEnvironment
    ).render(provider: provider, inputData: input)
    print(output)
  }

  static func agentIntegrationManager() -> AgentIntegrationManager {
    AgentIntegrationManager(
      paths: AgentPaths.current(environment: oneContextAgentRuntimeEnvironment()),
      claudeSettingsPath: AgentIntegrationManager.defaultClaudeSettingsPath(),
      executablePath: AgentIntegrationManager.preferredExecutablePath(
        currentExecutablePath: currentExecutablePath() ?? CommandLine.arguments[0]
      )
    )
  }

  static func agentIntegrations() throws {
    guard args.count == 3 else {
      throw CLIError.commandFailed("Usage: 1context agent integrations <status|install|repair|uninstall>")
    }

    let manager = agentIntegrationManager()

    let report: AgentIntegrationsReport
    switch args[2] {
    case "status":
      report = manager.status()
    case "install":
      report = try manager.install()
    case "repair":
      report = try manager.repair()
    case "uninstall":
      report = try manager.uninstall()
    default:
      throw CLIError.commandFailed("Unknown integrations command: \(args[2])")
    }

    print(manager.render(report))
  }

  static func memoryCore() throws {
    guard args.count >= 2 else {
      throw CLIError.commandFailed("memory-core requires a subcommand")
    }

    let adapter = MemoryCoreAdapter()
    switch args[1] {
    case "status":
      guard args.count == 2 else { throw CLIError.commandFailed("Usage: 1context memory-core status") }
      print("Memory Core\n")
      try printMemoryCoreStatusAndFailIfDegraded(adapter.status(forceCheck: true), redact: false)
    case "doctor":
      guard args.count == 2 else { throw CLIError.commandFailed("Usage: 1context memory-core doctor") }
      print("Memory Core Doctor\n")
      try printMemoryCoreStatusAndFailIfDegraded(adapter.doctor(), redact: false)
    case "configure":
      try memoryCoreConfigure(adapter: adapter)
    case "run":
      try memoryCoreRun(adapter: adapter)
    default:
      throw CLIError.commandFailed("Unknown memory-core subcommand: \(args[1])")
    }
  }

  static func memoryCoreConfigure(adapter: MemoryCoreAdapter) throws {
    let values = Array(args.dropFirst(2))
    if values == ["--clear"] {
      printMemoryCoreStatus(try adapter.clear(), redact: false)
      return
    }
    let executable = try optionValue("--executable", in: values)
    try rejectUnknownAgentOptions(values, allowed: ["--executable"])
    try printMemoryCoreStatusAndFailIfDegraded(try adapter.configure(executable: executable), redact: false)
  }

  static func memoryCoreRun(adapter: MemoryCoreAdapter) throws {
    guard args.count >= 4, args[2] == "--" else {
      throw CLIError.commandFailed("Usage: 1context memory-core run -- <memory-core args...>")
    }
    let runArgs = Array(args.dropFirst(3))
    let result = try adapter.run(arguments: runArgs)
    print(result.stdout, terminator: result.stdout.hasSuffix("\n") ? "" : "\n")
  }

  static func wiki() async throws {
    guard args.count >= 2 else {
      throw CLIError.commandFailed("wiki requires a subcommand")
    }

    switch args[1] {
    case "local-url":
      try rejectUnknownWikiArguments(allowed: [])
      try ensureRequiredSetupForUse()
      _ = try await RuntimeController().start()
      let snapshot = try ensureLocalWebEdgeForCLIWiki()
      print(snapshot.url)
    case "refresh":
      try rejectUnknownWikiArguments(allowed: [])
      try ensureRequiredSetupForUse()
      _ = try await RuntimeController().start()
      _ = try ensureLocalWebEdgeForCLIWiki()
      _ = try await wikiRPC("wiki.refresh", timeout: 5)
      let snapshot = try await waitForWikiRunning(timeout: 240)
      print("Refreshed 1Context wiki.")
      print("URL: \(snapshot.url)")
    default:
      throw CLIError.commandFailed("Unknown wiki subcommand: \(args[1])")
    }
  }

  static func ensureLocalWebEdgeForCLIWiki() throws -> LocalWebSnapshot {
    let manager = CaddyManager()
    let current = manager.status()
    let snapshot = current.running ? current : try manager.start()
    try? AgentConfigStore.writeWikiURL(snapshot.url)
    return snapshot
  }

  static func waitForWikiRunning(timeout: TimeInterval) async throws -> LocalWebSnapshot {
    let deadline = Date().addingTimeInterval(timeout)
    var last = LocalWebSnapshot(running: false, health: "starting")
    repeat {
      last = try await wikiRPC("wiki.status", timeout: 5)
      if last.running { return last }
      try await Task.sleep(nanoseconds: 500_000_000)
    } while Date() < deadline
    throw CLIError.commandFailed("Timed out preparing local wiki. Last state: \(last.health)")
  }

  static func wikiRPC(_ method: String, timeout: TimeInterval = 60) async throws -> LocalWebSnapshot {
    let deadline = Date().addingTimeInterval(timeout)
    var lastError: Error?
    let clientTimeout = Int32(max(2_000, min(120_000, Int(timeout * 1_000))))
    repeat {
      do {
        let result = try UnixJSONRPCClient(timeoutMilliseconds: clientTimeout).call(method: method)
        return wikiSnapshot(from: result)
      } catch {
        lastError = error
        try await Task.sleep(nanoseconds: 250_000_000)
      }
    } while Date() < deadline
    throw lastError ?? CLIError.commandFailed(method)
  }

  static func wikiSnapshot(from payload: [String: Any]) -> LocalWebSnapshot {
    LocalWebSnapshot(
      running: payload["running"] as? Bool ?? false,
      url: payload["url"] as? String ?? LocalWebDefaults.defaultWikiURL,
      pid: (payload["pid"] as? NSNumber)?.int32Value,
      route: payload["route"] as? String ?? LocalWebDefaults.wikiRoute,
      health: payload["health"] as? String ?? "unknown",
      lastError: payload["lastError"] as? String
    )
  }

  static func rejectUnknownWikiArguments(allowed: Set<String>) throws {
    let unknown = args.dropFirst(2).filter { !allowed.contains($0) }
    if let first = unknown.first {
      throw CLIError.unknownArgument(first)
    }
  }

  static func printMemoryCoreStatusAndFailIfDegraded(_ status: MemoryCoreStatus, redact: Bool) throws {
    printMemoryCoreStatus(status, redact: redact)
    if status.health == .degraded {
      throw CLIError.commandFailed("Memory core health is degraded")
    }
  }

  static func printMemoryCoreStatus(_ status: MemoryCoreStatus, redact: Bool) {
    print("  Configured: \(status.configured ? "yes" : "no")")
    print("  Enabled: \(status.enabled ? "yes" : "no")")
    print("  Executable: \(status.executable.map { displayPath($0, redact: redact) } ?? "missing")")
    print("  Health: \(status.health.rawValue)")
    print("  Config: \(displayPath(status.paths.configFile.path, redact: redact))")
    print("  State: \(displayPath(status.paths.stateFile.path, redact: redact))")
    print("  Log: \(displayPath(status.paths.logFile.path, redact: redact))")
    if let lastCheckedAt = status.lastCheckedAt {
      print("  Last Checked: \(ISO8601DateFormatter().string(from: lastCheckedAt))")
    }
    if let lastError = status.lastError {
      print("  Last Error: \(displayPath(lastError, redact: redact))")
    }
  }

  static func printPermissionDiagnostics() {
    let snapshots = PermissionReporter().snapshots()
    for line in PermissionDiagnostics.render(snapshots) {
      print(line)
    }
  }

  static func optionValue(_ name: String, in values: [String]) throws -> String {
    guard let index = values.firstIndex(of: name), index + 1 < values.count else {
      throw CLIError.commandFailed("Missing \(name)")
    }
    let value = values[index + 1]
    guard !value.hasPrefix("--") else {
      throw CLIError.commandFailed("Missing value for \(name)")
    }
    return value
  }

  static func rejectUnknownAgentOptions(_ values: [String], allowed: Set<String>) throws {
    var index = 0
    while index < values.count {
      let value = values[index]
      guard allowed.contains(value) else {
        throw CLIError.unknownArgument(value)
      }
      index += 2
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
    let fallbackProgram = label == LaunchAgentManager.menuLabel
      ? "/Applications/1Context.app/Contents/MacOS/1Context"
      : nil
    guard let output = launchctlPrint(label: label) else {
      if let fallbackProgram, processIsRunning(executablePath: fallbackProgram) {
        return (false, true, "running")
      }
      return (false, false, "not loaded")
    }
    let fields = launchctlFields(output)
    if let pid = fields["pid"], !pid.isEmpty {
      return (true, true, "running")
    }
    if let program = fields["program"], processIsRunning(executablePath: program) {
      return (true, true, "running")
    }
    if let fallbackProgram, processIsRunning(executablePath: fallbackProgram) {
      return (true, true, "running")
    }
    let state = fields["state"] ?? "loaded"
    return (true, false, "\(state), no process")
  }

  static func processIsRunning(executablePath: String) -> Bool {
    guard !executablePath.isEmpty, executablePath != "missing" else { return false }
    let pattern = "^\(NSRegularExpression.escapedPattern(for: executablePath))($| )"
    return runCapture("/usr/bin/pgrep", ["-f", pattern]).status == 0
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

  static func yesNo(_ value: Bool) -> String {
    value ? "yes" : "no"
  }

  static func readTrimmed(_ path: String) -> String? {
    try? String(contentsOfFile: path, encoding: .utf8)
      .trimmingCharacters(in: .whitespacesAndNewlines)
  }

  static func readJSON(paths path: URL) -> [String: Any]? {
    guard let data = try? Data(contentsOf: path) else { return nil }
    return try? JSONSerialization.jsonObject(with: data) as? [String: Any]
  }

  static func effectiveCurrentVersion() -> String {
    ProcessInfo.processInfo.environment["ONECONTEXT_TEST_CURRENT_VERSION"] ?? oneContextVersion
  }

  static func nativeUpdateSnapshot(currentVersion: String) async -> NativeUpdateSnapshot {
    let appBundleURL = installedAppBundleURL()
    return await SparkleNativeUpdater(
      configuration: SparkleUpdaterConfiguration(appBundleURL: appBundleURL),
      appContext: NativeUpdaterAppContext(
        bundleURL: appBundleURL,
        executableURL: appBundleURL.appendingPathComponent("Contents/MacOS/1Context")
      ),
      driver: CLIAppManagedSparkleDriver()
    ).snapshot(currentVersion: currentVersion)
  }

  static func installedAppBundleURL() -> URL {
    let environment = ProcessInfo.processInfo.environment
    let appPath = environment["ONECONTEXT_TEST_APP_BUNDLE_PATH"] ?? "/Applications/1Context.app"
    return URL(fileURLWithPath: appPath, isDirectory: true)
  }

  static func appVersion() -> String? {
    let infoPlist = installedAppBundleURL()
      .appendingPathComponent("Contents/Info.plist")
      .path
    return NSDictionary(contentsOfFile: infoPlist)?["CFBundleShortVersionString"] as? String
  }

  static func rejectRootInvocationForAppCleanup() throws {
    if geteuid() == 0 || ProcessInfo.processInfo.environment["SUDO_USER"] != nil {
      throw CLIError.commandFailed("Run 1Context uninstall as your normal macOS user, not with sudo or as root.")
    }
  }

  static func uninstallLaunchAgent(label: String) throws {
    let fileManager = FileManager.default
    let plist = fileManager.homeDirectoryForCurrentUser
      .appendingPathComponent("Library/LaunchAgents/\(label).plist")
    _ = runCapture("/bin/launchctl", ["bootout", "gui/\(getuid())/\(label)"])
    _ = runCapture("/bin/launchctl", ["bootout", "gui/\(getuid())", plist.path])
    if fileManager.fileExists(atPath: plist.path) {
      try fileManager.removeItem(at: plist)
    }
  }

  static func deleteApprovedUserData() throws {
    let fileManager = FileManager.default
    let home = fileManager.homeDirectoryForCurrentUser
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
    process.standardInput = FileHandle.standardInput
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
