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
    let executable = try writeScript(root: root, name: "memory-core", body: "printf '{\"ok\":true}\\n'\n")
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
    let executable = try writeScript(root: root, name: "memory-core", body: "printf '{\"ok\":true}\\n'\n")
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
    if [ "$1" = "status" ]; then printf '{"status":"ok"}\\n'; exit 0; fi
    printf '{"args":["%s","%s"]}\\n' "$1" "$2"
    """)
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    let result = try adapter.run(arguments: ["wiki", "list"])

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

    XCTAssertThrowsError(try adapter.run(arguments: ["status"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .invalidJSON)
    }
  }

  func testRunTimesOutCleanly() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "sleep 2\nprintf '{\"late\":true}\\n'\n")
    let paths = paths(root: root)
    try writeConfig(
      MemoryCoreConfig(enabled: true, executable: executable.path, defaultTimeoutSeconds: 0.1),
      to: paths.configFile
    )
    let adapter = MemoryCoreAdapter(paths: paths)

    XCTAssertThrowsError(try adapter.run(arguments: ["status"])) { error in
      guard case MemoryCoreError.timeout = error else {
        return XCTFail("expected timeout, got \(error)")
      }
    }
  }

  func testRunRejectsDisallowedCommand() throws {
    let root = try temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try writeScript(root: root, name: "memory-core", body: "printf '{\"ok\":true}\\n'\n")
    let adapter = adapter(root: root)
    _ = try adapter.configure(executable: executable.path)

    XCTAssertThrowsError(try adapter.run(arguments: ["hire"])) { error in
      XCTAssertEqual(error as? MemoryCoreError, .commandNotAllowed("hire"))
    }
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

  private func fileMode(_ url: URL) -> String {
    guard let attributes = try? FileManager.default.attributesOfItem(atPath: url.path),
      let mode = attributes[.posixPermissions] as? NSNumber
    else {
      return "missing"
    }
    return String(mode.intValue, radix: 8)
  }
}
