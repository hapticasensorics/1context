import Darwin
import Foundation
import OneContextPlatform
import XCTest
@testable import OneContextCapture

final class CaptureAlgorithmBenchmarkTests: XCTestCase {
  func testCaptureAlgorithmBenchmarks() throws {
    try XCTSkipUnless(
      ProcessInfo.processInfo.environment["ONECONTEXT_CAPTURE_BENCHMARKS"] == "1",
      "Set ONECONTEXT_CAPTURE_BENCHMARKS=1 to run algorithm benchmarks."
    )

    let parserIterations = envInt("ONECONTEXT_CAPTURE_BENCH_PARSER_ITERATIONS", defaultValue: 10_000)
    let uxBatches = envInt("ONECONTEXT_CAPTURE_BENCH_UX_BATCHES", defaultValue: 250)
    let storeReads = envInt("ONECONTEXT_CAPTURE_BENCH_STORE_READS", defaultValue: 100)
    let parserRects = makeDirtyRects(count: 96)
    try benchmark("parser_summarize_\(parserIterations)") {
      var checksum = 0.0
      for index in 0..<parserIterations {
        let summary = SCStreamFrameMetadataParser.summarize(
          dirtyRects: parserRects,
          bounds: CaptureRect(x: 0, y: 0, width: 1440, height: 900),
          previousWeightedCenterY: Double(index % 100),
          tileGridDimension: 16,
          maxRects: 32
        )
        checksum += summary.dirtyAreaRatio + summary.changedTileRatio + summary.estimatedDY
      }
      XCTAssertNotEqual(checksum, 0)
    }

    try benchmark("ux_aggregator_ingest_\(uxBatches * 100)") {
      let aggregator = UXEventAggregator(scrollBurstGap: 0.12, keyboardBurstGap: 0.12)
      var emitted = 0
      let base = Date(timeIntervalSince1970: 1_779_552_000)
      for batch in 0..<uxBatches {
        var events: [UXEventPrimitive] = []
        events.reserveCapacity(100)
        for index in 0..<100 {
          events.append(UXEventPrimitive(
            time: base.addingTimeInterval(Double(batch * 100 + index) * 0.001),
            kind: index % 11 == 0 ? .keyDown : .scrollWheel,
            scrollDeltaY: Double((index % 7) - 3),
            modifierFlagsRaw: index % 11 == 0 ? CGEventFlags.maskCommand.rawValue : 0,
            targetProcessID: 42
          ))
        }
        emitted += aggregator.ingest(events, now: base.addingTimeInterval(Double(batch) + 0.5)).count
      }
      emitted += aggregator.flush(now: base.addingTimeInterval(2_000)).count
      XCTAssertGreaterThan(emitted, 0)
    }

    try benchmark("store_latest_tail_\(storeReads)_reads") {
      let root = temporaryRoot()
      defer { try? FileManager.default.removeItem(at: root) }
      let runtime = runtimePaths(root: root)
      let store = OneContextCaptureLogStore(runtimePaths: runtime)
      try store.prepare()
      let paths = OneContextCapturePaths(runtimePaths: runtime)
      let logURL = paths.eventsDirectory.appendingPathComponent("2026-05-24.events.jsonl")
      let paddingLine = #"{"eventType":"noise","durability":"best_effort","payload":{"text":"padding"}}"# + "\n"
      let padding = String(repeating: paddingLine, count: 30_000)
      try RuntimePermissions.writePrivateData(Data(padding.utf8), to: logURL)
      try store.appendUXEventAnchors([
        uxAnchor(kind: .scrollBurst, endedAt: "2026-05-24T10:12:12.123Z"),
        uxAnchor(kind: .keyboardActivity, endedAt: "2026-05-24T10:12:13.123Z")
      ])

      var count = 0
      for _ in 0..<storeReads {
        count += try store.latestUXEventAnchors(limit: 2).count
      }
      XCTAssertEqual(count, storeReads * 2)
    }
  }

  private func benchmark(_ name: String, run: () throws -> Void) throws {
    let start = BenchmarkSample.current()
    try run()
    let end = BenchmarkSample.current()
    let wall = end.wall - start.wall
    let cpu = end.cpu - start.cpu
    let rssDeltaMB = max(0, end.maxRSSBytes - start.maxRSSBytes) / 1_048_576
    print(
      String(
        format: "BENCH %@ wall=%.4fs cpu=%.4fs max_rss_delta=%lluMB",
        name,
        wall,
        cpu,
        rssDeltaMB
      )
    )
  }

  private func envInt(_ name: String, defaultValue: Int) -> Int {
    guard let raw = ProcessInfo.processInfo.environment[name],
      let parsed = Int(raw)
    else {
      return defaultValue
    }
    return max(1, parsed)
  }

  private struct BenchmarkSample {
    var wall: TimeInterval
    var cpu: TimeInterval
    var maxRSSBytes: UInt64

    static func current() -> BenchmarkSample {
      var usage = rusage()
      getrusage(RUSAGE_SELF, &usage)
      return BenchmarkSample(
        wall: Date().timeIntervalSinceReferenceDate,
        cpu: timeInterval(usage.ru_utime) + timeInterval(usage.ru_stime),
        maxRSSBytes: UInt64(max(0, usage.ru_maxrss))
      )
    }

    private static func timeInterval(_ value: timeval) -> TimeInterval {
      TimeInterval(value.tv_sec) + TimeInterval(value.tv_usec) / 1_000_000
    }
  }

  private func makeDirtyRects(count: Int) -> [CaptureRect] {
    var rects: [CaptureRect] = []
    rects.reserveCapacity(count)
    for index in 0..<count {
      rects.append(CaptureRect(
        x: Double((index * 31) % 1320),
        y: Double((index * 47) % 820),
        width: Double(12 + (index % 41)),
        height: Double(10 + (index % 37))
      ))
    }
    return rects
  }

  private func uxAnchor(kind: UXEventAnchorKind, endedAt: String) -> UXEventAnchor {
    UXEventAnchor(
      kind: kind,
      startedAt: "2026-05-24T10:12:11.123Z",
      endedAt: endedAt,
      scroll: kind == .scrollBurst
        ? UXScrollBurstSummary(
          eventCount: 1,
          totalDX: 0,
          totalDY: -2,
          maxAbsDY: 2,
          momentumEventCount: 0,
          durationMilliseconds: 10
        )
        : nil,
      keyboardActivity: kind == .keyboardActivity
        ? UXKeyboardActivitySummary(
          eventCount: 1,
          keyDownCount: 1,
          keyUpCount: 0,
          autoRepeatCount: 0,
          modifiedKeyEventCount: 1,
          durationMilliseconds: 10
        )
        : nil
    )
  }

  private func runtimePaths(root: URL) -> RuntimePaths {
    RuntimePaths(
      userContentDirectory: root.appendingPathComponent("user", isDirectory: true),
      appSupportDirectory: root.appendingPathComponent("support", isDirectory: true),
      logDirectory: root.appendingPathComponent("logs", isDirectory: true),
      cacheDirectory: root.appendingPathComponent("cache", isDirectory: true),
      socketPath: root.appendingPathComponent("run/1context.sock").path,
      preferencesPath: root.appendingPathComponent("prefs.plist").path
    )
  }

  private func temporaryRoot() -> URL {
    FileManager.default.temporaryDirectory
      .appendingPathComponent("1ctx-capture-benchmarks-\(UUID().uuidString)", isDirectory: true)
  }
}
