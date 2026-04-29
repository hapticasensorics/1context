import Darwin
import Foundation
import OneContextPlatform

public struct MemoryCorePaths: Equatable, Sendable {
  public let directory: URL
  public let configFile: URL
  public let stateFile: URL
  public let logFile: URL

  public init(directory: URL, logFile: URL) {
    self.directory = directory
    self.configFile = directory.appendingPathComponent("config.json")
    self.stateFile = directory.appendingPathComponent("state.json")
    self.logFile = logFile
  }

  public static func current(environment: [String: String] = ProcessInfo.processInfo.environment) -> MemoryCorePaths {
    let runtime = RuntimePaths.current(environment: environment)
    let directory = URL(
      fileURLWithPath: environment["ONECONTEXT_MEMORY_CORE_DIR"]
        ?? runtime.appSupportDirectory.appendingPathComponent("memory-core", isDirectory: true).path,
      isDirectory: true
    )
    let logFile = URL(
      fileURLWithPath: environment["ONECONTEXT_MEMORY_CORE_LOG_PATH"]
        ?? runtime.logDirectory.appendingPathComponent("memory-core.log").path
    )
    return MemoryCorePaths(directory: directory, logFile: logFile)
  }
}

public struct MemoryCoreConfig: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var enabled: Bool
  public var executable: String?
  public var defaultTimeoutSeconds: Double
  public var allowedCommands: [String]

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case enabled
    case executable
    case defaultTimeoutSeconds = "default_timeout_seconds"
    case allowedCommands = "allowed_commands"
  }

  public init(
    schemaVersion: Int = 1,
    enabled: Bool = false,
    executable: String? = nil,
    defaultTimeoutSeconds: Double = 10,
    allowedCommands: [String] = Self.defaultAllowedCommands
  ) {
    self.schemaVersion = schemaVersion
    self.enabled = enabled
    self.executable = executable
    self.defaultTimeoutSeconds = defaultTimeoutSeconds
    self.allowedCommands = allowedCommands
  }

  public static let defaultAllowedCommands = [
    "status",
    "storage",
    "wiki",
    "memory"
  ]
}

public struct MemoryCoreState: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var updatedAt: Date
  public var lastCheckedAt: Date?
  public var lastStatus: String?
  public var lastError: String?

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case updatedAt = "updated_at"
    case lastCheckedAt = "last_checked_at"
    case lastStatus = "last_status"
    case lastError = "last_error"
  }

  public init(
    schemaVersion: Int = 1,
    updatedAt: Date = Date(),
    lastCheckedAt: Date? = nil,
    lastStatus: String? = nil,
    lastError: String? = nil
  ) {
    self.schemaVersion = schemaVersion
    self.updatedAt = updatedAt
    self.lastCheckedAt = lastCheckedAt
    self.lastStatus = lastStatus
    self.lastError = lastError
  }
}

public struct MemoryCoreStatus: Equatable, Sendable {
  public enum Health: String, Equatable, Sendable {
    case notConfigured = "not configured"
    case disabled
    case ok
    case degraded
  }

  public var configured: Bool
  public var enabled: Bool
  public var executable: String?
  public var health: Health
  public var lastCheckedAt: Date?
  public var lastError: String?
  public var paths: MemoryCorePaths

  public init(
    configured: Bool,
    enabled: Bool,
    executable: String?,
    health: Health,
    lastCheckedAt: Date? = nil,
    lastError: String? = nil,
    paths: MemoryCorePaths
  ) {
    self.configured = configured
    self.enabled = enabled
    self.executable = executable
    self.health = health
    self.lastCheckedAt = lastCheckedAt
    self.lastError = lastError
    self.paths = paths
  }
}

public struct MemoryCoreRunResult: Equatable, Sendable {
  public var stdout: String
  public var stderr: String
  public var exitCode: Int32

  public init(stdout: String, stderr: String, exitCode: Int32) {
    self.stdout = stdout
    self.stderr = stderr
    self.exitCode = exitCode
  }
}

public enum MemoryCoreError: Error, LocalizedError, Equatable {
  case notConfigured
  case disabled
  case missingExecutable(String)
  case notExecutable(String)
  case commandNotAllowed(String)
  case missingRunArguments
  case processFailed(String)
  case timeout(Double)
  case invalidJSON
  case invalidContract(String)

