import CryptoKit
import Darwin
import Foundation
import OneContextCore
import OneContextPlatform

public struct WikiServerSnapshot: Codable, Equatable, Sendable {
  public var running: Bool
  public var url: String
  public var pid: Int32?
  public var route: String
  public var health: String
  public var lastError: String?

  public init(
    running: Bool,
    url: String = WikiServerManager.defaultURL,
    pid: Int32? = nil,
    route: String = "/for-you",
    health: String,
    lastError: String? = nil
  ) {
    self.running = running
    self.url = url
    self.pid = pid
    self.route = route
    self.health = health
    self.lastError = lastError
  }
}

struct WikiServerState: Codable {
  var schemaVersion: Int = 1
  var pid: Int32
  var renderPID: Int32?
  var url: String
  var token: String
  var startedAt: Date

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case pid
    case renderPID = "render_pid"
    case url
    case token
    case startedAt = "started_at"
  }
}

public final class WikiServerManager: @unchecked Sendable {
  public static let defaultHost = "127.0.0.1"
  public static let defaultPort = 17319
  public static let route = "/for-you"
  public static let defaultURL = "http://127.0.0.1:17319/for-you"

  private let runtimePaths: RuntimePaths
  private let memoryPaths: MemoryCorePaths
  private let setup: MemoryCoreSetup
  private let adapter: MemoryCoreAdapter
  private let environment: [String: String]
  private let fileManager: FileManager
  private let host: String
  private let port: Int
  private let startQueue = DispatchQueue(label: "com.haptica.1context.wiki.start")
  private let stateLock = NSLock()
  private var startInProgress = false

  private var wikiURL: String {
    "http://\(host):\(port)\(Self.route)"
  }

  private var stateFile: URL {
    runtimePaths.appSupportDirectory.appendingPathComponent("wiki-server.json")
  }

  private var logFile: URL {
    runtimePaths.logDirectory.appendingPathComponent("wiki-server.log")
  }

  public init(
    runtimePaths: RuntimePaths = .current(),
    memoryPaths: MemoryCorePaths = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default
  ) {
    self.runtimePaths = runtimePaths
    self.memoryPaths = memoryPaths
    self.environment = environment
    self.fileManager = fileManager
    self.host = environment["ONECONTEXT_WIKI_HOST"] ?? Self.defaultHost
    self.port = Int(environment["ONECONTEXT_WIKI_PORT"] ?? "") ?? Self.defaultPort
    self.setup = MemoryCoreSetup(paths: memoryPaths, environment: environment, fileManager: fileManager)
    self.adapter = MemoryCoreAdapter(
      paths: memoryPaths,
      processRunner: MemoryCoreProcessRunner(environment: environment),
      fileManager: fileManager
    )
  }

  public func status() -> WikiServerSnapshot {
    if isStarting {
      return WikiServerSnapshot(running: false, url: wikiURL, health: "starting")
    }
    guard let state = readState() else {
      if healthPayload(state: nil, requireToken: false) != nil {
        return WikiServerSnapshot(running: true, url: wikiURL, health: "OK")
      }
      return WikiServerSnapshot(running: false, url: wikiURL, health: "not running")
    }
    guard processIsAlive(state.pid) else {
      if healthPayload(state: nil, requireToken: false) != nil {
        return WikiServerSnapshot(running: true, url: state.url, health: "OK")
      }
      return WikiServerSnapshot(running: false, url: state.url, pid: state.pid, health: "stale")
    }
    if healthPayload(state: state, requireToken: true) != nil || healthPayload(state: nil, requireToken: false) != nil {
      return WikiServerSnapshot(running: true, url: state.url, pid: state.pid, health: "OK")
    }
    return WikiServerSnapshot(running: false, url: state.url, pid: state.pid, health: "no response")
  }

  @discardableResult
  public func start() throws -> WikiServerSnapshot {
    if case let current = status(), current.running {
      return current
    }

    _ = try setup.ensureReady()
    _ = try adapter.run(arguments: ["wiki", "ensure", "--json"])

    guard let uv = firstExecutable(["/opt/homebrew/bin/uv", "/usr/local/bin/uv", "/usr/bin/uv"]) else {
      throw MemoryCoreSetupError.toolMissing("uv")
    }

    try RuntimePermissions.ensurePrivateDirectory(runtimePaths.appSupportDirectory)
    try RuntimePermissions.ensurePrivateDirectory(runtimePaths.logDirectory)
    let token = UUID().uuidString
    let process = Process()
    process.executableURL = URL(fileURLWithPath: uv)
    process.arguments = [
      "run",
      "--project", setup.coreDirectory.path,
      "1context",
      "--root", setup.coreDirectory.path,
      "wiki", "serve",
      "--host", host,
      "--port", "\(port)",
      "--no-port-fallback"
    ]
    process.currentDirectoryURL = setup.coreDirectory
    process.environment = setup.setupEnvironment(extra: ["ONECONTEXT_WIKI_SERVER_TOKEN": token])
    process.standardInput = FileHandle.nullDevice
    let logHandle = try appendLogHandle()
    process.standardOutput = logHandle
    process.standardError = logHandle
    try process.run()

    let state = WikiServerState(
      pid: process.processIdentifier,
      renderPID: nil,
      url: wikiURL,
      token: token,
      startedAt: Date()
    )
    try writeState(state)
    let snapshot = try waitForHealthy(state: state)
    startBackgroundRender()
    return snapshot
  }

