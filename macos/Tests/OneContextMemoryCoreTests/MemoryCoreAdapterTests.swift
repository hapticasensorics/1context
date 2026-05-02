import Darwin
import Foundation
import XCTest
@testable import OneContextMemoryCore

final class MemoryCoreAdapterTests: XCTestCase {
  func testDefaultStatusIsNotConfigured() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let adapter = adapter(root: root)

    let status = adapter.status()

    XCTAssertFalse(status.configured)
    XCTAssertFalse(status.enabled)
    XCTAssertEqual(status.health, .notConfigured)
  }

  func testConfigureWritesPrivateConfig() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "printf '{\"status\":\"ok\",\"schema_version\":1}\\n'\n")
    let paths = paths(root: root)
    let adapter = MemoryCoreAdapter(paths: paths)

    let status = try adapter.configure(executable: executable.path)

    XCTAssertTrue(status.configured)
    XCTAssertTrue(status.enabled)
    XCTAssertEqual(status.executable, executable.path)
    XCTAssertEqual(fileMode(paths.directory), "700")
    XCTAssertEqual(fileMode(paths.configFile), "600")
    XCTAssertEqual(fileMode(paths.stateFile), "600")
  }

  func testConfigureClearDisablesMemoryCore() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "printf '{\"status\":\"ok\",\"schema_version\":1}\\n'\n")
    let adapter = adapter(root: root)

    _ = try adapter.configure(executable: executable.path)
    let status = try adapter.clear()

    XCTAssertFalse(status.configured)
    XCTAssertFalse(status.enabled)
    XCTAssertEqual(adapter.config(), MemoryCoreConfig())
  }

  func testStatusHandlesMissingExecutable() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let adapter = adapter(root: root)

    _ = try adapter.configure(executable: root.appendingPathComponent("missing").path)
    let status = adapter.status()

    XCTAssertEqual(status.health, .degraded)
    XCTAssertTrue(status.lastError?.contains("missing") == true)
  }

  func testRunRefusesWhenDisabledOrNotConfigured() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let adapter = adapter(root: root)

    XCTAssertThrowsError(try adapter.run(arguments: ["status"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .notConfigured)
    }
  }

  func testRunCapturesJSONStdout() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: """
    if [ "$1" = "status" ]; then printf '{"status":"ok","schema_version":1}\\n'; exit 0; fi
    printf '{"status":"ok","schema_version":1,"args":["%s","%s"]}\\n' "$1" "$2"
    """)
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    let result = try adapter.run(arguments: ["wiki", "list", "--json"])

    XCTAssertEqual(result.exitCode, 0)
    XCTAssertTrue(result.stdout.contains("\"wiki\""))
    XCTAssertTrue(MemoryCoreAdapter.isJSON(result.stdout))
  }

  func testRunHandlesInvalidJSON() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "printf 'not json\\n'\n")
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    XCTAssertThrowsError(try adapter.run(arguments: ["status", "--json"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .invalidJSON)
    }
  }

  func testRunTimesOutCleanly() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "sleep 2\nprintf '{\"status\":\"ok\",\"schema_version\":1,\"late\":true}\\n'\n")
    let paths = paths(root: root)
    try writeConfig(
      MemoryCoreConfig(enabled: true, executable: executable.path, defaultTimeoutSeconds: 0.1),
      to: paths.configFile
    )
    let adapter = MemoryCoreAdapter(paths: paths)

    XCTAssertThrowsError(try adapter.run(arguments: ["status", "--json"])) { error in
      guard case MemoryCoreError.timeout = error else {
        return XCTFail("expected timeout, got \(error)")
      }
    }
  }

  func testRunRejectsDisallowedCommand() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "printf '{\"status\":\"ok\",\"schema_version\":1}\\n'\n")
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    XCTAssertThrowsError(try adapter.run(arguments: ["hire"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .commandNotAllowed("hire"))
    }
  }

  func testRunRejectsUnsafeSubcommandsWithinAllowedFamilies() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "printf '{\"status\":\"ok\",\"schema_version\":1}\\n'\n")
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    XCTAssertThrowsError(try adapter.run(arguments: ["wiki", "apply", "--json"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .commandNotAllowed("wiki apply --json"))
    }
    XCTAssertThrowsError(try adapter.run(arguments: ["memory", "migrations", "run", "--json"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .commandNotAllowed("memory migrations run --json"))
    }
    XCTAssertThrowsError(try adapter.run(arguments: ["memory", "cycles", "show", "../secret", "--json"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .commandNotAllowed("memory cycles show ../secret --json"))
    }
    XCTAssertThrowsError(try adapter.run(arguments: ["memory", "cycles", "show", ".", "--json"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .commandNotAllowed("memory cycles show . --json"))
    }
    XCTAssertThrowsError(try adapter.run(arguments: ["memory", "replay-dry-run", "--json"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .commandNotAllowed("memory replay-dry-run --json"))
    }
    XCTAssertThrowsError(try adapter.run(arguments: [
      "memory", "replay-dry-run",
      "--start", "2026-04-27T00:00:00Z",
      "--end", "2026-04-27T01:00:00Z",
      "--replay-run-id", ".",
      "--json"
    ])) { error in
      XCTAssertEqual(
        error as? MemoryCoreError,
        .commandNotAllowed("memory replay-dry-run --start 2026-04-27T00:00:00Z --end 2026-04-27T01:00:00Z --replay-run-id . --json")
      )
    }
  }

  func testRunAllowsBoundedParameterizedCommands() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: """
    if [ "$1" = "status" ]; then printf '{"status":"ok","schema_version":1}\\n'; exit 0; fi
    printf '{"status":"ok","schema_version":1,"args":["%s","%s","%s","%s"]}\\n' "$1" "$2" "$3" "$4"
    """)
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    XCTAssertNoThrow(try adapter.run(arguments: [
      "memory", "replay-dry-run",
      "--start", "2026-04-27T00:00:00Z",
      "--end", "2026-04-27T01:00:00Z",
      "--sources", "codex,claude-code",
      "--replay-run-id", "replay-test-1",
      "--json"
    ]))
    XCTAssertNoThrow(try adapter.run(arguments: ["memory", "cycles", "show", "cycle:test-1", "--json"]))
    XCTAssertNoThrow(try adapter.run(arguments: ["memory", "cycles", "validate", "cycle:test-1", "--json"]))
    XCTAssertNoThrow(try adapter.run(arguments: ["wiki", "render", "--no-evidence", "--json"]))
  }

  func testDoctorRequiresStatusJSONContract() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: """
    if [ "$1" = "--version" ]; then printf 'fake 1.0.0\\n'; exit 0; fi
    printf 'not json\\n'
    """)
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    let status = adapter.doctor()

    XCTAssertEqual(status.health, .degraded)
    XCTAssertTrue(status.lastError?.contains("invalid JSON") == true)
  }

  func testRunRejectsJSONOutsideContract() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "printf '{\"status\":\"error\",\"schema_version\":1}\\n'\n")
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    XCTAssertThrowsError(try adapter.run(arguments: ["status", "--json"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .invalidContract("Memory core JSON must include status: ok"))
    }
  }

  func testRunRedactsProcessFailureStderr() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: """
    printf 'failed at \(root.path)/secret.txt\\n' >&2
    exit 2
    """)
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    XCTAssertThrowsError(try adapter.run(arguments: ["status", "--json"])) { error in
      guard case MemoryCoreError.processFailed(let message) = error else {
        return XCTFail("expected process failure, got \(error)")
      }
      XCTAssertFalse(message.contains(root.path))
    }
  }

  func testRunPreservesNonzeroContractErrorFromStdout() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: """
    if [ "$1" = "status" ]; then printf '{"status":"ok","schema_version":1}\\n'; exit 0; fi
    printf '{"status":"error","schema_version":1,"error":{"code":"not_ready","message":"initialize storage first"}}\\n'
    exit 2
    """)
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    XCTAssertThrowsError(try adapter.run(arguments: ["wiki", "list", "--json"])) { error in
      guard case MemoryCoreError.processFailed(let message) = error else {
        return XCTFail("expected process failure, got \(error)")
      }
      XCTAssertTrue(message.contains("not_ready"))
      XCTAssertTrue(message.contains("initialize storage first"))
    }
  }

  func testRunnerPassesOneContextPathOverrides() throws {
    let runner = MemoryCoreProcessRunner(environment: [
      "HOME": "/Users/example",
      "PATH": "/usr/bin:/bin",
      "ONECONTEXT_APP_SUPPORT_DIR": "/tmp/app-support",
      "ONECONTEXT_USER_CONTENT_DIR": "/tmp/user-content",
      "ONECONTEXT_LOG_DIR": "/tmp/logs",
      "ONECONTEXT_CACHE_DIR": "/tmp/cache",
      "ONECONTEXT_MEMORY_CORE_DIR": "/tmp/memory-core",
      "ONECONTEXT_MEMORY_CORE_LOG_PATH": "/tmp/memory-core.log",
      "ONECONTEXT_UNRELATED": "drop-me"
    ])
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: """
    printf '{"status":"ok","schema_version":1,"app_support":"%s","unrelated":"%s"}\\n' "${ONECONTEXT_APP_SUPPORT_DIR:-}" "${ONECONTEXT_UNRELATED:-}"
    """)

    let result = try runner.run(executable: executable.path, arguments: ["status", "--json"], stdinData: Data(), timeout: 2)

    XCTAssertTrue(result.stdout.contains("/tmp/app-support"))
    XCTAssertTrue(result.stdout.contains("\"unrelated\":\"\""))
  }

  func testRenderStatusRedactsPaths() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let adapter = adapter(root: root)
    let status = MemoryCoreStatus(
      configured: true,
      enabled: true,
      executable: root.appendingPathComponent("memory-core").path,
      health: .degraded,
      lastError: "missing \(root.path)",
      paths: paths(root: root)
    )

    let rendered = adapter.renderStatus(status) { $0.replacingOccurrences(of: root.path, with: "~") }

    XCTAssertFalse(rendered.contains(root.path))
    XCTAssertTrue(rendered.contains("~/memory-core"))
  }

  func testBundleFingerprintChangesWhenRendererFilesChange() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let bundle = try writeMinimalBundle(root: root, marker: "one")
    let setup = MemoryCoreSetup(paths: paths(root: root), environment: ["ONECONTEXT_MEMORY_CORE_BUNDLE_DIR": bundle.path])

    let first = try setup.bundleFingerprint(bundle)
    try "two\n".write(to: bundle.appendingPathComponent("wiki-engine/tools/render-to-dir.mjs"), atomically: true, encoding: .utf8)
    let second = try setup.bundleFingerprint(bundle)

    XCTAssertNotEqual(first, second)
  }

  func testEnsureReadyReinstallsBundledCoreWhenFingerprintChangesWithoutVersionChange() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let bundle = try writeMinimalBundle(root: root, marker: "one")
    try writeFakePython(root: root)
    let setup = MemoryCoreSetup(
      paths: paths(root: root),
      environment: ["ONECONTEXT_MEMORY_CORE_BUNDLE_DIR": bundle.path]
    )

    _ = try setup.ensureReady(validateContract: false)
    let installedRenderer = setup.coreDirectory.appendingPathComponent("wiki-engine/tools/render-to-dir.mjs")
    XCTAssertEqual(try String(contentsOf: installedRenderer, encoding: .utf8), "one\n")

    try "two\n".write(to: bundle.appendingPathComponent("wiki-engine/tools/render-to-dir.mjs"), atomically: true, encoding: .utf8)
    _ = try setup.ensureReady(validateContract: false)

    XCTAssertEqual(try String(contentsOf: installedRenderer, encoding: .utf8), "two\n")
  }

  private func adapter(root: URL) -> MemoryCoreAdapter {
    MemoryCoreAdapter(paths: paths(root: root))
  }

  private func paths(root: URL) -> MemoryCorePaths {
    MemoryCorePaths(
      directory: root.appendingPathComponent("Application Support/1Context/memory-core", isDirectory: true),
      logFile: root.appendingPathComponent("Logs/1Context/memory-core.log")
    )
  }

  private func temporaryRoot() throws -> URL {
    let root = FileManager.default.temporaryDirectory
      .appendingPathComponent("1ctx-memory-core-\(UUID().uuidString)", isDirectory: true)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    return root
  }

  private func writeScript(root: URL, name: String, body: String) throws -> URL {
    let url = root.appendingPathComponent(name)
    try "#!/bin/sh\n\(body)".write(to: url, atomically: true, encoding: .utf8)
    chmod(url.path, 0o700)
    return url
  }

  private func writeConfig(_ config: MemoryCoreConfig, to url: URL) throws {
    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    try encoder.encode(config).write(to: url)
    chmod(url.path, 0o600)
  }

  private func writeMinimalBundle(root: URL, marker: String) throws -> URL {
    let bundle = root.appendingPathComponent("bundle", isDirectory: true)
    try FileManager.default.createDirectory(at: bundle.appendingPathComponent("bin", isDirectory: true), withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: bundle.appendingPathComponent("src/onectx/wiki", isDirectory: true), withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: bundle.appendingPathComponent("wiki-engine/tools", isDirectory: true), withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: bundle.appendingPathComponent("wiki-engine/theme/css", isDirectory: true), withIntermediateDirectories: true)
    try FileManager.default.createDirectory(at: bundle.appendingPathComponent("wiki/menu", isDirectory: true), withIntermediateDirectories: true)
    try "fixture\n".write(to: bundle.appendingPathComponent("pyproject.toml"), atomically: true, encoding: .utf8)
    try "fixture\n".write(to: bundle.appendingPathComponent("uv.lock"), atomically: true, encoding: .utf8)
    try marker.appending("\n").write(
      to: bundle.appendingPathComponent("wiki-engine/tools/render-to-dir.mjs"),
      atomically: true,
      encoding: .utf8
    )
    try "body {}\n".write(to: bundle.appendingPathComponent("wiki-engine/theme/css/theme.css"), atomically: true, encoding: .utf8)
    try "render\n".write(to: bundle.appendingPathComponent("src/onectx/wiki/render.py"), atomically: true, encoding: .utf8)
    let executable = bundle.appendingPathComponent("bin/1context-memory-core")
    try "#!/bin/sh\nprintf '{\"status\":\"ok\",\"schema_version\":1}\\n'\n".write(to: executable, atomically: true, encoding: .utf8)
    chmod(executable.path, 0o700)
    return bundle
  }

  private func writeFakePython(root: URL) throws {
    let python = paths(root: root).directory.appendingPathComponent("venv/bin/python3")
    try FileManager.default.createDirectory(at: python.deletingLastPathComponent(), withIntermediateDirectories: true)
    try "#!/bin/sh\nexit 0\n".write(to: python, atomically: true, encoding: .utf8)
    chmod(python.path, 0o700)
  }

  private func fileMode(_ url: URL) -> String {
    guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
      let mode = attributes[.posixPermissions] as? NSNumber
    else {
      return "missing"
    }
    return String(mode.intValue, radix: 8)
  }
}
