import Darwin
import Foundation
import OneContextPlatform

public struct WikiLocalAPIConfig: Equatable, Sendable {
  public var bindHost: String
  public var port: Int

  public init(
    bindHost: String = LocalWebDefaults.bindHost,
    port: Int = LocalWebDefaults.wikiAPIPort
  ) {
    self.bindHost = bindHost
    self.port = port
  }

  public init(environment: [String: String]) {
    self.init(
      bindHost: environment["ONECONTEXT_WIKI_API_BIND_HOST"] ?? LocalWebDefaults.bindHost,
      port: Int(environment["ONECONTEXT_WIKI_API_PORT"] ?? "") ?? LocalWebDefaults.wikiAPIPort
    )
  }

  public var baseURL: URL {
    URL(string: "http://\(bindHost):\(port)")!
  }

  public var healthURL: URL {
    baseURL.appendingPathComponent("api/wiki/health")
  }
}

public struct WikiLocalAPISnapshot: Codable, Equatable, Sendable {
  public var running: Bool
  public var url: String
  public var health: String
  public var port: Int
  public var lastError: String?

  public init(running: Bool, url: String, health: String, port: Int, lastError: String? = nil) {
    self.running = running
    self.url = url
    self.health = health
    self.port = port
    self.lastError = lastError
  }
}

public struct WikiLocalAPIRequest: Sendable {
  public var method: String
  public var path: String
  public var query: [String: String]
  public var body: Data

  public init(method: String, path: String, query: [String: String] = [:], body: Data = Data()) {
    self.method = method.uppercased()
    self.path = path
    self.query = query
    self.body = body
  }
}

public struct WikiLocalAPIResponse: Sendable {
  public var statusCode: Int
  public var reason: String
  public var headers: [String: String]
  public var body: Data

  public init(statusCode: Int, reason: String, headers: [String: String] = [:], body: Data = Data()) {
    self.statusCode = statusCode
    self.reason = reason
    self.headers = headers
    self.body = body
  }

  public static func json(_ payload: [String: Any], statusCode: Int = 200, reason: String = "OK") -> WikiLocalAPIResponse {
    let data = (try? JSONSerialization.data(withJSONObject: payload, options: [.sortedKeys])) ?? Data("{}".utf8)
    return WikiLocalAPIResponse(
      statusCode: statusCode,
      reason: reason,
      headers: [
        "Content-Type": "application/json; charset=utf-8",
        "Cache-Control": "no-store",
        "X-Content-Type-Options": "nosniff"
      ],
      body: data
    )
  }
}

public final class WikiLocalAPIHandler: @unchecked Sendable {
  public static let maxStateBodyBytes = 128 * 1024

  private let paths: LocalWebPaths
  private let fileManager: FileManager
  private let renderState: @Sendable () -> String

  public init(
    paths: LocalWebPaths,
    fileManager: FileManager = .default,
    renderState: @escaping @Sendable () -> String = { "idle" }
  ) {
    self.paths = paths
    self.fileManager = fileManager
    self.renderState = renderState
  }

  public func handle(_ request: WikiLocalAPIRequest) -> WikiLocalAPIResponse {
    switch (request.method, request.path) {
    case ("GET", "/api/wiki/health"):
      return .json(healthPayload())
    case ("GET", "/api/wiki/search"):
      return .json(searchPayload(query: request.query["q"] ?? ""))
    case ("GET", "/api/wiki/bookmarks"):
      let state = statePayload()
      return .json(["bookmarks": state["bookmarks"] as? [Any] ?? []])
    case ("GET", "/api/wiki/state"):
      return .json(statePayload())
    case ("POST", "/api/wiki/state"), ("PATCH", "/api/wiki/state"):
      return saveState(request.body)
    case ("GET", "/api/wiki/chat/config"):
      return .json(chatConfigPayload())
    case ("POST", "/api/wiki/chat/provider"):
      return saveProviderPreference(request.body)
    case ("POST", "/api/wiki/chat/reset"):
      return .json(["ok": true, "enabled": false, "message": chatUnavailableMessage])
    case ("POST", "/api/wiki/chat"):
      return .json([
        "enabled": false,
        "provider": "none",
        "text": chatUnavailableMessage,
        "message": chatUnavailableMessage
      ])
    case ("OPTIONS", _):
      return WikiLocalAPIResponse(statusCode: 204, reason: "No Content")
    default:
      return .json(["error": "not_found", "message": "Unknown wiki API route"], statusCode: 404, reason: "Not Found")
    }
  }

  private var chatUnavailableMessage: String {
    "The 1Context Librarian chat bridge is not enabled in this release yet. Search, bookmarks, and page state are live; chat will connect through the isolated memory bridge next."
  }

