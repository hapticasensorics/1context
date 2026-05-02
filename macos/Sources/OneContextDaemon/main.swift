import Foundation
import Darwin
import OneContextAgent
import OneContextLocalWeb
import OneContextMemoryCore
import OneContextRuntimeSupport
import OneContextSetup

nonisolated(unsafe) private var signalSocketPath: UnsafeMutablePointer<CChar>?
nonisolated(unsafe) private var signalPIDPath: UnsafeMutablePointer<CChar>?
nonisolated(unsafe) private var signalLogPath: UnsafeMutablePointer<CChar>?

private let daemonLogMaxBytes: UInt64 = 1_048_576
private let cacheMaxBytes: UInt64 = 50 * 1024 * 1024
private let cacheMaxAge: TimeInterval = 7 * 24 * 60 * 60
private let maxActiveClients = 32
private let requestDeadlineSeconds: TimeInterval = 2

final class Logger: @unchecked Sendable {
  private let path: String
  private let queue = DispatchQueue(label: "com.haptica.1contextd.logger")

  init(path: String) {
    self.path = path
  }

  func write(_ message: String) {
    queue.sync {
      rotateIfNeeded()
      let timestamp = ISO8601DateFormatter().string(from: Date())
      let line = "[\(timestamp)] \(message)\n"
      guard let data = line.data(using: .utf8) else { return }

      if FileManager.default.fileExists(atPath: path),
        let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: path))
      {
        defer {
          try? handle.close()
          RuntimePermissions.ensurePrivateFile(path)
        }
        _ = try? handle.seekToEnd()
        try? handle.write(contentsOf: data)
      } else {
        try? RuntimePermissions.writePrivateData(data, to: URL(fileURLWithPath: path))
      }
    }
  }

  private func rotateIfNeeded() {
    let fileManager = FileManager.default
    guard let attributes = try? fileManager.attributesOfItem(atPath: path),
      let size = attributes[.size] as? NSNumber,
      size.uint64Value >= daemonLogMaxBytes
    else {
      return
    }

    let current = URL(fileURLWithPath: path)
    let rotated = URL(fileURLWithPath: path + ".1")
    try? fileManager.removeItem(at: rotated)
    try? fileManager.moveItem(at: current, to: rotated)
    RuntimePermissions.ensurePrivateFile(rotated.path)
  }
}

final class OneContextDaemon: @unchecked Sendable {
  private let paths = RuntimePaths.current()
  private let startedAt = Date()
  private let clientQueue = DispatchQueue(label: "com.haptica.1contextd.clients", attributes: .concurrent)
  private let activeClients = DispatchSemaphore(value: maxActiveClients)
  private var listenFD: Int32 = -1
  private lazy var logger = Logger(path: paths.logPath)
  private lazy var localWeb = CaddyManager(runtimePaths: paths)
  private lazy var wikiPublisher = WikiSitePublisher()
  private lazy var wikiAPI = WikiLocalAPIServer(
    config: WikiLocalAPIConfig(environment: ProcessInfo.processInfo.environment),
    handler: WikiLocalAPIHandler(paths: LocalWebPaths(runtimePaths: paths), renderState: { [weak self] in
      self?.wikiRenderState ?? "idle"
    })
  )
  private let wikiQueue = DispatchQueue(label: "com.haptica.1context.wiki.publish")
  private let wikiStateLock = NSLock()
  private var wikiPreparing = false
  private var wikiRefreshing = false

  func run() throws {
    umask(0o077)
    signal(SIGPIPE, SIG_IGN)
    try prepareDirectories()
    try startSocket()
    try writePIDFile()
    installSignalHandlers()
    logger.write("1Context runtime started pid=\(getpid()) socket=\(paths.socketPath)")
    startWikiAPI()
    publishWikiInBackground(refresh: false)
    acceptLoop()
    cleanup()
  }

  private func prepareDirectories() throws {
    try RuntimePermissions.ensurePrivateDirectory(paths.userContentDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.appSupportDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.runDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.logDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.cacheDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.renderCacheDirectory)
    try RuntimePermissions.ensurePrivateDirectory(paths.downloadCacheDirectory)
    RuntimePermissions.repairRuntimePaths(paths)
    pruneCaches()
  }

  private func writePIDFile() throws {
    try RuntimePermissions.writePrivateString("\(getpid())\n", toFile: paths.pidPath)
  }

