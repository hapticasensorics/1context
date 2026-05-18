import Foundation
import OneContextPlatform

public enum WikiRenderStatus: String, Codable, Sendable {
  case published
  case failed
}

public struct WikiRenderResult: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var status: WikiRenderStatus
  public var trigger: String
  public var publishedAt: String
  public var sourceSite: String
  public var publishedSite: String
  public var message: String

  public init(
    schemaVersion: Int = 1,
    status: WikiRenderStatus,
    trigger: String,
    publishedAt: String,
    sourceSite: String = "user-wiki://site",
    publishedSite: String = "app-support://wiki-site/current",
    message: String
  ) {
    self.schemaVersion = schemaVersion
    self.status = status
    self.trigger = trigger
    self.publishedAt = publishedAt
    self.sourceSite = sourceSite
    self.publishedSite = publishedSite
    self.message = message
  }
}

public final class WikiRenderCoordinator: @unchecked Sendable {
  private let runtimePaths: RuntimePaths
  private let fileManager: FileManager
  private let rendererConfig: WikiEngineRendererConfig?
  private let now: @Sendable () -> Date
  private let log: @Sendable (String) -> Void
  private let encoder: JSONEncoder

  public init(
    runtimePaths: RuntimePaths,
    rendererConfig: WikiEngineRendererConfig? = nil,
    fileManager: FileManager = .default,
    now: @escaping @Sendable () -> Date = Date.init,
    log: @escaping @Sendable (String) -> Void = { _ in }
  ) {
    self.runtimePaths = runtimePaths
    self.fileManager = fileManager
    self.rendererConfig = rendererConfig
    self.now = now
    self.log = log
    self.encoder = JSONEncoder()
    self.encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
  }

  public var sourceSite: URL {
    runtimePaths.userWikiSiteDirectory
  }

  public var appSupportWikiSiteDirectory: URL {
    runtimePaths.appSupportDirectory.appendingPathComponent("wiki-site", isDirectory: true)
  }

  public var currentSite: URL {
    appSupportWikiSiteDirectory.appendingPathComponent("current", isDirectory: true)
  }

  public var nextSite: URL {
    appSupportWikiSiteDirectory.appendingPathComponent("next", isDirectory: true)
  }

  public var previousSite: URL {
    appSupportWikiSiteDirectory.appendingPathComponent("previous", isDirectory: true)
  }

  @discardableResult
  public func renderAndPublish(trigger: String) -> WikiRenderResult {
    guard let rendererConfig else {
      return publishExistingSite(trigger: trigger)
    }

    let sourceFingerprint: String
    do {
      sourceFingerprint = try WikiRenderFingerprint.compute(
        root: runtimePaths.userWikiSourceDirectory,
        fileManager: fileManager
      )
      if try existingSourceFingerprint() == sourceFingerprint {
        do {
          let validation = try WikiSiteValidator(fileManager: fileManager).validate(site: sourceSite)
          return publishExistingSite(
            trigger: trigger,
            successMessage: "Skipped renderer; accepted inputs unchanged. Validated \(validation.routeCount) route(s) and \(validation.markdownTwinCount) markdown twin(s), then published existing user-wiki://site."
          )
        } catch {
          log("wiki render skip bypassed trigger=\(trigger) reason=invalid_existing_site error=\(error.localizedDescription)")
        }
      }
    } catch {
      let result = failedResult(trigger: trigger, message: error.localizedDescription)
      try? record(result, under: sourceSite)
      return result
    }

    let staging = runtimePaths.renderCacheDirectory
      .appendingPathComponent("wiki-render-\(UUID().uuidString)", isDirectory: true)
    do {
      try RuntimePermissions.ensurePrivateDirectory(runtimePaths.renderCacheDirectory)
      let summary = try WikiEngineRenderer(config: rendererConfig, fileManager: fileManager)
        .render(runtimePaths: runtimePaths, outputDirectory: staging)
      let validation = try WikiSiteValidator(fileManager: fileManager).validate(site: staging)
      try writeSourceFingerprint(sourceFingerprint, under: staging)
      try replaceSourceSite(with: staging)
      return publishExistingSite(
        trigger: trigger,
        successMessage: "Rendered \(summary.sourceInputs) source page(s), \(summary.talkInputs) talk folder(s), \(validation.routeCount) route(s), and \(validation.markdownTwinCount) markdown twin(s), then published user-wiki://site."
      )
    } catch {
      try? fileManager.removeItem(at: staging)
      let result = failedResult(trigger: trigger, message: error.localizedDescription)
      try? record(result, under: sourceSite)
      log("wiki render failed trigger=\(trigger) error=\(error.localizedDescription)")
      return result
    }
  }

