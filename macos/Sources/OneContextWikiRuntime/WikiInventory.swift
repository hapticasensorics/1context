import CryptoKit
import Foundation
import OneContextPlatform

public enum WikiInventoryPageKind: String, Codable, Equatable, Sendable {
  case sourcePage = "source_page"
  case generatedPage = "generated_page"
  case alias
}

public enum WikiInventoryPageState: String, Codable, Equatable, Sendable {
  case rendered
  case needsPublish = "needs_publish"
  case sourceMissing = "source_missing"
  case talkMissing = "talk_missing"
  case invalid
  case tombstoned
  case orphanSource = "orphan_source"
  case disabled
  case generated
}

public enum WikiInventoryContentState: String, Codable, Equatable, Sendable {
  case templateUnedited = "template_unedited"
  case edited
  case generated
  case missingSource = "missing_source"
  case tombstoned
  case unknown
}

public struct WikiInventoryFlags: Codable, Equatable, Sendable {
  public var configured: Bool
  public var enabled: Bool
  public var sourceBacked: Bool
  public var rendered: Bool
  public var stale: Bool
  public var tombstoned: Bool
  public var talkReady: Bool
  public var templateDerived: Bool
  public var runtimeDefault: Bool
  public var customCreated: Bool
  public var userEdited: Bool

  enum CodingKeys: String, CodingKey {
    case configured
    case enabled
    case sourceBacked = "source_backed"
    case rendered
    case stale
    case tombstoned
    case talkReady = "talk_ready"
    case templateDerived = "template_derived"
    case runtimeDefault = "runtime_default"
    case customCreated = "custom_created"
    case userEdited = "user_edited"
  }
}

public struct WikiInventoryHandles: Codable, Equatable, Sendable {
  public var source: String?
  public var talk: String?
  public var curator: String?
  public var conventions: String?
  public var published: String?
}

public struct WikiInventoryValidationSummary: Codable, Equatable, Sendable {
  public var status: String
  public var issueCount: Int
  public var blockingCount: Int
  public var warningCount: Int
  public var highestSeverity: String?

  enum CodingKeys: String, CodingKey {
    case status
    case issueCount = "issue_count"
    case blockingCount = "blocking_count"
    case warningCount = "warning_count"
    case highestSeverity = "highest_severity"
  }
}

public struct WikiInventoryPage: Codable, Equatable, Sendable {
  public var id: String
  public var title: String
  public var route: String
  public var type: String
  public var collection: String
  public var kind: WikiInventoryPageKind
  public var state: WikiInventoryPageState
  public var contentState: WikiInventoryContentState
  public var origin: String
  public var templateState: String
  public var dirtySincePublish: Bool
  public var talkState: String
  public var flags: WikiInventoryFlags
  public var handles: WikiInventoryHandles
  public var validation: WikiInventoryValidationSummary
  public var allowedActions: [String]
  public var nextAction: String

  enum CodingKeys: String, CodingKey {
    case id
    case title
    case route
    case type
    case collection
    case kind
    case state
    case contentState = "content_state"
    case origin
    case templateState = "template_state"
    case dirtySincePublish = "dirty_since_publish"
    case talkState = "talk_state"
    case flags
    case handles
    case validation
    case allowedActions = "allowed_actions"
    case nextAction = "next_action"
  }
}

public struct WikiInventory: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var pages: [WikiInventoryPage]
  public var publishFingerprint: String?

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case pages
    case publishFingerprint = "publish_fingerprint"
  }
}

public final class WikiInventoryCompiler: @unchecked Sendable {
  private let runtimePaths: RuntimePaths
  private let fileManager: FileManager

  public init(runtimePaths: RuntimePaths, fileManager: FileManager = .default) {
    self.runtimePaths = runtimePaths
    self.fileManager = fileManager
  }

  public func compile() throws -> WikiInventory {
    let pageRecords = try parsePageRecords()
    let ledgerEvents = try WikiPageLedger(runtimePaths: runtimePaths, fileManager: fileManager).readEvents()
    let currentFingerprint = try? WikiRenderFingerprint.compute(root: runtimePaths.userWikiSourceDirectory, fileManager: fileManager)
    let publishedFingerprint = try readPublishedFingerprint()
    let stale = currentFingerprint != nil && publishedFingerprint != nil && currentFingerprint != publishedFingerprint

    let pages = pageRecords.map { record in
      buildPage(
        from: record,
        ledgerEvents: ledgerEvents.filter { $0.page == record.id },
        stale: stale
      )
    }
    return WikiInventory(schemaVersion: 1, pages: pages, publishFingerprint: currentFingerprint)
  }

