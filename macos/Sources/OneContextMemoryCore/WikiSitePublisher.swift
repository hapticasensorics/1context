import CryptoKit
import Foundation
import OneContextPlatform

public struct WikiSitePublishPaths: Sendable {
  public let current: URL
  public let next: URL
  public let previous: URL

  public init(current: URL, next: URL, previous: URL) {
    self.current = current
    self.next = next
    self.previous = previous
  }
}

public struct WikiSitePublishResult: Codable, Equatable, Sendable {
  public let currentDirectory: String
  public let entrypoint: String
  public let publishedFiles: Int
  public let generatedAt: String

  public init(currentDirectory: String, entrypoint: String, publishedFiles: Int, generatedAt: String) {
    self.currentDirectory = currentDirectory
    self.entrypoint = entrypoint
    self.publishedFiles = publishedFiles
    self.generatedAt = generatedAt
  }
}

public final class WikiSitePublisher {
  private let memoryPaths: MemoryCorePaths
  private let setup: MemoryCoreSetup
  private let adapter: MemoryCoreAdapter
  private let fileManager: FileManager

  public init(
    memoryPaths: MemoryCorePaths = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default
  ) {
    self.memoryPaths = memoryPaths
    self.setup = MemoryCoreSetup(paths: memoryPaths, environment: environment, fileManager: fileManager)
    self.adapter = MemoryCoreAdapter(
      paths: memoryPaths,
      processRunner: MemoryCoreProcessRunner(environment: environment),
      fileManager: fileManager
    )
    self.fileManager = fileManager
  }

  public func hasPublishedSite(at current: URL) -> Bool {
    fileManager.fileExists(atPath: current.appendingPathComponent("index.html").path)
      && fileManager.fileExists(atPath: current.appendingPathComponent("for-you.html").path)
  }

  @discardableResult
  public func publish(paths: WikiSitePublishPaths, refresh: Bool) throws -> WikiSitePublishResult {
    _ = try setup.ensureReady(validateContract: false)
    if refresh || !hasServableWiki() {
      _ = try adapter.run(arguments: ["wiki", "ensure", "--json"])
      _ = try adapter.run(arguments: ["wiki", "render", "for-you", "--no-evidence", "--json"])
    }
    try exportCurrentSite(to: paths)
    return try publishedResult(current: paths.current)
  }

  private func exportCurrentSite(to paths: WikiSitePublishPaths) throws {
    let generated = forYouGeneratedDirectory()
    let latest = generated.appendingPathComponent("latest_for_family.json")
    guard let latestData = try? Data(contentsOf: latest),
      let latestObject = try? JSONSerialization.jsonObject(with: latestData) as? [String: Any],
      let forYou = latestObject["for-you"] as? [String: Any],
      let slug = forYou["slug"] as? String,
      !slug.isEmpty
    else {
      throw WikiSitePublishError.missingRenderableForYou
    }

    let next = paths.next
    try? fileManager.removeItem(at: next)
    try RuntimePermissions.ensurePrivateDirectory(next)
    try copyAllGeneratedPublicFiles(to: next)
    try copyThemeAssets(to: next)
    try copySiteJSON(to: next)
    try RuntimePermissions.ensurePrivateDirectory(next.appendingPathComponent("__1context", isDirectory: true))
    try writeJSON(["status": "ok", "service": "1context-local-web"], to: next.appendingPathComponent("__1context/health"))

    let slugHTML = next.appendingPathComponent("\(slug).html")
    guard fileManager.fileExists(atPath: slugHTML.path) else {
      throw WikiSitePublishError.missingRenderableForYou
    }
    let entrypointHTML = next.appendingPathComponent("your-context.html")
    guard fileManager.fileExists(atPath: entrypointHTML.path) else {
      throw WikiSitePublishError.missingEntrypoint("/your-context")
    }
    try RuntimePermissions.ensurePrivateDirectory(next.appendingPathComponent("your-context", isDirectory: true))
    try RuntimePermissions.ensurePrivateDirectory(next.appendingPathComponent("for-you", isDirectory: true))
    try copyReplacing(entrypointHTML, to: next.appendingPathComponent("index.html"))
    try copyReplacing(entrypointHTML, to: next.appendingPathComponent("your-context/index.html"))
    try copyReplacing(slugHTML, to: next.appendingPathComponent("for-you.html"))
    try copyReplacing(slugHTML, to: next.appendingPathComponent("for-you/index.html"))
    let slugTalkHTML = next.appendingPathComponent("\(slug).talk.html")
    if fileManager.fileExists(atPath: slugTalkHTML.path) {
      try copyReplacing(slugTalkHTML, to: next.appendingPathComponent("for-you.talk.html"))
    }
    try writePublishManifest(siteRoot: next, slug: slug)

    if fileManager.fileExists(atPath: paths.previous.path) {
      try? fileManager.removeItem(at: paths.previous)
    }
    if fileManager.fileExists(atPath: paths.current.path) {
      try fileManager.moveItem(at: paths.current, to: paths.previous)
    }
    try fileManager.moveItem(at: next, to: paths.current)
  }