  public var errorDescription: String? {
    switch self {
    case .notConfigured:
      return "Memory core is not configured"
    case .disabled:
      return "Memory core is disabled"
    case .missingExecutable(let path):
      return "Memory core executable is missing: \(path)"
    case .notExecutable(let path):
      return "Memory core executable is not executable: \(path)"
    case .commandNotAllowed(let command):
      return "Memory core command is not allowed: \(command)"
    case .missingRunArguments:
      return "Memory core run requires arguments after --"
    case .processFailed(let detail):
      return detail.isEmpty ? "Memory core process failed" : detail
    case .timeout(let seconds):
      return "Memory core process timed out after \(seconds)s"
    case .invalidJSON:
      return "Memory core returned invalid JSON"
    case .invalidContract(let detail):
      return detail.isEmpty ? "Memory core returned JSON outside the public contract" : detail
    }
  }
}

public final class MemoryCoreAdapter {
  private let paths: MemoryCorePaths
  private let processRunner: MemoryCoreProcessRunning
  private let fileManager: FileManager

  public init(
    paths: MemoryCorePaths = .current(),
    processRunner: MemoryCoreProcessRunning = MemoryCoreProcessRunner(),
    fileManager: FileManager = .default
  ) {
    self.paths = paths
    self.processRunner = processRunner
    self.fileManager = fileManager
  }

  public func config() -> MemoryCoreConfig {
    guard let data = try? Data(contentsOf: paths.configFile),
      let config = try? JSONDecoder().decode(MemoryCoreConfig.self, from: data)
    else {
      return MemoryCoreConfig()
    }
    return config
  }

  public func configure(executable: String) throws -> MemoryCoreStatus {
    try ensurePrivateStorage()
    var config = config()
    config.enabled = true
    config.executable = executable
    try writeConfig(config)
    let status = self.status(forceCheck: true)
    try writeState(status: status)
    return status
  }

  public func clear() throws -> MemoryCoreStatus {
    try ensurePrivateStorage()
    try writeConfig(MemoryCoreConfig())
    let status = MemoryCoreStatus(
      configured: false,
      enabled: false,
      executable: nil,
      health: .notConfigured,
      paths: paths
    )
    try writeState(status: status)
    return status
  }

  public func status(forceCheck: Bool = true) -> MemoryCoreStatus {
    let config = config()
    guard config.enabled || config.executable != nil else {
      return MemoryCoreStatus(
        configured: false,
        enabled: false,
        executable: config.executable,
        health: .notConfigured,
        paths: paths
      )
    }

    guard config.enabled else {
      return MemoryCoreStatus(
        configured: true,
        enabled: false,
        executable: config.executable,
        health: .disabled,
        paths: paths
      )
    }

    guard let executable = config.executable, !executable.isEmpty else {
      return MemoryCoreStatus(
        configured: false,
        enabled: true,
        executable: nil,
        health: .notConfigured,
        paths: paths
      )
    }

    guard fileManager.fileExists(atPath: executable) else {
      return degraded(executable: executable, error: MemoryCoreError.missingExecutable(executable).localizedDescription)
    }
    guard fileManager.isExecutableFile(atPath: executable) else {
      return degraded(executable: executable, error: MemoryCoreError.notExecutable(executable).localizedDescription)
    }

    guard forceCheck else {
      return MemoryCoreStatus(
        configured: true,
        enabled: true,
        executable: executable,
        health: .ok,
        paths: paths
      )
    }

    do {
      _ = try probe(executable: executable, timeout: min(config.defaultTimeoutSeconds, 5))
      let checked = Date()
      let status = MemoryCoreStatus(
        configured: true,
        enabled: true,
        executable: executable,
        health: .ok,
        lastCheckedAt: checked,
        paths: paths
      )
      try? writeState(status: status)
      return status
    } catch {
      let checked = Date()
      let status = MemoryCoreStatus(
        configured: true,
        enabled: true,
        executable: executable,
        health: .degraded,
        lastCheckedAt: checked,
        lastError: error.localizedDescription,
        paths: paths
      )
      try? writeState(status: status)
      return status
    }
  }

