import AppKit
import ApplicationServices
import CoreGraphics
import Foundation
import ScreenCaptureKit

private let screenCaptureKitWindowTimeoutNanoseconds: UInt64 = 1_000_000_000
private let systemWindowAppNames: Set<String> = [
  "Window Server",
  "Dock",
  "Control Center",
  "Notification Center",
  "SystemUIServer"
]
private let systemWindowBundleIDs: Set<String> = [
  "com.apple.WindowServer",
  "com.apple.dock",
  "com.apple.controlcenter",
  "com.apple.notificationcenterui",
  "com.apple.systemuiserver"
]

public struct OneContextWindowIndexer {
  private let clock: @Sendable () -> Date
  private let uxMotionHintsProvider: @Sendable () -> UXMotionHints?

  public init(
    clock: @escaping @Sendable () -> Date = Date.init,
    uxMotionHintsProvider: @escaping @Sendable () -> UXMotionHints? = { nil }
  ) {
    self.clock = clock
    self.uxMotionHintsProvider = uxMotionHintsProvider
  }

  public func snapshot() async -> CaptureSnapshot {
    let isoFormatter = ISO8601DateFormatter()
    isoFormatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    let generatedAt = isoFormatter.string(from: clock())
    let prelude = await Self.mainThreadWindowPrelude(generatedAt: generatedAt)
    async let focusedContextResult = Self.axFocusedContext(
      generatedAt: generatedAt,
      activeApplication: prelude.activeApplication
    )
    let sckWindows = await Self.screenCaptureKitWindows(
      generatedAt: generatedAt,
      activePID: prelude.activeApplication?.processID
    )
    var focusedContext: CaptureAXFocusedContext? = await focusedContextResult
    var merged = Self.merge(cgWindows: prelude.cgWindows, sckWindows: sckWindows)
    Self.annotateWindowGeometry(displays: prelude.displays, windows: &merged)
    Self.applyFocusedContext(&focusedContext, to: &merged)
    Self.applyUXMotionHints(uxMotionHintsProvider(), to: &merged)
    let resolvedActive = Self.reconcileActiveWindowMetadata(activeApplication: prelude.activeApplication, windows: &merged)
    Self.annotateFocusedWindowFallback(in: &merged)

    return CaptureSnapshot(
      generatedAt: generatedAt,
      activeApplication: resolvedActive,
      displays: prelude.displays,
      windows: merged.sorted { lhs, rhs in
        if lhs.zRank == rhs.zRank {
          return lhs.windowID < rhs.windowID
        }
        return lhs.zRank < rhs.zRank
      },
      focusedContext: focusedContext
    )
  }

  @MainActor
  private static func mainThreadWindowPrelude(generatedAt: String) -> WindowSnapshotPrelude {
    let active = activeApplication()
    return WindowSnapshotPrelude(
      activeApplication: active,
      displays: displayStates(),
      cgWindows: coreGraphicsWindows(generatedAt: generatedAt, activePID: active?.processID)
    )
  }

  private static func activeApplication() -> CaptureActiveApplication? {
    guard let app = NSWorkspace.shared.frontmostApplication else { return nil }
    return CaptureActiveApplication(
      processID: app.processIdentifier,
      bundleID: app.bundleIdentifier,
      appName: app.localizedName ?? app.bundleIdentifier ?? "Unknown"
    )
  }

  private static func displayStates() -> [CaptureDisplayState] {
    NSScreen.screens.enumerated().map { index, screen in
      let displayID = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")]
        .flatMap { ($0 as? NSNumber)?.stringValue }
        ?? "screen-\(index)"
      return CaptureDisplayState(
        displayID: displayID,
        framePoints: screen.frame.captureRect,
        scaleFactor: screen.backingScaleFactor,
        isMain: screen == NSScreen.main
      )
    }
  }

