import CryptoKit
import Foundation

public struct WikiSiteValidationReport: Equatable, Sendable {
  public var routeCount: Int
  public var markdownTwinCount: Int
  public var assetCount: Int

  public init(routeCount: Int, markdownTwinCount: Int, assetCount: Int) {
    self.routeCount = routeCount
    self.markdownTwinCount = markdownTwinCount
    self.assetCount = assetCount
  }
}

public enum WikiSiteValidationError: LocalizedError, Equatable {
  case failed(String)

  public var errorDescription: String? {
    switch self {
    case .failed(let message):
      return message
    }
  }
}

public final class WikiSiteValidator: @unchecked Sendable {
  private struct RouteManifest: Decodable {
    var schema_version: String
    var route_count: Int
    var routes: [RouteEntry]
    var assets: [String]
  }

  private struct ContentIndex: Decodable {
    var schema_version: String
    var page_count: Int
    var talk_count: Int
    var markdown_twin_count: Int
    var pages: [RouteEntry]
    var markdown_twins: [MarkdownTwin]
    var export_allowlist: [String]
  }

  private struct RouteEntry: Codable, Equatable {
    var route: String
    var kind: String
    var slug: String
    var title: String?
    var access: String
    var status: String
    var html_path: String
    var route_index_path: String?
    var markdown_path: String
  }

  private struct MarkdownTwin: Decodable {
    var role: String
    var path: String
    var sha256: String
    var bytes: Int
    var content_type: String
    var kind: String
    var route: String?
    var html_path: String?
    var route_index_path: String?
    var slug: String
    var access: String
    var status: String
    var md_url: String
    var talk_enabled: Bool
  }

  private let fileManager: FileManager
  private let decoder: JSONDecoder

  public init(fileManager: FileManager = .default) {
    self.fileManager = fileManager
    self.decoder = JSONDecoder()
  }

  public func validate(site: URL) throws -> WikiSiteValidationReport {
    let routeManifestURL = site.appendingPathComponent(".1context/route-manifest.json")
    let contentIndexURL = site.appendingPathComponent(".1context/content-index.json")
    guard fileManager.fileExists(atPath: routeManifestURL.path) else {
      throw failure("Missing wiki site metadata: .1context/route-manifest.json")
    }
    guard fileManager.fileExists(atPath: contentIndexURL.path) else {
      throw failure("Missing wiki site metadata: .1context/content-index.json")
    }

    let routeManifest = try decode(RouteManifest.self, from: routeManifestURL)
    let contentIndex = try decode(ContentIndex.self, from: contentIndexURL)
    try validate(routeManifest: routeManifest, site: site)
    try validate(contentIndex: contentIndex, routeManifest: routeManifest, site: site)

    return WikiSiteValidationReport(
      routeCount: routeManifest.routes.count,
      markdownTwinCount: contentIndex.markdown_twins.count,
      assetCount: routeManifest.assets.count
    )
  }

  private func validate(routeManifest: RouteManifest, site: URL) throws {
    guard routeManifest.schema_version == "wiki.route-manifest.v1" else {
      throw failure("Invalid route manifest schema: \(routeManifest.schema_version)")
    }
    guard routeManifest.route_count == routeManifest.routes.count else {
      throw failure("Route manifest count mismatch.")
    }
    guard !routeManifest.routes.isEmpty else {
      throw failure("Route manifest contains no routes.")
    }

    var seenRoutes = Set<String>()
    for route in routeManifest.routes {
      try validate(route: route, site: site)
      guard seenRoutes.insert(route.route).inserted else {
        throw failure("Duplicate wiki route: \(route.route)")
      }
    }

    for asset in routeManifest.assets {
      let url = try existingRelativeFile(site: site, path: asset, label: "asset")
      guard !url.hasDirectoryPath else {
        throw failure("Wiki asset path is a directory: \(asset)")
      }
    }
  }

  private func validate(contentIndex: ContentIndex, routeManifest: RouteManifest, site: URL) throws {
    guard contentIndex.schema_version == "wiki.content-index.v1" else {
      throw failure("Invalid content index schema: \(contentIndex.schema_version)")
    }
    guard contentIndex.page_count == contentIndex.pages.filter({ $0.kind == "page" }).count else {
      throw failure("Content index page count mismatch.")
    }
    guard contentIndex.talk_count == contentIndex.pages.filter({ $0.kind == "talk" }).count else {
      throw failure("Content index talk count mismatch.")
    }
    guard contentIndex.markdown_twin_count == contentIndex.markdown_twins.count else {
      throw failure("Content index markdown twin count mismatch.")
    }
    guard contentIndex.pages == routeManifest.routes else {
      throw failure("Content index routes do not match route manifest.")
    }

    try validateExportAllowlist(contentIndex.export_allowlist)

    var routesByMarkdown: [String: RouteEntry] = [:]
    for route in routeManifest.routes {
      guard routesByMarkdown[route.markdown_path] == nil else {
        throw failure("Duplicate markdown route target: \(route.markdown_path)")
      }
      routesByMarkdown[route.markdown_path] = route
    }
    for twin in contentIndex.markdown_twins {
      try validate(twin: twin, site: site, routesByMarkdown: routesByMarkdown)
    }
    for route in routeManifest.routes {
      guard contentIndex.markdown_twins.contains(where: { $0.path == route.markdown_path }) else {
        throw failure("Route has no markdown twin: \(route.route)")
      }
    }
  }

