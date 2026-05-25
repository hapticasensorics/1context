import Darwin
import XCTest
@testable import OneContextCapture

final class SCStreamFrameMetadataParserTests: XCTestCase {
  func testLocalDirtyRectParsingBenchmark() throws {
    guard ProcessInfo.processInfo.environment["ONECONTEXT_RUN_METADATA_PARSER_BENCHMARK"] == "1" else {
      throw XCTSkip("Set ONECONTEXT_RUN_METADATA_PARSER_BENCHMARK=1 to run the local parser benchmark.")
    }

    let iterations = Int(ProcessInfo.processInfo.environment["ONECONTEXT_METADATA_PARSER_BENCHMARK_ITERATIONS"] ?? "")
      ?? 20_000
    let parser = SCStreamFrameMetadataParser(tileGridDimension: 16, maxRects: 32)
    let target = target()
    let dirtyRects = Self.benchmarkDirtyRects()
    let attachments: [String: Any] = [
      SCStreamFrameMetadataAttachmentKey.status: 0,
      SCStreamFrameMetadataAttachmentKey.displayTime: UInt64(1234),
      SCStreamFrameMetadataAttachmentKey.scaleFactor: 1,
      SCStreamFrameMetadataAttachmentKey.contentScale: 1,
      SCStreamFrameMetadataAttachmentKey.contentRect: CaptureRect(x: 0, y: 0, width: 1440, height: 900),
      SCStreamFrameMetadataAttachmentKey.dirtyRects: dirtyRects
    ]

    var checksum = 0.0
    let rssBefore = Self.maxRSSBytes()
    let start = DispatchTime.now().uptimeNanoseconds
    for index in 0..<iterations {
      let metadata = parser.parse(
        attachments: attachments,
        streamID: "bench-stream",
        sequence: index,
        target: target,
        previousWeightedCenterY: index.isMultiple(of: 2) ? 430 : nil,
        capturedAt: "2026-05-24T10:11:12.123Z"
      )
      checksum += metadata.dirtyRectSummary.dirtyAreaRatio
      checksum += metadata.dirtyRectSummary.changedTileRatio
      checksum += metadata.motionFeatures.estimatedDY
      checksum += Double(metadata.dirtyRectSummary.cappedRects.count)
    }
    let elapsedNanoseconds = DispatchTime.now().uptimeNanoseconds - start
    let rssAfter = Self.maxRSSBytes()
    let elapsedMilliseconds = Double(elapsedNanoseconds) / 1_000_000
    let perParseMicroseconds = Double(elapsedNanoseconds) / Double(max(1, iterations)) / 1_000

    print(
      """
      metadata_parser_benchmark iterations=\(iterations) rects=\(dirtyRects.count) \
      elapsed_ms=\(String(format: "%.3f", elapsedMilliseconds)) \
      per_parse_us=\(String(format: "%.3f", perParseMicroseconds)) \
      max_rss_delta_bytes=\(rssAfter - rssBefore) checksum=\(String(format: "%.6f", checksum))
      """
    )
  }