  private static func coreGraphicsWindows(generatedAt: String, activePID: Int32?) -> [CaptureWindowState] {
    guard let rawList = CGWindowListCopyWindowInfo([.optionAll], kCGNullWindowID) as? [[String: Any]] else {
      return []
    }

    var bundleIDsByPID: [Int32: String] = [:]
    var pidsWithoutBundleID = Set<Int32>()
    var windows: [CaptureWindowState] = []
    windows.reserveCapacity(rawList.count)
    for (zRank, info) in rawList.enumerated() {
      guard let windowIDNumber = info[kCGWindowNumber as String] as? NSNumber else { continue }
      let windowID = windowIDNumber.uint32Value
      let pid = (info[kCGWindowOwnerPID as String] as? NSNumber)?.int32Value ?? 0
      let ownerName = info[kCGWindowOwnerName as String] as? String ?? "Unknown"
      let title = info[kCGWindowName as String] as? String ?? ""
      let layer = (info[kCGWindowLayer as String] as? NSNumber)?.intValue ?? 0
      let alpha = (info[kCGWindowAlpha as String] as? NSNumber)?.doubleValue
      let isOnScreen = (info[kCGWindowIsOnscreen as String] as? NSNumber)?.boolValue ?? false
      let bounds = (info[kCGWindowBounds as String] as? NSDictionary)
        .flatMap { CGRect(dictionaryRepresentation: $0 as CFDictionary) }
        ?? .zero
      let bundleID: String?
      if let cached = bundleIDsByPID[pid] {
        bundleID = cached
      } else if pidsWithoutBundleID.contains(pid) {
        bundleID = nil
      } else {
        let resolvedBundleID = Self.bundleIdentifier(for: pid)
        if let resolvedBundleID {
          bundleIDsByPID[pid] = resolvedBundleID
        } else {
          pidsWithoutBundleID.insert(pid)
        }
        bundleID = resolvedBundleID
      }

      windows.append(CaptureWindowState(
        time: generatedAt,
        windowID: windowID,
        appPID: pid,
        appName: ownerName,
        bundleID: bundleID,
        title: title,
        framePoints: bounds.captureRect,
        zRank: zRank,
        layer: layer,
        alpha: alpha,
        isFocused: false,
        isOnScreen: isOnScreen,
        isMinimized: !isOnScreen,
        visibleFractionEstimate: isOnScreen ? 1 : 0,
        captureEligible: isOnScreen && layer == 0 && bounds.width > 1 && bounds.height > 1,
        source: "coregraphics"
      ))
    }

    Self.promoteFrontmostWindowFallback(in: &windows, activePID: activePID)
    _ = Self.reconcileActiveWindowMetadata(activeApplication: nil, windows: &windows)
    return windows
  }

  private static func screenCaptureKitWindows(generatedAt: String, activePID: Int32?) async -> [CaptureWindowState] {
    await withTaskGroup(of: [CaptureWindowState].self) { group in
      group.addTask {
        await Self.screenCaptureKitWindowsUnbounded(generatedAt: generatedAt, activePID: activePID)
      }
      group.addTask {
        try? await Task.sleep(nanoseconds: screenCaptureKitWindowTimeoutNanoseconds)
        return []
      }

      let windows = await group.next() ?? []
      group.cancelAll()
      return windows
    }
  }

  private static func screenCaptureKitWindowsUnbounded(generatedAt: String, activePID: Int32?) async -> [CaptureWindowState] {
    do {
      let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
      var windows = content.windows.enumerated().map { index, window in
        let app = window.owningApplication
        let pid = app?.processID ?? 0
        let bundleID = app?.bundleIdentifier
        let appName = app?.applicationName ?? bundleID ?? "Unknown"
        return CaptureWindowState(
          time: generatedAt,
          windowID: window.windowID,
          appPID: pid,
          appName: appName,
          bundleID: bundleID,
          title: window.title ?? "",
          framePoints: window.frame.captureRect,
          zRank: index,
          layer: window.windowLayer,
          isFocused: false,
          isOnScreen: window.isOnScreen,
          isMinimized: !window.isOnScreen,
          visibleFractionEstimate: window.isOnScreen ? 1 : 0,
          captureEligible: window.isOnScreen && window.frame.width > 1 && window.frame.height > 1,
          source: "screencapturekit"
        )
      }
      Self.promoteFrontmostWindowFallback(in: &windows, activePID: activePID)
      _ = Self.reconcileActiveWindowMetadata(activeApplication: nil, windows: &windows)
      return windows
    } catch {
      return []
    }
  }

