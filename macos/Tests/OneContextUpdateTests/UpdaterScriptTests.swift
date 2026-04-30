import XCTest
@testable import OneContextUpdate

final class UpdaterScriptTests: XCTestCase {
  // MARK: - Static-shape assertions

  func testScriptStartsWithZshShebang() {
    let script = UpdaterScript.render(
      cliExecutable: "/cli",
      alertExecutable: "/alert",
      logDirectory: "/logs"
    )
    XCTAssertTrue(
      script.hasPrefix("#!/bin/zsh\n"),
      "Updater script must start with /bin/zsh shebang so Terminal interprets it the same on every Mac."
    )
  }

  func testScriptShellQuotesExecutables() {
    let script = UpdaterScript.render(
      cliExecutable: "/Applications/1Context.app/Contents/MacOS/1context-cli",
      alertExecutable: "/Applications/1Context.app/Contents/MacOS/1Context",
      logDirectory: "/Users/test/Library/Logs/1Context"
    )

    XCTAssertTrue(
      script.contains("'/Applications/1Context.app/Contents/MacOS/1context-cli' update"),
      "CLI invocation must be single-quoted so paths with spaces survive shell parsing."
    )
    XCTAssertTrue(
      script.contains("'/Applications/1Context.app/Contents/MacOS/1Context' --update-success-alert"),
      "Alert invocation must be single-quoted."
    )
    XCTAssertTrue(
      script.contains("LOG_DIR='/Users/test/Library/Logs/1Context'"),
      "Log directory assignment must be single-quoted."
    )
  }

