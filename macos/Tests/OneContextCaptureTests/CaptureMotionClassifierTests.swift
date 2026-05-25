import CoreMedia
import XCTest
@testable import OneContextCapture

final class CaptureMotionClassifierTests: XCTestCase {
  func testClassifiesIdleWhenNothingChanges() {
    let classifier = CaptureMotionClassifier()

    XCTAssertEqual(classifier.classify(features()), .idle)
  }

  func testFocusedStaticWindowUsesWatchMode() {
    let classifier = CaptureMotionClassifier()

    XCTAssertEqual(classifier.classify(features(focused: true)), .watch)
  }

  func testKeyboardOrOCRNoveltyUsesActiveTextMode() {
    let classifier = CaptureMotionClassifier()

    XCTAssertEqual(classifier.classify(features(keyboardEventRecently: true)), .activeText)
    XCTAssertEqual(classifier.classify(features(ocrNewLineRate: 1.2)), .activeText)
  }

  func testVerticalFlowUsesScrollingTextMode() {
    let classifier = CaptureMotionClassifier()

    XCTAssertEqual(
      classifier.classify(features(dirtyAreaRatio: 0.08, estimatedDY: -18)),
      .scrollingText
    )
  }

  func testBroadNonVerticalChangeUsesVideoMotionMode() {
    let classifier = CaptureMotionClassifier()

    let decision = classifier.policy(for: features(changedTileRatio: 0.7, estimatedDY: 0.5))

    XCTAssertEqual(decision.mode, .videoMotion)
    XCTAssertEqual(decision.targetCaptureFPS, 1)
    XCTAssertFalse(decision.shouldEncodeVideoSegment)
    XCTAssertFalse(decision.shouldStoreKeyframe)
    XCTAssertFalse(decision.shouldOCRDirtyRegions)
  }

  func testModeControllerEscalatesImmediatelyAndDecaysSlowly() {
    var controller = CaptureModeController(lastEscalatedAt: Date(timeIntervalSince1970: 100))

    let start = Date(timeIntervalSince1970: 100)
    let scrolling = controller.update(
      features: features(dirtyAreaRatio: 0.09, estimatedDY: 14, scrollEventRecently: true),
      now: start
    )
    XCTAssertEqual(scrolling.mode, .scrollingText)

    let tooSoon = controller.update(features: features(), now: start.addingTimeInterval(2))
    XCTAssertEqual(tooSoon.mode, .scrollingText)

    let quietLongEnough = controller.update(features: features(), now: start.addingTimeInterval(9))
    XCTAssertEqual(quietLongEnough.mode, .idle)
  }

  func testAdaptiveMetadataControllerStartsFromFocusedWatchDecision() {
    var controller = ActiveWindowMetadataAdaptiveController()
    let decision = controller.start(
      target: target(),
      now: Date(timeIntervalSince1970: 100)
    )

    XCTAssertEqual(decision.classifierMode, .watch)
    XCTAssertEqual(decision.controllerMode, .watch)
    XCTAssertEqual(decision.proposedTargetFPS, 3)
    XCTAssertEqual(decision.targetFPS, 3)
    XCTAssertEqual(decision.updateReason, .initial)
    XCTAssertTrue(decision.shouldUpdateStreamConfiguration)
    XCTAssertEqual(decision.minimumFrameIntervalSeconds, 1.0 / 3.0, accuracy: 0.0001)
  }

