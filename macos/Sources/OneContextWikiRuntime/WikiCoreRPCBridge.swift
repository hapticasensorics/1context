import Foundation

public final class WikiCoreRPCBridge: @unchecked Sendable {
  private let callCore: ([String]) throws -> [String: Any]
  private let defaultWikiEngineDirectory: String?
  private let defaultNodeExecutable: String?

  public init(
    call: @escaping ([String]) throws -> [String: Any],
    defaultWikiEngineDirectory: String? = nil,
    defaultNodeExecutable: String? = nil
  ) {
    self.callCore = call
    self.defaultWikiEngineDirectory = defaultWikiEngineDirectory
    self.defaultNodeExecutable = defaultNodeExecutable
  }

  public convenience init(
    client: WikiCoreProcessClient,
    rendererConfig: WikiEngineRendererConfig? = WikiEngineRendererConfig.discover()
  ) {
    self.init(
      call: { arguments in
        try client.call(arguments)
      },
      defaultWikiEngineDirectory: rendererConfig?.engineDirectory.path,
      defaultNodeExecutable: rendererConfig?.nodeExecutable.path
    )
  }

  public static func supports(method: String) -> Bool {
    WikiCoreRPCMethod(method) != nil
  }

  public func supports(method: String) -> Bool {
    Self.supports(method: method)
  }

  public func call(method: String, params: [String: Any] = [:]) throws -> [String: Any] {
    guard let wikiMethod = WikiCoreRPCMethod(method) else {
      throw WikiCoreRPCBridgeError.unsupportedMethod(method)
    }

    switch wikiMethod {
    case .list:
      return try callCore(["list"])
    case .validate:
      return try callCore(["validate"])
    case .pageStatus:
      return try callCore(["page-status", try pageReference(params)])
    case .pageOpen:
      return try callCore(["page-open", try pageReference(params)])
    case .pageCreate:
      return try callCore(pageCreateArguments(params))
    case .pageWriteBody:
      return try callCore(pageWriteBodyArguments(params))
    case .pagePatchBody:
      return try callCore(pagePatchBodyArguments(params))
    case .pageDelete:
      return try callCore(pageDeleteArguments(params))
    case .pageRestore:
      return try callCore(["page-restore", try pageReference(params)])
    case .pageWatch:
      return try callCore(pageWatchArguments(params))
    case .pageUnwatch:
      return try callCore(pageUnwatchArguments(params))
    case .pageAssignRole:
      return try callCore(pageAssignRoleArguments(params))
    case .publishStatus:
      return try callCore(["publish-status"])
    case .publish:
      return try callCore(publishArguments(params))
    case .listCreate:
      return try callCore(listCreateArguments(params))
    case .lists:
      return try callCore(listsArguments(params))
    case .listStatus:
      return try callCore(listStatusArguments(params))
    case .listMembers:
      return try callCore(["list-members", try listReference(params)])
    case .agentRegister:
      return try callCore(agentIdentifyArguments("agent-register", params))
    case .agentIdentify:
      return try callCore(agentIdentifyArguments("agent-identify", params))
    case .agentHeartbeat:
      return try callCore(agentHeartbeatArguments(params))
    case .agentRetire:
      return try callCore(agentRetireArguments(params))
    case .agentWhoami:
      return try callCore(agentWhoamiArguments(params))
    case .agentList:
      return try callCore(agentListArguments(params))
    case .agentStatus:
      return try callCore(["agent-status", try agentReference(params)])
    case .agentInbox:
      return try callCore(mailboxArguments("agent-inbox", try agentReference(params), params))
    case .agentClaim:
      return try callCore(["agent-claim", try agentReference(params), try messageReference(params)])
    case .talkAppend:
      return try callCore(talkAppendArguments(params))
    case .mailInbox:
      return try callCore(mailboxArguments("mail-inbox", try recipientReference(params), params))
    case .mailRead:
      return try callCore(mailReadArguments(params))
    case .mailMark:
      return try callCore(mailMarkArguments(params))
    case .mailMarkAll:
      return try callCore(mailMarkAllArguments(params))
    case .mailClaim:
      return try callCore(mailClaimArguments(params))
    case .mailSubscribe:
      return try callCore(mailSubscribeArguments(params))
    case .mailUnsubscribe:
      return try callCore(mailUnsubscribeArguments(params))
    case .mailSubscriptions:
      return try callCore(mailSubscriptionsArguments(params))
    case .notifyPoll:
      return try callCore(["notify-poll", try agentReference(params)])
    case .notifyAck:
      return try callCore(notifyAckArguments(params))
    }
  }

