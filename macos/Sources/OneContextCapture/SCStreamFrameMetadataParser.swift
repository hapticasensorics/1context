import CoreGraphics
import CoreMedia
import Foundation
@preconcurrency import ScreenCaptureKit

public enum SCStreamFrameMetadataAttachmentKey {
  public static let status = "status"
  public static let displayTime = "display_time"
  public static let scaleFactor = "scale_factor"
  public static let contentScale = "content_scale"
  public static let contentRect = "content_rect"
  public static let dirtyRects = "dirty_rects"
}

public struct SCStreamFrameMetadataParser: Sendable {
  public var tileGridDimension: Int
  public var maxRects: Int

  public init(tileGridDimension: Int = 16, maxRects: Int = 32) {
    self.tileGridDimension = max(1, tileGridDimension)
    self.maxRects = max(0, maxRects)
  }

  public func parse(
    sampleBuffer: CMSampleBuffer,
    streamID: String,
    sequence: Int,
    target: ActiveWindowMetadataTarget,
    previousWeightedCenterY: Double?,
    capturedAt: String = SCStreamFrameMetadataParser.isoTimestamp(),
    uxMotionHints: UXMotionHints? = nil
  ) -> ActiveWindowFrameMetadata {
    guard let attachments = Self.attachments(from: sampleBuffer) else {
      return parse(
        attachments: [:],
        attachmentsPresent: false,
        streamID: streamID,
        sequence: sequence,
        target: target,
        previousWeightedCenterY: previousWeightedCenterY,
        capturedAt: capturedAt,
        uxMotionHints: uxMotionHints
      )
    }

    return parse(
      attachments: attachments,
      attachmentsPresent: true,
      streamID: streamID,
      sequence: sequence,
      target: target,
      previousWeightedCenterY: previousWeightedCenterY,
      capturedAt: capturedAt,
      uxMotionHints: uxMotionHints
    )
  }

  public func parse(
    attachments: [String: Any],
    attachmentsPresent: Bool = true,
    streamID: String,
    sequence: Int,
    target: ActiveWindowMetadataTarget,
    previousWeightedCenterY: Double?,
    capturedAt: String = SCStreamFrameMetadataParser.isoTimestamp(),
    uxMotionHints: UXMotionHints? = nil
  ) -> ActiveWindowFrameMetadata {
    var warnings: [String] = []
    if !attachmentsPresent {
      warnings.append("missing_attachments")
    }

    let rawStatus = Self.intValue(attachments[SCStreamFrameMetadataAttachmentKey.status])
    if rawStatus == nil {
      warnings.append("missing_status")
    }
    let status = CaptureFrameStatus(rawStatusValue: rawStatus)
    if status == .unknown, rawStatus != nil {
      warnings.append("unknown_status")
    }

    let scaleFactor = Self.doubleValue(attachments[SCStreamFrameMetadataAttachmentKey.scaleFactor])
    let contentScale = Self.doubleValue(attachments[SCStreamFrameMetadataAttachmentKey.contentScale])
    let displayTime = Self.uint64Value(attachments[SCStreamFrameMetadataAttachmentKey.displayTime])
    let contentRect = Self.rectValue(attachments[SCStreamFrameMetadataAttachmentKey.contentRect])
    if attachments.keys.contains(SCStreamFrameMetadataAttachmentKey.contentRect), contentRect == nil {
      warnings.append("malformed_content_rect")
    }

    let dirtyParse = Self.dirtyRects(from: attachments[SCStreamFrameMetadataAttachmentKey.dirtyRects])
    if !dirtyParse.present, status == .complete {
      warnings.append("missing_dirty_rects")
    }
    if dirtyParse.malformedCount > 0 {
      warnings.append("malformed_dirty_rects:\(dirtyParse.malformedCount)")
    }

    let bounds = Self.measurementBounds(
      contentRect: contentRect,
      scaleFactor: scaleFactor,
      target: target
    )
    let summary = Self.summarize(
      dirtyRects: dirtyParse.rects,
      bounds: bounds,
      malformedRectCount: dirtyParse.malformedCount,
      previousWeightedCenterY: previousWeightedCenterY,
      tileGridDimension: tileGridDimension,
      maxRects: maxRects
    )

    let feedsMotionClassifier = status.feedsMotionClassifier
    let motionSummary = status == .complete ? summary : .zero(cappedRectLimit: maxRects)
    let motionFeatures = feedsMotionClassifier
      ? Self.fuse(
        MotionFeatures(
          dirtyAreaRatio: motionSummary.dirtyAreaRatio,
          dirtyRectCount: motionSummary.dirtyRectCount,
          meanPixelDiff: 0,
          changedTileRatio: motionSummary.changedTileRatio,
          estimatedDY: motionSummary.estimatedDY,
          scrollEventRecently: false,
          keyboardEventRecently: false,
          ocrNewLineRate: 0,
          focused: target.isFocused
        ),
        with: uxMotionHints,
        dirtyRectSummary: motionSummary
      )
      : .zero(focused: target.isFocused)

    return ActiveWindowFrameMetadata(
      streamID: streamID,
      sequence: sequence,
      capturedAt: capturedAt,
      target: target,
      frameStatus: status,
      frameStatusRawValue: rawStatus,
      attachmentsPresent: attachmentsPresent,
      displayTime: displayTime,
      contentRect: contentRect,
      contentScale: contentScale,
      scaleFactor: scaleFactor,
      dirtyRectSummary: summary,
      motionFeatures: motionFeatures,
      uxMotionHints: uxMotionHints,
      uxMotionHintsFused: uxMotionHints != nil && feedsMotionClassifier,
      feedsMotionClassifier: feedsMotionClassifier,
      parseWarnings: warnings
    )
  }

