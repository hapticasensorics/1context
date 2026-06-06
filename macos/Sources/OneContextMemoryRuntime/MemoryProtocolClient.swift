import Darwin
import Foundation
import OneContextPlatform

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

public struct MemoryStorageHealthQuery: Sendable {
  public let reason: String?

  public init(reason: String? = nil) {
    self.reason = reason?.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty
  }
}

public struct MemoryEnsureStorageReadyQuery: Sendable {
  public let reason: String
  public let repair: Bool

  public init(reason: String = "swift", repair: Bool = false) {
    self.reason = reason.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty ?? "swift"
    self.repair = repair
  }
}

public struct MemoryEnsureRecentBackfillQuery: Sendable {
  public let reason: String
  public let windowHours: Int
  public let blockUntilReady: Bool
  public let minDensityBuckets: Int?

  public init(
    reason: String = "swift",
    windowHours: Int = 72,
    blockUntilReady: Bool = false,
    minDensityBuckets: Int? = nil
  ) {
    self.reason = reason.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty ?? "swift"
    self.windowHours = max(windowHours, 1)
    self.blockUntilReady = blockUntilReady
    self.minDensityBuckets = minDensityBuckets.map { max($0, 1) }
  }
}

public struct MemoryRepairStorageQuery: Sendable {
  public let reason: String

  public init(reason: String = "swift") {
    self.reason = reason.trimmingCharacters(in: .whitespacesAndNewlines).nilIfEmpty ?? "swift"
  }
}

public struct MemoryStorageDiagnosticsQuery: Sendable {
  public let includeLogs: Bool
  public let redact: Bool

  public init(includeLogs: Bool = false, redact: Bool = true) {
    self.includeLogs = includeLogs
    self.redact = redact
  }
}

public enum MemoryProtocolRequest: Sendable {
  case viewport(MemoryViewportQuery)
  case objectHydration(MemoryObjectHydrationQuery)
  case density(MemoryDensityQuery)
  case edges(MemoryEdgesQuery)
  case searchText(MemorySearchTextQuery)
  case storageHealth(MemoryStorageHealthQuery)
  case ensureStorageReady(MemoryEnsureStorageReadyQuery)
  case ensureRecentBackfill(MemoryEnsureRecentBackfillQuery)
  case repairStorage(MemoryRepairStorageQuery)
  case storageDiagnostics(MemoryStorageDiagnosticsQuery)
}

public struct MemoryProtocolResponse {
  public let payload: [String: Any]

  public init(payload: [String: Any]) {
    self.payload = payload
  }
}

public protocol MemoryProtocolClient: Sendable {
  func send(_ request: MemoryProtocolRequest) throws -> MemoryProtocolResponse
}

public extension MemoryProtocolClient {
  func queryViewport(_ query: MemoryViewportQuery = MemoryViewportQuery()) throws -> MemoryProtocolResponse {
    try send(.viewport(query))
  }

  func hydrateObject(_ query: MemoryObjectHydrationQuery) throws -> MemoryProtocolResponse {
    try send(.objectHydration(query))
  }

  func queryDensity(_ query: MemoryDensityQuery = MemoryDensityQuery()) throws -> MemoryProtocolResponse {
    try send(.density(query))
  }

  func queryEdges(_ query: MemoryEdgesQuery) throws -> MemoryProtocolResponse {
    try send(.edges(query))
  }

  func searchText(_ query: MemorySearchTextQuery) throws -> MemoryProtocolResponse {
    try send(.searchText(query))
  }

  func storageHealth(_ query: MemoryStorageHealthQuery = MemoryStorageHealthQuery()) throws -> MemoryProtocolResponse {
    try send(.storageHealth(query))
  }

  func ensureStorageReady(_ query: MemoryEnsureStorageReadyQuery = MemoryEnsureStorageReadyQuery()) throws -> MemoryProtocolResponse {
    try send(.ensureStorageReady(query))
  }

  func ensureRecentBackfill(_ query: MemoryEnsureRecentBackfillQuery = MemoryEnsureRecentBackfillQuery()) throws -> MemoryProtocolResponse {
    try send(.ensureRecentBackfill(query))
  }

  func repairStorage(_ query: MemoryRepairStorageQuery = MemoryRepairStorageQuery()) throws -> MemoryProtocolResponse {
    try send(.repairStorage(query))
  }

  func storageDiagnostics(_ query: MemoryStorageDiagnosticsQuery = MemoryStorageDiagnosticsQuery()) throws -> MemoryProtocolResponse {
    try send(.storageDiagnostics(query))
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
  public static func make(configuration: MemoryProtocolProcessConfiguration?) throws -> MemoryProtocolClient {
    guard let configuration else {
      throw MemoryProtocolClientError("onecontext-memoryd executable not found")
    }
    return MemoryProtocolProcessClient(configuration: configuration)
  }
}

public struct MemoryProtocolClientError: Error, LocalizedError, Sendable {
  public let message: String

