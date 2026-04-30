import Darwin
import CryptoKit
import Foundation
import OneContextCore
import OneContextPlatform

public struct MemoryCoreSetup {
  public let paths: MemoryCorePaths
  public let environment: [String: String]
  private let fileManager: FileManager

  public init(
    paths: MemoryCorePaths = .current(),
    environment: [String: String] = ProcessInfo.processInfo.environment,
    fileManager: FileManager = .default
  ) {
    self.paths = paths
    self.environment = environment
    self.fileManager = fileManager
  }

  public var coreDirectory: URL {
    paths.directory.appendingPathComponent("core", isDirectory: true)
  }

  public var executable: URL {
    coreDirectory.appendingPathComponent("bin/1context-memory-core")
  }

  public func ensureReady(validateContract: Bool = true) throws -> MemoryCoreStatus {
    try RuntimePermissions.ensurePrivateDirectory(paths.directory)
    try RuntimePermissions.ensurePrivateDirectory(paths.logFile.deletingLastPathComponent())
    let installedCore = try installBundledCoreIfNeeded()
    let installedJavaScript = try installJavaScriptDependenciesIfNeeded()
    let adapter = MemoryCoreAdapter(paths: paths, processRunner: MemoryCoreProcessRunner(environment: setupEnvironment()))
    let config = adapter.config()
    let venvPython = paths.directory.appendingPathComponent("venv/bin/python3").path
    let status: MemoryCoreStatus
    if installedCore || config.enabled != true || config.executable != executable.path || !fileManager.isExecutableFile(atPath: venvPython) {
      status = try adapter.configure(executable: executable.path)
    } else {
      status = adapter.status(forceCheck: validateContract)
      if status.health != .ok {
        return try adapter.configure(executable: executable.path)
      }
    }
    if installedCore || installedJavaScript {
      try prewarmPythonEnvironment()
    }
    return status
  }

