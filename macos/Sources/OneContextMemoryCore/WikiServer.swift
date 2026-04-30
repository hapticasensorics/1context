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
    if let payload = healthPayload(state: state, requireToken: true) {
      let url = payload["url"] as? String ?? state.url
      return WikiServerSnapshot(running: true, url: url, pid: state.pid, health: "OK")
    }
    guard processIsAlive(state.pid) else {
      return WikiServerSnapshot(running: false, url: state.url, pid: state.pid, health: "stale")
    }
    if healthPayload(state: nil, requireToken: false) != nil {
      return WikiServerSnapshot(running: true, url: state.url, pid: state.pid, health: "OK")
    }
    return WikiServerSnapshot(running: false, url: state.url, pid: state.pid, health: "no response")
  }

  @discardableResult
  public func start() throws -> WikiServerSnapshot {
    if case let current = status(), current.running {
      return current
    }

    _ = try setup.ensureReady(validateContract: false)
    let alreadyRenderable = hasServableForYou()
    if !alreadyRenderable {
      _ = try adapter.run(arguments: ["wiki", "ensure", "--json"])
      _ = try adapter.run(arguments: ["wiki", "render", "for-you", "--no-evidence", "--json"])
    }

    let python = memoryPaths.directory.appendingPathComponent("venv/bin/python3").path
    guard fileManager.isExecutableFile(atPath: python) else {
      throw MemoryCoreSetupError.toolMissing("python3")
    }

    try RuntimePermissions.ensurePrivateDirectory(runtimePaths.appSupportDirectory)
    try RuntimePermissions.ensurePrivateDirectory(runtimePaths.logDirectory)
    let token = UUID().uuidString
    let process = Process()
    process.executableURL = URL(fileURLWithPath: python)
    process.arguments = [
      "-m", "onectx.wiki.serve_main",
      "--root", setup.coreDirectory.path,
      "--host", host,
      "--port", "\(port)"
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
    if snapshot.url != state.url {
      var updated = state
      updated.url = snapshot.url
      try? writeState(updated)
    }
    try? writeAgentWikiURL(snapshot.url)
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

  private func waitForHealthy(state: WikiServerState, timeout: TimeInterval = 30) throws -> WikiServerSnapshot {
    let deadline = Date().addingTimeInterval(timeout)
    repeat {
      if processIsAlive(state.pid), let payload = healthPayload(state: state, requireToken: true) {
        let url = payload["url"] as? String ?? state.url
        return WikiServerSnapshot(running: true, url: url, pid: state.pid, health: "OK")
      }
      Thread.sleep(forTimeInterval: 0.25)
    } while Date() < deadline
    throw MemoryCoreSetupError.timedOut("wiki serve")
  }

  private func hasServableForYou() -> Bool {
    let generated = setup.coreDirectory.appendingPathComponent("wiki/menu/10-for-you/10-for-you/generated", isDirectory: true)
    let manifest = generated.appendingPathComponent("render-manifest.json")
    let latest = generated.appendingPathComponent("latest_for_family.json")
    let index = generated.appendingPathComponent("for-you-index.json")
    guard fileManager.fileExists(atPath: manifest.path),
      fileManager.fileExists(atPath: latest.path),
      fileManager.fileExists(atPath: index.path),
      let latestData = try? Data(contentsOf: latest),
      let latestObject = try? JSONSerialization.jsonObject(with: latestData) as? [String: Any],
      let forYou = latestObject["for-you"] as? [String: Any],
      let slug = forYou["slug"] as? String,
      !slug.isEmpty,
      fileManager.fileExists(atPath: generated.appendingPathComponent("\(slug).html").path),
      manifestInputsMatch(manifest: manifest)
    else {
      return false
    }
    return true
  }

  private func manifestInputsMatch(manifest: URL) -> Bool {
    guard let data = try? Data(contentsOf: manifest),
      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let inputs = object["inputs"] as? [[String: Any]]
    else {
      return false
    }
    for input in inputs {
      guard let relativePath = input["path"] as? String,
        let expectedHash = input["sha256"] as? String,
        !relativePath.isEmpty,
        !expectedHash.isEmpty
      else {
        return false
      }
      let inputURL = setup.coreDirectory.appendingPathComponent(relativePath)
      guard let inputData = try? Data(contentsOf: inputURL),
        sha256Hex(inputData) == expectedHash
      else {
        return false
      }
    }
    return true
  }

  private func sha256Hex(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
  }

  private func healthPayload(state: WikiServerState?, requireToken: Bool) -> [String: Any]? {
    let challenge = UUID().uuidString
    for healthURL in healthCandidateURLs(state: state, requireToken: requireToken) {
      if let payload = healthPayload(url: healthURL, state: state, requireToken: requireToken, challenge: challenge) {
        return payload
      }
    }
    return nil
  }

  private func healthPayload(
    url: URL,
    state: WikiServerState?,
    requireToken: Bool,
    challenge: String
  ) -> [String: Any]? {
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
      var copy = object
      if let actualURL = Self.routeURL(fromHealthURL: url) {
        copy["url"] = actualURL
      }
      payload = copy
    }
    task.resume()
    if semaphore.wait(timeout: .now() + 1.2) == .timedOut {
      task.cancel()
      return nil
    }
    return payload
  }

  private func healthCandidateURLs(state: WikiServerState?, requireToken: Bool) -> [URL] {
    var urls: [URL] = []
    if let state, let url = Self.healthURL(fromRouteURL: state.url) {
      urls.append(url)
    }
    if let configured = URL(string: "http://\(host):\(port)/__health") {
      urls.append(configured)
    }
    if requireToken {
      for candidatePort in port..<(port + 25) {
        if let url = URL(string: "http://\(host):\(candidatePort)/__health") {
          urls.append(url)
        }
      }
    }

    var seen: Set<String> = []
    return urls.filter { url in
      let key = url.absoluteString
      guard !seen.contains(key) else { return false }
      seen.insert(key)
      return true
    }
  }

  private static func healthURL(fromRouteURL value: String) -> URL? {
    guard var components = URLComponents(string: value) else { return nil }
    components.path = "/__health"
    components.query = nil
    components.fragment = nil
    return components.url
  }

  private static func routeURL(fromHealthURL url: URL) -> String? {
    guard var components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
    components.path = route
    components.query = nil
    components.fragment = nil
    return components.url?.absoluteString
  }

  private func writeAgentWikiURL(_ url: String) throws {
    let directory = runtimePaths.appSupportDirectory.appendingPathComponent("agent", isDirectory: true)
    let configFile = directory.appendingPathComponent("config.json")
    try RuntimePermissions.ensurePrivateDirectory(directory)

    var root: [String: Any] = [:]
    if let data = try? Data(contentsOf: configFile),
      let existing = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    {
      root = existing
    }
    root["wiki_url"] = url
    if root["status_line_label"] == nil {
      root["status_line_label"] = "1Context wiki"
    }
    let data = try JSONSerialization.data(withJSONObject: root, options: [.prettyPrinted, .sortedKeys])
    try RuntimePermissions.writePrivateData(data, to: configFile)
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

  private static func tokenProof(token: String, challenge: String) -> String {
    let digest = SHA256.hash(data: Data("\(token):\(challenge)".utf8))
    return digest.map { String(format: "%02x", $0) }.joined()
  }
}