  func testCompleteFrameExtractsDirtyRectMotionFeatures() {
    let parser = SCStreamFrameMetadataParser(tileGridDimension: 16, maxRects: 4)

    let metadata = parser.parse(
      attachments: [
        SCStreamFrameMetadataAttachmentKey.status: 0,
        SCStreamFrameMetadataAttachmentKey.displayTime: UInt64(1234),
        SCStreamFrameMetadataAttachmentKey.scaleFactor: 1,
        SCStreamFrameMetadataAttachmentKey.contentScale: 1,
        SCStreamFrameMetadataAttachmentKey.contentRect: CaptureRect(x: 0, y: 0, width: 160, height: 160),
        SCStreamFrameMetadataAttachmentKey.dirtyRects: [
          CaptureRect(x: 0, y: 0, width: 16, height: 16),
          CaptureRect(x: 144, y: 144, width: 16, height: 16)
        ]
      ],
      streamID: "stream-1",
      sequence: 1,
      target: target(),
      previousWeightedCenterY: nil,
      capturedAt: "2026-05-24T10:11:12.123Z"
    )

    XCTAssertEqual(metadata.frameStatus, .complete)
    XCTAssertEqual(metadata.frameStatusRawValue, 0)
    XCTAssertTrue(metadata.feedsMotionClassifier)
    XCTAssertEqual(metadata.dirtyRectSummary.dirtyRectCount, 2)
    XCTAssertEqual(metadata.dirtyRectSummary.cappedRects.count, 2)
    XCTAssertEqual(metadata.dirtyRectSummary.dirtyAreaRatio, 0.02, accuracy: 0.0001)
    XCTAssertEqual(metadata.dirtyRectSummary.changedTileRatio, 8.0 / 256.0, accuracy: 0.0001)
    XCTAssertEqual(metadata.dirtyRectSummary.unionRect, CaptureRect(x: 0, y: 0, width: 160, height: 160))
    XCTAssertEqual(try XCTUnwrap(metadata.dirtyRectSummary.weightedCenterY), 80, accuracy: 0.0001)
    XCTAssertEqual(metadata.motionFeatures.dirtyAreaRatio, metadata.dirtyRectSummary.dirtyAreaRatio)
    XCTAssertEqual(metadata.motionFeatures.dirtyRectCount, 2)
    XCTAssertEqual(metadata.motionFeatures.changedTileRatio, 8.0 / 256.0, accuracy: 0.0001)
    XCTAssertEqual(metadata.motionFeatures.estimatedDY, 0)
    XCTAssertEqual(metadata.displayTime, 1234)
    XCTAssertEqual(metadata.contentRect, CaptureRect(x: 0, y: 0, width: 160, height: 160))
  }

  func testEstimatedDYUsesWeightedCenterDelta() {
    let parser = SCStreamFrameMetadataParser(tileGridDimension: 16, maxRects: 4)

    let metadata = parser.parse(
      attachments: [
        SCStreamFrameMetadataAttachmentKey.status: 0,
        SCStreamFrameMetadataAttachmentKey.contentRect: CaptureRect(x: 0, y: 0, width: 160, height: 160),
        SCStreamFrameMetadataAttachmentKey.dirtyRects: [
          CaptureRect(x: 0, y: 40, width: 20, height: 20)
        ]
      ],
      streamID: "stream-1",
      sequence: 2,
      target: target(),
      previousWeightedCenterY: 30,
      capturedAt: "2026-05-24T10:11:12.223Z"
    )

    XCTAssertEqual(try XCTUnwrap(metadata.dirtyRectSummary.weightedCenterY), 50, accuracy: 0.0001)
    XCTAssertEqual(metadata.dirtyRectSummary.estimatedDY, 20, accuracy: 0.0001)
    XCTAssertEqual(metadata.motionFeatures.estimatedDY, 20, accuracy: 0.0001)
  }

  func testScrollHintProvidesEstimatedDYFallbackForCompatibleDirtyRects() {
    let parser = SCStreamFrameMetadataParser(tileGridDimension: 16, maxRects: 4)

    let metadata = parser.parse(
      attachments: [
        SCStreamFrameMetadataAttachmentKey.status: 0,
        SCStreamFrameMetadataAttachmentKey.contentRect: CaptureRect(x: 0, y: 0, width: 160, height: 160),
        SCStreamFrameMetadataAttachmentKey.dirtyRects: [
          CaptureRect(x: 20, y: 40, width: 90, height: 20)
        ]
      ],
      streamID: "stream-1",
      sequence: 7,
      target: target(),
      previousWeightedCenterY: nil,
      capturedAt: "2026-05-24T10:11:12.723Z",
      uxMotionHints: UXMotionHints(
        generatedAt: "2026-05-24T10:11:12.700Z",
        scrollEventRecently: true,
        keyboardActivityRecently: false,
        estimatedScrollDY: -18,
        focusedRecently: true
      )
    )

    XCTAssertTrue(metadata.uxMotionHintsFused)
    XCTAssertEqual(metadata.uxMotionHints?.estimatedScrollDY, -18)
    XCTAssertEqual(metadata.dirtyRectSummary.estimatedDY, 0)
    XCTAssertEqual(metadata.motionFeatures.estimatedDY, -18)
    XCTAssertTrue(metadata.motionFeatures.scrollEventRecently)
    XCTAssertTrue(metadata.motionFeatures.focused)
    XCTAssertEqual(CaptureMotionClassifier().classify(metadata.motionFeatures), .scrollingText)
  }

