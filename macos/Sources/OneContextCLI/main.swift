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
      1context start
      1context stop
      1context restart
      1context status [--debug]
    """)
  }

  static func maybeCheckForUpdate() async {
    do {
      let result = try await UpdateChecker().check(currentVersion: oneContextVersion)
      if result.updateAvailable, let latest = result.latest {
        FileHandle.standardError.write(Data("""
        1Context \(latest.version) is available. You have \(oneContextVersion).
        Update: \(latest.installCommand)

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
      Socket Path: \(paths.socketPath)
      Log: \(paths.logPath)
    """)
  }

  static func start() async throws {
    let result = try await RuntimeController().start()
    print(result.alreadyRunning ? "1Context is already running." : "1Context is running.")
  }

  static func stop() async throws {
    let stopped = try await RuntimeController().stop()
    print(stopped ? "1Context is stopped." : "1Context is not running.")
  }

  static func restart() async throws {
    _ = try await RuntimeController().restart()
    print("1Context is running.")
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
}
