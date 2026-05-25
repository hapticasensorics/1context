import Darwin
import Foundation

public struct MemoryViewportQuery: Sendable {
  public let limit: Int
  public let source: String?
  public let startTime: String?
  public let endTime: String?

  public init(limit: Int = 200, source: String? = nil, startTime: String? = nil, endTime: String? = nil) {
    self.limit = limit
    self.source = source
    self.startTime = startTime
    self.endTime = endTime
  }

  public var cappedLimit: Int {
    min(max(limit, 1), 5_000)
  }

  public var filterSource: String? {
    let normalizedSource = source?.trimmingCharacters(in: .whitespacesAndNewlines)
    return normalizedSource.flatMap { $0.isEmpty || $0 == "all" ? nil : $0 }
  }
}

public struct MemoryObjectHydrationQuery: Sendable {
  public let objectIDs: [String]

  public init(objectID: String) {
    self.objectIDs = [objectID]
  }

  public init(objectIDs: [String]) {
    self.objectIDs = objectIDs
  }

  public var firstObjectID: String {
    objectIDs.first ?? ""
  }
}

public struct MemoryDensityQuery: Sendable {
  public let startTime: String?
  public let endTime: String?
  public let bucket: String
  public let sources: [String]

  public init(startTime: String? = nil, endTime: String? = nil, bucket: String = "1m", sources: [String] = []) {
    self.startTime = startTime
    self.endTime = endTime
    self.bucket = bucket.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "1m" : bucket
    self.sources = sources.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty && $0 != "all" }
  }
}

public struct MemoryEdgesQuery: Sendable {
  public let objectID: String
  public let direction: String
  public let edgeKind: String?
  public let limit: Int
  public let includeObjectSummaries: Bool

  public init(
    objectID: String,
    direction: String = "both",
    edgeKind: String? = nil,
    limit: Int = 200,
    includeObjectSummaries: Bool = true
  ) {
    self.objectID = objectID
    self.direction = direction.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "both" : direction
    self.edgeKind = edgeKind?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
    self.limit = limit
    self.includeObjectSummaries = includeObjectSummaries
  }

  public var cappedLimit: Int {
    min(max(limit, 1), 1_000)
  }
}

public struct MemorySearchTextQuery: Sendable {
  public let query: String
  public let limit: Int
  public let source: String?

  public init(query: String, limit: Int = 50, source: String? = nil) {
    self.query = query
    self.limit = limit
    self.source = source
  }

  public var cappedLimit: Int {
    min(max(limit, 1), 500)
  }

  public var filterSource: String? {
    let normalizedSource = source?.trimmingCharacters(in: .whitespacesAndNewlines)
    return normalizedSource.flatMap { $0.isEmpty || $0 == "all" ? nil : $0 }
  }
}

public enum MemoryProtocolRequest: Sendable {
  case viewport(MemoryViewportQuery)
  case objectHydration(MemoryObjectHydrationQuery)
  case density(MemoryDensityQuery)
  case edges(MemoryEdgesQuery)
  case searchText(MemorySearchTextQuery)
}

public struct MemoryProtocolResponse {
  public let payload: [String: Any]

  public init(payload: [String: Any]) {
    self.payload = payload
  }
}

public protocol MemoryProtocolClient: Sendable {
  func send(_ request: MemoryProtocolRequest) -> MemoryProtocolResponse
}

public extension MemoryProtocolClient {
  func queryViewport(_ query: MemoryViewportQuery = MemoryViewportQuery()) -> MemoryProtocolResponse {
    send(.viewport(query))
  }

  func hydrateObject(_ query: MemoryObjectHydrationQuery) -> MemoryProtocolResponse {
    send(.objectHydration(query))
  }

  func queryDensity(_ query: MemoryDensityQuery = MemoryDensityQuery()) -> MemoryProtocolResponse {
    send(.density(query))
  }

  func queryEdges(_ query: MemoryEdgesQuery) -> MemoryProtocolResponse {
    send(.edges(query))
  }

  func searchText(_ query: MemorySearchTextQuery) -> MemoryProtocolResponse {
    send(.searchText(query))
  }
}

public struct MemoryProtocolProcessConfiguration: Sendable {
  public let executable: URL
  public let environment: [String: String]
  public let timeoutSeconds: TimeInterval

