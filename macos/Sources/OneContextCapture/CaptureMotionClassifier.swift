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

public struct ActiveWindowMetadataAdaptiveController: Sendable {
  public var modeController: CaptureModeController
  public var policy: ActiveWindowMetadataAdaptivePolicy
  public private(set) var currentTargetFPS: Int?
  public private(set) var lastConfigurationUpdateAt: Date
  public private(set) var latestDecision: ActiveWindowMetadataAdaptiveDecision?

  public init(
    modeController: CaptureModeController = CaptureModeController(),
    policy: ActiveWindowMetadataAdaptivePolicy = ActiveWindowMetadataAdaptivePolicy(),
    currentTargetFPS: Int? = nil,
    lastConfigurationUpdateAt: Date = .distantPast
  ) {
    self.modeController = modeController
    self.policy = policy
    self.currentTargetFPS = currentTargetFPS
    self.lastConfigurationUpdateAt = lastConfigurationUpdateAt
  }

  public mutating func start(
    target: ActiveWindowMetadataTarget,
    now: Date = Date()
  ) -> ActiveWindowMetadataAdaptiveDecision {
    update(
      features: .zero(focused: target.isFocused),
      uxMotionHintsFused: false,
      now: now,
      forceConfigurationUpdate: true
    )
  }

  public mutating func update(
    frame metadata: ActiveWindowFrameMetadata,
    now: Date = Date()
  ) -> ActiveWindowMetadataAdaptiveDecision {
    update(
      features: metadata.motionFeatures,
      uxMotionHintsFused: metadata.uxMotionHintsFused,
      now: now
    )
  }

  public mutating func update(
    features: MotionFeatures,
    uxMotionHintsFused: Bool,
    now: Date = Date(),
    forceConfigurationUpdate: Bool = false
  ) -> ActiveWindowMetadataAdaptiveDecision {
    let previousMode = modeController.currentMode
    let previousTargetFPS = currentTargetFPS
    let classifierMode = modeController.classifier.classify(features)
    let policyDecision = modeController.update(features: features, now: now)
    let proposedTargetFPS = policy.targetFPS(for: policyDecision, features: features)
    let configurationDecision = decideConfigurationUpdate(
      previousMode: previousMode,
      controllerMode: policyDecision.mode,
      previousTargetFPS: previousTargetFPS,
      proposedTargetFPS: proposedTargetFPS,
      now: now,
      forceConfigurationUpdate: forceConfigurationUpdate
    )

    if configurationDecision.shouldUpdate {
      currentTargetFPS = configurationDecision.targetFPS
      lastConfigurationUpdateAt = now
    }

    let targetFPS = configurationDecision.targetFPS
    let decision = ActiveWindowMetadataAdaptiveDecision(
      classifierMode: classifierMode,
      controllerMode: policyDecision.mode,
      proposedTargetFPS: proposedTargetFPS,
      targetFPS: targetFPS,
      previousTargetFPS: previousTargetFPS,
      targetAnalysisFPS: policyDecision.targetAnalysisFPS,
      minimumFrameIntervalSeconds: policy.minimumFrameIntervalSeconds(for: targetFPS),
      shouldUpdateStreamConfiguration: configurationDecision.shouldUpdate,
      updateReason: configurationDecision.reason,
      shouldStoreKeyframe: policyDecision.shouldStoreKeyframe,
      shouldOCRDirtyRegions: policyDecision.shouldOCRDirtyRegions,
      shouldEncodeVideoSegment: policyDecision.shouldEncodeVideoSegment,
      dirtyRectCount: features.dirtyRectCount,
      dirtyAreaRatio: features.dirtyAreaRatio,
      changedTileRatio: features.changedTileRatio,
      estimatedDY: features.estimatedDY,
      scrollEventRecently: features.scrollEventRecently,
      keyboardEventRecently: features.keyboardEventRecently,
      uxMotionHintsFused: uxMotionHintsFused
    )
    latestDecision = decision
    return decision
  }

  private func decideConfigurationUpdate(
    previousMode: CaptureMode,
    controllerMode: CaptureMode,
    previousTargetFPS: Int?,
    proposedTargetFPS: Int,
    now: Date,
    forceConfigurationUpdate: Bool
  ) -> (targetFPS: Int, shouldUpdate: Bool, reason: ActiveWindowMetadataConfigurationUpdateReason) {
    guard let previousTargetFPS else {
      return (proposedTargetFPS, true, .initial)
    }
    if forceConfigurationUpdate {
      return (proposedTargetFPS, true, .initial)
    }
    guard proposedTargetFPS != previousTargetFPS else {
      return (previousTargetFPS, false, .unchanged)
    }

    if proposedTargetFPS > previousTargetFPS {
      let reason: ActiveWindowMetadataConfigurationUpdateReason =
        previousMode != controllerMode ? .modeChanged : .fpsIncrease
      return (proposedTargetFPS, true, reason)
    }

    if now.timeIntervalSince(lastConfigurationUpdateAt) < policy.downgradeHysteresisSeconds {
      return (previousTargetFPS, false, .hysteresisHold)
    }

    let reason: ActiveWindowMetadataConfigurationUpdateReason =
      previousMode != controllerMode ? .modeChanged : .fpsDecrease
    return (proposedTargetFPS, true, reason)
  }
}