  private func pageCreateArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["page-create", try requiredString(params, keys: ["id", "page", "page_id"])]
    appendOption(&arguments, "--title", string(params, keys: ["title"]))
    appendOption(&arguments, "--route", string(params, keys: ["route"]))
    appendOption(&arguments, "--slug", string(params, keys: ["slug"]))
    appendOption(&arguments, "--family-group", string(params, keys: ["family_group", "familyGroup"]))
    appendOption(&arguments, "--family-group-title", string(params, keys: ["family_group_title", "familyGroupTitle"]))
    appendOption(&arguments, "--family-id", string(params, keys: ["family_id", "familyId"]))
    appendOption(&arguments, "--family-title", string(params, keys: ["family_title", "familyTitle"]))
    appendOption(&arguments, "--type", string(params, keys: ["type", "page_type", "pageType"]))
    appendOption(&arguments, "--template", string(params, keys: ["template"]))
    appendOption(
      &arguments,
      "--talk-conventions-template",
      string(params, keys: ["talk_conventions_template", "talkConventionsTemplate"])
    )
    appendOption(
      &arguments,
      "--talk-curator-template",
      string(params, keys: ["talk_curator_template", "talkCuratorTemplate"])
    )
    appendOption(&arguments, "--summary", string(params, keys: ["summary"]))
    appendOption(&arguments, "--nav-section", string(params, keys: ["nav_section", "navSection"]))
    appendOption(&arguments, "--nav-order", string(params, keys: ["nav_order", "navOrder"]))
    return arguments
  }

  private func pageWriteBodyArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["page-write-body", try pageReference(params)]
    if let body = string(params, keys: ["body", "body_markdown", "bodyMarkdown"]) {
      arguments += ["--body", body]
    } else if let bodyFile = string(params, keys: ["body_file", "bodyFile"]) {
      arguments += ["--body-file", bodyFile]
    } else {
      throw WikiCoreRPCBridgeError.missingParameter("body or bodyFile")
    }
    appendOption(
      &arguments,
      "--expected-source-sha256",
      string(params, keys: ["expected_source_sha256", "expectedSourceSha256"])
    )
    return arguments
  }

  private func pagePatchBodyArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["page-patch-body", try pageReference(params)]
    if let find = string(params, keys: ["find"]) {
      arguments += ["--find", find]
    } else if let findFile = string(params, keys: ["find_file", "findFile"]) {
      arguments += ["--find-file", findFile]
    } else {
      throw WikiCoreRPCBridgeError.missingParameter("find")
    }

    if let replace = string(params, keys: ["replace"]) {
      arguments += ["--replace", replace]
    } else if let replaceFile = string(params, keys: ["replace_file", "replaceFile"]) {
      arguments += ["--replace-file", replaceFile]
    } else {
      throw WikiCoreRPCBridgeError.missingParameter("replace")
    }

    appendOption(
      &arguments,
      "--expected-source-sha256",
      string(params, keys: ["expected_source_sha256", "expectedSourceSha256"])
    )
    return arguments
  }

  private func pageDeleteArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["page-delete", try pageReference(params)]
    appendOption(&arguments, "--mode", string(params, keys: ["mode"]))
    return arguments
  }

  private func pageWatchArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["page-watch", try pageReference(params), "--agent-id", try agentReference(params)]
    appendOption(&arguments, "--list", listString(params))
    appendRepeatedOption(&arguments, "--kind", strings(params, keys: ["kinds", "kind"]))
    appendOption(&arguments, "--ttl-seconds", string(params, keys: ["ttl_seconds", "ttlSeconds", "ttl"]))
    return arguments
  }

  private func pageUnwatchArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["page-unwatch", try pageReference(params), "--agent-id", try agentReference(params)]
    appendOption(&arguments, "--list", listString(params))
    appendRepeatedOption(&arguments, "--kind", strings(params, keys: ["kinds", "kind"]))
    return arguments
  }

  private func pageAssignRoleArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = [
      "page-assign-role",
      try pageReference(params),
      "--agent-id",
      try agentReference(params),
      "--role",
      try roleReference(params)
    ]
    appendRepeatedOption(&arguments, "--kind", strings(params, keys: ["kinds", "kind"]))
    appendOption(&arguments, "--ttl-seconds", string(params, keys: ["ttl_seconds", "ttlSeconds", "ttl"]))
    return arguments
  }

  private func publishArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["publish"]
    appendOption(
      &arguments,
      "--wiki-engine",
      string(params, keys: ["wiki_engine", "wikiEngine", "wiki_engine_directory", "wikiEngineDirectory"])
        ?? defaultWikiEngineDirectory
    )
    appendOption(
      &arguments,
      "--node",
      string(params, keys: ["node", "node_executable", "nodeExecutable"]) ?? defaultNodeArgument()
    )
    appendOption(&arguments, "--trigger", string(params, keys: ["trigger"]))
    if bool(params, keys: ["force"]) {
      arguments.append("--force")
    }
    return arguments
  }

  private func listCreateArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["list-create", "--address", try listReference(params)]
    appendOption(&arguments, "--title", string(params, keys: ["title"]))
    appendOption(&arguments, "--description", string(params, keys: ["description", "summary"]))
    appendOption(&arguments, "--page", optionalPageReference(params))
    appendOption(
      &arguments,
      "--owner",
      string(params, keys: ["owner", "owner_address", "ownerAddress", "agent_id", "agentId", "agent"])
    )
    return arguments
  }

  private func listsArguments(_ params: [String: Any]) -> [String] {
    var arguments = ["lists"]
    appendOption(&arguments, "--page", optionalPageReference(params))
    appendOption(&arguments, "--address", listString(params))
    return arguments
  }

  private func listStatusArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["list-status", try listReference(params)]
    appendBoolFlag(&arguments, "--include-archived", bool(params, keys: ["include_archived", "includeArchived"]))
    appendBoolFlag(&arguments, "--include-snoozed", bool(params, keys: ["include_snoozed", "includeSnoozed"]))
    return arguments
  }

  private func agentIdentifyArguments(_ command: String, _ params: [String: Any]) throws -> [String] {
    var arguments = [command, "--thread-id", try requiredString(params, keys: ["thread_id", "threadId", "thread"])]
    appendRepeatedOption(&arguments, "--role", strings(params, keys: ["roles", "role"]))
    appendRepeatedOption(&arguments, "--capability", strings(params, keys: ["capabilities", "capability"]))
    appendOption(&arguments, "--ttl-seconds", string(params, keys: ["ttl_seconds", "ttlSeconds", "ttl"]))
    return arguments
  }

  private func agentHeartbeatArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["agent-heartbeat", try agentReference(params)]
    appendOption(&arguments, "--ttl-seconds", string(params, keys: ["ttl_seconds", "ttlSeconds", "ttl"]))
    return arguments
  }

  private func agentRetireArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["agent-retire", try agentReference(params)]
    appendOption(&arguments, "--reason", string(params, keys: ["reason"]))
    return arguments
  }

  private func agentWhoamiArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["whoami"]
    appendOption(&arguments, "--thread-id", string(params, keys: ["thread_id", "threadId", "thread"]))
    appendOption(&arguments, "--agent-id", string(params, keys: ["agent_id", "agentId", "agent"]))
    if arguments.count == 1 {
      throw WikiCoreRPCBridgeError.missingParameter("thread_id")
    }
    return arguments
  }

  private func agentListArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["agent-list"]
    appendBoolFlag(&arguments, "--include-stale", bool(params, keys: ["include_stale", "includeStale"]))
    appendBoolFlag(&arguments, "--include-retired", bool(params, keys: ["include_retired", "includeRetired"]))
    return arguments
  }

  private func talkAppendArguments(_ params: [String: Any]) throws -> [String] {
    let message = dictionary(params, keys: ["message"]) ?? [:]
    var arguments = [
      "talk-append",
      "--page",
      try pageReference(params),
      "--subject",
      try requiredString(params, nested: message, keys: ["subject"]),
      "--from",
      try requiredString(params, nested: message, keys: ["from", "from_address", "fromAddress"])
    ]
    appendOption(&arguments, "--kind", string(params, nested: message, keys: ["kind"]))
    appendOption(&arguments, "--thread-id", string(params, nested: message, keys: ["thread_id", "threadId", "thread"]))
    appendOption(&arguments, "--reply-to", string(params, nested: message, keys: ["reply_to", "replyTo"]))
    appendRepeatedOption(&arguments, "--to", strings(params, nested: message, keys: ["to", "recipients"]))
    appendRepeatedOption(&arguments, "--to-role", strings(params, nested: message, keys: ["to_roles", "toRoles", "to_role", "toRole"]))
    appendRepeatedOption(&arguments, "--cc", strings(params, nested: message, keys: ["cc"]))
    appendRepeatedOption(&arguments, "--cc-role", strings(params, nested: message, keys: ["cc_roles", "ccRoles", "cc_role", "ccRole"]))
    if let body = string(params, nested: message, keys: ["body", "body_markdown", "bodyMarkdown"]) {
      arguments += ["--body", body]
    } else if let bodyFile = string(params, nested: message, keys: ["body_file", "bodyFile"]) {
      arguments += ["--body-file", bodyFile]
    } else {
      throw WikiCoreRPCBridgeError.missingParameter("body or bodyFile")
    }
    let attachments = attachmentArguments(params, keys: ["attachments", "attachment"])
    appendAttachmentOptions(
      &arguments,
      attachments.isEmpty ? attachmentArguments(message, keys: ["attachments", "attachment"]) : attachments
    )
    appendBoolFlag(&arguments, "--allow-tombstoned", bool(params, keys: ["allow_tombstoned", "allowTombstoned"]))
    return arguments
  }

  private func mailboxArguments(_ command: String, _ mailbox: String, _ params: [String: Any]) -> [String] {
    var arguments = [command, mailbox]
    appendBoolFlag(&arguments, "--include-archived", bool(params, keys: ["include_archived", "includeArchived"]))
    appendBoolFlag(&arguments, "--include-snoozed", bool(params, keys: ["include_snoozed", "includeSnoozed"]))
    return arguments
  }

  private func mailReadArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["mail-read"]
    appendOption(&arguments, "--message-id", string(params, keys: ["message_id", "messageId", "message"]))
    appendOption(&arguments, "--thread-id", string(params, keys: ["thread_id", "threadId", "thread"]))
    if arguments.count == 1 {
      throw WikiCoreRPCBridgeError.missingParameter("message_id")
    }
    return arguments
  }

  private func mailMarkArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = [
      "mail-mark",
      try messageReference(params),
      "--recipient",
      try recipientReference(params),
      "--state",
      try requiredString(params, keys: ["state"])
    ]
    appendOption(&arguments, "--until", string(params, keys: ["until", "snoozed_until", "snoozedUntil"]))
    return arguments
  }

  private func mailMarkAllArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["mail-mark-all", try messageReference(params), "--state", try requiredString(params, keys: ["state"])]
    appendOption(&arguments, "--until", string(params, keys: ["until", "snoozed_until", "snoozedUntil"]))
    return arguments
  }

  private func mailClaimArguments(_ params: [String: Any]) throws -> [String] {
    [
      "mail-claim",
      try messageReference(params),
      "--recipient",
      try recipientReference(params),
      "--agent-id",
      try agentReference(params)
    ]
  }

  private func mailSubscribeArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = [
      "mail-subscribe",
      "--agent-id",
      try agentReference(params),
      "--address",
      try recipientReference(params)
    ]
    appendOption(&arguments, "--relation", string(params, keys: ["relation"]))
    appendRepeatedOption(&arguments, "--kind", strings(params, keys: ["kinds", "kind"]))
    appendOption(&arguments, "--ttl-seconds", string(params, keys: ["ttl_seconds", "ttlSeconds", "ttl"]))
    return arguments
  }

  private func mailUnsubscribeArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = [
      "mail-unsubscribe",
      "--agent-id",
      try agentReference(params),
      "--address",
      try recipientReference(params)
    ]
    appendOption(&arguments, "--relation", string(params, keys: ["relation"]))
    appendRepeatedOption(&arguments, "--kind", strings(params, keys: ["kinds", "kind"]))
    return arguments
  }

  private func mailSubscriptionsArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["mail-subscriptions"]
    appendOption(&arguments, "--agent-id", string(params, keys: ["agent_id", "agentId", "agent"]))
    appendOption(&arguments, "--address", string(params, keys: ["recipient", "address", "mailbox"]))
    return arguments
  }

  private func notifyAckArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = [
      "notify-ack",
      try requiredString(params, keys: ["notification_id", "notificationId", "notification", "id"]),
      "--agent-id",
      try agentReference(params)
    ]
    appendOption(&arguments, "--state", string(params, keys: ["state"]))
    return arguments
  }

  private func pageReference(_ params: [String: Any]) throws -> String {
    if let value = string(params, keys: ["page", "id", "route", "reference"]) {
      return value
    }
    if let page = dictionary(params, keys: ["page"]) {
      return try requiredString(page, keys: ["id", "page", "route", "reference"])
    }
    throw WikiCoreRPCBridgeError.missingParameter("page")
  }

  private func optionalPageReference(_ params: [String: Any]) -> String? {
    if let value = string(params, keys: ["page", "page_id", "pageId"]) {
      return value
    }
    if let page = dictionary(params, keys: ["page"]) {
      return string(page, keys: ["id", "page", "route", "reference"])
    }
    return nil
  }

  private func agentReference(_ params: [String: Any]) throws -> String {
    try requiredString(params, keys: ["agent_id", "agentId", "agent"])
  }

  private func roleReference(_ params: [String: Any]) throws -> String {
    try requiredString(params, keys: ["role", "role_address", "roleAddress"])
  }

  private func listReference(_ params: [String: Any]) throws -> String {
    if let value = listString(params) {
      return value
    }
    throw WikiCoreRPCBridgeError.missingParameter("address")
  }

  private func listString(_ params: [String: Any]) -> String? {
    string(params, keys: ["address", "list", "list_address", "listAddress"])
  }

  private func messageReference(_ params: [String: Any]) throws -> String {
    try requiredString(params, keys: ["message_id", "messageId", "message"])
  }

  private func recipientReference(_ params: [String: Any]) throws -> String {
    try requiredString(params, keys: ["recipient", "address", "mailbox"])
  }

  private func requiredString(_ params: [String: Any], keys: [String]) throws -> String {
    if let value = string(params, keys: keys) {
      return value
    }
    throw WikiCoreRPCBridgeError.missingParameter(keys[0])
  }

  private func requiredString(_ params: [String: Any], nested: [String: Any], keys: [String]) throws -> String {
    if let value = string(params, nested: nested, keys: keys) {
      return value
    }
    throw WikiCoreRPCBridgeError.missingParameter(keys[0])
  }

  private func string(_ params: [String: Any], nested: [String: Any], keys: [String]) -> String? {
    string(params, keys: keys) ?? string(nested, keys: keys)
  }

  private func string(_ params: [String: Any], keys: [String]) -> String? {
    for key in keys {
      guard let value = params[key] else { continue }
      if let string = value as? String, !string.isEmpty {
        return string
      }
      if let int = value as? Int {
        return String(int)
      }
      if let number = value as? NSNumber {
        return number.stringValue
      }
    }
    return nil
  }

  private func strings(_ params: [String: Any], nested: [String: Any], keys: [String]) -> [String] {
    let values = strings(params, keys: keys)
    if !values.isEmpty {
      return values
    }
    return strings(nested, keys: keys)
  }

  private func strings(_ params: [String: Any], keys: [String]) -> [String] {
    for key in keys {
      guard let value = params[key] else { continue }
      if let values = value as? [String] {
        return values.filter { !$0.isEmpty }
      }
      if let values = value as? [Any] {
        return values.compactMap { item in
          if let string = item as? String, !string.isEmpty {
            return string
          }
          if let int = item as? Int {
            return String(int)
          }
          if let number = item as? NSNumber {
            return number.stringValue
          }
          return nil
        }
      }
      if let string = string(params, keys: [key]) {
        return [string]
      }
    }
    return []
  }

  private func dictionary(_ params: [String: Any], keys: [String]) -> [String: Any]? {
    for key in keys {
      guard let value = params[key] else { continue }
      if let dictionary = value as? [String: Any] {
        return dictionary
      }
      if let dictionary = value as? NSDictionary {
        return dictionary as? [String: Any]
      }
    }
    return nil
  }

  private func attachmentArguments(_ params: [String: Any], keys: [String]) -> [AttachmentArgument] {
    for key in keys {
      guard let value = params[key] else { continue }
      let values = attachmentValues(value, metadataParams: params)
      if !values.isEmpty {
        return values
      }
    }
    return []
  }

  private func attachmentValues(_ value: Any, metadataParams: [String: Any]) -> [AttachmentArgument] {
    let parsed: [AttachmentArgument]
    if let array = value as? [Any] {
      parsed = array.compactMap(attachmentValue)
    } else if let value = attachmentValue(value) {
      parsed = [value]
    } else {
      parsed = []
    }
    return applyingAlignedAttachmentMetadata(parsed, params: metadataParams)
  }

  private func attachmentValue(_ value: Any) -> AttachmentArgument? {
    if let string = value as? String, !string.isEmpty {
      return AttachmentArgument(path: string)
    }
    let dictionary = (value as? [String: Any]) ?? (value as? NSDictionary as? [String: Any])
    if let path = dictionary.flatMap({ string($0, keys: ["path"]) }) {
      return AttachmentArgument(
        path: path,
        filename: dictionary.flatMap { string($0, keys: ["filename", "name"]) },
        caption: dictionary.flatMap { string($0, keys: ["caption"]) },
        altText: dictionary.flatMap { string($0, keys: ["alt_text", "altText", "alt"]) }
      )
    }
    let source = dictionary?["source"]
    if let source = (source as? [String: Any]) ?? (source as? NSDictionary as? [String: Any]) {
      guard let path = string(source, keys: ["path"]) else { return nil }
      return AttachmentArgument(
        path: path,
        filename: dictionary.flatMap { string($0, keys: ["filename", "name"]) },
        caption: dictionary.flatMap { string($0, keys: ["caption"]) },
        altText: dictionary.flatMap { string($0, keys: ["alt_text", "altText", "alt"]) }
      )
    }
    return nil
  }

  private func applyingAlignedAttachmentMetadata(
    _ attachments: [AttachmentArgument],
    params: [String: Any]
  ) -> [AttachmentArgument] {
    guard !attachments.isEmpty else { return attachments }
    let filenames = strings(params, keys: ["attachment_filenames", "attachmentFilenames", "attachment_filename", "attachmentFilename"])
    let captions = strings(params, keys: ["attachment_captions", "attachmentCaptions", "attachment_caption", "attachmentCaption"])
    let altTexts = strings(params, keys: ["attachment_alt_texts", "attachmentAltTexts", "attachment_alts", "attachmentAlts", "attachment_alt", "attachmentAlt"])
    if filenames.isEmpty && captions.isEmpty && altTexts.isEmpty {
      return attachments
    }
    return attachments.enumerated().map { index, attachment in
      AttachmentArgument(
        path: attachment.path,
        filename: attachment.filename ?? filenames[safe: index],
        caption: attachment.caption ?? captions[safe: index],
        altText: attachment.altText ?? altTexts[safe: index]
      )
    }
  }

  private func bool(_ params: [String: Any], keys: [String]) -> Bool {
    for key in keys {
      guard let value = params[key] else { continue }
      if let bool = value as? Bool {
        return bool
      }
      if let number = value as? NSNumber {
        return number.boolValue
      }
      if let string = value as? String {
        let normalized = string.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        if ["1", "true", "yes", "on"].contains(normalized) {
          return true
        }
        if ["0", "false", "no", "off"].contains(normalized) {
          return false
        }
      }
    }
    return false
  }

  private func defaultNodeArgument() -> String? {
    guard let defaultNodeExecutable, !defaultNodeExecutable.isEmpty else {
      return nil
    }
    if URL(fileURLWithPath: defaultNodeExecutable).lastPathComponent == "env" {
      return nil
    }
    return defaultNodeExecutable
  }

  private func appendOption(_ arguments: inout [String], _ flag: String, _ value: String?) {
    guard let value, !value.isEmpty else { return }
    arguments += [flag, value]
  }

  private func appendRepeatedOption(_ arguments: inout [String], _ flag: String, _ values: [String]) {
    for value in values where !value.isEmpty {
      arguments += [flag, value]
    }
  }

  private func appendBoolFlag(_ arguments: inout [String], _ flag: String, _ present: Bool) {
    if present {
      arguments.append(flag)
    }
  }

  private func appendAttachmentOptions(_ arguments: inout [String], _ attachments: [AttachmentArgument]) {
    for attachment in attachments {
      arguments += ["--attachment", attachment.path]
      appendOption(&arguments, "--attachment-filename", attachment.filename)
      appendOption(&arguments, "--attachment-caption", attachment.caption)
      appendOption(&arguments, "--attachment-alt", attachment.altText)
    }
  }
}