  public func doctor() -> MemoryCoreStatus {
    do {
      try ensurePrivateStorage()
    } catch {
      return MemoryCoreStatus(
        configured: false,
        enabled: false,
        executable: config().executable,
        health: .degraded,
        lastCheckedAt: Date(),
        lastError: error.localizedDescription,
        paths: paths
      )
    }
    return status(forceCheck: true)
  }

  public func run(arguments: [String], stdinData: Data = Data()) throws -> MemoryCoreRunResult {
    let config = config()
    guard config.enabled, let executable = config.executable, !executable.isEmpty else {
      throw config.executable == nil ? MemoryCoreError.notConfigured : MemoryCoreError.disabled
    }
    guard fileManager.fileExists(atPath: executable) else {
      throw MemoryCoreError.missingExecutable(executable)
    }
    guard fileManager.isExecutableFile(atPath: executable) else {
      throw MemoryCoreError.notExecutable(executable)
    }
    guard let command = arguments.first else {
      throw MemoryCoreError.missingRunArguments
    }
    try Self.validateCommandShape(arguments, allowedTopLevelCommands: config.allowedCommands)

    let result = try processRunner.run(
      executable: executable,
      arguments: arguments,
      stdinData: stdinData,
      timeout: config.defaultTimeoutSeconds
    )
    guard result.exitCode == 0 else {
      try? appendLog("run command=\(command) exit=\(result.exitCode)")
      throw MemoryCoreError.processFailed(Self.redactDiagnostic(result.stderr.trimmingCharacters(in: .whitespacesAndNewlines)))
    }
    guard Self.isJSON(result.stdout) else {
      try? appendLog("run command=\(command) invalid_json")
      throw MemoryCoreError.invalidJSON
    }
    try Self.validateContractJSON(result.stdout)
    if !result.stderr.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      try? appendLog("run command=\(command) stderr_bytes=\(result.stderr.utf8.count)")
    }
    try? appendLog("run command=\(command) exit=0")
    return result
  }

  public func renderStatus(_ status: MemoryCoreStatus, redact: (String) -> String = { $0 }) -> String {
    var lines = [
      "Memory Core",
      "",
      "Configured: \(status.configured ? "yes" : "no")",
      "Enabled: \(status.enabled ? "yes" : "no")",
      "Executable: \(status.executable.map(redact) ?? "missing")",
      "Health: \(status.health.rawValue)",
      "Config: \(redact(status.paths.configFile.path))",
      "State: \(redact(status.paths.stateFile.path))",
      "Log: \(redact(status.paths.logFile.path))"
    ]
    if let lastCheckedAt = status.lastCheckedAt {
      lines.append("Last Checked: \(ISO8601DateFormatter().string(from: lastCheckedAt))")
    }
    if let lastError = status.lastError {
      lines.append("Last Error: \(redact(lastError))")
    }
    return lines.joined(separator: "\n")
  }

  public static func isJSON(_ text: String) -> Bool {
    guard let data = text.data(using: .utf8) else { return false }
    do {
      _ = try JSONSerialization.jsonObject(with: data)
      return true
    } catch {
      return false
    }
  }

  public static func validateCommandShape(_ arguments: [String], allowedTopLevelCommands: [String]) throws {
    guard let command = arguments.first else {
      throw MemoryCoreError.missingRunArguments
    }
    guard allowedTopLevelCommands.contains(command) else {
      throw MemoryCoreError.commandNotAllowed(command)
    }

    let allowedShapes: Set<[String]> = [
      ["status", "--json"],
      ["storage", "init", "--json"],
      ["wiki", "list", "--json"],
      ["wiki", "ensure", "--json"],
      ["wiki", "render", "--json"],
      ["wiki", "routes", "--json"],
      ["memory", "tick", "--wiki-only", "--json"],
      ["memory", "replay-dry-run", "--json"],
      ["memory", "cycles", "list", "--json"],
      ["memory", "cycles", "show", "--json"],
      ["memory", "cycles", "validate", "--json"]
    ]

    guard allowedShapes.contains(arguments) else {
      throw MemoryCoreError.commandNotAllowed(arguments.joined(separator: " "))
    }
  }

  public static func validateContractJSON(_ text: String) throws {
    guard let data = text.data(using: .utf8) else {
      throw MemoryCoreError.invalidJSON
    }
    let object: Any
    do {
      object = try JSONSerialization.jsonObject(with: data)
    } catch {
      throw MemoryCoreError.invalidJSON
    }
    guard let payload = object as? [String: Any] else {
      throw MemoryCoreError.invalidContract("Memory core JSON must be an object")
    }
    guard payload["status"] as? String == "ok" else {
      throw MemoryCoreError.invalidContract("Memory core JSON must include status: ok")
    }
    guard let schemaVersion = payload["schema_version"] as? Int, schemaVersion >= 1 else {
      throw MemoryCoreError.invalidContract("Memory core JSON must include schema_version")
    }
  }

  public static func redactDiagnostic(_ text: String, limit: Int = 800) -> String {
    var redacted = text
    let home = FileManager.default.homeDirectoryForCurrentUser.path
    redacted = redacted.replacingOccurrences(of: home, with: "~")
    redacted = redacted.replacingOccurrences(of: NSTemporaryDirectory(), with: "$TMPDIR/")
    if redacted.count > limit {
      let index = redacted.index(redacted.startIndex, offsetBy: limit)
      redacted = String(redacted[..<index]) + "... [truncated]"
    }
    return redacted
  }

  private func probe(executable: String, timeout: Double) throws -> MemoryCoreRunResult {
    let status = try processRunner.run(
      executable: executable,
      arguments: ["status", "--json"],
      stdinData: Data(),
      timeout: timeout
    )
    guard status.exitCode == 0 else {
      throw MemoryCoreError.processFailed(Self.redactDiagnostic(status.stderr.trimmingCharacters(in: .whitespacesAndNewlines)))
    }
    try Self.validateContractJSON(status.stdout)
    return status
  }

  private func degraded(executable: String?, error: String) -> MemoryCoreStatus {
    MemoryCoreStatus(
      configured: executable != nil,
      enabled: true,
      executable: executable,
      health: .degraded,
      lastCheckedAt: Date(),
      lastError: error,
      paths: paths
    )
  }

  private func ensurePrivateStorage() throws {
    try RuntimePermissions.ensurePrivateDirectory(paths.directory)
    try RuntimePermissions.ensurePrivateDirectory(paths.logFile.deletingLastPathComponent())
  }

  private func writeConfig(_ config: MemoryCoreConfig) throws {
    try ensurePrivateStorage()
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try RuntimePermissions.writePrivateData(try encoder.encode(config), to: paths.configFile)
  }

  private func writeState(status: MemoryCoreStatus) throws {
    try ensurePrivateStorage()
    let state = MemoryCoreState(
      updatedAt: Date(),
      lastCheckedAt: status.lastCheckedAt,
      lastStatus: status.health.rawValue,
      lastError: status.lastError
    )
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try RuntimePermissions.writePrivateData(try encoder.encode(state), to: paths.stateFile)
  }

  private func appendLog(_ line: String) throws {
    try ensurePrivateStorage()
    let text = "\(ISO8601DateFormatter().string(from: Date())) \(line)\n"
    if fileManager.fileExists(atPath: paths.logFile.path) {
      let handle = try FileHandle(forWritingTo: paths.logFile)
      defer { try? handle.close() }
      try handle.seekToEnd()
      handle.write(Data(text.utf8))
      chmod(paths.logFile.path, RuntimePermissions.privateFileMode)
    } else {
      try RuntimePermissions.writePrivateData(Data(text.utf8), to: paths.logFile)
    }
  }
}

