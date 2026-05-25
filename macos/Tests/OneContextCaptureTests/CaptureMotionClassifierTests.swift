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
}