  private static func merge(cgWindows: [CaptureWindowState], sckWindows: [CaptureWindowState]) -> [CaptureWindowState] {
    var mergedByID: [UInt32: CaptureWindowState] = [:]
    mergedByID.reserveCapacity(cgWindows.count + sckWindows.count)
    for cgWindow in cgWindows {
      mergedByID[cgWindow.windowID] = cgWindow
    }

    for sckWindow in sckWindows {
      guard var existing = mergedByID[sckWindow.windowID] else {
        mergedByID[sckWindow.windowID] = sckWindow
        continue
      }

      existing.bundleID = existing.bundleID ?? sckWindow.bundleID
      if existing.title.isEmpty {
        existing.title = sckWindow.title
      }
      existing.framePixels = sckWindow.framePixels
      existing.isOnScreen = existing.isOnScreen || sckWindow.isOnScreen
      existing.isMinimized = sckWindow.isMinimized
      existing.captureEligible = existing.captureEligible || sckWindow.captureEligible
      existing.isFocused = existing.isFocused || sckWindow.isFocused
      existing.focusMetadata = existing.focusMetadata ?? sckWindow.focusMetadata
      existing.source = "coregraphics+screencapturekit"
      mergedByID[sckWindow.windowID] = existing
    }

    return Array(mergedByID.values)
  }

  static func annotateWindowGeometry(displays: [CaptureDisplayState], windows: inout [CaptureWindowState]) {
    let originalWindows = windows
    for index in windows.indices {
      let windowRect = windows[index].framePoints.cgRect.standardized
      guard validArea(windowRect) > 0 else {
        windows[index].displayID = nil
        windows[index].framePixels = nil
        windows[index].visibleFractionEstimate = 0
        continue
      }

      if let match = bestDisplayMatch(for: windowRect, displays: displays) {
        windows[index].displayID = match.display.displayID
        windows[index].framePixels = displayPixelRect(
          for: windowRect,
          displayFrame: match.display.framePoints.cgRect.standardized,
          scaleFactor: match.display.scaleFactor
        )
      } else {
        windows[index].displayID = nil
        windows[index].framePixels = nil
      }

      windows[index].visibleFractionEstimate = visibleFractionEstimate(
        for: windows[index],
        displays: displays,
        windows: originalWindows
      )
    }
  }

  private static func bestDisplayMatch(
    for windowRect: CGRect,
    displays: [CaptureDisplayState]
  ) -> DisplayIntersection? {
    var bestMatch: DisplayIntersection?
    for display in displays {
      let displayRect = display.framePoints.cgRect.standardized
      let intersection = positiveIntersection(windowRect, displayRect)
      let area = intersection.map(validArea) ?? 0
      guard area > 0 else {
        continue
      }

      let match = DisplayIntersection(display: display, intersectionArea: area)
      if let current = bestMatch {
        if match.intersectionArea > current.intersectionArea
          || (match.intersectionArea == current.intersectionArea && match.display.isMain && !current.display.isMain)
          || (match.intersectionArea == current.intersectionArea
            && match.display.isMain == current.display.isMain
            && match.display.displayID < current.display.displayID)
        {
          bestMatch = match
        }
      } else {
        bestMatch = match
      }
    }
    return bestMatch
  }

  private static func displayPixelRect(
    for windowRect: CGRect,
    displayFrame: CGRect,
    scaleFactor: Double
  ) -> CaptureRect {
    let scale = max(scaleFactor, 0)
    return CaptureRect(
      x: (windowRect.minX - displayFrame.minX) * scale,
      y: (windowRect.minY - displayFrame.minY) * scale,
      width: windowRect.width * scale,
      height: windowRect.height * scale
    )
  }

  private static func visibleFractionEstimate(
    for target: CaptureWindowState,
    displays: [CaptureDisplayState],
    windows: [CaptureWindowState]
  ) -> Double {
    guard target.isOnScreen, !target.isMinimized else {
      return 0
    }

    let targetRect = target.framePoints.cgRect.standardized
    let targetArea = validArea(targetRect)
    guard targetArea > 0 else {
      return 0
    }

    var visibleRects = displays.compactMap { display -> CGRect? in
      positiveIntersection(targetRect, display.framePoints.cgRect.standardized)
    }
    guard !visibleRects.isEmpty else {
      return 0
    }

    if isWindowGeometryOcclusionTarget(target) {
      let occluders = windows
        .filter { occluder in
          occluder.windowID != target.windowID
            && zOrderPrecedes(occluder, target)
            && isWindowGeometryOccluder(occluder)
        }
        .sorted(by: zOrderPrecedes)

      for occluder in occluders {
        let occluderRect = occluder.framePoints.cgRect.standardized
        visibleRects = visibleRects.flatMap { subtract(occluderRect, from: $0) }
        if visibleRects.isEmpty {
          break
        }
      }
    }

    let visibleArea = visibleRects.reduce(0) { $0 + validArea($1) }
    return min(1, max(0, visibleArea / targetArea))
  }

