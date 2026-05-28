import Foundation
import XCTest
@testable import OneContextAgentRuntime
@testable import OneContextPlatform

final class AgentHarnessProcessClientTests: XCTestCase {
  func testCallPassesRuntimeRootAndArgumentsToDiscoveredExecutable() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '{"command":"%s","root":"%s","extra":"%s"}\\n' "$3" "$2" "$4"
      """
    )
    let paths = testRuntimePaths(root: root)
    let client = AgentHarnessProcessClient(
      runtimePaths: paths,
      environment: ["ONECONTEXT_AGENT_HARNESS_BIN": executable.path]
    )

    let result = try client.call(["status", "active"])

    XCTAssertEqual(result["command"] as? String, "status")
    XCTAssertEqual(result["root"] as? String, paths.userContentDirectory.path)
    XCTAssertEqual(result["extra"] as? String, "active")
  }

  func testBridgeOwnsSupportedHarnessMethodKnowledge() {
    let supportedMethods = [
      "agent.harness.status",
      "agent.harness.describe",
      "agent.harness.ensure",
      "agent.harness.call",
      "agent.harness.birth",
      "agent.harness.start-turn",
      "agent.harness.complete_turn",
      "agent.harness.observe-proof",
      "agent.harness.record_adapter_event",
      "agent.harness.transport-plan",
      "agent.harness.agents",
      "agent.harness.list",
      "agent.harness.agent-status",
      "agent.harness.retire"
    ]
    let daemonOwnedOrUnknownMethods = [
      "health",
      "wiki.status",
      "agent.harness.nope"
    ]

    for method in supportedMethods {
      XCTAssertTrue(AgentHarnessRPCBridge.supports(method: method), method)
    }
    for method in daemonOwnedOrUnknownMethods {
      XCTAssertFalse(AgentHarnessRPCBridge.supports(method: method), method)
    }
  }

  func testBridgeMapsReadOnlyMethodsToStableCommands() throws {
    let recorder = RecordingHarness()
    let bridge = AgentHarnessRPCBridge(call: recorder.call)

    _ = try bridge.call(method: "agent.harness.status")
    _ = try bridge.call(method: "agent.harness.describe")
    _ = try bridge.call(method: "agent.harness.ensure")

    XCTAssertEqual(recorder.calls, [
      ["status"],
      ["describe"],
      ["ensure"]
    ])
  }

  func testBridgeTypedHelpersMapToRequestCommands() throws {
    let recorder = RecordingHarness()
    let bridge = AgentHarnessRPCBridge(call: recorder.call)

    _ = try bridge.agents()
    _ = try bridge.agentStatus(unitID: "agent-1")
    _ = try bridge.call(request: ["template": "mail"])
    _ = try bridge.retire(unitID: "agent-1", reason: "complete")

    XCTAssertEqual(recorder.calls.count, 4)
    XCTAssertEqual(recorder.calls.map { $0[0] }, [
      "agents",
      "agent-status",
      "call",
      "retire"
    ])
    for call in recorder.calls {
      XCTAssertEqual(call[1], "--request-json")
    }
    XCTAssertTrue(try requestJSON(from: recorder.calls[0]).isEmpty)
    XCTAssertEqual(try requestJSON(from: recorder.calls[1])["unit_id"] as? String, "agent-1")
    XCTAssertEqual(try requestJSON(from: recorder.calls[2])["template"] as? String, "mail")
    XCTAssertEqual(try requestJSON(from: recorder.calls[3])["unit_id"] as? String, "agent-1")
    XCTAssertEqual(try requestJSON(from: recorder.calls[3])["reason"] as? String, "complete")
  }

  func testBridgeMapsFrontierDaemonMethodsToStableCommands() throws {
    let recorder = RecordingHarness()
    let bridge = AgentHarnessRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "agent.harness.start-turn",
      params: ["unit_id": "agent-1"]
    )
    _ = try bridge.call(
      method: "agent.harness.complete_turn",
      params: ["unit_id": "agent-1"]
    )
    _ = try bridge.call(
      method: "agent.harness.observe-proof",
      params: ["unit_id": "agent-1", "proof_key": "transport_identity"]
    )
    _ = try bridge.call(
      method: "agent.harness.record_adapter_event",
      params: [
        "unit_id": "agent-1",
        "kind": "context_injection",
        "status": "observed"
      ]
    )
    _ = try bridge.call(
      method: "agent.harness.transport-plan",
      params: ["unit_id": "agent-1"]
    )

    XCTAssertEqual(recorder.calls.map { $0[0] }, [
      "start-turn",
      "complete-turn",
      "observe-proof",
      "record-adapter-event",
      "transport-plan"
    ])
    XCTAssertEqual(try requestJSON(from: recorder.calls[0])["unit_id"] as? String, "agent-1")
    XCTAssertEqual(try requestJSON(from: recorder.calls[1])["unit_id"] as? String, "agent-1")
    XCTAssertEqual(try requestJSON(from: recorder.calls[2])["proof_key"] as? String, "transport_identity")
    XCTAssertEqual(try requestJSON(from: recorder.calls[3])["kind"] as? String, "context_injection")
    XCTAssertEqual(try requestJSON(from: recorder.calls[3])["status"] as? String, "observed")
    XCTAssertEqual(try requestJSON(from: recorder.calls[4])["unit_id"] as? String, "agent-1")
  }

  func testBridgeFrontierTypedHelpersPassRequestJSON() throws {
    let recorder = RecordingHarness()
    let bridge = AgentHarnessRPCBridge(call: recorder.call)

    _ = try bridge.startTurn(unitID: "agent-1", request: ["turn_id": "turn-1"])
    _ = try bridge.completeTurn(unitID: "agent-1", request: ["usage": ["input_tokens": 8]])
    _ = try bridge.observeProof(unitID: "agent-1", proofKey: "transport_identity")
    _ = try bridge.recordAdapterEvent(
      unitID: "agent-1",
      kind: "context_injection_executed",
      status: "observed",
      request: ["metadata": ["redacted": true]]
    )
    _ = try bridge.transportPlan(request: ["unit_id": "agent-1", "transport": "mcp"])

    XCTAssertEqual(recorder.calls.map { $0[0] }, [
      "start-turn",
      "complete-turn",
      "observe-proof",
      "record-adapter-event",
      "transport-plan"
    ])
    XCTAssertEqual(try requestJSON(from: recorder.calls[0])["unit_id"] as? String, "agent-1")
    XCTAssertEqual(try requestJSON(from: recorder.calls[0])["turn_id"] as? String, "turn-1")
    XCTAssertEqual(try requestJSON(from: recorder.calls[1])["unit_id"] as? String, "agent-1")
    let usage = try XCTUnwrap(requestJSON(from: recorder.calls[1])["usage"] as? [String: Any])
    XCTAssertEqual(usage["input_tokens"] as? Int, 8)
    XCTAssertEqual(try requestJSON(from: recorder.calls[2])["proof_key"] as? String, "transport_identity")
    XCTAssertEqual(try requestJSON(from: recorder.calls[3])["kind"] as? String, "context_injection_executed")
    XCTAssertEqual(try requestJSON(from: recorder.calls[3])["status"] as? String, "observed")
    let metadata = try XCTUnwrap(requestJSON(from: recorder.calls[3])["metadata"] as? [String: Any])
    XCTAssertEqual(metadata["redacted"] as? Bool, true)
    XCTAssertEqual(try requestJSON(from: recorder.calls[4])["transport"] as? String, "mcp")
  }

  func testBridgeRejectsInvalidRequestJSONBeforeProcessCall() {
    let recorder = RecordingHarness()
    let bridge = AgentHarnessRPCBridge(call: recorder.call)

    XCTAssertThrowsError(
      try bridge.call(method: "agent.harness.call", params: ["bad": Double.nan])
    ) { error in
      XCTAssertEqual(error as? AgentHarnessRPCBridgeError, .invalidJSONParameters("agent.harness.call"))
    }
    XCTAssertEqual(recorder.calls, [])
  }

  func testBridgeRejectsUnsupportedMethodsBeforeProcessCall() {
    let recorder = RecordingHarness()
    let bridge = AgentHarnessRPCBridge(call: recorder.call)

    XCTAssertThrowsError(try bridge.call(method: "agent.harness.nope")) { error in
      XCTAssertEqual(error as? AgentHarnessRPCBridgeError, .unsupportedMethod("agent.harness.nope"))
    }
    XCTAssertEqual(recorder.calls, [])
  }

  func testProcessClientTypedHelpersPassRequestJSON() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '{"command":"%s","flag":"%s","request":%s}\\n' "$3" "$4" "$5"
      """
    )
    let client = AgentHarnessProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_AGENT_HARNESS_BIN": executable.path]
    )

    let result = try client.agentStatus(unitID: "agent-1")
    let request = try XCTUnwrap(result["request"] as? [String: Any])

    XCTAssertEqual(result["command"] as? String, "agent-status")
    XCTAssertEqual(result["flag"] as? String, "--request-json")
    XCTAssertEqual(request["unit_id"] as? String, "agent-1")
  }

  func testProcessClientFrontierTypedHelpersPassRequestJSON() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      printf '{"command":"%s","flag":"%s","request":%s}\\n' "$3" "$4" "$5"
      """
    )
    let client = AgentHarnessProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_AGENT_HARNESS_BIN": executable.path]
    )

    let startTurn = try client.startTurn(unitID: "agent-1", request: ["turn_id": "turn-1"])
    let completeTurn = try client.completeTurn(unitID: "agent-1", request: ["usage": ["output_tokens": 13]])
    let observeProof = try client.observeProof(unitID: "agent-1", proofKey: "transport_identity")
    let adapterEvent = try client.recordAdapterEvent(
      unitID: "agent-1",
      kind: "transport_identity_observed",
      status: "observed",
      request: ["correlation": ["thread_id": "thread-1"]]
    )
    let transportPlan = try client.transportPlan(request: ["unit_id": "agent-1", "adapter": "local_test"])

    XCTAssertEqual(startTurn["command"] as? String, "start-turn")
    XCTAssertEqual(try requestJSON(from: startTurn)["turn_id"] as? String, "turn-1")
    XCTAssertEqual(completeTurn["command"] as? String, "complete-turn")
    let usage = try XCTUnwrap(requestJSON(from: completeTurn)["usage"] as? [String: Any])
    XCTAssertEqual(usage["output_tokens"] as? Int, 13)
    XCTAssertEqual(observeProof["command"] as? String, "observe-proof")
    XCTAssertEqual(try requestJSON(from: observeProof)["proof_key"] as? String, "transport_identity")
    XCTAssertEqual(adapterEvent["command"] as? String, "record-adapter-event")
    XCTAssertEqual(try requestJSON(from: adapterEvent)["kind"] as? String, "transport_identity_observed")
    let correlation = try XCTUnwrap(requestJSON(from: adapterEvent)["correlation"] as? [String: Any])
    XCTAssertEqual(correlation["thread_id"] as? String, "thread-1")
    XCTAssertEqual(transportPlan["command"] as? String, "transport-plan")
    XCTAssertEqual(transportPlan["flag"] as? String, "--request-json")
    XCTAssertEqual(try requestJSON(from: transportPlan)["adapter"] as? String, "local_test")
  }

  func testProcessClientPreservesStructuredRustErrorJSON() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      cat <<'JSON'
      {"schema_version":1,"status":"error","surface":"agent_harness","error":{"code":"agent_harness_store_unavailable","message":"agent store is unavailable"},"repair_hints":["Start the agent harness store and retry.","Keep the command receipt shape stable while restoring behavior."]}
      JSON
      exit 3
      """
    )
    let client = AgentHarnessProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_AGENT_HARNESS_BIN": executable.path]
    )

    XCTAssertThrowsError(try client.agents()) { error in
      let processError = error as? AgentHarnessProcessError
      XCTAssertEqual(processError?.terminationStatus, 3)
      XCTAssertEqual(processError?.message, "agent store is unavailable")
      XCTAssertEqual(processError?.errorCode, "agent_harness_store_unavailable")
      XCTAssertEqual(processError?.repairHints.count, 2)
      XCTAssertEqual(processError?.structuredPayload?["status"] as? String, "error")
    }
  }

  func testProcessClientPreservesStructuredRustErrorJSONFromStderr() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let executable = try fakeExecutable(
      root: root,
      body: """
      cat >&2 <<'JSON'
      {"schema_version":1,"status":"error","surface":"agent_harness","error":{"code":"agent_harness_invalid_request","message":"record-adapter-event requires status","details":{"command":"record-adapter-event"}},"repair_hints":["Add a non-empty status string to the request JSON."]}
      JSON
      exit 2
      """
    )
    let client = AgentHarnessProcessClient(
      runtimePaths: testRuntimePaths(root: root),
      environment: ["ONECONTEXT_AGENT_HARNESS_BIN": executable.path]
    )

    XCTAssertThrowsError(
      try client.recordAdapterEvent(request: ["unit_id": "agent-1", "kind": "context_injection"])
    ) { error in
      let processError = error as? AgentHarnessProcessError
      XCTAssertEqual(processError?.terminationStatus, 2)
      XCTAssertEqual(processError?.message, "record-adapter-event requires status")
      XCTAssertEqual(processError?.errorCode, "agent_harness_invalid_request")
      XCTAssertEqual(processError?.repairHints, ["Add a non-empty status string to the request JSON."])
      XCTAssertEqual(processError?.structuredPayload?["surface"] as? String, "agent_harness")
      XCTAssertTrue(processError?.stdout.isEmpty == true)
      XCTAssertTrue(processError?.stderr.contains("agent_harness_invalid_request") == true)
    }
  }

  private func temporaryRoot() -> URL {
    FileManager.default.temporaryDirectory
      .appendingPathComponent("1context-agent-harness-process-client-\(UUID().uuidString)", isDirectory: true)
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
    let executable = root.appendingPathComponent("fake-onecontext-agent-harness")
    try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    try ("#!/bin/sh\n" + body + "\n").write(to: executable, atomically: true, encoding: .utf8)
    try FileManager.default.setAttributes([.posixPermissions: 0o700], ofItemAtPath: executable.path)
    return executable
  }

  private func requestJSON(from call: [String]) throws -> [String: Any] {
    XCTAssertEqual(call[1], "--request-json")
    let data = try XCTUnwrap(call[2].data(using: .utf8))
    return try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
  }

  private func requestJSON(from result: [String: Any]) throws -> [String: Any] {
    XCTAssertEqual(result["flag"] as? String, "--request-json")
    return try XCTUnwrap(result["request"] as? [String: Any])
  }
}

private final class RecordingHarness {
  var calls: [[String]] = []

  func call(_ arguments: [String]) throws -> [String: Any] {
    calls.append(arguments)
    return ["status": "ok"]
  }
}
