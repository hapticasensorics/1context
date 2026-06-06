import Foundation
import OneContextPlatform

public struct WikiEngineRendererConfig: Equatable, Sendable {
  public var nodeExecutable: URL
  public var engineDirectory: URL
  public var renderTool: URL

  public init(nodeExecutable: URL, engineDirectory: URL, renderTool: URL) {
    self.nodeExecutable = nodeExecutable
    self.engineDirectory = engineDirectory
    self.renderTool = renderTool
  }

  public static func discover(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    resourceURL: URL? = Bundle.main.resourceURL,
    executableURL: URL? = Bundle.main.executableURL,
    nodeSearchPaths: [String]? = nil
  ) -> WikiEngineRendererConfig? {
    guard let engine = discoverEngineDirectory(
      environment: environment,
      resourceURL: resourceURL,
      executableURL: executableURL
    ) else {
      return nil
    }
    let nodeExecutable = discoverNodeExecutable(environment: environment, nodeSearchPaths: nodeSearchPaths)
    return WikiEngineRendererConfig(
      nodeExecutable: nodeExecutable,
      engineDirectory: engine,
      renderTool: engine.appendingPathComponent("tools/render-site.mjs")
    )
  }

  private static func discoverEngineDirectory(
    environment: [String: String],
    resourceURL: URL?,
    executableURL: URL?
  ) -> URL? {
    if let enginePath = environment["ONECONTEXT_WIKI_ENGINE_DIR"], !enginePath.isEmpty {
      return URL(fileURLWithPath: enginePath, isDirectory: true)
    }

    if let resourceURL {
      let candidate = resourceURL.appendingPathComponent("WikiEngine", isDirectory: true)
      if FileManager.default.fileExists(atPath: candidate.appendingPathComponent("tools/render-site.mjs").path) {
        return candidate
      }
    }

    guard let executableURL else {
      return nil
    }
    let candidate = executableURL
      .deletingLastPathComponent()
      .deletingLastPathComponent()
      .appendingPathComponent("Resources", isDirectory: true)
      .appendingPathComponent("WikiEngine", isDirectory: true)
    if FileManager.default.fileExists(atPath: candidate.appendingPathComponent("tools/render-site.mjs").path) {
      return candidate
    }
    return nil
  }

  private static func discoverNodeExecutable(environment: [String: String], nodeSearchPaths: [String]?) -> URL {
    if let nodePath = environment["ONECONTEXT_NODE"], !nodePath.isEmpty {
      return URL(fileURLWithPath: nodePath)
    }
    for path in candidateNodePaths(environment: environment, nodeSearchPaths: nodeSearchPaths) {
      if FileManager.default.isExecutableFile(atPath: path) {
        return URL(fileURLWithPath: path)
      }
    }
    return URL(fileURLWithPath: "/usr/bin/env")
  }

  private static func candidateNodePaths(environment: [String: String], nodeSearchPaths: [String]?) -> [String] {
    if let nodeSearchPaths {
      return nodeSearchPaths
    }
    let pathEntries = (environment["PATH"] ?? "")
      .split(separator: ":", omittingEmptySubsequences: true)
      .map { String($0) + "/node" }
    return dedupePaths(pathEntries + ["/opt/homebrew/bin/node", "/usr/local/bin/node", "/usr/bin/node"])
  }

  private static func dedupePaths(_ paths: [String]) -> [String] {
    var seen = Set<String>()
    var result: [String] = []
    for path in paths where !path.isEmpty && !seen.contains(path) {
      seen.insert(path)
      result.append(path)
    }
    return result
  }
}

public struct WikiEngineRenderSummary: Equatable, Sendable {
  public var sourceInputs: Int
  public var talkInputs: Int
  public var routeCount: Int
  public var markdownTwinCount: Int
  public var outputDirectory: URL

