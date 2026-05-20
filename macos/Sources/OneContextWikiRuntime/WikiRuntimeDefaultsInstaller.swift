import Foundation
import CryptoKit
import OneContextPlatform

public struct WikiRuntimeDefaultsInstallResult: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var status: String
  public var installedAt: String
  public var source: String
  public var packagedManifest: WikiRuntimeDefaultsManifestIdentity?
  public var copied: [String]
  public var preserved: [String]
  public var proposals: [String]

  public init(
    schemaVersion: Int = 1,
    status: String,
    installedAt: String,
    source: String,
    packagedManifest: WikiRuntimeDefaultsManifestIdentity? = nil,
    copied: [String],
    preserved: [String],
    proposals: [String] = []
  ) {
    self.schemaVersion = schemaVersion
    self.status = status
    self.installedAt = installedAt
    self.source = source
    self.packagedManifest = packagedManifest
    self.copied = copied
    self.preserved = preserved
    self.proposals = proposals
  }
}

public struct WikiRuntimeDefaultsManifestIdentity: Codable, Equatable, Sendable {
  public var schemaVersion: String
  public var releaseVersion: String
  public var runtimeDefaultsSourceHash: String
  public var runtimeDefaultsSiteHash: String
  public var wikiEngineHash: String
  public var wikiCoreHash: String?
  public var rendererHash: String?
  public var manifestWriterHash: String?
  public var gitCommit: String?
  public var gitDirty: Bool?
  public var renderStatus: String?
  public var renderRouteCount: Int?
  public var renderMarkdownTwinCount: Int?

  public init(
    schemaVersion: String,
    releaseVersion: String,
    runtimeDefaultsSourceHash: String,
    runtimeDefaultsSiteHash: String,
    wikiEngineHash: String,
    wikiCoreHash: String? = nil,
    rendererHash: String? = nil,
    manifestWriterHash: String? = nil,
    gitCommit: String? = nil,
    gitDirty: Bool? = nil,
    renderStatus: String? = nil,
    renderRouteCount: Int? = nil,
    renderMarkdownTwinCount: Int? = nil
  ) {
    self.schemaVersion = schemaVersion
    self.releaseVersion = releaseVersion
    self.runtimeDefaultsSourceHash = runtimeDefaultsSourceHash
    self.runtimeDefaultsSiteHash = runtimeDefaultsSiteHash
    self.wikiEngineHash = wikiEngineHash
    self.wikiCoreHash = wikiCoreHash
    self.rendererHash = rendererHash
    self.manifestWriterHash = manifestWriterHash
    self.gitCommit = gitCommit
    self.gitDirty = gitDirty
    self.renderStatus = renderStatus
    self.renderRouteCount = renderRouteCount
    self.renderMarkdownTwinCount = renderMarkdownTwinCount
  }
}

private struct RuntimeDefaultConflictProposal: Codable, Equatable {
  var schemaVersion: Int = 1
  var kind: String = "wiki.runtime_default_conflict_proposal"
  var status: String = "needs_review"
  var target: String
  var source: String
  var reason: String
  var proposedAction: String
  var packagedDefaultSHA256: String
  var userFileSHA256: String
}

private struct RuntimeDefaultsManifestDocument: Decodable {
  var schemaVersion: String
  var releaseVersion: String
  var sourceControl: SourceControl?
  var hashes: Hashes
  var renderSummary: RenderSummary?
  var renderResult: RenderSummary?

  struct SourceControl: Decodable {
    var gitCommit: String?
    var gitDirty: Bool?

    enum CodingKeys: String, CodingKey {
      case gitCommit = "git_commit"
      case gitDirty = "git_dirty"
    }
  }

  struct Hashes: Decodable {
    var runtimeDefaultsSource: String
    var runtimeDefaultsSite: String
    var wikiEngine: String
    var wikiCore: String?
    var renderer: String?
    var manifestWriter: String?

    enum CodingKeys: String, CodingKey {
      case runtimeDefaultsSource = "runtime_defaults_source"
      case runtimeDefaultsSite = "runtime_defaults_site"
      case wikiEngine = "wiki_engine"
      case wikiCore = "wiki_core"
      case renderer
      case manifestWriter = "manifest_writer"
    }
  }

  struct RenderSummary: Decodable {
    var status: String?
    var routeCount: Int?
    var markdownTwinCount: Int?

    enum CodingKeys: String, CodingKey {
      case status
      case routeCount = "route_count"
      case markdownTwinCount = "markdown_twin_count"
    }
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case releaseVersion = "release_version"
    case sourceControl = "source_control"
    case hashes
    case renderSummary = "render_summary"
    case renderResult = "render_result"
  }
}