  private static func fuse(
    _ features: MotionFeatures,
    with hints: UXMotionHints?,
    dirtyRectSummary: CaptureDirtyRectSummary
  ) -> MotionFeatures {
    guard let hints else { return features }

    var fused = features
    fused.scrollEventRecently = hints.scrollEventRecently
    fused.keyboardEventRecently = hints.keyboardActivityRecently
    fused.focused = features.focused || hints.focusedRecently

    if shouldUseScrollDYFallback(
      features: features,
      hints: hints,
      dirtyRectSummary: dirtyRectSummary
    ) {
      fused.estimatedDY = hints.estimatedScrollDY
    }

    return fused
  }

  private static func shouldUseScrollDYFallback(
    features: MotionFeatures,
    hints: UXMotionHints,
    dirtyRectSummary: CaptureDirtyRectSummary
  ) -> Bool {
    guard hints.scrollEventRecently,
      abs(features.estimatedDY) < 0.0001,
      abs(hints.estimatedScrollDY) > 3,
      dirtyRectSummary.dirtyRectCount > 0
    else {
      return false
    }

    return dirtyRectSummary.dirtyAreaRatio >= 0.003
      && dirtyRectSummary.changedTileRatio <= 0.45
  }

  public static func summarize(
    dirtyRects: [CaptureRect],
    bounds: CaptureRect,
    malformedRectCount: Int = 0,
    previousWeightedCenterY: Double? = nil,
    tileGridDimension: Int = 16,
    maxRects: Int = 32
  ) -> CaptureDirtyRectSummary {
    let grid = max(1, tileGridDimension)
    let rectLimit = max(0, maxRects)
    let normalizedBounds = bounds.normalized
    guard normalizedBounds.width > 0, normalizedBounds.height > 0 else {
      return .zero(cappedRectLimit: rectLimit, malformedRectCount: malformedRectCount)
    }

    let boundsArea = normalizedBounds.area
    let tileWidth = normalizedBounds.width / Double(grid)
    let tileHeight = normalizedBounds.height / Double(grid)
    let totalTiles = grid * grid
    var tiles = DirtyTileSet(capacity: totalTiles)
    var changedTiles = 0
    var clippedCount = 0
    var dirtyArea = 0.0
    var weightedArea = 0.0
    var weightedCenterYValue = 0.0
    var unionRect: CaptureRect?
    var cappedRects: [CaptureRect] = []
    if rectLimit > 0 {
      cappedRects.reserveCapacity(min(rectLimit, dirtyRects.count))
    }

    for rect in dirtyRects {
      guard let intersection = rect.normalized.intersection(with: normalizedBounds),
        intersection.width > 0,
        intersection.height > 0
      else {
        continue
      }

      clippedCount += 1
      if cappedRects.count < rectLimit {
        cappedRects.append(intersection)
      }

      let area = intersection.area
      dirtyArea += area
      weightedArea += area
      weightedCenterYValue += intersection.midY * area
      if let currentUnion = unionRect {
        unionRect = currentUnion.union(intersection)
      } else {
        unionRect = intersection
      }
      changedTiles += markChangedTiles(
        rect: intersection,
        bounds: normalizedBounds,
        tileWidth: tileWidth,
        tileHeight: tileHeight,
        grid: grid,
        tiles: &tiles
      )
    }

    guard clippedCount > 0 else {
      return .zero(cappedRectLimit: rectLimit, malformedRectCount: malformedRectCount)
    }

    let dirtyAreaRatio = boundsArea > 0 ? min(boundsArea, dirtyArea) / boundsArea : 0
    let weighted = weightedArea > 0 ? weightedCenterYValue / weightedArea : nil
    let estimatedDY = weighted.flatMap { center in
      previousWeightedCenterY.map { center - $0 }
    } ?? 0

    return CaptureDirtyRectSummary(
      dirtyRectCount: clippedCount,
      dirtyAreaRatio: min(1, max(0, dirtyAreaRatio)),
      changedTileRatio: totalTiles > 0 ? Double(changedTiles) / Double(totalTiles) : 0,
      unionRect: unionRect,
      cappedRects: cappedRects,
      cappedRectLimit: rectLimit,
      malformedRectCount: malformedRectCount,
      weightedCenterY: weighted,
      estimatedDY: estimatedDY
    )
  }