  @discardableResult
  public func publishExistingSite(trigger: String, successMessage: String? = nil) -> WikiRenderResult {
    let timestamp = ISO8601DateFormatter().string(from: now())

    do {
      try RuntimePermissions.ensurePrivateDirectory(sourceSite)
      try RuntimePermissions.ensurePrivateDirectory(appSupportWikiSiteDirectory)

      let validation = try WikiSiteValidator(fileManager: fileManager).validate(site: sourceSite)

      let result = WikiRenderResult(
        status: .published,
        trigger: trigger,
        publishedAt: timestamp,
        message: successMessage
          ?? "Validated \(validation.routeCount) route(s) and \(validation.markdownTwinCount) markdown twin(s), then published existing user-wiki://site to app-support://wiki-site/current."
      )
      try record(result, under: sourceSite)
      try stageSourceSite()
      try promoteStagedSite()
      log("wiki render published trigger=\(trigger)")
      return result
    } catch {
      let result = WikiRenderResult(
        status: .failed,
        trigger: trigger,
        publishedAt: timestamp,
        message: error.localizedDescription
      )
      try? record(result, under: sourceSite)
      log("wiki render failed trigger=\(trigger) error=\(error.localizedDescription)")
      return result
    }
  }

  private func failedResult(trigger: String, message: String) -> WikiRenderResult {
    WikiRenderResult(
      status: .failed,
      trigger: trigger,
      publishedAt: ISO8601DateFormatter().string(from: now()),
      message: message
    )
  }

  private func existingSourceFingerprint() throws -> String? {
    let url = sourceSite
      .appendingPathComponent(".1context", isDirectory: true)
      .appendingPathComponent("source-fingerprint.txt")
    guard fileManager.fileExists(atPath: url.path) else { return nil }
    return try String(contentsOf: url, encoding: .utf8)
      .trimmingCharacters(in: .whitespacesAndNewlines)
  }

  private func writeSourceFingerprint(_ fingerprint: String, under site: URL) throws {
    let metadataDirectory = site.appendingPathComponent(".1context", isDirectory: true)
    try RuntimePermissions.ensurePrivateDirectory(metadataDirectory)
    try RuntimePermissions.writePrivateData(
      Data((fingerprint + "\n").utf8),
      to: metadataDirectory.appendingPathComponent("source-fingerprint.txt")
    )
  }

  private func record(_ result: WikiRenderResult, under site: URL) throws {
    let metadataDirectory = site.appendingPathComponent(".1context", isDirectory: true)
    try RuntimePermissions.ensurePrivateDirectory(metadataDirectory)
    let stateData = try encoder.encode(result)
    try RuntimePermissions.writePrivateData(stateData, to: metadataDirectory.appendingPathComponent("current-render.json"))

    let lineEncoder = JSONEncoder()
    lineEncoder.outputFormatting = [.sortedKeys]
    let eventLine = try String(decoding: lineEncoder.encode(result), as: UTF8.self) + "\n"
    let eventURL = metadataDirectory.appendingPathComponent("render-events.jsonl")
    if fileManager.fileExists(atPath: eventURL.path) {
      let handle = try FileHandle(forWritingTo: eventURL)
      defer {
        try? handle.close()
        RuntimePermissions.ensurePrivateFile(eventURL.path)
      }
      _ = try handle.seekToEnd()
      try handle.write(contentsOf: Data(eventLine.utf8))
    } else {
      try RuntimePermissions.writePrivateData(Data(eventLine.utf8), to: eventURL)
    }
  }

  private func stageSourceSite() throws {
    if fileManager.fileExists(atPath: nextSite.path) {
      try fileManager.removeItem(at: nextSite)
    }
    try fileManager.copyItem(at: sourceSite, to: nextSite)
    try RuntimePermissions.ensurePrivateDirectory(nextSite)
  }

  private func replaceSourceSite(with renderedSite: URL) throws {
    let backup = runtimePaths.renderCacheDirectory
      .appendingPathComponent("wiki-site-backup-\(UUID().uuidString)", isDirectory: true)
    var hasBackup = false

    if fileManager.fileExists(atPath: backup.path) {
      try fileManager.removeItem(at: backup)
    }
    if fileManager.fileExists(atPath: sourceSite.path) {
      try fileManager.moveItem(at: sourceSite, to: backup)
      hasBackup = true
    }

    do {
      try RuntimePermissions.ensurePrivateDirectory(sourceSite.deletingLastPathComponent())
      try fileManager.moveItem(at: renderedSite, to: sourceSite)
      try RuntimePermissions.ensurePrivateDirectory(sourceSite)
      if hasBackup {
        try? fileManager.removeItem(at: backup)
      }
    } catch {
      if hasBackup && !fileManager.fileExists(atPath: sourceSite.path) {
        try? fileManager.moveItem(at: backup, to: sourceSite)
      }
      throw error
    }
  }

  private func promoteStagedSite() throws {
    if fileManager.fileExists(atPath: previousSite.path) {
      try fileManager.removeItem(at: previousSite)
    }
    if fileManager.fileExists(atPath: currentSite.path) {
      try fileManager.moveItem(at: currentSite, to: previousSite)
    }
    try fileManager.moveItem(at: nextSite, to: currentSite)
    try RuntimePermissions.ensurePrivateDirectory(currentSite)
  }
}
