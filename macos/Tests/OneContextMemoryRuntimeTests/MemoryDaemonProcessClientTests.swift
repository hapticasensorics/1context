import Foundation
import Darwin
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
    XCTAssertEqual(filters["source_keys"] as? [String], ["codex.local_sessions"])
    XCTAssertEqual(pagination["limit"] as? Int, 2)
  }

  func testAdditionalReadMethodsUseRustProtocolProcess() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let argsFile = root.appendingPathComponent("memoryd-args.txt")
    let requestsFile = root.appendingPathComponent("memoryd-requests.jsonl")
    let executable = root.appendingPathComponent("onecontext-memoryd")
    let script = """
    #!/bin/sh
    printf '%s\\n' "$*" >> \(shellQuoted(argsFile.path))
    cat >> \(shellQuoted(requestsFile.path))
    printf '\\n' >> \(shellQuoted(requestsFile.path))
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
    let requests = try String(contentsOf: requestsFile, encoding: .utf8)
    XCTAssertTrue(args.contains("protocol memory.hydrateObjects --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.queryDensity --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.queryEdges --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.searchText --request-json -"))
    XCTAssertTrue(requests.contains(#""method":"memory.queryDensity""#))
    XCTAssertFalse(requests.contains("source_keys"))
    XCTAssertFalse(requests.contains("source_types"))
  }

  func testStorageReadinessMethodsUseRustProtocolProcess() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let argsFile = root.appendingPathComponent("memoryd-args.txt")
    let requestsFile = root.appendingPathComponent("memoryd-requests.jsonl")
    let executable = root.appendingPathComponent("onecontext-memoryd")
    let script = """
    #!/bin/sh
    printf '%s\\n' "$*" >> \(shellQuoted(argsFile.path))
    request="$(cat)"
    printf '%s\\n' "$request" >> \(shellQuoted(requestsFile.path))
    method="$(printf '%s' "$request" | /usr/bin/python3 -c 'import json,sys; print(json.load(sys.stdin)["method"])')"
    printf '{"schema_version":1,"surface":"perception_db","protocol":"%s.v1","status":"ok","provider":"onecontext-memoryd"}\\n' "$method"
    """
    try script.write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)

    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root),
      environment: ["ONECONTEXT_MEMORYD_BIN": executable.path]
    )

    XCTAssertEqual(try client.storageHealth(MemoryStorageHealthQuery(reason: "launch"))["protocol"] as? String, "memory.storageHealth.v1")
    XCTAssertEqual(try client.ensureStorageReady(MemoryEnsureStorageReadyQuery(reason: "launch", repair: true))["protocol"] as? String, "memory.ensureStorageReady.v1")
    XCTAssertEqual(try client.ensureRecentBackfill(MemoryEnsureRecentBackfillQuery(reason: "launch", windowHours: 24, blockUntilReady: true, minDensityBuckets: 2))["protocol"] as? String, "memory.ensureRecentBackfill.v1")
    XCTAssertEqual(try client.repairStorage(MemoryRepairStorageQuery(reason: "manual"))["protocol"] as? String, "memory.repairStorage.v1")
    XCTAssertEqual(try client.stopStorage(MemoryStorageLifecycleQuery(reason: "manual"))["protocol"] as? String, "memory.stopStorage.v1")
    XCTAssertEqual(try client.restartStorage(MemoryStorageLifecycleQuery(reason: "manual"))["protocol"] as? String, "memory.restartStorage.v1")
    XCTAssertEqual(try client.storageDiagnostics(MemoryStorageDiagnosticsQuery(includeLogs: true, redact: false))["protocol"] as? String, "memory.storageDiagnostics.v1")

    let args = try String(contentsOf: argsFile, encoding: .utf8)
    XCTAssertTrue(args.contains("protocol memory.storageHealth --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.ensureStorageReady --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.ensureRecentBackfill --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.repairStorage --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.stopStorage --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.restartStorage --request-json -"))
    XCTAssertTrue(args.contains("protocol memory.storageDiagnostics --request-json -"))

    let requests = try String(contentsOf: requestsFile, encoding: .utf8)
      .split(separator: "\n")
      .compactMap { try? JSONSerialization.jsonObject(with: Data($0.utf8)) as? [String: Any] }
    let ensureReady = try XCTUnwrap(requests.first { $0["method"] as? String == "memory.ensureStorageReady" }?["params"] as? [String: Any])
    let recentBackfill = try XCTUnwrap(requests.first { $0["method"] as? String == "memory.ensureRecentBackfill" }?["params"] as? [String: Any])
    let diagnostics = try XCTUnwrap(requests.first { $0["method"] as? String == "memory.storageDiagnostics" }?["params"] as? [String: Any])
    XCTAssertEqual(ensureReady["repair"] as? Bool, true)
    XCTAssertEqual(recentBackfill["window_hours"] as? Int, 24)
    XCTAssertEqual(recentBackfill["block_until_ready"] as? Bool, true)
    XCTAssertEqual(recentBackfill["min_density_buckets"] as? Int, 2)
    XCTAssertEqual(diagnostics["include_logs"] as? Bool, true)
    XCTAssertEqual(diagnostics["redact"] as? Bool, false)
  }

  func testManagedStorageIsDefaultWithoutExplicitDatabaseURL() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let envFile = root.appendingPathComponent("memoryd-env.txt")
    let executable = root.appendingPathComponent("onecontext-memoryd")
    let script = """
    #!/bin/sh
    printf '%s\\n%s\\n' "$ONECONTEXT_STORAGE_BACKEND" "$ONECONTEXT_MEMORY_DB_URL" > \(shellQuoted(envFile.path))
    cat >/dev/null
    printf '%s\\n' '{"schema_version":1,"surface":"perception_db","protocol":"memory.storageHealth.v1","status":"ok","provider":"onecontext-memoryd"}'
    """
    try script.write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)

    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root, identity: .dev),
      environment: ["ONECONTEXT_MEMORYD_BIN": executable.path]
    )

    _ = try client.storageHealth()
    let lines = try String(contentsOf: envFile, encoding: .utf8)
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)

    XCTAssertEqual(lines.first, "managed_postgres")
    XCTAssertEqual(lines.dropFirst().first, "")
  }

  func testAppSpecificExplicitDatabaseURLDoesNotOverrideManagedStorageByDefault() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let envFile = root.appendingPathComponent("memoryd-env.txt")
    let executable = root.appendingPathComponent("onecontext-memoryd")
    let script = """
    #!/bin/sh
    printf '%s\\n%s\\n' "$ONECONTEXT_STORAGE_BACKEND" "$ONECONTEXT_MEMORY_DB_URL" > \(shellQuoted(envFile.path))
    cat >/dev/null
    printf '%s\\n' '{"schema_version":1,"surface":"perception_db","protocol":"memory.storageHealth.v1","status":"ok","provider":"onecontext-memoryd"}'
    """
    try script.write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)

    let explicit = "postgres://example.test/dev-memory"
    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root, identity: .dev),
      environment: [
        "ONECONTEXT_MEMORYD_BIN": executable.path,
        "ONECONTEXT_MEMORY_DB_URL": explicit
      ]
    )

    _ = try client.storageHealth()
    let lines = try String(contentsOf: envFile, encoding: .utf8)
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)

    XCTAssertEqual(lines.first, "managed_postgres")
    XCTAssertEqual(lines.dropFirst().first, "")
  }

  func testExplicitAllowFlagSelectsExternalStorageBackend() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let envFile = root.appendingPathComponent("memoryd-env.txt")
    let executable = root.appendingPathComponent("onecontext-memoryd")
    let script = """
    #!/bin/sh
    printf '%s\\n%s\\n' "$ONECONTEXT_STORAGE_BACKEND" "$ONECONTEXT_MEMORY_DB_URL" > \(shellQuoted(envFile.path))
    cat >/dev/null
    printf '%s\\n' '{"schema_version":1,"surface":"perception_db","protocol":"memory.storageHealth.v1","status":"ok","provider":"onecontext-memoryd"}'
    """
    try script.write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)

    let explicit = "postgres://example.test/dev-memory"
    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root, identity: .dev),
      environment: [
        "ONECONTEXT_MEMORYD_BIN": executable.path,
        "ONECONTEXT_ALLOW_EXTERNAL_POSTGRES": "1",
        "ONECONTEXT_MEMORY_DB_URL": explicit
      ]
    )

    _ = try client.storageHealth()
    let lines = try String(contentsOf: envFile, encoding: .utf8)
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)

    XCTAssertEqual(lines.first, "external_postgres")
    XCTAssertEqual(lines.dropFirst().first, explicit)
  }

  func testGenericDatabaseURLDoesNotOverrideManagedStorageByDefault() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let envFile = root.appendingPathComponent("memoryd-env.txt")
    let executable = root.appendingPathComponent("onecontext-memoryd")
    let script = """
    #!/bin/sh
    printf '%s\\n%s\\n' "$ONECONTEXT_STORAGE_BACKEND" "$ONECONTEXT_MEMORY_DB_URL" > \(shellQuoted(envFile.path))
    cat >/dev/null
    printf '%s\\n' '{"schema_version":1,"surface":"perception_db","protocol":"memory.storageHealth.v1","status":"ok","provider":"onecontext-memoryd"}'
    """
    try script.write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)

    let client = MemoryDaemonProcessClient(
      runtimePaths: runtimePaths(root: root, identity: .dev),
      environment: [
        "ONECONTEXT_MEMORYD_BIN": executable.path,
        "DATABASE_URL": "postgres://example.test/dev-memory"
      ]
    )

    _ = try client.storageHealth()
    let lines = try String(contentsOf: envFile, encoding: .utf8)
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)

    XCTAssertEqual(lines.first, "managed_postgres")
    XCTAssertEqual(lines.dropFirst().first, "")
  }

  func testStartIgnoresUntrustedLegacyPIDFile() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let executable = root.appendingPathComponent("onecontext-memoryd")
    try memorydSleepScript()
      .write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
    let paths = runtimePaths(root: root)
    try FileManager.default.createDirectory(at: paths.runDirectory, withIntermediateDirectories: true)
    let unrelated = try startSleepProcess()
    defer { terminateProcess(unrelated) }
    let pidFile = paths.runDirectory.appendingPathComponent("memoryd.pid")
    try "\(unrelated.processIdentifier)\n".write(to: pidFile, atomically: true, encoding: .utf8)

    let client = MemoryDaemonProcessClient(
      runtimePaths: paths,
      environment: ["ONECONTEXT_MEMORYD_BIN": executable.path]
    )

    client.startIfAvailable { _ in }
    usleep(100_000)
    defer { client.stop() }

    XCTAssertTrue(unrelated.isRunning)
    XCTAssertFalse(FileManager.default.fileExists(atPath: pidFile.path))
  }

  func testStartIgnoresPIDFileWhoseLiveProcessPathDoesNotMatch() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    let executable = root.appendingPathComponent("onecontext-memoryd")
    try memorydSleepScript()
      .write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
    let paths = runtimePaths(root: root)
    try FileManager.default.createDirectory(at: paths.runDirectory, withIntermediateDirectories: true)
    let unrelated = try startSleepProcess()
    defer { terminateProcess(unrelated) }
    let pidFile = paths.runDirectory.appendingPathComponent("memoryd.pid")
    let record: [String: Any] = [
      "pid": Int(unrelated.processIdentifier),
      "executable": executable.path,
      "started_at_ms": 1,
      "kind": "onecontext-memoryd"
    ]
    try JSONSerialization.data(withJSONObject: record, options: [.prettyPrinted])
      .write(to: pidFile, options: [.atomic])

    let client = MemoryDaemonProcessClient(
      runtimePaths: paths,
      environment: ["ONECONTEXT_MEMORYD_BIN": executable.path]
    )

    client.startIfAvailable { _ in }
    usleep(100_000)
    defer { client.stop() }

    XCTAssertTrue(unrelated.isRunning)
    XCTAssertFalse(FileManager.default.fileExists(atPath: pidFile.path))
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

  private func memorydSleepScript() -> String {
    """
    #!/bin/sh
    if [ "$1" = "protocol" ]; then
      cat >/dev/null
      printf '%s\\n' '{"schema_version":1,"surface":"perception_db","protocol":"memory.stopStorage.v1","status":"ok","provider":"onecontext-memoryd"}'
      exit 0
    fi
    sleep 2
    """
  }

  private func jsonObject(at url: URL) throws -> [String: Any]? {
    let data = try Data(contentsOf: url)
    return try JSONSerialization.jsonObject(with: data) as? [String: Any]
  }

  private func startSleepProcess() throws -> Process {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/sleep")
    process.arguments = ["5"]
    try process.run()
    return process
  }

  private func terminateProcess(_ process: Process) {
    guard process.isRunning else { return }
    process.terminate()
    for _ in 0..<20 {
      usleep(50_000)
      if !process.isRunning {
        return
      }
    }
    kill(process.processIdentifier, SIGKILL)
  }
}