public final class WikiRuntimeDefaultsInstaller: @unchecked Sendable {
  private let runtimePaths: RuntimePaths
  private let defaultsRoot: URL?
  private let fileManager: FileManager
  private let now: @Sendable () -> Date
  private let encoder: JSONEncoder

  public init(
    runtimePaths: RuntimePaths,
    defaultsRoot: URL? = WikiRuntimeDefaultsInstaller.discoverDefaultsRoot(),
    fileManager: FileManager = .default,
    now: @escaping @Sendable () -> Date = Date.init
  ) {
    self.runtimePaths = runtimePaths
    self.defaultsRoot = defaultsRoot
    self.fileManager = fileManager
    self.now = now
    self.encoder = JSONEncoder()
    self.encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
  }

  public static func discoverDefaultsRoot(environment: [String: String] = ProcessInfo.processInfo.environment) -> URL? {
    if let path = environment["ONECONTEXT_RUNTIME_DEFAULTS_DIR"], !path.isEmpty {
      return URL(fileURLWithPath: path, isDirectory: true)
    }

    if let resourceURL = Bundle.main.resourceURL {
      let candidate = resourceURL.appendingPathComponent("RuntimeDefaults", isDirectory: true)
      if FileManager.default.fileExists(atPath: candidate.appendingPathComponent("1Context", isDirectory: true).path) {
        return candidate
      }
    }

    guard let executable = Bundle.main.executableURL else {
      return nil
    }
    let contents = executable
      .deletingLastPathComponent()
      .deletingLastPathComponent()
    let candidate = contents
      .appendingPathComponent("Resources", isDirectory: true)
      .appendingPathComponent("RuntimeDefaults", isDirectory: true)
    if FileManager.default.fileExists(atPath: candidate.appendingPathComponent("1Context", isDirectory: true).path) {
      return candidate
    }
    return nil
  }

  @discardableResult
  public func installMissingDefaults() throws -> WikiRuntimeDefaultsInstallResult {
    guard let defaultsRoot else {
      let result = result(status: "missing_defaults", copied: [], preserved: [], source: "none")
      try writeLedger(result)
      return result
    }

    let sourceRoot = defaultsRoot.appendingPathComponent("1Context", isDirectory: true)
    guard fileManager.fileExists(atPath: sourceRoot.path) else {
      let result = result(status: "missing_defaults", copied: [], preserved: [], source: "app-bundle://RuntimeDefaults/1Context")
      try writeLedger(result)
      return result
    }
    let manifestIdentity = packagedManifestIdentity(sourceRoot: sourceRoot)

    try RuntimePermissions.ensurePrivateDirectory(runtimePaths.userContentDirectory)
    var copied: [String] = []
    var preserved: [String] = []
    var proposals: [String] = []

    if !fileManager.fileExists(atPath: runtimePaths.userContentDirectory.path) {
      try RuntimePermissions.ensurePrivateDirectory(runtimePaths.userContentDirectory)
    }

    guard let enumerator = fileManager.enumerator(
      at: sourceRoot,
      includingPropertiesForKeys: [.isDirectoryKey, .isSymbolicLinkKey],
      options: [],
      errorHandler: nil
    ) else {
      let result = result(
        status: "failed",
        copied: [],
        preserved: [],
        source: "app-bundle://RuntimeDefaults/1Context",
        packagedManifest: manifestIdentity
      )
      try writeLedger(result)
      return result
    }

    for case let sourceURL as URL in enumerator {
      let relative = relativePath(sourceURL, under: sourceRoot)
      if relative.isEmpty { continue }
      let destination = runtimePaths.userContentDirectory.appendingPathComponent(relative)
      let values = try sourceURL.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey])
      if relative == ".1context" || relative.hasPrefix(".1context/") {
        if values.isDirectory == true {
          enumerator.skipDescendants()
        }
        continue
      }
      if values.isSymbolicLink == true {
        preserved.append(relative)
        continue
      }
      if values.isDirectory == true {
        if fileManager.fileExists(atPath: destination.path) {
          preserved.append(relative + "/")
        } else {
          try RuntimePermissions.ensurePrivateDirectory(destination)
          copied.append(relative + "/")
        }
        continue
      }

