use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleState {
    Partial,
    Ready,
    Failed,
    Expired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RetentionClass {
    Ephemeral,
    FailedAudit,
    PinnedDebug,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaptureBundleManifest {
    pub schema_version: u32,
    pub contract_version: String,
    pub capture_id: String,
    pub state: BundleState,
    pub created_at: DateTime<Utc>,
    #[serde(default)]
    pub ready_at: Option<DateTime<Utc>>,
    pub time_start: DateTime<Utc>,
    pub time_end: DateTime<Utc>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    pub retention_class: RetentionClass,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub pin_reason: Option<String>,
    #[serde(default)]
    pub source_spool: Value,
    #[serde(default)]
    pub v0_required_files: Vec<String>,
    #[serde(default)]
    pub optional_files: Vec<String>,
    #[serde(default)]
    pub byte_count: u64,
    #[serde(default)]
    pub file_count: u64,
    #[serde(default)]
    pub lane_count: u64,
    #[serde(default)]
    pub known_gap_count: u64,
    #[serde(default)]
    pub producer: Value,
    #[serde(default)]
    pub app_identity: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceStatus {
    Present,
    Degraded,
    PermissionDenied,
    SourceUnavailable,
    DisabledByPolicy,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LaneSource {
    pub source_id: String,
    pub lane_id: String,
    pub status: SourceStatus,
    #[serde(default)]
    pub required_for_v0: bool,
    #[serde(default)]
    pub record_count: u64,
    #[serde(default)]
    pub degraded_reason: Option<String>,
    #[serde(default)]
    pub confidence: Option<f32>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceInventory {
    pub schema_version: u32,
    pub sources: Vec<LaneSource>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceEnvelopeRefKind {
    BundleRelative,
    ExternalPathMetadata,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SourceEnvelopeRefRecord {
    pub schema_version: u32,
    pub kind: SourceEnvelopeRefKind,
    pub path: String,
    #[serde(default)]
    pub absolute: bool,
    #[serde(default)]
    pub exists: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<DateTime<Utc>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct KnownGapRecord {
    pub schema_version: u32,
    pub time: DateTime<Utc>,
    pub source_id: String,
    pub severity: String,
    pub code: String,
    pub message: String,
    pub blocks_ready: bool,
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValidationSeverity {
    Info,
    Warning,
    Error,
    Fatal,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationFinding {
    pub severity: ValidationSeverity,
    pub code: String,
    #[serde(default)]
    pub path: Option<String>,
    pub message: String,
    #[serde(default)]
    pub blocks_ready: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ValidationReport {
    pub ok: bool,
    #[serde(default)]
    pub capture_id: Option<String>,
    pub findings: Vec<ValidationFinding>,
}

impl ValidationReport {
    pub fn new(capture_id: Option<String>) -> Self {
        Self {
            ok: true,
            capture_id,
            findings: Vec::new(),
        }
    }

    pub fn push(&mut self, finding: ValidationFinding) {
        if matches!(
            finding.severity,
            ValidationSeverity::Error | ValidationSeverity::Fatal
        ) || finding.blocks_ready
        {
            self.ok = false;
        }
        self.findings.push(finding);
    }
}
