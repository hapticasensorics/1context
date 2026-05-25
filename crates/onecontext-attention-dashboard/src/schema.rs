use serde::{Deserialize, Serialize};

pub type JsonMap = serde_json::Map<String, serde_json::Value>;

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AttentionDashboardSession {
    pub schema_version: String,
    pub session_id: String,
    pub title: String,
    pub created_at: String,
    pub fixture: DashboardFixtureConfig,
    pub media: DashboardMediaConfig,
    pub inputs: DashboardInputsConfig,
    pub filter_output: FilterOutputRef,
    pub review: ReviewConfig,
    pub ui: DashboardUiConfig,
    pub agent_work_packages: Vec<AgentWorkPackage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashboardFixtureConfig {
    pub run_id: String,
    pub root: String,
    pub duration_ms: u64,
    pub timezone: Option<String>,
    pub notes: Option<String>,
    pub source_manifest_ref: Option<String>,
    pub source_readme_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashboardMediaConfig {
    pub video_ref: String,
    pub video_width: u32,
    pub video_height: u32,
    pub video_duration_ms: u64,
    pub video_fps: Option<f32>,
    pub playback_mode: String,
    pub frame_cache: Option<FrameCacheConfig>,
    pub candidate_frame_sets: Vec<CandidateFrameSet>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FrameCacheConfig {
    pub root: String,
    pub index_ref: String,
    pub frame_width: u32,
    pub frame_height: u32,
    pub fps: f32,
    pub format: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CandidateFrameSet {
    pub id: String,
    pub root: String,
    pub fps: f32,
    pub count: usize,
    pub naming: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashboardInputsConfig {
    pub candidate_index_ref: Option<String>,
    pub snapshots_root: Option<String>,
    pub event_refs: Vec<EventRef>,
    pub timeline_lanes: Vec<TimelineLaneConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EventRef {
    pub id: String,
    pub kind: String,
    #[serde(rename = "ref")]
    pub path: String,
    pub format: String,
    pub required: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TimelineLaneConfig {
    pub id: String,
    pub title: String,
    pub kind: String,
    pub visible: bool,
    pub color: String,
    pub source_ref: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FilterOutputRef {
    #[serde(rename = "ref")]
    pub path: String,
    pub schema_version: String,
    pub generated_by: Option<String>,
    pub generated_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReviewConfig {
    pub labels_ref: String,
    pub autosave: bool,
    pub allowed_labels: Vec<String>,
    pub required_metrics: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DashboardUiConfig {
    pub default_left_panel: String,
    pub default_right_panel: String,
    pub default_bottom_panel: String,
    pub enabled_tabs: Vec<String>,
    pub feature_flags: FeatureFlags,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FeatureFlags {
    pub algorithm_toggles: bool,
    pub ablation_compare: bool,
    pub label_export: bool,
    pub overlay_editor: bool,
    pub side_by_side_runs: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentWorkPackage {
    pub id: String,
    pub title: String,
    pub owner: String,
    pub deliverables: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttentionFilterOutput {
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub capture_id: String,
    #[serde(default)]
    pub time_range_ms: [u64; 2],
    #[serde(default)]
    pub summary: AttentionSummary,
    #[serde(default)]
    pub saved_states: Vec<SavedAttentionState>,
    #[serde(default)]
    pub raw_buffer_audit: Vec<RawBufferItem>,
    #[serde(default)]
    pub composites: Vec<DashboardComposite>,
    #[serde(default)]
    pub agent_packet: AgentAttentionPacket,
    #[serde(default)]
    pub source_conflicts: Vec<SourceConflict>,
    #[serde(default)]
    pub attention_debt: Vec<AttentionDebtItem>,
    #[serde(default)]
    pub algorithms: Vec<AlgorithmRunSummary>,
    #[serde(default)]
    pub policy: Option<serde_json::Value>,
    #[serde(default)]
    pub provenance_refs: Vec<ProvenanceRef>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttentionSummary {
    #[serde(default)]
    pub activity_label: String,
    #[serde(default)]
    pub activity_summary: String,
    #[serde(default)]
    pub apps_seen: Vec<String>,
    #[serde(default)]
    pub windows_seen: Vec<String>,
    #[serde(default)]
    pub urls_seen: Vec<String>,
    #[serde(default)]
    pub files_seen: Vec<String>,
    #[serde(default)]
    pub commands_seen: Vec<String>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SavedAttentionState {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub candidate_id: Option<String>,
    #[serde(default)]
    pub decision: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub time_ms: u64,
    #[serde(default)]
    pub duration_ms: Option<u64>,
    #[serde(default)]
    pub app_name: Option<String>,
    #[serde(default)]
    pub window_title: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub active_file: Option<String>,
    #[serde(default)]
    pub terminal_command: Option<String>,
    #[serde(default)]
    pub base_screenshot_ref: Option<String>,
    #[serde(default)]
    pub thumbnail_ref: Option<String>,
    #[serde(default)]
    pub overlay_regions: Vec<AttentionRegion>,
    #[serde(default)]
    pub semantic_excerpt: Option<String>,
    #[serde(default)]
    pub redaction_summary: Option<String>,
    #[serde(default)]
    pub explanation: Option<DecisionExplanation>,
    #[serde(default)]
    pub proof_bundle: Option<ProofBundleSummary>,
    #[serde(default)]
    pub related_composite_ids: Vec<String>,
    #[serde(default)]
    pub related_object_lineage_ids: Vec<String>,
    #[serde(default)]
    pub provenance_refs: Vec<ProvenanceRef>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RawBufferItem {
    #[serde(default)]
    pub candidate_id: String,
    #[serde(default)]
    pub frame_id: String,
    #[serde(default)]
    pub t_ms: u64,
    #[serde(default)]
    pub thumbnail_ref: String,
    #[serde(default)]
    pub decision: String,
    #[serde(default)]
    pub nearest_saved_state_id: Option<String>,
    #[serde(default)]
    pub top_signals: Vec<DashboardSignal>,
    #[serde(default)]
    pub score_components: JsonMap,
    #[serde(default)]
    pub explanation: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DecisionExplanation {
    #[serde(default)]
    pub primary_reason: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub attention_score: f32,
    #[serde(default)]
    pub memory_value_score: f32,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub score_components: JsonMap,
    #[serde(default)]
    pub algorithm_votes: Vec<AlgorithmVote>,
    #[serde(default)]
    pub source_conflicts: Vec<serde_json::Value>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttentionRegion {
    #[serde(default)]
    pub bbox: Option<Rect>,
    #[serde(default)]
    pub score: f32,
    #[serde(default)]
    pub tint: String,
    #[serde(default)]
    pub label: String,
    #[serde(default)]
    pub reasons: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
    #[serde(default)]
    pub sensitive: Option<bool>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Rect {
    #[serde(default)]
    pub x: f32,
    #[serde(default)]
    pub y: f32,
    #[serde(default)]
    pub width: f32,
    #[serde(default)]
    pub height: f32,
    #[serde(default)]
    pub coordinate_space: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DashboardSignal {
    #[serde(default)]
    pub algorithm: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub strength: f32,
    #[serde(default)]
    pub hard_keep: Option<bool>,
    #[serde(default)]
    pub region: Option<AttentionRegion>,
    #[serde(default)]
    pub explanation: String,
    #[serde(default)]
    pub provenance_refs: Vec<ProvenanceRef>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AlgorithmVote {
    #[serde(default)]
    pub algorithm: String,
    #[serde(default)]
    pub vote: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub strength: f32,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProofBundleSummary {
    #[serde(default)]
    pub proof_tier: Option<String>,
    #[serde(default)]
    pub evidence_refs: Vec<EvidenceRef>,
    #[serde(default)]
    pub raw_event_refs: Vec<String>,
    #[serde(default)]
    pub screenshot_refs: Vec<String>,
    #[serde(default)]
    pub semantic_refs: Vec<String>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct EvidenceRef {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default, rename = "ref", alias = "path")]
    pub path: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub proof_tier: Option<String>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ProvenanceRef {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub kind: Option<String>,
    #[serde(default, rename = "ref", alias = "path")]
    pub path: Option<String>,
    #[serde(default)]
    pub t_ms: Option<u64>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct DashboardComposite {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "type", alias = "kind", alias = "composite_type")]
    pub composite_type: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub ref_path: Option<String>,
    #[serde(default)]
    pub state_ids: Vec<String>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentAttentionPacket {
    #[serde(default)]
    pub time_range_ms: [u64; 2],
    #[serde(default)]
    pub activity_summary: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub important_observations: Vec<AgentObservation>,
    #[serde(default)]
    pub extracted_text: Vec<ExtractedText>,
    #[serde(default)]
    pub composites: Vec<AgentComposite>,
    #[serde(default)]
    pub askable_evidence: Vec<AskableEvidence>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentObservation {
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub evidence_state_id: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub proof_tier: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct ExtractedText {
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub state_ids: Vec<String>,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub sensitive: Option<bool>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AgentComposite {
    #[serde(default)]
    pub id: String,
    #[serde(default, rename = "type", alias = "kind", alias = "composite_type")]
    pub composite_type: String,
    #[serde(default)]
    pub summary: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AskableEvidence {
    #[serde(default)]
    pub label: String,
    #[serde(default, rename = "ref", alias = "path")]
    pub path: String,
    #[serde(default)]
    pub proof_tier: String,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct SourceConflict {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub t_ms: Option<u64>,
    #[serde(default)]
    pub candidate_id: Option<String>,
    #[serde(default)]
    pub saved_state_id: Option<String>,
    #[serde(default)]
    pub source_a: Option<String>,
    #[serde(default)]
    pub source_b: Option<String>,
    #[serde(default)]
    pub conflict: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub severity: Option<String>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AttentionDebtItem {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub t_ms: Option<u64>,
    #[serde(default)]
    pub candidate_id: Option<String>,
    #[serde(default)]
    pub saved_state_id: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub resolution: Option<String>,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(flatten)]
    pub extra: JsonMap,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct AlgorithmRunSummary {
    #[serde(default, alias = "algorithm")]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub candidates_considered: Option<usize>,
    #[serde(default)]
    pub saved_count: Option<usize>,
    #[serde(default)]
    pub merged_count: Option<usize>,
    #[serde(default)]
    pub dropped_count: Option<usize>,
    #[serde(default)]
    pub runtime_ms: Option<f32>,
    #[serde(default)]
    pub votes: Vec<AlgorithmVote>,
    #[serde(default)]
    pub score_components: JsonMap,
    #[serde(default)]
    pub explanation: Option<String>,
    #[serde(flatten)]
    pub extra: JsonMap,
}
