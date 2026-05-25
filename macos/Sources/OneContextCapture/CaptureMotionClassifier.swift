import Foundation

public struct CaptureMotionClassifier: Sendable {
  public init() {}

  public func classify(_ features: MotionFeatures) -> CaptureMode {
    if features.changedTileRatio > 0.45 && abs(features.estimatedDY) < 2 {
      return .videoMotion
    }

    if abs(features.estimatedDY) > 3 && (features.scrollEventRecently || features.dirtyAreaRatio > 0.03) {
      return .scrollingText
    }

    if features.ocrNewLineRate > 0.5 || features.keyboardEventRecently {
      return .activeText
    }

    if features.dirtyAreaRatio > 0.005 || features.focused {
      return .watch
    }

    return .idle
  }

  public func policy(for features: MotionFeatures) -> CapturePolicyDecision {
    let mode = classify(features)
    return CapturePolicyDecision(
      mode: mode,
      targetCaptureFPS: mode.targetCaptureFPS,
      targetAnalysisFPS: targetAnalysisFPS(for: mode),
      shouldStoreKeyframe: shouldStoreKeyframe(for: mode),
      shouldOCRDirtyRegions: shouldOCRDirtyRegions(for: mode),
      shouldEncodeVideoSegment: false
    )
  }

  private func targetAnalysisFPS(for mode: CaptureMode) -> Int {
    switch mode {
    case .idle: return 1
    case .watch: return 2
    case .activeText: return 10
    case .scrollingText: return 30
    case .videoMotion: return 1
    }
  }

  private func shouldStoreKeyframe(for mode: CaptureMode) -> Bool {
    switch mode {
    case .idle:
      return false
    case .watch, .activeText, .scrollingText:
      return true
    case .videoMotion:
      return false
    }
  }

  private func shouldOCRDirtyRegions(for mode: CaptureMode) -> Bool {
    switch mode {
    case .idle, .videoMotion:
      return false
    case .watch, .activeText, .scrollingText:
      return true
    }
  }
}

public struct CaptureModeController: Sendable {
  public var currentMode: CaptureMode
  public var lastEscalatedAt: Date
  public var classifier: CaptureMotionClassifier

  public init(
    currentMode: CaptureMode = .idle,
    lastEscalatedAt: Date = .distantPast,
    classifier: CaptureMotionClassifier = CaptureMotionClassifier()
  ) {
    self.currentMode = currentMode
    self.lastEscalatedAt = lastEscalatedAt
    self.classifier = classifier
  }

  public mutating func update(features: MotionFeatures, now: Date = Date()) -> CapturePolicyDecision {
    let proposed = classifier.classify(features)
    if proposed > currentMode {
      currentMode = proposed
      lastEscalatedAt = now
    } else if proposed < currentMode && now.timeIntervalSince(lastEscalatedAt) >= downgradeDelay(for: currentMode) {
      currentMode = proposed
    }

    return CapturePolicyDecision(
      mode: currentMode,
      targetCaptureFPS: currentMode.targetCaptureFPS,
      targetAnalysisFPS: classifier.policy(for: features).targetAnalysisFPS,
      shouldStoreKeyframe: currentMode != .idle && currentMode != .videoMotion,
      shouldOCRDirtyRegions: currentMode != .idle && currentMode != .videoMotion,
      shouldEncodeVideoSegment: false
    )
  }

  private func downgradeDelay(for mode: CaptureMode) -> TimeInterval {
    switch mode {
    case .idle:
      return 0
    case .watch:
      return 2
    case .activeText:
      return 3
    case .scrollingText, .videoMotion:
      return 8
    }
  }
}
