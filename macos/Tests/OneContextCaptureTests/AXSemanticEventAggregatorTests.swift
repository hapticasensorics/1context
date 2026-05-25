import XCTest
@testable import OneContextCapture

final class AXSemanticEventAggregatorTests: XCTestCase {
  func testInitialFocusedContextEmitsSemanticEventTypes() throws {
    let aggregator = AXSemanticEventAggregator()

    let events = aggregator.ingest(
      focusedContext(
        generatedAt: "2026-05-24T10:11:12.123Z",
        windowTitle: "Project",
        elementTitle: "Search",
        valueCount: 12,
        selection: CaptureAXSelectionContext(
          range: CaptureAXTextRange(location: 0, length: 5),
          isInsertionPoint: false,
          selectedText: "hello",
          selectedTextCharacterCount: 5
        ),
        transientUI: CaptureAXTransientUIState(
          kind: .menu,
          source: "focused_element",
          role: "AXMenu",
          title: "File"
        )
      )
    )

    XCTAssertEqual(
      events.map(\.kind),
      [
        .focusedWindowChanged,
        .focusedElementChanged,
        .valueChanged,
        .selectedTextChanged,
        .transientUIStateChanged
      ]
    )
    XCTAssertEqual(events.map(\.eventType), [
      "capture.ax_semantic.focused_window_changed.v1",
      "capture.ax_semantic.focused_element_changed.v1",
      "capture.ax_semantic.value_changed.v1",
      "capture.ax_semantic.selected_text_changed.v1",
      "capture.ax_semantic.transient_ui_state_changed.v1"
    ])

    let object = try JSONSerialization.jsonObject(with: JSONEncoder().encode(events[3])) as? [String: Any]
    XCTAssertEqual(object?["eventType"] as? String, "capture.ax_semantic.selected_text_changed.v1")
    XCTAssertEqual(object?["kind"] as? String, "selected_text_changed")
  }

  func testAggregatorSuppressesUnchangedSnapshotsAndEmitsChangedValueAndSelection() {
    let aggregator = AXSemanticEventAggregator()
    let first = focusedContext(
      generatedAt: "2026-05-24T10:11:12.123Z",
      valueCount: 12,
      selection: CaptureAXSelectionContext(range: CaptureAXTextRange(location: 2, length: 0), isInsertionPoint: true)
    )

    XCTAssertFalse(aggregator.ingest(first).isEmpty)
    XCTAssertTrue(aggregator.ingest(first).isEmpty)

    let changed = focusedContext(
      generatedAt: "2026-05-24T10:11:13.123Z",
      valueCount: 13,
      selection: CaptureAXSelectionContext(
        range: CaptureAXTextRange(location: 2, length: 3),
        isInsertionPoint: false,
        selectedText: "abc",
        selectedTextCharacterCount: 3
      )
    )

    XCTAssertEqual(aggregator.ingest(changed).map(\.kind), [.valueChanged, .selectedTextChanged])
  }

  func testSemanticSelectionEventPreservesRedactionAndDoesNotLeakRawSecret() throws {
    let aggregator = AXSemanticEventAggregator()
    let events = aggregator.ingest(
      focusedContext(
        generatedAt: "2026-05-24T10:11:12.123Z",
        elementTitle: nil,
        valueShape: CaptureAXValueShape(kind: "redacted", redacted: true),
        selection: CaptureAXSelectionContext(selectedTextRedacted: true),
        isSensitive: true,
        redactionReasons: ["secure_text_field", "sensitive_metadata"]
      )
    )

    let selected = try XCTUnwrap(events.first { $0.kind == .selectedTextChanged })
    XCTAssertEqual(selected.selection?.selectedTextRedacted, true)
    XCTAssertNil(selected.selection?.selectedText)
    XCTAssertEqual(selected.focusedElement?.redactionReasons, ["secure_text_field", "sensitive_metadata"])

    let json = String(decoding: try JSONEncoder().encode(events), as: UTF8.self)
    XCTAssertTrue(json.contains("\"selectedTextRedacted\":true"))
    XCTAssertTrue(json.contains("\"redacted\":true"))
    XCTAssertFalse(json.contains("hunter2"))
    XCTAssertFalse(json.contains("raw value"))
  }