  private func healthPayload() -> [String: Any] {
    let publishManifest = readJSON(paths.wikiCurrent.appendingPathComponent("publish-manifest.json"))
    return [
      "status": "ok",
      "service": "1context-wiki-api",
      "render_state": renderState(),
      "current_site": paths.wikiCurrent.path,
      "current_site_exists": fileManager.fileExists(atPath: paths.wikiCurrent.appendingPathComponent("index.html").path),
      "published_at": publishManifest?["published_at"] as? String ?? NSNull()
    ]
  }

  private func searchPayload(query: String) -> [String: Any] {
    let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
    let pages = contentPages()
    guard !trimmed.isEmpty else {
      return ["query": trimmed, "matches": [], "pages": []]
    }

    let terms = trimmed.lowercased().split(whereSeparator: \.isWhitespace).map(String.init)
    let matches = pages
      .compactMap { page -> [String: Any]? in
        let haystack = searchText(for: page)
        guard terms.allSatisfy({ haystack.contains($0) }) else { return nil }
        var result = page
        result["excerpt"] = result["excerpt"] ?? result["description"] ?? result["summary"] ?? result["route"] ?? ""
        result["score"] = score(page: page, terms: terms)
        return result
      }
      .sorted { lhs, rhs in
        (lhs["score"] as? Int ?? 0) > (rhs["score"] as? Int ?? 0)
      }
      .prefix(20)
      .map { $0 }

    return ["query": trimmed, "matches": matches, "pages": matches]
  }

  private func contentPages() -> [[String: Any]] {
    for url in [
      paths.wikiCurrent.appendingPathComponent("content-index.json"),
      paths.wikiCurrent.appendingPathComponent("api/wiki/pages.json"),
      paths.wikiCurrent.appendingPathComponent("site-manifest.json")
    ] {
      guard let object = readJSON(url), let pages = object["pages"] as? [[String: Any]] else { continue }
      return pages
    }
    return []
  }

  private func searchText(for page: [String: Any]) -> String {
    [
      page["title"],
      page["matched_title"],
      page["description"],
      page["summary"],
      page["excerpt"],
      page["route"],
      page["url"],
      page["family_label"],
      page["family_id"]
    ]
    .compactMap { $0 as? String }
    .joined(separator: " ")
    .lowercased()
  }

  private func score(page: [String: Any], terms: [String]) -> Int {
    let title = (page["title"] as? String ?? "").lowercased()
    let route = (page["route"] as? String ?? page["url"] as? String ?? "").lowercased()
    return terms.reduce(0) { partial, term in
      partial + (title.contains(term) ? 10 : 0) + (route.contains(term) ? 4 : 0) + (searchText(for: page).contains(term) ? 1 : 0)
    }
  }

  private func statePayload() -> [String: Any] {
    var state = readJSON(paths.wikiBrowserStateFile) ?? [:]
    state["_storage"] = [
      "exists": fileManager.fileExists(atPath: paths.wikiBrowserStateFile.path),
      "path": paths.wikiBrowserStateFile.path
    ]
    if state["settings"] == nil { state["settings"] = [:] }
    if state["bookmarks"] == nil { state["bookmarks"] = [] }
    if state["chat"] == nil { state["chat"] = [:] }
    return state
  }

  private func saveState(_ body: Data) -> WikiLocalAPIResponse {
    guard body.count <= Self.maxStateBodyBytes else {
      return .json(["error": "payload_too_large", "max_bytes": Self.maxStateBodyBytes], statusCode: 413, reason: "Payload Too Large")
    }
    guard let incoming = decodeJSONObject(body) else {
      return .json(["error": "invalid_json", "message": "State must be a JSON object"], statusCode: 400, reason: "Bad Request")
    }
    var state = readJSON(paths.wikiBrowserStateFile) ?? [:]
    for (key, value) in incoming where key != "_storage" {
      state[key] = value
    }
    do {
      try RuntimePermissions.ensurePrivateDirectory(paths.wikiBrowserStateFile.deletingLastPathComponent())
      let data = try JSONSerialization.data(withJSONObject: state, options: [.prettyPrinted, .sortedKeys])
      try RuntimePermissions.writePrivateData(data, to: paths.wikiBrowserStateFile)
      return .json(statePayload())
    } catch {
      return .json(["error": "write_failed", "message": error.localizedDescription], statusCode: 500, reason: "Internal Server Error")
    }
  }

  private func chatConfigPayload() -> [String: Any] {
    let state = statePayload()
    let chat = state["chat"] as? [String: Any] ?? [:]
    let settings = state["settings"] as? [String: Any] ?? [:]
    let preferred = chat["preferred_provider"] as? String ?? settings["ai-provider"] as? String ?? "auto"
    return [
      "enabled": false,
      "chat_available": false,
      "preferred_provider": preferred,
      "providers": [
        ["id": "codex", "label": "Codex", "installed": false],
        ["id": "claude", "label": "Claude", "installed": false]
      ],
      "message": chatUnavailableMessage
    ]
  }

