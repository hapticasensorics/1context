import Foundation
import XCTest
@testable import OneContextPlatform
@testable import OneContextWikiRuntime

final class WikiCoreProcessClientTests: XCTestCase {
  func testCallPassesRuntimeRootAndArgumentsToDiscoveredExecutable() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '{"operation":"%s","root":"%s","arg":"%s"}\\n' "$3" "$2" "$4"
      """
    )
    let paths = testRuntimePaths(root: root)
    let client = WikiCoreProcessClient(
      runtimePaths: paths,
      environment: ["ONECONTEXT_WIKI_CORE_BIN": executable.path]
    )

    let result = try client.call(["page-status", "topics"])

    XCTAssertEqual(result["operation"] as? String, "page-status")
    XCTAssertEqual(result["root"] as? String, paths.userContentDirectory.path)
    XCTAssertEqual(result["arg"] as? String, "topics")
  }

  func testNonZeroJSONStdoutIsPreservedForTypedCoreFailures() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '{"status":"error","operation":"wiki.page.create","error":{"code":"tombstoned_page","message":"page is tombstoned"}}\\n'
      exit 1
      """
    )
    let client = WikiCoreProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_WIKI_CORE_BIN": executable.path]
    )

    XCTAssertThrowsError(try client.call(["page-create", "topics"])) { error in
      let processError = error as? WikiCoreProcessError
      XCTAssertTrue(processError?.message.contains(#""code":"tombstoned_page""#) == true)
      XCTAssertTrue(processError?.message.contains(#""operation":"wiki.page.create""#) == true)
    }
  }

  func testFailedPublishReceiptReturnsStructuredObjectOnNonZeroExit() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      cat <<'JSON'
      {
        "schema_version": 1,
        "status": "failed",
        "operation": "wiki.publish",
        "trigger": "blocked-proof",
        "validation": {
          "status": "error",
          "can_publish": false,
          "issues": [
            {
              "code": "invalid_page_route",
              "severity": "error",
              "message": "Route must start with /"
            }
          ]
        },
        "next_action": "repair_wiki_toml",
        "repair_hints": ["Fix wiki.toml"]
      }
      JSON
      exit 2
      """
    )
    let client = WikiCoreProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_WIKI_CORE_BIN": executable.path]
    )

    let receipt = try client.call(["publish", "--trigger", "blocked-proof"])

    XCTAssertEqual(receipt["operation"] as? String, "wiki.publish")
    XCTAssertEqual(receipt["status"] as? String, "failed")
    XCTAssertEqual(receipt["trigger"] as? String, "blocked-proof")
    XCTAssertEqual(receipt["next_action"] as? String, "repair_wiki_toml")
    XCTAssertEqual(receipt["repair_hints"] as? [String], ["Fix wiki.toml"])
    let validation = receipt["validation"] as? [String: Any]
    XCTAssertEqual(validation?["status"] as? String, "error")
    XCTAssertEqual(validation?["can_publish"] as? Bool, false)
    let issues = validation?["issues"] as? [[String: Any]]
    XCTAssertEqual(issues?.first?["code"] as? String, "invalid_page_route")
  }

  func testLargeFailedPublishReceiptReturnsStructuredObjectOnNonZeroExit() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      awk 'BEGIN {
        printf "{\\"schema_version\\":1,\\"operation\\":\\"wiki.publish\\",\\"status\\":\\"failed\\",\\"validation\\":{\\"status\\":\\"error\\",\\"can_publish\\":false,\\"issue_count\\":9000,\\"issues\\":["
        for (i = 0; i < 9000; i++) {
          if (i > 0) printf ","
          printf "{\\"code\\":\\"large_receipt_issue_%d\\",\\"severity\\":\\"error\\"}", i
        }
        printf "]},\\"next_action\\":\\"repair_wiki_toml\\",\\"large_receipt_marker\\":\\""
        for (i = 0; i < 200000; i++) printf "x"
        print "\\"}"
      }'
      exit 2
      """
    )
    let client = WikiCoreProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_WIKI_CORE_BIN": executable.path]
    )

    let receipt = try client.call(["publish", "--trigger", "large-failed-output"])

    XCTAssertEqual(receipt["operation"] as? String, "wiki.publish")
    XCTAssertEqual(receipt["status"] as? String, "failed")
    XCTAssertEqual(receipt["next_action"] as? String, "repair_wiki_toml")
    XCTAssertEqual((receipt["large_receipt_marker"] as? String)?.count, 200000)
    let validation = receipt["validation"] as? [String: Any]
    XCTAssertEqual(validation?["issue_count"] as? Int, 9000)
    let issues = validation?["issues"] as? [[String: Any]]
    XCTAssertEqual(issues?.count, 9000)
    XCTAssertEqual(issues?.first?["code"] as? String, "large_receipt_issue_0")
  }

  func testCallDrainsLargeStdoutBeforeWaitingForProcessExit() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      awk 'BEGIN {
        printf "{\\"operation\\":\\"large-output\\",\\"payload\\":\\""
        for (i = 0; i < 200000; i++) printf "x"
        print "\\"}"
      }'
      """
    )
    let client = WikiCoreProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_WIKI_CORE_BIN": executable.path]
    )

    let receipt = try client.call(["publish", "--trigger", "large-output"])

    XCTAssertEqual(receipt["operation"] as? String, "large-output")
    XCTAssertEqual((receipt["payload"] as? String)?.count, 200000)
  }

  func testRealWikiCoreLifecycleAndPublishStatusWhenOptedIn() throws {
    let environment = ProcessInfo.processInfo.environment
    guard
      let coreBin = environment["ONECONTEXT_WIKI_CORE_BIN"],
      !coreBin.isEmpty,
      let rootPath = environment["ONECONTEXT_WIKI_CORE_SWIFT_DOGFOOD_ROOT"],
      !rootPath.isEmpty,
      let enginePath = environment["ONECONTEXT_WIKI_ENGINE_DIR"],
      !enginePath.isEmpty
    else {
      throw XCTSkip("Set ONECONTEXT_WIKI_CORE_BIN, ONECONTEXT_WIKI_CORE_SWIFT_DOGFOOD_ROOT, and ONECONTEXT_WIKI_ENGINE_DIR to run the real wiki core bridge dogfood.")
    }

    let paths = RuntimePaths(
      userContentDirectory: URL(fileURLWithPath: rootPath, isDirectory: true),
      appSupportDirectory: URL(fileURLWithPath: rootPath)
        .deletingLastPathComponent()
        .appendingPathComponent("Library/Application Support/1Context", isDirectory: true),
      logDirectory: URL(fileURLWithPath: rootPath)
        .deletingLastPathComponent()
        .appendingPathComponent("Library/Logs/1Context", isDirectory: true),
      cacheDirectory: URL(fileURLWithPath: rootPath)
        .deletingLastPathComponent()
        .appendingPathComponent("Library/Caches/1Context", isDirectory: true)
    )
    let client = WikiCoreProcessClient(
      runtimePaths: paths,
      environment: ["ONECONTEXT_WIKI_CORE_BIN": coreBin]
    )

    _ = try client.call(["page-create-all"])
    let initialStatus = try client.call(["publish-status"])
    XCTAssertEqual(initialStatus["next_action"] as? String, "publish")

    let firstPublish = try client.call(["publish", "--wiki-engine", enginePath, "--trigger", "swift-bridge.initial"])
    XCTAssertEqual(firstPublish["status"] as? String, "published")
    XCTAssertEqual(firstPublish["trigger"] as? String, "swift-bridge.initial")
    let cleanStatus = try client.call(["publish-status"])
    XCTAssertEqual(cleanStatus["next_action"] as? String, "none")
    XCTAssertEqual(cleanStatus["render_required"] as? Bool, false)

    let agent = try client.call([
      "agent-identify",
      "--thread-id", "worker-ar-swift-bridge",
      "--role", "role://topics.curator",
      "--ttl-seconds", "3600"
    ])
    let agentID = try requireString(agent, "agent_id")
    _ = try client.call(["page-watch", "topics", "--agent-id", agentID])
    let talk = try client.call([
      "talk-append",
      "--page", "topics",
      "--kind", "proposal",
      "--subject", "Swift bridge talk-only proof",
      "--from", "agent://worker-ar/swift-bridge",
      "--to-role", "curator",
      "--body", "Talk and mail changes should not dirty rendered page content."
    ])
    XCTAssertEqual(talk["render_required"] as? Bool, false)
    let afterTalkStatus = try client.call(["publish-status"])
    XCTAssertEqual(afterTalkStatus["next_action"] as? String, "none")
    XCTAssertEqual(afterTalkStatus["render_required"] as? Bool, false)

    let create = try client.call([
      "page-create",
      "swift-bridge-proof",
      "--title", "Swift Bridge Proof",
      "--route", "/swift-bridge-proof",
      "--family-group", "agent-lab",
      "--type", "context-page",
      "--nav-section", "utility",
      "--nav-order", "901"
    ])
    XCTAssertEqual(create["operation"] as? String, "wiki.page.create")
    let createdStatus = try client.call(["page-status", "swift-bridge-proof"])
    XCTAssertEqual(createdStatus["id"] as? String, "swift-bridge-proof")
    XCTAssertEqual(createdStatus["next_action"] as? String, "publish")

    let secondPublish = try client.call(["publish", "--wiki-engine", enginePath, "--trigger", "swift-bridge.lifecycle"])
    XCTAssertEqual(secondPublish["status"] as? String, "published")
    XCTAssertEqual(secondPublish["trigger"] as? String, "swift-bridge.lifecycle")
    let finalStatus = try client.call(["publish-status"])
    XCTAssertEqual(finalStatus["next_action"] as? String, "none")
    XCTAssertEqual(finalStatus["render_required"] as? Bool, false)
  }

  private func temporaryRoot() -> URL {
    FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-wiki-core-process-client-\(UUID().uuidString)", isDirectory: true)
  }

  private func testRuntimePaths(root: URL) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: root.appendingPathComponent("1Context", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("Application Support/1Context", isDirectory: true),
      logDirectory: root.appendingPathComponent("Logs/1Context", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("Caches/1Context", isDirectory: true)
    )
  }

  private func fakeExecutable(root: URL, body: String) throws -> URL {
    let executable = root.appendingPathComponent("fake-onecontext-wiki")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    try ("#!/bin/sh\n" + body + "\n").write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
    return executable
  }

  private func requireString(_ object: [String: Any], _ key: String) throws -> String {
    guard let value = object[key] as? String else {
      throw NSError(
        domain: "WikiCoreProcessClientTests",
        code: 1,
        userInfo: [NSLocalizedDescriptionKey: "Missing string key \(key) in \(object)"]
      )
    }
    return value
  }
}