  func testBoundedBufferDropsOldestSemanticEvents() {
    let aggregator = AXSemanticEventAggregator(capacity: 3)

    _ = aggregator.ingest(focusedContext(generatedAt: "2026-05-24T10:11:12.123Z", windowTitle: "A", valueCount: 1))
    _ = aggregator.ingest(
      focusedContext(
        generatedAt: "2026-05-24T10:11:13.123Z",
        windowTitle: "B",
        valueCount: 2,
        selection: CaptureAXSelectionContext(
          range: CaptureAXTextRange(location: 0, length: 1),
          isInsertionPoint: false,
          selectedText: "b",
          selectedTextCharacterCount: 1
        )
      )
    )

    XCTAssertEqual(aggregator.recentEvents().count, 3)
    XCTAssertEqual(aggregator.snapshot().droppedCount, 4)
    XCTAssertEqual(aggregator.recentEvents().map(\.generatedAt), [
      "2026-05-24T10:11:13.123Z",
      "2026-05-24T10:11:13.123Z",
      "2026-05-24T10:11:13.123Z"
    ])
  }

  func testTransientUICloseEventUsesPreviousState() {
    let aggregator = AXSemanticEventAggregator()
    _ = aggregator.ingest(
      focusedContext(
        generatedAt: "2026-05-24T10:11:12.123Z",
        transientUI: CaptureAXTransientUIState(kind: .sheet, source: "focused_window", role: "AXSheet", title: "Save")
      )
    )

    let closedEvents = aggregator.ingest(focusedContext(generatedAt: "2026-05-24T10:11:13.123Z"))
    let closed = closedEvents.first { $0.kind == .transientUIStateChanged }
    XCTAssertEqual(closed?.transientUIState?.kind, .sheet)
    XCTAssertEqual(closed?.transientUIState?.visible, false)
  }

  private func focusedContext(
    generatedAt: String,
    windowTitle: String = "Project",
    elementTitle: String? = "Search",
    valueCount: Int = 12,
    valueShape: CaptureAXValueShape? = nil,
    selection: CaptureAXSelectionContext? = CaptureAXSelectionContext(
      range: CaptureAXTextRange(location: 2, length: 0),
      isInsertionPoint: true
    ),
    transientUI: CaptureAXTransientUIState? = nil,
    isSensitive: Bool = false,
    redactionReasons: [String] = []
  ) -> CaptureAXFocusedContext {
    CaptureAXFocusedContext(
      generatedAt: generatedAt,
      status: .available,
      isProcessTrusted: true,
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example"),
      focusedApplicationProcessID: 42,
      focusedWindow: CaptureAXNodeContext(
        role: transientUI?.source == "focused_window" ? transientUI?.role : "AXWindow",
        title: windowTitle,
        frame: CaptureRect(x: 0, y: 0, width: 900, height: 700),
        transientUI: transientUI?.source == "focused_window" ? transientUI : nil
      ),
      focusedElement: CaptureAXNodeContext(
        role: transientUI?.source == "focused_element" ? transientUI?.role : "AXTextField",
        title: isSensitive ? nil : elementTitle,
        frame: CaptureRect(x: 10, y: 20, width: 300, height: 30),
        valueShape: valueShape ?? CaptureAXValueShape(
          kind: "text",
          characterCount: valueCount,
          sourceAttribute: "AXNumberOfCharacters",
          redacted: isSensitive
        ),
        selection: selection,
        transientUI: transientUI?.source == "focused_element" ? transientUI : nil,
        isSensitive: isSensitive,
        redactionReasons: redactionReasons
      ),
      matchedWindowID: 7
    )
  }
}
