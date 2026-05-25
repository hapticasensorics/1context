import Foundation
import OneContextPlatform

public struct OneContextCaptureRecorder {
  private let runtimePaths: RuntimePaths
  private let snapshotProvider: () async -> CaptureSnapshot

  public init(runtimePaths: RuntimePaths, indexer: OneContextWindowIndexer = OneContextWindowIndexer()) {
    self.runtimePaths = runtimePaths
    self.snapshotProvider = {
      await indexer.snapshot()
    }
  }

  init(runtimePaths: RuntimePaths, snapshotProvider: @escaping () async -> CaptureSnapshot) {
    self.runtimePaths = runtimePaths
    self.snapshotProvider = snapshotProvider
  }

  public func recordWindowSnapshot() async throws -> CaptureSnapshot {
    let snapshot = await snapshotProvider()
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths)
    try store.prepare()
    try store.appendWindowSnapshot(snapshot)
    if let focusedContext = snapshot.focusedContext {
      try store.appendEvent(
        eventType: "capture.ax_focused_context",
        durability: .lossless,
        recordedAt: focusedContext.generatedAt,
        payload: focusedContext
      )
    }
    return snapshot
  }
}

public enum CaptureJSON {
  public static func dictionary<T: Encodable>(_ value: T) throws -> [String: Any] {
    let data = try encoder().encode(value)
    return try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:]
  }

  private static func encoder() -> JSONEncoder {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
    return encoder
  }
}
