import Foundation
import OneContextPlatform

public struct OneContextCaptureDashboard {
  private let runtimePaths: RuntimePaths

  public init(runtimePaths: RuntimePaths) {
    self.runtimePaths = runtimePaths
  }

  public func render(now: Date = Date()) throws -> String {
    let capturePaths = OneContextCapturePaths(runtimePaths: runtimePaths)
    let store = OneContextCaptureLogStore(paths: capturePaths)
    guard let stored = try store.latestWindowSnapshot() else {
      return """
      1Context Capture Dashboard

      No stored capture snapshots yet.

      Capture root:
      \(capturePaths.rootDirectory.path)

      Try:
      1context capture snapshot
      1context capture dashboard --snapshot
      """
    }

    let latestMetadata = try? store.latestActiveWindowFrameMetadata()
    return CaptureDashboardRenderer().render(
      stored: stored,
      paths: capturePaths,
      now: now,
      latestMetadata: latestMetadata
    )
  }
}

public struct CaptureDashboardRenderer: Sendable {
  public init() {}

  public func render(
    stored: StoredCaptureSnapshot,
    paths: OneContextCapturePaths,
    now: Date,
    latestMetadata: StoredCaptureEvent<ActiveWindowFrameMetadata>? = nil
  ) -> String {
    let snapshot = stored.snapshot
    let focused = snapshot.windows.filter(\.isFocused)
    let eligible = snapshot.windows
      .filter(\.captureEligible)
      .sorted { lhs, rhs in
        if lhs.zRank == rhs.zRank {
          return lhs.windowID < rhs.windowID
        }
        return lhs.zRank < rhs.zRank
      }
    let visible = snapshot.windows.filter(\.isOnScreen)
    let apps = Dictionary(grouping: snapshot.windows, by: \.appName)
    let topApps = apps
      .map { (app: $0.key, count: $0.value.count, visible: $0.value.filter(\.isOnScreen).count) }
      .sorted { lhs, rhs in
        if lhs.visible == rhs.visible {
          return lhs.count > rhs.count
        }
        return lhs.visible > rhs.visible
      }
      .prefix(8)

    var lines: [String] = []
    lines.append("1Context Capture Dashboard")
    lines.append("Generated: \(snapshot.generatedAt)")
    lines.append("Dashboard: \(ISO8601DateFormatter().string(from: now))")
    lines.append("Source: \(stored.fileURL.path)")
    lines.append("")
    lines.append("Totals")
    lines.append("  Displays: \(snapshot.displays.count)")
    lines.append("  Windows: \(snapshot.windows.count)")
    lines.append("  Visible: \(visible.count)")
    lines.append("  Capture eligible: \(eligible.count)")
    lines.append("  Focused records: \(focused.count)")
    lines.append("")

    lines.append("Focused")
    if focused.isEmpty {
      lines.append("  none")
    } else {
      for window in focused.prefix(5) {
        lines.append("  \(window.shortDashboardLine)")
      }
    }
    lines.append("")

    lines.append("Top Capture-Eligible Windows")
    if eligible.isEmpty {
      lines.append("  none")
    } else {
      for window in eligible.prefix(12) {
        lines.append("  \(window.shortDashboardLine)")
      }
    }
    lines.append("")

    lines.append("Top Apps")
    if topApps.isEmpty {
      lines.append("  none")
    } else {
      for app in topApps {
        lines.append("  \(app.app): windows=\(app.count), visible=\(app.visible)")
      }
    }
    lines.append("")

    lines.append("Active Window Metadata")
    if let latestMetadata {
      let metadata = latestMetadata.payload
      let summary = metadata.dirtyRectSummary
      lines.append("  Source: \(latestMetadata.fileURL.path)")
      lines.append("  Target: #\(metadata.target.windowID) \(metadata.target.appName) - \(metadata.target.title.isEmpty ? "(untitled)" : metadata.target.title)")
      lines.append("  Frame: seq=\(metadata.sequence) status=\(metadata.frameStatus.rawValue) feed=\(metadata.feedsMotionClassifier)")
      lines.append("  Dirty: rects=\(summary.dirtyRectCount) area=\(formatRatio(summary.dirtyAreaRatio)) tiles=\(formatRatio(summary.changedTileRatio)) dy=\(formatDouble(summary.estimatedDY))")
    } else {
      lines.append("  none")
      lines.append("  Try: 1context capture metadata-sample")
    }
    lines.append("")

    lines.append("Capture Store")
    lines.append("  Root: \(paths.rootDirectory.path)")
    lines.append("  Windows: \(paths.windowsDirectory.path)")
    lines.append("  Media: \(paths.mediaDirectory.path)")
    lines.append("")
    lines.append("Next artifacts expected here: text_flow sessions, scroll composites, sparse keyframes.")
    return lines.joined(separator: "\n")
  }
}

private func formatRatio(_ value: Double) -> String {
  String(format: "%.4f", value)
}

private func formatDouble(_ value: Double) -> String {
  String(format: "%.2f", value)
}

extension CaptureWindowState {
  fileprivate var shortDashboardLine: String {
    let titlePart = title.isEmpty ? "(untitled)" : title
    let frame = "\(Int(framePoints.width))x\(Int(framePoints.height))+\(Int(framePoints.x)),\(Int(framePoints.y))"
    return "#\(windowID) z=\(zRank) \(appName) - \(titlePart) [\(frame)] source=\(source)"
  }
}
