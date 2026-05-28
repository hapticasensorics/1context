import Darwin
import Foundation

public struct ProcessRunResult: Sendable {
  public let status: Int32
  public let stdout: Data
  public let stderr: Data
  public let timedOut: Bool

  public var stdoutText: String {
    String(data: stdout, encoding: .utf8) ?? ""
  }

  public var stderrText: String {
    String(data: stderr, encoding: .utf8) ?? ""
  }
}

public enum ProcessRunner {
  public static func run(
    executable: URL,
    arguments: [String],
    environment: [String: String]? = nil,
    standardInput: Data? = nil,
    timeoutSeconds: TimeInterval? = nil
  ) throws -> ProcessRunResult {
    let process = Process()
    let stdout = Pipe()
    let stderr = Pipe()
    let stdin = standardInput.map { _ in Pipe() }
    let outputBox = ProcessRunnerDataBox()
    let errorBox = ProcessRunnerDataBox()
    let readGroup = DispatchGroup()

    process.executableURL = executable
    process.arguments = arguments
    process.environment = environment
    process.standardOutput = stdout
    process.standardError = stderr
    if let stdin {
      process.standardInput = stdin
    }

    try process.run()
    if let standardInput, let stdin {
      stdin.fileHandleForWriting.write(standardInput)
      try? stdin.fileHandleForWriting.close()
    }

    read(stdout, into: outputBox, group: readGroup)
    read(stderr, into: errorBox, group: readGroup)

    let timedOut = wait(for: process, timeoutSeconds: timeoutSeconds)
    if timedOut {
      terminate(process)
    }

    process.waitUntilExit()
    _ = readGroup.wait(timeout: .now() + 1)
    return ProcessRunResult(
      status: timedOut ? 124 : process.terminationStatus,
      stdout: outputBox.get(),
      stderr: errorBox.get(),
      timedOut: timedOut
    )
  }

  private static func read(_ pipe: Pipe, into box: ProcessRunnerDataBox, group: DispatchGroup) {
    group.enter()
    DispatchQueue.global(qos: .utility).async {
      box.set(pipe.fileHandleForReading.readDataToEndOfFile())
      group.leave()
    }
  }

  private static func wait(for process: Process, timeoutSeconds: TimeInterval?) -> Bool {
    guard let timeoutSeconds else {
      process.waitUntilExit()
      return false
    }
    let deadline = Date().addingTimeInterval(timeoutSeconds)
    while process.isRunning && Date() < deadline {
      usleep(10_000)
    }
    return process.isRunning
  }

  private static func terminate(_ process: Process) {
    process.terminate()
    usleep(100_000)
    if process.isRunning {
      kill(process.processIdentifier, SIGKILL)
    }
  }
}

private final class ProcessRunnerDataBox: @unchecked Sendable {
  private let lock = NSLock()
  private var data = Data()

  func set(_ data: Data) {
    lock.lock()
    self.data = data
    lock.unlock()
  }

  func get() -> Data {
    lock.lock()
    let value = data
    lock.unlock()
    return value
  }
}