  public init(_ message: String) {
    self.message = message
  }

  public var errorDescription: String? {
    message
  }
}

public final class MemoryProtocolProcessClient: MemoryProtocolClient, @unchecked Sendable {
  private static let localUserID = "00000000-0000-0000-0000-000000000001"
  private let configuration: MemoryProtocolProcessConfiguration

  public init(configuration: MemoryProtocolProcessConfiguration) {
    self.configuration = configuration
  }

  public func send(_ request: MemoryProtocolRequest) throws -> MemoryProtocolResponse {
    switch request {
    case .viewport(let query):
      return try run(
        arguments: protocolArguments("memory.queryViewport"),
        standardInput: protocolRequest(method: "memory.queryViewport", params: viewportParams(query))
      )
    case .objectHydration(let query):
      return try run(
        arguments: protocolArguments("memory.hydrateObjects"),
        standardInput: protocolRequest(method: "memory.hydrateObjects", params: objectParams(query))
      )
    case .density(let query):
      return try run(
        arguments: protocolArguments("memory.queryDensity"),
        standardInput: protocolRequest(method: "memory.queryDensity", params: densityParams(query))
      )
    case .edges(let query):
      return try run(
        arguments: protocolArguments("memory.queryEdges"),
        standardInput: protocolRequest(method: "memory.queryEdges", params: edgesParams(query))
      )
    case .searchText(let query):
      return try run(
        arguments: protocolArguments("memory.searchText"),
        standardInput: protocolRequest(method: "memory.searchText", params: searchTextParams(query))
      )
    case .storageHealth(let query):
      return try run(
        arguments: protocolArguments("memory.storageHealth"),
        standardInput: protocolRequest(method: "memory.storageHealth", params: storageHealthParams(query))
      )
    case .ensureStorageReady(let query):
      return try run(
        arguments: protocolArguments("memory.ensureStorageReady"),
        standardInput: protocolRequest(method: "memory.ensureStorageReady", params: ensureStorageReadyParams(query))
      )
    case .ensureRecentBackfill(let query):
      return try run(
        arguments: protocolArguments("memory.ensureRecentBackfill"),
        standardInput: protocolRequest(method: "memory.ensureRecentBackfill", params: ensureRecentBackfillParams(query))
      )
    case .repairStorage(let query):
      return try run(
        arguments: protocolArguments("memory.repairStorage"),
        standardInput: protocolRequest(method: "memory.repairStorage", params: repairStorageParams(query))
      )
    case .storageDiagnostics(let query):
      return try run(
        arguments: protocolArguments("memory.storageDiagnostics"),
        standardInput: protocolRequest(method: "memory.storageDiagnostics", params: storageDiagnosticsParams(query))
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

  private func storageHealthParams(_ query: MemoryStorageHealthQuery) -> [String: Any] {
    var params: [String: Any] = [:]
    if let reason = query.reason {
      params["reason"] = reason
    }
    return params
  }

  private func ensureStorageReadyParams(_ query: MemoryEnsureStorageReadyQuery) -> [String: Any] {
    [
      "reason": query.reason,
      "repair": query.repair
    ]
  }

  private func ensureRecentBackfillParams(_ query: MemoryEnsureRecentBackfillQuery) -> [String: Any] {
    var params: [String: Any] = [
      "reason": query.reason,
      "window_hours": query.windowHours,
      "block_until_ready": query.blockUntilReady
    ]
    if let minDensityBuckets = query.minDensityBuckets {
      params["min_density_buckets"] = minDensityBuckets
    }
    return params
  }

  private func repairStorageParams(_ query: MemoryRepairStorageQuery) -> [String: Any] {
    ["reason": query.reason]
  }

  private func storageDiagnosticsParams(_ query: MemoryStorageDiagnosticsQuery) -> [String: Any] {
    [
      "include_logs": query.includeLogs,
      "redact": query.redact
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

  private func run(arguments: [String], standardInput: Data? = nil) throws -> MemoryProtocolResponse {
    let result = try ProcessRunner.run(
      executable: configuration.executable,
      arguments: arguments,
      environment: configuration.environment,
      standardInput: standardInput,
      timeoutSeconds: configuration.timeoutSeconds
    )
    if result.timedOut {
      throw MemoryProtocolClientError("memory protocol timed out")
    }
    guard result.status == 0 else {
      let message = String(decoding: result.stderr.isEmpty ? result.stdout : result.stderr, as: UTF8.self)
        .trimmingCharacters(in: .whitespacesAndNewlines)
      throw MemoryProtocolClientError(message.isEmpty ? "memory protocol exited \(result.status)" : message)
    }
    guard
      let object = try? JSONSerialization.jsonObject(with: result.stdout) as? [String: Any]
    else {
      throw MemoryProtocolClientError("memory protocol returned non-object JSON")
    }
    return MemoryProtocolResponse(payload: object)
  }
}

private extension String {
  var nilIfEmpty: String? {
    isEmpty ? nil : self
  }
}
