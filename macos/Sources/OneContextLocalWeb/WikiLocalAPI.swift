import Darwin
import Foundation
import OneContextPlatform

public struct WikiLocalAPIConfig: Equatable, Sendable {
  private static let defaultPortRegistry = WikiLocalAPIPortRegistry()

  public var bindHost: String
  public var port: Int

  public init(
    bindHost: String = LocalWebDefaults.bindHost,
    port: Int = LocalWebDefaults.wikiAPIPort
  ) {
    self.bindHost = bindHost
    self.port = port
  }

  public var baseURL: URL {
    URL(string: "http://\(bindHost):\(port)")!
  }

  public var healthURL: URL {
    baseURL.appendingPathComponent("api/wiki/health")
  }

  static var defaultPortCandidates: [Int] {
    [LocalWebDefaults.wikiAPIPort] + Array((LocalWebDefaults.wikiAPIPort + 1)...(LocalWebDefaults.wikiAPIPort + 40))
  }

  static func currentDefaultPort() -> Int {
    defaultPortRegistry.currentPort ?? LocalWebDefaults.wikiAPIPort
  }

  static func rememberDefaultPort(_ port: Int) {
    defaultPortRegistry.currentPort = port
  }

  static func resetPreferredPortForTesting() {
    defaultPortRegistry.currentPort = nil
  }
}

private final class WikiLocalAPIPortRegistry: @unchecked Sendable {
  private let lock = NSLock()
  private var selectedPort: Int?

  var currentPort: Int? {
    get {
      lock.lock()
      defer { lock.unlock() }
      return selectedPort
    }
    set {
      lock.lock()
      selectedPort = newValue
      lock.unlock()
    }
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

  public static func html(_ body: String, statusCode: Int = 200, reason: String = "OK") -> WikiLocalAPIResponse {
    WikiLocalAPIResponse(
      statusCode: statusCode,
      reason: reason,
      headers: [
        "Content-Type": "text/html; charset=utf-8",
        "Cache-Control": "no-store",
        "X-Content-Type-Options": "nosniff"
      ],
      body: Data(body.utf8)
    )
  }
}

public final class WikiLocalAPIHandler: @unchecked Sendable {
  public static let maxStateBodyBytes = 128 * 1024

  private let paths: LocalWebPaths
  private let fileManager: FileManager
  private let renderState: @Sendable () -> String
  private let memoryStatus: () -> [String: Any]
  private let memoryViewport: ([String: String]) throws -> [String: Any]
  private let memoryObject: ([String: String]) throws -> [String: Any]
  private let memoryDensity: ([String: String]) throws -> [String: Any]
  private let memoryEdges: ([String: String]) throws -> [String: Any]
  private let memorySearch: ([String: String]) throws -> [String: Any]

  public init(
    paths: LocalWebPaths,
    fileManager: FileManager = .default,
    renderState: @escaping @Sendable () -> String = { "idle" },
    memoryStatus: @escaping () -> [String: Any] = { ["status": "unavailable"] },
    memoryViewport: @escaping ([String: String]) throws -> [String: Any] = { _ in
      throw WikiLocalAPIError.memoryUnavailable("memory.queryViewport is not configured")
    },
    memoryObject: @escaping ([String: String]) throws -> [String: Any] = { _ in
      throw WikiLocalAPIError.memoryUnavailable("memory.hydrateObjects is not configured")
    },
    memoryDensity: @escaping ([String: String]) throws -> [String: Any] = { _ in
      throw WikiLocalAPIError.memoryUnavailable("memory.queryDensity is not configured")
    },
    memoryEdges: @escaping ([String: String]) throws -> [String: Any] = { _ in
      throw WikiLocalAPIError.memoryUnavailable("memory.queryEdges is not configured")
    },
    memorySearch: @escaping ([String: String]) throws -> [String: Any] = { _ in
      throw WikiLocalAPIError.memoryUnavailable("memory.searchText is not configured")
    }
  ) {
    self.paths = paths
    self.fileManager = fileManager
    self.renderState = renderState
    self.memoryStatus = memoryStatus
    self.memoryViewport = memoryViewport
    self.memoryObject = memoryObject
    self.memoryDensity = memoryDensity
    self.memoryEdges = memoryEdges
    self.memorySearch = memorySearch
  }

