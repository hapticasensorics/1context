import XCTest
@testable import OneContextWikiRuntime

final class WikiCoreRPCBridgeTests: XCTestCase {
  func testBridgeOwnsSupportedWikiCoreMethodKnowledge() {
    let supportedMethods = [
      "wiki.list",
      "wiki.validate",
      "wiki.page.open",
      "wiki.page-open",
      "wiki.page.write-body",
      "wiki.page-write-body",
      "wiki.asset.add",
      "wiki.asset-list",
      "wiki.reference.list",
      "wiki.references",
      "wiki.talk-append",
      "wiki.agent.identify",
      "wiki.agent.heartbeat",
      "wiki.agent.retire",
      "wiki.agent.status",
      "wiki.agent.inbox",
      "wiki.mail.open",
      "wiki.mail.send",
      "wiki.mail.claim",
      "wiki.mail.mark",
      "wiki.mail.snooze",
      "wiki.notify.poll",
      "wiki.notify.ack",
      "wiki.notify.dispatch",
      "wiki.publish.status",
      "wiki.publish"
    ]
    let daemonOwnedOrUnknownMethods = [
      "health",
      "wiki.status",
      "wiki.nope"
    ]

    for method in supportedMethods {
      XCTAssertTrue(WikiCoreRPCBridge.supports(method: method), method)
    }
    for method in daemonOwnedOrUnknownMethods {
      XCTAssertFalse(WikiCoreRPCBridge.supports(method: method), method)
    }
  }

  func testPageCommandAliasesMapToWikiCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(method: "wiki.page-status", params: ["route": "/topics"])
    _ = try bridge.call(method: "wiki.page_open", params: ["page": "topics"])
    _ = try bridge.call(method: "wiki.page-create", params: ["id": "rpc-proof"])
    _ = try bridge.call(
      method: "wiki.asset.add",
      params: ["page": "rpc-proof", "file": "/tmp/diagram.png", "caption": "Diagram", "altText": "A diagram"]
    )
    _ = try bridge.call(method: "wiki.asset-list", params: ["page": "rpc-proof"])
    _ = try bridge.call(method: "wiki.reference.list", params: ["page": "rpc-proof"])
    _ = try bridge.call(method: "wiki.references")
    _ = try bridge.call(method: "wiki.page-delete", params: ["page": "rpc-proof"])
    _ = try bridge.call(method: "wiki.page_restore", params: ["page": "rpc-proof"])

