import Foundation
import OneContextPlatform

public struct WikiCoreProcessError: Error, LocalizedError {
  public let message: String

  public var errorDescription: String? {
    message
  }
}

public final class WikiCoreProcessClient: @unchecked Sendable {
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

  public func call(_ arguments: [String]) throws -> [String: Any] {
    let executable = try discoverExecutable()
    let process = Process()
    let stdout = Pipe()
    let stderr = Pipe()
    process.executableURL = executable
    process.arguments = ["--root", runtimePaths.userContentDirectory.path] + arguments
    process.standardOutput = stdout
    process.standardError = stderr

    let outputBuffer = ProcessPipeBuffer()
    let errorBuffer = ProcessPipeBuffer()
    try process.run()
    let pipeDrainGroup = DispatchGroup()
    drain(stdout, into: outputBuffer, group: pipeDrainGroup)
    drain(stderr, into: errorBuffer, group: pipeDrainGroup)
    process.waitUntilExit()
    pipeDrainGroup.wait()

    let output = outputBuffer.data()
    let errorOutput = errorBuffer.data()
    let outputText = String(decoding: output, as: UTF8.self)
      .trimmingCharacters(in: .whitespacesAndNewlines)
    let errorText = String(decoding: errorOutput, as: UTF8.self)
      .trimmingCharacters(in: .whitespacesAndNewlines)
    let outputObject = try? JSONSerialization.jsonObject(with: output) as? [String: Any]
    guard process.terminationStatus == 0 else {
      if shouldReturnNonZeroReceipt(arguments: arguments, outputObject: outputObject) {
        return outputObject!
      }
      let message = !errorText.isEmpty
        ? errorText
        : (!outputText.isEmpty ? outputText : "wiki core exited with status \(process.terminationStatus)")
      throw WikiCoreProcessError(message: message)
    }

    guard let object = outputObject else {
      throw WikiCoreProcessError(message: "wiki core returned non-object JSON")
    }
    return object
  }

  private func shouldReturnNonZeroReceipt(arguments: [String], outputObject: [String: Any]?) -> Bool {
    arguments.first == "publish" && outputObject != nil
  }

  private func drain(_ pipe: Pipe, into buffer: ProcessPipeBuffer, group: DispatchGroup) {
    group.enter()
    DispatchQueue.global(qos: .utility).async {
      buffer.append(pipe.fileHandleForReading.readDataToEndOfFile())
      group.leave()
    }
  }

  public func discoverExecutable() throws -> URL {
    let candidates = executableCandidates()
    if let executable = candidates.first(where: { fileManager.isExecutableFile(atPath: $0.path) }) {
      return executable
    }
    throw WikiCoreProcessError(
      message: "onecontext-wiki core executable missing. Checked: \(candidates.map(\.path).joined(separator: ", "))"
    )
  }

  private func executableCandidates() -> [URL] {
    var candidates: [URL] = []
    if let override = environment["ONECONTEXT_WIKI_CORE_BIN"], !override.isEmpty {
      candidates.append(URL(fileURLWithPath: override))
    }

    if let executableDirectory = Bundle.main.executableURL?.deletingLastPathComponent() {
      candidates.append(executableDirectory.appendingPathComponent("onecontext-wiki"))
    }

    let cwd = URL(fileURLWithPath: fileManager.currentDirectoryPath, isDirectory: true)
    candidates.append(cwd.appendingPathComponent("target/debug/onecontext-wiki"))
    candidates.append(cwd.appendingPathComponent("target/release/onecontext-wiki"))
    candidates.append(cwd.appendingPathComponent("../target/debug/onecontext-wiki"))
    candidates.append(cwd.appendingPathComponent("../target/release/onecontext-wiki"))
    return candidates
  }
}

private final class ProcessPipeBuffer: @unchecked Sendable {
  private let lock = NSLock()
  private var chunks = Data()

  func append(_ data: Data) {
    guard !data.isEmpty else {
      return
    }
    lock.lock()
    chunks.append(data)
    lock.unlock()
  }

  func data() -> Data {
    lock.lock()
    let snapshot = chunks
    lock.unlock()
    return snapshot
  }
}