  public func handle(_ request: WikiLocalAPIRequest) -> WikiLocalAPIResponse {
    switch (request.method, request.path) {
    case ("GET", "/memory"):
      return .html(memoryViewerHTML())
    case ("GET", "/api/memory/status"):
      return .json(browserSafeMemoryPayload(memoryStatus()))
    case ("GET", "/api/memory/viewport"), ("POST", "/api/memory/viewport"):
      return memoryResponse(surface: "memory_viewport") {
        try memoryViewport(memoryQueryParameters(request))
      }
    case ("GET", "/api/memory/object"), ("POST", "/api/memory/object"):
      return memoryResponse(surface: "memory_object_hydration") {
        try memoryObject(memoryQueryParameters(request))
      }
    case ("GET", "/api/memory/density"), ("POST", "/api/memory/density"):
      return memoryResponse(surface: "memory_density") {
        try memoryDensity(memoryQueryParameters(request))
      }
    case ("GET", "/api/memory/edges"), ("POST", "/api/memory/edges"):
      return memoryResponse(surface: "memory_edges") {
        try memoryEdges(memoryQueryParameters(request))
      }
    case ("GET", "/api/memory/search"), ("POST", "/api/memory/search"):
      return memoryResponse(surface: "memory_search") {
        try memorySearch(memoryQueryParameters(request))
      }
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
    case ("OPTIONS", _):
      return WikiLocalAPIResponse(statusCode: 204, reason: "No Content")
    default:
      return .json(["error": "not_found", "message": "Unknown wiki API route"], statusCode: 404, reason: "Not Found")
    }
  }

  private func memoryResponse(
    surface: String,
    _ action: () throws -> [String: Any]
  ) -> WikiLocalAPIResponse {
    do {
      return .json(browserSafeMemoryPayload(try action()))
    } catch {
      return .json([
        "schema_version": 1,
        "surface": surface,
        "status": "error",
        "error": [
          "code": "memory_protocol_error",
          "message": error.localizedDescription
        ]
      ], statusCode: 503, reason: "Service Unavailable")
    }
  }

  private func healthPayload() -> [String: Any] {
    let currentRender = readJSON(
      paths.wikiCurrent
        .appendingPathComponent(".1context", isDirectory: true)
        .appendingPathComponent("current-render.json")
    )
    let publishedAt = publishedAt(from: currentRender)
    return [
      "status": "ok",
      "service": "1context-wiki-api",
      "render_state": renderState(),
      "current_site": "app-support://wiki-site/current",
      "current_site_exists": fileManager.fileExists(atPath: paths.wikiCurrent.appendingPathComponent("index.html").path),
      "published_at": publishedAt ?? NSNull()
    ]
  }

  private func publishedAt(from object: [String: Any]?) -> String? {
    object?["published_at"] as? String ?? object?["publishedAt"] as? String
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
      paths.wikiCurrent.appendingPathComponent(".1context/content-index.json"),
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
      "uri": "app-support://local-web/wiki-browser-state.json"
    ]
    if state["settings"] == nil { state["settings"] = [:] }
    if state["bookmarks"] == nil { state["bookmarks"] = [] }
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

  private func memoryQueryParameters(_ request: WikiLocalAPIRequest) -> [String: String] {
    var parameters = request.query
    guard let body = decodeJSONObject(request.body) else {
      return parameters
    }
    for (key, value) in body {
      switch value {
      case let string as String:
        parameters[key] = string
      case let number as NSNumber:
        parameters[key] = number.stringValue
      case let array as [String]:
        parameters[key] = array.joined(separator: ",")
      case let array as [Any]:
        parameters[key] = array.compactMap { $0 as? String }.joined(separator: ",")
      default:
        continue
      }
    }
    return parameters
  }

  private func browserSafeMemoryPayload(_ payload: [String: Any]) -> [String: Any] {
    redactBrowserVisibleValue(payload) as? [String: Any] ?? payload
  }