  private func saveProviderPreference(_ body: Data) -> WikiLocalAPIResponse {
    guard body.count <= Self.maxStateBodyBytes, let incoming = decodeJSONObject(body) else {
      return .json(["error": "invalid_json", "message": "Provider preference must be a JSON object"], statusCode: 400, reason: "Bad Request")
    }
    let provider = incoming["provider"] as? String ?? "auto"
    var state = readJSON(paths.wikiBrowserStateFile) ?? [:]
    var chat = state["chat"] as? [String: Any] ?? [:]
    chat["preferred_provider"] = provider
    state["chat"] = chat
    do {
      try RuntimePermissions.ensurePrivateDirectory(paths.wikiBrowserStateFile.deletingLastPathComponent())
      let data = try JSONSerialization.data(withJSONObject: state, options: [.prettyPrinted, .sortedKeys])
      try RuntimePermissions.writePrivateData(data, to: paths.wikiBrowserStateFile)
      return .json(["ok": true, "preferred_provider": provider, "enabled": false])
    } catch {
      return .json(["error": "write_failed", "message": error.localizedDescription], statusCode: 500, reason: "Internal Server Error")
    }
  }

  private func readJSON(_ url: URL) -> [String: Any]? {
    guard let data = try? Data(contentsOf: url),
      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
      return nil
    }
    return object
  }

  private func decodeJSONObject(_ body: Data) -> [String: Any]? {
    guard !body.isEmpty,
      let object = try? JSONSerialization.jsonObject(with: body) as? [String: Any]
    else {
      return nil
    }
    return object
  }
}

public final class WikiLocalAPIServer: @unchecked Sendable {
  private let config: WikiLocalAPIConfig
  private let handler: WikiLocalAPIHandler
  private let queue = DispatchQueue(label: "com.haptica.1context.wiki-api", attributes: .concurrent)
  private let lifecycleLock = NSLock()
  private var listenFD: Int32 = -1
  private var lastError: String?

  public init(config: WikiLocalAPIConfig, handler: WikiLocalAPIHandler) {
    self.config = config
    self.handler = handler
  }

  public var snapshot: WikiLocalAPISnapshot {
    lifecycleLock.lock()
    defer { lifecycleLock.unlock() }
    return WikiLocalAPISnapshot(
      running: listenFD >= 0,
      url: config.healthURL.absoluteString,
      health: listenFD >= 0 ? "OK" : "not running",
      port: config.port,
      lastError: lastError
    )
  }