  public init(
    sourceInputs: Int,
    talkInputs: Int,
    routeCount: Int,
    markdownTwinCount: Int,
    outputDirectory: URL
  ) {
    self.sourceInputs = sourceInputs
    self.talkInputs = talkInputs
    self.routeCount = routeCount
    self.markdownTwinCount = markdownTwinCount
    self.outputDirectory = outputDirectory
  }
}

public enum WikiEngineRendererError: LocalizedError, Equatable {
  case missingRenderTool(String)
  case missingSourceRoot(String)
  case renderFailed(input: String, detail: String)
  case invalidResult(String)

  public var errorDescription: String? {
    switch self {
    case .missingRenderTool(let path):
      return "Missing wiki renderer tool: \(path)"
    case .missingSourceRoot(let path):
      return "Missing wiki source root: \(path)"
    case .renderFailed(let input, let detail):
      return "Wiki render failed for \(input): \(detail)"
    case .invalidResult(let detail):
      return "Wiki renderer returned an invalid result: \(detail)"
    }
  }
}

public final class WikiEngineRenderer: @unchecked Sendable {
  private struct RenderSiteResult: Decodable {
    var schema_version: Int
    var status: String
    var source_input_count: Int?
    var talk_input_count: Int?
    var route_count: Int?
    var markdown_twin_count: Int?
    var error: String?
  }

  private let config: WikiEngineRendererConfig
  private let fileManager: FileManager

  public init(config: WikiEngineRendererConfig, fileManager: FileManager = .default) {
    self.config = config
    self.fileManager = fileManager
  }

  public func render(runtimePaths: RuntimePaths, outputDirectory: URL) throws -> WikiEngineRenderSummary {
    guard fileManager.fileExists(atPath: config.renderTool.path) else {
      throw WikiEngineRendererError.missingRenderTool(config.renderTool.path)
    }
    guard fileManager.fileExists(atPath: runtimePaths.userWikiSourceDirectory.path) else {
      throw WikiEngineRendererError.missingSourceRoot(runtimePaths.userWikiSourceDirectory.path)
    }

    if fileManager.fileExists(atPath: outputDirectory.path) {
      try fileManager.removeItem(at: outputDirectory)
    }
    try RuntimePermissions.ensurePrivateDirectory(outputDirectory.deletingLastPathComponent())

    let resultURL = outputDirectory
      .appendingPathComponent(".1context", isDirectory: true)
      .appendingPathComponent("render-site-result.json")
    try runRenderSite(
      sourceRoot: runtimePaths.userWikiSourceDirectory,
      outputDirectory: outputDirectory,
      resultURL: resultURL
    )

    let data = try Data(contentsOf: resultURL)
    let result = try JSONDecoder().decode(RenderSiteResult.self, from: data)
    guard result.schema_version == 1 else {
      throw WikiEngineRendererError.invalidResult("unexpected schema_version \(result.schema_version)")
    }
    guard result.status == "published" else {
      throw WikiEngineRendererError.renderFailed(
        input: runtimePaths.userWikiSourceDirectory.path,
        detail: result.error ?? "renderer status=\(result.status)"
      )
    }
    guard
      let sourceInputs = result.source_input_count,
      let talkInputs = result.talk_input_count,
      let routeCount = result.route_count,
      let markdownTwinCount = result.markdown_twin_count
    else {
      throw WikiEngineRendererError.invalidResult("missing input, route, or markdown twin counts")
    }

    return WikiEngineRenderSummary(
      sourceInputs: sourceInputs,
      talkInputs: talkInputs,
      routeCount: routeCount,
      markdownTwinCount: markdownTwinCount,
      outputDirectory: outputDirectory
    )
  }

