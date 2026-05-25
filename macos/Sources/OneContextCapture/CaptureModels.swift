import Foundation

public struct CaptureRect: Codable, Equatable, Sendable {
  public var x: Double
  public var y: Double
  public var width: Double
  public var height: Double

  public init(x: Double, y: Double, width: Double, height: Double) {
    self.x = x
    self.y = y
    self.width = width
    self.height = height
  }
}

public struct ActiveWindowMetadataTarget: Codable, Equatable, Sendable {
  public var windowID: UInt32
  public var appPID: Int32
  public var appName: String
  public var bundleID: String?
  public var title: String
  public var framePoints: CaptureRect
  public var framePixels: CaptureRect?
  public var displayID: String?
  public var zRank: Int
  public var isFocused: Bool
  public var captureEligible: Bool
  public var source: String

  public init(
    windowID: UInt32,
    appPID: Int32,
    appName: String,
    bundleID: String?,
    title: String,
    framePoints: CaptureRect,
    framePixels: CaptureRect? = nil,
    displayID: String? = nil,
    zRank: Int,
    isFocused: Bool,
    captureEligible: Bool,
    source: String
  ) {
    self.windowID = windowID
    self.appPID = appPID
    self.appName = appName
    self.bundleID = bundleID
    self.title = title
    self.framePoints = framePoints
    self.framePixels = framePixels
    self.displayID = displayID
    self.zRank = zRank
    self.isFocused = isFocused
    self.captureEligible = captureEligible
    self.source = source
  }

  public init(window: CaptureWindowState) {
    self.init(
      windowID: window.windowID,
      appPID: window.appPID,
      appName: window.appName,
      bundleID: window.bundleID,
      title: window.title,
      framePoints: window.framePoints,
      framePixels: window.framePixels,
      displayID: window.displayID,
      zRank: window.zRank,
      isFocused: window.isFocused,
      captureEligible: window.captureEligible,
      source: window.source
    )
  }

  public static func select(from snapshot: CaptureSnapshot) -> ActiveWindowMetadataTarget? {
    let candidates = snapshot.windows
      .filter { $0.captureEligible && $0.isOnScreen && !$0.isMinimized }
      .sorted { lhs, rhs in
        if lhs.isFocused != rhs.isFocused {
          return lhs.isFocused && !rhs.isFocused
        }
        if lhs.zRank == rhs.zRank {
          return lhs.windowID < rhs.windowID
        }
        return lhs.zRank < rhs.zRank
      }
    return candidates.first.map(ActiveWindowMetadataTarget.init(window:))
  }
}

public struct CaptureActiveApplication: Codable, Equatable, Sendable {
  public var processID: Int32
  public var bundleID: String?
  public var appName: String

  public init(processID: Int32, bundleID: String?, appName: String) {
    self.processID = processID
    self.bundleID = bundleID
    self.appName = appName
  }
}

public struct CaptureDisplayState: Codable, Equatable, Sendable {
  public var displayID: String
  public var framePoints: CaptureRect
  public var scaleFactor: Double
  public var isMain: Bool

  public init(displayID: String, framePoints: CaptureRect, scaleFactor: Double, isMain: Bool) {
    self.displayID = displayID
    self.framePoints = framePoints
    self.scaleFactor = scaleFactor
    self.isMain = isMain
  }
}

public struct CaptureWindowState: Codable, Equatable, Sendable {
  public var time: String
  public var windowID: UInt32
  public var appPID: Int32
  public var appName: String
  public var bundleID: String?
  public var title: String
  public var framePoints: CaptureRect
  public var framePixels: CaptureRect?
  public var displayID: String?
  public var zRank: Int
  public var layer: Int
  public var alpha: Double?
  public var isFocused: Bool
  public var isOnScreen: Bool
  public var isMinimized: Bool
  public var visibleFractionEstimate: Double?
  public var captureEligible: Bool
  public var source: String
  public var focusMetadata: CaptureWindowFocusMetadata?