  func testKeyboardHintMarksIdleFrameAsActiveTextWithoutDirtyRects() {
    let metadata = SCStreamFrameMetadataParser().parse(
      attachments: [
        SCStreamFrameMetadataAttachmentKey.status: 1,
        SCStreamFrameMetadataAttachmentKey.contentRect: CaptureRect(x: 0, y: 0, width: 160, height: 160)
      ],
      streamID: "stream-1",
      sequence: 8,
      target: target(isFocused: false),
      previousWeightedCenterY: nil,
      capturedAt: "2026-05-24T10:11:12.823Z",
      uxMotionHints: UXMotionHints(
        generatedAt: "2026-05-24T10:11:12.800Z",
        scrollEventRecently: false,
        keyboardActivityRecently: true,
        estimatedScrollDY: 0,
        focusedRecently: true
      )
    )

    XCTAssertEqual(metadata.frameStatus, .idle)
    XCTAssertTrue(metadata.uxMotionHintsFused)
    XCTAssertTrue(metadata.motionFeatures.keyboardEventRecently)
    XCTAssertTrue(metadata.motionFeatures.focused)
    XCTAssertEqual(metadata.motionFeatures.dirtyRectCount, 0)
    XCTAssertEqual(CaptureMotionClassifier().classify(metadata.motionFeatures), .activeText)
  }

  func testIdleFrameFeedsZeroMotion() {
    let metadata = SCStreamFrameMetadataParser().parse(
      attachments: [
        SCStreamFrameMetadataAttachmentKey.status: 1,
        SCStreamFrameMetadataAttachmentKey.contentRect: CaptureRect(x: 0, y: 0, width: 160, height: 160),
        SCStreamFrameMetadataAttachmentKey.dirtyRects: [
          CaptureRect(x: 0, y: 0, width: 160, height: 160)
        ]
      ],
      streamID: "stream-1",
      sequence: 3,
      target: target(),
      previousWeightedCenterY: 10,
      capturedAt: "2026-05-24T10:11:12.323Z"
    )

    XCTAssertEqual(metadata.frameStatus, .idle)
    XCTAssertTrue(metadata.feedsMotionClassifier)
    XCTAssertEqual(metadata.dirtyRectSummary.dirtyRectCount, 1)
    XCTAssertEqual(metadata.motionFeatures, .zero(focused: true))
  }

  func testNonCompleteFramePersistsMetadataButDoesNotFeedMotion() {
    let metadata = SCStreamFrameMetadataParser().parse(
      attachments: [
        SCStreamFrameMetadataAttachmentKey.status: 3,
        SCStreamFrameMetadataAttachmentKey.dirtyRects: [
          CaptureRect(x: 0, y: 0, width: 160, height: 160)
        ]
      ],
      streamID: "stream-1",
      sequence: 4,
      target: target(),
      previousWeightedCenterY: 10,
      capturedAt: "2026-05-24T10:11:12.423Z"
    )

    XCTAssertEqual(metadata.frameStatus, .suspended)
    XCTAssertFalse(metadata.feedsMotionClassifier)
    XCTAssertEqual(metadata.dirtyRectSummary.dirtyRectCount, 1)
    XCTAssertEqual(metadata.motionFeatures, .zero(focused: true))
  }

  func testMissingAttachmentsAreTolerated() {
    let metadata = SCStreamFrameMetadataParser().parse(
      attachments: [:],
      attachmentsPresent: false,
      streamID: "stream-1",
      sequence: 5,
      target: target(),
      previousWeightedCenterY: nil,
      capturedAt: "2026-05-24T10:11:12.523Z"
    )

    XCTAssertEqual(metadata.frameStatus, .unknown)
    XCTAssertFalse(metadata.attachmentsPresent)
    XCTAssertFalse(metadata.feedsMotionClassifier)
    XCTAssertEqual(metadata.motionFeatures, .zero(focused: true))
    XCTAssertTrue(metadata.parseWarnings.contains("missing_attachments"))
    XCTAssertTrue(metadata.parseWarnings.contains("missing_status"))
  }

