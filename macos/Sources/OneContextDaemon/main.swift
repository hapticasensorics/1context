import Foundation
import Darwin
import OneContextRuntimeSupport

nonisolated(unsafe) private var signalSocketPath: UnsafeMutablePointer<CChar>?
nonisolated(unsafe) private var signalPIDPath: UnsafeMutablePointer<CChar>?

final class Logger {
  private let path: String

  init(path: String) {
    self.path = path
  }

  func write(_ message: String) {
    let timestamp = ISO8601DateFormatter().string(from: Date())
    let line = "[\(timestamp)] \(message)\n"
    guard let data = line.data(using: .utf8) else { return }

    if FileManager.default.fileExists(atPath: path),
      let handle = try? FileHandle(forWritingTo: URL(fileURLWithPath: path))
    {
      defer { try? handle.close() }
      _ = try? handle.seekToEnd()
      try? handle.write(contentsOf: data)
    } else {
      try? data.write(to: URL(fileURLWithPath: path), options: .atomic)
    }
  }
}

final class OneContextDaemon {
  private let paths = RuntimePaths.current()
  private let startedAt = Date()
  private var listenFD: Int32 = -1
  private lazy var logger = Logger(path: paths.logPath)

  func run() throws {
    try prepareDirectories()
    try startSocket()
    try writePIDFile()
    installSignalHandlers()
    logger.write("1Context runtime started pid=\(getpid()) socket=\(paths.socketPath)")
    acceptLoop()
    cleanup()
  }

  private func prepareDirectories() throws {
    try FileManager.default.createDirectory(
      at: paths.userContentDirectory,
      withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
      at: paths.appSupportDirectory,
      withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
      at: paths.runDirectory,
      withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
      at: paths.logDirectory,
      withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
      at: paths.cacheDirectory,
      withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
      at: paths.renderCacheDirectory,
      withIntermediateDirectories: true
    )
    try FileManager.default.createDirectory(
      at: paths.downloadCacheDirectory,
      withIntermediateDirectories: true
    )
    chmod(paths.runDirectory.path, 0o700)
  }

  private func writePIDFile() throws {
    try "\(getpid())\n".write(
      toFile: paths.pidPath,
      atomically: true,
      encoding: .utf8
    )
  }

  private func startSocket() throws {
    if FileManager.default.fileExists(atPath: paths.socketPath) {
      let attributes = try? FileManager.default.attributesOfItem(atPath: paths.socketPath)
      if attributes?[.type] as? FileAttributeType == .typeSocket {
        unlink(paths.socketPath)
      } else {
        throw UnixSocketError.socketPathExists(paths.socketPath)
      }
    }

    listenFD = socket(AF_UNIX, SOCK_STREAM, 0)
    guard listenFD >= 0 else { throw UnixSocketError.socketFailed }

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
    signal(SIGTERM) { _ in
      if let socketPath = signalSocketPath {
        unlink(socketPath)
      }
      if let pidPath = signalPIDPath {
        unlink(pidPath)
      }
      _exit(0)
    }
    signal(SIGINT) { _ in
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
      handle(clientFD: clientFD)
      close(clientFD)
    }
  }

  private func handle(clientFD: Int32) {
    guard let request = readLine(from: clientFD) else { return }
    let response = responseData(for: request)
    response.withUnsafeBytes { buffer in
      _ = write(clientFD, buffer.baseAddress, response.count)
    }
  }

  private func readLine(from fd: Int32) -> Data? {
    var data = Data()
    var byte: UInt8 = 0

    while true {
      var pollFD = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
      guard poll(&pollFD, 1, 2_000) > 0 else { return nil }
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
    default:
      return encode(error: "Unknown method: \(method)", id: id)
    }
  }

  private func healthPayload() -> [String: Any] {
    [
      "status": "ok",
      "version": oneContextVersion,
      "uptimeSeconds": max(0, Int(Date().timeIntervalSince(startedAt))),
      "pid": Int(getpid())
    ]
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

  private func cleanup() {
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
    logger.write("1Context runtime stopped")
  }
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
