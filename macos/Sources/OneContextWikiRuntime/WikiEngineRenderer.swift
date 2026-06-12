import Foundation
import OneContextPlatform

public enum WikiEngineNodeSource: String, Equatable, Sendable {
  case envOverride = "env-override"
  case bundled
  case system
  case missing
}

public struct WikiEngineNodeResolution: Equatable, Sendable {
  public var executable: URL?
  public var source: WikiEngineNodeSource

  public init(executable: URL?, source: WikiEngineNodeSource) {
    self.executable = executable
    self.source = source
  }
}

public struct WikiEngineRendererConfig: Equatable, Sendable {
  public var node: WikiEngineNodeResolution
  public var engineDirectory: URL
  public var renderTool: URL

  public init(node: WikiEngineNodeResolution, engineDirectory: URL, renderTool: URL) {
    self.node = node
    self.engineDirectory = engineDirectory
    self.renderTool = renderTool
  }

  public static func discover(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    resourceURL: URL? = Bundle.main.resourceURL,
    executableURL: URL? = Bundle.main.executableURL
  ) -> WikiEngineRendererConfig? {
    guard let engine = discoverEngineDirectory(
      environment: environment,
      resourceURL: resourceURL,
      executableURL: executableURL
    ) else {
      return nil
    }
    return WikiEngineRendererConfig(
      node: resolveNode(
        environment: environment,
        resourceURL: resourceURL,
        executableURL: executableURL
      ),
      engineDirectory: engine,
      renderTool: engine.appendingPathComponent("tools/render-site.mjs")
    )
  }

  public static func resolveNode(
    environment: [String: String] = ProcessInfo.processInfo.environment,
    resourceURL: URL? = Bundle.main.resourceURL,
    executableURL: URL? = Bundle.main.executableURL,
    fileManager: FileManager = .default,
    systemSearchPaths: [String]? = nil
  ) -> WikiEngineNodeResolution {
    if let override = environment["ONECONTEXT_NODE"], !override.isEmpty {
      return WikiEngineNodeResolution(executable: URL(fileURLWithPath: override), source: .envOverride)
    }
    if let bundled = bundledNodeExecutable(
      resourceURL: resourceURL,
      executableURL: executableURL,
      fileManager: fileManager
    ) {
      return WikiEngineNodeResolution(executable: bundled, source: .bundled)
    }
    for directory in systemSearchPaths ?? nodeSearchPaths(environment: environment) {
      let candidate = URL(fileURLWithPath: directory, isDirectory: true).appendingPathComponent("node")
      if fileManager.isExecutableFile(atPath: candidate.path) {
        return WikiEngineNodeResolution(executable: candidate, source: .system)
      }
    }
    return WikiEngineNodeResolution(executable: nil, source: .missing)
  }

  private static func bundledNodeExecutable(
    resourceURL: URL?,
    executableURL: URL?,
    fileManager: FileManager
  ) -> URL? {
    var candidates: [URL] = []
    if let resourceURL {
      candidates.append(resourceURL.appendingPathComponent("node-runtime/bin/node"))
    }
    if let executableURL {
      candidates.append(
        executableURL
          .deletingLastPathComponent()
          .deletingLastPathComponent()
          .appendingPathComponent("Resources", isDirectory: true)
          .appendingPathComponent("node-runtime/bin/node")
      )
    }
    return candidates.first { fileManager.isExecutableFile(atPath: $0.path) }
  }

  static func nodeSearchPaths(environment: [String: String]) -> [String] {
    var parts: [String] = []
    for item in [environment["PATH"] ?? ""] + nvmNodeBinPaths() + [
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
    return parts
  }

  private static func nvmNodeBinPaths() -> [String] {
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
  case missingNodeRuntime
  case renderFailed(input: String, detail: String)
  case invalidResult(String)

  public var errorDescription: String? {
    switch self {
    case .missingRenderTool(let path):
      return "Missing wiki renderer tool: \(path)"
    case .missingSourceRoot(let path):
      return "Missing wiki source root: \(path)"
    case .missingNodeRuntime:
      return "Wiki renderer Node.js runtime is missing: checked ONECONTEXT_NODE, the bundled "
        + "Resources/node-runtime/bin/node, and PATH. Wiki re-publish cannot run until Node.js "
        + "is available. Set ONECONTEXT_NODE, install node, or reinstall a build with the bundled node runtime."
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
    guard let nodeExecutable = config.node.executable else {
      throw WikiEngineRendererError.missingNodeRuntime
    }
    let process = Process()
    process.executableURL = nodeExecutable
    // ONECONTEXT_NODE may point at /usr/bin/env; keep `env node` invocation working for that override.
    process.arguments = (nodeExecutable.lastPathComponent == "env" ? ["node"] : []) + [
      config.renderTool.path,
      "--source-root",
      sourceRoot.path,
      "--output",
      outputDirectory.path,
      "--result-json",
      resultURL.path
    ]
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
    environment["PATH"] = WikiEngineRendererConfig
      .nodeSearchPaths(environment: environment)
      .joined(separator: ":")
    return environment
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
