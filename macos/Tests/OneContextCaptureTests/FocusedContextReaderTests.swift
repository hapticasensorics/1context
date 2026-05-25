import CoreGraphics
import XCTest
@testable import OneContextCapture

final class FocusedContextReaderTests: XCTestCase {
  func testNoTrustReturnsStructuredFallback() {
    let reader = AXFocusedContextReader(client: FakeAXClient(isTrusted: false))

    let context = reader.read(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )

    XCTAssertEqual(context.status, .notTrusted)
    XCTAssertFalse(context.isProcessTrusted)
    XCTAssertNil(context.focusedWindow)
    XCTAssertEqual(context.issues.map(\.code), ["accessibility_not_trusted"])
  }

  func testSecureFocusedElementRedactsTextAndMetadata() {
    let reader = AXFocusedContextReader(client: FakeAXClient.focusedTextField(
      fieldSubrole: "AXSecureTextField",
      title: "Password",
      identifier: "account-password",
      selectedText: "hunter2"
    ))

    let context = reader.read(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )

    XCTAssertEqual(context.status, .available)
    let focusedElement = context.focusedElement
    XCTAssertEqual(focusedElement?.role, "AXTextField")
    XCTAssertEqual(focusedElement?.subrole, "AXSecureTextField")
    XCTAssertNil(focusedElement?.title)
    XCTAssertNil(focusedElement?.identifier)
    XCTAssertNil(focusedElement?.elementDescription)
    XCTAssertEqual(focusedElement?.valueShape?.kind, "redacted")
    XCTAssertEqual(focusedElement?.valueShape?.redacted, true)
    XCTAssertEqual(focusedElement?.selection?.selectedTextRedacted, true)
    XCTAssertNil(focusedElement?.selection?.selectedText)
    XCTAssertEqual(focusedElement?.redactionReasons, ["secure_text_field", "sensitive_metadata"])
  }

  func testTextLikeControlsUseShapeAndCappedSelectedText() {
    let selectedText = String(repeating: "a", count: 300)
    let reader = AXFocusedContextReader(
      client: FakeAXClient.focusedTextField(
        fieldSubrole: "AXSearchField",
        title: "Search",
        identifier: "query",
        selectedText: selectedText
      ),
      maxSelectedTextCharacters: 32
    )

    let context = reader.read(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )

    XCTAssertEqual(context.focusedElement?.valueShape?.kind, "text")
    XCTAssertEqual(context.focusedElement?.valueShape?.characterCount, 300)
    XCTAssertEqual(context.focusedElement?.valueShape?.sourceAttribute, AXFocusedAttributes.numberOfCharacters)
    XCTAssertEqual(context.focusedElement?.selection?.selectedText?.count, 32)
    XCTAssertEqual(context.focusedElement?.selection?.selectedTextCharacterCount, 300)
    XCTAssertEqual(context.focusedElement?.selection?.selectedTextTruncated, true)
  }

  func testFocusedElementIncludesVisibleRangeCaretAndScrollbarShape() {
    let reader = AXFocusedContextReader(
      client: FakeAXClient.focusedTextField(
        fieldSubrole: "AXSearchField",
        title: "Search",
        identifier: "query",
        selectedText: ""
      )
    )

    let context = reader.read(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )

    XCTAssertEqual(context.focusedElement?.visibleRange, CaptureAXTextRange(location: 12, length: 80))
    XCTAssertEqual(context.focusedElement?.selection?.range, CaptureAXTextRange(location: 0, length: 0))
    XCTAssertEqual(context.focusedElement?.selection?.isInsertionPoint, true)
    XCTAssertEqual(context.focusedElement?.scroll?.vertical?.orientation, "vertical")
    XCTAssertEqual(context.focusedElement?.scroll?.vertical?.role, "AXScrollBar")
    XCTAssertEqual(context.focusedElement?.scroll?.vertical?.valuePercent, 0.25)
  }

  func testFocusedWindowReportsSheetLikeTransientUIState() {
    let reader = AXFocusedContextReader(client: FakeAXClient.focusedSheet())

    let context = reader.read(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )

    XCTAssertEqual(context.focusedWindow?.transientUI?.kind, .sheet)
    XCTAssertEqual(context.focusedWindow?.transientUI?.visible, true)
    XCTAssertEqual(context.focusedWindow?.transientUI?.source, "focused_window")
  }