  public init(
    executable: URL,
    environment: [String: String],
    timeoutSeconds: TimeInterval = 5
  ) {
    self.executable = executable
    self.environment = environment
    self.timeoutSeconds = timeoutSeconds
  }
}

public enum MemoryProtocolClientFactory {
  public static func make(configuration: MemoryProtocolProcessConfiguration?) -> MemoryProtocolClient {
    guard let configuration else {
      return UnavailableMemoryProtocolClient(message: "onecontext-memoryd executable not found")
    }
    return MemoryProtocolProcessClient(configuration: configuration)
  }
}

public final class MemoryProtocolProcessClient: MemoryProtocolClient, @unchecked Sendable {
  private static let localUserID = "00000000-0000-0000-0000-000000000001"
  private let configuration: MemoryProtocolProcessConfiguration

  public init(configuration: MemoryProtocolProcessConfiguration) {
    self.configuration = configuration
  }

  public func send(_ request: MemoryProtocolRequest) -> MemoryProtocolResponse {
    switch request {
    case .viewport(let query):
      return run(
        arguments: protocolArguments("memory.queryViewport"),
        fallback: unavailableViewportPayload(query: query),
        standardInput: protocolRequest(method: "memory.queryViewport", params: viewportParams(query))
      )
    case .objectHydration(let query):
      return run(
        arguments: protocolArguments("memory.hydrateObjects"),
        fallback: unavailableObjectPayload(query: query),
        standardInput: protocolRequest(method: "memory.hydrateObjects", params: objectParams(query))
      )
    case .density(let query):
      return run(
        arguments: protocolArguments("memory.queryDensity"),
        fallback: unavailableDensityPayload(query: query),
        standardInput: protocolRequest(method: "memory.queryDensity", params: densityParams(query))
      )
    case .edges(let query):
      return run(
        arguments: protocolArguments("memory.queryEdges"),
        fallback: unavailableEdgesPayload(query: query),
        standardInput: protocolRequest(method: "memory.queryEdges", params: edgesParams(query))
      )
    case .searchText(let query):
      return run(
        arguments: protocolArguments("memory.searchText"),
        fallback: unavailableSearchTextPayload(query: query),
        standardInput: protocolRequest(method: "memory.searchText", params: searchTextParams(query))
      )
    }
  }

  private func protocolArguments(_ method: String) -> [String] {
    ["protocol", method, "--request-json", "-"]
  }

  private func protocolRequest(method: String, params: [String: Any]) -> Data {
    let request: [String: Any] = [
      "schema_version": 1,
      "request_id": "swift-\(UUID().uuidString)",
      "method": method,
      "params": params
    ]
    return (try? JSONSerialization.data(withJSONObject: request, options: [.sortedKeys])) ?? Data("{}".utf8)
  }

  private func viewportParams(_ query: MemoryViewportQuery) -> [String: Any] {
    var filters: [String: Any] = [:]
    if let source = query.filterSource {
      filters["source_types"] = [source]
    }
    let time = viewportTimeRange(startTime: query.startTime, endTime: query.endTime)
    return [
      "user_id": Self.localUserID,
      "time": time,
      "filters": filters,
      "pagination": ["limit": query.cappedLimit],
      "include": [
        "payload": false,
        "blob_descriptor": true,
        "source_record": true,
        "edges_count": true
      ],
      "explain": false
    ]
  }

