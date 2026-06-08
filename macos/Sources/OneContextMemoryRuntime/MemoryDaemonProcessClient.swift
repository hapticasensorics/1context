import Darwin
import Foundation
import OneContextCore
import OneContextPlatform

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
  private struct PIDRecord {
    let pid: Int32
    let executable: String?
    let startedAtMilliseconds: Int?
    let kind: String?
    let trusted: Bool
  }

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
    self.environment = Self.environmentWithStorageDefaults(
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
      terminateExistingHelperIfNeeded(expectedExecutable: executable, log: log)
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
    _ = try? stopStorage(MemoryStorageLifecycleQuery(reason: "app-shutdown"))

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

  public func storageHealth(_ query: MemoryStorageHealthQuery = MemoryStorageHealthQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .storageHealth(query)
      .payload
  }

  public func ensureStorageReady(_ query: MemoryEnsureStorageReadyQuery = MemoryEnsureStorageReadyQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .ensureStorageReady(query)
      .payload
  }

  public func ensureRecentBackfill(_ query: MemoryEnsureRecentBackfillQuery = MemoryEnsureRecentBackfillQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .ensureRecentBackfill(query)
      .payload
  }

  public func repairStorage(_ query: MemoryRepairStorageQuery = MemoryRepairStorageQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .repairStorage(query)
      .payload
  }

  public func stopStorage(_ query: MemoryStorageLifecycleQuery = MemoryStorageLifecycleQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .stopStorage(query)
      .payload
  }

  public func restartStorage(_ query: MemoryStorageLifecycleQuery = MemoryStorageLifecycleQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .restartStorage(query)
      .payload
  }

  public func storageDiagnostics(_ query: MemoryStorageDiagnosticsQuery = MemoryStorageDiagnosticsQuery()) throws -> [String: Any] {
    try MemoryProtocolClientFactory
      .make(configuration: protocolConfiguration())
      .storageDiagnostics(query)
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

  private static func environmentWithStorageDefaults(
    _ environment: [String: String],
    identity: OneContextAppIdentity
  ) -> [String: String] {
    var env = environment
    let allowExternalPostgres = env["ONECONTEXT_ALLOW_EXTERNAL_POSTGRES"] == "1"
    let explicitDatabaseURL = allowExternalPostgres ? normalizedExplicitDatabaseURL(from: env) : nil
    if let explicitDatabaseURL {
      env["ONECONTEXT_MEMORY_DB_URL"] = explicitDatabaseURL
    } else {
      env.removeValue(forKey: "ONECONTEXT_MEMORY_DB_URL")
    }
    let storageBackend = env["ONECONTEXT_STORAGE_BACKEND"]?
      .trimmingCharacters(in: .whitespacesAndNewlines)
      .lowercased()
      .replacingOccurrences(of: "-", with: "_")
    if storageBackend?.isEmpty != false || (!allowExternalPostgres && storageBackend == "external_postgres") {
      env["ONECONTEXT_STORAGE_BACKEND"] = explicitDatabaseURL == nil ? "managed_postgres" : "external_postgres"
    }
    _ = identity
    return env
  }

  private static func normalizedExplicitDatabaseURL(from environment: [String: String]) -> String? {
    let keys = [
      "ONECONTEXT_MEMORY_DB_URL",
      "ONECONTEXT_MEMORY_DATABASE_URL"
    ] + (environment["ONECONTEXT_ALLOW_GENERIC_DATABASE_URL"] == "1" ? ["DATABASE_URL"] : [])
    for key in keys {
      if let value = environment[key]?.trimmingCharacters(in: .whitespacesAndNewlines),
        !value.isEmpty
      {
        return value
      }
    }
    return nil
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
        environment: processEnvironment(),
        timeoutSeconds: 30
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

  private func terminateExistingHelperIfNeeded(expectedExecutable: URL, log: @escaping @Sendable (String) -> Void) {
    guard let record = readPIDRecord(), record.pid > 0 else {
      return
    }
    guard processIsAlive(record.pid) else {
      try? fileManager.removeItem(at: pidPath)
      return
    }
    guard pidRecord(record, matchesExpectedExecutable: expectedExecutable) else {
      log("memory daemon ignored untrusted pid file pid=\(record.pid)")
      try? fileManager.removeItem(at: pidPath)
      return
    }

    log("memory daemon terminating stale helper pid=\(record.pid)")
    kill(record.pid, SIGTERM)
    for _ in 0..<20 {
      usleep(50_000)
      if !processIsAlive(record.pid) {
        return
      }
    }
    if pidRecord(record, matchesExpectedExecutable: expectedExecutable) {
      kill(record.pid, SIGKILL)
    }
  }

  private func processIsAlive(_ pid: Int32) -> Bool {
    if kill(pid, 0) == 0 {
      return true
    }
    return errno == EPERM
  }

  private func readPIDRecord() -> PIDRecord? {
    guard let data = try? Data(contentsOf: pidPath) else {
      return nil
    }
    if let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let pid = int32Value(object["pid"])
    {
      return PIDRecord(
        pid: pid,
        executable: object["executable"] as? String,
        startedAtMilliseconds: object["started_at_ms"] as? Int,
        kind: object["kind"] as? String,
        trusted: true
      )
    }
    guard let text = String(data: data, encoding: .utf8),
      let pid = Int32(text.trimmingCharacters(in: .whitespacesAndNewlines))
    else {
      return nil
    }
    return PIDRecord(
      pid: pid,
      executable: nil,
      startedAtMilliseconds: nil,
      kind: nil,
      trusted: false
    )
  }

  private func pidRecord(_ record: PIDRecord, matchesExpectedExecutable expectedExecutable: URL) -> Bool {
    guard record.trusted,
      record.kind == "onecontext-memoryd",
      let recordedExecutable = record.executable
    else {
      return false
    }
    let expectedPath = expectedExecutable.standardizedFileURL.path
    guard URL(fileURLWithPath: recordedExecutable).standardizedFileURL.path == expectedPath,
      liveExecutablePath(for: record.pid).map({ URL(fileURLWithPath: $0).standardizedFileURL.path }) == expectedPath
    else {
      return false
    }
    _ = record.startedAtMilliseconds
    return true
  }

  private func liveExecutablePath(for pid: Int32) -> String? {
    var buffer = [CChar](repeating: 0, count: 4_096)
    let count = proc_pidpath(pid, &buffer, UInt32(buffer.count))
    guard count > 0 else {
      return nil
    }
    let bytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    return String(decoding: bytes, as: UTF8.self)
  }

  private func int32Value(_ value: Any?) -> Int32? {
    if let value = value as? Int, value > 0, value <= Int(Int32.max) {
      return Int32(value)
    }
    if let value = value as? NSNumber, value.int64Value > 0, value.int64Value <= Int64(Int32.max) {
      return value.int32Value
    }
    return nil
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