public protocol MemoryCoreProcessRunning: Sendable {
  func run(executable: String, arguments: [String], stdinData: Data, timeout: Double) throws -> MemoryCoreRunResult
}

public struct MemoryCoreProcessRunner: MemoryCoreProcessRunning {
  private let environment: [String: String]

  public init(environment: [String: String] = ProcessInfo.processInfo.environment) {
    self.environment = environment
  }

  public func run(
    executable: String,
    arguments: [String],
    stdinData: Data,
    timeout: Double
  ) throws -> MemoryCoreRunResult {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    process.environment = sanitizedEnvironment()

    let stdinPipe = Pipe()
    let captureDirectory = URL(fileURLWithPath: NSTemporaryDirectory(), isDirectory: true)
      .appendingPathComponent("1context-memory-core-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: captureDirectory, withIntermediateDirectories: true)
    chmod(captureDirectory.path, RuntimePermissions.privateDirectoryMode)
    defer { try? FileManager.default.removeItem(at: captureDirectory) }
    let stdoutURL = captureDirectory.appendingPathComponent("stdout")
    let stderrURL = captureDirectory.appendingPathComponent("stderr")
    FileManager.default.createFile(atPath: stdoutURL.path, contents: nil)
    FileManager.default.createFile(atPath: stderrURL.path, contents: nil)
    chmod(stdoutURL.path, RuntimePermissions.privateFileMode)
    chmod(stderrURL.path, RuntimePermissions.privateFileMode)
    let stdoutHandle = try FileHandle(forWritingTo: stdoutURL)
    let stderrHandle = try FileHandle(forWritingTo: stderrURL)
    defer {
      try? stdoutHandle.close()
      try? stderrHandle.close()
    }
    process.standardInput = stdinPipe
    process.standardOutput = stdoutHandle
    process.standardError = stderrHandle

    let group = DispatchGroup()
    group.enter()
    process.terminationHandler = { _ in group.leave() }

    do {
      try process.run()
      if !stdinData.isEmpty {
        try stdinPipe.fileHandleForWriting.write(contentsOf: stdinData)
      }
      try? stdinPipe.fileHandleForWriting.close()
    } catch {
      return MemoryCoreRunResult(stdout: "", stderr: error.localizedDescription, exitCode: 1)
    }

    let deadline = DispatchTime.now() + timeout
    if group.wait(timeout: deadline) == .timedOut {
      process.terminate()
      if group.wait(timeout: .now() + 1) == .timedOut {
        killProcessTree(rootPID: process.processIdentifier, signal: SIGKILL)
        _ = group.wait(timeout: .now() + 1)
      }
      throw MemoryCoreError.timeout(timeout)
    }

    try? stdoutHandle.synchronize()
    try? stderrHandle.synchronize()
    let stdout = (try? String(contentsOf: stdoutURL, encoding: .utf8)) ?? ""
    let stderr = (try? String(contentsOf: stderrURL, encoding: .utf8)) ?? ""

    return MemoryCoreRunResult(stdout: stdout, stderr: stderr, exitCode: process.terminationStatus)
  }

  private func sanitizedEnvironment() -> [String: String] {
    let keep = [
      "HOME",
      "PATH",
      "SHELL",
      "TMPDIR",
      "USER",
      "LOGNAME",
      "LANG",
      "LC_ALL",
      "LC_CTYPE",
      "ONECONTEXT_APP_SUPPORT_DIR",
      "ONECONTEXT_USER_CONTENT_DIR",
      "ONECONTEXT_LOG_DIR",
      "ONECONTEXT_CACHE_DIR",
      "ONECONTEXT_UPDATE_STATE_DIR",
      "ONECONTEXT_MEMORY_CORE_DIR",
      "ONECONTEXT_MEMORY_CORE_LOG_PATH"
    ]
    var result: [String: String] = [:]
    for key in keep {
      if let value = environment[key] {
        result[key] = value
      }
    }
    if result["PATH"] == nil {
      result["PATH"] = "/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:/usr/sbin:/sbin"
    }
    return result
  }

  private func killProcessTree(rootPID: Int32, signal: Int32) {
    for child in childPIDs(of: rootPID) {
      killProcessTree(rootPID: child, signal: signal)
    }
    kill(rootPID, signal)
  }

  private func childPIDs(of pid: Int32) -> [Int32] {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/usr/bin/pgrep")
    process.arguments = ["-P", "\(pid)"]
    let stdout = Pipe()
    process.standardOutput = stdout
    process.standardError = Pipe()
    do {
      try process.run()
      process.waitUntilExit()
    } catch {
      return []
    }
    guard process.terminationStatus == 0 else { return [] }
    let data = stdout.fileHandleForReading.readDataToEndOfFile()
    let text = String(data: data, encoding: .utf8) ?? ""
    return text
      .split(whereSeparator: \.isNewline)
      .compactMap { Int32($0.trimmingCharacters(in: .whitespacesAndNewlines)) }
  }
}