  private func startSocket() throws {
    if FileManager.default.fileExists(atPath: paths.socketPath) {
      let attributes = try? FileManager.default.attributesOfItem(atPath: paths.socketPath)
      if attributes?[.type] as? FileAttributeType == .typeSocket {
        if isSocketAcceptingConnections(paths.socketPath) {
          throw UnixSocketError.socketPathExists(paths.socketPath)
        }
        unlink(paths.socketPath)
      } else {
        throw UnixSocketError.socketPathExists(paths.socketPath)
      }
    }

    listenFD = socket(AF_UNIX, SOCK_STREAM, 0)
    guard listenFD >= 0 else { throw UnixSocketError.socketFailed }
    setNoSigPipe(listenFD)

    let bindResult = try withUnixSocketAddress(path: paths.socketPath) { pointer, length in
      Darwin.bind(listenFD, pointer, length)
    }
    guard bindResult == 0 else {
      close(listenFD)
      throw UnixSocketError.connectFailed(paths.socketPath)
    }
    chmod(paths.socketPath, 0o600)

    guard listen(listenFD, 16) == 0 else {
      close(listenFD)
      throw UnixSocketError.socketFailed
    }
  }

  private func installSignalHandlers() {
    signalSocketPath = strdup(paths.socketPath)
    signalPIDPath = strdup(paths.pidPath)
    signalLogPath = strdup(paths.logPath)
    signal(SIGTERM) { _ in
      writeSignalLog("1Context runtime stopping signal=SIGTERM\n")
      if let socketPath = signalSocketPath {
        unlink(socketPath)
      }
      if let pidPath = signalPIDPath {
        unlink(pidPath)
      }
      _exit(0)
    }
    signal(SIGINT) { _ in
      writeSignalLog("1Context runtime stopping signal=SIGINT\n")
      if let socketPath = signalSocketPath {
        unlink(socketPath)
      }
      if let pidPath = signalPIDPath {
        unlink(pidPath)
      }
      _exit(0)
    }
  }

  private func acceptLoop() {
    while listenFD >= 0 {
      let clientFD = accept(listenFD, nil, nil)
      if clientFD < 0 { continue }
      guard activeClients.wait(timeout: .now()) == .success else {
        close(clientFD)
        continue
      }
      clientQueue.async { [self] in
        defer {
          close(clientFD)
          activeClients.signal()
        }
        autoreleasepool {
          handle(clientFD: clientFD)
        }
      }
    }
  }

  private func handle(clientFD: Int32) {
    setNoSigPipe(clientFD)
    guard let request = readLine(from: clientFD) else { return }
    let response = responseData(for: request)
    _ = writeAll(response, to: clientFD)
  }

  private func writeAll(_ data: Data, to fd: Int32) -> Bool {
    data.withUnsafeBytes { rawBuffer in
      guard let baseAddress = rawBuffer.baseAddress else { return false }
      var sent = 0
      while sent < data.count {
        let count = write(fd, baseAddress.advanced(by: sent), data.count - sent)
        if count > 0 {
          sent += count
        } else if count < 0 && errno == EINTR {
          continue
        } else {
          return false
        }
      }
      return true
    }
  }

  private func isSocketAcceptingConnections(_ path: String) -> Bool {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { return false }
    defer { close(fd) }
    setNoSigPipe(fd)
    let result = try? withUnixSocketAddress(path: path) { pointer, length in
      connect(fd, pointer, length)
    }
    return result == 0
  }