  func testAdaptiveMetadataControllerEscalatesFromDirtyScrollHints() {
    var controller = ActiveWindowMetadataAdaptiveController()
    let start = Date(timeIntervalSince1970: 100)
    _ = controller.start(target: target(), now: start)

    let decision = controller.update(
      features: features(
        dirtyAreaRatio: 0.08,
        dirtyRectCount: 4,
        changedTileRatio: 0.2,
        estimatedDY: -18,
        scrollEventRecently: true,
        focused: true
      ),
      uxMotionHintsFused: true,
      now: start.addingTimeInterval(1)
    )

    XCTAssertEqual(decision.classifierMode, .scrollingText)
    XCTAssertEqual(decision.controllerMode, .scrollingText)
    XCTAssertEqual(decision.previousTargetFPS, 3)
    XCTAssertEqual(decision.proposedTargetFPS, 30)
    XCTAssertEqual(decision.targetFPS, 30)
    XCTAssertEqual(decision.updateReason, .modeChanged)
    XCTAssertTrue(decision.shouldUpdateStreamConfiguration)
    XCTAssertTrue(decision.uxMotionHintsFused)
  }

  func testAdaptiveMetadataControllerHoldsSmallDowngradeDuringConfigurationHysteresis() {
    let start = Date(timeIntervalSince1970: 100)
    var controller = ActiveWindowMetadataAdaptiveController(
      policy: ActiveWindowMetadataAdaptivePolicy(downgradeHysteresisSeconds: 1.5),
      currentTargetFPS: 10,
      lastConfigurationUpdateAt: start
    )

    let held = controller.update(
      features: features(),
      uxMotionHintsFused: false,
      now: start.addingTimeInterval(0.5)
    )
    XCTAssertEqual(held.proposedTargetFPS, 1)
    XCTAssertEqual(held.targetFPS, 10)
    XCTAssertEqual(held.updateReason, .hysteresisHold)
    XCTAssertFalse(held.shouldUpdateStreamConfiguration)

    let downgraded = controller.update(
      features: features(),
      uxMotionHintsFused: false,
      now: start.addingTimeInterval(2)
    )
    XCTAssertEqual(downgraded.targetFPS, 1)
    XCTAssertEqual(downgraded.updateReason, .fpsDecrease)
    XCTAssertTrue(downgraded.shouldUpdateStreamConfiguration)
  }

  func testActiveWindowMetadataConfigurationUsesAdaptiveFPS() {
    guard #available(macOS 13.0, *) else { return }

    let configuration = ActiveWindowMetadataStream.configuration(for: target(), targetFPS: 30)

    XCTAssertEqual(CMTimeGetSeconds(configuration.minimumFrameInterval), 1.0 / 30.0, accuracy: 0.0001)
    XCTAssertEqual(configuration.queueDepth, 1)
    XCTAssertFalse(configuration.showsCursor)
    XCTAssertFalse(configuration.capturesAudio)
    XCTAssertLessThanOrEqual(configuration.width, 480)
    XCTAssertLessThanOrEqual(configuration.height, 480)
  }

  private func features(
    dirtyAreaRatio: Double = 0,
    dirtyRectCount: Int = 0,
    meanPixelDiff: Double = 0,
    changedTileRatio: Double = 0,
    estimatedDY: Double = 0,
    scrollEventRecently: Bool = false,
    keyboardEventRecently: Bool = false,
    ocrNewLineRate: Double = 0,
    focused: Bool = false
  ) -> MotionFeatures {
    MotionFeatures(
      dirtyAreaRatio: dirtyAreaRatio,
      dirtyRectCount: dirtyRectCount,
      meanPixelDiff: meanPixelDiff,
      changedTileRatio: changedTileRatio,
      estimatedDY: estimatedDY,
      scrollEventRecently: scrollEventRecently,
      keyboardEventRecently: keyboardEventRecently,
      ocrNewLineRate: ocrNewLineRate,
      focused: focused
    )
  }

  private func target() -> ActiveWindowMetadataTarget {
    ActiveWindowMetadataTarget(
      windowID: 7,
      appPID: 42,
      appName: "Terminal",
      bundleID: "com.apple.Terminal",
      title: "build",
      framePoints: CaptureRect(x: 0, y: 0, width: 1280, height: 800),
      framePixels: CaptureRect(x: 0, y: 0, width: 1280, height: 800),
      zRank: 0,
      isFocused: true,
      captureEligible: true,
      source: "test"
    )
  }
}