  private func runRenderSite(sourceRoot: URL, outputDirectory: URL, resultURL: URL) throws {
    let process = Process()
    if config.nodeExecutable.lastPathComponent == "env" {
      process.executableURL = config.nodeExecutable
      process.arguments = [
        "node",
        config.renderTool.path,
        "--source-root",
        sourceRoot.path,
        "--output",
        outputDirectory.path,
        "--result-json",
        resultURL.path
      ]
    } else {
      process.executableURL = config.nodeExecutable
      process.arguments = [
        config.renderTool.path,
        "--source-root",
        sourceRoot.path,
        "--output",
        outputDirectory.path,
        "--result-json",
        resultURL.path
      ]
    }
    process.currentDirectoryURL = config.engineDirectory
    process.environment = rendererEnvironment()
    process.standardInput = FileHandle.nullDevice

    let stdout = Pipe()
    let stderr = Pipe()
    process.standardOutput = stdout
    process.standardError = stderr
    try process.run()
    let stdoutBuffer = RendererPipeBuffer()
    let stderrBuffer = RendererPipeBuffer()
    let pipeDrainGroup = DispatchGroup()
    drain(stdout, into: stdoutBuffer, group: pipeDrainGroup)
    drain(stderr, into: stderrBuffer, group: pipeDrainGroup)
    process.waitUntilExit()
    pipeDrainGroup.wait()

    guard process.terminationStatus == 0 else {
      let detail = [
        stderrBuffer.string(),
        stdoutBuffer.string()
      ]
      .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
      .filter { !$0.isEmpty }
      .joined(separator: "\n")
      throw WikiEngineRendererError.renderFailed(input: sourceRoot.path, detail: detail)
    }
  }

  private func drain(_ pipe: Pipe, into buffer: RendererPipeBuffer, group: DispatchGroup) {
    group.enter()
    DispatchQueue.global(qos: .utility).async {
      buffer.append(pipe.fileHandleForReading.readDataToEndOfFile())
      group.leave()
    }
  }

  private func rendererEnvironment() -> [String: String] {
    var environment = ProcessInfo.processInfo.environment
    environment["PATH"] = developerToolPath(existing: environment["PATH"])
    return environment
  }

  private func developerToolPath(existing: String?) -> String {
    var parts: [String] = []
    for item in [existing ?? ""] + nvmNodeBinPaths() + [
      "\(FileManager.default.homeDirectoryForCurrentUser.path)/.local/bin",
      "\(FileManager.default.homeDirectoryForCurrentUser.path)/.cargo/bin",
      "/opt/homebrew/bin",
      "/opt/homebrew/sbin",
      "/usr/local/bin",
      "/usr/bin",
      "/bin",
      "/usr/sbin",
      "/sbin"
    ] where !item.isEmpty {
      for segment in item.split(separator: ":").map(String.init) where !segment.isEmpty && !parts.contains(segment) {
        parts.append(segment)
      }
    }
    return parts.joined(separator: ":")
  }

  private func nvmNodeBinPaths() -> [String] {
    let versionsRoot = FileManager.default.homeDirectoryForCurrentUser
      .appendingPathComponent(".nvm", isDirectory: true)
      .appendingPathComponent("versions", isDirectory: true)
      .appendingPathComponent("node", isDirectory: true)
    guard let versionDirectories = try? FileManager.default.contentsOfDirectory(
      at: versionsRoot,
      includingPropertiesForKeys: [.isDirectoryKey],
      options: [.skipsHiddenFiles]
    ) else {
      return []
    }
    return versionDirectories
      .filter { url in
        guard let values = try? url.resourceValues(forKeys: [.isDirectoryKey]) else {
          return false
        }
        return values.isDirectory == true
      }
      .sorted {
        $0.lastPathComponent.localizedStandardCompare($1.lastPathComponent) == .orderedDescending
      }
      .map { $0.appendingPathComponent("bin", isDirectory: true).path }
  }
}

private final class RendererPipeBuffer: @unchecked Sendable {
  private let lock = NSLock()
  private var chunks = Data()

  func append(_ data: Data) {
    guard !data.isEmpty else { return }
    lock.lock()
    chunks.append(data)
    lock.unlock()
  }

  func string() -> String {
    lock.lock()
    let snapshot = chunks
    lock.unlock()
    return String(decoding: snapshot, as: UTF8.self)
  }
}