  public static func isoTimestamp(_ date: Date = Date()) -> String {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter.string(from: date)
  }

  private static func attachments(from sampleBuffer: CMSampleBuffer) -> [String: Any]? {
    guard CMSampleBufferIsValid(sampleBuffer),
      let attachmentsArray = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false)
        as? [[SCStreamFrameInfo: Any]],
      let attachments = attachmentsArray.first
    else {
      return nil
    }

    var raw: [String: Any] = [:]
    raw[SCStreamFrameMetadataAttachmentKey.status] = attachments[.status]
    raw[SCStreamFrameMetadataAttachmentKey.displayTime] = attachments[.displayTime]
    raw[SCStreamFrameMetadataAttachmentKey.scaleFactor] = attachments[.scaleFactor]
    raw[SCStreamFrameMetadataAttachmentKey.contentScale] = attachments[.contentScale]
    raw[SCStreamFrameMetadataAttachmentKey.contentRect] = attachments[.contentRect]
    raw[SCStreamFrameMetadataAttachmentKey.dirtyRects] = attachments[.dirtyRects]
    return raw
  }

  private static func intValue(_ value: Any?) -> Int? {
    switch value {
    case let number as NSNumber:
      return number.intValue
    case let int as Int:
      return int
    case let int32 as Int32:
      return Int(int32)
    case let int64 as Int64:
      return Int(int64)
    case let uint as UInt:
      return Int(uint)
    case let string as String:
      return Int(string)
    default:
      return nil
    }
  }

  private static func uint64Value(_ value: Any?) -> UInt64? {
    switch value {
    case let number as NSNumber:
      return number.uint64Value
    case let uint64 as UInt64:
      return uint64
    case let uint as UInt:
      return UInt64(uint)
    case let int as Int where int >= 0:
      return UInt64(int)
    case let int64 as Int64 where int64 >= 0:
      return UInt64(int64)
    case let string as String:
      return UInt64(string)
    default:
      return nil
    }
  }

  private static func doubleValue(_ value: Any?) -> Double? {
    switch value {
    case let number as NSNumber:
      return number.doubleValue
    case let double as Double:
      return double
    case let float as Float:
      return Double(float)
    case let cgFloat as CGFloat:
      return Double(cgFloat)
    case let int as Int:
      return Double(int)
    case let string as String:
      return Double(string)
    default:
      return nil
    }
  }

  private static func rectValue(_ value: Any?) -> CaptureRect? {
    switch value {
    case let rect as CaptureRect:
      return rect
    case let rect as CGRect:
      return rect.captureRect
    case let value as NSValue:
      return value.rectValue.captureRect
    case let dict as NSDictionary:
      return CGRect(dictionaryRepresentation: dict)?.captureRect
    case let dict as [String: Any]:
      guard let x = doubleValue(dict["x"] ?? dict["X"]),
        let y = doubleValue(dict["y"] ?? dict["Y"]),
        let width = doubleValue(dict["width"] ?? dict["Width"]),
        let height = doubleValue(dict["height"] ?? dict["Height"])
      else {
        return nil
      }
      return CaptureRect(x: x, y: y, width: width, height: height)
    default:
      return nil
    }
  }

  private static func dirtyRects(from value: Any?) -> (present: Bool, rects: [CaptureRect], malformedCount: Int) {
    guard let value else {
      return (false, [], 0)
    }

    switch value {
    case let rects as [CaptureRect]:
      return parseDirtyRects(rects)
    case let rects as [CGRect]:
      return parseDirtyRects(rects.lazy.map(\.captureRect))
    case let rects as [NSValue]:
      return parseDirtyRects(rects.lazy.map { $0.rectValue.captureRect })
    case let rects as [[String: Any]]:
      return parseDirtyRectValues(rects)
    case let rects as [Any]:
      return parseDirtyRectValues(rects)
    default:
      return (true, [], 1)
    }
  }

  private static func parseDirtyRects<S: Sequence>(
    _ rawRects: S
  ) -> (present: Bool, rects: [CaptureRect], malformedCount: Int) where S.Element == CaptureRect {
    var rects: [CaptureRect] = []
    var malformed = 0
    let estimatedCount = rawRects.underestimatedCount
    if estimatedCount > 0 {
      rects.reserveCapacity(estimatedCount)
    }
    for rect in rawRects {
      guard rect.width > 0, rect.height > 0 else {
        malformed += 1
        continue
      }
      rects.append(rect)
    }
    return (true, rects, malformed)
  }

  private static func parseDirtyRectValues<S: Sequence>(
    _ rawRects: S
  ) -> (present: Bool, rects: [CaptureRect], malformedCount: Int) {
    var rects: [CaptureRect] = []
    var malformed = 0
    if rawRects.underestimatedCount > 0 {
      rects.reserveCapacity(rawRects.underestimatedCount)
    }
    for rawRect in rawRects {
      guard let rect = rectValue(rawRect), rect.width > 0, rect.height > 0 else {
        malformed += 1
        continue
      }
      rects.append(rect)
    }
    return (true, rects, malformed)
  }

  private static func measurementBounds(
    contentRect: CaptureRect?,
    scaleFactor: Double?,
    target: ActiveWindowMetadataTarget
  ) -> CaptureRect {
    let scale = max(0.0001, scaleFactor ?? 1)
    if let contentRect, contentRect.width > 0, contentRect.height > 0 {
      return CaptureRect(
        x: contentRect.x * scale,
        y: contentRect.y * scale,
        width: contentRect.width * scale,
        height: contentRect.height * scale
      )
    }
    if let framePixels = target.framePixels, framePixels.width > 0, framePixels.height > 0 {
      return CaptureRect(x: 0, y: 0, width: framePixels.width, height: framePixels.height)
    }
    return CaptureRect(
      x: 0,
      y: 0,
      width: target.framePoints.width * scale,
      height: target.framePoints.height * scale
    )
  }

  private static func markChangedTiles(
    rect: CaptureRect,
    bounds: CaptureRect,
    tileWidth: Double,
    tileHeight: Double,
    grid: Int,
    tiles: inout DirtyTileSet
  ) -> Int {
    guard tileWidth > 0, tileHeight > 0 else { return 0 }

    let minX = clamp(Int(floor((rect.minX - bounds.minX) / tileWidth)), lower: 0, upper: grid - 1)
    let maxX = clamp(Int(floor(((rect.maxX - bounds.minX) - Double.ulpOfOne) / tileWidth)), lower: 0, upper: grid - 1)
    let minY = clamp(Int(floor((rect.minY - bounds.minY) / tileHeight)), lower: 0, upper: grid - 1)
    let maxY = clamp(Int(floor(((rect.maxY - bounds.minY) - Double.ulpOfOne) / tileHeight)), lower: 0, upper: grid - 1)
    guard minX <= maxX, minY <= maxY else { return 0 }

    var inserted = 0
    for y in minY...maxY {
      for x in minX...maxX {
        if tiles.insert(y * grid + x) {
          inserted += 1
        }
      }
    }
    return inserted
  }

  private static func clamp(_ value: Int, lower: Int, upper: Int) -> Int {
    min(max(value, lower), upper)
  }
}

