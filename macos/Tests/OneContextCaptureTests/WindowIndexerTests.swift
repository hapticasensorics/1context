import XCTest
@testable import OneContextCapture

final class WindowIndexerTests: XCTestCase {
  func testActiveApplicationFallbackWinsOverTopZRankWhenFocusIsMissing() {
    var windows = [
      window(windowID: 20, appPID: 20, appName: "Google Chrome", bundleID: "com.google.Chrome", zRank: 0),
      window(windowID: 10, appPID: 10, appName: "Terminal", bundleID: "com.apple.Terminal", zRank: 1)
    ]
    let activeApplication = CaptureActiveApplication(
      processID: 10,
      bundleID: "com.apple.Terminal",
      appName: "Terminal"
    )

    let resolvedActive = OneContextWindowIndexer.reconcileActiveWindowMetadata(
      activeApplication: activeApplication,
      windows: &windows
    )

    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [10])
    XCTAssertEqual(resolvedActive?.processID, 10)
    XCTAssertEqual(resolvedActive?.bundleID, "com.apple.Terminal")
    XCTAssertEqual(resolvedActive?.appName, "Terminal")
  }

  func testFocusedSignalBehindTopZRankIsPreserved() {
    var windows = [
      window(windowID: 20, appPID: 20, appName: "Google Chrome", bundleID: "com.google.Chrome", zRank: 0),
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        zRank: 1,
        isFocused: true
      )
    ]
    let activeApplication = CaptureActiveApplication(
      processID: 10,
      bundleID: "com.apple.Terminal",
      appName: "Terminal"
    )

    let resolvedActive = OneContextWindowIndexer.reconcileActiveWindowMetadata(
      activeApplication: activeApplication,
      windows: &windows
    )

    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [10])
    XCTAssertEqual(resolvedActive?.processID, 10)
  }

  func testReconciliationLeavesOnlyOneFocusedContentWindow() {
    var windows = [
      window(
        windowID: 20,
        appPID: 20,
        appName: "Google Chrome",
        bundleID: "com.google.Chrome",
        zRank: 0,
        isFocused: true
      ),
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        zRank: 1,
        isFocused: true
      )
    ]

    let resolvedActive = OneContextWindowIndexer.reconcileActiveWindowMetadata(
      activeApplication: nil,
      windows: &windows
    )

    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [20])
    XCTAssertEqual(resolvedActive?.processID, 20)
  }

  func testFocusedActivePIDPriorityBeatsFrontmostFocusedWindow() {
    var windows = [
      window(
        windowID: 20,
        appPID: 20,
        appName: "Google Chrome",
        bundleID: "com.google.Chrome",
        zRank: 0,
        isFocused: true
      ),
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        zRank: 5,
        isFocused: true
      )
    ]
    let activeApplication = CaptureActiveApplication(
      processID: 10,
      bundleID: "com.apple.Terminal",
      appName: "Terminal"
    )

    let resolvedActive = OneContextWindowIndexer.reconcileActiveWindowMetadata(
      activeApplication: activeApplication,
      windows: &windows
    )

    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [10])
    XCTAssertEqual(resolvedActive?.processID, 10)
  }

  func testDashboardFocusIsNotRewrittenToTopCaptureCandidate() {
    var windows = [
      window(
        windowID: 30,
        appPID: 30,
        appName: "onecontext-capture-dashboard",
        bundleID: "com.haptica.1context.capture-dashboard",
        title: "1Context Capture Dashboard",
        zRank: 0,
        isFocused: true
      ),
      window(windowID: 20, appPID: 20, appName: "Google Chrome", bundleID: "com.google.Chrome", zRank: 1)
    ]
    let dashboardActive = CaptureActiveApplication(
      processID: 30,
      bundleID: "com.haptica.1context.capture-dashboard",
      appName: "onecontext-capture-dashboard"
    )

    let resolvedActive = OneContextWindowIndexer.reconcileActiveWindowMetadata(
      activeApplication: dashboardActive,
      windows: &windows
    )

    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [30])
    XCTAssertEqual(resolvedActive?.processID, 30)
  }

  func testAXFocusedContextAnnotatesMatchingWindowWithoutReplacingItByZRank() {
    var windows = [
      window(windowID: 10, appPID: 99, appName: "Google Chrome", bundleID: "com.google.Chrome", title: "Docs", zRank: 0),
      window(windowID: 20, appPID: 42, appName: "Example", bundleID: "com.example.App", title: "Editor", zRank: 1)
    ]
    var focusedContext: CaptureAXFocusedContext? = CaptureAXFocusedContext(
      generatedAt: "2026-05-24T10:11:12.123Z",
      status: .available,
      isProcessTrusted: true,
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example"),
      focusedApplicationProcessID: 42,
      focusedWindow: CaptureAXNodeContext(
        role: "AXWindow",
        title: "Editor",
        frame: CaptureRect(x: 0, y: 0, width: 1280, height: 800)
      )
    )

    OneContextWindowIndexer.applyFocusedContext(&focusedContext, to: &windows)
    let resolvedActive = OneContextWindowIndexer.reconcileActiveWindowMetadata(activeApplication: nil, windows: &windows)

    XCTAssertEqual(focusedContext?.matchedWindowID, 20)
    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [20])
    XCTAssertEqual(resolvedActive?.processID, 42)
    XCTAssertEqual(windows.last?.focusMetadata?.source, "ax_focused_context")
    XCTAssertEqual(windows.last?.focusMetadata?.matchSignals, ["pid", "title", "bounds"])
  }

  func testAXFocusedContextTieBreaksByZOrder() {
    var windows = [
      window(windowID: 10, appPID: 42, appName: "Example", bundleID: "com.example.App", title: "Editor", zRank: 5),
      window(windowID: 20, appPID: 42, appName: "Example", bundleID: "com.example.App", title: "Editor", zRank: 1)
    ]
    var focusedContext: CaptureAXFocusedContext? = CaptureAXFocusedContext(
      generatedAt: "2026-05-24T10:11:12.123Z",
      status: .available,
      isProcessTrusted: true,
      activeApplication: CaptureActiveApplication(processID: 42, bundleID: "com.example.App", appName: "Example"),
      focusedApplicationProcessID: 42,
      focusedWindow: CaptureAXNodeContext(
        role: "AXWindow",
        title: "Editor",
        frame: CaptureRect(x: 0, y: 0, width: 1280, height: 800)
      )
    )

    OneContextWindowIndexer.applyFocusedContext(&focusedContext, to: &windows)

    XCTAssertEqual(focusedContext?.matchedWindowID, 20)
    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [20])
    XCTAssertEqual(windows.last?.focusMetadata?.source, "ax_focused_context")
  }

  func testAXFocusedApplicationPIDFallbackMarksWindowBehindTopZRank() {
    var windows = [
      window(windowID: 10, appPID: 99, appName: "Google Chrome", bundleID: "com.google.Chrome", title: "Docs", zRank: 0),
      window(windowID: 20, appPID: 42, appName: "Codex", bundleID: "com.openai.codex", title: "Codex", zRank: 1)
    ]
    var focusedContext: CaptureAXFocusedContext? = CaptureAXFocusedContext(
      generatedAt: "2026-05-24T10:11:12.123Z",
      status: .noFocusedWindow,
      isProcessTrusted: true,
      activeApplication: CaptureActiveApplication(processID: 99, bundleID: "com.google.Chrome", appName: "Google Chrome"),
      focusedApplicationProcessID: 42
    )

    OneContextWindowIndexer.applyFocusedContext(&focusedContext, to: &windows)
    let resolvedActive = OneContextWindowIndexer.reconcileActiveWindowMetadata(activeApplication: nil, windows: &windows)

    XCTAssertEqual(focusedContext?.matchedWindowID, 20)
    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [20])
    XCTAssertEqual(resolvedActive?.processID, 42)
    XCTAssertEqual(windows.last?.focusMetadata?.source, "ax_focused_application")
    XCTAssertEqual(windows.last?.focusMetadata?.matchSignals, ["pid"])
  }

  func testUXEventTargetPIDOverridesStaleFocusSource() {
    var windows = [
      window(
        windowID: 10,
        appPID: 99,
        appName: "Google Chrome",
        bundleID: "com.google.Chrome",
        title: "Docs",
        zRank: 0,
        isFocused: true,
        focusMetadata: CaptureWindowFocusMetadata(
          source: "ax_focused_context",
          status: "matched",
          confidence: "medium",
          matchedWindowID: 10,
          matchSignals: ["pid"]
        )
      ),
      window(windowID: 20, appPID: 42, appName: "Codex", bundleID: "com.openai.codex", title: "Codex", zRank: 1)
    ]
    let hints = UXMotionHints(
      generatedAt: "2026-05-24T10:11:12.123Z",
      scrollEventRecently: false,
      keyboardActivityRecently: true,
      estimatedScrollDY: 0,
      focusedRecently: true,
      recentTargetProcessID: 42
    )

    OneContextWindowIndexer.applyUXMotionHints(hints, to: &windows)

    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [20])
    XCTAssertEqual(windows.last?.focusMetadata?.source, "ux_event_tap_target")
    XCTAssertEqual(windows.last?.focusMetadata?.confidence, "high")
    XCTAssertEqual(windows.last?.focusMetadata?.matchSignals, ["event_target_pid", "keyboard_activity_recent"])
  }

  func testUXEventTargetUsesFrontmostEligibleNonSystemWindow() {
    var windows = [
      window(
        windowID: 10,
        appPID: 42,
        appName: "SystemUIServer",
        bundleID: "com.apple.systemuiserver",
        title: "Menu Bar",
        zRank: 0
      ),
      window(
        windowID: 20,
        appPID: 42,
        appName: "onecontext-capture-dashboard",
        bundleID: "com.haptica.1context.capture-dashboard",
        title: "1Context Capture Dashboard",
        zRank: 1
      ),
      window(windowID: 30, appPID: 42, appName: "Codex", bundleID: "com.openai.codex", title: "Codex", zRank: 2),
      window(windowID: 40, appPID: 99, appName: "Terminal", bundleID: "com.apple.Terminal", zRank: 3, isFocused: true)
    ]
    let hints = UXMotionHints(
      generatedAt: "2026-05-24T10:11:12.123Z",
      scrollEventRecently: false,
      keyboardActivityRecently: false,
      estimatedScrollDY: 0,
      focusedRecently: true,
      recentTargetProcessID: 42
    )

    OneContextWindowIndexer.applyUXMotionHints(hints, to: &windows)

    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [20])
    XCTAssertEqual(windows[1].focusMetadata?.source, "ux_event_tap_target")
    XCTAssertEqual(windows[1].focusMetadata?.confidence, "medium")
  }

  func testWindowGeometryFullyVisibleAssignsDisplayAndPixels() {
    var windows = [
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        framePoints: CaptureRect(x: 100, y: 50, width: 200, height: 100),
        zRank: 0
      )
    ]

    OneContextWindowIndexer.annotateWindowGeometry(displays: [
      display(displayID: "main", framePoints: CaptureRect(x: 0, y: 0, width: 1000, height: 800), scaleFactor: 2)
    ], windows: &windows)

    XCTAssertEqual(windows[0].displayID, "main")
    XCTAssertEqual(windows[0].framePixels, CaptureRect(x: 200, y: 100, width: 400, height: 200))
    XCTAssertEqual(windows[0].visibleFractionEstimate ?? -1, 1, accuracy: 0.001)
  }

  func testWindowGeometryPartiallyOffscreenClipsToDisplayBounds() {
    var windows = [
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        framePoints: CaptureRect(x: -50, y: 20, width: 100, height: 100),
        zRank: 0
      )
    ]

    OneContextWindowIndexer.annotateWindowGeometry(displays: [
      display(displayID: "main", framePoints: CaptureRect(x: 0, y: 0, width: 1000, height: 800))
    ], windows: &windows)

    XCTAssertEqual(windows[0].displayID, "main")
    XCTAssertEqual(windows[0].framePixels, CaptureRect(x: -50, y: 20, width: 100, height: 100))
    XCTAssertEqual(windows[0].visibleFractionEstimate ?? -1, 0.5, accuracy: 0.001)
  }

  func testWindowGeometryFullyOccludedByHigherZRankWindow() {
    var windows = [
      window(
        windowID: 20,
        appPID: 20,
        appName: "Chrome",
        bundleID: "com.google.Chrome",
        framePoints: CaptureRect(x: 100, y: 100, width: 200, height: 100),
        zRank: 0
      ),
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        framePoints: CaptureRect(x: 100, y: 100, width: 200, height: 100),
        zRank: 1
      )
    ]

    OneContextWindowIndexer.annotateWindowGeometry(displays: [
      display(displayID: "main", framePoints: CaptureRect(x: 0, y: 0, width: 1000, height: 800))
    ], windows: &windows)

    XCTAssertEqual(windows[1].visibleFractionEstimate ?? -1, 0, accuracy: 0.001)
  }

  func testWindowGeometryPartiallyOccludedByHigherZRankWindow() {
    var windows = [
      window(
        windowID: 20,
        appPID: 20,
        appName: "Chrome",
        bundleID: "com.google.Chrome",
        framePoints: CaptureRect(x: 100, y: 100, width: 100, height: 100),
        zRank: 0
      ),
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        framePoints: CaptureRect(x: 100, y: 100, width: 200, height: 100),
        zRank: 1
      )
    ]

    OneContextWindowIndexer.annotateWindowGeometry(displays: [
      display(displayID: "main", framePoints: CaptureRect(x: 0, y: 0, width: 1000, height: 800))
    ], windows: &windows)

    XCTAssertEqual(windows[1].visibleFractionEstimate ?? -1, 0.5, accuracy: 0.001)
  }

  func testWindowGeometryChoosesBestIntersectingDisplayAndScale() {
    var windows = [
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        framePoints: CaptureRect(x: 900, y: 100, width: 300, height: 200),
        zRank: 0
      )
    ]

    OneContextWindowIndexer.annotateWindowGeometry(displays: [
      display(displayID: "main", framePoints: CaptureRect(x: 0, y: 0, width: 1000, height: 800), scaleFactor: 2, isMain: true),
      display(displayID: "external", framePoints: CaptureRect(x: 1000, y: 0, width: 800, height: 600), scaleFactor: 1.5)
    ], windows: &windows)

    XCTAssertEqual(windows[0].displayID, "external")
    XCTAssertEqual(windows[0].framePixels, CaptureRect(x: -150, y: 150, width: 450, height: 300))
    XCTAssertEqual(windows[0].visibleFractionEstimate ?? -1, 1, accuracy: 0.001)
  }

  func testWindowGeometryMinimizedWindowWithoutDisplayHasNoVisibleArea() {
    var windows = [
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        framePoints: CaptureRect(x: 2000, y: 2000, width: 200, height: 100),
        zRank: 0,
        isOnScreen: false,
        isMinimized: true
      )
    ]

    OneContextWindowIndexer.annotateWindowGeometry(displays: [], windows: &windows)

    XCTAssertNil(windows[0].displayID)
    XCTAssertNil(windows[0].framePixels)
    XCTAssertEqual(windows[0].visibleFractionEstimate ?? -1, 0, accuracy: 0.001)
  }

  func testWindowGeometryIgnoresTransparentSystemMinimizedAndOffscreenOccluders() {
    var windows = [
      window(
        windowID: 50,
        appPID: 50,
        appName: "Overlay",
        bundleID: "com.example.overlay",
        framePoints: CaptureRect(x: 100, y: 100, width: 200, height: 100),
        zRank: 0,
        alpha: 0
      ),
      window(
        windowID: 40,
        appPID: 40,
        appName: "SystemUIServer",
        bundleID: "com.apple.systemuiserver",
        framePoints: CaptureRect(x: 100, y: 100, width: 200, height: 100),
        zRank: 1
      ),
      window(
        windowID: 30,
        appPID: 30,
        appName: "Minimized",
        bundleID: "com.example.minimized",
        framePoints: CaptureRect(x: 100, y: 100, width: 200, height: 100),
        zRank: 2,
        isOnScreen: false,
        isMinimized: true
      ),
      window(
        windowID: 20,
        appPID: 20,
        appName: "Offscreen",
        bundleID: "com.example.offscreen",
        framePoints: CaptureRect(x: 100, y: 100, width: 200, height: 100),
        zRank: 3,
        isOnScreen: false
      ),
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        framePoints: CaptureRect(x: 100, y: 100, width: 200, height: 100),
        zRank: 4
      )
    ]

    OneContextWindowIndexer.annotateWindowGeometry(displays: [
      display(displayID: "main", framePoints: CaptureRect(x: 0, y: 0, width: 1000, height: 800))
    ], windows: &windows)

    XCTAssertEqual(windows[4].visibleFractionEstimate ?? -1, 1, accuracy: 0.001)
  }

  func testGeometryDoesNotRewriteFocusedWindowBehindTopCandidate() {
    var windows = [
      window(
        windowID: 20,
        appPID: 20,
        appName: "Google Chrome",
        bundleID: "com.google.Chrome",
        framePoints: CaptureRect(x: 0, y: 0, width: 1280, height: 800),
        zRank: 0
      ),
      window(
        windowID: 10,
        appPID: 10,
        appName: "Terminal",
        bundleID: "com.apple.Terminal",
        framePoints: CaptureRect(x: 0, y: 0, width: 1280, height: 800),
        zRank: 1,
        isFocused: true
      )
    ]
    let activeApplication = CaptureActiveApplication(
      processID: 10,
      bundleID: "com.apple.Terminal",
      appName: "Terminal"
    )

    OneContextWindowIndexer.annotateWindowGeometry(displays: [
      display(displayID: "main", framePoints: CaptureRect(x: 0, y: 0, width: 1280, height: 800))
    ], windows: &windows)
    let resolvedActive = OneContextWindowIndexer.reconcileActiveWindowMetadata(
      activeApplication: activeApplication,
      windows: &windows
    )

    XCTAssertEqual(windows.filter(\.isFocused).map(\.windowID), [10])
    XCTAssertEqual(windows[1].visibleFractionEstimate ?? -1, 0, accuracy: 0.001)
    XCTAssertEqual(resolvedActive?.processID, 10)
  }

  private func display(
    displayID: String,
    framePoints: CaptureRect,
    scaleFactor: Double = 1,
    isMain: Bool = false
  ) -> CaptureDisplayState {
    CaptureDisplayState(
      displayID: displayID,
      framePoints: framePoints,
      scaleFactor: scaleFactor,
      isMain: isMain
    )
  }

  private func window(
    windowID: UInt32,
    appPID: Int32,
    appName: String,
    bundleID: String?,
    title: String = "main",
    framePoints: CaptureRect = CaptureRect(x: 0, y: 0, width: 1280, height: 800),
    zRank: Int,
    layer: Int = 0,
    alpha: Double? = 1,
    isFocused: Bool = false,
    isOnScreen: Bool = true,
    isMinimized: Bool = false,
    captureEligible: Bool = true,
    focusMetadata: CaptureWindowFocusMetadata? = nil
  ) -> CaptureWindowState {
    CaptureWindowState(
      time: "2026-05-24T10:11:12.123Z",
      windowID: windowID,
      appPID: appPID,
      appName: appName,
      bundleID: bundleID,
      title: title,
      framePoints: framePoints,
      zRank: zRank,
      layer: layer,
      alpha: alpha,
      isFocused: isFocused,
      isOnScreen: isOnScreen,
      isMinimized: isMinimized,
      captureEligible: captureEligible,
      source: "test",
      focusMetadata: focusMetadata
    )
  }
}