  func testWrongAXTypesBecomeIssuesInsteadOfCrashes() {
    var client = FakeAXClient.focusedTextField(
      fieldSubrole: "AXSearchField",
      title: "Search",
      identifier: "query",
      selectedText: "abc"
    )
    client.attributes["field"]?[AXFocusedAttributes.role] = AXFocusedAttributeRead(status: .success, value: .int(99))
    client.attributes["field"]?[AXFocusedAttributes.position] = AXFocusedAttributeRead(status: .success, value: .string("not a point"))
    client.attributes["field"]?[AXFocusedAttributes.selectedTextRange] = AXFocusedAttributeRead(status: .success, value: .string("not a range"))

    let context = AXFocusedContextReader(client: client).read(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )

    let wrongTypeIssues = context.issues.filter { $0.code == "ax_attribute_wrong_type" }
    XCTAssertTrue(wrongTypeIssues.contains { $0.attribute == AXFocusedAttributes.role && $0.expectedType == "string" })
    XCTAssertTrue(wrongTypeIssues.contains { $0.attribute == AXFocusedAttributes.position && $0.expectedType == "point" })
    XCTAssertTrue(wrongTypeIssues.contains { $0.attribute == AXFocusedAttributes.selectedTextRange && $0.expectedType == "range" })
  }

  func testVisibleContextBoundsRedactsAndKeepsTextShapeOnly() throws {
    let reader = AXFocusedContextReader(
      client: FakeAXClient.focusedVisibleMap(),
      maxVisibleRegionCount: 3,
      maxVisibleChildrenPerNode: 2,
      maxVisibleDepth: 1
    )

    let first = reader.read(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )
    let second = reader.read(
      generatedAt: "2026-05-24T10:11:13.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )

    let visibleContext = try XCTUnwrap(first.visibleContext)
    XCTAssertEqual(visibleContext.regions.count, 3)
    XCTAssertEqual(visibleContext.capturedRegionCount, 3)
    XCTAssertEqual(visibleContext.maxRegionCount, 3)
    XCTAssertEqual(visibleContext.maxDepth, 1)
    XCTAssertEqual(visibleContext.truncated, true)
    XCTAssertEqual(visibleContext.focusedWindowRegionID, visibleContext.regions.first?.regionID)
    XCTAssertEqual(visibleContext.focusedWindowRegionID, second.visibleContext?.focusedWindowRegionID)

    let windowRegion = try XCTUnwrap(visibleContext.regions.first)
    XCTAssertEqual(windowRegion.childCount, 4)
    XCTAssertEqual(windowRegion.capturedChildCount, 2)
    XCTAssertEqual(windowRegion.childrenTruncated, true)

    let normalRow = visibleContext.regions[1]
    XCTAssertEqual(normalRow.role, "AXRow")
    XCTAssertEqual(normalRow.titleShape?.characterCount, "Inbox Thread".count)
    XCTAssertEqual(normalRow.titleShape?.redacted, false)
    XCTAssertEqual(normalRow.visibleRange, CaptureAXTextRange(location: 20, length: 50))
    XCTAssertEqual(normalRow.selectedTextRange, CaptureAXTextRange(location: 22, length: 0))
    XCTAssertEqual(normalRow.isInsertionPoint, true)

    let sensitiveRow = visibleContext.regions[2]
    XCTAssertEqual(sensitiveRow.role, "AXRow")
    XCTAssertEqual(sensitiveRow.titleShape?.redacted, true)
    XCTAssertNil(sensitiveRow.titleShape?.characterCount)
    XCTAssertEqual(sensitiveRow.valueShape?.kind, "redacted")
    XCTAssertEqual(sensitiveRow.redactionReasons, ["sensitive_metadata"])

    let json = String(data: try JSONEncoder().encode(visibleContext), encoding: .utf8) ?? ""
    XCTAssertFalse(json.contains("Password Reset Token 123"))
    XCTAssertFalse(json.contains("selected row text should not serialize"))
  }

  func testElementUnderPointerHintDoesNotSerializePointerCoordinates() throws {
    let reader = AXFocusedContextReader(client: FakeAXClient.focusedVisibleMap(
      pointerLocation: CGPoint(x: 12345.25, y: 67890.5),
      elementAtPointer: "pointer-button"
    ))

    let context = reader.read(
      generatedAt: "2026-05-24T10:11:12.123Z",
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example")
    )

    let hint = try XCTUnwrap(context.visibleContext?.elementUnderPointer)
    XCTAssertEqual(hint.role, "AXButton")
    XCTAssertNotNil(hint.regionID)
    XCTAssertEqual(hint.titleShape?.redacted, true)
    XCTAssertEqual(hint.redactionReasons, ["sensitive_metadata"])

    let json = String(data: try JSONEncoder().encode(context.visibleContext), encoding: .utf8) ?? ""
    XCTAssertFalse(json.contains("12345.25"))
    XCTAssertFalse(json.contains("67890.5"))
    XCTAssertFalse(json.contains("Hover Password Token"))
  }
}

private struct FakeAXClient: AXFocusedContextClient {
  var isTrusted = true
  var attributes: [String: [String: AXFocusedAttributeRead<String>]] = [:]
  var processIDs: [String: Int32] = [:]
  var pointerLocation: CGPoint?
  var elementAtPointer: String?