  public init(
    time: String,
    windowID: UInt32,
    appPID: Int32,
    appName: String,
    bundleID: String?,
    title: String,
    framePoints: CaptureRect,
    framePixels: CaptureRect? = nil,
    displayID: String? = nil,
    zRank: Int,
    layer: Int,
    alpha: Double? = nil,
    isFocused: Bool,
    isOnScreen: Bool,
    isMinimized: Bool,
    visibleFractionEstimate: Double? = nil,
    captureEligible: Bool,
    source: String,
    focusMetadata: CaptureWindowFocusMetadata? = nil
  ) {
    self.time = time
    self.windowID = windowID
    self.appPID = appPID
    self.appName = appName
    self.bundleID = bundleID
    self.title = title
    self.framePoints = framePoints
    self.framePixels = framePixels
    self.displayID = displayID
    self.zRank = zRank
    self.layer = layer
    self.alpha = alpha
    self.isFocused = isFocused
    self.isOnScreen = isOnScreen
    self.isMinimized = isMinimized
    self.visibleFractionEstimate = visibleFractionEstimate
    self.captureEligible = captureEligible
    self.source = source
    self.focusMetadata = focusMetadata
  }
}

public struct CaptureSnapshot: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var generatedAt: String
  public var activeApplication: CaptureActiveApplication?
  public var displays: [CaptureDisplayState]
  public var windows: [CaptureWindowState]
  public var focusedContext: CaptureAXFocusedContext?

  public init(
    schemaVersion: Int = 1,
    generatedAt: String,
    activeApplication: CaptureActiveApplication?,
    displays: [CaptureDisplayState],
    windows: [CaptureWindowState],
    focusedContext: CaptureAXFocusedContext? = nil
  ) {
    self.schemaVersion = schemaVersion
    self.generatedAt = generatedAt
    self.activeApplication = activeApplication
    self.displays = displays
    self.windows = windows
    self.focusedContext = focusedContext
  }
}

public struct CaptureWindowFocusMetadata: Codable, Equatable, Sendable {
  public var source: String
  public var status: String
  public var confidence: String
  public var matchedWindowID: UInt32?
  public var matchSignals: [String]

  public init(
    source: String,
    status: String,
    confidence: String,
    matchedWindowID: UInt32? = nil,
    matchSignals: [String] = []
  ) {
    self.source = source
    self.status = status
    self.confidence = confidence
    self.matchedWindowID = matchedWindowID
    self.matchSignals = matchSignals
  }
}

public enum CaptureAXFocusedContextStatus: String, Codable, Equatable, Sendable {
  case available
  case partial
  case notTrusted = "not_trusted"
  case noFocusedApplication = "no_focused_application"
  case noFocusedWindow = "no_focused_window"
  case noFocusedElement = "no_focused_element"
}

public enum CaptureAXReadStatus: String, Codable, Equatable, Sendable {
  case success
  case failure
  case illegalArgument = "illegal_argument"
  case invalidUIElement = "invalid_ui_element"
  case invalidUIElementObserver = "invalid_ui_element_observer"
  case cannotComplete = "cannot_complete"
  case attributeUnsupported = "attribute_unsupported"
  case actionUnsupported = "action_unsupported"
  case notificationUnsupported = "notification_unsupported"
  case notImplemented = "not_implemented"
  case notificationAlreadyRegistered = "notification_already_registered"
  case notificationNotRegistered = "notification_not_registered"
  case apiDisabled = "api_disabled"
  case noValue = "no_value"
  case parameterizedAttributeUnsupported = "parameterized_attribute_unsupported"
  case notEnoughPrecision = "not_enough_precision"
  case wrongType = "wrong_type"
  case unknown
}

