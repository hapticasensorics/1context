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
      case "uninstall":
        try await uninstall()
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
    Public bootstrap. Runtime coming soon.
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
      1context uninstall [--delete-data]
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

  static func uninstall() async throws {
    let deleteData = args.contains("--delete-data")
    if deleteData {
      try confirmDeleteData()
    }

    print("Uninstalling 1Context...")
    try await RuntimeController().uninstall(deleteData: deleteData)
    if deleteData {
      print("""
      Removed launch items.
      Deleted local data and logs.
      """)
    } else {
      print("""
      Removed launch items.
      Preserved local data and logs.
      """)
    }

    let homebrew = HomebrewUninstaller()
    if homebrew.isCurrentExecutableInstalledByHomebrew() {
      try homebrew.uninstallFormula()
      print("Removed Homebrew package.")
    } else {
      print("Homebrew package was not removed because this 1context binary is not running from Homebrew.")
    }
    print("Done.")
  }

  static func confirmDeleteData() throws {
    print("""
    This will delete local 1Context data from this Mac.
    Type DELETE to continue:
    """)
    guard readLine() == "DELETE" else {
      throw CLIError.cancelled
    }
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

enum CLIError: Error, LocalizedError {
  case cancelled
  case brewNotFound
  case brewFailed(String)

  var errorDescription: String? {
    switch self {
    case .cancelled:
      return "Uninstall cancelled"
    case .brewNotFound:
      return "Homebrew was not found"
    case .brewFailed(let message):
      return message.isEmpty ? "Homebrew uninstall failed" : message
    }
  }
}

struct HomebrewUninstaller {
  private let formula = "hapticasensorics/tap/1context"

  func isCurrentExecutableInstalledByHomebrew() -> Bool {
    guard let brew = findBrew(),
      run(brew, ["list", "--formula", formula]).status == 0,
      let formulaPrefix = brewOutput(brew, ["--prefix", formula])
    else {
      return false
    }

    let executable = URL(fileURLWithPath: CommandLine.arguments[0]).resolvingSymlinksInPath().path
    let prefix = URL(fileURLWithPath: formulaPrefix).resolvingSymlinksInPath().path
    return executable == prefix || executable.hasPrefix(prefix + "/")
  }

  func uninstallFormula() throws {
    guard let brew = findBrew() else { throw CLIError.brewNotFound }
    let result = run(brew, ["uninstall", formula])
    guard result.status == 0 else {
      throw CLIError.brewFailed((result.stderr + result.stdout).trimmingCharacters(in: .whitespacesAndNewlines))
    }
  }

  private func findBrew() -> String? {
    for candidate in ["/opt/homebrew/bin/brew", "/usr/local/bin/brew"] {
      if FileManager.default.isExecutableFile(atPath: candidate) {
        return candidate
      }
    }
    return nil
  }

  private func brewOutput(_ brew: String, _ arguments: [String]) -> String? {
    let result = run(brew, arguments)
    guard result.status == 0 else { return nil }
    let output = result.stdout.trimmingCharacters(in: .whitespacesAndNewlines)
    return output.isEmpty ? nil : output
  }

  private func run(_ executable: String, _ arguments: [String]) -> (status: Int32, stdout: String, stderr: String) {
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