  private func objectParams(_ query: MemoryObjectHydrationQuery) -> [String: Any] {
    [
      "user_id": Self.localUserID,
      "object_ids": query.objectIDs.map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }.filter { !$0.isEmpty },
      "include": [
        "payload": true,
        "blob_descriptor": true,
        "source_record": true,
        "edges": true
      ]
    ]
  }

  private func densityParams(_ query: MemoryDensityQuery) -> [String: Any] {
    var time: [String: Any] = [:]
    if let startTime = query.startTime?.trimmingCharacters(in: .whitespacesAndNewlines), !startTime.isEmpty {
      time["start"] = startTime
    }
    if let endTime = query.endTime?.trimmingCharacters(in: .whitespacesAndNewlines), !endTime.isEmpty {
      time["end"] = endTime
    }
    var filters: [String: Any] = [:]
    if !query.sources.isEmpty {
      filters["source_types"] = query.sources
    }
    return [
      "user_id": Self.localUserID,
      "time": time,
      "bucket": query.bucket,
      "filters": filters,
      "explain": false
    ]
  }

  private func edgesParams(_ query: MemoryEdgesQuery) -> [String: Any] {
    var params: [String: Any] = [
      "user_id": Self.localUserID,
      "object_ids": [query.objectID].filter { !$0.isEmpty },
      "direction": query.direction,
      "limit": query.cappedLimit,
      "hydrate": query.includeObjectSummaries
    ]
    if let edgeKind = query.edgeKind {
      params["edge_kinds"] = [edgeKind]
    }
    return params
  }

  private func searchTextParams(_ query: MemorySearchTextQuery) -> [String: Any] {
    var filters: [String: Any] = [:]
    if let source = query.filterSource {
      filters["source_types"] = [source]
    }
    return [
      "user_id": Self.localUserID,
      "query": query.query,
      "filters": filters,
      "limit": query.cappedLimit
    ]
  }

  private func viewportTimeRange(startTime: String?, endTime: String?) -> [String: Any] {
    let now = Date()
    let end = normalizedTimestamp(endTime) ?? Self.iso8601(now)
    let start = normalizedTimestamp(startTime) ?? Self.iso8601(now.addingTimeInterval(-7 * 24 * 60 * 60))
    return ["start": start, "end": end]
  }

  private func normalizedTimestamp(_ value: String?) -> String? {
    let trimmed = value?.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.flatMap { $0.isEmpty ? nil : $0 }
  }

  private static func iso8601(_ date: Date) -> String {
    ISO8601DateFormatter().string(from: date)
  }

  private func run(arguments: [String], fallback: [String: Any], standardInput: Data? = nil) -> MemoryProtocolResponse {
    let process = Process()
    let stdout = Pipe()
    let stderr = Pipe()
    let stdin = standardInput.map { _ in Pipe() }
    process.executableURL = configuration.executable
    process.arguments = arguments
    process.environment = configuration.environment
    process.standardOutput = stdout
    process.standardError = stderr
    if let stdin {
      process.standardInput = stdin
    }
    do {
      try process.run()
      if let standardInput, let stdin {
        stdin.fileHandleForWriting.write(standardInput)
        try? stdin.fileHandleForWriting.close()
      }
    } catch {
      return MemoryProtocolResponse(payload: fallbackPayload(fallback, error: error.localizedDescription))
    }

    let readGroup = DispatchGroup()
    let outputBox = LockedDataBox()
    let errorBox = LockedDataBox()
    readGroup.enter()
    DispatchQueue.global(qos: .utility).async {
      let data = stdout.fileHandleForReading.readDataToEndOfFile()
      outputBox.set(data)
      readGroup.leave()
    }
    readGroup.enter()
    DispatchQueue.global(qos: .utility).async {
      let data = stderr.fileHandleForReading.readDataToEndOfFile()
      errorBox.set(data)
      readGroup.leave()
    }

    let deadline = Date().addingTimeInterval(configuration.timeoutSeconds)
    while process.isRunning && Date() < deadline {
      usleep(10_000)
    }
    if process.isRunning {
      process.terminate()
      usleep(100_000)
      if process.isRunning {
        kill(process.processIdentifier, SIGKILL)
      }
      process.waitUntilExit()
      _ = readGroup.wait(timeout: .now() + 1)
      return MemoryProtocolResponse(payload: fallbackPayload(fallback, error: "memory protocol timed out"))
    }

    process.waitUntilExit()
    _ = readGroup.wait(timeout: .now() + 1)
    let output = outputBox.get()
    let errorOutput = errorBox.get()
    guard process.terminationStatus == 0 else {
      let message = String(decoding: errorOutput.isEmpty ? output : errorOutput, as: UTF8.self)
        .trimmingCharacters(in: .whitespacesAndNewlines)
      return MemoryProtocolResponse(payload: fallbackPayload(fallback, error: message.isEmpty ? "memory protocol exited \(process.terminationStatus)" : message))
    }
    guard
      let object = try? JSONSerialization.jsonObject(with: output) as? [String: Any]
    else {
      return MemoryProtocolResponse(payload: fallbackPayload(fallback, error: "memory protocol returned non-object JSON"))
    }
    return MemoryProtocolResponse(payload: object)
  }

  private func fallbackPayload(_ payload: [String: Any], error: String) -> [String: Any] {
    var result = payload
    result["message"] = error
    return result
  }
}