  private func redactBrowserVisibleValue(_ value: Any) -> Any {
    if let object = value as? [String: Any] {
      return object.mapValues { redactBrowserVisibleValue($0) }
    }
    if let array = value as? [Any] {
      return array.map { redactBrowserVisibleValue($0) }
    }
    guard var string = value as? String else {
      return value
    }

    let appSupportRoot = paths.directory.deletingLastPathComponent().path
    let appSupportParent = paths.directory
      .deletingLastPathComponent()
      .deletingLastPathComponent()
    let inferredRuntimeRoot = appSupportParent.deletingLastPathComponent()
    let inferredUserContentRoot = inferredRuntimeRoot.appendingPathComponent("1Context", isDirectory: true).path
    if string.hasPrefix("file://"),
      let url = URL(string: string),
      url.isFileURL
    {
      string = url.path
    }
    let replacements = [
      (paths.wikiSiteDirectory.path, "app-support://wiki-site"),
      (appSupportRoot, "app-support://"),
      (inferredUserContentRoot, "user-content://"),
      (NSHomeDirectory(), "~"),
      (inferredRuntimeRoot.path, "runtime://")
    ]
    for (prefix, replacement) in replacements where !prefix.isEmpty {
      if string == prefix {
        string = replacement
      } else if string.hasPrefix(prefix + "/") {
        var suffix = String(string.dropFirst(prefix.count))
        if replacement.hasSuffix("://"), suffix.hasPrefix("/") {
          suffix.removeFirst()
        }
        string = replacement + suffix
      }
    }
    return string
  }