public struct CaptureAXFocusedContextIssue: Codable, Equatable, Sendable {
  public var code: String
  public var status: CaptureAXReadStatus?
  public var element: String?
  public var attribute: String?
  public var expectedType: String?
  public var actualType: String?
  public var message: String

  public init(
    code: String,
    status: CaptureAXReadStatus? = nil,
    element: String? = nil,
    attribute: String? = nil,
    expectedType: String? = nil,
    actualType: String? = nil,
    message: String
  ) {
    self.code = code
    self.status = status
    self.element = element
    self.attribute = attribute
    self.expectedType = expectedType
    self.actualType = actualType
    self.message = message
  }
}

public struct CaptureAXTextRange: Codable, Equatable, Sendable {
  public var location: Int
  public var length: Int

  public init(location: Int, length: Int) {
    self.location = location
    self.length = length
  }
}

public struct CaptureAXSelectionContext: Codable, Equatable, Sendable {
  public var range: CaptureAXTextRange?
  public var selectedText: String?
  public var selectedTextCharacterCount: Int?
  public var selectedTextTruncated: Bool
  public var selectedTextRedacted: Bool

  public init(
    range: CaptureAXTextRange? = nil,
    selectedText: String? = nil,
    selectedTextCharacterCount: Int? = nil,
    selectedTextTruncated: Bool = false,
    selectedTextRedacted: Bool = false
  ) {
    self.range = range
    self.selectedText = selectedText
    self.selectedTextCharacterCount = selectedTextCharacterCount
    self.selectedTextTruncated = selectedTextTruncated
    self.selectedTextRedacted = selectedTextRedacted
  }
}

public struct CaptureAXValueShape: Codable, Equatable, Sendable {
  public var kind: String
  public var characterCount: Int?
  public var sourceAttribute: String?
  public var redacted: Bool

  public init(
    kind: String,
    characterCount: Int? = nil,
    sourceAttribute: String? = nil,
    redacted: Bool = false
  ) {
    self.kind = kind
    self.characterCount = characterCount
    self.sourceAttribute = sourceAttribute
    self.redacted = redacted
  }
}

public struct CaptureAXNodeContext: Codable, Equatable, Sendable {
  public var role: String?
  public var subrole: String?
  public var title: String?
  public var identifier: String?
  public var elementDescription: String?
  public var frame: CaptureRect?
  public var valueShape: CaptureAXValueShape?
  public var selection: CaptureAXSelectionContext?
  public var isSensitive: Bool
  public var redactionReasons: [String]

  public init(
    role: String? = nil,
    subrole: String? = nil,
    title: String? = nil,
    identifier: String? = nil,
    elementDescription: String? = nil,
    frame: CaptureRect? = nil,
    valueShape: CaptureAXValueShape? = nil,
    selection: CaptureAXSelectionContext? = nil,
    isSensitive: Bool = false,
    redactionReasons: [String] = []
  ) {
    self.role = role
    self.subrole = subrole
    self.title = title
    self.identifier = identifier
    self.elementDescription = elementDescription
    self.frame = frame
    self.valueShape = valueShape
    self.selection = selection
    self.isSensitive = isSensitive
    self.redactionReasons = redactionReasons
  }
}

public struct CaptureAXFocusedContext: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var generatedAt: String
  public var status: CaptureAXFocusedContextStatus
  public var isProcessTrusted: Bool
  public var activeApplication: CaptureActiveApplication?
  public var focusedApplicationProcessID: Int32?
  public var focusedWindow: CaptureAXNodeContext?
  public var focusedElement: CaptureAXNodeContext?
  public var matchedWindowID: UInt32?
  public var issues: [CaptureAXFocusedContextIssue]

  public init(
    schemaVersion: Int = 1,
    generatedAt: String,
    status: CaptureAXFocusedContextStatus,
    isProcessTrusted: Bool,
    activeApplication: CaptureActiveApplication? = nil,
    focusedApplicationProcessID: Int32? = nil,
    focusedWindow: CaptureAXNodeContext? = nil,
    focusedElement: CaptureAXNodeContext? = nil,
    matchedWindowID: UInt32? = nil,
    issues: [CaptureAXFocusedContextIssue] = []
  ) {
    self.schemaVersion = schemaVersion
    self.generatedAt = generatedAt
    self.status = status
    self.isProcessTrusted = isProcessTrusted
    self.activeApplication = activeApplication
    self.focusedApplicationProcessID = focusedApplicationProcessID
    self.focusedWindow = focusedWindow
    self.focusedElement = focusedElement
    self.matchedWindowID = matchedWindowID
    self.issues = issues
  }
}

