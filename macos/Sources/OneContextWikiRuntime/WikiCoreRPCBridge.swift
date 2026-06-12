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
    case .assetAdd:
      return try callCore(assetAddArguments(params))
    case .assetList:
      return try callCore(["asset-list", try pageReference(params)])
    case .referenceList:
      return try callCore(referenceListArguments(params))
    case .pageDelete:
      return try callCore(pageDeleteArguments(params))
    case .pageRestore:
      return try callCore(["page-restore", try pageReference(params)])
    case .publishStatus:
      return try callCore(["publish-status"])
    case .publish:
      return try callCore(publishArguments(params))
    case .agentIdentify:
      return try callCore(agentIdentifyArguments(params))
    case .agentHeartbeat:
      return try callCore(agentHeartbeatArguments(params))
    case .agentRetire:
      return try callCore(["agent-retire", try requiredString(params, keys: ["agent_id", "agentId", "agent"])])
    case .agentStatus:
      return try callCore(["agent-status", try requiredString(params, keys: ["agent_id", "agentId", "agent"])])
    case .agentStatusByThread:
      return try callCore([
        "agent-status-by-thread",
        try requiredString(params, keys: ["thread_id", "threadId", "thread"])
      ])
    case .agentInbox:
      return try callCore(["agent-inbox", try requiredString(params, keys: ["agent_id", "agentId", "agent"])])
    case .mailOpen:
      return try callCore(mailOpenArguments(params))
    case .mailRecordInjection:
      return try callCore(mailRecordInjectionArguments(params))
    case .mailSend:
      return try callCore(mailSendArguments(params))
    case .mailClaim:
      return try callCore(mailClaimArguments(params))
    case .mailMark:
      return try callCore(mailMarkArguments(params))
    case .mailSnooze:
      return try callCore(mailSnoozeArguments(params))
    case .notifyPoll:
      return try callCore(notifyPollArguments(params))
    case .notifyAck:
      return try callCore(notifyAckArguments(params))
    case .talkAppend:
      return try callCore(talkAppendArguments(params))
    }
  }

  public func identifyAgent(
    threadID: String,
    roles: [String] = [],
    capabilities: [String] = [],
    ttlSeconds: Int? = nil
  ) throws -> [String: Any] {
    var params: [String: Any] = [
      "thread_id": threadID,
      "roles": roles,
      "capabilities": capabilities
    ]
    if let ttlSeconds {
      params["ttl_seconds"] = ttlSeconds
    }
    return try call(method: "wiki.agent.identify", params: params)
  }

  public func heartbeatAgent(agentID: String, ttlSeconds: Int? = nil) throws -> [String: Any] {
    var params: [String: Any] = ["agent_id": agentID]
    if let ttlSeconds {
      params["ttl_seconds"] = ttlSeconds
    }
    return try call(method: "wiki.agent.heartbeat", params: params)
  }

  public func agentStatus(agentID: String) throws -> [String: Any] {
    try call(method: "wiki.agent.status", params: ["agent_id": agentID])
  }

  public func agentStatusByThread(threadID: String) throws -> [String: Any] {
    try call(method: "wiki.agent.status_by_thread", params: ["thread_id": threadID])
  }

  public func retireAgent(agentID: String) throws -> [String: Any] {
    try call(method: "wiki.agent.retire", params: ["agent_id": agentID])
  }

  public func agentInbox(agentID: String) throws -> [String: Any] {
    try call(method: "wiki.agent.inbox", params: ["agent_id": agentID])
  }

  public func openMail(deliveryID: String, agentID: String) throws -> [String: Any] {
    try call(method: "wiki.mail.open", params: ["delivery_id": deliveryID, "agent_id": agentID])
  }

  public func recordMailInjection(
    deliveryID: String,
    agentID: String,
    result: String = "ok",
    itemCount: Int? = nil,
    threadID: String? = nil,
    error: String? = nil
  ) throws -> [String: Any] {
    var params: [String: Any] = [
      "delivery_id": deliveryID,
      "agent_id": agentID,
      "result": result
    ]
    if let itemCount {
      params["item_count"] = itemCount
    }
    if let threadID {
      params["thread_id"] = threadID
    }
    if let error {
      params["error"] = error
    }
    return try call(method: "wiki.mail.record_injection", params: params)
  }

  public func sendMail(params: [String: Any]) throws -> [String: Any] {
    try call(method: "wiki.mail.send", params: params)
  }

  public func claimMail(deliveryID: String, agentID: String) throws -> [String: Any] {
    try call(method: "wiki.mail.claim", params: ["delivery_id": deliveryID, "agent_id": agentID])
  }

  public func markMail(deliveryID: String, agentID: String, state: String) throws -> [String: Any] {
    try call(
      method: "wiki.mail.mark",
      params: ["delivery_id": deliveryID, "agent_id": agentID, "state": state]
    )
  }

  public func snoozeMail(deliveryID: String, agentID: String, until: String) throws -> [String: Any] {
    try call(
      method: "wiki.mail.snooze",
      params: ["delivery_id": deliveryID, "agent_id": agentID, "until": until]
    )
  }

  public func pollNotifications(agentID: String, cursor: String? = nil) throws -> [String: Any] {
    var params: [String: Any] = ["agent_id": agentID]
    if let cursor {
      params["cursor"] = cursor
    }
    return try call(method: "wiki.notify.poll", params: params)
  }

  public func acknowledgeNotification(notificationID: String, agentID: String) throws -> [String: Any] {
    try call(method: "wiki.notify.ack", params: ["notification_id": notificationID, "agent_id": agentID])
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

  private func assetAddArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["asset-add", try pageReference(params), "--file", try requiredString(params, keys: ["file", "file_path", "filePath", "path"])]
    appendOption(&arguments, "--filename", string(params, keys: ["filename", "name"]))
    appendOption(&arguments, "--purpose", string(params, keys: ["purpose"]))
    appendOption(&arguments, "--caption", string(params, keys: ["caption", "label"]))
    appendOption(&arguments, "--alt-text", string(params, keys: ["alt_text", "altText"]))
    return arguments
  }

  private func referenceListArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["reference-list"]
    if let reference = optionalPageReference(params) {
      arguments.append(reference)
    }
    return arguments
  }

  private func pageDeleteArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["page-delete", try pageReference(params)]
    appendOption(&arguments, "--mode", string(params, keys: ["mode"]))
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
    appendOption(&arguments, "--operation-id", string(params, nested: message, keys: ["operation_id", "operationId"]))
    appendOption(&arguments, "--delivery-mode", string(params, nested: message, keys: ["delivery_mode", "deliveryMode"]))
    appendRepeatedOption(&arguments, "--to", strings(params, nested: message, keys: ["to", "recipients"]))
    appendRepeatedOption(&arguments, "--cc", strings(params, nested: message, keys: ["cc"]))
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

  private func agentIdentifyArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["agent-identify", "--thread-id", try requiredString(params, keys: ["thread_id", "threadId"])]
    appendRepeatedOption(&arguments, "--role", strings(params, keys: ["roles", "role"]))
    appendRepeatedOption(&arguments, "--capability", strings(params, keys: ["capabilities", "capability"]))
    appendOption(&arguments, "--ttl-seconds", string(params, keys: ["ttl_seconds", "ttlSeconds"]))
    return arguments
  }

  private func agentHeartbeatArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["agent-heartbeat", try requiredString(params, keys: ["agent_id", "agentId", "agent"])]
    appendOption(&arguments, "--ttl-seconds", string(params, keys: ["ttl_seconds", "ttlSeconds"]))
    return arguments
  }

  private func mailClaimArguments(_ params: [String: Any]) throws -> [String] {
    [
      "mail-claim",
      try requiredString(params, keys: ["delivery_id", "deliveryId", "delivery"]),
      "--agent-id",
      try requiredString(params, keys: ["agent_id", "agentId", "agent"])
    ]
  }

  private func mailOpenArguments(_ params: [String: Any]) throws -> [String] {
    [
      "mail-open",
      try requiredString(params, keys: ["delivery_id", "deliveryId", "delivery"]),
      "--agent-id",
      try requiredString(params, keys: ["agent_id", "agentId", "agent"])
    ]
  }

  private func mailRecordInjectionArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = [
      "mail-record-injection",
      try requiredString(params, keys: ["delivery_id", "deliveryId", "delivery"]),
      "--agent-id",
      try requiredString(params, keys: ["agent_id", "agentId", "agent"])
    ]
    appendOption(&arguments, "--thread-id", string(params, keys: ["thread_id", "threadId", "thread"]))
    appendOption(&arguments, "--result", string(params, keys: ["result"]))
    appendOption(&arguments, "--item-count", string(params, keys: ["item_count", "itemCount"]))
    appendOption(&arguments, "--error", string(params, keys: ["error"]))
    return arguments
  }

  private func mailSendArguments(_ params: [String: Any]) throws -> [String] {
    var mailParams = params
    if string(mailParams, keys: ["delivery_mode", "deliveryMode"]) == nil {
      mailParams["delivery_mode"] = "mail"
    }
    return try talkAppendArguments(mailParams)
  }

  private func mailMarkArguments(_ params: [String: Any]) throws -> [String] {
    [
      "mail-mark",
      try requiredString(params, keys: ["delivery_id", "deliveryId", "delivery"]),
      "--agent-id",
      try requiredString(params, keys: ["agent_id", "agentId", "agent"]),
      "--state",
      try requiredString(params, keys: ["state"])
    ]
  }

  private func mailSnoozeArguments(_ params: [String: Any]) throws -> [String] {
    [
      "mail-snooze",
      try requiredString(params, keys: ["delivery_id", "deliveryId", "delivery"]),
      "--agent-id",
      try requiredString(params, keys: ["agent_id", "agentId", "agent"]),
      "--until",
      try requiredString(params, keys: ["until", "snoozed_until", "snoozedUntil"])
    ]
  }

  private func notifyPollArguments(_ params: [String: Any]) throws -> [String] {
    var arguments = ["notify-poll", try requiredString(params, keys: ["agent_id", "agentId", "agent"])]
    appendOption(&arguments, "--cursor", string(params, keys: ["cursor"]))
    return arguments
  }

  private func notifyAckArguments(_ params: [String: Any]) throws -> [String] {
    [
      "notify-ack",
      try requiredString(params, keys: ["notification_id", "notificationId", "notification"]),
      "--agent-id",
      try requiredString(params, keys: ["agent_id", "agentId", "agent"])
    ]
  }

  private func pageReference(_ params: [String: Any]) throws -> String {
    if let value = optionalPageReference(params) {
      return value
    }
    throw WikiCoreRPCBridgeError.missingParameter("page")
  }

  private func optionalPageReference(_ params: [String: Any]) -> String? {
    if let value = string(params, keys: ["page", "id", "route", "reference", "page_id", "pageId"]) {
      return value
    }
    if let page = dictionary(params, keys: ["page"]) {
      return string(page, keys: ["id", "page", "route", "reference"])
    }
    return nil
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
  case assetAdd
  case assetList
  case referenceList
  case pageDelete
  case pageRestore
  case publishStatus
  case publish
  case agentIdentify
  case agentHeartbeat
  case agentRetire
  case agentStatus
  case agentStatusByThread
  case agentInbox
  case mailOpen
  case mailRecordInjection
  case mailSend
  case mailClaim
  case mailMark
  case mailSnooze
  case notifyPoll
  case notifyAck
  case talkAppend

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
    case "wiki.asset.add", "wiki.asset-add", "wiki.asset_add", "wiki.page.asset_add", "wiki.page.asset-add":
      self = .assetAdd
    case "wiki.asset.list", "wiki.asset-list", "wiki.asset_list", "wiki.page.asset_list", "wiki.page.asset-list":
      self = .assetList
    case "wiki.reference.list", "wiki.reference-list", "wiki.reference_list", "wiki.references":
      self = .referenceList
    case "wiki.page.delete", "wiki.page-delete", "wiki.page_delete":
      self = .pageDelete
    case "wiki.page.restore", "wiki.page-restore", "wiki.page_restore":
      self = .pageRestore
    case "wiki.publish.status", "wiki.publish-status":
      self = .publishStatus
    case "wiki.publish":
      self = .publish
    case "wiki.agent.identify", "wiki.agent-identify", "wiki.agent_identify":
      self = .agentIdentify
    case "wiki.agent.heartbeat", "wiki.agent-heartbeat", "wiki.agent_heartbeat":
      self = .agentHeartbeat
    case "wiki.agent.retire", "wiki.agent-retire", "wiki.agent_retire":
      self = .agentRetire
    case "wiki.agent.status", "wiki.agent-status", "wiki.agent_status":
      self = .agentStatus
    case "wiki.agent.status_by_thread", "wiki.agent-status-by-thread", "wiki.agent_status_by_thread":
      self = .agentStatusByThread
    case "wiki.agent.inbox", "wiki.agent-inbox", "wiki.agent_inbox":
      self = .agentInbox
    case "wiki.mail.open", "wiki.mail-open", "wiki.mail_open":
      self = .mailOpen
    case "wiki.mail.record_injection", "wiki.mail-record-injection", "wiki.mail_record_injection":
      self = .mailRecordInjection
    case "wiki.mail.send", "wiki.mail-send", "wiki.mail_send":
      self = .mailSend
    case "wiki.mail.claim", "wiki.mail-claim", "wiki.mail_claim":
      self = .mailClaim
    case "wiki.mail.mark", "wiki.mail-mark", "wiki.mail_mark":
      self = .mailMark
    case "wiki.mail.snooze", "wiki.mail-snooze", "wiki.mail_snooze":
      self = .mailSnooze
    case "wiki.notify.poll", "wiki.notify-poll", "wiki.notify_poll":
      self = .notifyPoll
    case "wiki.notify.ack", "wiki.notify-ack", "wiki.notify_ack":
      self = .notifyAck
    case "wiki.talk.append", "wiki.talk-append", "wiki.talk_append":
      self = .talkAppend
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
