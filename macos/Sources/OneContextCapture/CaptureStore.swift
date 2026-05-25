import Foundation
import Darwin
import OneContextPlatform

public struct OneContextCapturePaths: Equatable, Sendable {
  public var rootDirectory: URL
  public var eventsDirectory: URL
  public var windowsDirectory: URL
  public var displaysDirectory: URL
  public var mediaDirectory: URL
  public var displayMediaDirectory: URL
  public var windowMediaDirectory: URL
  public var keyframeMediaDirectory: URL

  public init(runtimePaths: RuntimePaths) {
    self.rootDirectory = runtimePaths.appSupportDirectory.appendingPathComponent("capture", isDirectory: true)
    self.eventsDirectory = rootDirectory.appendingPathComponent("events", isDirectory: true)
    self.windowsDirectory = rootDirectory.appendingPathComponent("windows", isDirectory: true)
    self.displaysDirectory = rootDirectory.appendingPathComponent("displays", isDirectory: true)
    self.mediaDirectory = rootDirectory.appendingPathComponent("media", isDirectory: true)
    self.displayMediaDirectory = mediaDirectory.appendingPathComponent("display", isDirectory: true)
    self.windowMediaDirectory = mediaDirectory.appendingPathComponent("windows", isDirectory: true)
    self.keyframeMediaDirectory = mediaDirectory.appendingPathComponent("keyframes", isDirectory: true)
  }

  public func ensureDirectories() throws {
    for directory in [
      rootDirectory,
      eventsDirectory,
      windowsDirectory,
      displaysDirectory,
      mediaDirectory,
      displayMediaDirectory,
      windowMediaDirectory,
      keyframeMediaDirectory
    ] {
      try RuntimePermissions.ensurePrivateDirectory(directory)
    }
  }
}

public final class OneContextCaptureLogStore: @unchecked Sendable {
  private let paths: OneContextCapturePaths
  private let encoder: JSONEncoder
  private let queue = DispatchQueue(label: "com.haptica.1context.capture-log-store")
  private var prepared = false

  public init(paths: OneContextCapturePaths) {
    self.paths = paths
    self.encoder = JSONEncoder()
    self.encoder.outputFormatting = [.sortedKeys]
  }

  public convenience init(runtimePaths: RuntimePaths) {
    self.init(paths: OneContextCapturePaths(runtimePaths: runtimePaths))
  }

  public func prepare() throws {
    try queue.sync {
      try prepareLocked()
    }
  }

  public func latestWindowSnapshot() throws -> StoredCaptureSnapshot? {
    let files = try sortedJSONLFiles(in: paths.windowsDirectory, suffix: ".windows.jsonl")

    let decoder = JSONDecoder()
    for file in files {
      guard let line = try lastNonEmptyLine(in: file) else { continue }
      let envelope = try decoder.decode(CaptureEventEnvelope<CaptureSnapshot>.self, from: Data(line.utf8))
      return StoredCaptureSnapshot(
        fileURL: file,
        envelope: envelope,
        snapshot: envelope.payload
      )
    }
    return nil
  }

  public func latestActiveWindowFrameMetadata() throws -> StoredCaptureEvent<ActiveWindowFrameMetadata>? {
    try latestEvent(
      eventType: "capture.active_window_frame_metadata",
      payloadType: ActiveWindowFrameMetadata.self
    )
  }

  public func latestUXEventAnchor() throws -> StoredCaptureEvent<UXEventAnchor>? {
    try latestUXEventAnchors(limit: 1).first
  }

  public func latestUXEventAnchor(kind: UXEventAnchorKind) throws -> StoredCaptureEvent<UXEventAnchor>? {
    try latestUXEventAnchors(kinds: [kind], limit: 1).first
  }

  public func latestUXEventAnchors(
    kinds: Set<UXEventAnchorKind>? = nil,
    limit: Int = 50
  ) throws -> [StoredCaptureEvent<UXEventAnchor>] {
    let maxCount = max(0, limit)
    guard maxCount > 0 else { return [] }
    let eventTypes = kinds.map { Set($0.map(\.captureEventType)) }
    return try recentEvents(
      eventTypes: eventTypes ?? UXEventAnchorKind.allCaptureEventTypes,
      payloadType: UXEventAnchor.self,
      limit: maxCount
    )
  }

  @discardableResult
  public func appendWindowSnapshot(_ snapshot: CaptureSnapshot) throws -> URL {
    let envelope = CaptureEventEnvelope(
      eventType: "capture.window_snapshot",
      durability: .lossless,
      recordedAt: snapshot.generatedAt,
      payload: snapshot
    )
    return try append(envelope, to: windowsLogURL(for: snapshot.generatedAt))
  }

  @discardableResult
  public func appendActiveWindowFrameMetadata(_ metadata: ActiveWindowFrameMetadata) throws -> URL {
    try appendEvent(
      eventType: "capture.active_window_frame_metadata",
      durability: .bestEffort,
      recordedAt: metadata.capturedAt,
      payload: metadata
    )
  }

