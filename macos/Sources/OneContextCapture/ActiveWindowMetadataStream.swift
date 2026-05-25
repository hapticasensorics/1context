import CoreMedia
import Foundation
import OneContextPlatform
@preconcurrency import ScreenCaptureKit

public enum ActiveWindowMetadataStreamError: LocalizedError, Sendable {
  case noActiveWindow
  case activeWindowNotShareable(windowID: UInt32)
  case invalidSampleRequest

  public var errorDescription: String? {
    switch self {
    case .noActiveWindow:
      return "No capture-eligible active window was available for metadata sampling."
    case .activeWindowNotShareable(let windowID):
      return "Active window \(windowID) was not present in ScreenCaptureKit shareable content."
    case .invalidSampleRequest:
      return "Active-window metadata sample requires a positive duration and max frame count."
    }
  }
}

@available(macOS 13.0, *)
public final class ActiveWindowMetadataStream: NSObject, @unchecked Sendable, SCStreamOutput {
  public typealias UXMotionHintsProvider = @Sendable () -> UXMotionHints?
  public typealias FocusedContextHandler = @Sendable (CaptureAXFocusedContext) -> Void

  private let runtimePaths: RuntimePaths
  private let indexer: OneContextWindowIndexer
  private let parser: SCStreamFrameMetadataParser
  private let uxMotionHintsProvider: UXMotionHintsProvider?
  private let focusedContextHandler: FocusedContextHandler?
  private let stateQueue = DispatchQueue(label: "com.haptica.1context.active-window-metadata")
  private let frameLimitSemaphore = DispatchSemaphore(value: 0)
  private var session: ActiveWindowMetadataSession?

  public init(
    runtimePaths: RuntimePaths,
    indexer: OneContextWindowIndexer = OneContextWindowIndexer(),
    parser: SCStreamFrameMetadataParser = SCStreamFrameMetadataParser(),
    uxMotionHintsProvider: UXMotionHintsProvider? = nil,
    focusedContextHandler: FocusedContextHandler? = nil
  ) {
    self.runtimePaths = runtimePaths
    self.indexer = indexer
    self.parser = parser
    self.uxMotionHintsProvider = uxMotionHintsProvider
    self.focusedContextHandler = focusedContextHandler
  }

  public func sample(durationSeconds: TimeInterval = 3, maxFrames: Int = 30) async throws -> ActiveWindowMetadataSample {
    guard durationSeconds > 0, maxFrames > 0 else {
      throw ActiveWindowMetadataStreamError.invalidSampleRequest
    }

    let snapshot = await indexer.snapshot()
    guard let target = ActiveWindowMetadataTarget.select(from: snapshot) else {
      throw ActiveWindowMetadataStreamError.noActiveWindow
    }

    let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
    guard let scWindow = content.windows.first(where: { $0.windowID == target.windowID }) else {
      throw ActiveWindowMetadataStreamError.activeWindowNotShareable(windowID: target.windowID)
    }

    drainFrameLimitSemaphore()
    let streamID = UUID().uuidString
    let startedAt = SCStreamFrameMetadataParser.isoTimestamp()
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths)
    try store.prepare()
    var initialPersistErrors: [String] = []
    if let focusedContext = snapshot.focusedContext {
      focusedContextHandler?(focusedContext)
      do {
        try store.appendEvent(
          eventType: "capture.ax_focused_context",
          durability: .lossless,
          recordedAt: focusedContext.generatedAt,
          payload: focusedContext
        )
      } catch {
        initialPersistErrors.append("ax_focused_context: \(error.localizedDescription)")
      }
    }
    var adaptiveController = ActiveWindowMetadataAdaptiveController()
    let initialAdaptiveDecision = adaptiveController.start(target: target)
    stateQueue.sync {
      session = ActiveWindowMetadataSession(
        streamID: streamID,
        startedAt: startedAt,
        requestedDurationSeconds: durationSeconds,
        requestedMaxFrames: maxFrames,
        target: target,
        parser: parser,
        uxMotionHintsProvider: uxMotionHintsProvider,
        store: store,
        adaptiveController: adaptiveController,
        initialAdaptiveDecision: initialAdaptiveDecision
      )
      session?.persistErrors.append(contentsOf: initialPersistErrors)
    }

    let filter = SCContentFilter(desktopIndependentWindow: scWindow)
    let configuration = Self.configuration(for: target, targetFPS: initialAdaptiveDecision.targetFPS)
    let stream = SCStream(filter: filter, configuration: configuration, delegate: nil)
    let queue = DispatchQueue(label: "com.haptica.1context.active-window-metadata.sck")
    try stream.addStreamOutput(self, type: .screen, sampleHandlerQueue: queue)