  func testMalformedDirtyRectsAreSkippedWithoutFailingFrame() {
    let metadata = SCStreamFrameMetadataParser(tileGridDimension: 16, maxRects: 1).parse(
      attachments: [
        SCStreamFrameMetadataAttachmentKey.status: 0,
        SCStreamFrameMetadataAttachmentKey.contentRect: CaptureRect(x: 0, y: 0, width: 100, height: 100),
        SCStreamFrameMetadataAttachmentKey.dirtyRects: [
          CaptureRect(x: 0, y: 0, width: 10, height: 10),
          "bad",
          CaptureRect(x: 20, y: 20, width: 0, height: 10)
        ] as [Any]
      ],
      streamID: "stream-1",
      sequence: 6,
      target: target(),
      previousWeightedCenterY: nil,
      capturedAt: "2026-05-24T10:11:12.623Z"
    )

    XCTAssertEqual(metadata.frameStatus, .complete)
    XCTAssertEqual(metadata.dirtyRectSummary.dirtyRectCount, 1)
    XCTAssertEqual(metadata.dirtyRectSummary.malformedRectCount, 2)
    XCTAssertEqual(metadata.dirtyRectSummary.cappedRects.count, 1)
    XCTAssertTrue(metadata.parseWarnings.contains("malformed_dirty_rects:2"))
  }

  func testActiveTargetSelectionPrefersFocusedEligibleWindow() {
    let snapshot = CaptureSnapshot(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: nil,
      displays: [],
      windows: [
        window(windowID: 20, zRank: 0, isFocused: false),
        window(windowID: 30, zRank: 1, isFocused: true),
        window(windowID: 40, zRank: 2, isFocused: true, captureEligible: false)
      ]
    )

    XCTAssertEqual(ActiveWindowMetadataTarget.select(from: snapshot)?.windowID, 30)
  }

  private func target(isFocused: Bool = true) -> ActiveWindowMetadataTarget {
    ActiveWindowMetadataTarget(
      windowID: 7,
      appPID: 42,
      appName: "Terminal",
      bundleID: "com.apple.Terminal",
      title: "build",
      framePoints: CaptureRect(x: 0, y: 0, width: 160, height: 160),
      zRank: 0,
      isFocused: isFocused,
      captureEligible: true,
      source: "test"
    )
  }

  private func window(
    windowID: UInt32,
    zRank: Int,
    isFocused: Bool,
    captureEligible: Bool = true
  ) -> CaptureWindowState {
    CaptureWindowState(
      time: "2026-05-24T10:11:12.123Z",
      windowID: windowID,
      appPID: Int32(windowID),
      appName: "App \(windowID)",
      bundleID: "com.example.\(windowID)",
      title: "main",
      framePoints: CaptureRect(x: 0, y: 0, width: 1280, height: 800),
      zRank: zRank,
      layer: 0,
      isFocused: isFocused,
      isOnScreen: true,
      isMinimized: false,
      captureEligible: captureEligible,
      source: "test"
    )
  }

  private static func benchmarkDirtyRects() -> [CaptureRect] {
    var rects: [CaptureRect] = []
    rects.reserveCapacity(192)
    for index in 0..<192 {
      let column = index % 24
      let row = index / 24
      let x = Double((column * 59 + row * 17) % 1420)
      let y = Double((row * 83 + column * 11) % 880)
      let width = Double(8 + (index % 13))
      let height = Double(6 + ((index * 7) % 19))
      rects.append(CaptureRect(x: x, y: y, width: width, height: height))
    }
    return rects
  }

  private static func maxRSSBytes() -> Int64 {
    var usage = rusage()
    guard getrusage(RUSAGE_SELF, &usage) == 0 else { return 0 }
    return Int64(usage.ru_maxrss)
  }
}