  private func memoryViewerHTML() -> String {
    """
    <!doctype html>
    <html lang="en">
    <head>
      <meta charset="utf-8">
      <meta name="viewport" content="width=device-width, initial-scale=1">
      <title>1Context Memory</title>
      <style>
        :root {
          color-scheme: light dark;
          --bg: #f6f5f1;
          --ink: #161616;
          --muted: #68645d;
          --panel: #ffffff;
          --line: #d8d2c5;
          --codex: #2d6cdf;
          --claude: #b84d2c;
          --imessage: #0d8f65;
          --other: #6f5aa7;
        }

        @media (prefers-color-scheme: dark) {
          :root {
            --bg: #151515;
            --ink: #f2f0ea;
            --muted: #b6b0a5;
            --panel: #20201f;
            --line: #393734;
          }
        }

        * { box-sizing: border-box; }

        body {
          margin: 0;
          background: var(--bg);
          color: var(--ink);
          font: 14px/1.4 -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
        }

        header {
          display: flex;
          align-items: center;
          justify-content: space-between;
          gap: 16px;
          padding: 18px 22px;
          border-bottom: 1px solid var(--line);
        }

        h1 {
          margin: 0;
          font-size: 18px;
          font-weight: 700;
        }

        button {
          appearance: none;
          border: 1px solid var(--line);
          background: var(--panel);
          color: var(--ink);
          border-radius: 8px;
          padding: 7px 11px;
          font: inherit;
          cursor: pointer;
        }

        main {
          display: grid;
          grid-template-columns: minmax(260px, 330px) minmax(0, 1fr);
          min-height: calc(100vh - 62px);
        }

        aside {
          border-right: 1px solid var(--line);
          padding: 18px;
        }

        .content { padding: 18px 22px 28px; }
        .panel {
          background: var(--panel);
          border: 1px solid var(--line);
          border-radius: 8px;
          padding: 14px;
        }

        .metric-grid {
          display: grid;
          grid-template-columns: 1fr 1fr;
          gap: 10px;
          margin-bottom: 14px;
        }

        .metric span {
          display: block;
          color: var(--muted);
          font-size: 11px;
          text-transform: uppercase;
          letter-spacing: .04em;
        }

        .metric strong {
          display: block;
          margin-top: 3px;
          font-size: 18px;
        }

        .metric small {
          display: block;
          margin-top: 2px;
          color: var(--muted);
          font-size: 12px;
        }

        .filters {
          display: flex;
          flex-wrap: wrap;
          gap: 8px;
          margin-top: 14px;
        }

        .filters button.active {
          border-color: var(--ink);
          box-shadow: inset 0 0 0 1px var(--ink);
        }

        .lane {
          display: grid;
          grid-template-columns: 180px minmax(0, 1fr);
          gap: 12px;
          align-items: stretch;
          padding: 12px 0;
          border-bottom: 1px solid var(--line);
        }

        .lane-name {
          font-weight: 650;
          overflow-wrap: anywhere;
        }

        .lane-name span {
          display: block;
          color: var(--muted);
          font-size: 12px;
          font-weight: 400;
        }

        .timeline {
          position: relative;
          height: 34px;
          border-radius: 8px;
          background: color-mix(in srgb, var(--line) 42%, transparent);
          overflow: hidden;
        }

        .bar {
          position: absolute;
          top: 7px;
          min-width: 4px;
          height: 20px;
          border-radius: 5px;
          background: var(--other);
        }

        .bar.codex { background: var(--codex); }
        .bar.claude { background: var(--claude); }
        .bar.imessage { background: var(--imessage); }

        .records {
          display: grid;
          gap: 10px;
          margin-top: 16px;
        }

        .record {
          display: grid;
          grid-template-columns: 136px minmax(0, 1fr);
          gap: 12px;
          background: var(--panel);
          border: 1px solid var(--line);
          border-radius: 8px;
          padding: 12px;
          cursor: pointer;
        }

        .record.selected {
          border-color: var(--ink);
          box-shadow: inset 0 0 0 1px var(--ink);
        }

        .stamp {
          color: var(--muted);
          font-size: 12px;
          overflow-wrap: anywhere;
        }

        .title {
          display: flex;
          align-items: center;
          gap: 8px;
          font-weight: 650;
        }

        .dot {
          width: 9px;
          height: 9px;
          border-radius: 999px;
          background: var(--other);
          flex: 0 0 auto;
        }

        .dot.codex { background: var(--codex); }
        .dot.claude { background: var(--claude); }
        .dot.imessage { background: var(--imessage); }

        .text {
          margin-top: 4px;
          color: var(--muted);
          white-space: pre-wrap;
          overflow-wrap: anywhere;
          max-height: 8.4em;
          overflow: hidden;
        }

        .empty {
          color: var(--muted);
          padding: 24px;
          text-align: center;
        }

        .state {
          margin-bottom: 12px;
          color: var(--muted);
        }

        .state.error {
          color: #b3261e;
          border-color: #e2a6a0;
        }

        .density, .detail {
          margin-top: 14px;
        }

        .density-bars {
          display: grid;
          grid-template-columns: repeat(auto-fit, minmax(28px, 1fr));
          align-items: end;
          gap: 4px;
          height: 70px;
        }

        .bucket {
          min-height: 3px;
          border-radius: 4px 4px 0 0;
          background: var(--codex);
        }

        .detail pre {
          margin: 10px 0 0;
          max-height: 260px;
          overflow: auto;
          white-space: pre-wrap;
          overflow-wrap: anywhere;
        }

        @media (max-width: 760px) {
          main { grid-template-columns: 1fr; }
          aside { border-right: 0; border-bottom: 1px solid var(--line); }
          .lane { grid-template-columns: 1fr; }
          .record { grid-template-columns: 1fr; }
        }
      </style>
    </head>
    <body>
      <header>
        <h1>1Context Memory</h1>
        <button id="refresh" type="button">Refresh</button>
      </header>
      <main>
        <aside>
          <div class="metric-grid">
            <div class="metric panel"><span>Records</span><strong id="record-count">0</strong><small id="record-detail"></small></div>
            <div class="metric panel"><span>Sources</span><strong id="source-count">0</strong></div>
            <div class="metric panel"><span>Daemon</span><strong id="daemon-state">-</strong></div>
            <div class="metric panel"><span>Tick</span><strong id="tick-ms">-</strong></div>
          </div>
          <div class="panel">
            <div class="lane-name">Sources</div>
            <div id="filters" class="filters"></div>
          </div>
        </aside>
        <section class="content">
          <div id="load-state" class="state panel">Loading memory viewport...</div>
          <div id="lanes" class="panel"></div>
          <div id="density" class="density panel"></div>
          <div id="detail" class="detail panel"></div>
          <div id="records" class="records"></div>
        </section>
      </main>
      <script>
        const state = { source: "all", selectedObjectID: null };
        const sourceClass = source => {
          const value = String(source || "").toLowerCase();
          if (value.includes("codex")) return "codex";
          if (value.includes("claude")) return "claude";
          if (value.includes("imessage")) return "imessage";
          return "other";
        };
        const fmt = value => {
          const date = value ? new Date(value) : null;
          return date && !Number.isNaN(date.getTime()) ? date.toLocaleString() : "-";
        };
        const text = value => value == null ? "" : String(value);
        const normalize = item => {
          const record = item.record || item;
          const envelope = record.envelope || {};
          const sourceRecord = item.source_record || record.source_record || {};
          const sourceValue = item.source || item.source_type || item.source_id || sourceRecord.source_key || record.connector_key || "unknown";
          return {
            objectID: item.object_id || record.object_id || envelope.object_id || "",
            source: sourceValue,
            kind: item.kind || record.kind || envelope.kind || "object",
            title: item.display_title || record.display_title || envelope.display_title || envelope.kind || "Captured object",
            body: item.display_text_preview || item.display_text || record.display_text_preview || record.display_text || envelope.display_text || redactionText(envelope),
            start: item.event_start || record.event_start || envelope.event_start || item.stored_at,
            end: item.event_end || record.event_end || envelope.event_end || envelope.event_start || item.stored_at,
            hash: item.source_record_hash || item.source_hash || sourceRecord.source_record_hash || record.source_hash || "",
            uri: item.source_uri || sourceRecord.source_record_key || record.source_uri || ""
          };
        };
        const redactionText = envelope => {
          const payload = envelope.payload || {};
          if (payload.text_redacted) {
            const count = payload.text_char_count || 0;
            return count ? `Sensitive text redacted (${count} characters)` : "Sensitive text redacted";
          }
          return "No display text";
        };
        async function load() {
          setLoadState("Loading memory viewport...");
          const [status, viewport, density] = await Promise.all([
            fetchJSON("/api/memory/status"),
            fetchJSON(`/api/memory/viewport?limit=180&source=${encodeURIComponent(state.source)}`),
            fetchJSON(`/api/memory/density?bucket=1m&source=${encodeURIComponent(state.source)}`)
          ]);
          const viewportProblem = protocolProblem(viewport);
          const viewportResult = protocolResult(viewport);
          const statusBody = status.status || {};
          const records = (viewportResult.objects || viewportResult.records || []).map(normalize).sort((a, b) => new Date(a.start) - new Date(b.start));
          const defaultSources = ["codex.local_sessions", "claude.local_sessions", "imessage.chat_db"];
          const sources = Array.from(new Set([...defaultSources, ...(viewportResult.sources || []), ...records.map(r => r.source).filter(Boolean)])).sort();
          const shownCount = viewportResult.shown_object_count ?? viewportResult.shown_record_count ?? viewportResult.object_count ?? viewportResult.record_count ?? records.length;
          const totalCount = viewportResult.total_object_count ?? viewportResult.total_record_count ?? shownCount;
          document.getElementById("record-count").textContent = totalCount.toLocaleString();
          document.getElementById("record-detail").textContent = shownCount === totalCount ? "shown" : `${shownCount.toLocaleString()} shown`;
          document.getElementById("source-count").textContent = sources.length;
          document.getElementById("daemon-state").textContent = status.running ? "running" : "stopped";
          document.getElementById("tick-ms").textContent = statusBody.elapsed_ms == null ? "-" : `${statusBody.elapsed_ms} ms`;
          renderFilters(sources);
          renderLanes(records);
          renderDensity(density);
          renderRecords(records.slice(-160).reverse());
          if (state.selectedObjectID) {
            await openObject(state.selectedObjectID, { preserveSelection: true });
          } else {
            renderDetail(null);
          }
          if (viewportProblem) {
            setLoadState(viewportProblem, true);
          } else {
            setLoadState(records.length ? "" : "No memory objects in this viewport yet.");
          }
        }
        function renderFilters(sources) {
          const host = document.getElementById("filters");
          const all = ["all", ...sources];
          host.innerHTML = all.map(source => `<button type="button" data-source="${escapeAttr(source)}" class="${state.source === source ? "active" : ""}">${escapeHTML(source)}</button>`).join("");
          host.querySelectorAll("button").forEach(button => {
            button.addEventListener("click", () => {
              state.source = button.dataset.source;
              load();
            });
          });
        }
        function renderLanes(records) {
          const host = document.getElementById("lanes");
          if (!records.length) {
            host.innerHTML = `<div class="empty">No memory objects in this viewport yet.</div>`;
            return;
          }
          const times = records.flatMap(r => [new Date(r.start).getTime(), new Date(r.end).getTime()]).filter(Number.isFinite);
          const min = Math.min(...times);
          const max = Math.max(...times);
          const span = Math.max(1, max - min);
          const bySource = Map.groupBy ? Map.groupBy(records, r => r.source) : records.reduce((map, r) => {
            if (!map.has(r.source)) map.set(r.source, []);
            map.get(r.source).push(r);
            return map;
          }, new Map());
          host.innerHTML = Array.from(bySource.entries()).map(([source, rows]) => {
            const bars = rows.map(row => {
              const left = Math.max(0, Math.min(100, ((new Date(row.start).getTime() - min) / span) * 100));
              const width = Math.max(0.8, Math.min(100 - left, ((new Date(row.end).getTime() - new Date(row.start).getTime()) / span) * 100));
              return `<span class="bar ${sourceClass(source)}" title="${escapeAttr(`${row.kind} ${fmt(row.start)}`)}" style="left:${left}%;width:${width}%"></span>`;
            }).join("");
            return `<div class="lane"><div class="lane-name">${escapeHTML(source)}<span>${rows.length} objects</span></div><div class="timeline">${bars}</div></div>`;
          }).join("");
        }
        function renderRecords(records) {
          const host = document.getElementById("records");
          host.innerHTML = records.map(row => `
            <article class="record ${state.selectedObjectID && state.selectedObjectID === row.objectID ? "selected" : ""}" data-object-id="${escapeAttr(row.objectID)}" tabindex="0" role="button">
              <div class="stamp">${escapeHTML(fmt(row.start))}<br>${escapeHTML(row.source)}</div>
              <div>
                <div class="title"><span class="dot ${sourceClass(row.source)}"></span>${escapeHTML(row.title)} · ${escapeHTML(row.kind)}</div>
                <div class="text">${escapeHTML(row.body)}</div>
              </div>
            </article>
          `).join("");
          host.querySelectorAll(".record[data-object-id]").forEach(record => {
            const objectID = record.dataset.objectId;
            if (!objectID) return;
            const activate = () => openObject(objectID);
            record.addEventListener("click", activate);
            record.addEventListener("keydown", event => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                activate();
              }
            });
          });
        }
        function renderDensity(payload) {
          const host = document.getElementById("density");
          const densityResult = protocolResult(payload);
          const buckets = densityResult.buckets || densityResult.density || [];
          if (!buckets.length) {
            host.innerHTML = `<div class="lane-name">Density<span>No density buckets available.</span></div>`;
            return;
          }
          const values = buckets.map(bucket => Number(bucket.object_count ?? bucket.count ?? bucket.value ?? 0));
          const max = Math.max(1, ...values);
          host.innerHTML = `
            <div class="lane-name">Density<span>${buckets.length} buckets</span></div>
            <div class="density-bars">
              ${buckets.map((bucket, index) => {
                const value = values[index];
                const height = Math.max(3, Math.round((value / max) * 70));
                return `<span class="bucket" style="height:${height}px" title="${escapeAttr(`${bucket.bucket_start || bucket.start || bucket.time || ""}: ${value}`)}"></span>`;
              }).join("")}
            </div>
          `;
        }
        async function openObject(objectID, options = {}) {
          state.selectedObjectID = objectID;
          renderDetail({ status: "loading", object_id: objectID });
          try {
            const detail = await fetchJSON(`/api/memory/object?object_id=${encodeURIComponent(objectID)}`);
            renderDetail(detail);
            if (!options.preserveSelection) {
              document.querySelectorAll(".record").forEach(item => {
                item.classList.toggle("selected", item.dataset.objectId === objectID);
              });
            }
          } catch (error) {
            renderDetail({ status: "error", object_id: objectID, message: error.message });
          }
        }
        function renderDetail(payload) {
          const host = document.getElementById("detail");
          if (!payload) {
            host.innerHTML = `<div class="lane-name">Object details<span>Select an object to hydrate it.</span></div>`;
            return;
          }
          if (payload.status === "loading") {
            host.innerHTML = `<div class="lane-name">Object details<span>Loading ${escapeHTML(payload.object_id)}...</span></div>`;
            return;
          }
          const detailResult = protocolResult(payload);
          const object = detailResult.object || (detailResult.objects || [])[0] || detailResult;
          const title = object.display_title || object.title || object.object_id || payload.object_id || "Object details";
          const body = JSON.stringify(object, null, 2);
          host.innerHTML = `
            <div class="lane-name">${escapeHTML(title)}<span>${escapeHTML(payload.status || "ok")}</span></div>
            ${payload.message ? `<div class="text">${escapeHTML(payload.message)}</div>` : ""}
            <pre>${escapeHTML(body)}</pre>
          `;
        }
        function setLoadState(message, isError = false) {
          const host = document.getElementById("load-state");
          host.textContent = message;
          host.classList.toggle("error", isError);
          host.hidden = !message;
        }
        async function fetchJSON(url) {
          const response = await fetch(url);
          const payload = await response.json().catch(() => ({}));
          if (!response.ok) {
            throw new Error(payload.message || payload.error || `${response.status} ${response.statusText}`);
          }
          return payload;
        }
        function protocolResult(payload) {
          return payload && payload.result && typeof payload.result === "object" ? payload.result : (payload || {});
        }
        function protocolProblem(payload) {
          if (!payload || payload.status === "ok" || payload.status == null) return "";
          const error = payload.error && typeof payload.error === "object" ? payload.error.message : payload.error;
          return payload.message || error || `Memory protocol ${payload.status}`;
        }
        function escapeHTML(value) {
          return text(value).replace(/[&<>"']/g, char => ({
            "&": "&amp;",
            "<": "&lt;",
            ">": "&gt;",
            '"': "&quot;",
            "'": "&#039;"
          })[char]);
        }
        function escapeAttr(value) {
          return escapeHTML(value).replace(/`/g, "&#096;");
        }
        document.getElementById("refresh").addEventListener("click", load);
        load().catch(error => {
          setLoadState(error.message, true);
          document.getElementById("records").innerHTML = "";
        });
      </script>
    </body>
    </html>
    """
  }
}

private enum WikiLocalAPIListenerFailure: Error {
  case socket(String)
  case bind(code: Int32, message: String)
  case listen(String)
}

public final class WikiLocalAPIServer: @unchecked Sendable {
  private let config: WikiLocalAPIConfig
  private let handler: WikiLocalAPIHandler
  private let queue = DispatchQueue(label: "com.haptica.1context.wiki-api", attributes: .concurrent)
  private let lifecycleLock = NSLock()
  private var listenFD: Int32 = -1
  private var activeConfig: WikiLocalAPIConfig
  private var lastError: String?

