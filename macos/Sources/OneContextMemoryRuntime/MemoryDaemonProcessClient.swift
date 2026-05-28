import Darwin
import Foundation
import OneContextCore
import OneContextPlatform

private let defaultDevMemoryDatabaseURL = "postgres://onecontext:onecontext_dev@127.0.0.1:15432/onecontext_memory?connect_timeout=1"

public struct MemoryDaemonSnapshot {
  public let configured: Bool
  public let running: Bool
  public let executable: String?
  public let pid: Int32?
  public let statusPath: String
  public let cursorPath: String
  public let status: [String: Any]

  public var payload: [String: Any] {
    var payload: [String: Any] = [
      "surface": "memory_daemon_status",
      "configured": configured,
      "running": running,
      "status_path": statusPath,
      "cursor_path": cursorPath,
      "status": status
    ]
    if let executable {
      payload["executable"] = executable
    }
    if let pid {
      payload["pid"] = Int(pid)
    }
    return payload
  }
}

public struct MemoryDaemonProcessError: Error, LocalizedError {
  public let message: String

  public var errorDescription: String? {
    message
  }
}

public final class MemoryDaemonProcessClient: @unchecked Sendable {
  private let runtimePaths: RuntimePaths
  private let fileManager: FileManager
  private let environment: [String: String]
  private let sources: String
  private let intervalMilliseconds: Int
  private let maxEvents: Int
  private let maxLines: Int
  private let processLock = NSLock()
  private var process: Process?

  public init(
    runtimePaths: RuntimePaths,
    fileManager: FileManager = .default,
    environment: [String: String] = ProcessInfo.processInfo.environment,
    sources: String = "codex,claude,imessage",
    intervalMilliseconds: Int = 60_000,
    maxEvents: Int = 1_000,
    maxLines: Int = 50_000
  ) {
    self.runtimePaths = runtimePaths
    self.fileManager = fileManager
    self.environment = Self.environmentWithDevMemoryDefaults(
      environment,
      identity: runtimePaths.identity
    )
    self.sources = sources
    self.intervalMilliseconds = intervalMilliseconds
    self.maxEvents = maxEvents
    self.maxLines = maxLines
  }

  public func startIfAvailable(log: @escaping @Sendable (String) -> Void) {
    processLock.lock()
    defer { processLock.unlock() }

    if let process, process.isRunning {
      return
    }

    guard let executable = discoverExecutable() else {
      log("memory daemon skipped: onecontext-memoryd executable not found")
      return
    }

    do {
      try RuntimePermissions.ensurePrivateDirectory(runtimePaths.runDirectory)
      try RuntimePermissions.ensurePrivateDirectory(runtimePaths.contextEngineDirectory)
      terminateExistingHelperIfNeeded(log: log)
      let process = Process()
      process.executableURL = executable
      process.arguments = daemonArguments()
      process.environment = processEnvironment()

      let stdout = Pipe()
      let stderr = Pipe()
      process.standardOutput = stdout
      process.standardError = stderr
      attachPipe(stdout, label: "memoryd", log: log)
      attachPipe(stderr, label: "memoryd", log: log)
      process.terminationHandler = { [weak self] process in
        stdout.fileHandleForReading.readabilityHandler = nil
        stderr.fileHandleForReading.readabilityHandler = nil
        log("memory daemon exited status=\(process.terminationStatus)")
        self?.clearProcessIfCurrent(process)
      }

      try process.run()
      self.process = process
      log("memory daemon started pid=\(process.processIdentifier) path=\(executable.path)")
    } catch {
      log("memory daemon failed to start: \(error.localizedDescription)")
    }
  }

  public func stop() {
    processLock.lock()
    let process = self.process
    self.process = nil
    processLock.unlock()

    guard let process, process.isRunning else { return }
    process.terminate()
  }

  public func status() -> MemoryDaemonSnapshot {
    let executable = discoverExecutable()
    let running = isRunning()
    let pid = running ? processIdentifier() : nil
    return MemoryDaemonSnapshot(
      configured: executable != nil,
      running: running,
      executable: executable?.path,
      pid: pid,
      statusPath: statusPath.path,
      cursorPath: cursorPath.path,
      status: readStatus()
    )
  }

  public func benchmark(maxEvents: Int = 1_000, maxLines: Int = 50_000) throws -> [String: Any] {
    guard let executable = discoverExecutable() else {
      throw MemoryDaemonProcessError(message: "onecontext-memoryd executable missing")
    }

    let result = try ProcessRunner.run(
      executable: executable,
      arguments: [
        "bench",
        "--home", fileManager.homeDirectoryForCurrentUser.path,
        "--context-engine-root", runtimePaths.contextEngineDirectory.path,
        "--run-dir", runtimePaths.runDirectory.path,
        "--sources", sources,
        "--max-events", "\(maxEvents)",
        "--max-lines", "\(maxLines)"
      ],
      environment: processEnvironment()
    )
    guard result.status == 0 else {
      let message = String(decoding: result.stderr.isEmpty ? result.stdout : result.stderr, as: UTF8.self)
      throw MemoryDaemonProcessError(message: message.trimmingCharacters(in: .whitespacesAndNewlines))
    }
    guard let object = try JSONSerialization.jsonObject(with: result.stdout) as? [String: Any] else {
      throw MemoryDaemonProcessError(message: "onecontext-memoryd bench returned non-object JSON")
    }
    return object
  }

