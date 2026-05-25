import Foundation

public struct CapturePermissionDerivedMetadata: Codable, Equatable, Sendable {
  public var schemaVersion: Int
  public var generatedAt: String
  public var privacy: CapturePermissionMetadataPrivacy
  public var processIdentities: [CaptureProcessIdentity]
  public var capturePaths: CaptureStatusPathMetadata
  public var signals: [String: CapturePermissionSignalMetadata]

  public init(
    schemaVersion: Int = 1,
    generatedAt: String,
    privacy: CapturePermissionMetadataPrivacy = CapturePermissionMetadataPrivacy(),
    processIdentities: [CaptureProcessIdentity],
    capturePaths: CaptureStatusPathMetadata,
    signals: [String: CapturePermissionSignalMetadata]
  ) {
    self.schemaVersion = schemaVersion
    self.generatedAt = generatedAt
    self.privacy = privacy
    self.processIdentities = processIdentities
    self.capturePaths = capturePaths
    self.signals = signals
  }

  enum CodingKeys: String, CodingKey {
    case schemaVersion = "schema_version"
    case generatedAt = "generated_at"
    case privacy
    case processIdentities = "process_identities"
    case capturePaths = "capture_paths"
    case signals
  }
}

public struct CapturePermissionMetadataPrivacy: Codable, Equatable, Sendable {
  public var rawKeystrokesIncluded: Bool
  public var rawTextIncluded: Bool
  public var coordinatesIncluded: Bool
  public var aggregatesAndCountsOnly: Bool

  public init(
    rawKeystrokesIncluded: Bool = false,
    rawTextIncluded: Bool = false,
    coordinatesIncluded: Bool = false,
    aggregatesAndCountsOnly: Bool = true
  ) {
    self.rawKeystrokesIncluded = rawKeystrokesIncluded
    self.rawTextIncluded = rawTextIncluded
    self.coordinatesIncluded = coordinatesIncluded
    self.aggregatesAndCountsOnly = aggregatesAndCountsOnly
  }

  enum CodingKeys: String, CodingKey {
    case rawKeystrokesIncluded = "raw_keystrokes_included"
    case rawTextIncluded = "raw_text_included"
    case coordinatesIncluded = "coordinates_included"
    case aggregatesAndCountsOnly = "aggregates_and_counts_only"
  }
}

public struct CaptureProcessIdentity: Codable, Equatable, Sendable {
  public var role: String
  public var pid: Int?
  public var executablePath: String?
  public var bundleIdentifier: String?
  public var appVersion: String?
  public var designatedRequirementSHA256: String?

  public init(
    role: String,
    pid: Int? = nil,
    executablePath: String? = nil,
    bundleIdentifier: String? = nil,
    appVersion: String? = nil,
    designatedRequirementSHA256: String? = nil
  ) {
    self.role = role
    self.pid = pid
    self.executablePath = executablePath
    self.bundleIdentifier = bundleIdentifier
    self.appVersion = appVersion
    self.designatedRequirementSHA256 = designatedRequirementSHA256
  }

  enum CodingKeys: String, CodingKey {
    case role
    case pid
    case executablePath = "executable_path"
    case bundleIdentifier = "bundle_identifier"
    case appVersion = "app_version"
    case designatedRequirementSHA256 = "designated_requirement_sha256"
  }
}

public struct CaptureStatusPathMetadata: Codable, Equatable, Sendable {
  public var rootDirectory: String
  public var eventsDirectory: String
  public var windowsDirectory: String
  public var mediaDirectory: String

  public init(
    rootDirectory: String,
    eventsDirectory: String,
    windowsDirectory: String,
    mediaDirectory: String
  ) {
    self.rootDirectory = rootDirectory
    self.eventsDirectory = eventsDirectory
    self.windowsDirectory = windowsDirectory
    self.mediaDirectory = mediaDirectory
  }

  enum CodingKeys: String, CodingKey {
    case rootDirectory = "root_directory"
    case eventsDirectory = "events_directory"
    case windowsDirectory = "windows_directory"
    case mediaDirectory = "media_directory"
  }
}