  private func validate(route: RouteEntry, site: URL) throws {
    guard route.route.hasPrefix("/"), !route.route.contains("//"), !route.route.contains("..") else {
      throw failure("Invalid wiki route: \(route.route)")
    }
    guard route.kind == "page" || route.kind == "talk" else {
      throw failure("Invalid wiki route kind for \(route.route): \(route.kind)")
    }
    try validateAccess(route.access, label: "route \(route.route)")

    let html = try existingRelativeFile(site: site, path: route.html_path, label: "route html")
    _ = try existingRelativeFile(site: site, path: route.markdown_path, label: "route markdown")
    if let routeIndexPath = route.route_index_path {
      _ = try existingRelativeFile(site: site, path: routeIndexPath, label: "route index")
    }

    let htmlText = try String(contentsOf: html, encoding: .utf8)
    guard htmlText.contains("data-tier=\"\(route.access)\"") else {
      throw failure("Route \(route.route) is missing access label data-tier=\"\(route.access)\".")
    }
    if route.kind == "talk" {
      guard route.route.hasSuffix("/talk"), route.markdown_path.hasSuffix(".talk.md") else {
        throw failure("Talk route has mismatched route or markdown path: \(route.route)")
      }
    }
  }

  private func validate(twin: MarkdownTwin, site: URL, routesByMarkdown: [String: RouteEntry]) throws {
    guard twin.kind == "page" || twin.kind == "talk" else {
      throw failure("Invalid markdown twin kind for \(twin.path): \(twin.kind)")
    }
    try validateAccess(twin.access, label: "markdown twin \(twin.path)")
    guard twin.content_type == "text/markdown; charset=utf-8" else {
      throw failure("Invalid markdown twin content type for \(twin.path).")
    }

    let markdownURL = try existingRelativeFile(site: site, path: twin.path, label: "markdown twin")
    let data = try Data(contentsOf: markdownURL)
    guard data.count == twin.bytes else {
      throw failure("Markdown twin byte count mismatch for \(twin.path).")
    }
    guard sha256Hex(data) == twin.sha256 else {
      throw failure("Markdown twin sha256 mismatch for \(twin.path).")
    }

    if let htmlPath = twin.html_path {
      _ = try existingRelativeFile(site: site, path: htmlPath, label: "markdown twin html")
    }
    if let routeIndexPath = twin.route_index_path {
      _ = try existingRelativeFile(site: site, path: routeIndexPath, label: "markdown twin route index")
    }
    if let route = twin.route {
      guard routesByMarkdown[twin.path]?.route == route else {
        throw failure("Markdown twin route does not match route manifest for \(twin.path).")
      }
    }
  }

  private func validateExportAllowlist(_ allowlist: [String]) throws {
    guard !allowlist.isEmpty else {
      throw failure("Static export allowlist is empty.")
    }
    let forbidden = [
      "context-engine",
      "runtime-test",
      "source/families",
      "_curator",
      "_conventions",
      "prompts/",
      "observations/",
      "runs/",
      "artifacts/wiki/previews",
      "/Users/",
      "private-fixtures"
    ]
    for value in allowlist {
      for fragment in forbidden where value.contains(fragment) {
        throw failure("Static export allowlist includes forbidden path fragment: \(fragment)")
      }
    }
  }

  private func validateAccess(_ access: String, label: String) throws {
    guard ["private", "shared", "public"].contains(access) else {
      throw failure("Invalid access tier for \(label): \(access)")
    }
  }

  private func existingRelativeFile(site: URL, path: String, label: String) throws -> URL {
    let url = try relativeFile(site: site, path: path, label: label)
    var isDirectory: ObjCBool = false
    guard fileManager.fileExists(atPath: url.path, isDirectory: &isDirectory), !isDirectory.boolValue else {
      throw failure("Missing \(label): \(path)")
    }
    return url
  }

  private func relativeFile(site: URL, path: String, label: String) throws -> URL {
    guard !path.isEmpty, !path.hasPrefix("/") else {
      throw failure("Unsafe absolute \(label) path: \(path)")
    }
    let components = path.split(separator: "/", omittingEmptySubsequences: false).map(String.init)
    guard !components.contains(".."), !components.contains(""), !components.contains(".") else {
      throw failure("Unsafe \(label) path: \(path)")
    }
    return components.reduce(site) { partial, component in
      partial.appendingPathComponent(component)
    }
  }

  private func decode<T: Decodable>(_ type: T.Type, from url: URL) throws -> T {
    do {
      return try decoder.decode(T.self, from: Data(contentsOf: url))
    } catch {
      throw failure("Invalid wiki site JSON at \(url.lastPathComponent): \(error.localizedDescription)")
    }
  }

  private func sha256Hex(_ data: Data) -> String {
    SHA256.hash(data: data).map { String(format: "%02x", $0) }.joined()
  }

  private func failure(_ message: String) -> WikiSiteValidationError {
    WikiSiteValidationError.failed(message)
  }
}
