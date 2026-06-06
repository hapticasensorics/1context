import Foundation
import XCTest
@testable import OneContextAgentRuntime
@testable import OneContextPlatform

final class ContextEngineProcessClientTests: XCTestCase {
  func testUpdateWikiUsesEnvironmentOverrideAndRuntimeRoot() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let argsFile = root.appendingPathComponent("context-engine-args.txt")
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '%s\\n' "$@" > \(shellQuoted(argsFile.path))
      cat <<'JSON'
      {"schema_version":1,"status":"planned","surface":"context_engine.update_wiki","run_id":"manual-run","phase_count":7}
      JSON
      """
    )
    let paths = testRuntimePaths(root: root)
    let client = ContextEngineProcessClient(
      runtimePaths: paths,
      environment: ["ONECONTEXT_CONTEXT_ENGINE_BIN": executable.path]
    )

    let payload = try client.updateWiki(
      runID: "manual-run",
      trigger: "refresh-button",
      executeAgents: true,
      maxConcurrent: 5,
      sourceWindowDays: 3,
      mode: .recentFirst,
      timeoutSeconds: 30
    )
    let args = try String(contentsOf: argsFile, encoding: .utf8)

    XCTAssertEqual(payload["surface"] as? String, "context_engine.update_wiki")
    XCTAssertEqual(payload["status"] as? String, "planned")
    XCTAssertEqual(payload["run_id"] as? String, "manual-run")
    XCTAssertTrue(args.contains("update-wiki\n--root\n\(paths.userContentDirectory.path)"))
    XCTAssertTrue(args.contains("--run-id\nmanual-run"))
    XCTAssertTrue(args.contains("--trigger\nrefresh-button"))
    XCTAssertTrue(args.contains("--execute-agents"))
    XCTAssertTrue(args.contains("--max-concurrent\n5"))
    XCTAssertTrue(args.contains("--source-window-days\n3"))
    XCTAssertTrue(args.contains("--mode\nrecent-first"))
    XCTAssertTrue(args.hasSuffix("--json\n"))
  }

  func testUpdateWikiCanDisableAgents() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let argsFile = root.appendingPathComponent("context-engine-args.txt")
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '%s\\n' "$@" > \(shellQuoted(argsFile.path))
      printf '%s\\n' '{"schema_version":1,"status":"planned","surface":"context_engine.update_wiki"}'
      """
    )
    let client = ContextEngineProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_CONTEXT_ENGINE_BIN": executable.path]
    )

    _ = try client.updateWiki(trigger: "test", executeAgents: false, timeoutSeconds: 30)
    let args = try String(contentsOf: argsFile, encoding: .utf8)

    XCTAssertTrue(args.contains("--no-agents"))
    XCTAssertFalse(args.contains("--execute-agents"))
  }

  func testUpdateWikiReturnsJsonReceiptOnFailedOperationExit() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      cat <<'JSON'
      {"schema_version":1,"status":"failed","surface":"context_engine.update_wiki","error":{"message":"codex app-server unavailable"}}
      JSON
      exit 1
      """
    )
    let client = ContextEngineProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_CONTEXT_ENGINE_BIN": executable.path]
    )

    let payload = try client.updateWiki(trigger: "test", timeoutSeconds: 30)

    XCTAssertEqual(payload["surface"] as? String, "context_engine.update_wiki")
    XCTAssertEqual(payload["status"] as? String, "failed")
  }

  func testManagedStorageIsDefaultWithoutExplicitDatabaseURL() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let envFile = root.appendingPathComponent("context-engine-env.txt")
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '%s\\n%s\\n' "$ONECONTEXT_STORAGE_BACKEND" "$ONECONTEXT_MEMORY_DB_URL" > \(shellQuoted(envFile.path))
      printf '%s\\n' '{"schema_version":1,"status":"planned","surface":"context_engine.update_wiki"}'
      """
    )
    let client = ContextEngineProcessClient(
      runtimePaths: testRuntimePaths(root: root, identity: .dev),
      environment: ["ONECONTEXT_CONTEXT_ENGINE_BIN": executable.path]
    )

    _ = try client.updateWiki(trigger: "test", timeoutSeconds: 30)
    let lines = try String(contentsOf: envFile, encoding: .utf8)
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)

    XCTAssertEqual(lines.first, "managed_postgres")
    XCTAssertEqual(lines.dropFirst().first, "")
  }

  func testExplicitMemoryDatabaseURLSelectsExternalStorageBackend() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let envFile = root.appendingPathComponent("context-engine-env.txt")
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '%s\\n%s\\n' "$ONECONTEXT_STORAGE_BACKEND" "$ONECONTEXT_MEMORY_DB_URL" > \(shellQuoted(envFile.path))
      printf '%s\\n' '{"schema_version":1,"status":"planned","surface":"context_engine.update_wiki"}'
      """
    )
    let explicit = "postgres://example.test/real"
    let client = ContextEngineProcessClient(
      runtimePaths: testRuntimePaths(root: root, identity: .dev),
      environment: [
        "ONECONTEXT_CONTEXT_ENGINE_BIN": executable.path,
        "ONECONTEXT_MEMORY_DB_URL": explicit
      ]
    )

    _ = try client.updateWiki(trigger: "test", timeoutSeconds: 30)
    let lines = try String(contentsOf: envFile, encoding: .utf8)
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)

    XCTAssertEqual(lines.first, "external_postgres")
    XCTAssertEqual(lines.dropFirst().first, explicit)
  }

  func testSelectedCodexRuntimeIsPassedToContextEngineEnvironment() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let envFile = root.appendingPathComponent("context-engine-env.txt")
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '%s\\n%s\\n%s\\n' "$ONECONTEXT_CODEX_BIN" "$ONECONTEXT_CODEX_RUNTIME_MODE" "$ONECONTEXT_CODEX_PLAINTEXT_MAV2" > \(shellQuoted(envFile.path))
      printf '%s\\n' '{"schema_version":1,"status":"planned","surface":"context_engine.update_wiki"}'
      """
    )
    let paths = testRuntimePaths(root: root)
    let selectedCodex = root.appendingPathComponent("onecontext-codex")
    try "#!/bin/sh\nexit 0\n".write(to: selectedCodex, atomically: true, encoding: .utf8)
    try CodexRuntimeCapability.persist(
      CodexRuntimeCapabilitySnapshot(
        status: .ready,
        mode: .bundledCompatibility,
        selectedBinaryPath: selectedCodex.path,
        userTitle: "Codex Runtime",
        userDetail: "Using bundled 1Context Codex runtime.",
        nativeMultiAgentV2: false,
        plaintextMultiAgentV2: true,
        harnessOnlyAgents: true,
        probeSummary: "test"
      ),
      runtimePaths: paths
    )
    let client = ContextEngineProcessClient(
      runtimePaths: paths,
      environment: ["ONECONTEXT_CONTEXT_ENGINE_BIN": executable.path]
    )

    _ = try client.updateWiki(trigger: "test", timeoutSeconds: 30)
    let lines = try String(contentsOf: envFile, encoding: .utf8)
      .split(separator: "\n", omittingEmptySubsequences: false)
      .map(String.init)

    XCTAssertEqual(lines.first, selectedCodex.path)
    XCTAssertEqual(lines.dropFirst().first, "bundled_compatibility")
    XCTAssertEqual(lines.dropFirst(2).first, "1")
  }

  func testThrowsStructuredErrorFromStderr() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      cat >&2 <<'JSON'
      {"schema_version":1,"status":"error","surface":"context_engine","error":{"code":"missing_root","message":"--root is required"}}
      JSON
      exit 2
      """
    )
    let client = ContextEngineProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_CONTEXT_ENGINE_BIN": executable.path]
    )

    XCTAssertThrowsError(try client.describe(timeoutSeconds: 30)) { error in
      let processError = error as? ContextEngineProcessError
      XCTAssertEqual(processError?.terminationStatus, 2)
      XCTAssertEqual(processError?.message, "--root is required")
      XCTAssertEqual(processError?.errorCode, "missing_root")
      XCTAssertEqual(processError?.structuredPayload?["surface"] as? String, "context_engine")
    }
  }

  private func temporaryRoot() -> URL {
    FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-context-engine-process-client-\(UUID().uuidString)", isDirectory: true)
  }

  private func testRuntimePaths(root: URL, identity: OneContextAppIdentity = .official) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true),
      identity: identity
    )
  }

  private func fakeExecutable(root: URL, body: String) throws -> URL {
    let executable = root.appendingPathComponent("fake-onecontext-context-engine")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    try ("#!/bin/sh\n" + body + "\n").write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
    return executable
  }

  private func shellQuoted(_ value: String) -> String {
    "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
  }
}
