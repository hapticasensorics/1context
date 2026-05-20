import Foundation
import OneContextPlatform

public struct WikiPageLedgerActor: Codable, Equatable, Sendable {
  public var kind: String
  public var name: String?

  public init(kind: String, name: String? = nil) {
    self.kind = kind
    self.name = name
  }
}

public struct WikiPageLedgerEvent: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var event: String
  public var page: String
  public var at: String
  public var actor: WikiPageLedgerActor?
  public var origin: String?
  public var sourceSHA256: String?
  public var templateSHA256: String?
  public var publishFingerprint: String?

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case event
    case page
    case at
    case actor
    case origin
    case sourceSHA256 = "source_sha256"
    case templateSHA256 = "template_sha256"
    case publishFingerprint = "publish_fingerprint"
  }

  public init(
    schemaVersion: Int = 1,
    event: String,
    page: String,
    at: String,
    actor: WikiPageLedgerActor? = nil,
    origin: String? = nil,
    sourceSHA256: String? = nil,
    templateSHA256: String? = nil,
    publishFingerprint: String? = nil
  ) {
    self.schemaVersion = schemaVersion
    self.event = event
    self.page = page
    self.at = at
    self.actor = actor
    self.origin = origin
    self.sourceSHA256 = sourceSHA256
    self.templateSHA256 = templateSHA256
    self.publishFingerprint = publishFingerprint
  }
}

public final class WikiPageLedger: @unchecked Sendable {
  public let url: URL
  private let fileManager: FileManager
  private let encoder: JSONEncoder
  private let decoder: JSONDecoder

  public init(runtimePaths: RuntimePaths, fileManager: FileManager = .default) {
    self.url = runtimePaths.userWikiDirectory
      .appendingPathComponent(".1context", isDirectory: true)
      .appendingPathComponent("page-ledger.jsonl")
    self.fileManager = fileManager
    self.encoder = JSONEncoder()
    self.encoder.outputFormatting = [.sortedKeys]
    self.decoder = JSONDecoder()
  }

  public init(url: URL, fileManager: FileManager = .default) {
    self.url = url
    self.fileManager = fileManager
    self.encoder = JSONEncoder()
    self.encoder.outputFormatting = [.sortedKeys]
    self.decoder = JSONDecoder()
  }

  public func readEvents() throws -> [WikiPageLedgerEvent] {
    guard fileManager.fileExists(atPath: url.path) else { return [] }
    let text = try String(contentsOf: url, encoding: .utf8)
    return try text
      .split(whereSeparator: \.isNewline)
      .map { line in
        try decoder.decode(WikiPageLedgerEvent.self, from: Data(line.utf8))
      }
  }

  public func append(_ event: WikiPageLedgerEvent) throws {
    try RuntimePermissions.ensurePrivateDirectory(url.deletingLastPathComponent())
    let line = try String(decoding: encoder.encode(event), as: UTF8.self) + "\n"
    let data = Data(line.utf8)
    if fileManager.fileExists(atPath: url.path) {
      let handle = try FileHandle(forWritingTo: url)
      defer {
        try? handle.close()
        RuntimePermissions.ensurePrivateFile(url.path)
      }
      _ = try handle.seekToEnd()
      try handle.write(contentsOf: data)
    } else {
      try RuntimePermissions.writePrivateData(data, to: url)
    }
  }
}