  private func copyAllGeneratedPublicFiles(to siteRoot: URL) throws {
    for generated in generatedDirectories() {
      try copyGeneratedPublicFiles(from: generated, to: siteRoot)
    }
  }

  private func copyGeneratedPublicFiles(from generated: URL, to siteRoot: URL) throws {
    guard let enumerator = fileManager.enumerator(at: generated, includingPropertiesForKeys: [.isRegularFileKey]) else {
      return
    }
    for case let source as URL in enumerator {
      let resourceValues = try? source.resourceValues(forKeys: [.isRegularFileKey])
      guard resourceValues?.isRegularFile == true, isPublicGeneratedFile(source) else { continue }
      let relative = try source.relativePath(from: generated)
      try copyReplacing(source, to: siteRoot.appendingPathComponent(relative))
    }
  }

  private func copyThemeAssets(to siteRoot: URL) throws {
    let engine = setup.coreDirectory.appendingPathComponent("wiki-engine", isDirectory: true)
    let assets = siteRoot.appendingPathComponent("assets", isDirectory: true)
    try RuntimePermissions.ensurePrivateDirectory(assets)
    try copyReplacing(engine.appendingPathComponent("theme/css/theme.css"), to: assets.appendingPathComponent("theme.css"))
    try copyReplacing(engine.appendingPathComponent("theme/js/enhance.js"), to: assets.appendingPathComponent("enhance.js"))
    let assetSource = engine.appendingPathComponent("theme/assets", isDirectory: true)
    if let enumerator = fileManager.enumerator(at: assetSource, includingPropertiesForKeys: [.isRegularFileKey]) {
      for case let source as URL in enumerator {
        let resourceValues = try? source.resourceValues(forKeys: [.isRegularFileKey])
        guard resourceValues?.isRegularFile == true else { continue }
        let relative = try source.relativePath(from: assetSource)
        try copyReplacing(source, to: assets.appendingPathComponent(relative))
      }
    }
  }

  private func copySiteJSON(to siteRoot: URL) throws {
    let source = setup.coreDirectory.appendingPathComponent("wiki/generated", isDirectory: true)
    let api = siteRoot.appendingPathComponent("api/wiki", isDirectory: true)
    let chat = api.appendingPathComponent("chat", isDirectory: true)
    try RuntimePermissions.ensurePrivateDirectory(api)
    try RuntimePermissions.ensurePrivateDirectory(chat)

    try copyIfPresent(source.appendingPathComponent("site-manifest.json"), to: siteRoot.appendingPathComponent("site-manifest.json"))
    try copyIfPresent(source.appendingPathComponent("content-index.json"), to: siteRoot.appendingPathComponent("content-index.json"))
    try copyIfPresent(source.appendingPathComponent("wiki-stats.json"), to: siteRoot.appendingPathComponent("wiki-stats.json"))
    try copyIfPresent(source.appendingPathComponent("site-manifest.json"), to: api.appendingPathComponent("site.json"))
    try copyIfPresent(source.appendingPathComponent("content-index.json"), to: api.appendingPathComponent("pages.json"))
    try copyIfPresent(source.appendingPathComponent("wiki-stats.json"), to: api.appendingPathComponent("stats.json"))
    try writeJSON(["query": "", "matches": [], "pages": []], to: api.appendingPathComponent("search.json"))
    try writeJSON(["bookmarks": []], to: api.appendingPathComponent("bookmarks.json"))
    try writeJSON([:], to: api.appendingPathComponent("state.json"))
    try writeJSON([:], to: chat.appendingPathComponent("config.json"))
  }

  private func publishedResult(current: URL) throws -> WikiSitePublishResult {
    let files = allFiles(under: current)
    return WikiSitePublishResult(
      currentDirectory: current.path,
      entrypoint: "/your-context",
      publishedFiles: files.count,
      generatedAt: ISO8601DateFormatter().string(from: Date())
    )
  }

  private func writePublishManifest(siteRoot: URL, slug: String) throws {
    let payload: [String: Any] = [
      "schema_version": "wiki.published-site.v1",
      "published_at": ISO8601DateFormatter().string(from: Date()),
      "entrypoint": "/your-context",
      "source_slug": slug,
      "files": allFiles(under: siteRoot).map { $0.path }
    ]
    try writeJSON(payload, to: siteRoot.appendingPathComponent("publish-manifest.json"))
  }

  private func hasServableForYou() -> Bool {
    let generated = forYouGeneratedDirectory()
    let manifest = generated.appendingPathComponent("render-manifest.json")
    let latest = generated.appendingPathComponent("latest_for_family.json")
    let index = generated.appendingPathComponent("for-you-index.json")
    guard fileManager.fileExists(atPath: manifest.path),
      fileManager.fileExists(atPath: latest.path),
      fileManager.fileExists(atPath: index.path),
      let latestData = try? Data(contentsOf: latest),
      let latestObject = try? JSONSerialization.jsonObject(with: latestData) as? [String: Any],
      let forYou = latestObject["for-you"] as? [String: Any],
      let slug = forYou["slug"] as? String,
      !slug.isEmpty,
      fileManager.fileExists(atPath: generated.appendingPathComponent("\(slug).html").path),
      manifestInputsMatch(manifest: manifest)
    else {
      return false
    }
    return true
  }