    try await startCapture(stream)
    _ = await waitForFrameLimit(timeout: durationSeconds)
    try await stopCapture(stream)

    let endedAt = SCStreamFrameMetadataParser.isoTimestamp()
    return stateQueue.sync {
      defer { session = nil }
      return session?.summary(endedAt: endedAt) ?? ActiveWindowMetadataSample(
        streamID: streamID,
        startedAt: startedAt,
        endedAt: endedAt,
        requestedDurationSeconds: durationSeconds,
        requestedMaxFrames: maxFrames,
        target: target,
        frameCount: 0,
        completeFrameCount: 0,
        idleFrameCount: 0,
        nonCompleteFrameCount: 0,
        classifierFeedFrameCount: 0,
        persistedEventCount: 0,
        persistErrors: [],
        uxMotionHintsFusedFrameCount: 0,
        adaptiveDecisionCount: 0,
        configurationUpdateDecisionCount: initialAdaptiveDecision.shouldUpdateStreamConfiguration ? 1 : 0,
        configurationUpdateErrors: [],
        initialAdaptiveDecision: initialAdaptiveDecision,
        latestAdaptiveDecision: initialAdaptiveDecision,
        latestUXMotionHints: nil,
        latestFrame: nil,
        frames: []
      )
    }
  }

  public func stream(
    _ stream: SCStream,
    didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
    of type: SCStreamOutputType
  ) {
    guard type == .screen, CMSampleBufferIsValid(sampleBuffer) else { return }
    let result = stateQueue.sync { () -> ActiveWindowMetadataRecordResult in
      guard var session else { return .inactive }
      let result = session.record(sampleBuffer: sampleBuffer)
      self.session = session
      return result
    }
    if let decision = result.configurationDecision,
      decision.shouldUpdateStreamConfiguration,
      let target = result.target
    {
      applyConfigurationUpdate(stream, target: target, decision: decision)
    }
    if result.reachedFrameLimit {
      frameLimitSemaphore.signal()
    }
  }

  public static func configuration(
    for target: ActiveWindowMetadataTarget,
    targetFPS: Int
  ) -> SCStreamConfiguration {
    let configuration = SCStreamConfiguration()
    let width = max(64, target.framePixels.map { Int($0.width.rounded()) } ?? Int(target.framePoints.width.rounded()))
    let height = max(64, target.framePixels.map { Int($0.height.rounded()) } ?? Int(target.framePoints.height.rounded()))
    let maxDimension = 480
    let scale = min(1, Double(maxDimension) / Double(max(width, height)))
    configuration.width = max(64, Int((Double(width) * scale).rounded()))
    configuration.height = max(64, Int((Double(height) * scale).rounded()))
    let cappedFPS = min(30, max(1, targetFPS))
    configuration.minimumFrameInterval = CMTime(seconds: 1 / Double(cappedFPS), preferredTimescale: 600)
    configuration.queueDepth = 1
    configuration.showsCursor = false
    configuration.capturesAudio = false
    configuration.excludesCurrentProcessAudio = true
    return configuration
  }

  private func applyConfigurationUpdate(
    _ stream: SCStream,
    target: ActiveWindowMetadataTarget,
    decision: ActiveWindowMetadataAdaptiveDecision
  ) {
    let configuration = Self.configuration(for: target, targetFPS: decision.targetFPS)
    stream.updateConfiguration(configuration) { [weak self] error in
      guard let error, let self else { return }
      self.stateQueue.async {
        guard var session = self.session else { return }
        session.recordConfigurationUpdateError(
          "target_fps=\(decision.targetFPS) reason=\(decision.updateReason.rawValue): \(error.localizedDescription)"
        )
        self.session = session
      }
    }
  }

  private func drainFrameLimitSemaphore() {
    while frameLimitSemaphore.wait(timeout: .now()) == .success {}
  }

  private func waitForFrameLimit(timeout: TimeInterval) async -> Bool {
    await withCheckedContinuation { continuation in
      DispatchQueue.global(qos: .utility).async { [frameLimitSemaphore] in
        continuation.resume(returning: frameLimitSemaphore.wait(timeout: .now() + timeout) == .success)
      }
    }
  }

  private func startCapture(_ stream: SCStream) async throws {
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      stream.startCapture { error in
        if let error {
          continuation.resume(throwing: error)
        } else {
          continuation.resume()
        }
      }
    }
  }

  private func stopCapture(_ stream: SCStream) async throws {
    try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, Error>) in
      stream.stopCapture { error in
        if let error {
          continuation.resume(throwing: error)
        } else {
          continuation.resume()
        }
      }
    }
  }
}

