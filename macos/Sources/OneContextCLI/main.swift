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
        try await start()
      case "stop":
        try await stop()
      case "restart":
        try await restart()
      case "status":
        await status()
      case "update":
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
      1context restart [--debug]
      1context status [--debug]
      1context update
    """)
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
    try runShell(oneContextHomebrewUpdateCommand)
  }

  static func status() async {
    let debug = args.contains("--debug")
    let controller = RuntimeController()
    switch controller.status() {
    case .success(let health):
      print("""
      1Context is running.

      Version: \(health.version)
      Health: OK
      """)
      if debug { await printDebug(controller: controller, error: nil) }
    case .failure(let error):
      print("""
      1Context is not running.

      Start it with:
        1context start
      """)
      if debug { await printDebug(controller: controller, error: error) }
    }
  }

  static func runShell(_ command: String) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/zsh")
    process.arguments = ["-lc", command]
    process.standardOutput = FileHandle.standardOutput
    process.standardError = FileHandle.standardError
    try process.run()
    process.waitUntilExit()

    guard process.terminationStatus == 0 else {
      throw CLIError.commandFailed(command)
    }
  }
}

enum CLIError: Error, LocalizedError {
  case commandFailed(String)
  case runtimeStopped

  var errorDescription: String? {
    switch self {
    case .commandFailed(let command):
      return "Command failed: \(command)"
    case .runtimeStopped:
      return "1Context is stopped"
    }
  }
}