  public func startInBackground() {
    stateLock.lock()
    if startInProgress {
      stateLock.unlock()
      return
    }
    startInProgress = true
    stateLock.unlock()
    startQueue.async { [self] in
      defer { setStarting(false) }
      do {
        _ = try start()
      } catch {
        try? appendLogLine("1Context wiki failed: \(error.localizedDescription)")
      }
    }
  }

  public func stop() {
    setStarting(false)
    guard let state = readState() else { return }
    if let renderPID = state.renderPID, processIsAlive(renderPID) {
      kill(renderPID, SIGTERM)
    }
    if processIsAlive(state.pid), healthPayload(state: state, requireToken: true) != nil {
      kill(state.pid, SIGTERM)
    }
    try? fileManager.removeItem(at: stateFile)
  }

  public func cleanupForDaemonExit() {
    stop()
  }

  private func startBackgroundRender() {
    guard let uv = firstExecutable(["/opt/homebrew/bin/uv", "/usr/local/bin/uv", "/usr/bin/uv"]) else {
      return
    }
    let process = Process()
    process.executableURL = URL(fileURLWithPath: uv)
    process.arguments = [
      "run",
      "--project", setup.coreDirectory.path,
      "1context-memory-core",
      "wiki", "render", "for-you", "--no-evidence", "--json"
    ]
    process.currentDirectoryURL = setup.coreDirectory
    process.environment = setup.setupEnvironment()
    process.standardInput = FileHandle.nullDevice
    if let logHandle = try? appendLogHandle() {
      process.standardOutput = logHandle
      process.standardError = logHandle
    }
    try? process.run()
    if var state = readState() {
      state.renderPID = process.processIdentifier
      try? writeState(state)
    }
  }

  private func waitForHealthy(state: WikiServerState, timeout: TimeInterval = 30) throws -> WikiServerSnapshot {
    let deadline = Date().addingTimeInterval(timeout)
    repeat {
      if processIsAlive(state.pid), healthPayload(state: state, requireToken: false) != nil {
        return WikiServerSnapshot(running: true, url: state.url, pid: state.pid, health: "OK")
      }
      Thread.sleep(forTimeInterval: 0.25)
    } while Date() < deadline
    throw MemoryCoreSetupError.timedOut("wiki serve")
  }

  private func healthPayload(state: WikiServerState?, requireToken: Bool) -> [String: Any]? {
    let challenge = UUID().uuidString
    guard let url = URL(string: "http://\(host):\(port)/__health") else { return nil }
    var request = URLRequest(url: url)
    request.timeoutInterval = 1
    request.setValue(challenge, forHTTPHeaderField: "X-1Context-Wiki-Challenge")

    let semaphore = DispatchSemaphore(value: 0)
    nonisolated(unsafe) var payload: [String: Any]?
    let task = URLSession.shared.dataTask(with: request) { data, _, _ in
      defer { semaphore.signal() }
      guard let data,
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        object["status"] as? String == "ok"
      else {
        return
      }
      if requireToken {
        guard let state,
          let proof = object["server_token_proof"] as? String,
          proof == Self.tokenProof(token: state.token, challenge: challenge)
        else {
          return
        }
      }
      payload = object
    }
    task.resume()
    if semaphore.wait(timeout: .now() + 1.2) == .timedOut {
      task.cancel()
      return nil
    }
    return payload
  }

  private func readState() -> WikiServerState? {
    guard let data = try? Data(contentsOf: stateFile) else { return nil }
    let decoder = JSONDecoder()
    decoder.dateDecodingStrategy = .iso8601
    return try? decoder.decode(WikiServerState.self, from: data)
  }

  private func writeState(_ state: WikiServerState) throws {
    let encoder = JSONEncoder()
    encoder.dateEncodingStrategy = .iso8601
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try RuntimePermissions.writePrivateData(try encoder.encode(state), to: stateFile)
  }

  private func appendLogHandle() throws -> FileHandle {
    try RuntimePermissions.ensurePrivateDirectory(logFile.deletingLastPathComponent())
    if !fileManager.fileExists(atPath: logFile.path) {
      fileManager.createFile(atPath: logFile.path, contents: nil)
      chmod(logFile.path, RuntimePermissions.privateFileMode)
    }
    let handle = try FileHandle(forWritingTo: logFile)
    try handle.seekToEnd()
    return handle
  }

  private func appendLogLine(_ line: String) throws {
    let handle = try appendLogHandle()
    defer { try? handle.close() }
    try handle.write(contentsOf: Data("[\(ISO8601DateFormatter().string(from: Date()))] \(line)\n".utf8))
  }

  private var isStarting: Bool {
    stateLock.lock()
    defer { stateLock.unlock() }
    return startInProgress
  }

  private func setStarting(_ value: Bool) {
    stateLock.lock()
    startInProgress = value
    stateLock.unlock()
  }

  private func processIsAlive(_ pid: Int32) -> Bool {
    pid > 0 && kill(pid, 0) == 0
  }

  private func firstExecutable(_ candidates: [String]) -> String? {
    candidates.first { fileManager.isExecutableFile(atPath: $0) }
  }

  private static func tokenProof(token: String, challenge: String) -> String {
    let digest = SHA256.hash(data: Data("\(token):\(challenge)".utf8))
    return digest.map { String(format: "%02x", $0) }.joined()
  }
}