private struct ActiveWindowMetadataRecordResult {
  var reachedFrameLimit: Bool
  var target: ActiveWindowMetadataTarget?
  var configurationDecision: ActiveWindowMetadataAdaptiveDecision?

  static let inactive = ActiveWindowMetadataRecordResult(
    reachedFrameLimit: false,
    target: nil,
    configurationDecision: nil
  )
}

private struct ActiveWindowMetadataSession {
  var streamID: String
  var startedAt: String
  var requestedDurationSeconds: Double
  var requestedMaxFrames: Int
  var target: ActiveWindowMetadataTarget
  var parser: SCStreamFrameMetadataParser
  var uxMotionHintsProvider: ActiveWindowMetadataStream.UXMotionHintsProvider?
  var store: OneContextCaptureLogStore
  var adaptiveController: ActiveWindowMetadataAdaptiveController
  var initialAdaptiveDecision: ActiveWindowMetadataAdaptiveDecision
  var sequence = 0
  var previousWeightedCenterY: Double?
  var frames: [ActiveWindowFrameMetadata] = []
  var persistedEventCount = 0
  var persistErrors: [String] = []
  var configurationUpdateErrors: [String] = []

  mutating func record(sampleBuffer: CMSampleBuffer) -> ActiveWindowMetadataRecordResult {
    sequence += 1
    let uxMotionHints = uxMotionHintsProvider?()
    var metadata = parser.parse(
      sampleBuffer: sampleBuffer,
      streamID: streamID,
      sequence: sequence,
      target: target,
      previousWeightedCenterY: previousWeightedCenterY,
      uxMotionHints: uxMotionHints
    )
    let adaptiveDecision = adaptiveController.update(frame: metadata)
    metadata.adaptiveDecision = adaptiveDecision
    if metadata.frameStatus == .complete,
      let center = metadata.dirtyRectSummary.weightedCenterY
    {
      previousWeightedCenterY = center
    }
    frames.append(metadata)
    do {
      try store.appendActiveWindowFrameMetadata(metadata)
      persistedEventCount += 1
    } catch {
      persistErrors.append(error.localizedDescription)
    }
    return ActiveWindowMetadataRecordResult(
      reachedFrameLimit: frames.count >= requestedMaxFrames,
      target: target,
      configurationDecision: adaptiveDecision
    )
  }

  mutating func recordConfigurationUpdateError(_ error: String) {
    configurationUpdateErrors.append(error)
  }

  func summary(endedAt: String) -> ActiveWindowMetadataSample {
    let complete = frames.filter { $0.frameStatus == .complete }.count
    let idle = frames.filter { $0.frameStatus == .idle }.count
    let feed = frames.filter(\.feedsMotionClassifier).count
    let hintFused = frames.filter(\.uxMotionHintsFused).count
    let latestHints = frames.last(where: { $0.uxMotionHints != nil })?.uxMotionHints
    let adaptiveDecisions = frames.compactMap(\.adaptiveDecision)
    let latestAdaptiveDecision = adaptiveDecisions.last ?? initialAdaptiveDecision
    let updateDecisionCount = adaptiveDecisions.filter(\.shouldUpdateStreamConfiguration).count
      + (initialAdaptiveDecision.shouldUpdateStreamConfiguration ? 1 : 0)
    return ActiveWindowMetadataSample(
      streamID: streamID,
      startedAt: startedAt,
      endedAt: endedAt,
      requestedDurationSeconds: requestedDurationSeconds,
      requestedMaxFrames: requestedMaxFrames,
      target: target,
      frameCount: frames.count,
      completeFrameCount: complete,
      idleFrameCount: idle,
      nonCompleteFrameCount: max(0, frames.count - complete - idle),
      classifierFeedFrameCount: feed,
      persistedEventCount: persistedEventCount,
      persistErrors: persistErrors,
      uxMotionHintsFusedFrameCount: hintFused,
      adaptiveDecisionCount: adaptiveDecisions.count,
      configurationUpdateDecisionCount: updateDecisionCount,
      configurationUpdateErrors: configurationUpdateErrors,
      initialAdaptiveDecision: initialAdaptiveDecision,
      latestAdaptiveDecision: latestAdaptiveDecision,
      latestUXMotionHints: latestHints,
      latestFrame: frames.last,
      frames: frames
    )
  }
}