      if fileManager.fileExists(atPath: destination.path) {
        if try filesDiffer(sourceURL, destination) {
          let proposalPath = try writeConflictProposal(relative: relative, sourceURL: sourceURL, destinationURL: destination)
          proposals.append(proposalPath)
        }
        preserved.append(relative)
        continue
      }
      try RuntimePermissions.ensurePrivateDirectory(destination.deletingLastPathComponent())
      try fileManager.copyItem(at: sourceURL, to: destination)
      RuntimePermissions.ensurePrivateFile(destination.path)
      copied.append(relative)
    }

    let result = result(
      status: proposals.isEmpty ? (copied.isEmpty ? "already_current" : "installed") : "installed_with_conflicts",
      copied: copied.sorted(),
      preserved: preserved.sorted(),
      proposals: proposals.sorted(),
      source: "app-bundle://RuntimeDefaults/1Context",
      packagedManifest: manifestIdentity
    )
    try writeLedger(result)
    return result
  }

  private func result(
    status: String,
    copied: [String],
    preserved: [String],
    proposals: [String] = [],
    source: String,
    packagedManifest: WikiRuntimeDefaultsManifestIdentity? = nil
  ) -> WikiRuntimeDefaultsInstallResult {
    WikiRuntimeDefaultsInstallResult(
      status: status,
      installedAt: ISO8601DateFormatter().string(from: now()),
      source: source,
      packagedManifest: packagedManifest,
      copied: copied,
      preserved: preserved,
      proposals: proposals
    )
  }

  private func writeLedger(_ result: WikiRuntimeDefaultsInstallResult) throws {
    try RuntimePermissions.ensurePrivateDirectory(runtimePaths.appSupportSetupDirectory)
    let url = runtimePaths.appSupportSetupDirectory.appendingPathComponent("runtime-defaults-install.json")
    let data = try encoder.encode(result)
    try RuntimePermissions.writePrivateData(data, to: url)
  }

  private func packagedManifestIdentity(sourceRoot: URL) -> WikiRuntimeDefaultsManifestIdentity? {
    let manifestURL = sourceRoot
      .appendingPathComponent(".1context", isDirectory: true)
      .appendingPathComponent("runtime-defaults-manifest.json")
    guard let data = try? Data(contentsOf: manifestURL),
      let manifest = try? JSONDecoder().decode(RuntimeDefaultsManifestDocument.self, from: data)
    else {
      return nil
    }
    return WikiRuntimeDefaultsManifestIdentity(
      schemaVersion: manifest.schemaVersion,
      releaseVersion: manifest.releaseVersion,
      runtimeDefaultsSourceHash: manifest.hashes.runtimeDefaultsSource,
      runtimeDefaultsSiteHash: manifest.hashes.runtimeDefaultsSite,
      wikiEngineHash: manifest.hashes.wikiEngine,
      wikiCoreHash: manifest.hashes.wikiCore,
      rendererHash: manifest.hashes.renderer,
      manifestWriterHash: manifest.hashes.manifestWriter,
      gitCommit: manifest.sourceControl?.gitCommit,
      gitDirty: manifest.sourceControl?.gitDirty,
      renderStatus: (manifest.renderSummary ?? manifest.renderResult)?.status,
      renderRouteCount: (manifest.renderSummary ?? manifest.renderResult)?.routeCount,
      renderMarkdownTwinCount: (manifest.renderSummary ?? manifest.renderResult)?.markdownTwinCount
    )
  }

  private func relativePath(_ url: URL, under root: URL) -> String {
    let rootPath = root.standardizedFileURL.path
    let path = url.standardizedFileURL.path
    guard path.hasPrefix(rootPath + "/") else {
      return ""
    }
    return String(path.dropFirst(rootPath.count + 1))
  }

  private func filesDiffer(_ lhs: URL, _ rhs: URL) throws -> Bool {
    try Data(contentsOf: lhs) != Data(contentsOf: rhs)
  }

  private func writeConflictProposal(relative: String, sourceURL: URL, destinationURL: URL) throws -> String {
    let proposal = RuntimeDefaultConflictProposal(
      target: "1Context/\(relative)",
      source: "app-bundle://RuntimeDefaults/1Context/\(relative)",
      reason: "Packaged runtime default differs from an existing user-owned file.",
      proposedAction: "Review the packaged default and merge intentionally; the installer preserved the user file.",
      packagedDefaultSHA256: try sha256(sourceURL),
      userFileSHA256: try sha256(destinationURL)
    )
    let relativeProposalPath = "context-engine/proposals/wiki/runtime-defaults/\(proposalFileName(relative))"
    let proposalURL = runtimePaths.userContentDirectory.appendingPathComponent(relativeProposalPath)
    try RuntimePermissions.ensurePrivateDirectory(proposalURL.deletingLastPathComponent())
    let data = try encoder.encode(proposal)
    try RuntimePermissions.writePrivateData(data, to: proposalURL)
    return "1Context/\(relativeProposalPath)"
  }

  private func proposalFileName(_ relative: String) -> String {
    let sanitized = relative
      .replacingOccurrences(of: "/", with: "__")
      .replacingOccurrences(of: ":", with: "_")
    return sanitized + ".proposal.json"
  }

  private func sha256(_ url: URL) throws -> String {
    let digest = SHA256.hash(data: try Data(contentsOf: url))
    return digest.map { String(format: "%02x", $0) }.joined()
  }
}