  public init(config: WikiLocalAPIConfig, handler: WikiLocalAPIHandler) {
    self.config = config
    self.activeConfig = config
    self.handler = handler
  }

  public var snapshot: WikiLocalAPISnapshot {
    lifecycleLock.lock()
    defer { lifecycleLock.unlock() }
    return WikiLocalAPISnapshot(
      running: listenFD >= 0,
      url: activeConfig.healthURL.absoluteString,
      health: listenFD >= 0 ? "OK" : "not running",
      port: activeConfig.port,
      lastError: lastError
    )
  }

  public func start() throws -> WikiLocalAPISnapshot {
    lifecycleLock.lock()
    defer { lifecycleLock.unlock() }
    if listenFD >= 0 {
      return WikiLocalAPISnapshot(running: true, url: activeConfig.healthURL.absoluteString, health: "OK", port: activeConfig.port, lastError: lastError)
    }

    let listener = try openListener()

    listenFD = listener.fd
    activeConfig = listener.config
    lastError = listener.warning
    if config.bindHost == LocalWebDefaults.bindHost, config.port == LocalWebDefaults.wikiAPIPort {
      WikiLocalAPIConfig.rememberDefaultPort(listener.config.port)
    }
    queue.async { [self] in acceptLoop(listener.fd) }
    return WikiLocalAPISnapshot(
      running: true,
      url: listener.config.healthURL.absoluteString,
      health: "OK",
      port: listener.config.port,
      lastError: lastError
    )
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

  private func openListener() throws -> (fd: Int32, config: WikiLocalAPIConfig, warning: String?) {
    var lastBindError: WikiLocalAPIError?
    for candidatePort in candidatePorts() {
      switch bindListener(port: candidatePort) {
      case .success(let fd):
        let selectedConfig = WikiLocalAPIConfig(bindHost: config.bindHost, port: candidatePort)
        let warning = candidatePort == config.port
          ? nil
          : "Default 1Context wiki API port \(config.port) is busy; using \(candidatePort)."
        return (fd, selectedConfig, warning)
      case .failure(.bind(let code, let message)):
        let error = WikiLocalAPIError.bindFailed(config.bindHost, candidatePort, message)
        if code == EADDRINUSE, shouldTryFallback(after: candidatePort) {
          lastBindError = error
          continue
        }
        lastError = error.localizedDescription
        throw error
      case .failure(.socket(let message)), .failure(.listen(let message)):
        let error = WikiLocalAPIError.socketFailed(message)
        lastError = error.localizedDescription
        throw error
      }
    }
    let error = lastBindError ?? WikiLocalAPIError.bindFailed(config.bindHost, config.port, "no available loopback port")
    lastError = error.localizedDescription
    throw error
  }

  private func bindListener(port: Int) -> Result<Int32, WikiLocalAPIListenerFailure> {
    let fd = socket(AF_INET, SOCK_STREAM, 0)
    guard fd >= 0 else {
      return .failure(.socket(String(cString: strerror(errno))))
    }

    var reuse: Int32 = 1
    setsockopt(fd, SOL_SOCKET, SO_REUSEADDR, &reuse, socklen_t(MemoryLayout<Int32>.size))
    setNoSigPipe(fd)

    var address = sockaddr_in()
    address.sin_len = UInt8(MemoryLayout<sockaddr_in>.size)
    address.sin_family = sa_family_t(AF_INET)
    address.sin_port = UInt16(port).bigEndian
    address.sin_addr = in_addr(s_addr: inet_addr(config.bindHost))

    let bindResult = withUnsafePointer(to: &address) {
      $0.withMemoryRebound(to: sockaddr.self, capacity: 1) {
        Darwin.bind(fd, $0, socklen_t(MemoryLayout<sockaddr_in>.size))
      }
    }
    guard bindResult == 0 else {
      let code = errno
      let message = String(cString: strerror(code))
      close(fd)
      return .failure(.bind(code: code, message: message))
    }

    guard listen(fd, 16) == 0 else {
      let code = errno
      let message = String(cString: strerror(code))
      close(fd)
      return .failure(.listen(message))
    }
    return .success(fd)
  }

  private func candidatePorts() -> [Int] {
    guard config.bindHost == LocalWebDefaults.bindHost, config.port == LocalWebDefaults.wikiAPIPort else {
      return [config.port]
    }
    return WikiLocalAPIConfig.defaultPortCandidates
  }

  private func shouldTryFallback(after port: Int) -> Bool {
    config.bindHost == LocalWebDefaults.bindHost
      && config.port == LocalWebDefaults.wikiAPIPort
      && port != WikiLocalAPIConfig.defaultPortCandidates.last
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
    var query: [String: String] = [:]
    for item in components.queryItems ?? [] {
      query[item.name] = item.value ?? ""
    }
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
  public static func health(config: WikiLocalAPIConfig = WikiLocalAPIConfig()) -> String {
    var request = URLRequest(url: config.healthURL)
    request.timeoutInterval = 0.5
    let semaphore = DispatchSemaphore(value: 0)
    nonisolated(unsafe) var result = "no response"
    let configuration = URLSessionConfiguration.ephemeral
    configuration.connectionProxyDictionary = [:]
    let session = URLSession(configuration: configuration)
    let task = session.dataTask(with: request) { data, _, _ in
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
      session.invalidateAndCancel()
      return "timeout"
    }
    session.finishTasksAndInvalidate()
    return result
  }
}

public enum WikiLocalAPIError: Error, LocalizedError, Equatable {
  case socketFailed(String)
  case bindFailed(String, Int, String)
  case memoryUnavailable(String)

  public var errorDescription: String? {
    switch self {
    case .socketFailed(let message):
      return message.isEmpty
        ? "Could not start the 1Context wiki API listener"
        : "Could not start the 1Context wiki API listener: \(message)"
    case .bindFailed(let host, let port, let message):
      return message.isEmpty
        ? "Could not bind the 1Context wiki API listener to \(host):\(port)"
        : "Could not bind the 1Context wiki API listener to \(host):\(port): \(message)"
    case .memoryUnavailable(let message):
      return message.isEmpty ? "Memory protocol is unavailable" : message
    }
  }
}
