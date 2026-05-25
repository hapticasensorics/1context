import ApplicationServices
import CoreGraphics
import Foundation

enum AXFocusedAttributes {
  static let focusedApplication = kAXFocusedApplicationAttribute as String
  static let focusedWindow = kAXFocusedWindowAttribute as String
  static let focusedUIElement = kAXFocusedUIElementAttribute as String
  static let children = kAXChildrenAttribute as String
  static let role = kAXRoleAttribute as String
  static let subrole = kAXSubroleAttribute as String
  static let title = kAXTitleAttribute as String
  static let identifier = kAXIdentifierAttribute as String
  static let description = kAXDescriptionAttribute as String
  static let position = kAXPositionAttribute as String
  static let size = kAXSizeAttribute as String
  static let selectedText = kAXSelectedTextAttribute as String
  static let selectedTextRange = kAXSelectedTextRangeAttribute as String
  static let numberOfCharacters = kAXNumberOfCharactersAttribute as String
  static let value = kAXValueAttribute as String
  static let visibleCharacterRange = "AXVisibleCharacterRange"
  static let verticalScrollBar = "AXVerticalScrollBar"
  static let horizontalScrollBar = "AXHorizontalScrollBar"
  static let orientation = "AXOrientation"
  static let enabled = "AXEnabled"
  static let minValue = "AXMinValue"
  static let maxValue = "AXMaxValue"
  static let modal = "AXModal"
  static let visibleChildren = "AXVisibleChildren"
}

protocol AXFocusedContextClient {
  associatedtype Element

  func isProcessTrusted() -> Bool
  func systemWideElement() -> Element
  func applicationElement(pid: Int32) -> Element
  func processIdentifier(for element: Element) -> Int32?
  func setMessagingTimeout(_ element: Element, seconds: Float) -> CaptureAXReadStatus
  func copyAttribute(_ attribute: String, from element: Element) -> AXFocusedAttributeRead<Element>
  func currentPointerLocation() -> CGPoint?
  func elementAtPosition(_ point: CGPoint, in element: Element) -> AXFocusedAttributeRead<Element>
}

extension AXFocusedContextClient {
  func currentPointerLocation() -> CGPoint? {
    nil
  }

  func elementAtPosition(_ point: CGPoint, in element: Element) -> AXFocusedAttributeRead<Element> {
    AXFocusedAttributeRead(status: .noValue)
  }
}

struct AXFocusedAttributeRead<Element> {
  var status: CaptureAXReadStatus
  var value: AXFocusedAttributeValue<Element>?

  init(status: CaptureAXReadStatus, value: AXFocusedAttributeValue<Element>? = nil) {
    self.status = status
    self.value = value
  }
}

enum AXFocusedAttributeValue<Element> {
  case string(String)
  case int(Int)
  case double(Double)
  case bool(Bool)
  case element(Element)
  case elementArray([Element])
  case point(CGPoint)
  case size(CGSize)
  case range(location: Int, length: Int)
  case array(count: Int)
  case unsupported(String)

  var elementValue: Element? {
    guard case .element(let element) = self else { return nil }
    return element
  }

  var integerValue: Int? {
    switch self {
    case .int(let value):
      return value
    case .double(let value) where value.isFinite:
      return Int(value)
    default:
      return nil
    }
  }

  var doubleValue: Double? {
    switch self {
    case .int(let value):
      return Double(value)
    case .double(let value) where value.isFinite:
      return value
    default:
      return nil
    }
  }

  var valueKind: String {
    switch self {
    case .string:
      return "string"
    case .int, .double:
      return "number"
    case .bool:
      return "boolean"
    case .element:
      return "ax_element"
    case .elementArray:
      return "ax_element_array"
    case .point:
      return "point"
    case .size:
      return "size"
    case .range:
      return "range"
    case .array:
      return "array"
    case .unsupported(let description):
      return description
    }
  }
}

struct AXFocusedContextReader<Client: AXFocusedContextClient> {
  private let client: Client
  private let messagingTimeoutSeconds: Float
  private let maxMetadataCharacters: Int
  private let maxSelectedTextCharacters: Int
  private let maxVisibleRegionCount: Int
  private let maxVisibleChildrenPerNode: Int
  private let maxVisibleDepth: Int

  init(
    client: Client,
    messagingTimeoutSeconds: Float = 0.08,
    maxMetadataCharacters: Int = 256,
    maxSelectedTextCharacters: Int = 256,
    maxVisibleRegionCount: Int = 48,
    maxVisibleChildrenPerNode: Int = 24,
    maxVisibleDepth: Int = 2
  ) {
    self.client = client
    self.messagingTimeoutSeconds = messagingTimeoutSeconds
    self.maxMetadataCharacters = maxMetadataCharacters
    self.maxSelectedTextCharacters = maxSelectedTextCharacters
    self.maxVisibleRegionCount = max(1, maxVisibleRegionCount)
    self.maxVisibleChildrenPerNode = max(1, maxVisibleChildrenPerNode)
    self.maxVisibleDepth = max(0, maxVisibleDepth)
  }