  public func bundledCoreDirectory() -> URL? {
    if let override = environment["ONECONTEXT_MEMORY_CORE_BUNDLE_DIR"], !override.isEmpty {
      return URL(fileURLWithPath: override, isDirectory: true)
    }

    if let executableDirectory = currentExecutableURL()?.deletingLastPathComponent() {
      let resources = executableDirectory
        .deletingLastPathComponent()
        .appendingPathComponent("Resources/memory-core", isDirectory: true)
      if fileManager.fileExists(atPath: resources.appendingPathComponent("pyproject.toml").path) {
        return resources
      }
    }

    var directory = URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true)
    for _ in 0..<6 {
      let candidate = directory.appendingPathComponent("memory-core", isDirectory: true)
      if fileManager.fileExists(atPath: candidate.appendingPathComponent("pyproject.toml").path) {
        return candidate
      }
      directory.deleteLastPathComponent()
    }
    return nil
  }

  public func setupEnvironment(extra: [String: String] = [:]) -> [String: String] {
    var env = ProcessInfo.processInfo.environment.merging(environment) { _, new in new }
    env["PATH"] = expandedPath(env["PATH"])
    env["ONECONTEXT_MEMORY_CORE_DIR"] = paths.directory.path
    env["ONECONTEXT_MEMORY_CORE_LOG_PATH"] = paths.logFile.path
    env["ONECONTEXT_MEMORY_CORE_ROOT"] = coreDirectory.path
    env["ONECONTEXT_MEMORY_CORE_VENV"] = paths.directory.appendingPathComponent("venv", isDirectory: true).path
    env["UV_PROJECT_ENVIRONMENT"] = env["ONECONTEXT_MEMORY_CORE_VENV"]
    if env["LANG"] == nil || env["LANG"]?.uppercased() == "C.UTF-8" {
      env["LANG"] = "en_US.UTF-8"
    }
    if env["LC_CTYPE"] == nil || env["LC_CTYPE"]?.uppercased() == "C.UTF-8" {
      env["LC_CTYPE"] = "en_US.UTF-8"
    }
    env.removeValue(forKey: "LC_ALL")
    for (key, value) in extra {
      env[key] = value
    }
    return env
  }

  private func installBundledCoreIfNeeded() throws -> Bool {
    guard let source = bundledCoreDirectory() else {
      throw MemoryCoreSetupError.bundleMissing
    }

    let versionMarker = coreDirectory.appendingPathComponent(".1context-bundle-version")
    let fingerprintMarker = coreDirectory.appendingPathComponent(".1context-bundle-fingerprint")
    let sourceFingerprint = try bundleFingerprint(source)
    let installedVersion = (try? String(contentsOf: versionMarker, encoding: .utf8))?
      .trimmingCharacters(in: .whitespacesAndNewlines)
    let installedFingerprint = (try? String(contentsOf: fingerprintMarker, encoding: .utf8))?
      .trimmingCharacters(in: .whitespacesAndNewlines)

    let needsInstall = installedVersion != oneContextVersion
      || installedFingerprint != sourceFingerprint
      || !fileManager.fileExists(atPath: executable.path)
    if needsInstall {
      try? fileManager.removeItem(at: coreDirectory)
      try RuntimePermissions.ensurePrivateDirectory(paths.directory)
      try fileManager.copyItem(at: source, to: coreDirectory)
      try RuntimePermissions.writePrivateString("\(oneContextVersion)\n", toFile: versionMarker.path)
      try RuntimePermissions.writePrivateString("\(sourceFingerprint)\n", toFile: fingerprintMarker.path)
    }

    chmod(executable.path, 0o755)
    try rewriteManagedConfig()
    return needsInstall
  }

  public func bundleFingerprint(_ source: URL) throws -> String {
    var hasher = SHA256()
    let selected = [
      "pyproject.toml",
      "uv.lock",
      "bin",
      "src",
      "wiki-engine/src",
      "wiki-engine/tools",
      "wiki-engine/theme",
      "wiki/menu",
    ]

    for relative in selected {
      let url = source.appendingPathComponent(relative)
      guard fileManager.fileExists(atPath: url.path) else { continue }
      if isDirectory(url) {
        for file in filesUnder(url) {
          try hash(file: file, relativeTo: source, into: &hasher)
        }
      } else {
        try hash(file: url, relativeTo: source, into: &hasher)
      }
    }

    return hasher.finalize().map { String(format: "%02x", $0) }.joined()
  }

  private func hash(file: URL, relativeTo root: URL, into hasher: inout SHA256) throws {
    guard let relative = relativePath(file, from: root) else { return }
    hasher.update(data: Data(relative.utf8))
    hasher.update(data: Data([0]))
    hasher.update(data: try Data(contentsOf: file))
    hasher.update(data: Data([0]))
  }

  private func relativePath(_ url: URL, from root: URL) -> String? {
    let rootPath = root.standardizedFileURL.path
    let path = url.standardizedFileURL.path
    guard path.hasPrefix(rootPath + "/") else { return nil }
    return String(path.dropFirst(rootPath.count + 1))
  }

  private func filesUnder(_ root: URL) -> [URL] {
    guard let enumerator = fileManager.enumerator(at: root, includingPropertiesForKeys: [.isRegularFileKey]) else {
      return []
    }
    return enumerator.compactMap { item in
      guard let url = item as? URL,
        (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true
      else {
        return nil
      }
      return url
    }.sorted { $0.path < $1.path }
  }

  private func isDirectory(_ url: URL) -> Bool {
    var isDirectory: ObjCBool = false
    return fileManager.fileExists(atPath: url.path, isDirectory: &isDirectory) && isDirectory.boolValue
  }

  private func rewriteManagedConfig() throws {
    let config = coreDirectory.appendingPathComponent("1context.toml")
    guard fileManager.fileExists(atPath: config.path) else { return }
    let text = """
    active_plugin = "base-memory-v1"
    plugin_dirs = ["memory/plugins", "memory/runtime/plugins"]
    runtime_dir = "\(tomlString(paths.directory.appendingPathComponent("runtime", isDirectory: true).path))"
    storage_dir = "\(tomlString(paths.directory.appendingPathComponent("storage/lakestore", isDirectory: true).path))"
    accounts_file = "\(tomlString(coreDirectory.appendingPathComponent("accounts.toml").path))"

    [runtime_policy]
    max_concurrent_agents = 8
    default_harness_isolation = "account_clean"
    """
    try RuntimePermissions.writePrivateString(text + "\n", toFile: config.path)
  }

  private func installJavaScriptDependenciesIfNeeded() throws -> Bool {
    let packageLock = coreDirectory.appendingPathComponent("wiki-engine/package-lock.json")
    guard fileManager.fileExists(atPath: packageLock.path) else { return false }
    let nodeModules = coreDirectory.appendingPathComponent("wiki-engine/node_modules", isDirectory: true)
    guard !fileManager.fileExists(atPath: nodeModules.path) else { return false }
    guard let npm = firstExecutable(["/opt/homebrew/bin/npm", "/usr/local/bin/npm", "/usr/bin/npm"]) else {
      throw MemoryCoreSetupError.toolMissing("npm")
    }
    try runProcess(
      executable: npm,
      arguments: ["ci", "--silent"],
      currentDirectory: coreDirectory.appendingPathComponent("wiki-engine", isDirectory: true),
      timeout: 600
    )
    return true
  }

  private func prewarmPythonEnvironment() throws {
    let python = paths.directory.appendingPathComponent("venv/bin/python3").path
    guard fileManager.isExecutableFile(atPath: python) else {
      throw MemoryCoreSetupError.toolMissing("python3")
    }
    try runProcess(
      executable: python,
      arguments: ["-m", "compileall", "-q", coreDirectory.appendingPathComponent("src", isDirectory: true).path],
      currentDirectory: coreDirectory,
      timeout: 120
    )
  }

  public func runProcess(
    executable: String,
    arguments: [String],
    currentDirectory: URL? = nil,
    timeout: TimeInterval = 120
  ) throws {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: executable)
    process.arguments = arguments
    process.currentDirectoryURL = currentDirectory
    process.environment = setupEnvironment()
    process.standardInput = FileHandle.nullDevice
    process.standardOutput = FileHandle.nullDevice
    process.standardError = FileHandle.nullDevice

    let group = DispatchGroup()
    group.enter()
    process.terminationHandler = { _ in group.leave() }
    try process.run()
    if group.wait(timeout: .now() + timeout) == .timedOut {
      process.terminate()
      throw MemoryCoreSetupError.timedOut(arguments.joined(separator: " "))
    }
    guard process.terminationStatus == 0 else {
      throw MemoryCoreSetupError.commandFailed(([executable] + arguments).joined(separator: " "))
    }
  }

  private func firstExecutable(_ candidates: [String]) -> String? {
    candidates.first { fileManager.isExecutableFile(atPath: $0) }
  }

  private func expandedPath(_ path: String?) -> String {
    let extras = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"]
    var parts = (path ?? "").split(separator: ":").map(String.init)
    for extra in extras where !parts.contains(extra) {
      parts.append(extra)
    }
    return parts.joined(separator: ":")
  }

  private func currentExecutableURL() -> URL? {
    var size = UInt32(0)
    _NSGetExecutablePath(nil, &size)
    var buffer = [CChar](repeating: 0, count: Int(size))
    guard _NSGetExecutablePath(&buffer, &size) == 0 else { return nil }
    let pathBytes = buffer.prefix { $0 != 0 }.map { UInt8(bitPattern: $0) }
    return URL(fileURLWithPath: String(decoding: pathBytes, as: UTF8.self)).resolvingSymlinksInPath()
  }

  private func tomlString(_ value: String) -> String {
    value
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
  }
}

public enum MemoryCoreSetupError: Error, LocalizedError, Equatable {
  case bundleMissing
  case toolMissing(String)
  case commandFailed(String)
  case timedOut(String)

  public var errorDescription: String? {
    switch self {
    case .bundleMissing:
      return "Bundled 1Context memory core was not found"
    case .toolMissing(let tool):
      return "\(tool) is required to prepare the local wiki"
    case .commandFailed(let command):
      return "Command failed while preparing the local wiki: \(command)"
    case .timedOut(let command):
      return "Timed out while preparing the local wiki: \(command)"
    }
  }
}