  public func queryViewport(_ query: MemoryViewportQuery = MemoryViewportQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .queryViewport(query)
      .payload
  }

  public func hydrateObjects(_ query: MemoryObjectHydrationQuery) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .hydrateObject(query)
      .payload
  }

  public func queryDensity(_ query: MemoryDensityQuery = MemoryDensityQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .queryDensity(query)
      .payload
  }

  public func queryEdges(_ query: MemoryEdgesQuery) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .queryEdges(query)
      .payload
  }

  public func searchText(_ query: MemorySearchTextQuery) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .searchText(query)
      .payload
  }

  public func viewport(limit: Int = 200, source: String? = nil) throws -> [String: Any] {
    try queryViewport(MemoryViewportQuery(limit: limit, source: source))
  }

  public func discoverExecutable() -> URL? {
    executableCandidates().first { fileManager.isExecutableFile(atPath: $0.path) }
  }

  private func daemonArguments() -> [String] {
    [
      "daemon",
      "--home", fileManager.homeDirectoryForCurrentUser.path,
      "--context-engine-root", runtimePaths.contextEngineDirectory.path,
      "--run-dir", runtimePaths.runDirectory.path,
      "--sources", sources,
      "--interval-ms", "\(intervalMilliseconds)",
      "--max-events", "\(maxEvents)",
      "--max-lines", "\(maxLines)"
    ]
  }

  private func processEnvironment() -> [String: String] {
    var env = environment
    env[OneContextAppIdentity.environmentKey] = runtimePaths.identity.environmentValue
    return env
  }

  private static func environmentWithDevMemoryDefaults(
    _ environment: [String: String],
    identity: OneContextAppIdentity
  ) -> [String: String] {
    guard identity.kind == .dev else { return environment }
    var env = environment
    if env["ONECONTEXT_MEMORY_DB_URL"]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false,
      env["ONECONTEXT_MEMORY_DATABASE_URL"]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false,
      env["DATABASE_URL"]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty != false
    {
      env["ONECONTEXT_MEMORY_DB_URL"] = defaultDevMemoryDatabaseURL
    }
    return env
  }

  private func executableCandidates() -> [URL] {
    var candidates: [URL] = []
    if let override = environment["ONECONTEXT_MEMORYD_BIN"], !override.isEmpty {
      return [URL(fileURLWithPath: override)]
    }

    if let executableDirectory = Bundle.main.executableURL?.deletingLastPathComponent() {
      candidates.append(executableDirectory.appendingPathComponent("onecontext-memoryd"))
    }

    let cwd = URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true)
    candidates.append(cwd.appendingPathComponent("target/debug/onecontext-memoryd"))
    candidates.append(cwd.appendingPathComponent("target/release/onecontext-memoryd"))
    candidates.append(cwd.appendingPathComponent("../target/debug/onecontext-memoryd"))
    candidates.append(cwd.appendingPathComponent("../target/release/onecontext-memoryd"))
    return candidates
  }

  private func protocolConfiguration() -> MemoryProtocolProcessConfiguration? {
    discoverExecutable().map {
      MemoryProtocolProcessConfiguration(
        executable: $0,
        environment: processEnvironment()
      )
    }
  }

  private var statusPath: URL {
    runtimePaths.runDirectory.appendingPathComponent("memoryd-status.json")
  }

  private var cursorPath: URL {
    runtimePaths.contextEngineDirectory.appendingPathComponent("memory-db/cursors/local-source-cursors.json")
  }

  private var pidPath: URL {
    runtimePaths.runDirectory.appendingPathComponent("memoryd.pid")
  }

  private func terminateExistingHelperIfNeeded(log: @escaping @Sendable (String) -> Void) {
    guard let text = try? String(contentsOf: pidPath, encoding: .utf8),
      let pid = Int32(text.trimmingCharacters(in: .whitespacesAndNewlines)),
      pid > 0,
      processIsAlive(pid)
    else {
      return
    }

    log("memory daemon terminating stale helper pid=\(pid)")
    kill(pid, SIGTERM)
    for _ in 0..<20 {
      usleep(50_000)
      if !processIsAlive(pid) {
        return
      }
    }
    kill(pid, SIGKILL)
  }

  private func processIsAlive(_ pid: Int32) -> Bool {
    if kill(pid, 0) == 0 {
      return true
    }
    return errno == EPERM
  }

  private func readStatus() -> [String: Any] {
    guard let data = try? Data(contentsOf: statusPath),
      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
      return ["status": isRunning() ? "starting" : "not running"]
    }
    return object
  }

  private func isRunning() -> Bool {
    processLock.lock()
    defer { processLock.unlock() }
    return process?.isRunning == true
  }

  private func processIdentifier() -> Int32? {
    processLock.lock()
    defer { processLock.unlock() }
    guard let process, process.isRunning else { return nil }
    return process.processIdentifier
  }

  private func clearProcessIfCurrent(_ terminated: Process) {
    processLock.lock()
    if process === terminated {
      process = nil
    }
    processLock.unlock()
  }

  private func attachPipe(_ pipe: Pipe, label: String, log: @escaping @Sendable (String) -> Void) {
    pipe.fileHandleForReading.readabilityHandler = { handle in
      let data = handle.availableData
      guard !data.isEmpty else { return }
      let text = String(decoding: data, as: UTF8.self)
        .trimmingCharacters(in: .whitespacesAndNewlines)
      if !text.isEmpty {
        log("\(label): \(text)")
      }
    }
  }
}