  private static func isWindowGeometryOcclusionTarget(_ window: CaptureWindowState) -> Bool {
    window.isOnScreen
      && !window.isMinimized
      && window.layer == 0
      && (window.alpha.map { $0 > 0.01 } ?? true)
      && window.framePoints.width > 1
      && window.framePoints.height > 1
      && !isSystemWindow(window)
  }

  private static func isWindowGeometryOccluder(_ window: CaptureWindowState) -> Bool {
    isWindowGeometryOcclusionTarget(window)
  }

  private static func positiveIntersection(_ lhs: CGRect, _ rhs: CGRect) -> CGRect? {
    let intersection = lhs.intersection(rhs).standardized
    guard !intersection.isNull, !intersection.isInfinite, validArea(intersection) > 0 else {
      return nil
    }
    return intersection
  }

  private static func subtract(_ occluder: CGRect, from rect: CGRect) -> [CGRect] {
    guard let intersection = positiveIntersection(rect, occluder) else {
      return [rect]
    }

    let candidates = [
      CGRect(
        x: rect.minX,
        y: rect.minY,
        width: intersection.minX - rect.minX,
        height: rect.height
      ),
      CGRect(
        x: intersection.maxX,
        y: rect.minY,
        width: rect.maxX - intersection.maxX,
        height: rect.height
      ),
      CGRect(
        x: intersection.minX,
        y: rect.minY,
        width: intersection.width,
        height: intersection.minY - rect.minY
      ),
      CGRect(
        x: intersection.minX,
        y: intersection.maxY,
        width: intersection.width,
        height: rect.maxY - intersection.maxY
      )
    ]

    return candidates.filter { validArea($0) > 0 }
  }

  private static func validArea(_ rect: CGRect) -> Double {
    guard !rect.isNull, !rect.isInfinite, rect.width > 0, rect.height > 0 else {
      return 0
    }
    return rect.width * rect.height
  }

  private struct DisplayIntersection {
    var display: CaptureDisplayState
    var intersectionArea: Double
  }

  private static func promoteFrontmostWindowFallback(in windows: inout [CaptureWindowState], activePID: Int32?) {
    guard let activePID else { return }
    var fallbackIndex: Int?
    for index in windows.indices {
      if windows[index].isFocused {
        return
      }
      guard fallbackIndex == nil else { continue }
      let window = windows[index]
      if window.appPID == activePID
        && window.isOnScreen
        && window.layer == 0
        && window.framePoints.width > 1
        && window.framePoints.height > 1
      {
        fallbackIndex = index
      }
    }
    guard let fallbackIndex else {
      return
    }
    windows[fallbackIndex].isFocused = true
  }

  static func reconcileActiveWindowMetadata(
    activeApplication: CaptureActiveApplication?,
    windows: inout [CaptureWindowState]
  ) -> CaptureActiveApplication? {
    promoteFrontmostWindowFallback(in: &windows, activePID: activeApplication?.processID)
    keepSingleFocusedWindow(in: &windows, activePID: activeApplication?.processID)
    return resolvedActiveApplication(activeApplication, windows: windows)
  }

  private static func keepSingleFocusedWindow(in windows: inout [CaptureWindowState], activePID: Int32?) {
    var focusedCount = 0
    var bestIndex: Int?
    var bestPriority = Int.min

    for index in windows.indices where windows[index].isFocused {
      focusedCount += 1
      let priority = focusedPriority(for: windows[index], activePID: activePID)
      if bestIndex == nil
        || priority > bestPriority
        || (priority == bestPriority && zOrderPrecedes(windows[index], windows[bestIndex!]))
      {
        bestIndex = index
        bestPriority = priority
      }
    }

    guard focusedCount > 1, let bestIndex else {
      return
    }

    for index in windows.indices {
      windows[index].isFocused = index == bestIndex
    }
  }

  private static func focusedPriority(for window: CaptureWindowState, activePID: Int32?) -> Int {
    if window.focusMetadata?.source == "ax_focused_context" {
      return 40
    }
    if window.focusMetadata?.source == "ax_focused_application" {
      return 35
    }
    if let activePID, window.appPID == activePID {
      return 30
    }
    if isFocusableContentWindow(window) {
      return 20
    }
    return 10
  }

