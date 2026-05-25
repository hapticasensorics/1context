import CoreGraphics
import XCTest
@testable import OneContextCapture

final class UXEventAggregatorTests: XCTestCase {
  func testPersistentTapStatusSerializesLifecycleAndOwnerFields() throws {
    let status = OneContextUXEventTapStatus.inactive(
      startupWired: true,
      owner: OneContextUXEventTapOwner(
        pid: 42,
        executable: "/Applications/1Context Dev.app/Contents/MacOS/1contextd",
        bundle: "com.haptica.1context.dev"
      ),
      lifecycleState: "degraded",
      lastError: "Input Monitoring missing",
      note: "daemon continued"
    )

    let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(status)) as? [String: Any]
    XCTAssertEqual(object?["startup_wired"] as? Bool, true)
    XCTAssertEqual(object?["lifecycle_state"] as? String, "degraded")
    XCTAssertEqual(object?["tap_active"] as? Bool, false)
    XCTAssertEqual(object?["tap_owner_pid"] as? Int, 42)
    XCTAssertEqual(object?["tap_owner_executable"] as? String, "/Applications/1Context Dev.app/Contents/MacOS/1contextd")
    XCTAssertEqual(object?["tap_owner_bundle"] as? String, "com.haptica.1context.dev")
    XCTAssertEqual(object?["last_error"] as? String, "Input Monitoring missing")
    XCTAssertEqual(object?["disabled_count"] as? Int, 0)
    XCTAssertEqual(object?["reenable_attempt_count"] as? Int, 0)
    XCTAssertEqual(object?["callback_average_us"] as? Double, 0)
    XCTAssertEqual(object?["target_pid_available"] as? Bool, false)
    XCTAssertEqual(object?["target_pid_observed_count"] as? Int, 0)
    XCTAssertNil(object?["recent_target_process_id"])
  }

  func testTapStatusSerializesBoundedTargetPIDAvailability() throws {
    let status = OneContextUXEventTapStatus(
      startupWired: true,
      lifecycleState: "running",
      tapActive: true,
      lastEventAt: "2026-05-24T10:11:12.123Z",
      disabledCount: 0,
      droppedCount: 0,
      coalescedCount: 0,
      queueDepth: 0,
      observedEventCount: 3,
      callbackCount: 3,
      callbackAverageMicroseconds: 4.5,
      callbackMaxMicroseconds: 7.25,
      targetProcessIDAvailable: true,
      targetProcessIDObservedCount: 2,
      recentTargetProcessID: 42
    )

    let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(status)) as? [String: Any]
    XCTAssertEqual(object?["target_pid_available"] as? Bool, true)
    XCTAssertEqual(object?["target_pid_observed_count"] as? Int, 2)
    XCTAssertEqual(object?["recent_target_process_id"] as? Int, 42)
    XCTAssertNil(object?["recentTargetProcessID"])
  }

  func testSerializedKeyboardActivityCannotContainRawTextOrKeyCodes() throws {
    let base = Date(timeIntervalSince1970: 1_779_552_000)
    let aggregator = UXEventAggregator(keyboardBurstGap: 0.05)
    let anchors = aggregator.ingest(
      [
        UXEventPrimitive(
          time: base,
          kind: .keyDown,
          modifierFlagsRaw: CGEventFlags.maskCommand.rawValue,
          targetProcessID: 42
        ),
        UXEventPrimitive(
          time: base.addingTimeInterval(0.01),
          kind: .keyUp,
          modifierFlagsRaw: CGEventFlags.maskCommand.rawValue,
          targetProcessID: 42
        )
      ],
      now: base.addingTimeInterval(0.2)
    )

    let keyboard = try XCTUnwrap(anchors.first { $0.kind == .keyboardActivity })
    XCTAssertEqual(keyboard.keyboardActivity?.eventCount, 2)
    XCTAssertEqual(keyboard.keyboardActivity?.modifiedKeyEventCount, 2)
    XCTAssertEqual(keyboard.recentTargetProcessID, 42)

    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    let json = String(decoding: try encoder.encode(anchors), as: UTF8.self)

    XCTAssertTrue(json.contains("\"recent_target_process_id\":42"))
    XCTAssertFalse(json.contains("recentTargetProcessID"))
    XCTAssertFalse(json.contains("keyCode"))
    XCTAssertFalse(json.contains("key_code"))
    XCTAssertFalse(json.contains("characters"))
    XCTAssertFalse(json.contains("\"text\""))
    XCTAssertFalse(json.contains("hello"))
    XCTAssertFalse(json.contains("locationX"))
    XCTAssertFalse(json.contains("locationY"))
  }

  func testAnchorKindsSelectDashboardEventTypes() {
    XCTAssertEqual(UXEventAnchorKind.scrollBurst.captureEventType, "capture.ux.scroll_burst.v1")
    XCTAssertEqual(UXEventAnchorKind.pointer.captureEventType, "capture.ux.pointer.v1")
    XCTAssertEqual(UXEventAnchorKind.keyboardActivity.captureEventType, "capture.ux.keyboard_activity.v1")
    XCTAssertEqual(UXEventAnchorKind.modifiers.captureEventType, "capture.ux.modifiers.v1")
  }

  func testScrollEventsAggregateIntoOneBurst() throws {
    let base = Date(timeIntervalSince1970: 1_779_552_100)
    let aggregator = UXEventAggregator(scrollBurstGap: 0.12)

    let anchors = aggregator.ingest(
      [
        scroll(time: base, dy: -4),
        scroll(time: base.addingTimeInterval(0.03), dy: -7),
        scroll(time: base.addingTimeInterval(0.08), dy: -2, momentum: true)
      ],
      now: base.addingTimeInterval(0.3)
    )

    let burst = try XCTUnwrap(anchors.first { $0.kind == .scrollBurst }?.scroll)
    XCTAssertEqual(burst.eventCount, 3)
    XCTAssertEqual(burst.totalDY, -13)
    XCTAssertEqual(burst.maxAbsDY, 7)
    XCTAssertEqual(burst.momentumEventCount, 1)
    XCTAssertEqual(aggregator.snapshot().coalescedCount, 2)
  }

  func testOutOfOrderBatchStillAggregatesChronologically() throws {
    let base = Date(timeIntervalSince1970: 1_779_552_125)
    let aggregator = UXEventAggregator(scrollBurstGap: 0.05)

    let anchors = aggregator.ingest(
      [
        scroll(time: base.addingTimeInterval(0.04), dy: -4),
        scroll(time: base, dy: -3)
      ],
      now: base.addingTimeInterval(0.2)
    )

    let anchor = try XCTUnwrap(anchors.first { $0.kind == .scrollBurst })
    XCTAssertEqual(anchor.startedAt, UXEventTime.isoString(base))
    XCTAssertEqual(anchor.endedAt, UXEventTime.isoString(base.addingTimeInterval(0.04)))
    XCTAssertEqual(anchor.scroll?.eventCount, 2)
    XCTAssertEqual(anchor.scroll?.totalDY, -7)
  }

  func testFlushEmitsOpenBurstsWithoutWaitingForGap() throws {
    let base = Date(timeIntervalSince1970: 1_779_552_150)
    let aggregator = UXEventAggregator(scrollBurstGap: 30, keyboardBurstGap: 30)

    let immediate = aggregator.ingest(
      [
        scroll(time: base, dy: -2),
        scroll(time: base.addingTimeInterval(0.01), dy: -3),
        UXEventPrimitive(time: base.addingTimeInterval(0.02), kind: .keyDown)
      ],
      now: base.addingTimeInterval(0.03)
    )
    XCTAssertTrue(immediate.isEmpty)

    let flushed = aggregator.flush(now: base.addingTimeInterval(0.04))
    XCTAssertEqual(flushed.map(\.captureEventType), [
      "capture.ux.scroll_burst.v1",
      "capture.ux.keyboard_activity.v1"
    ])
    XCTAssertEqual(flushed.first?.scroll?.eventCount, 2)
    XCTAssertEqual(flushed.last?.keyboardActivity?.eventCount, 1)
  }

  func testBoundedQueueDropsOldestEventsAndReportsDepth() {
    let queue = UXEventPrimitiveRingBuffer(capacity: 2)
    let base = Date(timeIntervalSince1970: 1_779_552_200)

    for index in 0..<5 {
      queue.push(scroll(time: base.addingTimeInterval(Double(index)), dy: Double(index)))
    }

    let snapshot = queue.snapshot()
    XCTAssertEqual(snapshot.capacity, 2)
    XCTAssertEqual(snapshot.queueDepth, 2)
    XCTAssertEqual(snapshot.droppedCount, 3)
    XCTAssertEqual(snapshot.enqueuedCount, 5)
    XCTAssertEqual(queue.drain().map(\.scrollDeltaY), [3, 4])
  }

  func testMotionHintsExposeRecentSparseSignals() throws {
    let base = Date(timeIntervalSince1970: 1_779_552_300)
    let aggregator = UXEventAggregator(recentWindow: 1)

    _ = aggregator.ingest(
      [
        scroll(time: base, dy: -9),
        UXEventPrimitive(time: base.addingTimeInterval(0.2), kind: .keyDown, targetProcessID: 42)
      ],
      now: base.addingTimeInterval(0.4)
    )

    let recent = aggregator.motionHints(now: base.addingTimeInterval(0.5))
    XCTAssertTrue(recent.scrollEventRecently)
    XCTAssertTrue(recent.keyboardActivityRecently)
    XCTAssertTrue(recent.focusedRecently)
    XCTAssertEqual(recent.recentTargetProcessID, 42)
    XCTAssertEqual(recent.estimatedScrollDY, -9)
    let recentJSON = String(decoding: try JSONEncoder().encode(recent), as: UTF8.self)
    XCTAssertTrue(recentJSON.contains("\"recent_target_process_id\":42"))
    XCTAssertFalse(recentJSON.contains("recentTargetProcessID"))

    let expired = aggregator.motionHints(now: base.addingTimeInterval(2))
    XCTAssertFalse(expired.scrollEventRecently)
    XCTAssertFalse(expired.keyboardActivityRecently)
    XCTAssertFalse(expired.focusedRecently)
    XCTAssertNil(expired.recentTargetProcessID)
    let expiredJSON = String(decoding: try JSONEncoder().encode(expired), as: UTF8.self)
    XCTAssertFalse(expiredJSON.contains("recent_target_process_id"))
    XCTAssertFalse(expiredJSON.contains("recentTargetProcessID"))
  }

  func testAnchorsDoNotBorrowUnrelatedRecentTargetPID() throws {
    let base = Date(timeIntervalSince1970: 1_779_552_350)
    let aggregator = UXEventAggregator(scrollBurstGap: 0.05, keyboardBurstGap: 0.05, recentWindow: 1)

    _ = aggregator.ingest(
      [UXEventPrimitive(time: base, kind: .keyDown, targetProcessID: 42)],
      now: base.addingTimeInterval(0.01)
    )
    let anchors = aggregator.ingest(
      [scroll(time: base.addingTimeInterval(0.1), dy: -4)],
      now: base.addingTimeInterval(0.2)
    )

    let keyboard = try XCTUnwrap(anchors.first { $0.kind == .keyboardActivity })
    let scroll = try XCTUnwrap(anchors.first { $0.kind == .scrollBurst })
    XCTAssertEqual(keyboard.recentTargetProcessID, 42)
    XCTAssertNil(scroll.recentTargetProcessID)
    XCTAssertEqual(aggregator.motionHints(now: base.addingTimeInterval(0.2)).recentTargetProcessID, 42)
  }

  func testPointerDragSummaryOmitsRawCoordinates() throws {
    let base = Date(timeIntervalSince1970: 1_779_552_400)
    let aggregator = UXEventAggregator()
    let anchors = aggregator.ingest(
      [
        UXEventPrimitive(
          time: base,
          kind: .leftMouseDown,
          locationX: 10,
          locationY: 20,
          clickState: 1,
          targetProcessID: 42
        ),
        UXEventPrimitive(
          time: base.addingTimeInterval(0.1),
          kind: .leftMouseDragged,
          locationX: 10,
          locationY: 80,
          targetProcessID: 42
        ),
        UXEventPrimitive(
          time: base.addingTimeInterval(0.2),
          kind: .leftMouseUp,
          locationX: 10,
          locationY: 95,
          targetProcessID: 42
        )
      ],
      now: base.addingTimeInterval(0.3)
    )

    let drag = try XCTUnwrap(anchors.last { $0.pointer?.action == .drag }?.pointer)
    XCTAssertEqual(drag.button, .left)
    XCTAssertEqual(drag.dominantAxis, .vertical)
    XCTAssertGreaterThan(drag.distancePoints, 70)
    XCTAssertEqual(anchors.last { $0.pointer?.action == .drag }?.recentTargetProcessID, 42)

    let json = String(decoding: try JSONEncoder().encode(anchors), as: UTF8.self)
    XCTAssertTrue(json.contains("\"recent_target_process_id\":42"))
    XCTAssertFalse(json.contains("recentTargetProcessID"))
    XCTAssertFalse(json.contains("locationX"))
    XCTAssertFalse(json.contains("locationY"))
    XCTAssertFalse(json.contains("\"x\""))
    XCTAssertFalse(json.contains("\"y\""))
  }

  func testPrimitiveCapturesOnlyTargetPIDMetadataFromCGEvent() throws {
    let base = Date(timeIntervalSince1970: 1_779_552_450)
    let event = try XCTUnwrap(CGEvent(keyboardEventSource: nil, virtualKey: 12, keyDown: true))
    event.setIntegerValueField(.eventTargetUnixProcessID, value: 42)

    let primitive = try XCTUnwrap(OneContextUXEventTap.primitive(from: .keyDown, event: event, at: base))
    XCTAssertEqual(primitive.kind, .keyDown)
    XCTAssertEqual(primitive.targetProcessID, 42)
    XCTAssertNil(primitive.locationX)
    XCTAssertNil(primitive.locationY)
  }

  private func scroll(time: Date, dy: Double, momentum: Bool = false) -> UXEventPrimitive {
    UXEventPrimitive(
      time: time,
      kind: .scrollWheel,
      scrollDeltaY: dy,
      isMomentumScroll: momentum
    )
  }
}