public struct CapturePermissionSignalMetadata: Codable, Equatable, Sendable {
  public var ready: Bool
  public var status: String
  public var source: String
  public var ownerRole: String
  public var permissionSubjectRole: String?
  public var note: String?
  public var focusedContext: CaptureFocusedContextAvailability?
  public var eventTap: CaptureInputEventTapMetadata?
  public var proof: CapturePermissionProofSummary?

  public init(
    ready: Bool,
    status: String,
    source: String,
    ownerRole: String,
    permissionSubjectRole: String? = nil,
    note: String? = nil,
    focusedContext: CaptureFocusedContextAvailability? = nil,
    eventTap: CaptureInputEventTapMetadata? = nil,
    proof: CapturePermissionProofSummary? = nil
  ) {
    self.ready = ready
    self.status = status
    self.source = source
    self.ownerRole = ownerRole
    self.permissionSubjectRole = permissionSubjectRole
    self.note = note
    self.focusedContext = focusedContext
    self.eventTap = eventTap
    self.proof = proof
  }

  enum CodingKeys: String, CodingKey {
    case ready
    case status
    case source
    case ownerRole = "owner_role"
    case permissionSubjectRole = "permission_subject_role"
    case note
    case focusedContext = "focused_context"
    case eventTap = "event_tap"
    case proof
  }
}

public struct CaptureFocusedContextAvailability: Codable, Equatable, Sendable {
  public var available: Bool
  public var trusted: Bool
  public var status: String
  public var source: String

  public init(available: Bool, trusted: Bool, status: String, source: String) {
    self.available = available
    self.trusted = trusted
    self.status = status
    self.source = source
  }
}

public struct CaptureInputEventTapMetadata: Codable, Equatable, Sendable {
  public var active: Bool
  public var lifecycleState: String
  public var eventTap: String
  public var tapOptions: String
  public var eventMask: [String]
  public var observedEventCount: Int
  public var queueDepth: Int
  public var droppedCount: Int
  public var coalescedCount: Int
  public var lastEventAt: String?

  public init(
    active: Bool,
    lifecycleState: String,
    eventTap: String,
    tapOptions: String,
    eventMask: [String],
    observedEventCount: Int,
    queueDepth: Int,
    droppedCount: Int,
    coalescedCount: Int,
    lastEventAt: String? = nil
  ) {
    self.active = active
    self.lifecycleState = lifecycleState
    self.eventTap = eventTap
    self.tapOptions = tapOptions
    self.eventMask = eventMask
    self.observedEventCount = observedEventCount
    self.queueDepth = queueDepth
    self.droppedCount = droppedCount
    self.coalescedCount = coalescedCount
    self.lastEventAt = lastEventAt
  }

  enum CodingKeys: String, CodingKey {
    case active
    case lifecycleState = "lifecycle_state"
    case eventTap = "event_tap"
    case tapOptions = "tap_options"
    case eventMask = "event_mask"
    case observedEventCount = "observed_event_count"
    case queueDepth = "queue_depth"
    case droppedCount = "dropped_count"
    case coalescedCount = "coalesced_count"
    case lastEventAt = "last_event_at"
  }
}

public struct CapturePermissionProofSummary: Codable, Equatable, Sendable {
  public var proofKey: String
  public var recorded: Bool
  public var matchesCurrentSubject: Bool
  public var method: String?
  public var provedAt: String?
  public var details: [String: String]

  public init(
    proofKey: String,
    recorded: Bool,
    matchesCurrentSubject: Bool,
    method: String? = nil,
    provedAt: String? = nil,
    details: [String: String] = [:]
  ) {
    self.proofKey = proofKey
    self.recorded = recorded
    self.matchesCurrentSubject = matchesCurrentSubject
    self.method = method
    self.provedAt = provedAt
    self.details = details
  }

  enum CodingKeys: String, CodingKey {
    case proofKey = "proof_key"
    case recorded
    case matchesCurrentSubject = "matches_current_subject"
    case method
    case provedAt = "proved_at"
    case details
  }
}