  private static func resolvedActiveApplication(
    _ active: CaptureActiveApplication?,
    windows: [CaptureWindowState]
  ) -> CaptureActiveApplication? {
    var focusedIndex: Int?
    for index in windows.indices {
      guard windows[index].isFocused, isFocusableContentWindow(windows[index]) else {
        continue
      }
      if focusedIndex == nil || zOrderPrecedes(windows[index], windows[focusedIndex!]) {
        focusedIndex = index
      }
    }

    guard let focusedIndex else {
      return active
    }
    let focused = windows[focusedIndex]

    guard active?.processID != focused.appPID else {
      return active
    }

    return CaptureActiveApplication(
      processID: focused.appPID,
      bundleID: focused.bundleID,
      appName: focused.appName
    )
  }

  private static func zOrderPrecedes(_ lhs: CaptureWindowState, _ rhs: CaptureWindowState) -> Bool {
    if lhs.zRank == rhs.zRank {
      return lhs.windowID < rhs.windowID
    }
    return lhs.zRank < rhs.zRank
  }

  private static func isFocusableContentWindow(_ window: CaptureWindowState) -> Bool {
    window.captureEligible
      && window.isOnScreen
      && !window.isMinimized
      && window.layer == 0
      && (window.alpha.map { $0 > 0 } ?? true)
      && window.framePoints.width >= 120
      && window.framePoints.height >= 80
      && !isDashboardWindow(window)
      && !isSystemWindow(window)
  }

  private static func isDashboardWindow(_ window: CaptureWindowState) -> Bool {
    window.appName.caseInsensitiveCompare("onecontext-capture-dashboard") == .orderedSame
      || window.title.contains("1Context Capture Dashboard")
  }

  private static func isSystemWindow(_ window: CaptureWindowState) -> Bool {
    systemWindowAppNames.contains(window.appName)
      || window.bundleID.map { systemWindowBundleIDs.contains($0) } == true
  }

  private static func bundleIdentifier(for pid: Int32) -> String? {
    NSRunningApplication(processIdentifier: pid)?.bundleIdentifier
  }

  private struct WindowSnapshotPrelude: Sendable {
    var activeApplication: CaptureActiveApplication?
    var displays: [CaptureDisplayState]
    var cgWindows: [CaptureWindowState]
  }

  private static func axFocusedContext(
    generatedAt: String,
    activeApplication: CaptureActiveApplication?
  ) async -> CaptureAXFocusedContext {
    AXFocusedContextReader(client: SystemAXFocusedContextClient())
      .read(generatedAt: generatedAt, activeApplication: activeApplication)
  }

  static func applyFocusedContext(
    _ focusedContext: inout CaptureAXFocusedContext?,
    to windows: inout [CaptureWindowState]
  ) {
    guard var context = focusedContext, context.isProcessTrusted else {
      return
    }

    guard let match = focusedContextWindowMatch(context: context, windows: windows)
      ?? focusedApplicationWindowMatch(context: context, windows: windows)
    else {
      return
    }

    clearFocus(in: &windows)
    windows[match.index].isFocused = true
    windows[match.index].focusMetadata = CaptureWindowFocusMetadata(
      source: match.source,
      status: "matched",
      confidence: match.confidence,
      matchedWindowID: windows[match.index].windowID,
      matchSignals: match.signals
    )
    context.matchedWindowID = windows[match.index].windowID
    focusedContext = context
  }

  private static func annotateFocusedWindowFallback(in windows: inout [CaptureWindowState]) {
    for index in windows.indices where windows[index].isFocused && windows[index].focusMetadata == nil {
      windows[index].focusMetadata = CaptureWindowFocusMetadata(
        source: "active_application",
        status: "fallback",
        confidence: "medium",
        matchedWindowID: windows[index].windowID,
        matchSignals: ["active_pid", "frontmost_app"]
      )
    }
  }

  static func applyUXMotionHints(_ hints: UXMotionHints?, to windows: inout [CaptureWindowState]) {
    guard let hints,
      hints.focusedRecently,
      let targetPID = hints.recentTargetProcessID,
      targetPID > 0
    else {
      return
    }
    var targetIndex: Int?
    for index in windows.indices {
      guard windows[index].appPID == targetPID, candidateCanReceiveUXFocus(windows[index]) else {
        continue
      }
      if targetIndex == nil || zOrderPrecedes(windows[index], windows[targetIndex!]) {
        targetIndex = index
      }
    }

    guard let targetIndex else {
      return
    }

    clearFocus(in: &windows)
    var signals = ["event_target_pid"]
    if hints.keyboardActivityRecently {
      signals.append("keyboard_activity_recent")
    }
    windows[targetIndex].isFocused = true
    windows[targetIndex].focusMetadata = CaptureWindowFocusMetadata(
      source: "ux_event_tap_target",
      status: "matched",
      confidence: hints.keyboardActivityRecently ? "high" : "medium",
      matchedWindowID: windows[targetIndex].windowID,
      matchSignals: signals
    )
  }