public enum CaptureEventDurability: String, Codable, Sendable {
  case lossless
  case bestEffort = "best_effort"
}

public struct CaptureEventEnvelope<Payload: Codable & Sendable>: Codable, Sendable {
  public var schemaVersion: Int
  public var eventType: String
  public var durability: CaptureEventDurability
  public var recordedAt: String
  public var payload: Payload

  public init(
    schemaVersion: Int = 1,
    eventType: String,
    durability: CaptureEventDurability,
    recordedAt: String,
    payload: Payload
  ) {
    self.schemaVersion = schemaVersion
    self.eventType = eventType
    self.durability = durability
    self.recordedAt = recordedAt
    self.payload = payload
  }
}

public enum CaptureMode: String, Codable, Comparable, Sendable {
  case idle
  case watch
  case activeText = "active_text"
  case scrollingText = "scrolling_text"
  case videoMotion = "video_motion"

  public static func < (lhs: CaptureMode, rhs: CaptureMode) -> Bool {
    lhs.priority < rhs.priority
  }

  public var priority: Int {
    switch self {
    case .idle: return 0
    case .watch: return 1
    case .activeText: return 2
    case .scrollingText: return 3
    case .videoMotion: return 4
    }
  }

  public var targetCaptureFPS: Int {
    switch self {
    case .idle: return 1
    case .watch: return 3
    case .activeText: return 10
    case .scrollingText: return 30
    case .videoMotion: return 1
    }
  }
}

public struct MotionFeatures: Codable, Equatable, Sendable {
  public var dirtyAreaRatio: Double
  public var dirtyRectCount: Int
  public var meanPixelDiff: Double
  public var changedTileRatio: Double
  public var estimatedDY: Double
  public var scrollEventRecently: Bool
  public var keyboardEventRecently: Bool
  public var ocrNewLineRate: Double
  public var focused: Bool

  public init(
    dirtyAreaRatio: Double,
    dirtyRectCount: Int,
    meanPixelDiff: Double,
    changedTileRatio: Double,
    estimatedDY: Double,
    scrollEventRecently: Bool,
    keyboardEventRecently: Bool,
    ocrNewLineRate: Double,
    focused: Bool
  ) {
    self.dirtyAreaRatio = dirtyAreaRatio
    self.dirtyRectCount = dirtyRectCount
    self.meanPixelDiff = meanPixelDiff
    self.changedTileRatio = changedTileRatio
    self.estimatedDY = estimatedDY
    self.scrollEventRecently = scrollEventRecently
    self.keyboardEventRecently = keyboardEventRecently
    self.ocrNewLineRate = ocrNewLineRate
    self.focused = focused
  }

  public static func zero(focused: Bool = false) -> MotionFeatures {
    MotionFeatures(
      dirtyAreaRatio: 0,
      dirtyRectCount: 0,
      meanPixelDiff: 0,
      changedTileRatio: 0,
      estimatedDY: 0,
      scrollEventRecently: false,
      keyboardEventRecently: false,
      ocrNewLineRate: 0,
      focused: focused
    )
  }
}

public enum CaptureFrameStatus: String, Codable, Equatable, Sendable {
  case complete
  case idle
  case blank
  case suspended
  case started
  case stopped
  case unknown

