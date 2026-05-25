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
}

private struct FakeAXClient: AXFocusedContextClient {
  var isTrusted = true
  var attributes: [String: [String: AXFocusedAttributeRead<String>]] = [:]
  var processIDs: [String: Int32] = [:]

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
          AXFocusedAttributes.value: AXFocusedAttributeRead(status: .success, value: .string("raw value should not be used"))
        ]
      ],
      processIDs: [
        "app": 42
      ]
    )
  }
}
