import Foundation
import Darwin
import OneContextCore
import OneContextPlatform

public enum UnixSocketError: Error, LocalizedError {
  case pathTooLong(String)
  case socketFailed
  case connectFailed(String)
  case writeFailed
  case emptyResponse
  case invalidResponse
  case rpcError(String)
  case socketPathExists(String)

  public var errorDescription: String? {
    switch self {
    case .pathTooLong(let path):
      return "Socket path is too long: \(path)"
    case .socketFailed:
      return "Could not create socket"
    case .connectFailed(let path):
      return "Could not connect to \(path)"
    case .writeFailed:
      return "Could not write request"
    case .emptyResponse:
      return "1Context did not return a response"
    case .invalidResponse:
      return "1Context returned an invalid response"
    case .rpcError(let message):
      return message
    case .socketPathExists(let path):
      return "Socket path already exists and is not a socket: \(path)"
    }
  }
}

public enum RuntimeRPCMethod: String, Codable, Sendable {
  case health
  case status
  case version
}

public struct EmptyRPCParams: Codable, Sendable {
  public init() {}
}

public struct JSONRPCRequest<Params: Encodable>: Encodable {
  public let jsonrpc: String
  public let id: Int
  public let method: String
  public let params: Params

  public init(id: Int = 1, method: RuntimeRPCMethod, params: Params) {
    self.jsonrpc = "2.0"
    self.id = id
    self.method = method.rawValue
    self.params = params
  }
}

public struct JSONRPCErrorPayload: Codable, Sendable {
  public let code: Int
  public let message: String
}

public struct JSONRPCResponse<Result: Decodable>: Decodable {
  public let jsonrpc: String?
  public let id: Int?
  public let result: Result?
  public let error: JSONRPCErrorPayload?

  private enum CodingKeys: String, CodingKey {
    case jsonrpc
    case id
    case result
    case error
  }

  public init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    self.jsonrpc = try container.decodeIfPresent(String.self, forKey: .jsonrpc)
    self.id = try container.decodeIfPresent(Int.self, forKey: .id)
    self.result = try container.decodeIfPresent(Result.self, forKey: .result)
    self.error = try container.decodeIfPresent(JSONRPCErrorPayload.self, forKey: .error)
  }
}

public func withUnixSocketAddress<T>(
  path: String,
  _ body: (UnsafePointer<sockaddr>, socklen_t) throws -> T
) throws -> T {
  var address = sockaddr_un()
  address.sun_family = sa_family_t(AF_UNIX)

  let pathBytes = path.utf8CString.map { UInt8(bitPattern: $0) }
  let maxPathBytes = MemoryLayout.size(ofValue: address.sun_path)
  guard pathBytes.count <= maxPathBytes else {
    throw UnixSocketError.pathTooLong(path)
  }

  withUnsafeMutableBytes(of: &address.sun_path) { buffer in
    buffer.initializeMemory(as: UInt8.self, repeating: 0)
    buffer.copyBytes(from: pathBytes)
  }

  return try withUnsafePointer(to: &address) { pointer in
    try pointer.withMemoryRebound(to: sockaddr.self, capacity: 1) { sockaddrPointer in
      try body(sockaddrPointer, socklen_t(MemoryLayout<sockaddr_un>.size))
    }
  }
}

public final class UnixJSONRPCClient {
  private let socketPath: String
  private let timeoutMilliseconds: Int32
  private let maxResponseBytes = 64 * 1024

  public init(socketPath: String = RuntimePaths.current().socketPath, timeoutMilliseconds: Int32 = 2_000) {
    self.socketPath = socketPath
    self.timeoutMilliseconds = timeoutMilliseconds
  }

  public func call<Result: Decodable>(
    method: RuntimeRPCMethod,
    responseType: Result.Type
  ) throws -> Result {
    let request = JSONRPCRequest(id: 1, method: method, params: EmptyRPCParams())
    let responseData = try send(requestData: try JSONEncoder().encode(request) + Data([UInt8(ascii: "\n")]))
    let response = try JSONDecoder().decode(JSONRPCResponse<Result>.self, from: responseData)
    if let error = response.error {
      throw UnixSocketError.rpcError(error.message)
    }
    guard let result = response.result else {
      throw UnixSocketError.invalidResponse
    }
    return result
  }

  public func call(method: String, params: [String: Any] = [:]) throws -> [String: Any] {
    let payload: [String: Any] = [
      "jsonrpc": "2.0",
      "id": 1,
      "method": method,
      "params": params
    ]
    let requestData = try JSONSerialization.data(withJSONObject: payload)
      + Data([UInt8(ascii: "\n")])
    let responseData = try send(requestData: requestData)
    let object = try JSONSerialization.jsonObject(with: responseData)
    guard let dictionary = object as? [String: Any] else {
      throw UnixSocketError.invalidResponse
    }

    if let error = dictionary["error"] as? [String: Any] {
      throw UnixSocketError.rpcError(error["message"] as? String ?? "1Context returned an error")
    }

    return dictionary["result"] as? [String: Any] ?? [:]
  }

  private func send(requestData: Data) throws -> Data {
    let fd = socket(AF_UNIX, SOCK_STREAM, 0)
    guard fd >= 0 else { throw UnixSocketError.socketFailed }
    defer { close(fd) }
    setNoSigPipe(fd)

    let connected = try withUnixSocketAddress(path: socketPath) { pointer, length in
      connect(fd, pointer, length)
    }
    guard connected == 0 else { throw UnixSocketError.connectFailed(socketPath) }

    guard writeAll(requestData, to: fd) else { throw UnixSocketError.writeFailed }

    var response = Data()
    var buffer = [UInt8](repeating: 0, count: 4096)
    while true {
      guard waitForReadable(fd) else { throw UnixSocketError.emptyResponse }
      let count = read(fd, &buffer, buffer.count)
      if count <= 0 { break }
      response.append(buffer, count: count)
      guard response.count <= maxResponseBytes else { throw UnixSocketError.invalidResponse }
      if response.contains(UInt8(ascii: "\n")) { break }
    }

    guard !response.isEmpty else { throw UnixSocketError.emptyResponse }
    let line = response.split(separator: UInt8(ascii: "\n"), maxSplits: 1).first ?? response[...]
    return Data(line)
  }

  private func waitForReadable(_ fd: Int32) -> Bool {
    var pollFD = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
    return poll(&pollFD, 1, timeoutMilliseconds) > 0
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

  private func setNoSigPipe(_ fd: Int32) {
    var enabled: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_NOSIGPIPE, &enabled, socklen_t(MemoryLayout<Int32>.size))
  }

  public func health() throws -> RuntimeHealth {
    try call(method: .health, responseType: RuntimeHealth.self)
  }
}
