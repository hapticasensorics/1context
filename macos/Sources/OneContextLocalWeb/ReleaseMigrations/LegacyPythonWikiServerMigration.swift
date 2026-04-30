import Darwin
import Foundation
import OneContextPlatform

public struct LegacyPythonWikiServerMigrationResult: Codable, Equatable, Sendable {
  public var removedPaths: [String]
  public var stoppedPID: Int32?

  public init(removedPaths: [String] = [], stoppedPID: Int32? = nil) {
    self.removedPaths = removedPaths
    self.stoppedPID = stoppedPID
  }
}

public enum LegacyPythonWikiServerMigration {
  public static func run(
    runtimePaths: RuntimePaths = .current(),
    fileManager: FileManager = .default
  ) -> LegacyPythonWikiServerMigrationResult {
    let legacyRoot = runtimePaths.appSupportDirectory.appendingPathComponent("memory-core", isDirectory: true)
    var removed: [String] = []
    var stoppedPID: Int32?

    let stateFile = legacyRoot.appendingPathComponent("wiki-server.json")
    if let pid = readLegacyPID(from: stateFile), isLegacyWikiServer(pid: pid) {
      kill(pid, SIGTERM)
      usleep(150_000)
      if processIsAlive(pid) {
        kill(pid, SIGKILL)
      }
      stoppedPID = pid
    }

    for path in legacyFiles(under: legacyRoot) {
      guard fileManager.fileExists(atPath: path.path) else { continue }
      do {
        try fileManager.removeItem(at: path)
        removed.append(path.path)
      } catch {
        continue
      }
    }

    return LegacyPythonWikiServerMigrationResult(removedPaths: removed.sorted(), stoppedPID: stoppedPID)
  }

  private static func legacyFiles(under legacyRoot: URL) -> [URL] {
    [
      legacyRoot.appendingPathComponent("wiki-server.json"),
      legacyRoot.appendingPathComponent("wiki-server.log"),
      legacyRoot.appendingPathComponent("core/src/onectx/wiki/server.py"),
      legacyRoot.appendingPathComponent("core/src/onectx/wiki/serve_main.py"),
    ]
  }

  private static func readLegacyPID(from stateFile: URL) -> Int32? {
    guard let data = try? Data(contentsOf: stateFile),
      let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any]
    else {
      return nil
    }
    if let number = object["pid"] as? NSNumber {
      return number.int32Value
    }
    if let string = object["pid"] as? String, let value = Int32(string) {
      return value
    }
    return nil
  }

  private static func isLegacyWikiServer(pid: Int32) -> Bool {
    guard pid > 0, processIsAlive(pid), let command = commandLine(pid: pid) else { return false }
    return command.contains("onectx.wiki.serve_main")
      || command.contains("1context wiki serve")
      || command.contains("wiki serve --port")
  }

  private static func processIsAlive(_ pid: Int32) -> Bool {
    pid > 0 && kill(pid, 0) == 0
  }

  private static func commandLine(pid: Int32) -> String? {
    let process = Process()
    process.executableURL = URL(fileURLWithPath: "/bin/ps")
    process.arguments = ["-p", "\(pid)", "-o", "command="]
    process.standardInput = FileHandle.nullDevice
    let pipe = Pipe()
    process.standardOutput = pipe
    process.standardError = FileHandle.nullDevice

    do {
      try process.run()
      process.waitUntilExit()
      guard process.terminationStatus == 0 else { return nil }
      let data = pipe.fileHandleForReading.readDataToEndOfFile()
      return String(data: data, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
    } catch {
      return nil
    }
  }
}