  private func buildPage(
    from record: WikiPageRecord,
    ledgerEvents: [WikiPageLedgerEvent],
    stale: Bool
  ) -> WikiInventoryPage {
    let source = sourceURL(for: record)
    let tombstone = source.deletingPathExtension().appendingPathExtension("tombstone.toml")
    let talk = talkURL(for: record)
    let sourceExists = fileManager.fileExists(atPath: source.path)
    let tombstoned = fileManager.fileExists(atPath: tombstone.path)
    let rendered = routeExists(record.route)
    let talkReady = talkReady(talk)
    let origin = latestOrigin(from: ledgerEvents, fallback: record.origin)
    let templateBaseline = ledgerEvents.last { $0.event == "template.baseline" }?.sourceSHA256
    let sourceHash = sourceExists ? (try? sha256(url: source)) : nil
    let templateDerived = pageOriginIsTemplateDerived(origin)
    let contentState = contentState(
      sourceExists: sourceExists,
      tombstoned: tombstoned,
      templateBaseline: templateBaseline,
      sourceHash: sourceHash
    )
    let userEdited = contentState == .edited
    let state = pageState(
      enabled: record.enabled,
      sourceExists: sourceExists,
      tombstoned: tombstoned,
      talkReady: talkReady,
      rendered: rendered,
      stale: stale
    )
    let issueCount = issueCount(for: state, rendered: rendered)

    return WikiInventoryPage(
      id: record.id,
      title: record.title,
      route: record.route,
      type: record.type,
      collection: record.familyGroup,
      kind: .sourcePage,
      state: state,
      contentState: contentState,
      origin: origin,
      templateState: templateState(templateDerived: templateDerived, contentState: contentState),
      dirtySincePublish: stale,
      talkState: talkReady ? "ready" : "missing",
      flags: WikiInventoryFlags(
        configured: true,
        enabled: record.enabled,
        sourceBacked: sourceExists,
        rendered: rendered,
        stale: stale,
        tombstoned: tombstoned,
        talkReady: talkReady,
        templateDerived: templateDerived,
        runtimeDefault: origin == "runtime_default",
        customCreated: origin == "created_from_template",
        userEdited: userEdited
      ),
      handles: WikiInventoryHandles(
        source: "user-wiki://page/\(record.id)/source",
        talk: "user-wiki://page/\(record.id)/talk",
        curator: "user-wiki://page/\(record.id)/curator",
        conventions: "user-wiki://page/\(record.id)/conventions",
        published: "app-support://wiki\(record.route)"
      ),
      validation: WikiInventoryValidationSummary(
        status: issueCount == 0 ? "ok" : "warning",
        issueCount: issueCount,
        blockingCount: state == .invalid || state == .sourceMissing ? issueCount : 0,
        warningCount: state == .invalid || state == .sourceMissing ? 0 : issueCount,
        highestSeverity: issueCount == 0 ? nil : (state == .invalid || state == .sourceMissing ? "error" : "warning")
      ),
      allowedActions: allowedActions(for: state, rendered: rendered),
      nextAction: nextAction(for: state, rendered: rendered)
    )
  }

  private func pageState(
    enabled: Bool,
    sourceExists: Bool,
    tombstoned: Bool,
    talkReady: Bool,
    rendered: Bool,
    stale: Bool
  ) -> WikiInventoryPageState {
    if tombstoned { return .tombstoned }
    if !enabled { return .disabled }
    if !sourceExists { return .sourceMissing }
    if !talkReady { return .talkMissing }
    if stale { return .needsPublish }
    if rendered { return .rendered }
    return .needsPublish
  }

  private func contentState(
    sourceExists: Bool,
    tombstoned: Bool,
    templateBaseline: String?,
    sourceHash: String?
  ) -> WikiInventoryContentState {
    if tombstoned { return .tombstoned }
    guard sourceExists else { return .missingSource }
    guard let templateBaseline, let sourceHash else { return .unknown }
    return templateBaseline == sourceHash ? .templateUnedited : .edited
  }

  private func templateState(templateDerived: Bool, contentState: WikiInventoryContentState) -> String {
    guard templateDerived else { return "not_template_backed" }
    switch contentState {
    case .templateUnedited:
      return "template_unedited"
    case .edited:
      return "edited_from_template"
    default:
      return "unknown"
    }
  }

  private func latestOrigin(from events: [WikiPageLedgerEvent], fallback: String) -> String {
    if let origin = events.last(where: { $0.event == "page.created" })?.origin {
      return origin
    }
    return fallback
  }

  private func pageOriginIsTemplateDerived(_ origin: String) -> Bool {
    origin == "created_from_template" || origin == "runtime_default"
  }

  private func issueCount(for state: WikiInventoryPageState, rendered: Bool) -> Int {
    switch state {
    case .rendered, .generated:
      return 0
    case .tombstoned, .disabled:
      return rendered ? 1 : 0
    default:
      return 1
    }
  }

  private func allowedActions(for state: WikiInventoryPageState, rendered: Bool) -> [String] {
    switch state {
    case .sourceMissing:
      return ["create", "validate"]
    case .tombstoned, .disabled:
      return rendered ? ["validate", "publish"] : ["validate"]
    default:
      return ["open", "validate", "publish", "delete"]
    }
  }