private struct AttachmentArgument {
  let path: String
  let filename: String?
  let caption: String?
  let altText: String?

  init(path: String, filename: String? = nil, caption: String? = nil, altText: String? = nil) {
    self.path = path
    self.filename = filename
    self.caption = caption
    self.altText = altText
  }
}

private extension Array where Element == String {
  subscript(safe index: Int) -> String? {
    indices.contains(index) ? self[index] : nil
  }
}

private enum WikiCoreRPCMethod {
  case list
  case validate
  case pageStatus
  case pageOpen
  case pageCreate
  case pageWriteBody
  case pagePatchBody
  case pageDelete
  case pageRestore
  case pageWatch
  case pageUnwatch
  case pageAssignRole
  case publishStatus
  case publish
  case listCreate
  case lists
  case listStatus
  case listMembers
  case agentRegister
  case agentIdentify
  case agentHeartbeat
  case agentRetire
  case agentWhoami
  case agentList
  case agentStatus
  case agentInbox
  case agentClaim
  case talkAppend
  case mailInbox
  case mailRead
  case mailMark
  case mailMarkAll
  case mailClaim
  case mailSubscribe
  case mailUnsubscribe
  case mailSubscriptions
  case notifyPoll
  case notifyAck

  init?(_ method: String) {
    switch method {
    case "wiki.list":
      self = .list
    case "wiki.validate":
      self = .validate
    case "wiki.page.status", "wiki.page-status", "wiki.page_status":
      self = .pageStatus
    case "wiki.page.open", "wiki.page-open", "wiki.page_open":
      self = .pageOpen
    case "wiki.page.create", "wiki.page-create", "wiki.page_create":
      self = .pageCreate
    case "wiki.page.write_body", "wiki.page.write-body", "wiki.page-write-body", "wiki.page_write_body":
      self = .pageWriteBody
    case "wiki.page.patch_body", "wiki.page.patch-body", "wiki.page-patch-body", "wiki.page_patch_body":
      self = .pagePatchBody
    case "wiki.page.delete", "wiki.page-delete", "wiki.page_delete":
      self = .pageDelete
    case "wiki.page.restore", "wiki.page-restore", "wiki.page_restore":
      self = .pageRestore
    case "wiki.page.watch", "wiki.page-watch", "wiki.page_watch":
      self = .pageWatch
    case "wiki.page.unwatch", "wiki.page-unwatch", "wiki.page_unwatch":
      self = .pageUnwatch
    case "wiki.page.assign_role", "wiki.page.assign-role", "wiki.page.assignRole", "wiki.page-assign-role", "wiki.page_assign_role":
      self = .pageAssignRole
    case "wiki.publish.status", "wiki.publish-status":
      self = .publishStatus
    case "wiki.publish":
      self = .publish
    case "wiki.list.create", "wiki.list-create", "wiki.list_create":
      self = .listCreate
    case "wiki.lists", "wiki.list.lists":
      self = .lists
    case "wiki.list.status", "wiki.list-status", "wiki.list_status":
      self = .listStatus
    case "wiki.list.members", "wiki.list-members", "wiki.list_members":
      self = .listMembers
    case "wiki.agent.register":
      self = .agentRegister
    case "wiki.agent.identify":
      self = .agentIdentify
    case "wiki.agent.heartbeat":
      self = .agentHeartbeat
    case "wiki.agent.retire":
      self = .agentRetire
    case "wiki.agent.whoami":
      self = .agentWhoami
    case "wiki.agent.list":
      self = .agentList
    case "wiki.agent.status":
      self = .agentStatus
    case "wiki.agent.inbox":
      self = .agentInbox
    case "wiki.agent.claim":
      self = .agentClaim
    case "wiki.talk.append", "wiki.talk-append", "wiki.talk_append":
      self = .talkAppend
    case "wiki.mail.inbox", "wiki.mail-inbox", "wiki.mail_inbox":
      self = .mailInbox
    case "wiki.mail.read", "wiki.mail-read", "wiki.mail_read":
      self = .mailRead
    case "wiki.mail.mark", "wiki.mail-mark", "wiki.mail_mark":
      self = .mailMark
    case "wiki.mail.mark_all", "wiki.mail.mark-all", "wiki.mail-mark-all", "wiki.mail_mark_all":
      self = .mailMarkAll
    case "wiki.mail.claim", "wiki.mail-claim", "wiki.mail_claim":
      self = .mailClaim
    case "wiki.mail.subscribe", "wiki.mail-subscribe", "wiki.mail_subscribe":
      self = .mailSubscribe
    case "wiki.mail.unsubscribe", "wiki.mail-unsubscribe", "wiki.mail_unsubscribe":
      self = .mailUnsubscribe
    case "wiki.mail.subscriptions", "wiki.mail-subscriptions", "wiki.mail_subscriptions":
      self = .mailSubscriptions
    case "wiki.notify.poll", "wiki.notify-poll", "wiki.notify_poll":
      self = .notifyPoll
    case "wiki.notify.ack", "wiki.notify-ack", "wiki.notify_ack":
      self = .notifyAck
    default:
      return nil
    }
  }
}

public enum WikiCoreRPCBridgeError: Error, LocalizedError {
  case missingParameter(String)
  case unsupportedMethod(String)

  public var errorDescription: String? {
    switch self {
    case let .missingParameter(name):
      return "wiki RPC requires params.\(name)"
    case let .unsupportedMethod(method):
      return "unsupported wiki RPC method: \(method)"
    }
  }
}
