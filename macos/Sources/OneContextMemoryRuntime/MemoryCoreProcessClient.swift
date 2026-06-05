import Foundation
import OneContextPlatform

public struct MemoryCoreProcessError: Error, LocalizedError, Sendable {
  public let message: String

  public init(message: String) {
    self.message = message
  }

  public var errorDescription: String? {
    message
  }
}

public final class MemoryCoreProcessClient: @unchecked Sendable {
  private struct Command {
    let executable: URL
    let argumentsPrefix: [String]
    let root: URL?
  }

  private let runtimePaths: RuntimePaths
  private let fileManager: FileManager
  private let environment: [String: String]

  public init(
    runtimePaths: RuntimePaths,
    fileManager: FileManager = .default,
    environment: [String: String] = ProcessInfo.processInfo.environment
  ) {
    self.runtimePaths = runtimePaths
    self.fileManager = fileManager
    self.environment = environment
  }

  public func updateWiki(
    provider: String = "codex",
    runID: String? = nil,
    executeAgents: Bool = false,
    maxConcurrent: Int? = nil,
    timeoutSeconds: Int = 60,
    importSources: Bool = false,
    importTicks: Int = 1,
    runtimeRoot: URL? = nil,
    wikiCoreBin: URL? = nil
  ) throws -> [String: Any] {
    let command = try discoverCommand()
    var arguments = command.argumentsPrefix + [
      "memory",
      "update-wiki",
      "--provider",
      normalizedProvider(provider)
    ]
    if let runID, !runID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
      arguments += ["--run-id", runID]
    }
    if executeAgents {
      arguments.append("--execute-agents")
    }
    if let maxConcurrent {
      arguments += ["--max-concurrent", "\(max(1, maxConcurrent))"]
    }
    if importSources {
      arguments.append("--import-sources")
      arguments += ["--import-ticks", "\(max(1, importTicks))"]
    }
    if let runtimeRoot {
      arguments += ["--runtime-root", runtimeRoot.path]
    }
    if let wikiCoreBin {
      arguments += ["--wiki-core-bin", wikiCoreBin.path]
    }
    arguments += ["--timeout-seconds", "\(max(1, timeoutSeconds))", "--json"]

    let result = try ProcessRunner.run(
      executable: command.executable,
      arguments: arguments,
      environment: processEnvironment(root: command.root),
      timeoutSeconds: TimeInterval(max(5, timeoutSeconds + 5))
    )
    if result.timedOut {
      throw MemoryCoreProcessError(message: "memory-core update-wiki timed out")
    }
    guard result.status == 0 else {
      let message = String(decoding: result.stderr.isEmpty ? result.stdout : result.stderr, as: UTF8.self)
        .trimmingCharacters(in: .whitespacesAndNewlines)
      throw MemoryCoreProcessError(message: message.isEmpty ? "memory-core exited \(result.status)" : message)
    }
    guard let object = try JSONSerialization.jsonObject(with: result.stdout) as? [String: Any] else {
      throw MemoryCoreProcessError(message: "memory-core returned non-object JSON")
    }
    return object
  }

  private func discoverCommand() throws -> Command {
    if let override = environment["ONECONTEXT_MEMORY_CORE_BIN"], !override.isEmpty {
      let executable = URL(fileURLWithPath: override)
      if fileManager.isExecutableFile(atPath: executable.path) {
        return Command(executable: executable, argumentsPrefix: [], root: memoryCoreRoot())
      }
      throw MemoryCoreProcessError(message: "ONECONTEXT_MEMORY_CORE_BIN is not executable: \(override)")
    }

    guard let root = memoryCoreRoot() else {
      throw MemoryCoreProcessError(message: "memory-core root not found")
    }

    let direct = root.appendingPathComponent(".venv/bin/1context-memory-core")
    if fileManager.isExecutableFile(atPath: direct.path) {
      return Command(executable: direct, argumentsPrefix: [], root: root)
    }

    let env = URL(fileURLWithPath: "/usr/bin/env")
    if fileManager.isExecutableFile(atPath: env.path) {
      return Command(
        executable: env,
        argumentsPrefix: ["uv", "run", "--project", root.path, "1context-memory-core"],
        root: root
      )
    }

    throw MemoryCoreProcessError(message: "memory-core command not found")
  }

  private func processEnvironment(root: URL?) -> [String: String] {
    var env = environment
    env[OneContextAppIdentity.environmentKey] = runtimePaths.identity.environmentValue
    if let root {
      env["ONECONTEXT_MEMORY_CORE_ROOT"] = root.path
    }
    env["PATH"] = developerToolPath(existing: env["PATH"])
    return env
  }

  private func memoryCoreRoot() -> URL? {
    for candidate in memoryCoreRootCandidates() {
      if let root = normalizeMemoryCoreRoot(candidate) {
        return root
      }
    }
    return nil
  }

  private func memoryCoreRootCandidates() -> [URL] {
    var candidates: [URL] = []
    if let override = environment["ONECONTEXT_MEMORY_CORE_ROOT"], !override.isEmpty {
      candidates.append(URL(fileURLWithPath: override, isDirectory: true))
    }
    if let resourceURL = Bundle.main.resourceURL {
      let marker = resourceURL.appendingPathComponent("DevMemoryCoreRoot.txt")
      if let text = try? String(contentsOf: marker, encoding: .utf8) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if !trimmed.isEmpty {
          candidates.append(URL(fileURLWithPath: trimmed, isDirectory: true))
        }
      }
    }
    candidates.append(URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true))
    candidates += ancestorDirectories(from: Bundle.main.executableURL)
    candidates += ancestorDirectories(from: URL(fileURLWithPath: CommandLine.arguments.first ?? fileManager.currentDirectoryPath))
    return candidates
  }

  private func normalizeMemoryCoreRoot(_ candidate: URL) -> URL? {
    let directory = candidate.hasDirectoryPath ? candidate : candidate.deletingLastPathComponent()
    let direct = directory.appendingPathComponent("pyproject.toml")
    if directory.lastPathComponent == "memory-core", fileManager.fileExists(atPath: direct.path) {
      return directory
    }
    let child = directory.appendingPathComponent("memory-core", isDirectory: true)
    if fileManager.fileExists(atPath: child.appendingPathComponent("pyproject.toml").path) {
      return child
    }
    return nil
  }

  private func ancestorDirectories(from url: URL?) -> [URL] {
    guard var cursor = url else { return [] }
    if !cursor.hasDirectoryPath {
      cursor.deleteLastPathComponent()
    }
    var result: [URL] = []
    for _ in 0..<12 {
      result.append(cursor)
      let next = cursor.deletingLastPathComponent()
      if next.path == cursor.path {
        break
      }
      cursor = next
    }
    return result
  }

  private func normalizedProvider(_ provider: String) -> String {
    let trimmed = provider.trimmingCharacters(in: .whitespacesAndNewlines)
    return trimmed.isEmpty ? "codex" : trimmed
  }

  private func developerToolPath(existing: String?) -> String {
    var parts: [String] = []
    let home = fileManager.homeDirectoryForCurrentUser.path
    for item in [
      existing ?? "",
      "\(home)/.local/bin",
      "\(home)/.cargo/bin",
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
}