  @discardableResult
  public func appendUXEventAnchor(_ anchor: UXEventAnchor) throws -> URL {
    try appendEvent(
      eventType: anchor.captureEventType,
      durability: .bestEffort,
      recordedAt: anchor.endedAt,
      payload: anchor
    )
  }

  @discardableResult
  public func appendUXEventAnchors(_ anchors: [UXEventAnchor]) throws -> [URL] {
    guard !anchors.isEmpty else { return [] }
    return try queue.sync {
      try prepareLocked()
      var urls: [URL] = []
      urls.reserveCapacity(anchors.count)
      var linesByURL: [URL: Data] = [:]

      for anchor in anchors {
        let envelope = CaptureEventEnvelope(
          eventType: anchor.captureEventType,
          durability: CaptureEventDurability.bestEffort,
          recordedAt: anchor.endedAt,
          payload: anchor
        )
        var line = try encoder.encode(envelope)
        line.append(0x0A)
        let url = eventsLogURL(for: anchor.endedAt)
        urls.append(url)
        linesByURL[url, default: Data()].append(line)
      }

      for (url, data) in linesByURL {
        try appendLineDataLocked(data, to: url)
      }
      return urls
    }
  }

  @discardableResult
  public func appendEvent<Payload: Codable & Sendable>(
    eventType: String,
    durability: CaptureEventDurability,
    recordedAt: String,
    payload: Payload
  ) throws -> URL {
    let envelope = CaptureEventEnvelope(
      eventType: eventType,
      durability: durability,
      recordedAt: recordedAt,
      payload: payload
    )
    return try append(envelope, to: eventsLogURL(for: recordedAt))
  }

  private func append<Payload: Codable & Sendable>(
    _ envelope: CaptureEventEnvelope<Payload>,
    to fileURL: URL
  ) throws -> URL {
    try queue.sync {
      try prepareLocked()
      let data = try encoder.encode(envelope)
      var line = data
      line.append(0x0A)
      try appendLineDataLocked(line, to: fileURL)
      return fileURL
    }
  }

  private func prepareLocked() throws {
    guard !prepared else { return }
    try paths.ensureDirectories()
    prepared = true
  }

  private func appendLineDataLocked(_ data: Data, to fileURL: URL) throws {
    try appendPrivateData(data, to: fileURL)
  }

  private func windowsLogURL(for isoTimestamp: String) -> URL {
    windowsDirectoryFile(prefix: dayPrefix(from: isoTimestamp), suffix: "windows.jsonl")
  }

  private func eventsLogURL(for isoTimestamp: String) -> URL {
    paths.eventsDirectory.appendingPathComponent("\(dayPrefix(from: isoTimestamp)).events.jsonl")
  }

  private func latestEvent<Payload: Codable & Sendable>(
    eventType: String,
    payloadType: Payload.Type
  ) throws -> StoredCaptureEvent<Payload>? {
    try recentEvents(eventTypes: [eventType], payloadType: payloadType, limit: 1).first
  }

  private func recentEvents<Payload: Codable & Sendable>(
    eventTypes: Set<String>,
    payloadType: Payload.Type,
    limit: Int
  ) throws -> [StoredCaptureEvent<Payload>] {
    guard limit > 0 else { return [] }
    let files = try sortedJSONLFiles(in: paths.eventsDirectory, suffix: ".events.jsonl")

    let decoder = JSONDecoder()
    var matches: [StoredCaptureEvent<Payload>] = []
    for file in files {
      let lines = try recentNonEmptyLines(
        in: file,
        targetLineCount: limit,
        matchingEventTypes: eventTypes
      ).reversed()
      for line in lines {
        guard lineContainsAnyEventType(line, eventTypes: eventTypes),
          let data = line.data(using: .utf8),
          let envelope = try? decoder.decode(CaptureEventEnvelope<Payload>.self, from: data),
          eventTypes.contains(envelope.eventType)
        else {
          continue
        }
        matches.append(StoredCaptureEvent(fileURL: file, envelope: envelope, payload: envelope.payload))
        if matches.count >= limit {
          return matches
        }
      }
    }
    return matches
  }

  private func windowsDirectoryFile(prefix: String, suffix: String) -> URL {
    paths.windowsDirectory.appendingPathComponent("\(prefix).\(suffix)")
  }

  private func dayPrefix(from isoTimestamp: String) -> String {
    String(isoTimestamp.prefix(10))
  }

  private func lastNonEmptyLine(in fileURL: URL) throws -> String? {
    try recentNonEmptyLines(in: fileURL, targetLineCount: 1, matchingEventTypes: nil).last
  }