  func read(generatedAt: String, activeApplication: CaptureActiveApplication?) -> CaptureAXFocusedContext {
    var issues: [CaptureAXFocusedContextIssue] = []

    guard client.isProcessTrusted() else {
      return CaptureAXFocusedContext(
        generatedAt: generatedAt,
        status: .notTrusted,
        isProcessTrusted: false,
        activeApplication: activeApplication,
        issues: [
          CaptureAXFocusedContextIssue(
            code: "accessibility_not_trusted",
            status: .apiDisabled,
            message: "Accessibility permission is not trusted; AX focused context was not read."
          )
        ]
      )
    }

    let systemElement = client.systemWideElement()
    noteTimeoutStatus(
      client.setMessagingTimeout(systemElement, seconds: messagingTimeoutSeconds),
      element: "system",
      issues: &issues
    )

    let focusedApplicationRead = elementAttribute(
      AXFocusedAttributes.focusedApplication,
      from: systemElement,
      elementName: "system",
      includeMisses: true,
      issues: &issues
    )

    let focusedApplicationElement: Client.Element?
    if let focusedApplicationRead {
      focusedApplicationElement = focusedApplicationRead
    } else if let pid = activeApplication?.processID {
      focusedApplicationElement = client.applicationElement(pid: pid)
      issues.append(
        CaptureAXFocusedContextIssue(
          code: "focused_application_fallback",
          status: .success,
          element: "application",
          attribute: AXFocusedAttributes.focusedApplication,
          message: "Fell back to NSWorkspace frontmost application PID for AX reads."
        )
      )
    } else {
      return CaptureAXFocusedContext(
        generatedAt: generatedAt,
        status: .noFocusedApplication,
        isProcessTrusted: true,
        activeApplication: activeApplication,
        issues: issues
      )
    }

    guard let appElement = focusedApplicationElement else {
      return CaptureAXFocusedContext(
        generatedAt: generatedAt,
        status: .noFocusedApplication,
        isProcessTrusted: true,
        activeApplication: activeApplication,
        issues: issues
      )
    }

    noteTimeoutStatus(
      client.setMessagingTimeout(appElement, seconds: messagingTimeoutSeconds),
      element: "application",
      issues: &issues
    )

    let focusedApplicationPID = client.processIdentifier(for: appElement) ?? activeApplication?.processID
    let focusedWindowElement = elementAttribute(
      AXFocusedAttributes.focusedWindow,
      from: appElement,
      elementName: "application",
      includeMisses: true,
      issues: &issues
    )

    let systemFocusedElement = elementAttribute(
      AXFocusedAttributes.focusedUIElement,
      from: systemElement,
      elementName: "system",
      includeMisses: false,
      issues: &issues
    )
    let focusedUIElement = systemFocusedElement ?? elementAttribute(
      AXFocusedAttributes.focusedUIElement,
      from: appElement,
      elementName: "application",
      includeMisses: true,
      issues: &issues
    )

    let focusedWindow = focusedWindowElement.map {
      noteTimeoutStatus(
        client.setMessagingTimeout($0, seconds: messagingTimeoutSeconds),
        element: "focused_window",
        issues: &issues
      )
      return nodeContext(for: $0, elementName: "focused_window", issues: &issues)
    }

    let focusedElement = focusedUIElement.map {
      noteTimeoutStatus(
        client.setMessagingTimeout($0, seconds: messagingTimeoutSeconds),
        element: "focused_element",
        issues: &issues
      )
      return nodeContext(for: $0, elementName: "focused_element", issues: &issues)
    }

    let visibleContext: CaptureAXVisibleContext?
    if let focusedWindowElement {
      visibleContext = self.visibleContext(
        for: focusedWindowElement,
        appElement: appElement,
        issues: &issues
      )
    } else {
      visibleContext = nil
    }

    return CaptureAXFocusedContext(
      generatedAt: generatedAt,
      status: status(focusedWindow: focusedWindow, focusedElement: focusedElement, issues: issues),
      isProcessTrusted: true,
      activeApplication: activeApplication,
      focusedApplicationProcessID: focusedApplicationPID,
      focusedWindow: focusedWindow,
      focusedElement: focusedElement,
      visibleContext: visibleContext,
      issues: issues
    )
  }

  private func status(
    focusedWindow: CaptureAXNodeContext?,
    focusedElement: CaptureAXNodeContext?,
    issues: [CaptureAXFocusedContextIssue]
  ) -> CaptureAXFocusedContextStatus {
    switch (focusedWindow, focusedElement) {
    case (.some, .some):
      return issues.isEmpty ? .available : .partial
    case (.none, .some):
      return .noFocusedWindow
    case (.some, .none):
      return .noFocusedElement
    case (.none, .none):
      return .noFocusedElement
    }
  }