  func isProcessTrusted() -> Bool {
    isTrusted
  }

  func systemWideElement() -> String {
    "system"
  }

  func applicationElement(pid: Int32) -> String {
    "app-\(pid)"
  }

  func processIdentifier(for element: String) -> Int32? {
    processIDs[element]
  }

  func setMessagingTimeout(_ element: String, seconds: Float) -> CaptureAXReadStatus {
    .success
  }

  func copyAttribute(_ attribute: String, from element: String) -> AXFocusedAttributeRead<String> {
    attributes[element]?[attribute] ?? AXFocusedAttributeRead(status: .noValue)
  }

  func currentPointerLocation() -> CGPoint? {
    pointerLocation
  }

  func elementAtPosition(_ point: CGPoint, in element: String) -> AXFocusedAttributeRead<String> {
    guard let elementAtPointer else {
      return AXFocusedAttributeRead(status: .noValue)
    }
    return AXFocusedAttributeRead(status: .success, value: .element(elementAtPointer))
  }

  static func focusedTextField(
    fieldSubrole: String,
    title: String,
    identifier: String,
    selectedText: String
  ) -> FakeAXClient {
    FakeAXClient(
      attributes: [
        "system": [
          AXFocusedAttributes.focusedApplication: AXFocusedAttributeRead(status: .success, value: .element("app")),
          AXFocusedAttributes.focusedUIElement: AXFocusedAttributeRead(status: .success, value: .element("field"))
        ],
        "app": [
          AXFocusedAttributes.focusedWindow: AXFocusedAttributeRead(status: .success, value: .element("window"))
        ],
        "window": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXWindow")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Example Window")),
          AXFocusedAttributes.position: AXFocusedAttributeRead(status: .success, value: .point(CGPoint(x: 10, y: 20))),
          AXFocusedAttributes.size: AXFocusedAttributeRead(status: .success, value: .size(CGSize(width: 800, height: 600)))
        ],
        "field": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXTextField")),
          AXFocusedAttributes.subrole: AXFocusedAttributeRead(status: .success, value: .string(fieldSubrole)),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string(title)),
          AXFocusedAttributes.identifier: AXFocusedAttributeRead(status: .success, value: .string(identifier)),
          AXFocusedAttributes.description: AXFocusedAttributeRead(status: .success, value: .string(title)),
          AXFocusedAttributes.position: AXFocusedAttributeRead(status: .success, value: .point(CGPoint(x: 30, y: 40))),
          AXFocusedAttributes.size: AXFocusedAttributeRead(status: .success, value: .size(CGSize(width: 300, height: 28))),
          AXFocusedAttributes.numberOfCharacters: AXFocusedAttributeRead(status: .success, value: .int(selectedText.count)),
          AXFocusedAttributes.selectedText: AXFocusedAttributeRead(status: .success, value: .string(selectedText)),
          AXFocusedAttributes.selectedTextRange: AXFocusedAttributeRead(
            status: .success,
            value: .range(location: 0, length: selectedText.count)
          ),
          AXFocusedAttributes.visibleCharacterRange: AXFocusedAttributeRead(
            status: .success,
            value: .range(location: 12, length: 80)
          ),
          AXFocusedAttributes.verticalScrollBar: AXFocusedAttributeRead(status: .success, value: .element("field-vscroll")),
          AXFocusedAttributes.value: AXFocusedAttributeRead(status: .success, value: .string("raw value should not be used"))
        ],
        "field-vscroll": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXScrollBar")),
          AXFocusedAttributes.orientation: AXFocusedAttributeRead(status: .success, value: .string("AXVerticalOrientation")),
          AXFocusedAttributes.enabled: AXFocusedAttributeRead(status: .success, value: .bool(true)),
          AXFocusedAttributes.value: AXFocusedAttributeRead(status: .success, value: .double(25)),
          AXFocusedAttributes.minValue: AXFocusedAttributeRead(status: .success, value: .double(0)),
          AXFocusedAttributes.maxValue: AXFocusedAttributeRead(status: .success, value: .double(100))
        ]
      ],
      processIDs: [
        "app": 42
      ]
    )
  }

  static func focusedSheet() -> FakeAXClient {
    FakeAXClient(
      attributes: [
        "system": [
          AXFocusedAttributes.focusedApplication: AXFocusedAttributeRead(status: .success, value: .element("app")),
          AXFocusedAttributes.focusedUIElement: AXFocusedAttributeRead(status: .success, value: .element("button"))
        ],
        "app": [
          AXFocusedAttributes.focusedWindow: AXFocusedAttributeRead(status: .success, value: .element("sheet"))
        ],
        "sheet": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXSheet")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Save Changes")),
          AXFocusedAttributes.modal: AXFocusedAttributeRead(status: .success, value: .bool(true))
        ],
        "button": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXButton")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Save"))
        ]
      ],
      processIDs: [
        "app": 42
      ]
    )
  }

  static func focusedVisibleMap(
    pointerLocation: CGPoint? = nil,
    elementAtPointer: String? = nil
  ) -> FakeAXClient {
    FakeAXClient(
      attributes: [
        "system": [
          AXFocusedAttributes.focusedApplication: AXFocusedAttributeRead(status: .success, value: .element("app")),
          AXFocusedAttributes.focusedUIElement: AXFocusedAttributeRead(status: .success, value: .element("row-0"))
        ],
        "app": [
          AXFocusedAttributes.focusedWindow: AXFocusedAttributeRead(status: .success, value: .element("window"))
        ],
        "window": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXWindow")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Example Window")),
          AXFocusedAttributes.position: AXFocusedAttributeRead(status: .success, value: .point(CGPoint(x: 10, y: 20))),
          AXFocusedAttributes.size: AXFocusedAttributeRead(status: .success, value: .size(CGSize(width: 800, height: 600))),
          AXFocusedAttributes.visibleChildren: AXFocusedAttributeRead(
            status: .success,
            value: .elementArray(["row-0", "row-1", "row-2", "row-3"])
          )
        ],
        "row-0": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXRow")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Inbox Thread")),
          AXFocusedAttributes.position: AXFocusedAttributeRead(status: .success, value: .point(CGPoint(x: 20, y: 70))),
          AXFocusedAttributes.size: AXFocusedAttributeRead(status: .success, value: .size(CGSize(width: 760, height: 36))),
          AXFocusedAttributes.value: AXFocusedAttributeRead(status: .success, value: .string("selected row text should not serialize")),
          AXFocusedAttributes.selectedText: AXFocusedAttributeRead(status: .success, value: .string("selected row text should not serialize")),
          AXFocusedAttributes.selectedTextRange: AXFocusedAttributeRead(status: .success, value: .range(location: 22, length: 0)),
          AXFocusedAttributes.visibleCharacterRange: AXFocusedAttributeRead(status: .success, value: .range(location: 20, length: 50))
        ],
        "row-1": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXRow")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Password Reset Token 123")),
          AXFocusedAttributes.identifier: AXFocusedAttributeRead(status: .success, value: .string("password-row")),
          AXFocusedAttributes.position: AXFocusedAttributeRead(status: .success, value: .point(CGPoint(x: 20, y: 106))),
          AXFocusedAttributes.size: AXFocusedAttributeRead(status: .success, value: .size(CGSize(width: 760, height: 36))),
          AXFocusedAttributes.value: AXFocusedAttributeRead(status: .success, value: .string("secret row value should not serialize"))
        ],
        "row-2": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXRow")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Archive Thread"))
        ],
        "row-3": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXRow")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Later Thread"))
        ],
        "pointer-button": [
          AXFocusedAttributes.role: AXFocusedAttributeRead(status: .success, value: .string("AXButton")),
          AXFocusedAttributes.title: AXFocusedAttributeRead(status: .success, value: .string("Hover Password Token")),
          AXFocusedAttributes.position: AXFocusedAttributeRead(status: .success, value: .point(CGPoint(x: 300, y: 220))),
          AXFocusedAttributes.size: AXFocusedAttributeRead(status: .success, value: .size(CGSize(width: 120, height: 32)))
        ]
      ],
      processIDs: [
        "app": 42
      ],
      pointerLocation: pointerLocation,
      elementAtPointer: elementAtPointer
    )
  }
}