  public init(rawStatusValue: Int?) {
    switch rawStatusValue {
    case 0:
      self = .complete
    case 1:
      self = .idle
    case 2:
      self = .blank
    case 3:
      self = .suspended
    case 4:
      self = .started
    case 5:
      self = .stopped
    default:
      self = .unknown
    }
  }

  public var feedsMotionClassifier: Bool {
    self == .complete || self == .idle
  }
}

public struct CaptureDirtyRectSummary: Codable, Equatable, Sendable {
  public var dirtyRectCount: Int
  public var dirtyAreaRatio: Double
  public var changedTileRatio: Double
  public var unionRect: CaptureRect?
  public var cappedRects: [CaptureRect]
  public var cappedRectLimit: Int
  public var malformedRectCount: Int
  public var weightedCenterY: Double?
  public var estimatedDY: Double

  public init(
    dirtyRectCount: Int,
    dirtyAreaRatio: Double,
    changedTileRatio: Double,
    unionRect: CaptureRect?,
    cappedRects: [CaptureRect],
    cappedRectLimit: Int,
    malformedRectCount: Int = 0,
    weightedCenterY: Double?,
    estimatedDY: Double
  ) {
    self.dirtyRectCount = dirtyRectCount
    self.dirtyAreaRatio = dirtyAreaRatio
    self.changedTileRatio = changedTileRatio
    self.unionRect = unionRect
    self.cappedRects = cappedRects
    self.cappedRectLimit = cappedRectLimit
    self.malformedRectCount = malformedRectCount
    self.weightedCenterY = weightedCenterY
    self.estimatedDY = estimatedDY
  }

  public static func zero(cappedRectLimit: Int = 0, malformedRectCount: Int = 0) -> CaptureDirtyRectSummary {
    CaptureDirtyRectSummary(
      dirtyRectCount: 0,
      dirtyAreaRatio: 0,
      changedTileRatio: 0,
      unionRect: nil,
      cappedRects: [],
      cappedRectLimit: cappedRectLimit,
      malformedRectCount: malformedRectCount,
      weightedCenterY: nil,
      estimatedDY: 0
    )
  }
}

public struct ActiveWindowFrameMetadata: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var streamID: String
  public var sequence: Int
  public var capturedAt: String
  public var target: ActiveWindowMetadataTarget
  public var frameStatus: CaptureFrameStatus
  public var frameStatusRawValue: Int?
  public var attachmentsPresent: Bool
  public var displayTime: UInt64?
  public var contentRect: CaptureRect?
  public var contentScale: Double?
  public var scaleFactor: Double?
  public var dirtyRectSummary: CaptureDirtyRectSummary
  public var motionFeatures: MotionFeatures
  public var uxMotionHints: UXMotionHints?
  public var uxMotionHintsFused: Bool
  public var feedsMotionClassifier: Bool
  public var parseWarnings: [String]

  public init(
    schemaVersion: Int = 1,
    streamID: String,
    sequence: Int,
    capturedAt: String,
    target: ActiveWindowMetadataTarget,
    frameStatus: CaptureFrameStatus,
    frameStatusRawValue: Int?,
    attachmentsPresent: Bool,
    displayTime: UInt64?,
    contentRect: CaptureRect?,
    contentScale: Double?,
    scaleFactor: Double?,
    dirtyRectSummary: CaptureDirtyRectSummary,
    motionFeatures: MotionFeatures,
    uxMotionHints: UXMotionHints? = nil,
    uxMotionHintsFused: Bool = false,
    feedsMotionClassifier: Bool,
    parseWarnings: [String]
  ) {
    self.schemaVersion = schemaVersion
    self.streamID = streamID
    self.sequence = sequence
    self.capturedAt = capturedAt
    self.target = target
    self.frameStatus = frameStatus
    self.frameStatusRawValue = frameStatusRawValue
    self.attachmentsPresent = attachmentsPresent
    self.displayTime = displayTime
    self.contentRect = contentRect
    self.contentScale = contentScale
    self.scaleFactor = scaleFactor
    self.dirtyRectSummary = dirtyRectSummary
    self.motionFeatures = motionFeatures
    self.uxMotionHints = uxMotionHints
    self.uxMotionHintsFused = uxMotionHintsFused
    self.feedsMotionClassifier = feedsMotionClassifier
    self.parseWarnings = parseWarnings
  }
}

