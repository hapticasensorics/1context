use serde::{Deserialize, Serialize};

pub type JsonMap = serde_json::Map<String, serde_json::Value>;

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardSession {
    pub session_id: String,
    pub title: String,
    pub created_at: String,
    pub fixture: DashboardFixtureConfig,
    pub media: DashboardMediaConfig,
    pub inputs: DashboardInputsConfig,
    pub filter_output: FilterOutputRef,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardFixtureConfig {
    pub run_id: String,
    pub root: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardMediaConfig {
    pub video_ref: String,
    pub video_width: u32,
    pub video_height: u32,
    pub candidate_frame_sets: Vec<CandidateFrameSet>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CandidateFrameSet {
    pub id: String,
    pub root: String,
    pub fps: f32,
    pub count: usize,
    pub naming: String,
    pub frame_times_ms: Option<Vec<u64>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DashboardInputsConfig {
    pub candidate_index_ref: Option<String>,
    pub snapshots_root: Option<String>,
    pub event_refs: Vec<EventRef>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct EventRef {
    pub id: String,
    pub kind: String,
    #[serde(rename = "ref")]
    pub path: String,
    pub format: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct FilterOutputRef {
    #[serde(rename = "ref")]
    pub path: String,
}

#[derive(Debug, Clone)]
pub struct CaptureEvent {
    pub id: String,
    pub event_type: String,
    pub t_ms: u64,
    pub duration_ms: Option<u64>,
    pub payload: serde_json::Value,
    pub source_ref: String,
    pub source_line: usize,
}

impl CaptureEvent {
    pub fn app_name(&self) -> Option<&str> {
        string_path(&self.payload, &["payload", "activeApplication", "appName"])
            .or_else(|| string_path(&self.payload, &["payload", "target", "appName"]))
    }

    pub fn bundle_id(&self) -> Option<&str> {
        string_path(&self.payload, &["payload", "activeApplication", "bundleID"])
            .or_else(|| string_path(&self.payload, &["payload", "target", "bundleID"]))
    }

    pub fn window_title(&self) -> Option<&str> {
        string_path(&self.payload, &["payload", "focusedWindow", "title"])
            .or_else(|| string_path(&self.payload, &["payload", "target", "title"]))
    }

    pub fn durability(&self) -> Option<&str> {
        string_key(&self.payload, "durability")
    }

    pub fn privacy_class(&self) -> Option<&str> {
        string_key(&self.payload, "privacy_class")
    }

    pub fn privacy_shape(&self) -> Option<&str> {
        string_key(&self.payload, "privacy_shape")
    }

    pub fn source_clock(&self) -> Option<&str> {
        string_key(&self.payload, "source_clock")
    }

    pub fn lane_id(&self) -> Option<&str> {
        string_key(&self.payload, "lane_id")
    }

    pub fn stream_id(&self) -> Option<&str> {
        string_key(&self.payload, "stream_id")
    }
}

fn string_key<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(serde_json::Value::as_str)
}

fn string_path<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_str()
}

#[derive(Debug, Clone)]
pub struct CandidateState {
    pub id: String,
    pub frame_id: String,
    pub t_ms: u64,
    pub image_ref: String,
    pub nearby_events: Vec<CaptureEvent>,
    pub signals: Vec<AttentionSignal>,
    pub attention_score: f32,
    pub memory_value_score: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttentionFilterOutput {
    pub version: String,
    pub capture_id: String,
    pub time_range_ms: [u64; 2],
    pub summary: AttentionSummary,
    pub saved_states: Vec<SavedAttentionState>,
    pub raw_buffer_audit: Vec<RawBufferItem>,
    pub composites: Vec<serde_json::Value>,
    pub agent_packet: AgentAttentionPacket,
    pub source_conflicts: Vec<serde_json::Value>,
    pub attention_debt: Vec<serde_json::Value>,
    pub algorithms: Vec<AlgorithmRunSummary>,
    pub policy: JsonMap,
    pub provenance_refs: Vec<ProvenanceRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttentionSummary {
    pub activity_label: String,
    pub activity_summary: String,
    pub confidence: f32,
    pub apps_seen: Vec<String>,
    pub windows_seen: Vec<String>,
    pub urls_seen: Vec<String>,
    pub files_seen: Vec<String>,
    pub commands_seen: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SavedAttentionState {
    pub id: String,
    pub candidate_id: Option<String>,
    pub decision: String,
    pub title: String,
    pub time_ms: u64,
    pub duration_ms: Option<u64>,
    pub app_name: String,
    pub window_title: String,
    pub url: Option<String>,
    pub active_file: Option<String>,
    pub terminal_command: Option<String>,
    pub base_screenshot_ref: Option<String>,
    pub thumbnail_ref: Option<String>,
    pub overlay_regions: Vec<AttentionRegion>,
    pub semantic_excerpt: String,
    pub redaction_summary: Option<String>,
    pub explanation: DecisionExplanation,
    pub proof_bundle: ProofBundleSummary,
    pub related_composite_ids: Vec<String>,
    pub related_object_lineage_ids: Vec<String>,
    pub provenance_refs: Vec<ProvenanceRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RawBufferItem {
    pub candidate_id: String,
    pub frame_id: String,
    pub t_ms: u64,
    pub decision: String,
    pub thumbnail_ref: String,
    pub nearest_saved_state_id: Option<String>,
    pub explanation: String,
    pub score_components: JsonMap,
    pub top_signals: Vec<AttentionSignal>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DecisionExplanation {
    pub primary_reason: String,
    pub reasons: Vec<String>,
    pub attention_score: f32,
    pub memory_value_score: f32,
    pub confidence: f32,
    pub score_components: JsonMap,
    pub algorithm_votes: Vec<AlgorithmVote>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttentionSignal {
    pub algorithm: String,
    pub kind: String,
    pub strength: f32,
    pub hard_keep: Option<bool>,
    pub region: Option<AttentionRegion>,
    pub explanation: String,
    pub provenance_refs: Vec<ProvenanceRef>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AttentionRegion {
    pub bbox: Rect,
    pub score: f32,
    pub tint: String,
    pub label: String,
    pub explanation: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlgorithmVote {
    pub algorithm: String,
    pub vote: String,
    pub reason: String,
    pub strength: f32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProofBundleSummary {
    pub proof_tier: Option<String>,
    pub evidence_refs: Vec<EvidenceRefSummary>,
    pub raw_event_refs: Vec<String>,
    pub screenshot_refs: Vec<String>,
    pub semantic_refs: Vec<String>,
    pub explanation: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EvidenceRefSummary {
    pub kind: String,
    pub path: String,
    pub label: Option<String>,
    pub t_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ProvenanceRef {
    pub id: Option<String>,
    pub kind: Option<String>,
    pub path: Option<String>,
    pub t_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AgentAttentionPacket {
    pub time_range_ms: [u64; 2],
    pub activity_summary: String,
    pub confidence: f32,
    pub important_observations: Vec<serde_json::Value>,
    pub extracted_text: Vec<serde_json::Value>,
    pub composites: Vec<serde_json::Value>,
    pub askable_evidence: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AlgorithmRunSummary {
    pub id: String,
    pub name: String,
    pub version: Option<String>,
    pub enabled: Option<bool>,
    pub status: Option<String>,
    pub summary: Option<String>,
    pub explanation: Option<String>,
    pub candidates_considered: Option<usize>,
    pub saved_count: Option<usize>,
    pub merged_count: Option<usize>,
    pub dropped_count: Option<usize>,
    pub runtime_ms: Option<f32>,
    pub score_components: JsonMap,
    pub votes: Vec<AlgorithmVote>,
}