  func testScriptEscapesSingleQuotesInPaths() {
    // POSIX single-quote escape: end the quote, escaped quote, restart quote.
    let script = UpdaterScript.render(
      cliExecutable: "/path with 'quote'/cli",
      alertExecutable: "/safe/alert",
      logDirectory: "/safe/logs"
    )
    XCTAssertTrue(
      script.contains(#"'/path with '\''quote'\''/cli' update"#),
      "Single quotes inside paths must be escaped using the '\\'' idiom."
    )
  }

  func testScriptCapturesAndPropagatesUpdateExitStatus() {
    let script = UpdaterScript.render(
      cliExecutable: "/cli",
      alertExecutable: "/alert",
      logDirectory: "/logs"
    )
    // Note: not "status" — zsh reserves that name as a read-only alias for $?.
    XCTAssertTrue(script.contains("cli_status=$?"), "Must capture CLI exit status into a non-reserved name.")
    XCTAssertFalse(
      script.contains("\nstatus=$?"),
      "Must not assign to bare `status`; that is a read-only variable in zsh."
    )
    XCTAssertTrue(script.contains(#"exit "$cli_status""#), "Must propagate captured status as the script's exit code.")
  }

  func testScriptTeesOutputToLogFile() {
    let script = UpdaterScript.render(
      cliExecutable: "/cli",
      alertExecutable: "/alert",
      logDirectory: "/logs"
    )
    XCTAssertTrue(
      script.contains(#"exec > >(tee -a "$LOG_FILE") 2>&1"#),
      "Must mirror stdout+stderr through tee so output survives auto-closed Terminal windows."
    )
    XCTAssertTrue(
      script.contains(#"LOG_FILE="$LOG_DIR/update-"#),
      "Log file name must include an update-<timestamp> prefix for sortability."
    )
  }

  func testScriptWaitsForKeypressBeforeExit() {
    let script = UpdaterScript.render(
      cliExecutable: "/cli",
      alertExecutable: "/alert",
      logDirectory: "/logs"
    )
    XCTAssertTrue(script.contains("Press Return to close"), "Must prompt the user to confirm close.")
    XCTAssertTrue(script.contains("read -r _"), "Must wait on stdin so the window stays readable.")
  }

  func testScriptShowsLogPathOnFailure() {
    let script = UpdaterScript.render(
      cliExecutable: "/cli",
      alertExecutable: "/alert",
      logDirectory: "/logs"
    )
    XCTAssertTrue(
      script.contains(#"Details: $LOG_FILE"#),
      "Failure path must surface the log file path so users can inspect it later."
    )
  }

  func testScriptSelfDeletesViaExitTrap() {
    let script = UpdaterScript.render(
      cliExecutable: "/cli",
      alertExecutable: "/alert",
      logDirectory: "/logs"
    )
    XCTAssertTrue(
      script.contains(#"trap 'rm -f "$0"' EXIT"#),
      "Script must clean itself up from /var/folders on exit."
    )
  }

  // MARK: - End-to-end execution

  /// Runs the generated script against a stub CLI, confirming the success
  /// path: output gets logged, the script exits 0, and it self-deletes.
  func testGeneratedScriptLogsAndExitsZeroOnSuccess() throws {
    try runScriptHarness(
      cliExitStatus: 0,
      cliStdout: "fake-cli running update step\n",
      expecting: { result in
        XCTAssertEqual(result.terminationStatus, 0, "Expected zero exit status on CLI success.")
        XCTAssertTrue(result.logContents.contains("Updating 1Context..."))
        XCTAssertTrue(result.logContents.contains("fake-cli running update step"))
        XCTAssertTrue(result.logContents.contains("Done."))
        XCTAssertFalse(
          FileManager.default.fileExists(atPath: result.scriptPath),
          "EXIT trap must remove the temp script."
        )
      }
    )
  }

  /// Runs the generated script against a stub CLI that fails, confirming the
  /// failure path: status propagates, log captures the failure message.
  func testGeneratedScriptPropagatesFailureExitCode() throws {
    try runScriptHarness(
      cliExitStatus: 17,
      cliStdout: "fake-cli pretending to fail\n",
      expecting: { result in
        XCTAssertEqual(
          result.terminationStatus, 17,
          "CLI exit status must propagate so callers can detect failure."
        )
        XCTAssertTrue(result.logContents.contains("Update failed."))
        XCTAssertTrue(result.logContents.contains("Details:"))
      }
    )
  }

  // MARK: - Harness

  private struct HarnessResult {
    let terminationStatus: Int32
    let scriptPath: String
    let logContents: String
  }

  private func runScriptHarness(
    cliExitStatus: Int32,
    cliStdout: String,
    expecting verify: (HarnessResult) -> Void
  ) throws {
    let workDir = FileManager.default.temporaryDirectory
      .appendingPathComponent("updater-script-\(UUID().uuidString)", isDirectory: true)
    defer { try? FileManager.default.removeItem(at: workDir) }
    try FileManager.default.createDirectory(at: workDir, withIntermediateDirectories: true)

    // Stub `1context-cli`.
    let cliPath = workDir.appendingPathComponent("fake-cli").path
    let cliBody = """
    #!/bin/sh
    printf '%s' \(shellSingleQuote(cliStdout))
    exit \(cliExitStatus)
    """
    try cliBody.write(toFile: cliPath, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: cliPath)

    // Stub the menu-bar alert binary.
    let alertPath = workDir.appendingPathComponent("fake-alert").path
    try "#!/bin/sh\nexit 0\n".write(toFile: alertPath, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: alertPath)

    // Stub `osascript` so tests don't pop up dialogs on dev machines or CI.
    let stubBinDir = workDir.appendingPathComponent("bin").path
    try FileManager.default.createDirectory(atPath: stubBinDir, withIntermediateDirectories: true)
    let osascriptStub = "\(stubBinDir)/osascript"
    try "#!/bin/sh\nexit 0\n".write(toFile: osascriptStub, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: osascriptStub)

    let logDir = workDir.appendingPathComponent("logs").path
    let scriptPath = workDir.appendingPathComponent("script.zsh").path
    let scriptSource = UpdaterScript.render(
      cliExecutable: cliPath,
      alertExecutable: alertPath,
      logDirectory: logDir
    )
    try scriptSource.write(toFile: scriptPath, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o755], ofItemAtPath: scriptPath)

    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/zsh")
    process.arguments = [scriptPath]
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice
    process.environment = [
      "PATH": "\(stubBinDir):/usr/bin:/bin",
      "HOME": NSHomeDirectory(),
      "TERM": "dumb"
    ]
    try process.run()
    process.waitUntilExit()

    let entries = (try? FileManager.default.contentsOfDirectory(atPath: logDir)) ?? []
    let logs = entries.filter { $0.hasPrefix("update-") && $0.hasSuffix(".log") }
    XCTAssertEqual(logs.count, 1, "Expected exactly one update log; got \(logs)")
    let logContents = try String(contentsOfFile: "\(logDir)/\(logs[0])", encoding: .utf8)

    verify(
      HarnessResult(
        terminationStatus: process.terminationStatus,
        scriptPath: scriptPath,
        logContents: logContents
      )
    )
  }

  private func shellSingleQuote(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }
}
