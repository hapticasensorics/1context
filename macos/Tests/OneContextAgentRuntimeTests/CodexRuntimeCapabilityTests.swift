import Foundation
import XCTest
@testable import OneContextAgentRuntime
@testable import OneContextPlatform

final class CodexRuntimeCapabilityTests: XCTestCase {
  func testSelectsInstalledCodexWhenPlaintextMultiAgentV2ConfigWorks() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let installed = try fakeExecutable(root: root, name: "installed-codex")

    let snapshot = CodexRuntimeCapability.probe(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_INSTALLED_CODEX_BIN": installed.path],
      timeoutSeconds: 1,
      runner: runner(plaintextReady: [installed.path])
    )

    XCTAssertEqual(snapshot.status, .ready)
    XCTAssertEqual(snapshot.mode, .installedPlaintextMultiAgentV2)
    XCTAssertEqual(snapshot.selectedBinaryPath, installed.path)
    XCTAssertTrue(snapshot.plaintextMultiAgentV2)
    XCTAssertEqual(snapshot.userDetail, "Using installed Codex with compatible agent tools.")
  }

  func testSelectsBundledCompatibilityRuntimeWhenInstalledCodexLacksPlaintextConfig() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let installed = try fakeExecutable(root: root, name: "installed-codex")
    let bundled = try fakeExecutable(root: root, name: "onecontext-codex")

    let snapshot = CodexRuntimeCapability.probe(
      runtimePaths: testRuntimePaths(root: root),
      environment: [
        "ONECONTEXT_INSTALLED_CODEX_BIN": installed.path,
        "ONECONTEXT_BUNDLED_CODEX_BIN": bundled.path
      ],
      timeoutSeconds: 1,
      runner: runner(plaintextReady: [bundled.path])
    )

    XCTAssertEqual(snapshot.status, .ready)
    XCTAssertEqual(snapshot.mode, .bundledCompatibility)
    XCTAssertEqual(snapshot.selectedBinaryPath, bundled.path)
    XCTAssertEqual(snapshot.userDetail, "Using bundled 1Context Codex runtime.")
  }

  func testFallsBackToHarnessOnlyWhenNoNativeAgentToolPathWorks() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let installed = try fakeExecutable(root: root, name: "installed-codex")

    let snapshot = CodexRuntimeCapability.probe(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_INSTALLED_CODEX_BIN": installed.path],
      timeoutSeconds: 1,
      runner: runner(plaintextReady: [])
    )

    XCTAssertEqual(snapshot.status, .limited)
    XCTAssertEqual(snapshot.mode, .harnessOnly)
    XCTAssertEqual(snapshot.selectedBinaryPath, installed.path)
    XCTAssertFalse(snapshot.plaintextMultiAgentV2)
    XCTAssertTrue(snapshot.harnessOnlyAgents)
  }

  func testPersistsAndAppliesSelectedRuntimeEnvironment() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let paths = testRuntimePaths(root: root)
    let selected = try fakeExecutable(root: root, name: "onecontext-codex")
    let snapshot = CodexRuntimeCapabilitySnapshot(
      status: .ready,
      mode: .bundledCompatibility,
      selectedBinaryPath: selected.path,
      userTitle: "Codex Runtime",
      userDetail: "Using bundled 1Context Codex runtime.",
      nativeMultiAgentV2: false,
      plaintextMultiAgentV2: true,
      harnessOnlyAgents: true,
      probeSummary: "test"
    )

    try CodexRuntimeCapability.persist(snapshot, runtimePaths: paths)
    let loaded = try XCTUnwrap(CodexRuntimeCapability.load(runtimePaths: paths))
    let env = CodexRuntimeCapability.applySelectedRuntime(from: loaded, to: [:])

    XCTAssertEqual(loaded, snapshot)
    XCTAssertEqual(env["ONECONTEXT_CODEX_BIN"], selected.path)
    XCTAssertEqual(env["ONECONTEXT_CODEX_RUNTIME_MODE"], "bundled_compatibility")
    XCTAssertEqual(env["ONECONTEXT_CODEX_PLAINTEXT_MAV2"], "1")
  }

  private func runner(plaintextReady: Set<String>) -> CodexRuntimeCapability.Runner {
    { executable, arguments, _, _ in
      if arguments == ["--version"] {
        return ProcessRunResult(status: 0, stdout: Data("codex test\n".utf8), stderr: Data(), timedOut: false)
      }
      if arguments.contains("features.multi_agent_v2.encrypted_messages=false"),
        plaintextReady.contains(executable.path)
      {
        return ProcessRunResult(status: 0, stdout: Data("help\n".utf8), stderr: Data(), timedOut: false)
      }
      return ProcessRunResult(status: 2, stdout: Data(), stderr: Data("unknown config\n".utf8), timedOut: false)
    }
  }

  private func temporaryRoot() -> URL {
    FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-codex-runtime-\(UUID().uuidString)", isDirectory: true)
  }

  private func testRuntimePaths(root: URL) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true),
      identity: .official
    )
  }

  private func fakeExecutable(root: URL, name: String) throws -> URL {
    let executable = root.appendingPathComponent(name)
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    try "#!/bin/sh\nexit 0\n".write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
    return executable
  }
}