public struct ActiveWindowMetadataSample: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var surface: String
  public var streamID: String
  public var startedAt: String
  public var endedAt: String
  public var requestedDurationSeconds: Double
  public var requestedMaxFrames: Int
  public var target: ActiveWindowMetadataTarget
  public var frameCount: Int
  public var completeFrameCount: Int
  public var idleFrameCount: Int
  public var nonCompleteFrameCount: Int
  public var classifierFeedFrameCount: Int
  public var persistedEventCount: Int
  public var persistErrors: [String]
  public var uxMotionHintsFusedFrameCount: Int
  public var latestUXMotionHints: UXMotionHints?
  public var latestFrame: ActiveWindowFrameMetadata?
  public var frames: [ActiveWindowFrameMetadata]

  public init(
    schemaVersion: Int = 1,
    surface: String = "capture_active_window_metadata_sample",
    streamID: String,
    startedAt: String,
    endedAt: String,
    requestedDurationSeconds: Double,
    requestedMaxFrames: Int,
    target: ActiveWindowMetadataTarget,
    frameCount: Int,
    completeFrameCount: Int,
    idleFrameCount: Int,
    nonCompleteFrameCount: Int,
    classifierFeedFrameCount: Int,
    persistedEventCount: Int,
    persistErrors: [String],
    uxMotionHintsFusedFrameCount: Int = 0,
    latestUXMotionHints: UXMotionHints? = nil,
    latestFrame: ActiveWindowFrameMetadata?,
    frames: [ActiveWindowFrameMetadata]
  ) {
    self.schemaVersion = schemaVersion
    self.surface = surface
    self.streamID = streamID
    self.startedAt = startedAt
    self.endedAt = endedAt
    self.requestedDurationSeconds = requestedDurationSeconds
    self.requestedMaxFrames = requestedMaxFrames
    self.target = target
    self.frameCount = frameCount
    self.completeFrameCount = completeFrameCount
    self.idleFrameCount = idleFrameCount
    self.nonCompleteFrameCount = nonCompleteFrameCount
    self.classifierFeedFrameCount = classifierFeedFrameCount
    self.persistedEventCount = persistedEventCount
    self.persistErrors = persistErrors
    self.uxMotionHintsFusedFrameCount = uxMotionHintsFusedFrameCount
    self.latestUXMotionHints = latestUXMotionHints
    self.latestFrame = latestFrame
    self.frames = frames
  }
}

public struct CapturePolicyDecision: Equatable, Sendable {
  public var mode: CaptureMode
  public var targetCaptureFPS: Int
  public var targetAnalysisFPS: Int
  public var shouldStoreKeyframe: Bool
  public var shouldOCRDirtyRegions: Bool
  public var shouldEncodeVideoSegment: Bool

  public init(
    mode: CaptureMode,
    targetCaptureFPS: Int,
    targetAnalysisFPS: Int,
    shouldStoreKeyframe: Bool,
    shouldOCRDirtyRegions: Bool,
    shouldEncodeVideoSegment: Bool
  ) {
    self.mode = mode
    self.targetCaptureFPS = targetCaptureFPS
    self.targetAnalysisFPS = targetAnalysisFPS
    self.shouldStoreKeyframe = shouldStoreKeyframe
    self.shouldOCRDirtyRegions = shouldOCRDirtyRegions
    self.shouldEncodeVideoSegment = shouldEncodeVideoSegment
  }
}
