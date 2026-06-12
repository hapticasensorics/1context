import Darwin
import Foundation
import OneContextCapture
import OneContextInstall
import OneContextLocalWeb
import OneContextMemoryRuntime
import OneContextCore
import OneContextPlatform
import OneContextProtocol
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
      case "capture":
        try capture()
      default:
        FileHandle.standardError.write(Data("Unknown command: \(command ?? "")\n".utf8))
        printHelp()
        Foundation.exit(1)
      }
    } catch {
      if command == "wiki" {
        writeWikiFailure(error)
        Foundation.exit(1)
      }
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
      1context capture status
      1context capture snapshot
      1context capture metadata-sample [--duration-seconds <seconds>] [--max-frames <n>]
      1context capture dashboard [--snapshot] [--watch] [--interval-seconds <seconds>]
      1context wiki local-url
      1context wiki list
      1context wiki validate
      1context wiki page-status <page-id-or-route>
      1context wiki page-open <page-id-or-route>
      1context wiki page-create <page-id> [--title <title>] [--route <route>] [--summary <summary>] [--template <template>]
      1context wiki page-write-body <page-id-or-route> (--body <markdown> | --body-file <path>) [--expected-source-sha256 <hash>]
      1context wiki page-patch-body <page-id-or-route> (--find <text> | --find-file <path>) (--replace <text> | --replace-file <path>) [--expected-source-sha256 <hash>]
      1context wiki asset-add <page-id-or-route> --file <path> [--filename <name>] [--purpose inline_image|download|decorative] [--caption <text>] [--alt-text <text>]
      1context wiki asset-list <page-id-or-route>
      1context wiki page-delete <page-id-or-route> [--mode tombstone]
      1context wiki page-restore <page-id-or-route>
      1context wiki agent-identify --thread-id <codex-thread-id> [--role <address>]... [--capability <name>]... [--ttl-seconds <seconds>]
      1context wiki agent-heartbeat <agent-id> [--ttl-seconds <seconds>]
      1context wiki agent-retire <agent-id>
      1context wiki agent-status <agent-id>
      1context wiki agent-inbox <agent-id>
      1context wiki mail-open <delivery-id> --agent-id <agent-id>
      1context wiki mail-claim <delivery-id> --agent-id <agent-id>
      1context wiki mail-mark <delivery-id> --agent-id <agent-id> --state read|done|archived|rejected
      1context wiki mail-snooze <delivery-id> --agent-id <agent-id> --until <rfc3339>
      1context wiki notify-poll <agent-id> [--cursor <cursor>]
      1context wiki notify-ack <notification-id> --agent-id <agent-id>
      1context wiki talk-append <page-id-or-route> --subject <subject> --from <address> (--body <markdown> | --body-file <path>) [--operation-id <id>] [--delivery-mode labels-only|mail] [--to <address>]... [--cc <address>]... [--attachment <path>]... [--attachment-filename <name>]... [--attachment-caption <caption>]... [--attachment-alt <text>]...
      1context wiki publish-status
      1context wiki publish [--trigger <label>] [--force] [--wiki-engine <dir>] [--node <path>]
    """)
  }

  static func printWikiHelp() {
    print("""
    1Context Wiki

    Usage:
      1context wiki local-url
      1context wiki list
      1context wiki validate
      1context wiki page-status <page-id-or-route>
      1context wiki page-open <page-id-or-route>
      1context wiki page-create <page-id> [--title <title>] [--route <route>] [--summary <summary>] [--template <template>]
      1context wiki page-write-body <page-id-or-route> (--body <markdown> | --body-file <path>) [--expected-source-sha256 <hash>]
      1context wiki page-patch-body <page-id-or-route> (--find <text> | --find-file <path>) (--replace <text> | --replace-file <path>) [--expected-source-sha256 <hash>]
      1context wiki asset-add <page-id-or-route> --file <path> [--filename <name>] [--purpose inline_image|download|decorative] [--caption <text>] [--alt-text <text>]
      1context wiki asset-list <page-id-or-route>
      1context wiki page-delete <page-id-or-route> [--mode tombstone]
      1context wiki page-restore <page-id-or-route>
      1context wiki agent-identify --thread-id <codex-thread-id> [--role <address>]... [--capability <name>]... [--ttl-seconds <seconds>]
      1context wiki agent-heartbeat <agent-id> [--ttl-seconds <seconds>]
      1context wiki agent-retire <agent-id>
      1context wiki agent-status <agent-id>
      1context wiki agent-inbox <agent-id>
      1context wiki mail-open <delivery-id> --agent-id <agent-id>
      1context wiki mail-claim <delivery-id> --agent-id <agent-id>
      1context wiki mail-mark <delivery-id> --agent-id <agent-id> --state read|done|archived|rejected
      1context wiki mail-snooze <delivery-id> --agent-id <agent-id> --until <rfc3339>
      1context wiki notify-poll <agent-id> [--cursor <cursor>]
      1context wiki notify-ack <notification-id> --agent-id <agent-id>
      1context wiki talk-append <page-id-or-route> --subject <subject> --from <address> (--body <markdown> | --body-file <path>) [--operation-id <id>] [--delivery-mode labels-only|mail] [--to <address>]... [--cc <address>]... [--attachment <path>]... [--attachment-filename <name>]... [--attachment-caption <caption>]... [--attachment-alt <text>]...
      1context wiki publish-status
      1context wiki publish [--trigger <label>] [--force] [--wiki-engine <dir>] [--node <path>]
    """)
  }

  static func printCaptureHelp() {
    print("""
    1Context Capture

    Usage:
      1context capture status
      1context capture snapshot
      1context capture metadata-sample [--duration-seconds <seconds>] [--max-frames <n>]
      1context capture dashboard [--snapshot] [--watch] [--interval-seconds <seconds>]
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
    let identity = paths.identity
    let controller = RuntimeController()
    let health = controller.status()

    print("1Context Diagnose\n")
    print("CLI:")
    print("  Version: \(oneContextVersion)")
    print("  Executable: \(displayPath(currentExecutablePath() ?? CommandLine.arguments[0], redact: redact))")
    print("  App Bundle: \(displayPath(installedAppBundleURL().path, redact: redact))")
    print("  App Identity: \(identity.environmentValue)")
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

    printMemoryDiagnostics(redact: redact)
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
    print("  Readiness Probe URL: \(diagnostics.readinessProbeURL)")
    print("  Readiness Probe Health: \(diagnostics.readinessProbeHealth)")
    print("  Privileged Proxy Probe URL: \(diagnostics.privilegedProxyProbeURL)")
    print("  Privileged Proxy Probe Health: \(diagnostics.privilegedProxyProbeHealth)")
    print("  Branded Host Probe URL: \(diagnostics.brandedProbeURL)")
    print("  Branded Host Probe Health: \(diagnostics.brandedProbeHealth)")
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
    print("  Configured Caddy Path: \(displayPath(diagnostics.caddyExecutable, redact: redact))")
    if let runningCaddyExecutable = diagnostics.runningCaddyExecutable {
      print("  Running Caddy Path: \(displayPath(runningCaddyExecutable, redact: redact))")
    }
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

  static func printMemoryDiagnostics(redact: Bool) {
    print("\nMemory:")
    do {
      let payload = try UnixJSONRPCClient().call(method: "memory.status")
      print("  Health: \((payload["running"] as? Bool) == true ? "OK" : "not running")")
      if let executable = payload["executable"] as? String {
        print("  Executable: \(displayPath(executable, redact: redact))")
      }
      if let statusPath = payload["status_path"] as? String {
        print("  Status: \(displayPath(statusPath, redact: redact))")
      }
      if let cursorPath = payload["cursor_path"] as? String {
        print("  Cursors: \(displayPath(cursorPath, redact: redact))")
      }
      if let status = payload["status"] as? [String: Any],
        let sources = status["sources"] as? [[String: Any]]
      {
        for source in sources {
          let name = source["source"] as? String ?? "unknown"
          let state = source["status"] as? String ?? "unknown"
          let records = source["record_count"] as? Int ?? 0
          let elapsed = source["elapsed_ms"] as? Int ?? 0
          print("  Source \(name): \(state), records=\(records), elapsed_ms=\(elapsed)")
        }
      }
    } catch {
      print("  Health: no response")
      print("  Error: \(error.localizedDescription)")
    }

    print("  Storage:")
    do {
      let payload = try MemoryDaemonProcessClient(runtimePaths: RuntimePaths.current())
        .storageHealth(MemoryStorageHealthQuery(reason: "diagnose"))
      let health = (payload["result"] as? [String: Any]) ?? payload
      printMemoryStorageHealth(health, redact: redact)
    } catch {
      print("    Health: no response")
      print("    Error: \(displayPath(error.localizedDescription, redact: redact))")
    }
  }

  static func printMemoryStorageHealth(_ health: [String: Any], redact: Bool) {
    print("    Backend: \(health["backend"] as? String ?? "unknown")")
    print("    Status: \(health["status"] as? String ?? "unknown")")
    print("    Ready: \(yesNo((health["ready"] as? Bool) == true))")
    print("    Storage Ready: \(yesNo((health["storage_ready"] as? Bool) == true))")
    print("    Recent History Ready: \(yesNo((health["recent_history_ready"] as? Bool) == true))")
    if let schemaState = health["schema_state"] as? String {
      print("    Schema State: \(schemaState)")
    }
    if let schemaVersion = health["schema_version"] {
      print("    Schema Version: \(schemaVersion)")
    }
    if let expectedSchemaVersion = health["expected_schema_version"] {
      print("    Expected Schema Version: \(expectedSchemaVersion)")
    }
    if let socketDir = health["socket_dir"] as? String {
      print("    Socket Dir: \(displayPath(socketDir, redact: redact))")
    }
    if let pgdataDir = health["pgdata_dir"] as? String {
      print("    PGDATA Dir: \(displayPath(pgdataDir, redact: redact))")
    }
    if let appSupportDir = health["app_support_dir"] as? String {
      print("    App Support Dir: \(displayPath(appSupportDir, redact: redact))")
    }
    if let bundlePrefix = health["bundle_prefix"] as? String {
      print("    Bundle Prefix: \(displayPath(bundlePrefix, redact: redact))")
    }
    if let postgresVersion = health["bundle_postgres_version"] as? String {
      print("    Bundle PostgreSQL: \(postgresVersion)")
    }
    if let timescaleVersion = health["bundle_timescale_version"] as? String {
      print("    Bundle TimescaleDB: \(timescaleVersion)")
    }
    if let buildID = health["bundle_build_id"] as? String {
      print("    Bundle Build ID: \(buildID)")
    }
    if let extensions = health["required_extensions"] as? [[String: Any]], !extensions.isEmpty {
      for ext in extensions {
        let name = ext["name"] as? String ?? "unknown"
        let installed = (ext["installed"] as? Bool) == true
        let version = ext["version"] as? String
        let preloadRequired = (ext["preload_required"] as? Bool) == true
        let preload = ext["preload_active"] as? Bool
        var parts = [installed ? "installed" : "missing"]
        if let version {
          parts.append("version=\(version)")
        }
        if preloadRequired {
          parts.append("preload=\(preload.map(yesNo) ?? "unknown")")
        }
        print("    Extension \(name): \(parts.joined(separator: ", "))")
      }
    }
    if let message = health["message"] as? String {
      print("    Message: \(displayPath(message, redact: redact))")
    }
    if let detail = health["detail"] as? String {
      print("    Detail: \(displayPath(detail, redact: redact))")
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

    if args.count == 2, ["--help", "-h", "help"].contains(args[1]) {
      printWikiHelp()
      return
    }
    if args.count >= 3, ["--help", "-h", "help"].contains(args[2]) {
      printWikiHelp()
      return
    }

    switch args[1] {
    case "local-url":
      try rejectUnknownWikiArguments(allowed: [])
      let localWeb = CaddyManager()
      let diagnostics = localWeb.diagnostics()
      guard diagnostics.setup.ready else {
        throw CLIError.commandFailed("""
        Local wiki access is not set up.

        Open 1Context and choose Settings > Setup...
        """)
      }
      let snapshot = try localWeb.start()
      print(snapshot.url)
    case "list":
      try requireWikiArgumentCount(2)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.list"))
    case "validate":
      try requireWikiArgumentCount(2)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.validate"))
    case "page-status":
      try requireWikiArgumentCount(3)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.page.status", params: ["page": args[2]]))
    case "page-open":
      try requireWikiArgumentCount(3)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.page.open", params: ["page": args[2]]))
    case "page-create":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.page.create", params: try wikiPageCreateParams()))
    case "page-write-body":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.page.write_body", params: try wikiPageWriteBodyParams()))
    case "page-patch-body":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.page.patch_body", params: try wikiPagePatchBodyParams()))
    case "asset-add":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.asset.add", params: try wikiAssetAddParams()))
    case "asset-list":
      try requireWikiArgumentCount(3)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.asset.list", params: ["page": args[2]]))
    case "page-delete":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.page.delete", params: try wikiPageDeleteParams()))
    case "page-restore":
      try requireWikiArgumentCount(3)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.page.restore", params: ["page": args[2]]))
    case "agent-identify":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.agent.identify", params: try wikiAgentIdentifyParams()))
    case "agent-heartbeat":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.agent.heartbeat", params: try wikiAgentHeartbeatParams()))
    case "agent-retire":
      try requireWikiArgumentCount(3)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.agent.retire", params: ["agent_id": args[2]]))
    case "agent-status":
      try requireWikiArgumentCount(3)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.agent.status", params: ["agent_id": args[2]]))
    case "agent-inbox":
      try requireWikiArgumentCount(3)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.agent.inbox", params: ["agent_id": args[2]]))
    case "mail-open":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.mail.open", params: try wikiDeliveryAgentParams("mail-open")))
    case "mail-claim":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.mail.claim", params: try wikiDeliveryAgentParams("mail-claim")))
    case "mail-mark":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.mail.mark", params: try wikiMailMarkParams()))
    case "mail-snooze":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.mail.snooze", params: try wikiMailSnoozeParams()))
    case "notify-poll":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.notify.poll", params: try wikiNotifyPollParams()))
    case "notify-ack":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.notify.ack", params: try wikiNotifyAckParams()))
    case "talk-append":
      try printJSON(UnixJSONRPCClient().call(method: "wiki.talk.append", params: try wikiTalkAppendParams()))
    case "publish-status":
      try requireWikiArgumentCount(2)
      try printJSON(UnixJSONRPCClient().call(method: "wiki.publish.status"))
    case "publish":
      try printJSON(UnixJSONRPCClient(timeoutMilliseconds: 60_000).call(
        method: "wiki.publish",
        params: try wikiPublishParams()
      ))
    default:
      throw CLIError.commandFailed("Unknown wiki subcommand: \(args[1])")
    }
  }

  static func capture() throws {
    guard args.count >= 2 else {
      throw CLIError.commandFailed("capture requires a subcommand")
    }

    if args.count == 2, ["--help", "-h", "help"].contains(args[1]) {
      printCaptureHelp()
      return
    }

    switch args[1] {
    case "status":
      try requireCaptureArgumentCount(2)
      try printJSON(UnixJSONRPCClient().call(method: "capture.status"))
    case "snapshot":
      try requireCaptureArgumentCount(2)
      try printJSON(UnixJSONRPCClient(timeoutMilliseconds: 10_000).call(method: "capture.snapshot"))
    case "metadata-sample":
      try captureMetadataSample()
    case "dashboard":
      try captureDashboard()
    default:
      throw CLIError.commandFailed("Unknown capture subcommand: \(args[1])")
    }
  }

  static func captureMetadataSample() throws {
    let options = try captureMetadataSampleOptions()
    let timeoutMilliseconds = Int32((options.durationSeconds + 10) * 1_000)
    try printJSON(UnixJSONRPCClient(timeoutMilliseconds: timeoutMilliseconds).call(
      method: "capture.active_window_metadata_sample",
      params: [
        "duration_seconds": options.durationSeconds,
        "max_frames": options.maxFrames
      ]
    ))
  }

  struct CaptureMetadataSampleOptions {
    var durationSeconds: TimeInterval
    var maxFrames: Int
  }

  static func captureMetadataSampleOptions() throws -> CaptureMetadataSampleOptions {
    var durationSeconds: TimeInterval = 3
    var maxFrames = 30
    var index = 2
    while index < args.count {
      switch args[index] {
      case "--duration-seconds":
        index += 1
        guard index < args.count, let parsed = Double(args[index]), parsed > 0 else {
          throw CLIError.commandFailed("capture metadata-sample requires a positive value for --duration-seconds")
        }
        durationSeconds = parsed
        index += 1
      case "--max-frames":
        index += 1
        guard index < args.count, let parsed = Int(args[index]), parsed > 0 else {
          throw CLIError.commandFailed("capture metadata-sample requires a positive integer for --max-frames")
        }
        maxFrames = parsed
        index += 1
      default:
        throw CLIError.unknownArgument(args[index])
      }
    }
    return CaptureMetadataSampleOptions(durationSeconds: durationSeconds, maxFrames: maxFrames)
  }

  static func captureDashboard() throws {
    let options = try captureDashboardOptions()
    repeat {
      if options.captureFreshSnapshot {
        _ = try? UnixJSONRPCClient(timeoutMilliseconds: 10_000).call(method: "capture.snapshot")
      }
      print("\u{001B}[2J\u{001B}[H", terminator: "")
      print(try OneContextCaptureDashboard(runtimePaths: RuntimePaths.current()).render())
      fflush(stdout)
      if options.watch {
        Thread.sleep(forTimeInterval: options.intervalSeconds)
      }
    } while options.watch
  }

  struct CaptureDashboardOptions {
    var captureFreshSnapshot: Bool
    var watch: Bool
    var intervalSeconds: TimeInterval
  }

  static func captureDashboardOptions() throws -> CaptureDashboardOptions {
    var captureFreshSnapshot = false
    var watch = false
    var intervalSeconds: TimeInterval = 2
    var index = 2
    while index < args.count {
      switch args[index] {
      case "--snapshot":
        captureFreshSnapshot = true
        index += 1
      case "--watch":
        watch = true
        index += 1
      case "--interval-seconds":
        index += 1
        guard index < args.count, let parsed = Double(args[index]), parsed > 0 else {
          throw CLIError.commandFailed("capture dashboard requires a positive value for --interval-seconds")
        }
        intervalSeconds = parsed
        index += 1
      default:
        throw CLIError.unknownArgument(args[index])
      }
    }
    return CaptureDashboardOptions(
      captureFreshSnapshot: captureFreshSnapshot,
      watch: watch,
      intervalSeconds: intervalSeconds
    )
  }

  static func wikiPageCreateParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki page-create requires a page id")
    }
    var params = try wikiParams(
      startIndex: 3,
      valueFlags: [
        "--title": "title",
        "--route": "route",
        "--slug": "slug",
        "--family-group": "family_group",
        "--family-group-title": "family_group_title",
        "--family-id": "family_id",
        "--family-title": "family_title",
        "--type": "type",
        "--template": "template",
        "--talk-conventions-template": "talk_conventions_template",
        "--talk-curator-template": "talk_curator_template",
        "--summary": "summary",
        "--nav-section": "nav_section",
        "--nav-order": "nav_order"
      ]
    )
    params["id"] = args[2]
    return params
  }

  static func wikiPageWriteBodyParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki page-write-body requires a page id or route")
    }
    var params = try wikiParams(
      startIndex: 3,
      valueFlags: [
        "--body": "body",
        "--body-file": "body_file",
        "--expected-source-sha256": "expected_source_sha256"
      ]
    )
    if params["body"] == nil && params["body_file"] == nil {
      throw CLIError.commandFailed("wiki page-write-body requires --body or --body-file")
    }
    try rejectMutuallyExclusive(params, command: "page-write-body", "--body", "body", "--body-file", "body_file")
    params["page"] = args[2]
    return params
  }

  static func wikiPagePatchBodyParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki page-patch-body requires a page id or route")
    }
    var params = try wikiParams(
      startIndex: 3,
      valueFlags: [
        "--find": "find",
        "--find-file": "find_file",
        "--replace": "replace",
        "--replace-file": "replace_file",
        "--expected-source-sha256": "expected_source_sha256"
      ]
    )
    if params["find"] == nil && params["find_file"] == nil {
      throw CLIError.commandFailed("wiki page-patch-body requires --find or --find-file")
    }
    if params["replace"] == nil && params["replace_file"] == nil {
      throw CLIError.commandFailed("wiki page-patch-body requires --replace or --replace-file")
    }
    try rejectMutuallyExclusive(params, command: "page-patch-body", "--find", "find", "--find-file", "find_file")
    try rejectMutuallyExclusive(
      params,
      command: "page-patch-body",
      "--replace",
      "replace",
      "--replace-file",
      "replace_file"
    )
    params["page"] = args[2]
    return params
  }

  static func wikiAssetAddParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki asset-add requires a page id or route")
    }
    var params = try wikiParams(
      startIndex: 3,
      valueFlags: [
        "--file": "file",
        "--filename": "filename",
        "--purpose": "purpose",
        "--caption": "caption",
        "--alt-text": "alt_text"
      ]
    )
    guard params["file"] != nil else {
      throw CLIError.commandFailed("wiki asset-add requires --file")
    }
    params["page"] = args[2]
    return params
  }

  static func wikiPageDeleteParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki page-delete requires a page id or route")
    }
    var params = try wikiParams(startIndex: 3, valueFlags: ["--mode": "mode"])
    params["page"] = args[2]
    return params
  }

  static func wikiAgentIdentifyParams() throws -> [String: Any] {
    let params = try wikiParams(
      startIndex: 2,
      valueFlags: [
        "--thread-id": "thread_id",
        "--ttl-seconds": "ttl_seconds"
      ],
      repeatedFlags: [
        "--role": "roles",
        "--capability": "capabilities"
      ]
    )
    guard params["thread_id"] != nil else {
      throw CLIError.commandFailed("wiki agent-identify requires --thread-id")
    }
    return params
  }

  static func wikiAgentHeartbeatParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki agent-heartbeat requires an agent id")
    }
    var params = try wikiParams(startIndex: 3, valueFlags: ["--ttl-seconds": "ttl_seconds"])
    params["agent_id"] = args[2]
    return params
  }

  static func wikiDeliveryAgentParams(_ command: String) throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki \(command) requires a delivery id")
    }
    var params = try wikiParams(startIndex: 3, valueFlags: ["--agent-id": "agent_id"])
    guard params["agent_id"] != nil else {
      throw CLIError.commandFailed("wiki \(command) requires --agent-id")
    }
    params["delivery_id"] = args[2]
    return params
  }

  static func wikiMailMarkParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki mail-mark requires a delivery id")
    }
    var params = try wikiParams(
      startIndex: 3,
      valueFlags: [
        "--agent-id": "agent_id",
        "--state": "state"
      ]
    )
    for required in ["agent_id", "state"] where params[required] == nil {
      throw CLIError.commandFailed("wiki mail-mark requires --\(required.replacingOccurrences(of: "_", with: "-"))")
    }
    params["delivery_id"] = args[2]
    return params
  }

  static func wikiMailSnoozeParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki mail-snooze requires a delivery id")
    }
    var params = try wikiParams(
      startIndex: 3,
      valueFlags: [
        "--agent-id": "agent_id",
        "--until": "until"
      ]
    )
    guard params["agent_id"] != nil else {
      throw CLIError.commandFailed("wiki mail-snooze requires --agent-id")
    }
    guard params["until"] != nil else {
      throw CLIError.commandFailed("wiki mail-snooze requires --until")
    }
    params["delivery_id"] = args[2]
    return params
  }

  static func wikiNotifyPollParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki notify-poll requires an agent id")
    }
    var params = try wikiParams(startIndex: 3, valueFlags: ["--cursor": "cursor"])
    params["agent_id"] = args[2]
    return params
  }

  static func wikiNotifyAckParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki notify-ack requires a notification id")
    }
    var params = try wikiParams(startIndex: 3, valueFlags: ["--agent-id": "agent_id"])
    guard params["agent_id"] != nil else {
      throw CLIError.commandFailed("wiki notify-ack requires --agent-id")
    }
    params["notification_id"] = args[2]
    return params
  }

  static func wikiTalkAppendParams() throws -> [String: Any] {
    guard args.count >= 3 else {
      throw CLIError.commandFailed("wiki talk-append requires a page id or route")
    }
    var params = try wikiParams(
      startIndex: 3,
      valueFlags: [
        "--kind": "kind",
        "--subject": "subject",
        "--from": "from",
        "--thread-id": "thread_id",
        "--reply-to": "reply_to",
        "--operation-id": "operation_id",
        "--delivery-mode": "delivery_mode",
        "--body": "body",
        "--body-file": "body_file"
      ],
      repeatedFlags: [
        "--to": "to",
        "--cc": "cc",
        "--attachment": "attachments",
        "--attachment-filename": "attachment_filenames",
        "--attachment-caption": "attachment_captions",
        "--attachment-alt": "attachment_alts"
      ],
      boolFlags: ["--allow-tombstoned": "allow_tombstoned"]
    )
    for required in ["subject", "from"] where params[required] == nil {
      throw CLIError.commandFailed("wiki talk-append requires --\(required)")
    }
    if params["body"] == nil && params["body_file"] == nil {
      throw CLIError.commandFailed("wiki talk-append requires --body or --body-file")
    }
    try rejectMutuallyExclusive(params, command: "talk-append", "--body", "body", "--body-file", "body_file")
    try validateTalkAttachmentMetadata(params)
    params["page"] = args[2]
    return params
  }

  static func rejectMutuallyExclusive(
    _ params: [String: Any],
    command: String,
    _ firstFlag: String,
    _ firstKey: String,
    _ secondFlag: String,
    _ secondKey: String
  ) throws {
    if params[firstKey] != nil && params[secondKey] != nil {
      throw CLIError.commandFailed("wiki \(command) accepts either \(firstFlag) or \(secondFlag), not both")
    }
  }

  static func validateTalkAttachmentMetadata(_ params: [String: Any]) throws {
    let attachmentCount = (params["attachments"] as? [String])?.count ?? 0
    for (key, flag) in [
      ("attachment_filenames", "--attachment-filename"),
      ("attachment_captions", "--attachment-caption"),
      ("attachment_alts", "--attachment-alt")
    ] {
      let count = (params[key] as? [String])?.count ?? 0
      if count > attachmentCount {
        throw CLIError.commandFailed("wiki talk-append received \(flag) metadata without a matching --attachment")
      }
    }
  }

  static func wikiPublishParams() throws -> [String: Any] {
    var params: [String: Any] = [:]
    var index = 2
    while index < args.count {
      switch args[index] {
      case "--trigger":
        index += 1
        guard index < args.count else {
          throw CLIError.commandFailed("wiki publish requires a value for --trigger")
        }
        params["trigger"] = args[index]
      case "--force":
        params["force"] = true
      case "--wiki-engine":
        index += 1
        guard index < args.count else {
          throw CLIError.commandFailed("wiki publish requires a value for --wiki-engine")
        }
        params["wiki_engine"] = args[index]
      case "--node":
        index += 1
        guard index < args.count else {
          throw CLIError.commandFailed("wiki publish requires a value for --node")
        }
        params["node"] = args[index]
      default:
        throw CLIError.unknownArgument(args[index])
      }
      index += 1
    }
    return params
  }

  static func wikiParams(
    startIndex: Int,
    valueFlags: [String: String] = [:],
    repeatedFlags: [String: String] = [:],
    boolFlags: [String: String] = [:]
  ) throws -> [String: Any] {
    var params: [String: Any] = [:]
    var index = startIndex
    while index < args.count {
      let flag = args[index]
      if let param = boolFlags[flag] {
        params[param] = true
        index += 1
        continue
      }
      let param: String
      let repeated: Bool
      if let mapped = valueFlags[flag] {
        param = mapped
        repeated = false
      } else if let mapped = repeatedFlags[flag] {
        param = mapped
        repeated = true
      } else {
        throw CLIError.unknownArgument(flag)
      }
      index += 1
      guard index < args.count else {
        throw CLIError.commandFailed("wiki \(args[1]) requires a value for \(flag)")
      }
      if repeated {
        var values = params[param] as? [String] ?? []
        values.append(args[index])
        params[param] = values
      } else {
        params[param] = args[index]
      }
      index += 1
    }
    return params
  }

  static func requireWikiArgumentCount(_ count: Int) throws {
    if args.count > count {
      throw CLIError.unknownArgument(args[count])
    }
    guard args.count == count else {
      throw CLIError.commandFailed("wiki \(args[1]) expected \(count - 2) argument(s)")
    }
  }

  static func requireCaptureArgumentCount(_ count: Int) throws {
    if args.count > count {
      throw CLIError.unknownArgument(args[count])
    }
    guard args.count == count else {
      throw CLIError.commandFailed("capture \(args[1]) expected \(count - 2) argument(s)")
    }
  }

  static func rejectUnknownWikiArguments(allowed: Set<String>) throws {
    let unknown = args.dropFirst(2).filter { !allowed.contains($0) }
    if let first = unknown.first {
      throw CLIError.unknownArgument(first)
    }
  }

  static func printJSON(_ object: [String: Any]) throws {
    let data = try JSONSerialization.data(withJSONObject: object, options: [.prettyPrinted, .sortedKeys])
    print(String(decoding: data, as: UTF8.self))
  }

  static func writeWikiFailure(_ error: Error) {
    let envelope = wikiFailureEnvelope(error)
    do {
      try printJSON(envelope)
    } catch {
      FileHandle.standardError.write(Data("1Context wiki failed: \(error.localizedDescription)\n".utf8))
    }
  }

  static func wikiFailureEnvelope(_ error: Error) -> [String: Any] {
    if let rpcEnvelope = parseJSONObject(error.localizedDescription),
      rpcEnvelope["schema_version"] != nil,
      rpcEnvelope["status"] != nil,
      rpcEnvelope["error"] != nil
    {
      return rpcEnvelope
    }

    return [
      "schema_version": 1,
      "status": "error",
      "surface": "swift_cli",
      "operation": wikiOperationName(),
      "command": wikiCommandName(),
      "error": [
        "code": wikiErrorCode(error),
        "message": wikiErrorMessage(error)
      ]
    ]
  }

  static func parseJSONObject(_ text: String) -> [String: Any]? {
    let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
    if let object = parseJSONObjectData(trimmed) {
      return object
    }
    guard let start = trimmed.firstIndex(of: "{"),
      let end = trimmed.lastIndex(of: "}"),
      start < end
    else {
      return nil
    }
    return parseJSONObjectData(String(trimmed[start...end]))
  }

  static func parseJSONObjectData(_ text: String) -> [String: Any]? {
    guard let data = text.data(using: .utf8),
      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
      return nil
    }
    return object
  }

  static func wikiOperationName() -> String {
    guard args.count >= 2 else { return "wiki.unknown" }
    return wikiOperationNames[args[1]] ?? "wiki.\(args[1].replacingOccurrences(of: "-", with: "."))"
  }

  static func wikiCommandName() -> String {
    args.count >= 2 ? args[1] : "wiki"
  }

  static let wikiOperationNames: [String: String] = [
    "local-url": "wiki.local_url",
    "list": "wiki.list",
    "validate": "wiki.validate",
    "page-status": "wiki.page.status",
    "page-open": "wiki.page.open",
    "page-create": "wiki.page.create",
    "page-write-body": "wiki.page.write_body",
    "page-patch-body": "wiki.page.patch_body",
    "asset-add": "wiki.asset.add",
    "asset-list": "wiki.asset.list",
    "page-delete": "wiki.page.delete",
    "page-restore": "wiki.page.restore",
    "agent-identify": "wiki.agent.identify",
    "agent-heartbeat": "wiki.agent.heartbeat",
    "agent-retire": "wiki.agent.retire",
    "agent-status": "wiki.agent.status",
    "agent-inbox": "wiki.agent.inbox",
    "mail-open": "wiki.mail.open",
    "mail-claim": "wiki.mail.claim",
    "mail-mark": "wiki.mail.mark",
    "mail-snooze": "wiki.mail.snooze",
    "notify-poll": "wiki.notify.poll",
    "notify-ack": "wiki.notify.ack",
    "talk-append": "wiki.talk.append",
    "publish-status": "wiki.publish.status",
    "publish": "wiki.publish"
  ]

  static func wikiErrorCode(_ error: Error) -> String {
    if let cliError = error as? CLIError {
      switch cliError {
      case .unknownArgument:
        return "unexpected_arguments"
      case .commandFailed(let message):
        if message.contains("Unknown wiki subcommand") {
          return "unknown_command"
        }
        return "invalid_arguments"
      }
    }
    if let socketError = error as? UnixSocketError {
      switch socketError {
      case .connectFailed, .socketFailed, .socketPathExists, .pathTooLong:
        return "daemon_unavailable"
      case .writeFailed, .emptyResponse, .invalidResponse:
        return "transport_error"
      case .rpcError:
        return "runtime_error"
      }
    }
    return "runtime_error"
  }

  static func wikiErrorMessage(_ error: Error) -> String {
    if let cliError = error as? CLIError {
      switch cliError {
      case .commandFailed(let message):
        return message
      case .unknownArgument(let argument):
        return "unexpected argument: \(argument)"
      }
    }
    return error.localizedDescription
  }

  static func printLaunchAgent(label: String, redact: Bool = false) {
    let home = FileManager.default.homeDirectoryForCurrentUser
    let plist = home.appendingPathComponent("Library/LaunchAgents/\(label).plist")
    let loaded = launchctlPrint(label: label)
    let loadedFields = loaded.map(launchctlFields) ?? [:]
    let program = plistProgram(path: plist.path) ?? loadedFields["program"]
    let matchingPIDs = program.map(processIDs(matchingExecutable:)) ?? []

    print("  \(label):")
    print("    Plist: \(displayPath(plist.path, redact: redact))")
    print("    Plist Exists: \(FileManager.default.fileExists(atPath: plist.path) ? "yes" : "no")")
    print("    Plist Program: \(displayPath(program ?? "missing", redact: redact))")
    print("    Loaded: \(loaded == nil ? "no" : "yes")")
    print("    State: \(loadedFields["state"] ?? "missing")")
    print("    Loaded Program: \(displayPath(loadedFields["program"] ?? "missing", redact: redact))")
    print("    PID: \(loadedFields["pid"] ?? "missing")")
    print("    Matching Processes: \(matchingPIDs.isEmpty ? "none" : matchingPIDs.joined(separator: ", "))")
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

  static func processIDs(matchingExecutable executable: String) -> [String] {
    let pattern = NSRegularExpression.escapedPattern(for: executable)
    let result = runCapture("/usr/bin/pgrep", ["-f", pattern])
    guard result.status == 0 else { return [] }
    return result.stdout.split(separator: "\n").map { pid in
      String(pid).trimmingCharacters(in: .whitespaces)
    }.filter {
      !$0.isEmpty
    }
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
    OneContextAppIdentity.current().appBundleURL
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
    let identity = OneContextAppIdentity.current()
    let relativePaths = [
      identity.userContentDirectoryName,
      "Library/Application Support/\(identity.appSupportDirectoryName)",
      "Library/Logs/\(identity.logDirectoryName)",
      "Library/Caches/\(identity.cacheDirectoryName)",
      "Library/Caches/\(identity.bundleIdentifier)",
      "Library/Caches/\(identity.menuLaunchAgentLabel)",
      "Library/HTTPStorages/\(identity.bundleIdentifier)",
      "Library/HTTPStorages/\(identity.bundleIdentifier).binarycookies",
      "Library/HTTPStorages/1context",
      "Library/HTTPStorages/1context.binarycookies",
      "Library/HTTPStorages/\(identity.menuLaunchAgentLabel)",
      "Library/HTTPStorages/\(identity.menuLaunchAgentLabel).binarycookies",
      "Library/Preferences/\(identity.preferencesFileName)",
      "Library/Saved Application State/\(identity.bundleIdentifier).savedState",
      "Library/Saved Application State/\(identity.menuLaunchAgentLabel).savedState",
      "Library/WebKit/\(identity.bundleIdentifier)",
      "Library/WebKit/\(identity.menuLaunchAgentLabel)"
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
    do {
      let result = try ProcessRunner.run(executable: URL(fileURLWithPath: executable), arguments: arguments)
      return (result.status, result.stdoutText, result.stderrText)
    } catch {
      return (1, "", error.localizedDescription)
    }
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