    XCTAssertEqual(recorder.calls, [
      ["page-status", "/topics"],
      ["page-open", "topics"],
      ["page-create", "rpc-proof"],
      ["asset-add", "rpc-proof", "--file", "/tmp/diagram.png", "--caption", "Diagram", "--alt-text", "A diagram"],
      ["asset-list", "rpc-proof"],
      ["reference-list", "rpc-proof"],
      ["reference-list"],
      ["page-delete", "rpc-proof"],
      ["page-restore", "rpc-proof"]
    ])
  }

  func testReadOnlyAndPublishMethodsMapToWikiCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(
      call: recorder.call,
      defaultWikiEngineDirectory: "/opt/1Context/WikiEngine",
      defaultNodeExecutable: "/usr/bin/env"
    )

    _ = try bridge.call(method: "wiki.list")
    _ = try bridge.call(method: "wiki.validate")
    _ = try bridge.call(method: "wiki.publish.status")
    _ = try bridge.call(method: "wiki.publish-status")
    _ = try bridge.call(method: "wiki.publish", params: ["trigger": "rpc-proof", "force": true])

    XCTAssertEqual(recorder.calls, [
      ["list"],
      ["validate"],
      ["publish-status"],
      ["publish-status"],
      ["publish", "--wiki-engine", "/opt/1Context/WikiEngine", "--trigger", "rpc-proof", "--force"]
    ])
  }

  func testPublishPassesDiscoveredNodeExecutable() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(
      call: recorder.call,
      defaultWikiEngineDirectory: "/opt/1Context/WikiEngine",
      defaultNodeExecutable: "/opt/homebrew/bin/node"
    )

    _ = try bridge.call(method: "wiki.publish", params: ["trigger": "rpc-proof", "force": true])

    XCTAssertEqual(recorder.calls, [
      [
        "publish",
        "--wiki-engine",
        "/opt/1Context/WikiEngine",
        "--node",
        "/opt/homebrew/bin/node",
        "--trigger",
        "rpc-proof",
        "--force"
      ]
    ])
  }

  func testPageCreateMapsConsumerParamsToExplicitPlacementFlags() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "wiki.page.create",
      params: [
        "id": "rpc-proof",
        "title": "RPC Proof",
        "route": "/agent-lab/rpc-proof",
        "slug": "rpc-proof-slug",
        "familyGroup": "80-agent-lab",
        "familyGroupTitle": "Agent Lab",
        "family_id": "10-rpc-proof",
        "family_title": "RPC Proof",
        "type": "context-page",
        "template": "pages/context-page.md",
        "talkConventionsTemplate": "talk/conventions/default.md",
        "talk_curator_template": "talk/curator/default.md",
        "summary": "A page created through daemon RPC.",
        "navSection": "utility",
        "nav_order": 55
      ]
    )

    XCTAssertEqual(recorder.calls[0], [
      "page-create",
      "rpc-proof",
      "--title",
      "RPC Proof",
      "--route",
      "/agent-lab/rpc-proof",
      "--slug",
      "rpc-proof-slug",
      "--family-group",
      "80-agent-lab",
      "--family-group-title",
      "Agent Lab",
      "--family-id",
      "10-rpc-proof",
      "--family-title",
      "RPC Proof",
      "--type",
      "context-page",
      "--template",
      "pages/context-page.md",
      "--talk-conventions-template",
      "talk/conventions/default.md",
      "--talk-curator-template",
      "talk/curator/default.md",
      "--summary",
      "A page created through daemon RPC.",
      "--nav-section",
      "utility",
      "--nav-order",
      "55"
    ])
  }

  func testFileBackedBodyAndTalkParamsMapToCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "wiki.page-write-body",
      params: [
        "page": "topics",
        "bodyFile": "/tmp/page-body.md",
        "expected_source_sha256": "abc123"
      ]
    )
    _ = try bridge.call(
      method: "wiki.page_patch_body",
      params: [
        "page": "topics",
        "findFile": "/tmp/find.md",
        "replace_file": "/tmp/replace.md"
      ]
    )
    _ = try bridge.call(
      method: "wiki.talk-append",
      params: [
        "page": ["id": "topics"],
        "message": [
          "subject": "File-backed nested talk",
          "fromAddress": "agent://codex/worker-bl",
          "to": ["role://topics.curator"],
          "body_file": "/tmp/talk-body.md",
          "attachments": [["path": "/tmp/evidence.txt"]]
        ]
      ]
    )

    XCTAssertEqual(recorder.calls, [
      ["page-write-body", "topics", "--body-file", "/tmp/page-body.md", "--expected-source-sha256", "abc123"],
      ["page-patch-body", "topics", "--find-file", "/tmp/find.md", "--replace-file", "/tmp/replace.md"],
      [
        "talk-append",
        "--page",
        "topics",
        "--subject",
        "File-backed nested talk",
        "--from",
        "agent://codex/worker-bl",
        "--to",
        "role://topics.curator",
        "--body-file",
        "/tmp/talk-body.md",
        "--attachment",
        "/tmp/evidence.txt"
      ]
    ])
  }

  func testTalkAppendAcceptsNestedTargetSchemaParams() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "wiki.talk.append",
      params: [
        "page": ["id": "topics"],
        "message": [
          "kind": "reply",
          "subject": "Thread follow-up",
          "threadId": "thread-topics-proof",
          "operationId": "swift-rpc-mail-001",
          "deliveryMode": "mail",
          "from": "agent://codex/worker-ba",
          "to": ["role://topics.curator"],
          "cc": ["role://topics.reviewer"],
          "body_markdown": "Nested consumer params should map to CLI flags."
        ],
        "attachments": [
          [
            "source": ["path": "/tmp/source-proof.png"],
            "filename": "agent-facing.png",
            "caption": "Rendered proof",
            "altText": "A diagram proving the rendered path"
          ],
          ["path": "/tmp/direct-proof.txt"]
        ]
      ]
    )

    XCTAssertEqual(recorder.calls[0], [
      "talk-append",
      "--page",
      "topics",
      "--subject",
      "Thread follow-up",
      "--from",
      "agent://codex/worker-ba",
      "--kind",
      "reply",
      "--thread-id",
      "thread-topics-proof",
      "--operation-id",
      "swift-rpc-mail-001",
      "--delivery-mode",
      "mail",
      "--to",
      "role://topics.curator",
      "--cc",
      "role://topics.reviewer",
      "--body",
      "Nested consumer params should map to CLI flags.",
      "--attachment",
      "/tmp/source-proof.png",
      "--attachment-filename",
      "agent-facing.png",
      "--attachment-caption",
      "Rendered proof",
      "--attachment-alt",
      "A diagram proving the rendered path",
      "--attachment",
      "/tmp/direct-proof.txt"
    ])
  }

  func testAgentMailAndNotificationMethodsMapToCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "wiki.agent.identify",
      params: [
        "threadId": "019e3f72-3471-7da1-92a8-56e5d25aaa01",
        "roles": ["role://topics.curator"],
        "capabilities": ["wiki.mail"],
        "ttlSeconds": 120
      ]
    )
    _ = try bridge.call(
      method: "wiki.agent.heartbeat",
      params: ["agentId": "agent_codex_abc", "ttlSeconds": 90]
    )
    _ = try bridge.call(method: "wiki.agent.retire", params: ["agentId": "agent_codex_old"])
    _ = try bridge.call(method: "wiki.agent.status", params: ["agentId": "agent_codex_abc"])
    _ = try bridge.call(method: "wiki.agent.status_by_thread", params: ["threadId": "019e3f72-3471-7da1-92a8-56e5d25aaa01"])
    _ = try bridge.call(method: "wiki.agent.inbox", params: ["agentId": "agent_codex_abc"])
    _ = try bridge.call(
      method: "wiki.mail.open",
      params: ["deliveryId": "delivery_001", "agentId": "agent_codex_abc"]
    )
    _ = try bridge.call(
      method: "wiki.mail.record_injection",
      params: [
        "deliveryId": "delivery_001",
        "agentId": "agent_codex_abc",
        "threadId": "019e3f72-3471-7da1-92a8-56e5d25aaa01",
        "result": "ok",
        "itemCount": "1"
      ]
    )
    _ = try bridge.call(
      method: "wiki.mail.send",
      params: [
        "page": "topics",
        "subject": "Mail send through talk append",
        "from": "agent://codex/sender",
        "to": ["role://topics.curator"],
        "body": "This should use the current durable send path."
      ]
    )
    _ = try bridge.call(
      method: "wiki.mail.claim",
      params: ["deliveryId": "delivery_001", "agentId": "agent_codex_abc"]
    )
    _ = try bridge.call(
      method: "wiki.mail.mark",
      params: ["deliveryId": "delivery_001", "agentId": "agent_codex_abc", "state": "done"]
    )
    _ = try bridge.call(
      method: "wiki.mail.snooze",
      params: ["deliveryId": "delivery_002", "agentId": "agent_codex_abc", "until": "2026-05-21T08:00:00Z"]
    )
    _ = try bridge.call(method: "wiki.notify.poll", params: ["agentId": "agent_codex_abc"])
    _ = try bridge.call(
      method: "wiki.notify.ack",
      params: ["notificationId": "notif_001", "agentId": "agent_codex_abc"]
    )
    _ = try bridge.call(
      method: "wiki.notify.dispatch",
      params: [
        "agentId": "agent_codex_abc",
        "dryRun": true,
        "steeringCommand": "/usr/bin/true",
        "steeringArgs": ["--unused"],
        "payloadFormat": "json",
        "limit": 3
      ]
    )

    XCTAssertEqual(recorder.calls, [
      [
        "agent-identify",
        "--thread-id",
        "019e3f72-3471-7da1-92a8-56e5d25aaa01",
        "--role",
        "role://topics.curator",
        "--capability",
        "wiki.mail",
        "--ttl-seconds",
        "120"
      ],
      ["agent-heartbeat", "agent_codex_abc", "--ttl-seconds", "90"],
      ["agent-retire", "agent_codex_old"],
      ["agent-status", "agent_codex_abc"],
      ["agent-status-by-thread", "019e3f72-3471-7da1-92a8-56e5d25aaa01"],
      ["agent-inbox", "agent_codex_abc"],
      ["mail-open", "delivery_001", "--agent-id", "agent_codex_abc"],
      [
        "mail-record-injection",
        "delivery_001",
        "--agent-id",
        "agent_codex_abc",
        "--thread-id",
        "019e3f72-3471-7da1-92a8-56e5d25aaa01",
        "--result",
        "ok",
        "--item-count",
        "1"
      ],
      [
        "talk-append",
        "--page",
        "topics",
        "--subject",
        "Mail send through talk append",
        "--from",
        "agent://codex/sender",
        "--delivery-mode",
        "mail",
        "--to",
        "role://topics.curator",
        "--body",
        "This should use the current durable send path."
      ],
      ["mail-claim", "delivery_001", "--agent-id", "agent_codex_abc"],
      ["mail-mark", "delivery_001", "--agent-id", "agent_codex_abc", "--state", "done"],
      [
        "mail-snooze",
        "delivery_002",
        "--agent-id",
        "agent_codex_abc",
        "--until",
        "2026-05-21T08:00:00Z"
      ],
      ["notify-poll", "agent_codex_abc"],
      ["notify-ack", "notif_001", "--agent-id", "agent_codex_abc"],
      [
        "notify-dispatch",
        "agent_codex_abc",
        "--dry-run",
        "--steering-command",
        "/usr/bin/true",
        "--steering-arg",
        "--unused",
        "--payload-format",
        "json",
        "--limit",
        "3"
      ]
    ])
  }

  func testConvenienceAgentMailAndNotificationMethodsStayThin() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.identifyAgent(
      threadID: "thread-clean-api",
      roles: ["role://topics.curator"],
      capabilities: ["wiki.mail"],
      ttlSeconds: 300
    )
    _ = try bridge.heartbeatAgent(agentID: "agent_codex_clean", ttlSeconds: 180)
    _ = try bridge.agentStatus(agentID: "agent_codex_clean")
    _ = try bridge.agentStatusByThread(threadID: "thread-clean-api")
    _ = try bridge.agentInbox(agentID: "agent_codex_clean")
    _ = try bridge.openMail(deliveryID: "delivery_clean", agentID: "agent_codex_clean")
    _ = try bridge.recordMailInjection(
      deliveryID: "delivery_clean",
      agentID: "agent_codex_clean",
      itemCount: 1,
      threadID: "thread-clean-api"
    )
    _ = try bridge.sendMail(
      params: [
        "page": "topics",
        "subject": "Convenience send",
        "from": "agent://codex/clean-sender",
        "to": ["role://topics.curator"],
        "body": "Convenience send remains talk-mail."
      ]
    )
    _ = try bridge.claimMail(deliveryID: "delivery_clean", agentID: "agent_codex_clean")
    _ = try bridge.markMail(deliveryID: "delivery_clean", agentID: "agent_codex_clean", state: "done")
    _ = try bridge.pollNotifications(agentID: "agent_codex_clean", cursor: "notifcur_1")
    _ = try bridge.acknowledgeNotification(notificationID: "notif_clean", agentID: "agent_codex_clean")
    _ = try bridge.dispatchNotifications(
      agentID: "agent_codex_clean",
      dryRun: true,
      payloadFormat: "json",
      limit: 1
    )

    XCTAssertEqual(recorder.calls, [
      [
        "agent-identify",
        "--thread-id",
        "thread-clean-api",
        "--role",
        "role://topics.curator",
        "--capability",
        "wiki.mail",
        "--ttl-seconds",
        "300"
      ],
      ["agent-heartbeat", "agent_codex_clean", "--ttl-seconds", "180"],
      ["agent-status", "agent_codex_clean"],
      ["agent-status-by-thread", "thread-clean-api"],
      ["agent-inbox", "agent_codex_clean"],
      ["mail-open", "delivery_clean", "--agent-id", "agent_codex_clean"],
      [
        "mail-record-injection",
        "delivery_clean",
        "--agent-id",
        "agent_codex_clean",
        "--thread-id",
        "thread-clean-api",
        "--result",
        "ok",
        "--item-count",
        "1"
      ],
      [
        "talk-append",
        "--page",
        "topics",
        "--subject",
        "Convenience send",
        "--from",
        "agent://codex/clean-sender",
        "--delivery-mode",
        "mail",
        "--to",
        "role://topics.curator",
        "--body",
        "Convenience send remains talk-mail."
      ],
      ["mail-claim", "delivery_clean", "--agent-id", "agent_codex_clean"],
      ["mail-mark", "delivery_clean", "--agent-id", "agent_codex_clean", "--state", "done"],
      ["notify-poll", "agent_codex_clean", "--cursor", "notifcur_1"],
      ["notify-ack", "notif_clean", "--agent-id", "agent_codex_clean"],
      ["notify-dispatch", "agent_codex_clean", "--dry-run", "--payload-format", "json", "--limit", "1"]
    ])
  }

  func testMissingRequiredParamsReturnActionableErrors() throws {
    let bridge = WikiCoreRPCBridge(call: { _ in [:] })

    XCTAssertThrowsError(try bridge.call(method: "wiki.page.open")) { error in
      XCTAssertEqual(error.localizedDescription, "wiki RPC requires params.page")
    }
    XCTAssertThrowsError(try bridge.call(method: "wiki.page.write_body", params: ["page": "topics"])) { error in
      XCTAssertEqual(error.localizedDescription, "wiki RPC requires params.body or bodyFile")
    }
    XCTAssertThrowsError(
      try bridge.call(
        method: "wiki.talk.append",
        params: [
          "page": "topics",
          "message": [
            "subject": "Missing body",
            "from": "agent://codex/worker-bl"
          ]
        ]
      )
    ) { error in
      XCTAssertEqual(error.localizedDescription, "wiki RPC requires params.body or bodyFile")
    }
    XCTAssertThrowsError(try bridge.call(method: "wiki.nope")) { error in
      XCTAssertEqual(error.localizedDescription, "unsupported wiki RPC method: wiki.nope")
    }
  }
}

private final class RecordingCore {
  private(set) var calls: [[String]] = []

  func call(_ arguments: [String]) throws -> [String: Any] {
    calls.append(arguments)
    return ["operation": arguments[0]]
  }
}