  private func sortedJSONLFiles(in directory: URL, suffix: String) throws -> [URL] {
    try FileManager.default.contentsOfDirectory(
      at: directory,
      includingPropertiesForKeys: [.contentModificationDateKey],
      options: [.skipsHiddenFiles]
    )
    .compactMap { url -> LogFileCandidate? in
      guard url.lastPathComponent.hasSuffix(suffix) else { return nil }
      let modifiedAt = (try? url.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
      return LogFileCandidate(url: url, modifiedAt: modifiedAt)
    }
    .sorted { lhs, rhs in
      if lhs.modifiedAt == rhs.modifiedAt {
        return lhs.url.lastPathComponent > rhs.url.lastPathComponent
      }
      return lhs.modifiedAt > rhs.modifiedAt
    }
    .map(\.url)
  }

  private func recentNonEmptyLines(
    in fileURL: URL,
    targetLineCount: Int,
    matchingEventTypes: Set<String>?
  ) throws -> [String] {
    let targetLineCount = max(1, targetLineCount)
    let attributes = try FileManager.default.attributesOfItem(atPath: fileURL.path)
    let fileSize = (attributes[.size] as? NSNumber)?.uint64Value ?? 0
    guard fileSize > 0 else { return [] }

    let handle = try FileHandle(forReadingFrom: fileURL)
    defer { try? handle.close() }

    let chunkSize = 64 * 1024
    var offset = fileSize
    var carriedLineSuffix = Data()
    var newestFirst: [String] = []
    newestFirst.reserveCapacity(targetLineCount)

    while offset > 0, newestFirst.count < targetLineCount {
      let readSize = min(UInt64(chunkSize), offset)
      offset -= readSize
      try handle.seek(toOffset: offset)
      var chunk = try handle.read(upToCount: Int(readSize)) ?? Data()
      if !carriedLineSuffix.isEmpty {
        chunk.append(carriedLineSuffix)
        carriedLineSuffix.removeAll(keepingCapacity: true)
      }

      let completeRange: Range<Data.Index>
      if offset > 0 {
        guard let firstNewline = chunk.firstIndex(of: 0x0A) else {
          carriedLineSuffix = chunk
          continue
        }
        if firstNewline > chunk.startIndex {
          carriedLineSuffix = chunk.subdata(in: chunk.startIndex..<firstNewline)
        }
        completeRange = chunk.index(after: firstNewline)..<chunk.endIndex
      } else {
        completeRange = chunk.startIndex..<chunk.endIndex
      }

      guard !completeRange.isEmpty else { continue }
      let completeData = chunk.subdata(in: completeRange)
      guard let text = String(data: completeData, encoding: .utf8), !text.isEmpty else { continue }

      for line in text.split(separator: "\n", omittingEmptySubsequences: true).reversed() {
        let line = String(line)
        if let matchingEventTypes, !lineContainsAnyEventType(line, eventTypes: matchingEventTypes) {
          continue
        }
        newestFirst.append(line)
        if newestFirst.count >= targetLineCount {
          break
        }
      }
    }

    return newestFirst.reversed()
  }

  private func lineContainsAnyEventType(_ line: String, eventTypes: Set<String>) -> Bool {
    for eventType in eventTypes where line.contains(eventType) {
      return true
    }
    return false
  }

  private func appendPrivateData(_ data: Data, to fileURL: URL) throws {
    let descriptor = open(
      fileURL.path,
      O_WRONLY | O_CREAT | O_APPEND | O_CLOEXEC,
      RuntimePermissions.privateFileMode
    )
    guard descriptor >= 0 else {
      throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
    }
    defer { close(descriptor) }

    guard fchmod(descriptor, RuntimePermissions.privateFileMode) == 0 else {
      throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
    }

    try data.withUnsafeBytes { buffer in
      guard let baseAddress = buffer.baseAddress else { return }
      var pointer = baseAddress
      var remaining = buffer.count
      while remaining > 0 {
        let written = Darwin.write(descriptor, pointer, remaining)
        if written < 0 {
          if errno == EINTR {
            continue
          }
          throw POSIXError(POSIXErrorCode(rawValue: errno) ?? .EIO)
        }
        guard written > 0 else {
          throw POSIXError(.EIO)
        }
        pointer = pointer.advanced(by: written)
        remaining -= written
      }
    }
  }
}

private struct LogFileCandidate {
  var url: URL
  var modifiedAt: Date
}

private extension UXEventAnchorKind {
  static let allCaptureEventTypes: Set<String> = [
    UXEventAnchorKind.scrollBurst.captureEventType,
    UXEventAnchorKind.pointer.captureEventType,
    UXEventAnchorKind.modifiers.captureEventType,
    UXEventAnchorKind.keyboardActivity.captureEventType
  ]
}

public struct StoredCaptureSnapshot: Sendable {
  public var fileURL: URL
  public var envelope: CaptureEventEnvelope<CaptureSnapshot>
  public var snapshot: CaptureSnapshot

  public init(
    fileURL: URL,
    envelope: CaptureEventEnvelope<CaptureSnapshot>,
    snapshot: CaptureSnapshot
  ) {
    self.fileURL = fileURL
    self.envelope = envelope
    self.snapshot = snapshot
  }
}

public struct StoredCaptureEvent<Payload: Codable & Sendable>: Sendable {
  public var fileURL: URL
  public var envelope: CaptureEventEnvelope<Payload>
  public var payload: Payload

  public init(
    fileURL: URL,
    envelope: CaptureEventEnvelope<Payload>,
    payload: Payload
  ) {
    self.fileURL = fileURL
    self.envelope = envelope
    self.payload = payload
  }
}