  private func nextAction(for state: WikiInventoryPageState, rendered: Bool) -> String {
    switch state {
    case .sourceMissing:
      return "create"
    case .needsPublish, .talkMissing:
      return "publish"
    case .rendered:
      return "open"
    case .tombstoned, .disabled:
      return rendered ? "publish" : "none"
    default:
      return "validate"
    }
  }

  private func sourceURL(for record: WikiPageRecord) -> URL {
    runtimePaths.userWikiSourceDirectory
      .appendingPathComponent("families/\(record.familyGroup)/\(record.familyID)/source", isDirectory: true)
      .appendingPathComponent("\(record.slug).md")
  }

  private func talkURL(for record: WikiPageRecord) -> URL {
    runtimePaths.userWikiSourceDirectory
      .appendingPathComponent("families/\(record.familyGroup)/\(record.familyID)/talk/\(record.slug).talk", isDirectory: true)
  }

  private func talkReady(_ url: URL) -> Bool {
    fileManager.fileExists(atPath: url.appendingPathComponent("_meta.yaml").path)
      && fileManager.fileExists(atPath: url.appendingPathComponent("_conventions.md").path)
      && fileManager.fileExists(atPath: url.appendingPathComponent("_curator.md").path)
  }

  private func routeExists(_ route: String) -> Bool {
    let trimmed = route.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
    if trimmed.isEmpty {
      return fileManager.fileExists(atPath: runtimePaths.userWikiSiteDirectory.appendingPathComponent("index.html").path)
    }
    let html = runtimePaths.userWikiSiteDirectory.appendingPathComponent("\(trimmed).html")
    let index = runtimePaths.userWikiSiteDirectory.appendingPathComponent(trimmed, isDirectory: true).appendingPathComponent("index.html")
    return fileManager.fileExists(atPath: html.path) || fileManager.fileExists(atPath: index.path)
  }

  private func readPublishedFingerprint() throws -> String? {
    let url = runtimePaths.userWikiSiteDirectory
      .appendingPathComponent(".1context", isDirectory: true)
      .appendingPathComponent("source-fingerprint.txt")
    guard fileManager.fileExists(atPath: url.path) else { return nil }
    return try String(contentsOf: url, encoding: .utf8)
      .trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private func sha256(url: URL) throws -> String {
    SHA256.hash(data: try Data(contentsOf: url)).map { String(format: "%02x", $0) }.joined()
  }

  private func parsePageRecords() throws -> [WikiPageRecord] {
    let wikiConfig = runtimePaths.userWikiDirectory.appendingPathComponent("wiki.toml")
    guard fileManager.fileExists(atPath: wikiConfig.path) else { return [] }
    let text = try String(contentsOf: wikiConfig, encoding: .utf8)
    return WikiPageRecord.parse(text)
  }
}

private struct WikiPageRecord {
  var id: String
  var enabled: Bool
  var title: String
  var slug: String
  var route: String
  var familyGroup: String
  var familyID: String
  var type: String
  var origin: String

  static func parse(_ text: String) -> [WikiPageRecord] {
    var pages: [[String: String]] = []
    var current: [String: String]?
    for rawLine in text.split(whereSeparator: \.isNewline) {
      let line = rawLine.trimmingCharacters(in: .whitespaces)
      if line == "[[pages]]" {
        if let current { pages.append(current) }
        current = [:]
        continue
      }
      guard current != nil, !line.isEmpty, !line.hasPrefix("#") else { continue }
      let parts = line.split(separator: "=", maxSplits: 1).map {
        $0.trimmingCharacters(in: .whitespaces)
      }
      guard parts.count == 2 else { continue }
      current?[parts[0]] = unquote(parts[1])
    }
    if let current { pages.append(current) }
    return pages.compactMap(WikiPageRecord.init(values:))
  }

  init?(values: [String: String]) {
    guard let id = values["id"], !id.isEmpty else { return nil }
    self.id = id
    self.enabled = values["enabled"].map { $0 != "false" } ?? true
    self.title = values["title"] ?? id
    self.slug = values["slug"] ?? id
    self.route = values["route"] ?? "/\(self.slug)"
    self.familyGroup = values["family_group"] ?? values["collection"] ?? "pages"
    self.familyID = values["family_id"] ?? self.slug
    self.type = values["type"] ?? "context-page"
    self.origin = values["origin"] ?? "created_from_template"
  }

  private static func unquote(_ value: String) -> String {
    var result = value
    if let commentIndex = result.firstIndex(of: "#") {
      result = String(result[..<commentIndex]).trimmingCharacters(in: .whitespaces)
    }
    if result.hasPrefix("\""), result.hasSuffix("\""), result.count >= 2 {
      result.removeFirst()
      result.removeLast()
    }
    return result
  }
}