  private static func candidateCanReceiveUXFocus(_ window: CaptureWindowState) -> Bool {
    window.captureEligible
      && window.isOnScreen
      && !window.isMinimized
      && window.layer == 0
      && (window.alpha.map { $0 > 0 } ?? true)
      && window.framePoints.width >= 120
      && window.framePoints.height >= 80
      && !isSystemWindow(window)
  }

  private static func clearFocus(in windows: inout [CaptureWindowState]) {
    for index in windows.indices {
      windows[index].isFocused = false
      windows[index].focusMetadata = nil
    }
  }

  private static func focusedContextWindowMatch(
    context: CaptureAXFocusedContext,
    windows: [CaptureWindowState]
  ) -> FocusedContextWindowMatch? {
    guard let focusedWindow = context.focusedWindow else {
      return nil
    }

    let focusedPID = context.focusedApplicationProcessID ?? context.activeApplication?.processID
    let focusedTitle = focusedWindow.title?.trimmingCharacters(in: .whitespacesAndNewlines)
    let focusedFrame = focusedWindow.frame?.cgRect

    var bestMatch: FocusedContextWindowMatch?
    for index in windows.indices {
      let window = windows[index]
      guard isFocusableContentWindow(window) else {
        continue
      }
      if let focusedPID, window.appPID != focusedPID {
        continue
      }

      var score = 0
      var signals: [String] = []
      if focusedPID != nil {
        score += 4
        signals.append("pid")
      }
      if let focusedTitle,
        !focusedTitle.isEmpty,
        !window.title.isEmpty,
        focusedTitle == window.title
      {
        score += 3
        signals.append("title")
      }
      if let focusedFrame, framesLikelyMatch(focusedFrame, window.framePoints.cgRect) {
        score += 4
        signals.append("bounds")
      }

      guard score >= 7 || (focusedFrame != nil && score >= 4) else {
        continue
      }

      let confidence: String
      if signals.contains("bounds") && signals.contains("title") {
        confidence = "high"
      } else if signals.contains("bounds") || signals.contains("title") {
        confidence = "medium"
      } else {
        confidence = "low"
      }
      let match = FocusedContextWindowMatch(
        index: index,
        score: score,
        signals: signals,
        confidence: confidence,
        source: "ax_focused_context"
      )

      if let currentBest = bestMatch {
        if match.score > currentBest.score
          || (match.score == currentBest.score && zOrderPrecedes(windows[match.index], windows[currentBest.index]))
        {
          bestMatch = match
        }
      } else {
        bestMatch = match
      }
    }
    return bestMatch
  }

  private static func focusedApplicationWindowMatch(
    context: CaptureAXFocusedContext,
    windows: [CaptureWindowState]
  ) -> FocusedContextWindowMatch? {
    guard let focusedPID = context.focusedApplicationProcessID ?? context.activeApplication?.processID else {
      return nil
    }
    var focusedIndex: Int?
    for index in windows.indices {
      guard isFocusableContentWindow(windows[index]), windows[index].appPID == focusedPID else {
        continue
      }
      if focusedIndex == nil || zOrderPrecedes(windows[index], windows[focusedIndex!]) {
        focusedIndex = index
      }
    }

    guard let focusedIndex else {
      return nil
    }

    return FocusedContextWindowMatch(
      index: focusedIndex,
      score: 4,
      signals: ["pid"],
      confidence: "medium",
      source: "ax_focused_application"
    )
  }

  private static func framesLikelyMatch(_ lhs: CGRect, _ rhs: CGRect) -> Bool {
    abs(lhs.origin.x - rhs.origin.x) <= 2
      && abs(lhs.origin.y - rhs.origin.y) <= 2
      && abs(lhs.width - rhs.width) <= 4
      && abs(lhs.height - rhs.height) <= 4
  }

  private struct FocusedContextWindowMatch {
    var index: Int
    var score: Int
    var signals: [String]
    var confidence: String
    var source: String
  }
}

extension CGRect {
  var captureRect: CaptureRect {
    CaptureRect(x: origin.x, y: origin.y, width: width, height: height)
  }
}

private extension CaptureRect {
  var cgRect: CGRect {
    CGRect(x: x, y: y, width: width, height: height)
  }
}