  private func nodeContext(
    for element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXNodeContext {
    let role = stringAttribute(AXFocusedAttributes.role, from: element, elementName: elementName, issues: &issues)
    let subrole = stringAttribute(AXFocusedAttributes.subrole, from: element, elementName: elementName, issues: &issues)
    let title = stringAttribute(AXFocusedAttributes.title, from: element, elementName: elementName, issues: &issues)
    let identifier = stringAttribute(AXFocusedAttributes.identifier, from: element, elementName: elementName, issues: &issues)
    let description = stringAttribute(AXFocusedAttributes.description, from: element, elementName: elementName, issues: &issues)
    let isModal = boolAttribute(AXFocusedAttributes.modal, from: element, elementName: elementName, issues: &issues)
    let redactionReasons = sensitiveReasons(role: role, subrole: subrole, title: title, identifier: identifier, description: description)
    let isSensitive = !redactionReasons.isEmpty
    let safeTitle = isSensitive ? nil : title
    let frame = rectAttribute(from: element, elementName: elementName, issues: &issues)

    return CaptureAXNodeContext(
      role: role,
      subrole: subrole,
      title: safeTitle,
      identifier: isSensitive ? nil : identifier,
      elementDescription: isSensitive ? nil : description,
      frame: frame,
      valueShape: valueShape(
        for: element,
        role: role,
        subrole: subrole,
        isSensitive: isSensitive,
        elementName: elementName,
        issues: &issues
      ),
      selection: selectionContext(
        for: element,
        isSensitive: isSensitive,
        elementName: elementName,
        issues: &issues
      ),
      visibleRange: visibleTextRangeAttribute(from: element, elementName: elementName, issues: &issues),
      scroll: scrollContext(for: element, elementName: elementName, issues: &issues),
      transientUI: transientUIState(
        role: role,
        subrole: subrole,
        title: safeTitle,
        isModal: isModal,
        source: elementName
      ),
      isSensitive: isSensitive,
      redactionReasons: redactionReasons
    )
  }

  private func visibleContext(
    for focusedWindowElement: Client.Element,
    appElement: Client.Element,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXVisibleContext {
    var regions: [CaptureAXVisibleRegionSummary] = []
    var truncated = false
    appendVisibleRegion(
      focusedWindowElement,
      source: "focused_window",
      depth: 0,
      regions: &regions,
      truncated: &truncated,
      issues: &issues
    )

    let pointerHint = elementUnderPointerHint(in: appElement, issues: &issues)
    return CaptureAXVisibleContext(
      focusedWindowRegionID: regions.first?.regionID,
      regions: regions,
      capturedRegionCount: regions.count,
      maxRegionCount: maxVisibleRegionCount,
      maxDepth: maxVisibleDepth,
      truncated: truncated,
      elementUnderPointer: pointerHint
    )
  }

  private func appendVisibleRegion(
    _ element: Client.Element,
    source: String,
    depth: Int,
    regions: inout [CaptureAXVisibleRegionSummary],
    truncated: inout Bool,
    issues: inout [CaptureAXFocusedContextIssue]
  ) {
    guard regions.count < maxVisibleRegionCount else {
      truncated = true
      return
    }

    let elementName = "visible_region.\(source)"
    var summary = visibleRegionSummary(
      for: element,
      source: source,
      depth: depth,
      elementName: elementName,
      issues: &issues
    )
    var children: [Client.Element] = []
    var childCount: Int?
    var childrenTruncated = false

    if depth < maxVisibleDepth {
      let read = visibleChildElements(from: element, elementName: elementName, issues: &issues)
      children = read.elements
      childCount = read.reportedCount
      childrenTruncated = read.truncated
      if read.truncated {
        truncated = true
      }
    }

    summary.childCount = childCount
    summary.capturedChildCount = children.count
    summary.childrenTruncated = childrenTruncated
    regions.append(summary)

    guard depth < maxVisibleDepth else { return }
    for (index, child) in children.enumerated() {
      guard regions.count < maxVisibleRegionCount else {
        truncated = true
        return
      }
      appendVisibleRegion(
        child,
        source: "\(source).\(index)",
        depth: depth + 1,
        regions: &regions,
        truncated: &truncated,
        issues: &issues
      )
    }
  }

  private func visibleRegionSummary(
    for element: Client.Element,
    source: String,
    depth: Int,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXVisibleRegionSummary {
    let role = stringAttribute(AXFocusedAttributes.role, from: element, elementName: elementName, issues: &issues)
    let subrole = stringAttribute(AXFocusedAttributes.subrole, from: element, elementName: elementName, issues: &issues)
    let title = stringAttribute(AXFocusedAttributes.title, from: element, elementName: elementName, issues: &issues)
    let identifier = stringAttribute(AXFocusedAttributes.identifier, from: element, elementName: elementName, issues: &issues)
    let description = stringAttribute(AXFocusedAttributes.description, from: element, elementName: elementName, issues: &issues)
    let redactionReasons = sensitiveReasons(role: role, subrole: subrole, title: title, identifier: identifier, description: description)
    let isSensitive = !redactionReasons.isEmpty
    let titleShape = visibleTextShape(
      sourceAttribute: AXFocusedAttributes.title,
      text: title,
      redacted: isSensitive
    )
    let frame = rectAttribute(from: element, elementName: elementName, issues: &issues)
    let selectedTextRange = textRangeAttribute(from: element, elementName: elementName, issues: &issues)

    return CaptureAXVisibleRegionSummary(
      regionID: stableRegionID(
        role: role,
        subrole: subrole,
        titleShape: titleShape,
        frame: frame
      ),
      source: source,
      depth: depth,
      role: role,
      subrole: subrole,
      titleShape: titleShape,
      frame: frame,
      valueShape: valueShape(
        for: element,
        role: role,
        subrole: subrole,
        isSensitive: isSensitive,
        elementName: elementName,
        issues: &issues
      ),
      visibleRange: visibleTextRangeAttribute(from: element, elementName: elementName, issues: &issues),
      selectedTextRange: selectedTextRange,
      isInsertionPoint: selectedTextRange.map { $0.length == 0 },
      scroll: scrollContext(for: element, elementName: elementName, issues: &issues),
      isSensitive: isSensitive,
      redactionReasons: redactionReasons
    )
  }

  private func visibleTextShape(
    sourceAttribute: String,
    text: String?,
    redacted: Bool
  ) -> CaptureAXVisibleTextShape? {
    guard let text else { return nil }
    if redacted {
      return CaptureAXVisibleTextShape(sourceAttribute: sourceAttribute, redacted: true)
    }
    let cappedText = capped(text, maxCharacters: maxMetadataCharacters)
    return CaptureAXVisibleTextShape(
      sourceAttribute: sourceAttribute,
      characterCount: text.count,
      truncated: cappedText.truncated
    )
  }

  private func visibleChildElements(
    from element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> (elements: [Client.Element], reportedCount: Int?, truncated: Bool) {
    let visibleRead = childElementsAttribute(
      AXFocusedAttributes.visibleChildren,
      from: element,
      elementName: elementName,
      issues: &issues
    )
    if visibleRead.status == .success {
      return boundedChildren(from: visibleRead.elements, reportedCount: visibleRead.reportedCount)
    }

    let fallbackRead = childElementsAttribute(
      AXFocusedAttributes.children,
      from: element,
      elementName: elementName,
      issues: &issues
    )
    if fallbackRead.status == .success {
      return boundedChildren(from: fallbackRead.elements, reportedCount: fallbackRead.reportedCount)
    }

    return ([], nil, false)
  }

  private func boundedChildren(
    from elements: [Client.Element],
    reportedCount: Int?
  ) -> (elements: [Client.Element], reportedCount: Int?, truncated: Bool) {
    let bounded = Array(elements.prefix(maxVisibleChildrenPerNode))
    let totalCount = reportedCount ?? elements.count
    return (bounded, totalCount, totalCount > bounded.count)
  }

  private func childElementsAttribute(
    _ attribute: String,
    from element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> (status: CaptureAXReadStatus, elements: [Client.Element], reportedCount: Int?) {
    let read = client.copyAttribute(attribute, from: element)
    guard read.status == .success, let value = read.value else {
      return (read.status, [], nil)
    }
    switch value {
    case .elementArray(let elements):
      return (.success, elements, elements.count)
    case .array(let count):
      return (.success, [], count)
    default:
      appendWrongTypeIssue(
        element: elementName,
        attribute: attribute,
        expectedType: "ax_element_array",
        actualType: value.valueKind,
        issues: &issues
      )
      return (.wrongType, [], nil)
    }
  }

  private func elementUnderPointerHint(
    in appElement: Client.Element,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXElementUnderPointerHint? {
    guard let pointerLocation = client.currentPointerLocation() else {
      return nil
    }
    let read = client.elementAtPosition(pointerLocation, in: appElement)
    guard read.status == .success, let value = read.value else {
      return nil
    }
    guard let element = value.elementValue else {
      appendWrongTypeIssue(
        element: "element_under_pointer",
        attribute: "AXElementAtPosition",
        expectedType: "ax_element",
        actualType: value.valueKind,
        issues: &issues
      )
      return nil
    }

    let summary = visibleRegionSummary(
      for: element,
      source: "element_under_pointer",
      depth: 0,
      elementName: "element_under_pointer",
      issues: &issues
    )
    return CaptureAXElementUnderPointerHint(
      regionID: summary.regionID,
      role: summary.role,
      subrole: summary.subrole,
      titleShape: summary.titleShape,
      frame: summary.frame,
      isSensitive: summary.isSensitive,
      redactionReasons: summary.redactionReasons
    )
  }

  private func stringAttribute(
    _ attribute: String,
    from element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> String? {
    let read = client.copyAttribute(attribute, from: element)
    guard read.status == .success else {
      return nil
    }
    guard let value = read.value else {
      return nil
    }
    guard case .string(let string) = value else {
      appendWrongTypeIssue(
        element: elementName,
        attribute: attribute,
        expectedType: "string",
        actualType: value.valueKind,
        issues: &issues
      )
      return nil
    }
    return capped(string, maxCharacters: maxMetadataCharacters).text
  }

  private func boolAttribute(
    _ attribute: String,
    from element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> Bool? {
    let read = client.copyAttribute(attribute, from: element)
    guard read.status == .success, let value = read.value else {
      return nil
    }
    guard case .bool(let bool) = value else {
      appendWrongTypeIssue(
        element: elementName,
        attribute: attribute,
        expectedType: "boolean",
        actualType: value.valueKind,
        issues: &issues
      )
      return nil
    }
    return bool
  }

  private func numericAttribute(
    _ attribute: String,
    from element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> Double? {
    let read = client.copyAttribute(attribute, from: element)
    guard read.status == .success, let value = read.value else {
      return nil
    }
    guard let double = value.doubleValue else {
      appendWrongTypeIssue(
        element: elementName,
        attribute: attribute,
        expectedType: "number",
        actualType: value.valueKind,
        issues: &issues
      )
      return nil
    }
    return double
  }

  private func rectAttribute(
    from element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureRect? {
    let positionRead = client.copyAttribute(AXFocusedAttributes.position, from: element)
    let sizeRead = client.copyAttribute(AXFocusedAttributes.size, from: element)
    guard positionRead.status == .success, let positionValue = positionRead.value else {
      return nil
    }
    guard sizeRead.status == .success, let sizeValue = sizeRead.value else {
      return nil
    }
    guard case .point(let point) = positionValue else {
      appendWrongTypeIssue(
        element: elementName,
        attribute: AXFocusedAttributes.position,
        expectedType: "point",
        actualType: positionValue.valueKind,
        issues: &issues
      )
      return nil
    }
    guard case .size(let size) = sizeValue else {
      appendWrongTypeIssue(
        element: elementName,
        attribute: AXFocusedAttributes.size,
        expectedType: "size",
        actualType: sizeValue.valueKind,
        issues: &issues
      )
      return nil
    }
    return CaptureRect(x: point.x, y: point.y, width: size.width, height: size.height)
  }

  private func visibleTextRangeAttribute(
    from element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXTextRange? {
    let read = client.copyAttribute(AXFocusedAttributes.visibleCharacterRange, from: element)
    guard read.status == .success, let value = read.value else {
      return nil
    }
    guard case .range(let location, let length) = value else {
      appendWrongTypeIssue(
        element: elementName,
        attribute: AXFocusedAttributes.visibleCharacterRange,
        expectedType: "range",
        actualType: value.valueKind,
        issues: &issues
      )
      return nil
    }
    return CaptureAXTextRange(location: location, length: length)
  }

  private func scrollContext(
    for element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXScrollContext? {
    let verticalElement = elementAttribute(
      AXFocusedAttributes.verticalScrollBar,
      from: element,
      elementName: elementName,
      includeMisses: false,
      issues: &issues
    )
    let horizontalElement = elementAttribute(
      AXFocusedAttributes.horizontalScrollBar,
      from: element,
      elementName: elementName,
      includeMisses: false,
      issues: &issues
    )

    let vertical = verticalElement.map {
      scrollBarContext(for: $0, orientationHint: "vertical", elementName: "\(elementName).vertical_scrollbar", issues: &issues)
    }
    let horizontal = horizontalElement.map {
      scrollBarContext(for: $0, orientationHint: "horizontal", elementName: "\(elementName).horizontal_scrollbar", issues: &issues)
    }

    guard vertical != nil || horizontal != nil else {
      return nil
    }
    return CaptureAXScrollContext(vertical: vertical, horizontal: horizontal)
  }

  private func scrollBarContext(
    for element: Client.Element,
    orientationHint: String,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXScrollBarContext {
    let role = stringAttribute(AXFocusedAttributes.role, from: element, elementName: elementName, issues: &issues)
    let orientation = stringAttribute(AXFocusedAttributes.orientation, from: element, elementName: elementName, issues: &issues)
      ?? orientationHint
    let isEnabled = boolAttribute(AXFocusedAttributes.enabled, from: element, elementName: elementName, issues: &issues)
    let value = numericAttribute(AXFocusedAttributes.value, from: element, elementName: elementName, issues: &issues)
    let minValue = numericAttribute(AXFocusedAttributes.minValue, from: element, elementName: elementName, issues: &issues)
    let maxValue = numericAttribute(AXFocusedAttributes.maxValue, from: element, elementName: elementName, issues: &issues)
    return CaptureAXScrollBarContext(
      orientation: normalizedScrollOrientation(orientation),
      role: role,
      isEnabled: isEnabled,
      value: value,
      minValue: minValue,
      maxValue: maxValue,
      valuePercent: scrollPercent(value: value, minValue: minValue, maxValue: maxValue)
    )
  }

  private func normalizedScrollOrientation(_ orientation: String) -> String {
    let lowercased = orientation.lowercased()
    if lowercased.contains("vertical") {
      return "vertical"
    }
    if lowercased.contains("horizontal") {
      return "horizontal"
    }
    return orientation
  }

  private func scrollPercent(value: Double?, minValue: Double?, maxValue: Double?) -> Double? {
    guard let value, let minValue, let maxValue, maxValue > minValue else {
      return nil
    }
    return max(0, min(1, (value - minValue) / (maxValue - minValue)))
  }

  private func valueShape(
    for element: Client.Element,
    role: String?,
    subrole: String?,
    isSensitive: Bool,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXValueShape? {
    if isSensitive {
      return CaptureAXValueShape(kind: "redacted", redacted: true)
    }

    if isTextLike(role: role, subrole: subrole) {
      let read = client.copyAttribute(AXFocusedAttributes.numberOfCharacters, from: element)
      if read.status == .success, let value = read.value {
        guard let characterCount = value.integerValue else {
          appendWrongTypeIssue(
            element: elementName,
            attribute: AXFocusedAttributes.numberOfCharacters,
            expectedType: "number",
            actualType: value.valueKind,
            issues: &issues
          )
          return CaptureAXValueShape(kind: "text", sourceAttribute: AXFocusedAttributes.numberOfCharacters)
        }
        return CaptureAXValueShape(
          kind: "text",
          characterCount: characterCount,
          sourceAttribute: AXFocusedAttributes.numberOfCharacters
        )
      }
      return CaptureAXValueShape(kind: "text", sourceAttribute: AXFocusedAttributes.numberOfCharacters)
    }

    let read = client.copyAttribute(AXFocusedAttributes.value, from: element)
    guard read.status == .success, let value = read.value else {
      return nil
    }
    switch value {
    case .string(let string):
      return CaptureAXValueShape(kind: "string", characterCount: string.count, sourceAttribute: AXFocusedAttributes.value)
    case .int, .double:
      return CaptureAXValueShape(kind: "number", sourceAttribute: AXFocusedAttributes.value)
    case .bool:
      return CaptureAXValueShape(kind: "boolean", sourceAttribute: AXFocusedAttributes.value)
    case .point:
      return CaptureAXValueShape(kind: "point", sourceAttribute: AXFocusedAttributes.value)
    case .size:
      return CaptureAXValueShape(kind: "size", sourceAttribute: AXFocusedAttributes.value)
    case .range:
      return CaptureAXValueShape(kind: "range", sourceAttribute: AXFocusedAttributes.value)
    case .element:
      return CaptureAXValueShape(kind: "ax_element", sourceAttribute: AXFocusedAttributes.value)
    case .elementArray(let elements):
      return CaptureAXValueShape(kind: "ax_element_array", characterCount: elements.count, sourceAttribute: AXFocusedAttributes.value)
    case .array(let count):
      return CaptureAXValueShape(kind: "array", characterCount: count, sourceAttribute: AXFocusedAttributes.value)
    case .unsupported(let description):
      return CaptureAXValueShape(kind: description, sourceAttribute: AXFocusedAttributes.value)
    }
  }

  private func selectionContext(
    for element: Client.Element,
    isSensitive: Bool,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXSelectionContext? {
    if isSensitive {
      return CaptureAXSelectionContext(selectedTextRedacted: true)
    }

    let range = textRangeAttribute(from: element, elementName: elementName, issues: &issues)
    let selectedTextRead = client.copyAttribute(AXFocusedAttributes.selectedText, from: element)
    var selectedText: String?
    var selectedTextCharacterCount: Int?
    var selectedTextTruncated = false

    if selectedTextRead.status == .success, let value = selectedTextRead.value {
      guard case .string(let string) = value else {
        appendWrongTypeIssue(
          element: elementName,
          attribute: AXFocusedAttributes.selectedText,
          expectedType: "string",
          actualType: value.valueKind,
          issues: &issues
        )
        return range.map {
          CaptureAXSelectionContext(range: $0)
        }
      }
      let cappedText = capped(string, maxCharacters: maxSelectedTextCharacters)
      selectedText = cappedText.text
      selectedTextCharacterCount = string.count
      selectedTextTruncated = cappedText.truncated
    }

    guard range != nil || selectedText != nil else {
      return nil
    }

    return CaptureAXSelectionContext(
      range: range,
      isInsertionPoint: range.map { $0.length == 0 },
      selectedText: selectedText,
      selectedTextCharacterCount: selectedTextCharacterCount,
      selectedTextTruncated: selectedTextTruncated
    )
  }

  private func textRangeAttribute(
    from element: Client.Element,
    elementName: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> CaptureAXTextRange? {
    let read = client.copyAttribute(AXFocusedAttributes.selectedTextRange, from: element)
    guard read.status == .success, let value = read.value else {
      return nil
    }
    guard case .range(let location, let length) = value else {
      appendWrongTypeIssue(
        element: elementName,
        attribute: AXFocusedAttributes.selectedTextRange,
        expectedType: "range",
        actualType: value.valueKind,
        issues: &issues
      )
      return nil
    }
    return CaptureAXTextRange(location: location, length: length)
  }

  private func elementAttribute(
    _ attribute: String,
    from element: Client.Element,
    elementName: String,
    includeMisses: Bool,
    issues: inout [CaptureAXFocusedContextIssue]
  ) -> Client.Element? {
    let read = client.copyAttribute(attribute, from: element)
    guard read.status == .success else {
      if includeMisses {
        issues.append(
          CaptureAXFocusedContextIssue(
            code: "ax_attribute_unavailable",
            status: read.status,
            element: elementName,
            attribute: attribute,
            message: "AX attribute was not available."
          )
        )
      }
      return nil
    }
    guard let value = read.value else {
      if includeMisses {
        issues.append(
          CaptureAXFocusedContextIssue(
            code: "ax_attribute_empty",
            status: .noValue,
            element: elementName,
            attribute: attribute,
            expectedType: "ax_element",
            message: "AX attribute returned no value."
          )
        )
      }
      return nil
    }
    guard let element = value.elementValue else {
      appendWrongTypeIssue(
        element: elementName,
        attribute: attribute,
        expectedType: "ax_element",
        actualType: value.valueKind,
        issues: &issues
      )
      return nil
    }
    return element
  }

  private func noteTimeoutStatus(
    _ status: CaptureAXReadStatus,
    element: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) {
    guard status != .success else { return }
    issues.append(
      CaptureAXFocusedContextIssue(
        code: "ax_messaging_timeout_setup_failed",
        status: status,
        element: element,
        message: "AX messaging timeout could not be applied."
      )
    )
  }

  private func appendWrongTypeIssue(
    element: String,
    attribute: String,
    expectedType: String,
    actualType: String,
    issues: inout [CaptureAXFocusedContextIssue]
  ) {
    issues.append(
      CaptureAXFocusedContextIssue(
        code: "ax_attribute_wrong_type",
        status: .wrongType,
        element: element,
        attribute: attribute,
        expectedType: expectedType,
        actualType: actualType,
        message: "AX attribute returned an unexpected value type."
      )
    )
  }

  private func isTextLike(role: String?, subrole: String?) -> Bool {
    [
      "AXTextField",
      "AXTextArea",
      "AXComboBox"
    ].contains(role ?? "")
      || [
        "AXSearchField",
        "AXSecureTextField"
      ].contains(subrole ?? "")
  }

  private func sensitiveReasons(
    role: String?,
    subrole: String?,
    title: String?,
    identifier: String?,
    description: String?
  ) -> [String] {
    var reasons: [String] = []
    if subrole == "AXSecureTextField" {
      reasons.append("secure_text_field")
    }

    let metadata = [role, subrole, title, identifier, description]
      .compactMap { $0?.lowercased() }
      .joined(separator: " ")
    let markers = [
      "password",
      "passcode",
      "pass phrase",
      "secret",
      "token",
      "api key",
      "auth code",
      "security code",
      "one-time code",
      "otp",
      "credit card",
      "ssn",
      "social security"
    ]
    if markers.contains(where: { metadata.contains($0) }) {
      reasons.append("sensitive_metadata")
    }
    return Array(Set(reasons)).sorted()
  }

  private func transientUIState(
    role: String?,
    subrole: String?,
    title: String?,
    isModal: Bool?,
    source: String
  ) -> CaptureAXTransientUIState? {
    if let kind = transientKind(role: role, subrole: subrole, isModal: isModal) {
      return CaptureAXTransientUIState(
        kind: kind,
        source: source,
        role: role,
        subrole: subrole,
        title: title,
        isModal: isModal
      )
    }
    return nil
  }

  private func transientKind(role: String?, subrole: String?, isModal: Bool?) -> CaptureAXTransientUIKind? {
    let role = role ?? ""
    let subrole = subrole ?? ""
    if role == "AXSheet" || subrole == "AXSheet" {
      return .sheet
    }
    if role == "AXMenu" || role == "AXMenuItem" || role == "AXMenuBar" || subrole.contains("Menu") {
      return .menu
    }
    if role == "AXPopover" || subrole == "AXPopover" {
      return .popover
    }
    if role == "AXDialog" || subrole.contains("Dialog") || subrole.contains("SystemDialog") {
      return .dialog
    }
    if isModal == true {
      return .modal
    }
    return nil
  }

  private func stableRegionID(
    role: String?,
    subrole: String?,
    titleShape: CaptureAXVisibleTextShape?,
    frame: CaptureRect?
  ) -> String {
    let components = [
      role ?? "",
      subrole ?? "",
      titleShape?.sourceAttribute ?? "",
      titleShape?.characterCount.map(String.init) ?? "",
      titleShape?.redacted == true ? "redacted" : "clear",
      rectSignature(frame)
    ]
    return "axr_\(fnv1a64Hex(components.joined(separator: "\u{1f}")))"
  }

  private func rectSignature(_ rect: CaptureRect?) -> String {
    guard let rect else { return "" }
    return [
      rounded(rect.x),
      rounded(rect.y),
      rounded(rect.width),
      rounded(rect.height)
    ].joined(separator: ",")
  }

  private func rounded(_ value: Double) -> String {
    String(format: "%.2f", value)
  }

  private func fnv1a64Hex(_ string: String) -> String {
    var hash: UInt64 = 0xcbf29ce484222325
    for byte in string.utf8 {
      hash ^= UInt64(byte)
      hash &*= 0x100000001b3
    }
    return String(format: "%016llx", hash)
  }

  private func capped(_ text: String, maxCharacters: Int) -> (text: String, truncated: Bool) {
    guard text.count > maxCharacters else {
      return (text, false)
    }
    return (String(text.prefix(maxCharacters)), true)
  }
}

struct SystemAXFocusedContextClient: AXFocusedContextClient {
  func isProcessTrusted() -> Bool {
    AXIsProcessTrusted()
  }

  func systemWideElement() -> AXUIElement {
    AXUIElementCreateSystemWide()
  }

  func applicationElement(pid: Int32) -> AXUIElement {
    AXUIElementCreateApplication(pid_t(pid))
  }

  func processIdentifier(for element: AXUIElement) -> Int32? {
    var pid: pid_t = 0
    guard AXUIElementGetPid(element, &pid) == .success else {
      return nil
    }
    return Int32(pid)
  }

  func setMessagingTimeout(_ element: AXUIElement, seconds: Float) -> CaptureAXReadStatus {
    CaptureAXReadStatus(axError: AXUIElementSetMessagingTimeout(element, seconds))
  }

  func copyAttribute(_ attribute: String, from element: AXUIElement) -> AXFocusedAttributeRead<AXUIElement> {
    var value: CFTypeRef?
    let error = AXUIElementCopyAttributeValue(element, attribute as CFString, &value)
    let status = CaptureAXReadStatus(axError: error)
    guard status == .success, let value else {
      return AXFocusedAttributeRead(status: status)
    }
    return AXFocusedAttributeRead(status: status, value: Self.convert(value))
  }

  func currentPointerLocation() -> CGPoint? {
    CGEvent(source: nil)?.location
  }

  func elementAtPosition(_ point: CGPoint, in element: AXUIElement) -> AXFocusedAttributeRead<AXUIElement> {
    var target: AXUIElement?
    let error = AXUIElementCopyElementAtPosition(element, Float(point.x), Float(point.y), &target)
    let status = CaptureAXReadStatus(axError: error)
    guard status == .success, let target else {
      return AXFocusedAttributeRead(status: status)
    }
    return AXFocusedAttributeRead(status: status, value: .element(target))
  }

  private static func convert(_ value: CFTypeRef) -> AXFocusedAttributeValue<AXUIElement> {
    let typeID = CFGetTypeID(value)

    if typeID == AXUIElementGetTypeID(),
      let element: AXUIElement = checkedCFType(value, expectedTypeID: AXUIElementGetTypeID())
    {
      return .element(element)
    }

    if typeID == AXValueGetTypeID(),
      let axValue: AXValue = checkedCFType(value, expectedTypeID: AXValueGetTypeID())
    {
      switch AXValueGetType(axValue) {
      case .cgPoint:
        var point = CGPoint.zero
        guard AXValueGetValue(axValue, .cgPoint, &point) else {
          return .unsupported("ax_value_point_unreadable")
        }
        return .point(point)
      case .cgSize:
        var size = CGSize.zero
        guard AXValueGetValue(axValue, .cgSize, &size) else {
          return .unsupported("ax_value_size_unreadable")
        }
        return .size(size)
      case .cfRange:
        var range = CFRange(location: 0, length: 0)
        guard AXValueGetValue(axValue, .cfRange, &range) else {
          return .unsupported("ax_value_range_unreadable")
        }
        return .range(location: range.location, length: range.length)
      default:
        return .unsupported("ax_value_\(AXValueGetType(axValue).rawValue)")
      }
    }

    if let string = value as? String {
      return .string(string)
    }

    if let bool = value as? Bool {
      return .bool(bool)
    }

    if let number = value as? NSNumber {
      let doubleValue = number.doubleValue
      if doubleValue.rounded() == doubleValue {
        return .int(number.intValue)
      }
      return .double(doubleValue)
    }

    if typeID == CFArrayGetTypeID(),
      let array: CFArray = checkedCFType(value, expectedTypeID: CFArrayGetTypeID())
    {
      let count = CFArrayGetCount(array)
      var elements: [AXUIElement] = []
      elements.reserveCapacity(count)
      for index in 0..<count {
        guard let rawValue = CFArrayGetValueAtIndex(array, index) else {
          continue
        }
        let item = unsafeBitCast(rawValue, to: CFTypeRef.self)
        if CFGetTypeID(item) == AXUIElementGetTypeID(),
          let element: AXUIElement = checkedCFType(item, expectedTypeID: AXUIElementGetTypeID())
        {
          elements.append(element)
        }
      }
      if elements.count == count {
        return .elementArray(elements)
      }
      return .array(count: count)
    }

    return .unsupported("cf_type_\(typeID)")
  }

  private static func checkedCFType<T>(_ value: CFTypeRef, expectedTypeID: CFTypeID) -> T? {
    guard CFGetTypeID(value) == expectedTypeID else {
      return nil
    }
    // Swift disallows conditional casts for these CF-backed AX types after a type-id check.
    return unsafeBitCast(value, to: T.self)
  }
}

extension CaptureAXReadStatus {
  init(axError: AXError) {
    switch axError {
    case .success:
      self = .success
    case .failure:
      self = .failure
    case .illegalArgument:
      self = .illegalArgument
    case .invalidUIElement:
      self = .invalidUIElement
    case .invalidUIElementObserver:
      self = .invalidUIElementObserver
    case .cannotComplete:
      self = .cannotComplete
    case .attributeUnsupported:
      self = .attributeUnsupported
    case .actionUnsupported:
      self = .actionUnsupported
    case .notificationUnsupported:
      self = .notificationUnsupported
    case .notImplemented:
      self = .notImplemented
    case .notificationAlreadyRegistered:
      self = .notificationAlreadyRegistered
    case .notificationNotRegistered:
      self = .notificationNotRegistered
    case .apiDisabled:
      self = .apiDisabled
    case .noValue:
      self = .noValue
    case .parameterizedAttributeUnsupported:
      self = .parameterizedAttributeUnsupported
    case .notEnoughPrecision:
      self = .notEnoughPrecision
    @unknown default:
      self = .unknown
    }
  }
}