  public func start() throws -> WikiLocalAPISnapshot {
    lifecycleLock.lock()
    defer { lifecycleLock.unlock() }
    if listenFD >= 0 {
      return WikiLocalAPISnapshot(running: true, url: config.healthURL.absoluteString, health: "OK", port: config.port, lastError: lastError)
    }

    let fd = socket(AF_INET, SOCK_STREAM, 0)
    guard fd >= 0 else { throw WikiLocalAPIError.socketFailed }

    var reuse: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))
    setNoSigPipe(fd)

    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_port = UInt16(config.port).bigEndian
    address.sin_addr = in_addr(s_addr: inet_addr(config.bindHost))

    let bindResult = withUnsafePointer(to: &address) {
      $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
        Darwin.bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
      }
    }
    guard bindResult == 0 else {
      close(fd)
      throw WikiLocalAPIError.bindFailed(config.bindHost, config.port)
    }

    guard listen(fd, 16) == 0 else {
      close(fd)
      throw WikiLocalAPIError.socketFailed
    }

    listenFD = fd
    queue.async { [self] in acceptLoop(fd) }
    return WikiLocalAPISnapshot(running: true, url: config.healthURL.absoluteString, health: "OK", port: config.port, lastError: lastError)
  }

  public func stop() {
    lifecycleLock.lock()
    let fd = listenFD
    listenFD = -1
    lifecycleLock.unlock()
    if fd >= 0 {
      shutdown(fd, SHUT_RDWR)
      close(fd)
    }
  }

  private func acceptLoop(_ fd: Int32) {
    while true {
      let client = accept(fd, nil, nil)
      if client < 0 { break }
      queue.async { [self] in
        handle(client)
        close(client)
      }
    }
  }

  private func handle(_ fd: Int32) {
    setNoSigPipe(fd)
    guard let request = readHTTPRequest(fd) else {
      writeResponse(.json(["error": "bad_request"], statusCode: 400, reason: "Bad Request"), to: fd)
      return
    }
    writeResponse(handler.handle(request), to: fd)
  }

  private func readHTTPRequest(_ fd: Int32) -> WikiLocalAPIRequest? {
    var data = Data()
    var buffer = [UInt8](repeating: 0, count: 4096)
    var headerEnd: Range<Data.Index>?
    let deadline = Date().addingTimeInterval(2)

    repeat {
      var pollFD = pollfd(fd: fd, events: Int16(POLLIN), revents: 0)
      guard poll(&pollFD, 1, 200) > 0 else { continue }
      let count = read(fd, &buffer, buffer.count)
      guard count > 0 else { return nil }
      data.append(buffer, count: count)
      headerEnd = data.range(of: Data("\r\n\r\n".utf8))
      if data.count > 256 * 1024 { return nil }
    } while headerEnd == nil && Date() < deadline

    guard let headerEnd else { return nil }
    let headerData = data[..<headerEnd.lowerBound]
    guard let headerText = String(data: headerData, encoding: .utf8) else { return nil }
    let lines = headerText.components(separatedBy: "\r\n")
    guard let requestLine = lines.first else { return nil }
    let parts = requestLine.split(separator: " ")
    guard parts.count >= 2 else { return nil }

    let method = String(parts[0])
    let target = String(parts[1])
    let contentLength = lines.compactMap { line -> Int? in
      let pieces = line.split(separator: ":", maxSplits: 1).map { String($0).trimmingCharacters(in: .whitespaces) }
      guard pieces.count == 2, pieces[0].lowercased() == "content-length" else { return nil }
      return Int(pieces[1])
    }.first ?? 0

    let bodyStart = headerEnd.upperBound
    var body = Data(data[bodyStart...])
    while body.count < contentLength && Date() < deadline {
      let count = read(fd, &buffer, min(buffer.count, contentLength - body.count))
      guard count > 0 else { break }
      body.append(buffer, count: count)
    }
    if body.count > contentLength {
      body = Data(body.prefix(contentLength))
    }

    let parsed = parseTarget(target)
    return WikiLocalAPIRequest(method: method, path: parsed.path, query: parsed.query, body: body)
  }

  private func parseTarget(_ target: String) -> (path: String, query: [String: String]) {
    var components = URLComponents()
    components.percentEncodedPath = target.split(separator: "?", maxSplits: 1).first.map(String.init) ?? target
    if let queryPart = target.split(separator: "?", maxSplits: 1).dropFirst().first {
      components.percentEncodedQuery = String(queryPart)
    }
    let path = components.path.isEmpty ? "/" : components.path
    let query = Dictionary(uniqueKeysWithValues: (components.queryItems ?? []).map { ($0.name, $0.value ?? "") })
    return (path, query)
  }

  private func writeResponse(_ response: WikiLocalAPIResponse, to fd: Int32) {
    var headers = response.headers
    headers["Content-Length"] = "\(response.body.count)"
    headers["Connection"] = "close"
    let lines = ["HTTP/1.1 \(response.statusCode) \(response.reason)"]
      + headers.map { "\($0.key): \($0.value)" }.sorted()
      + ["", ""]
    let data = Data(lines.joined(separator: "\r\n").utf8) + response.body
    _ = writeAll(data, to: fd)
  }

  private func writeAll(_ data: Data, to fd: Int32) -> Bool {
    data.withUnsafeBytes { raw in
      guard let base = raw.baseAddress else { return false }
      var sent = 0
      while sent < data.count {
        let count = write(fd, base.advanced(by: sent), data.count - sent)
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
}

public enum WikiLocalAPIProbe {
  public static func health(environment: [String: String] = ProcessInfo.processInfo.environment) -> String {
    let config = WikiLocalAPIConfig(environment: environment)
    var request = URLRequest(url: config.healthURL)
    request.timeoutInterval = 0.5
    let semaphore = DispatchSemaphore(value: 0)
    nonisolated(unsafe) var result = "no response"
    let task = URLSession.shared.dataTask(with: request) { data, _, _ in
      defer { semaphore.signal() }
      guard let data,
        let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
        object["status"] as? String == "ok"
      else {
        return
      }
      result = "OK"
    }
    task.resume()
    if semaphore.wait(timeout: .now() + 1) == .timedOut {
      task.cancel()
      return "timeout"
    }
    return result
  }
}

public enum WikiLocalAPIError: Error, LocalizedError, Equatable {
  case socketFailed
  case bindFailed(String, Int)

  public var errorDescription: String? {
    switch self {
    case .socketFailed:
      return "Could not start the 1Context wiki API listener"
    case .bindFailed(let host, let port):
      return "Could not bind the 1Context wiki API listener to \(host):\(port)"
    }
  }
}