public final class UnavailableMemoryProtocolClient: MemoryProtocolClient, @unchecked Sendable {
  private let message: String

  public init(message: String = "Memory Protocol bridge is not wired yet") {
    self.message = message
  }

  public func send(_ request: MemoryProtocolRequest) -> MemoryProtocolResponse {
    switch request {
    case .viewport(let query):
      return MemoryProtocolResponse(payload: unavailableViewportPayload(query: query, message: message))
    case .objectHydration(let query):
      return MemoryProtocolResponse(payload: unavailableObjectPayload(query: query, message: message))
    case .density(let query):
      return MemoryProtocolResponse(payload: unavailableDensityPayload(query: query, message: message))
    case .edges(let query):
      return MemoryProtocolResponse(payload: unavailableEdgesPayload(query: query, message: message))
    case .searchText(let query):
      return MemoryProtocolResponse(payload: unavailableSearchTextPayload(query: query, message: message))
    }
  }
}

private func unavailableViewportPayload(
  query: MemoryViewportQuery,
  message: String = "Memory Protocol unavailable"
) -> [String: Any] {
  [
    "schema_version": 1,
    "surface": "memory_viewport",
    "protocol": "memory.queryViewport.v1",
    "status": "unavailable",
    "provider": "memory_protocol",
    "protocol_status": "unavailable",
    "error": "memory_protocol_unavailable",
    "message": message,
    "limit": query.cappedLimit,
    "source": query.filterSource ?? "all",
    "object_count": 0,
    "shown_object_count": 0,
    "total_object_count": 0,
    "sources": [],
    "objects": []
  ]
}

private func unavailableObjectPayload(
  query: MemoryObjectHydrationQuery,
  message: String = "Memory Protocol unavailable"
) -> [String: Any] {
  [
    "schema_version": 1,
    "surface": "memory_object_hydration",
    "protocol": "memory.hydrateObjects.v1",
    "status": "unavailable",
    "provider": "memory_protocol",
    "protocol_status": "unavailable",
    "error": "memory_protocol_unavailable",
    "message": message,
    "object_id": query.firstObjectID,
    "object_ids": query.objectIDs,
    "object": NSNull()
  ]
}

private func unavailableDensityPayload(
  query: MemoryDensityQuery,
  message: String = "Memory Protocol unavailable"
) -> [String: Any] {
  [
    "schema_version": 1,
    "surface": "memory_density",
    "protocol": "memory.queryDensity.v1",
    "status": "unavailable",
    "provider": "memory_protocol",
    "protocol_status": "unavailable",
    "error": "memory_protocol_unavailable",
    "message": message,
    "bucket": query.bucket,
    "sources": query.sources,
    "buckets": []
  ]
}

private func unavailableEdgesPayload(
  query: MemoryEdgesQuery,
  message: String = "Memory Protocol unavailable"
) -> [String: Any] {
  [
    "schema_version": 1,
    "surface": "memory_edges",
    "protocol": "memory.queryEdges.v1",
    "status": "unavailable",
    "provider": "memory_protocol",
    "protocol_status": "unavailable",
    "error": "memory_protocol_unavailable",
    "message": message,
    "object_id": query.objectID,
    "direction": query.direction,
    "edge_count": 0,
    "edges": []
  ]
}

private func unavailableSearchTextPayload(
  query: MemorySearchTextQuery,
  message: String = "Memory Protocol unavailable"
) -> [String: Any] {
  [
    "schema_version": 1,
    "surface": "memory_search",
    "protocol": "memory.searchText.v1",
    "status": "unavailable",
    "provider": "memory_protocol",
    "protocol_status": "unavailable",
    "error": "memory_protocol_unavailable",
    "message": message,
    "query": query.query,
    "limit": query.cappedLimit,
    "source": query.filterSource ?? "all",
    "object_count": 0,
    "objects": []
  ]
}

private extension String {
  var nilIfEmpty: String? {
    isEmpty ? nil : self
  }
}

private final class LockedDataBox: @unchecked Sendable {
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
