import XCTest
import OneContextPlatform
@testable import OneContextCapture

final class CaptureStoreTests: XCTestCase {
  func testCapturePathsUsePrivateAppSupportCaptureTree() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let paths = OneContextCapturePaths(runtimePaths: runtimePaths(root: root))

    try paths.ensureDirectories()

    XCTAssertTrue(FileManager.default.fileExists(atPath: paths.rootDirectory.path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: paths.windowsDirectory.path))
    XCTAssertTrue(FileManager.default.fileExists(atPath: paths.displayMediaDirectory.path))
    XCTAssertEqual(try mode(paths.rootDirectory), 0o700)
    XCTAssertEqual(try mode(paths.windowMediaDirectory), 0o700)
  }

  func testWindowSnapshotsAreAppendedAsLosslessJSONL() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let runtime = runtimePaths(root: root)
    let store = OneContextCaptureLogStore(runtimePaths: runtime)
    let snapshot = CaptureSnapshot(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.Terminal", appName: "Terminal"),
      displays: [],
      windows: [
        CaptureWindowState(
          time: "2026-05-24T10:11:12.123Z",
          windowID: 7,
          appPID: 42,
          appName: "Terminal",
          bundleID: "com.example.Terminal",
          title: "build logs",
          framePoints: CaptureRect(x: 10, y: 20, width: 800, height: 600),
          zRank: 0,
          layer: 0,
          isFocused: true,
          isOnScreen: true,
          isMinimized: false,
          captureEligible: true,
          source: "test"
        )
      ]
    )

    let logURL = try store.appendWindowSnapshot(snapshot)
    let text = try String(contentsOf: logURL, encoding: .utf8)

    XCTAssertTrue(text.contains("\"eventType\":\"capture.window_snapshot\""))
    XCTAssertTrue(text.contains("\"durability\":\"lossless\""))
    XCTAssertTrue(text.contains("\"windowID\":7"))
    XCTAssertEqual(try mode(logURL), 0o600)
  }

  func testLatestWindowSnapshotReadsMostRecentPersistedEnvelope() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let first = snapshot(generatedAt: "2026-05-24T10:11:12.123Z", windowID: 7, title: "old")
    let second = snapshot(generatedAt: "2026-05-24T10:12:12.123Z", windowID: 8, title: "new")

    try store.appendWindowSnapshot(first)
    try store.appendWindowSnapshot(second)

    let latest = try XCTUnwrap(store.latestWindowSnapshot())
    XCTAssertEqual(latest.envelope.eventType, "capture.window_snapshot")
    XCTAssertEqual(latest.snapshot.windows.first?.windowID, 8)
    XCTAssertEqual(latest.snapshot.windows.first?.title, "new")
  }

  func testSnapshotDecodesWithoutAXFocusedOptionalFields() throws {
    let json = """
      {
        "schemaVersion": 1,
        "generatedAt": "2026-05-24T10:12:12.123Z",
        "activeApplication": {
          "processID": 42,
          "bundleID": "com.example.Terminal",
          "appName": "Terminal"
        },
        "displays": [],
        "windows": [
          {
            "time": "2026-05-24T10:12:12.123Z",
            "windowID": 8,
            "appPID": 42,
            "appName": "Terminal",
            "bundleID": "com.example.Terminal",
            "title": "new",
            "framePoints": { "x": 10, "y": 20, "width": 800, "height": 600 },
            "zRank": 0,
            "layer": 0,
            "isFocused": true,
            "isOnScreen": true,
            "isMinimized": false,
            "captureEligible": true,
            "source": "test"
          }
        ]
      }
      """

    let snapshot = try JSONDecoder().decode(CaptureSnapshot.self, from: Data(json.utf8))

    XCTAssertNil(snapshot.focusedContext)
    XCTAssertNil(snapshot.windows.first?.focusMetadata)
  }

  func testOldMinimalCaptureEventEnvelopeStillDecodes() throws {
    let json = """
      {
        "schemaVersion": 1,
        "eventType": "capture.window_snapshot",
        "durability": "lossless",
        "recordedAt": "2026-05-24T10:12:12.123Z",
        "payload": {
          "schemaVersion": 1,
          "generatedAt": "2026-05-24T10:12:12.123Z",
          "activeApplication": null,
          "displays": [],
          "windows": []
        }
      }
      """

    let envelope = try JSONDecoder().decode(CaptureEventEnvelope<CaptureSnapshot>.self, from: Data(json.utf8))

    XCTAssertEqual(envelope.eventType, "capture.window_snapshot")
    XCTAssertEqual(envelope.recordedAt, "2026-05-24T10:12:12.123Z")
    XCTAssertNil(envelope.eventTimeStart)
    XCTAssertNil(envelope.ingestedAt)
    XCTAssertNil(envelope.privacyClass)
  }

  func testWindowSnapshotSerializesCanonicalTimelineSourceAndPrivacyFields() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let snapshot = snapshot(generatedAt: "2026-05-24T10:12:12.123Z", windowID: 8, title: "new")

    let logURL = try store.appendWindowSnapshot(snapshot)
    let text = try String(contentsOf: logURL, encoding: .utf8)
    let envelope = try firstEnvelope(from: logURL, payloadType: CaptureSnapshot.self)

    XCTAssertTrue(text.contains("\"event_time_start\":\"2026-05-24T10:12:12.123Z\""))
    XCTAssertTrue(text.contains("\"ingested_at\""))
    XCTAssertTrue(text.contains("\"privacy_class\":\"private_metadata\""))
    XCTAssertEqual(envelope.eventTimeStart, "2026-05-24T10:12:12.123Z")
    XCTAssertEqual(envelope.eventTimeEnd, "2026-05-24T10:12:12.123Z")
    XCTAssertNotNil(envelope.ingestedAt)
    XCTAssertEqual(envelope.laneID, "capture.windows")
    XCTAssertEqual(envelope.streamID, "com.example.Terminal")
    XCTAssertEqual(envelope.sourceRecordID, "window_snapshot:2026-05-24T10:12:12.123Z")
    XCTAssertEqual(envelope.captureBundleID, "window-capture:2026-05-24T10:12:12.123Z")
    XCTAssertEqual(envelope.privacyClass, .privateMetadata)
    XCTAssertEqual(envelope.privacyShape, .windowTopology)
    XCTAssertEqual(envelope.sourceClock, .systemUTC)
  }

  func testActiveWindowFrameMetadataIsBestEffortEventAndLatestReadable() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let metadata = activeWindowMetadata(
      capturedAt: "2026-05-24T10:12:12.123Z",
      sequence: 1,
      dirtyRectCount: 2
    )

    let logURL = try store.appendActiveWindowFrameMetadata(metadata)
    let text = try String(contentsOf: logURL, encoding: .utf8)

    XCTAssertTrue(text.contains("\"eventType\":\"capture.active_window_frame_metadata\""))
    XCTAssertTrue(text.contains("\"durability\":\"best_effort\""))
    XCTAssertTrue(text.contains("\"dirtyRectCount\":2"))
    XCTAssertEqual(try mode(logURL), 0o600)

    let latest = try XCTUnwrap(store.latestActiveWindowFrameMetadata())
    XCTAssertEqual(latest.envelope.eventType, "capture.active_window_frame_metadata")
    XCTAssertEqual(latest.payload.sequence, 1)
    XCTAssertEqual(latest.payload.dirtyRectSummary.dirtyRectCount, 2)
  }

  func testActiveWindowFrameMetadataSetsCanonicalEventTimeAndSourceFields() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let metadata = activeWindowMetadata(
      capturedAt: "2026-05-24T10:12:12.123Z",
      sequence: 7,
      dirtyRectCount: 2
    )

    let logURL = try store.appendActiveWindowFrameMetadata(metadata)
    let envelope = try firstEnvelope(from: logURL, payloadType: ActiveWindowFrameMetadata.self)

    XCTAssertEqual(envelope.eventTimeStart, "2026-05-24T10:12:12.123Z")
    XCTAssertEqual(envelope.eventTimeEnd, "2026-05-24T10:12:12.123Z")
    XCTAssertEqual(envelope.laneID, "capture.active_window_frames")
    XCTAssertEqual(envelope.streamID, "stream-1")
    XCTAssertEqual(envelope.sourceRecordID, "active_window_frame:stream-1:7")
    XCTAssertEqual(envelope.captureBundleID, "active-window:com.example.Terminal:7:stream-1")
    XCTAssertEqual(envelope.privacyClass, .privateMetadata)
    XCTAssertEqual(envelope.privacyShape, .frameMetadata)
    XCTAssertEqual(envelope.sourceClock, .screenCaptureKit)
  }

  func testUXEventAnchorsPersistAsBestEffortJSONLWithDashboardEventTypes() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let anchors = [
      uxAnchor(kind: .scrollBurst, endedAt: "2026-05-24T10:12:12.123Z"),
      uxAnchor(kind: .pointer, endedAt: "2026-05-24T10:12:13.123Z"),
      uxAnchor(kind: .keyboardActivity, endedAt: "2026-05-24T10:12:14.123Z"),
      uxAnchor(kind: .modifiers, endedAt: "2026-05-24T10:12:15.123Z"),
      uxAnchor(kind: .shortcut, endedAt: "2026-05-24T10:12:16.123Z"),
      uxAnchor(kind: .focusTransition, endedAt: "2026-05-24T10:12:17.123Z")
    ]

    let logURLs = try store.appendUXEventAnchors(anchors)
    let logURL = try XCTUnwrap(logURLs.last)
    let text = try String(contentsOf: logURL, encoding: .utf8)

    XCTAssertTrue(text.contains("\"eventType\":\"capture.ux.scroll_burst.v1\""))
    XCTAssertTrue(text.contains("\"eventType\":\"capture.ux.pointer.v1\""))
    XCTAssertTrue(text.contains("\"eventType\":\"capture.ux.keyboard_activity.v1\""))
    XCTAssertTrue(text.contains("\"eventType\":\"capture.ux.modifiers.v1\""))
    XCTAssertTrue(text.contains("\"eventType\":\"capture.ux.shortcut.v1\""))
    XCTAssertTrue(text.contains("\"eventType\":\"capture.ux.focus_transition.v1\""))
    XCTAssertEqual(text.split(separator: "\n").count, 6)
    XCTAssertEqual(try mode(logURL), 0o600)

    let latest = try XCTUnwrap(store.latestUXEventAnchor())
    XCTAssertEqual(latest.envelope.eventType, "capture.ux.focus_transition.v1")
    XCTAssertEqual(latest.payload.kind, .focusTransition)
  }

  func testUXEventAnchorsSetCanonicalEventTimeAndSharedBatchMetadata() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let anchors = [
      uxAnchor(kind: .scrollBurst, endedAt: "2026-05-24T10:12:12.123Z"),
      uxAnchor(kind: .keyboardActivity, endedAt: "2026-05-24T10:12:13.123Z")
    ]

    let logURL = try XCTUnwrap(try store.appendUXEventAnchors(anchors).last)
    let envelopes = try allEnvelopes(from: logURL, payloadType: UXEventAnchor.self)

    XCTAssertEqual(envelopes.count, 2)
    XCTAssertEqual(envelopes.map(\.eventTimeStart), [
      "2026-05-24T10:12:12.000Z",
      "2026-05-24T10:12:12.000Z"
    ])
    XCTAssertEqual(envelopes.map(\.eventTimeEnd), [
      "2026-05-24T10:12:12.123Z",
      "2026-05-24T10:12:13.123Z"
    ])
    XCTAssertEqual(Set(envelopes.compactMap(\.captureBundleID)).count, 1)
    XCTAssertEqual(envelopes.map(\.laneID), ["capture.ux", "capture.ux"])
    XCTAssertEqual(envelopes.map(\.streamID), ["ux.scroll_burst", "ux.keyboard_activity"])
    XCTAssertEqual(envelopes.map(\.privacyClass), [.interactionMetadata, .interactionMetadata])
    XCTAssertEqual(envelopes.map(\.privacyShape), [.uxAnchor, .uxAnchor])
    XCTAssertEqual(envelopes.map(\.sourceClock), [.cgEventTap, .cgEventTap])
  }

  func testAXSemanticEventsPersistAsBestEffortDashboardEvents() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let events = [
      CaptureAXSemanticEvent(
        kind: .focusedWindowChanged,
        generatedAt: "2026-05-24T10:12:12.123Z",
        activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.Terminal", appName: "Terminal"),
        focusedApplicationProcessID: 42,
        matchedWindowID: 7
      ),
      CaptureAXSemanticEvent(
        kind: .selectedTextChanged,
        generatedAt: "2026-05-24T10:12:13.123Z",
        selection: CaptureAXSelectionContext(selectedTextRedacted: true)
      )
    ]

    let logURLs = try store.appendAXSemanticEvents(events)
    let logURL = try XCTUnwrap(logURLs.last)
    let text = try String(contentsOf: logURL, encoding: .utf8)

    XCTAssertTrue(text.contains("\"eventType\":\"capture.ax_semantic.focused_window_changed.v1\""))
    XCTAssertTrue(text.contains("\"eventType\":\"capture.ax_semantic.selected_text_changed.v1\""))
    XCTAssertTrue(text.contains("\"durability\":\"best_effort\""))
    XCTAssertEqual(text.split(separator: "\n").count, 2)
    XCTAssertEqual(try mode(logURL), 0o600)
  }

  func testAXSemanticEventsSetCanonicalEventTimeAndSharedBatchMetadata() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let events = [
      CaptureAXSemanticEvent(
        kind: .focusedWindowChanged,
        generatedAt: "2026-05-24T10:12:12.123Z",
        matchedWindowID: 7
      ),
      CaptureAXSemanticEvent(
        kind: .selectedTextChanged,
        generatedAt: "2026-05-24T10:12:13.123Z",
        selection: CaptureAXSelectionContext(selectedTextRedacted: true)
      )
    ]

    let logURL = try XCTUnwrap(try store.appendAXSemanticEvents(events).last)
    let envelopes = try allEnvelopes(from: logURL, payloadType: CaptureAXSemanticEvent.self)

    XCTAssertEqual(envelopes.count, 2)
    XCTAssertEqual(envelopes.map(\.eventTimeStart), [
      "2026-05-24T10:12:12.123Z",
      "2026-05-24T10:12:13.123Z"
    ])
    XCTAssertEqual(envelopes.map(\.eventTimeEnd), [
      "2026-05-24T10:12:12.123Z",
      "2026-05-24T10:12:13.123Z"
    ])
    XCTAssertEqual(Set(envelopes.compactMap(\.captureBundleID)).count, 1)
    XCTAssertEqual(envelopes.map(\.laneID), ["capture.ax_semantic", "capture.ax_semantic"])
    XCTAssertEqual(envelopes.map(\.streamID), ["ax_semantic.focused_window_changed", "ax_semantic.selected_text_changed"])
    XCTAssertEqual(envelopes.map(\.privacyClass), [.accessibilitySemantic, .accessibilitySemantic])
    XCTAssertEqual(envelopes.map(\.privacyShape), [.axSemanticEvent, .axSemanticEvent])
    XCTAssertEqual(envelopes.map(\.sourceClock), [.accessibilityAPI, .accessibilityAPI])
  }

  func testRecentUXEventAnchorsCanFilterForDashboardLanes() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))

    try store.appendUXEventAnchors([
      uxAnchor(kind: .scrollBurst, endedAt: "2026-05-24T10:12:12.123Z"),
      uxAnchor(kind: .pointer, endedAt: "2026-05-24T10:12:13.123Z"),
      uxAnchor(kind: .keyboardActivity, endedAt: "2026-05-24T10:12:14.123Z")
    ])

    let recent = try store.latestUXEventAnchors(kinds: [.scrollBurst, .keyboardActivity], limit: 2)

    XCTAssertEqual(recent.map(\.envelope.eventType), [
      "capture.ux.keyboard_activity.v1",
      "capture.ux.scroll_burst.v1"
    ])
    XCTAssertEqual(recent.map(\.payload.kind), [.keyboardActivity, .scrollBurst])
  }

  func testLatestUXEventAnchorsReadFromTailOfLargeDailyLog() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    try store.prepare()
    let paths = OneContextCapturePaths(runtimePaths: runtimePaths(root: root))
    let logURL = paths.eventsDirectory.appendingPathComponent("2026-05-24.events.jsonl")
    let padding = String(repeating: "{\"eventType\":\"noise\"}\n", count: 5_000)
    try RuntimePermissions.writePrivateData(Data(padding.utf8), to: logURL)

    try store.appendUXEventAnchors([
      uxAnchor(kind: .scrollBurst, endedAt: "2026-05-24T10:12:12.123Z"),
      uxAnchor(kind: .keyboardActivity, endedAt: "2026-05-24T10:12:13.123Z")
    ])

    let recent = try store.latestUXEventAnchors(limit: 2)

    XCTAssertEqual(recent.map(\.payload.kind), [.keyboardActivity, .scrollBurst])
  }

  func testLatestUXEventAnchorsScanPastNonMatchingTailEvents() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))

    try store.appendUXEventAnchors([
      uxAnchor(kind: .scrollBurst, endedAt: "2026-05-24T10:12:12.123Z"),
      uxAnchor(kind: .keyboardActivity, endedAt: "2026-05-24T10:12:13.123Z")
    ])

    for index in 0..<500 {
      _ = try store.appendActiveWindowFrameMetadata(
        activeWindowMetadata(
          capturedAt: String(format: "2026-05-24T11:%02d:%02d.123Z", (index / 60) % 60, index % 60),
          sequence: index,
          dirtyRectCount: index % 3
        )
      )
    }

    let recent = try store.latestUXEventAnchors(limit: 2)

    XCTAssertEqual(recent.map(\.payload.kind), [.keyboardActivity, .scrollBurst])
  }

  func testPersistedUXKeyboardAnchorOmitsRawTextKeyCodesAndCoordinates() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))

    let logURL = try store.appendUXEventAnchor(
      uxAnchor(kind: .keyboardActivity, endedAt: "2026-05-24T10:12:14.123Z")
    )
    let text = try String(contentsOf: logURL, encoding: .utf8)

    XCTAssertFalse(text.contains("keyCode"))
    XCTAssertFalse(text.contains("key_code"))
    XCTAssertFalse(text.contains("characters"))
    XCTAssertFalse(text.contains("rawText"))
    XCTAssertFalse(text.contains("raw_text"))
    XCTAssertFalse(text.contains("locationX"))
    XCTAssertFalse(text.contains("locationY"))
    XCTAssertFalse(text.contains("\"x\""))
    XCTAssertFalse(text.contains("\"y\""))
  }

  func testCoalescedUXAnchorsPersistOneBoundedAggregateEvent() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))
    let base = Date(timeIntervalSince1970: 1_779_552_500)
    let aggregator = UXEventAggregator(scrollBurstGap: 0.12)
    let anchors = aggregator.ingest(
      [
        UXEventPrimitive(time: base, kind: .scrollWheel, scrollDeltaY: -1),
        UXEventPrimitive(time: base.addingTimeInterval(0.01), kind: .scrollWheel, scrollDeltaY: -2),
        UXEventPrimitive(time: base.addingTimeInterval(0.02), kind: .scrollWheel, scrollDeltaY: -3)
      ],
      now: base.addingTimeInterval(0.2)
    )

    try store.appendUXEventAnchors(anchors)

    let recent = try store.latestUXEventAnchors(limit: 10)
    XCTAssertEqual(recent.count, 1)
    XCTAssertEqual(recent.first?.payload.scroll?.eventCount, 3)
    XCTAssertEqual(recent.first?.payload.scroll?.totalDY, -6)
    XCTAssertEqual(aggregator.snapshot().coalescedCount, 2)
  }

  func testDashboardRendersFocusedAndEligibleWindows() throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let runtime = runtimePaths(root: root)
    let store = OneContextCaptureLogStore(runtimePaths: runtime)
    try store.appendWindowSnapshot(snapshot(generatedAt: "2026-05-24T10:12:12.123Z", windowID: 8, title: "new"))

    let rendered = try OneContextCaptureDashboard(runtimePaths: runtime)
      .render(now: Date(timeIntervalSince1970: 0))

    XCTAssertTrue(rendered.contains("1Context Capture Dashboard"))
    XCTAssertTrue(rendered.contains("Focused"))
    XCTAssertTrue(rendered.contains("Top Capture-Eligible Windows"))
    XCTAssertTrue(rendered.contains("Terminal - new"))
  }

  func testRecorderAppendsAXFocusedContextAsLosslessEvent() async throws {
    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let runtime = runtimePaths(root: root)
    let recorder = OneContextCaptureRecorder(runtimePaths: runtime) {
      self.snapshot(
        generatedAt: "2026-05-24T10:12:12.123Z",
        windowID: 8,
        title: "new",
        focusedContext: CaptureAXFocusedContext(
          generatedAt: "2026-05-24T10:12:12.123Z",
          status: .available,
          isProcessTrusted: true,
          activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.Terminal", appName: "Terminal"),
          focusedApplicationProcessID: 42,
          focusedWindow: CaptureAXNodeContext(role: "AXWindow", title: "new"),
          matchedWindowID: 8
        )
      )
    }

    _ = try await recorder.recordWindowSnapshot()

    let eventsURL = OneContextCapturePaths(runtimePaths: runtime)
      .eventsDirectory
      .appendingPathComponent("2026-05-24.events.jsonl")
    let text = try String(contentsOf: eventsURL, encoding: .utf8)
    XCTAssertTrue(text.contains("\"eventType\":\"capture.ax_focused_context\""))
    XCTAssertTrue(text.contains("\"durability\":\"lossless\""))
    XCTAssertTrue(text.contains("\"matchedWindowID\":8"))
  }

  func testCaptureStoreAlgorithmBenchmarkLargeJSONLAppendAndLatestRead() throws {
    try XCTSkipUnless(
      ProcessInfo.processInfo.environment["ONECONTEXT_CAPTURE_STORE_BENCHMARK"] == "1",
      "Set ONECONTEXT_CAPTURE_STORE_BENCHMARK=1 to run the CaptureStore algorithm benchmark."
    )

    let root = temporaryRoot()
    defer { try? FileManager.default.removeItem(at: root) }
    let store = OneContextCaptureLogStore(runtimePaths: runtimePaths(root: root))

    let appendCount = 5_000
    let appendElapsed = try elapsedMilliseconds {
      for index in 0..<appendCount {
        _ = try store.appendActiveWindowFrameMetadata(
          activeWindowMetadata(
            capturedAt: String(format: "2026-05-24T10:%02d:%02d.123Z", (index / 60) % 60, index % 60),
            sequence: index,
            dirtyRectCount: index % 4
          )
        )
      }
    }

    let anchors = (0..<250).map { index in
      uxAnchor(
        kind: index.isMultiple(of: 2) ? .scrollBurst : .keyboardActivity,
        endedAt: String(format: "2026-05-24T12:%02d:%02d.123Z", (index / 60) % 60, index % 60)
      )
    }
    _ = try store.appendUXEventAnchors(anchors)

    let readIterations = 200
    var latestKinds: [UXEventAnchorKind] = []
    let readElapsed = try elapsedMilliseconds {
      for _ in 0..<readIterations {
        latestKinds = try store.latestUXEventAnchors(kinds: [.scrollBurst], limit: 50).map(\.payload.kind)
      }
    }

    XCTAssertEqual(latestKinds.count, 50)
    XCTAssertTrue(latestKinds.allSatisfy { $0 == .scrollBurst })
    print(
      String(
        format: "CaptureStoreBenchmark append %d metadata events: %.2f ms; latest filtered reads x%d: %.2f ms",
        appendCount,
        appendElapsed,
        readIterations,
        readElapsed
      )
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
      .appendingPathComponent("1ctx-capture-tests-\(UUID().uuidString)", isDirectory: true)
  }

  private func snapshot(
    generatedAt: String,
    windowID: UInt32,
    title: String,
    focusedContext: CaptureAXFocusedContext? = nil
  ) -> CaptureSnapshot {
    CaptureSnapshot(
      generatedAt: generatedAt,
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.Terminal", appName: "Terminal"),
      displays: [
        CaptureDisplayState(
          displayID: "1",
          framePoints: CaptureRect(x: 0, y: 0, width: 1440, height: 900),
          scaleFactor: 2,
          isMain: true
        )
      ],
      windows: [
        CaptureWindowState(
          time: generatedAt,
          windowID: windowID,
          appPID: 42,
          appName: "Terminal",
          bundleID: "com.example.Terminal",
          title: title,
          framePoints: CaptureRect(x: 10, y: 20, width: 800, height: 600),
          zRank: 0,
          layer: 0,
          isFocused: true,
          isOnScreen: true,
          isMinimized: false,
          captureEligible: true,
          source: "test"
        )
      ],
      focusedContext: focusedContext
    )
  }

  private func activeWindowMetadata(
    capturedAt: String,
    sequence: Int,
    dirtyRectCount: Int
  ) -> ActiveWindowFrameMetadata {
    ActiveWindowFrameMetadata(
      streamID: "stream-1",
      sequence: sequence,
      capturedAt: capturedAt,
      target: ActiveWindowMetadataTarget(window: snapshot(generatedAt: capturedAt, windowID: 7, title: "new").windows[0]),
      frameStatus: .complete,
      frameStatusRawValue: 0,
      attachmentsPresent: true,
      displayTime: 123,
      contentRect: CaptureRect(x: 0, y: 0, width: 800, height: 600),
      contentScale: 1,
      scaleFactor: 1,
      dirtyRectSummary: CaptureDirtyRectSummary(
        dirtyRectCount: dirtyRectCount,
        dirtyAreaRatio: 0.1,
        changedTileRatio: 0.2,
        unionRect: CaptureRect(x: 0, y: 0, width: 80, height: 60),
        cappedRects: [CaptureRect(x: 0, y: 0, width: 80, height: 60)],
        cappedRectLimit: 32,
        weightedCenterY: 30,
        estimatedDY: 0
      ),
      motionFeatures: MotionFeatures(
        dirtyAreaRatio: 0.1,
        dirtyRectCount: dirtyRectCount,
        meanPixelDiff: 0,
        changedTileRatio: 0.2,
        estimatedDY: 0,
        scrollEventRecently: false,
        keyboardEventRecently: false,
        ocrNewLineRate: 0,
        focused: true
      ),
      feedsMotionClassifier: true,
      parseWarnings: []
    )
  }

  private func uxAnchor(kind: UXEventAnchorKind, endedAt: String) -> UXEventAnchor {
    switch kind {
    case .scrollBurst:
      return UXEventAnchor(
        kind: .scrollBurst,
        startedAt: "2026-05-24T10:12:12.000Z",
        endedAt: endedAt,
        scroll: UXScrollBurstSummary(
          eventCount: 3,
          totalDX: 0,
          totalDY: -42,
          maxAbsDY: 20,
          momentumEventCount: 1,
          durationMilliseconds: 300
        )
      )
    case .pointer:
      return UXEventAnchor(
        kind: .pointer,
        startedAt: "2026-05-24T10:12:12.000Z",
        endedAt: endedAt,
        pointer: UXPointerSummary(
          action: .click,
          button: .left,
          eventCount: 2,
          durationMilliseconds: 80,
          distancePoints: 1,
          dominantAxis: .none,
          clickCount: 1
        )
      )
    case .keyboardActivity:
      return UXEventAnchor(
        kind: .keyboardActivity,
        startedAt: "2026-05-24T10:12:12.000Z",
        endedAt: endedAt,
        keyboardActivity: UXKeyboardActivitySummary(
          eventCount: 4,
          keyDownCount: 2,
          keyUpCount: 2,
          autoRepeatCount: 0,
          modifiedKeyEventCount: 1,
          durationMilliseconds: 180
        )
      )
    case .modifiers:
      return UXEventAnchor(
        kind: .modifiers,
        startedAt: "2026-05-24T10:12:12.000Z",
        endedAt: endedAt,
        modifiers: UXModifierSummary(
          activeModifiers: ["command"],
          changedModifiers: ["command"]
        )
      )
    case .shortcut:
      return UXEventAnchor(
        kind: .shortcut,
        startedAt: "2026-05-24T10:12:12.000Z",
        endedAt: endedAt,
        recentTargetProcessID: 42,
        shortcut: UXShortcutSummary(
          eventCount: 1,
          keyDownCount: 1,
          autoRepeatCount: 0,
          modifierCombinations: [
            UXShortcutModifierCombinationSummary(modifiers: ["command"], eventCount: 1)
          ],
          actionCategories: [
            UXShortcutActionCategorySummary(category: .editing, eventCount: 1)
          ],
          durationMilliseconds: 0
        )
      )
    case .focusTransition:
      return UXEventAnchor(
        kind: .focusTransition,
        startedAt: "2026-05-24T10:12:12.000Z",
        endedAt: endedAt,
        recentTargetProcessID: 42,
        focusTransition: UXFocusTransitionSummary(
          previousProcessID: nil,
          currentProcessID: 42,
          trigger: .keyboard,
          currentTarget: UXFocusTargetHint(processID: 42, source: "cg_event_target_pid")
        )
      )
    }
  }

  private func firstEnvelope<Payload: Codable & Sendable>(
    from url: URL,
    payloadType: Payload.Type
  ) throws -> CaptureEventEnvelope<Payload> {
    try XCTUnwrap(allEnvelopes(from: url, payloadType: payloadType).first)
  }

  private func allEnvelopes<Payload: Codable & Sendable>(
    from url: URL,
    payloadType: Payload.Type
  ) throws -> [CaptureEventEnvelope<Payload>] {
    let text = try String(contentsOf: url, encoding: .utf8)
    return try text
      .split(separator: "\n", omittingEmptySubsequences: true)
      .map { line in
        try JSONDecoder().decode(CaptureEventEnvelope<Payload>.self, from: Data(line.utf8))
      }
  }

  private func mode(_ url: URL) throws -> Int {
    let attrs = try FileManager.default.attributesOfItem(atPath: url.path)
    return (attrs[.posixPermissions] as? NSNumber)?.intValue ?? -1
  }

  private func elapsedMilliseconds(_ body: () throws -> Void) rethrows -> Double {
    let start = DispatchTime.now().uptimeNanoseconds
    try body()
    let end = DispatchTime.now().uptimeNanoseconds
    return Double(end - start) / 1_000_000
  }
}
