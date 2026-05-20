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
      "wiki.page_watch",
      "wiki.page-assign-role",
      "wiki.talk-append",
      "wiki.publish.status",
      "wiki.publish",
      "wiki.mail.mark-all",
      "wiki.mail-claim",
      "wiki.mail_subscriptions",
      "wiki.notify.ack",
      "wiki.notify-ack"
    ]
    let daemonOwnedOrUnknownMethods = [
      "health",
      "wiki.status",
      "wiki.start",
      "wiki.refresh",
      "wiki.stop",
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
    _ = try bridge.call(method: "wiki.page-delete", params: ["page": "rpc-proof"])
    _ = try bridge.call(method: "wiki.page_restore", params: ["page": "rpc-proof"])
    _ = try bridge.call(
      method: "wiki.page-assign-role",
      params: [
        "page": "topics",
        "agentId": "agent-1",
        "role": "curator"
      ]
    )

    XCTAssertEqual(recorder.calls, [
      ["page-status", "/topics"],
      ["page-open", "topics"],
      ["page-create", "rpc-proof"],
      ["asset-add", "rpc-proof", "--file", "/tmp/diagram.png", "--caption", "Diagram", "--alt-text", "A diagram"],
      ["asset-list", "rpc-proof"],
      ["page-delete", "rpc-proof"],
      ["page-restore", "rpc-proof"],
      ["page-assign-role", "topics", "--agent-id", "agent-1", "--role", "curator"]
    ])
  }

  func testReadOnlyMethodsMapToWikiCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(method: "wiki.list")
    _ = try bridge.call(method: "wiki.validate")
    _ = try bridge.call(method: "wiki.page.status", params: ["route": "/topics"])
    _ = try bridge.call(method: "wiki.page.open", params: ["page": "topics"])
    _ = try bridge.call(method: "wiki.publish.status")
    _ = try bridge.call(method: "wiki.publish-status")

    XCTAssertEqual(recorder.calls, [
      ["list"],
      ["validate"],
      ["page-status", "/topics"],
      ["page-open", "topics"],
      ["publish-status"],
      ["publish-status"]
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

  func testPublishMapsToWikiCoreCommandWithDefaultsAndOverrides() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(
      call: recorder.call,
      defaultWikiEngineDirectory: "/opt/1Context/WikiEngine",
      defaultNodeExecutable: "/usr/bin/env"
    )

    _ = try bridge.call(method: "wiki.publish", params: ["trigger": "rpc-proof", "force": true])
    _ = try bridge.call(
      method: "wiki.publish",
      params: [
        "wikiEngine": "/tmp/WikiEngine",
        "nodeExecutable": "/opt/node/bin/node",
        "trigger": "override"
      ]
    )

    XCTAssertEqual(recorder.calls, [
      ["publish", "--wiki-engine", "/opt/1Context/WikiEngine", "--trigger", "rpc-proof", "--force"],
      ["publish", "--wiki-engine", "/tmp/WikiEngine", "--node", "/opt/node/bin/node", "--trigger", "override"]
    ])
  }

  func testFailedPublishReceiptPassesThroughBridgeAsStructuredResult() throws {
    let receipt: [String: Any] = [
      "schema_version": 1,
      "status": "failed",
      "operation": "wiki.publish",
      "next_action": "repair_wiki_toml",
      "validation": [
        "status": "error",
        "can_publish": false,
        "issues": [
          [
            "code": "invalid_nav_section",
            "severity": "error"
          ]
        ]
      ],
      "repair_hints": ["Run wiki.validate and repair blocking wiki.toml issues before publishing."]
    ]
    let bridge = WikiCoreRPCBridge(call: { arguments in
      XCTAssertEqual(arguments.first, "publish")
      return receipt
    })

    let result = try bridge.call(method: "wiki.publish", params: ["trigger": "structured-failure-proof"])

    XCTAssertEqual(result["operation"] as? String, "wiki.publish")
    XCTAssertEqual(result["status"] as? String, "failed")
    XCTAssertEqual(result["next_action"] as? String, "repair_wiki_toml")
    let validation = result["validation"] as? [String: Any]
    XCTAssertEqual(validation?["status"] as? String, "error")
    XCTAssertEqual(validation?["can_publish"] as? Bool, false)
    let issues = validation?["issues"] as? [[String: Any]]
    XCTAssertEqual(issues?.first?["code"] as? String, "invalid_nav_section")
  }

  func testFileBackedBodyParamsMapToCoreCommands() throws {
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
          "toRoles": ["curator"],
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
        "--to-role",
        "curator",
        "--body-file",
        "/tmp/talk-body.md",
        "--attachment",
        "/tmp/evidence.txt"
      ]
    ])
  }

  func testPageEditDeleteAndRestoreMapToWikiCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "wiki.page.write_body",
      params: [
        "page": "rpc-proof",
        "body": "# Body",
        "expectedSourceSha256": "abc123"
      ]
    )
    _ = try bridge.call(
      method: "wiki.page.patch-body",
      params: [
        "id": "rpc-proof",
        "find": "Body",
        "replace": "Better body",
        "expected_source_sha256": "def456"
      ]
    )
    _ = try bridge.call(method: "wiki.page.delete", params: ["page": "rpc-proof", "mode": "tombstone"])
    _ = try bridge.call(method: "wiki.page.restore", params: ["page": "rpc-proof"])

    XCTAssertEqual(recorder.calls, [
      ["page-write-body", "rpc-proof", "--body", "# Body", "--expected-source-sha256", "abc123"],
      ["page-patch-body", "rpc-proof", "--find", "Body", "--replace", "Better body", "--expected-source-sha256", "def456"],
      ["page-delete", "rpc-proof", "--mode", "tombstone"],
      ["page-restore", "rpc-proof"]
    ])
  }

  func testPageWatchRoleAndListMethodsMapConsumerParamsToCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "wiki.page.watch",
      params: [
        "route": "/topics",
        "agentId": "agent-1",
        "listAddress": "list://topics.watchers",
        "kinds": ["proposal", "question"],
        "ttl": 1200
      ]
    )
    _ = try bridge.call(
      method: "wiki.page-unwatch",
      params: [
        "page": ["id": "topics"],
        "agent": "agent-1",
        "list": "list://topics.watchers",
        "kind": "proposal"
      ]
    )
    _ = try bridge.call(
      method: "wiki.page.assign-role",
      params: [
        "id": "topics",
        "agent_id": "agent-2",
        "roleAddress": "role://topics.reviewer",
        "kind": ["proposal"],
        "ttlSeconds": 600
      ]
    )
    _ = try bridge.call(
      method: "wiki.list-create",
      params: [
        "listAddress": "list://topics.watchers",
        "title": "Topics Watchers",
        "description": "Agents watching Topics.",
        "page": ["id": "topics"],
        "agentId": "agent-2"
      ]
    )
    _ = try bridge.call(
      method: "wiki.list.lists",
      params: [
        "pageId": "topics",
        "address": "list://topics.watchers"
      ]
    )
    _ = try bridge.call(
      method: "wiki.list_status",
      params: [
        "list": "list://topics.watchers",
        "includeArchived": "yes",
        "include_snoozed": 1
      ]
    )
    _ = try bridge.call(method: "wiki.list-members", params: ["address": "list://topics.watchers"])

    XCTAssertEqual(recorder.calls, [
      [
        "page-watch",
        "/topics",
        "--agent-id",
        "agent-1",
        "--list",
        "list://topics.watchers",
        "--kind",
        "proposal",
        "--kind",
        "question",
        "--ttl-seconds",
        "1200"
      ],
      [
        "page-unwatch",
        "topics",
        "--agent-id",
        "agent-1",
        "--list",
        "list://topics.watchers",
        "--kind",
        "proposal"
      ],
      [
        "page-assign-role",
        "topics",
        "--agent-id",
        "agent-2",
        "--role",
        "role://topics.reviewer",
        "--kind",
        "proposal",
        "--ttl-seconds",
        "600"
      ],
      [
        "list-create",
        "--address",
        "list://topics.watchers",
        "--title",
        "Topics Watchers",
        "--description",
        "Agents watching Topics.",
        "--page",
        "topics",
        "--owner",
        "agent-2"
      ],
      ["lists", "--page", "topics", "--address", "list://topics.watchers"],
      ["list-status", "list://topics.watchers", "--include-archived", "--include-snoozed"],
      ["list-members", "list://topics.watchers"]
    ])
  }

  func testMailAndNotifyAliasesMapToWikiCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(method: "wiki.mail-inbox", params: ["recipient": "role://topics.curator"])
    _ = try bridge.call(method: "wiki.mail_read", params: ["threadId": "thread-1"])
    _ = try bridge.call(method: "wiki.mail-mark-all", params: ["message": "msg-1", "state": "done"])
    _ = try bridge.call(
      method: "wiki.mail-claim",
      params: [
        "messageId": "msg-1",
        "recipient": "role://topics.curator",
        "agentId": "agent-1"
      ]
    )
    _ = try bridge.call(
      method: "wiki.mail-subscribe",
      params: [
        "agent": "agent-1",
        "address": "list://topics.watchers",
        "relation": "watcher",
        "kind": ["proposal"],
        "ttl": 300
      ]
    )
    _ = try bridge.call(
      method: "wiki.mail_unsubscribe",
      params: [
        "agentId": "agent-1",
        "address": "list://topics.watchers",
        "relation": "watcher",
        "kinds": ["proposal", "reply"]
      ]
    )
    _ = try bridge.call(method: "wiki.mail-subscriptions", params: ["agentId": "agent-1"])
    _ = try bridge.call(method: "wiki.notify-poll", params: ["agentId": "agent-1"])
    _ = try bridge.call(method: "wiki.notify_ack", params: ["notificationId": "notif-1", "agentId": "agent-1"])

    XCTAssertEqual(recorder.calls, [
      ["mail-inbox", "role://topics.curator"],
      ["mail-read", "--thread-id", "thread-1"],
      ["mail-mark-all", "msg-1", "--state", "done"],
      ["mail-claim", "msg-1", "--recipient", "role://topics.curator", "--agent-id", "agent-1"],
      [
        "mail-subscribe",
        "--agent-id",
        "agent-1",
        "--address",
        "list://topics.watchers",
        "--relation",
        "watcher",
        "--kind",
        "proposal",
        "--ttl-seconds",
        "300"
      ],
      [
        "mail-unsubscribe",
        "--agent-id",
        "agent-1",
        "--address",
        "list://topics.watchers",
        "--relation",
        "watcher",
        "--kind",
        "proposal",
        "--kind",
        "reply"
      ],
      ["mail-subscriptions", "--agent-id", "agent-1"],
      ["notify-poll", "agent-1"],
      ["notify-ack", "notif-1", "--agent-id", "agent-1"]
    ])
  }

  func testCommandStyleAliasesAcceptConsumerTypedParams() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "wiki.list-create",
      params: [
        "address": "list://dogfood.worker-bo",
        "title": "Worker BO Dogfood",
        "owner": "agent://codex/worker-bo"
      ]
    )
    _ = try bridge.call(
      method: "wiki.list-status",
      params: [
        "listAddress": "list://dogfood.worker-bo",
        "includeArchived": true
      ]
    )
    _ = try bridge.call(
      method: "wiki.list-members",
      params: ["list": "list://dogfood.worker-bo"]
    )
    _ = try bridge.call(
      method: "wiki.page-write-body",
      params: [
        "page": "dogfood-worker-bo",
        "body_file": "/tmp/worker-bo-page.md"
      ]
    )
    _ = try bridge.call(
      method: "wiki.page-patch-body",
      params: [
        "page": "dogfood-worker-bo",
        "find_file": "/tmp/worker-bo-find.md",
        "replaceFile": "/tmp/worker-bo-replace.md"
      ]
    )
    _ = try bridge.call(
      method: "wiki.mail-read",
      params: ["message": "msg-worker-bo"]
    )
    _ = try bridge.call(
      method: "wiki.mail-mark",
      params: [
        "messageId": "msg-worker-bo",
        "mailbox": "role://dogfood.curator",
        "state": "done"
      ]
    )
    _ = try bridge.call(
      method: "wiki.mail-unsubscribe",
      params: [
        "agentId": "agent-worker-bo",
        "recipient": "role://dogfood.curator"
      ]
    )
    _ = try bridge.call(
      method: "wiki.notify-ack",
      params: ["notification": "notif-worker-bo", "agent": "agent-worker-bo"]
    )
    _ = try bridge.call(
      method: "wiki.talk-append",
      params: [
        "page": ["route": "/dogfood/worker-bo"],
        "message": [
          "subject": "Nested attachment dogfood",
          "fromAddress": "agent://codex/worker-bo",
          "toRoles": ["curator"],
          "bodyFile": "/tmp/worker-bo-talk.md",
          "attachments": [
            ["source": ["path": "/tmp/worker-bo-evidence.txt"]]
          ]
        ]
      ]
    )

    XCTAssertEqual(recorder.calls, [
      [
        "list-create",
        "--address",
        "list://dogfood.worker-bo",
        "--title",
        "Worker BO Dogfood",
        "--owner",
        "agent://codex/worker-bo"
      ],
      ["list-status", "list://dogfood.worker-bo", "--include-archived"],
      ["list-members", "list://dogfood.worker-bo"],
      ["page-write-body", "dogfood-worker-bo", "--body-file", "/tmp/worker-bo-page.md"],
      [
        "page-patch-body",
        "dogfood-worker-bo",
        "--find-file",
        "/tmp/worker-bo-find.md",
        "--replace-file",
        "/tmp/worker-bo-replace.md"
      ],
      ["mail-read", "--message-id", "msg-worker-bo"],
      [
        "mail-mark",
        "msg-worker-bo",
        "--recipient",
        "role://dogfood.curator",
        "--state",
        "done"
      ],
      [
        "mail-unsubscribe",
        "--agent-id",
        "agent-worker-bo",
        "--address",
        "role://dogfood.curator"
      ],
      ["notify-ack", "notif-worker-bo", "--agent-id", "agent-worker-bo"],
      [
        "talk-append",
        "--page",
        "/dogfood/worker-bo",
        "--subject",
        "Nested attachment dogfood",
        "--from",
        "agent://codex/worker-bo",
        "--to-role",
        "curator",
        "--body-file",
        "/tmp/worker-bo-talk.md",
        "--attachment",
        "/tmp/worker-bo-evidence.txt"
      ]
    ])
  }

  func testAgentTalkMailAndNotifyMethodsMapToWikiCoreCommands() throws {
    let recorder = RecordingCore()
    let bridge = WikiCoreRPCBridge(call: recorder.call)

    _ = try bridge.call(
      method: "wiki.agent.register",
      params: [
        "thread_id": "thread-0",
        "role": "role://topics.curator",
        "capabilities": ["wiki.mail", "wiki.talk"]
      ]
    )
    _ = try bridge.call(
      method: "wiki.agent.identify",
      params: [
        "threadId": "thread-1",
        "roles": ["role://topics.curator", "role://projects.curator"],
        "capability": "wiki.mail",
        "ttlSeconds": 900
      ]
    )
    _ = try bridge.call(method: "wiki.agent.status", params: ["agentId": "agent-1"])
    _ = try bridge.call(method: "wiki.agent.list", params: ["includeStale": "true", "include_retired": 1])
    _ = try bridge.call(
      method: "wiki.agent.inbox",
      params: ["agent": "agent-1", "includeArchived": true, "include_snoozed": true]
    )
    _ = try bridge.call(
      method: "wiki.talk.append",
      params: [
        "page": "topics",
        "kind": "proposal",
        "subject": "Index update",
        "from": "agent://agent-1",
        "toRoles": ["curator"],
        "cc": ["list://wiki.watchers"],
        "bodyMarkdown": "Please review.",
        "attachments": ["/tmp/proof.png"],
        "allowTombstoned": true
      ]
    )
    _ = try bridge.call(method: "wiki.mail.inbox", params: ["recipient": "role://topics.curator"])
    _ = try bridge.call(method: "wiki.mail.read", params: ["messageId": "msg-1"])
    _ = try bridge.call(
      method: "wiki.mail.mark",
      params: [
        "message": "msg-1",
        "recipient": "role://topics.curator",
        "state": "snoozed",
        "snoozedUntil": "2026-05-21T00:00:00Z"
      ]
    )
    _ = try bridge.call(method: "wiki.notify.poll", params: ["agent": "agent-1"])
    _ = try bridge.call(
      method: "wiki.notify.ack",
      params: ["id": "notif-1", "agentId": "agent-1", "state": "delivered"]
    )

    XCTAssertEqual(recorder.calls, [
      [
        "agent-register",
        "--thread-id",
        "thread-0",
        "--role",
        "role://topics.curator",
        "--capability",
        "wiki.mail",
        "--capability",
        "wiki.talk"
      ],
      [
        "agent-identify",
        "--thread-id",
        "thread-1",
        "--role",
        "role://topics.curator",
        "--role",
        "role://projects.curator",
        "--capability",
        "wiki.mail",
        "--ttl-seconds",
        "900"
      ],
      ["agent-status", "agent-1"],
      ["agent-list", "--include-stale", "--include-retired"],
      ["agent-inbox", "agent-1", "--include-archived", "--include-snoozed"],
      [
        "talk-append",
        "--page",
        "topics",
        "--subject",
        "Index update",
        "--from",
        "agent://agent-1",
        "--kind",
        "proposal",
        "--to-role",
        "curator",
        "--cc",
        "list://wiki.watchers",
        "--body",
        "Please review.",
        "--attachment",
        "/tmp/proof.png",
        "--allow-tombstoned"
      ],
      ["mail-inbox", "role://topics.curator"],
      ["mail-read", "--message-id", "msg-1"],
      [
        "mail-mark",
        "msg-1",
        "--recipient",
        "role://topics.curator",
        "--state",
        "snoozed",
        "--until",
        "2026-05-21T00:00:00Z"
      ],
      ["notify-poll", "agent-1"],
      ["notify-ack", "notif-1", "--agent-id", "agent-1", "--state", "delivered"]
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
          "from": "agent://codex/worker-ba",
          "to": ["role://topics.curator"],
          "ccRoles": ["reviewer"],
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
    _ = try bridge.call(
      method: "wiki.talk.append",
      params: [
        "page": "topics",
        "message": [
          "subject": "File-backed talk",
          "fromAddress": "agent://codex/worker-ba",
          "toRoles": ["curator"],
          "bodyFile": "/tmp/talk-body.md"
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
      "--to",
      "role://topics.curator",
      "--cc-role",
      "reviewer",
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
    XCTAssertEqual(recorder.calls[1], [
      "talk-append",
      "--page",
      "topics",
      "--subject",
      "File-backed talk",
      "--from",
      "agent://codex/worker-ba",
      "--to-role",
      "curator",
      "--body-file",
      "/tmp/talk-body.md"
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
            "from": "agent://codex/worker-bl",
            "toRoles": ["curator"]
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
