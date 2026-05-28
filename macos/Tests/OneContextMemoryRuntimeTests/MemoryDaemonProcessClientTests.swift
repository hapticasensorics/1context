import Foundation
import XCTest
import OneContextMemoryRuntime
import OneContextPlatform

final class MemoryDaemonProcessClientTests: XCTestCase {
  func testDiscoversExecutableFromEnvironmentOverride() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let executable = root.appendingPathComponent("onecontext-memoryd")
    try "#!/bin/sh\nexit 0\n".write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)

    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root),
      environment: ["ONECONTEXT_MEMORYD_BIN": executable.path]
    )

    XCTAssertEqual(client.discoverExecutable()?.path, executable.path)
    XCTAssertTrue(client.status().configured)
  }

  func testStatusPayloadHasStableShapeWhenExecutableIsMissing() {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root),
      environment: ["ONECONTEXT_MEMORYD_BIN": root.appendingPathComponent("missing-memoryd").path]
    )

    let snapshot = client.status()
    XCTAssertFalse(snapshot.running)
    XCTAssertTrue(snapshot.statusPath.hasSuffix("memoryd-status.json"))
    XCTAssertTrue(snapshot.cursorPath.hasSuffix("memory-db/cursors/local-source-cursors.json"))
    XCTAssertNil(snapshot.payload["trace_path"])
    XCTAssertNotNil(snapshot.status["status"])
    XCTAssertEqual(snapshot.payload["surface"] as? String, "memory_daemon_status")
  }

  func testViewportUsesRustProtocolProcess() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let argsFile = root.appendingPathComponent("memoryd-args.txt")
    let requestFile = root.appendingPathComponent("memoryd-request.json")
    let executable = root.appendingPathComponent("onecontext-memoryd")
    let script = """
    #!/bin/sh
    printf '%s\\n' "$@" > \(shellQuoted(argsFile.path))
    cat > \(shellQuoted(requestFile.path))
    cat <<'JSON'
    {"schema_version":1,"surface":"memory_viewport","protocol":"memory.queryViewport.v1","status":"ok","provider":"onecontext-memoryd","limit":2,"source":"codex.local_sessions","object_count":1,"shown_object_count":1,"sources":["codex.local_sessions"],"objects":[{"source":"codex.local_sessions","kind":"codex_message","display_text":"from rust protocol"}]}
    JSON
    """
    try script.write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)

    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root),
      environment: ["ONECONTEXT_MEMORYD_BIN": executable.path]
    )

    let payload = try client.queryViewport(MemoryViewportQuery(limit: 2, source: "codex.local_sessions"))
    let objects = try XCTUnwrap(payload["objects"] as? [[String: Any]])
    let args = try String(contentsOf: argsFile, encoding: .utf8)
    let request = try XCTUnwrap(jsonObject(at: requestFile))
    let params = try XCTUnwrap(request["params"] as? [String: Any])
    let time = try XCTUnwrap(params["time"] as? [String: Any])
    let filters = try XCTUnwrap(params["filters"] as? [String: Any])
    let pagination = try XCTUnwrap(params["pagination"] as? [String: Any])

    XCTAssertEqual(payload["surface"] as? String, "memory_viewport")
    XCTAssertEqual(payload["protocol"] as? String, "memory.queryViewport.v1")
    XCTAssertEqual(payload["provider"] as? String, "onecontext-memoryd")
    XCTAssertEqual(payload["object_count"] as? Int, 1)
    XCTAssertEqual(objects.first?["display_text"] as? String, "from rust protocol")
    let argv = args.split(separator: "\n").map(String.init)
    XCTAssertEqual(argv.first, "protocol")
    XCTAssertEqual(argv.dropFirst().first, "memory.queryViewport")
    XCTAssertTrue(args.contains("--request-json"))
    XCTAssertTrue(args.contains("-"))
    XCTAssertEqual(request["schema_version"] as? Int, 1)
    XCTAssertEqual(request["method"] as? String, "memory.queryViewport")
    XCTAssertTrue((request["request_id"] as? String)?.hasPrefix("swift-") == true)
    XCTAssertEqual(params["user_id"] as? String, "00000000-0000-0000-0000-000000000001")
    XCTAssertNotNil(time["start"] as? String)
    XCTAssertNotNil(time["end"] as? String)
    XCTAssertEqual(filters["source_types"] as? [String], ["codex.local_sessions"])
    XCTAssertEqual(pagination["limit"] as? Int, 2)
  }

  func testAdditionalReadMethodsUseRustProtocolProcess() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let argsFile = root.appendingPathComponent("memoryd-args.txt")
    let executable = root.appendingPathComponent("onecontext-memoryd")
    let script = """
    #!/bin/sh
    printf '%s\\n' "$*" >> \(shellQuoted(argsFile.path))
    cat >/dev/null
    case "$2" in
      memory.hydrateObjects)
        printf '%s\\n' '{"schema_version":1,"surface":"memory_object_hydration","protocol":"memory.hydrateObjects.v1","status":"ok","provider":"onecontext-memoryd","object":{"object_id":"object-123"}}'
        ;;
      memory.queryDensity)
        printf '%s\\n' '{"schema_version":1,"surface":"memory_density","protocol":"memory.queryDensity.v1","status":"ok","provider":"onecontext-memoryd","buckets":[{"object_count":3}]}'
        ;;
      memory.queryEdges)
        printf '%s\\n' '{"schema_version":1,"surface":"memory_edges","protocol":"memory.queryEdges.v1","status":"ok","provider":"onecontext-memoryd","edges":[]}'
        ;;
      memory.searchText)
        printf '%s\\n' '{"schema_version":1,"surface":"memory_search","protocol":"memory.searchText.v1","status":"ok","provider":"onecontext-memoryd","objects":[]}'
        ;;
    esac
    """
    try script.write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)

    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root),
      environment: ["ONECONTEXT_MEMORYD_BIN": executable.path]
    )

    XCTAssertEqual(
      try client.hydrateObjects(MemoryObjectHydrationQuery(objectID: "object-123"))["protocol"] as? String,
      "memory.hydrateObjects.v1"
    )
    XCTAssertEqual(
      try client.queryDensity(MemoryDensityQuery(startTime: "2026-05-24T00:00:00Z", endTime: "2026-05-24T01:00:00Z", sources: ["codex.local_sessions"]))["protocol"] as? String,
      "memory.queryDensity.v1"
    )
    XCTAssertEqual(
      try client.queryEdges(MemoryEdgesQuery(objectID: "object-123"))["protocol"] as? String,
      "memory.queryEdges.v1"
    )
    XCTAssertEqual(
      try client.searchText(MemorySearchTextQuery(query: "ship it"))["protocol"] as? String,
      "memory.searchText.v1"
    )

    let args = try String(contentsOf: argsFile, encoding: .utf8)
    XCTAssertTrue(args.contains("protocol memory.hydrateObjects --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.queryDensity --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.queryEdges --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.searchText --request-json -"))
  }

  func testViewportDoesNotUseLegacyPSQLOrJSONLProviders() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let marker = root.appendingPathComponent("psql-was-called.txt")
    let psql = root.appendingPathComponent("psql")
    try "#!/bin/sh\ntouch \(shellQuoted(marker.path))\nexit 1\n"
      .write(to: psql, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: psql.path)
    let memoryd = root.appendingPathComponent("onecontext-memoryd")
    try "#!/bin/sh\ncat >/dev/null\nprintf 'protocol failed' >&2\nexit 7\n"
      .write(to: memoryd, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: memoryd.path)

    let paths = runtimePaths(root: root)
    let legacyTraceFile = paths.contextEngineDirectory
      .appendingPathComponent("memory-db/traces/local-source-records.jsonl")
    try FileManager.default.createDirectory(at: legacyTraceFile.deletingLastPathComponent(), withIntermediateDirectories: true)
    try #"{"source":"codex.local_sessions","record":{"connector_key":"codex.local_sessions"}}"#
      .write(to: legacyTraceFile, atomically: true, encoding: .utf8)

    let client = MemoryDaemonProcessClient(
      runtimePaths: paths,
      environment: [
        "ONECONTEXT_MEMORY_TRACE_PROVIDER": "sql",
        "ONECONTEXT_MEMORY_DB_URL": "postgresql://localhost/onecontext",
        "ONECONTEXT_MEMORY_PSQL_BIN": psql.path,
        "ONECONTEXT_MEMORYD_BIN": memoryd.path
      ]
    )

    XCTAssertThrowsError(
      try client.queryViewport(MemoryViewportQuery(limit: 2, source: "codex.local_sessions"))
    ) { error in
      XCTAssertTrue(error.localizedDescription.contains("protocol failed"))
    }
    XCTAssertFalse(FileManager.default.fileExists(atPath: marker.path))
  }

  func testReadMethodsThrowWhenMemorydIsMissing() {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root),
      environment: ["ONECONTEXT_MEMORYD_BIN": root.appendingPathComponent("missing-memoryd").path]
    )

    XCTAssertThrowsError(try client.hydrateObjects(MemoryObjectHydrationQuery(objectID: "object-123"))) { error in
      XCTAssertTrue(error.localizedDescription.contains("onecontext-memoryd executable not found"))
    }
  }

  private func runtimePaths(root: URL, identity: OneContextAppIdentity = .official) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Library/Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Library/Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Library/Caches/1Context", isDirectory: true),
      identity: identity
    )
  }

  private func temporaryRoot() -> URL {
    FileManager.default.temporaryDirectory
      .appendingPathComponent("onecontext-memory-runtime-tests-\(UUID().uuidString)", isDirectory: true)
  }

  private func shellQuoted(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }

  private func jsonObject(at url: URL) throws -> [String: Any]? {
    let data = try Data(contentsOf: url)
    return try JSONSerialization.jsonObject(with: data) as? [String: Any]
  }
}