  private func hasServableWiki() -> Bool {
    guard hasServableForYou() else { return false }
    let families = familyManifestDirectories()
    guard !families.isEmpty else { return false }
    for family in families {
      let manifest = family.appendingPathComponent("generated/render-manifest.json")
      guard fileManager.fileExists(atPath: manifest.path), manifestInputsMatch(manifest: manifest) else {
        return false
      }
    }
    return true
  }

  private func manifestInputsMatch(manifest: URL) -> Bool {
    guard let data = try? Data(contentsOf: manifest),
      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
      let inputs = object["inputs"] as? [[String: Any]]
    else {
      return false
    }
    for input in inputs {
      guard let relativePath = input["path"] as? String,
        let expectedHash = input["sha256"] as? String,
        !relativePath.isEmpty,
        !expectedHash.isEmpty
      else {
        return false
      }
      let inputURL = setup.coreDirectory.appendingPathComponent(relativePath)
      guard let inputData = try? Data(contentsOf: inputURL),
        sha256Hex(inputData) == expectedHash
      else {
        return false
      }
    }
    return true
  }

  private func forYouGeneratedDirectory() -> URL {
    setup.coreDirectory.appendingPathComponent("wiki/menu/10-for-you/10-for-you/generated", isDirectory: true)
  }

  private func familyManifestDirectories() -> [URL] {
    let menu = setup.coreDirectory.appendingPathComponent("wiki/menu", isDirectory: true)
    guard let enumerator = fileManager.enumerator(at: menu, includingPropertiesForKeys: [.isRegularFileKey]) else {
      return []
    }
    return enumerator.compactMap { item in
      guard let url = item as? URL,
        url.lastPathComponent == "family.toml",
        (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true
      else {
        return nil
      }
      return url.deletingLastPathComponent()
    }.sorted { $0.path < $1.path }
  }

  private func generatedDirectories() -> [URL] {
    familyManifestDirectories()
      .map { $0.appendingPathComponent("generated", isDirectory: true) }
      .filter { fileManager.fileExists(atPath: $0.path) }
  }

  private func isPublicGeneratedFile(_ url: URL) -> Bool {
    let name = url.lastPathComponent.lowercased()
    if name == ".gitignore" || name == "render-manifest.json" { return false }
    return !name.contains(".private.") && !name.contains(".internal.")
  }

  private func copyIfPresent(_ source: URL, to destination: URL) throws {
    guard fileManager.fileExists(atPath: source.path) else { return }
    try copyReplacing(source, to: destination)
  }

  private func copyReplacing(_ source: URL, to destination: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(destination.deletingLastPathComponent())
    if source.standardizedFileURL == destination.standardizedFileURL {
      return
    }
    try? fileManager.removeItem(at: destination)
    do {
      try fileManager.copyItem(at: source, to: destination)
    } catch {
      if fileManager.fileExists(atPath: destination.path) {
        try fileManager.removeItem(at: destination)
        try fileManager.copyItem(at: source, to: destination)
      } else {
        throw error
      }
    }
    chmod(destination.path, RuntimePermissions.privateFileMode)
  }

  private func writeJSON(_ payload: [String: Any], to destination: URL) throws {
    try RuntimePermissions.ensurePrivateDirectory(destination.deletingLastPathComponent())
    let data = try JSONSerialization.data(withJSONObject: payload, options: [.prettyPrinted, .sortedKeys])
    try RuntimePermissions.writePrivateData(data + Data("\n".utf8), to: destination)
  }

  private func allFiles(under root: URL) -> [URL] {
    guard let enumerator = fileManager.enumerator(at: root, includingPropertiesForKeys: [.isRegularFileKey]) else {
      return []
    }
    return enumerator.compactMap { item in
      guard let url = item as? URL,
        (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true,
        let relative = try? url.relativePath(from: root)
      else {
        return nil
      }
      return URL(fileURLWithPath: relative)
    }.sorted { $0.path < $1.path }
  }

  private func sha256Hex(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
  }
}

public enum WikiSitePublishError: Error, LocalizedError, Equatable {
  case missingRenderableForYou
  case missingEntrypoint(String)

  public var errorDescription: String? {
    switch self {
    case .missingRenderableForYou:
      return "No renderable For You wiki artifact is available"
    case .missingEntrypoint(let route):
      return "No renderable wiki entrypoint is available for \(route)"
    }
  }
}

private extension URL {
  func relativePath(from base: URL) throws -> String {
    let path = standardizedFileURL.path
    let basePath = base.standardizedFileURL.path
    guard path.hasPrefix(basePath + "/") else {
      throw WikiSitePublishError.missingRenderableForYou
    }
    return String(path.dropFirst(basePath.count + 1))
  }
}