  private func setNoSigPipe(_ fd: Int32) {
    var enabled: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &enabled, socklen_t(MemoryLayout<Int32>.size))
  }

  private func readLine(from fd: Int32) -> Data? {
    var data = Data()
    var byte: UInt8 = 0
    let deadline = Date().addingTimeInterval(requestDeadlineSeconds)

    while true {
      let remaining = deadline.timeIntervalSinceNow
      guard remaining > 0 else { return nil }
      let timeoutMs = max(1, min(2_000, Int32(remaining * 1_000)))
      var pollFD = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
      guard poll(&pollFD, 1, timeoutMs) > 0 else { return nil }
      guard read(fd, &byte, 1) == 1 else { break }
      if byte == UInt8(ascii: "\n") { break }
      data.append(byte)
      if data.count > 64 * 1024 { return nil }
    }

    return data.isEmpty ? nil : data
  }

  private func responseData(for requestData: Data) -> Data {
    let id: Any
    let method: String

    do {
      let object = try JSONSerialization.jsonObject(with: requestData)
      guard let request = object as? [String: Any],
        let requestMethod = request["method"] as? String
      else {
        return encode(error: "Invalid request", id: NSNull())
      }

      id = request["id"] ?? NSNull()
      method = requestMethod
    } catch {
      return encode(error: "Invalid JSON", id: NSNull())
    }

    switch method {
    case "health", "status":
      return encode(result: healthPayload(), id: id)
    case "version":
      return encode(result: ["version": oneContextVersion], id: id)
    case "wiki.status":
      let snapshot = wikiStatus()
      recordAgentWikiURL(snapshot)
      return encode(result: wikiPayload(snapshot), id: id)
    case "wiki.start":
      logger.write("wiki.start requested")
      let current = wikiStatus()
      if current.running {
        logger.write("wiki.start already running")
        recordAgentWikiURL(current)
        return encode(result: wikiPayload(current), id: id)
      }
      publishWikiInBackground(refresh: false)
      logger.write("wiki.start accepted")
      return encode(result: wikiPayload(pendingWikiSnapshot(health: "starting")), id: id)
    case "wiki.refresh":
      logger.write("wiki.refresh requested")
      publishWikiInBackground(refresh: true)
      logger.write("wiki.refresh accepted")
      return encode(result: wikiPayload(pendingWikiSnapshot(health: "refreshing")), id: id)
    case "wiki.stop":
      logger.write("wiki.stop requested")
      return encode(result: wikiPayload(wikiStatus()), id: id)
    default:
      return encode(error: "Unknown method: \(method)", id: id)
    }
  }

  private func wikiPayload(_ snapshot: LocalWebSnapshot) -> [String: Any] {
    var payload: [String: Any] = [
      "running": snapshot.running,
      "url": snapshot.url,
      "route": snapshot.route,
      "health": snapshot.health,
      "api": wikiAPIPayload()
    ]
    if let pid = snapshot.pid {
      payload["pid"] = Int(pid)
    }
    if let lastError = snapshot.lastError {
      payload["lastError"] = lastError
    }
    return payload
  }

  private func healthPayload() -> [String: Any] {
    let readiness = OneContextAppReadiness.current(localWeb: localWeb)
    return [
      "status": "ok",
      "version": oneContextVersion,
      "currentTime": ISO8601DateFormatter().string(from: Date()),
      "uptimeSeconds": max(0, Int(Date().timeIntervalSince(startedAt))),
      "pid": Int(getpid()),
      "requiredSetupReady": readiness.requiredSetupReady,
      "requiredSetupSummary": readiness.requiredSetupSummary
    ]
  }

  private func startWikiAPI() {
    do {
      let snapshot = try wikiAPI.start()
      logger.write("wiki API started url=\(snapshot.url)")
    } catch {
      logger.write("wiki API failed: \(error.localizedDescription)")
    }
  }

  private func wikiStatus() -> LocalWebSnapshot {
    if isWikiRefreshing {
      return pendingWikiSnapshot(health: "refreshing")
    }
    if isWikiPreparing {
      return pendingWikiSnapshot(health: "starting")
    }
    return localWeb.status()
  }

  private func pendingWikiSnapshot(health: String) -> LocalWebSnapshot {
    let current = localWeb.status()
    return LocalWebSnapshot(running: false, url: current.url, pid: current.pid, route: current.route, health: health)
  }

  private func publishWikiInBackground(refresh: Bool) {
    wikiStateLock.lock()
    if wikiPreparing || wikiRefreshing {
      wikiStateLock.unlock()
      return
    }
    if refresh {
      wikiRefreshing = true
    } else {
      wikiPreparing = true
    }
    wikiStateLock.unlock()

    wikiQueue.async { [self] in
      defer {
        wikiStateLock.lock()
        wikiPreparing = false
        wikiRefreshing = false
        wikiStateLock.unlock()
      }
      do {
        let webPaths = LocalWebPaths(runtimePaths: paths)
        _ = try wikiPublisher.publish(
          paths: WikiSitePublishPaths(
            current: webPaths.wikiCurrent,
            next: webPaths.wikiNext,
            previous: webPaths.wikiPrevious
          ),
          refresh: refresh
        )
        let snapshot = localWeb.status()
        recordAgentWikiURL(snapshot)
        logger.write("wiki published refresh=\(refresh) url=\(snapshot.url)")
      } catch {
        logger.write("wiki publish failed refresh=\(refresh): \(error.localizedDescription)")
      }
    }
  }

  private var isWikiPreparing: Bool {
    wikiStateLock.lock()
    defer { wikiStateLock.unlock() }
    return wikiPreparing
  }

  private var isWikiRefreshing: Bool {
    wikiStateLock.lock()
    defer { wikiStateLock.unlock() }
    return wikiRefreshing
  }

  private var wikiRenderState: String {
    if isWikiRefreshing { return "refreshing" }
    if isWikiPreparing { return "starting" }
    return "idle"
  }

  private func wikiAPIPayload() -> [String: Any] {
    let snapshot = wikiAPI.snapshot
    var payload: [String: Any] = [
      "running": snapshot.running,
      "url": snapshot.url,
      "health": snapshot.health,
      "port": snapshot.port
    ]
    if let lastError = snapshot.lastError {
      payload["lastError"] = lastError
    }
    return payload
  }

  private func writeAgentWikiURL(_ url: String) throws {
    try AgentConfigStore.writeWikiURL(url, paths: AgentPaths.current())
  }

  private func recordAgentWikiURL(_ snapshot: LocalWebSnapshot) {
    do {
      try writeAgentWikiURL(snapshot.url)
    } catch {
      logger.write("agent wiki URL update failed: \(error.localizedDescription)")
    }
  }

  private func encode(result: [String: Any], id: Any) -> Data {
    let payload: [String: Any] = [
      "jsonrpc": "2.0",
      "id": id,
      "result": result
    ]
    return encode(payload)
  }

  private func encode(error message: String, id: Any) -> Data {
    let payload: [String: Any] = [
      "jsonrpc": "2.0",
      "id": id,
      "error": [
        "code": -32603,
        "message": message
      ]
    ]
    return encode(payload)
  }

  private func encode(_ payload: [String: Any]) -> Data {
    let data = (try? JSONSerialization.data(withJSONObject: payload)) ?? Data("{}".utf8)
    return data + Data([UInt8(ascii: "\n")])
  }

  private func pruneCaches() {
    pruneCacheDirectory(paths.renderCacheDirectory)
    pruneCacheDirectory(paths.downloadCacheDirectory)
  }

  private func pruneCacheDirectory(_ directory: URL) {
    let fileManager = FileManager.default
    guard let enumerator = fileManager.enumerator(
      at: directory,
      includingPropertiesForKeys: [.isRegularFileKey, .contentModificationDateKey, .fileSizeKey],
      options: [.skipsHiddenFiles]
    ) else {
      return
    }

    let now = Date()
    var files: [(url: URL, size: UInt64, modifiedAt: Date)] = []

    for case let url as URL in enumerator {
      guard let values = try? url.resourceValues(
        forKeys: [.isRegularFileKey, .contentModificationDateKey, .fileSizeKey]
      ), values.isRegularFile == true
      else {
        continue
      }

      let modifiedAt = values.contentModificationDate ?? .distantPast
      if now.timeIntervalSince(modifiedAt) > cacheMaxAge {
        try? fileManager.removeItem(at: url)
        continue
      }

      files.append((url, UInt64(values.fileSize ?? 0), modifiedAt))
    }

    var totalBytes = files.reduce(UInt64(0)) { $0 + $1.size }
    guard totalBytes > cacheMaxBytes else { return }

    for file in files.sorted(by: { $0.modifiedAt < $1.modifiedAt }) {
      try? fileManager.removeItem(at: file.url)
      totalBytes = totalBytes > file.size ? totalBytes - file.size : 0
      if totalBytes <= cacheMaxBytes { break }
    }
  }

  private func cleanup() {
    wikiAPI.stop()
    if listenFD >= 0 {
      close(listenFD)
    }
    unlink(paths.socketPath)
    unlink(paths.pidPath)
    if let socketPath = signalSocketPath {
      free(socketPath)
      signalSocketPath = nil
    }
    if let pidPath = signalPIDPath {
      free(pidPath)
      signalPIDPath = nil
    }
    if let logPath = signalLogPath {
      free(logPath)
      signalLogPath = nil
    }
    logger.write("1Context runtime stopped")
  }
}

private func writeSignalLog(_ message: StaticString) {
  guard let logPath = signalLogPath else { return }
  let fd = open(logPath, O_WRONLY | O_CREAT | O_APPEND, S_IRUSR | S_IWUSR)
  guard fd >= 0 else { return }
  message.withUTF8Buffer { buffer in
    if let baseAddress = buffer.baseAddress {
      _ = write(fd, baseAddress, buffer.count)
    }
  }
  close(fd)
}

do {
  try OneContextDaemon().run()
} catch {
  let paths = RuntimePaths.current()
  try? FileManager.default.createDirectory(at: paths.logDirectory, withIntermediateDirectories: true)
  Logger(path: paths.logPath).write("1Context runtime failed: \(error.localizedDescription)")
  fputs("1Context runtime failed: \(error.localizedDescription)\n", stderr)
  exit(1)
}