private struct DirtyTileSet {
  private var words: [UInt64]

  init(capacity: Int) {
    words = Array(repeating: 0, count: max(0, capacity + 63) / 64)
  }

  mutating func insert(_ index: Int) -> Bool {
    let wordIndex = index / 64
    guard wordIndex >= 0, wordIndex < words.count else { return false }
    let mask = UInt64(1) << UInt64(index & 63)
    let oldValue = words[wordIndex]
    guard oldValue & mask == 0 else { return false }
    words[wordIndex] = oldValue | mask
    return true
  }
}

private extension CaptureRect {
  var minX: Double { min(x, x + width) }
  var minY: Double { min(y, y + height) }
  var maxX: Double { max(x, x + width) }
  var maxY: Double { max(y, y + height) }
  var midY: Double { minY + height / 2 }
  var area: Double { max(0, width) * max(0, height) }

  var normalized: CaptureRect {
    CaptureRect(x: minX, y: minY, width: abs(width), height: abs(height))
  }

  func intersection(with other: CaptureRect) -> CaptureRect? {
    let nx = max(minX, other.minX)
    let ny = max(minY, other.minY)
    let mx = min(maxX, other.maxX)
    let my = min(maxY, other.maxY)
    guard mx > nx, my > ny else { return nil }
    return CaptureRect(x: nx, y: ny, width: mx - nx, height: my - ny)
  }

  func union(_ other: CaptureRect) -> CaptureRect {
    let nx = min(minX, other.minX)
    let ny = min(minY, other.minY)
    let mx = max(maxX, other.maxX)
    let my = max(maxY, other.maxY)
    return CaptureRect(x: nx, y: ny, width: mx - nx, height: my - ny)
  }
}
