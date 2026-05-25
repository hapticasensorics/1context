use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::envelope::CaptureEnvelope;

pub const SCROLL_DOCUMENT_SCHEMA: &str = "onecontext.scroll_document";
pub const AGENT_OBSERVATION_PACKET_SCHEMA: &str = "onecontext.agent_observation_packet";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservationTimeRange {
    pub start: String,
    pub end: String,
}

impl ObservationTimeRange {
    pub fn new(start: impl Into<String>, end: impl Into<String>) -> Self {
        Self {
            start: start.into(),
            end: end.into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservationRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl ObservationRect {
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EvidenceRef {
    pub evidence_id: String,
    pub kind: String,
    pub uri: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_range: Option<ObservationTimeRange>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureSourceRef {
    pub source_id: String,
    pub app_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bundle_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub window_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frame_ref: Option<EvidenceRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility_ref: Option<EvidenceRef>,
    #[serde(default)]
    pub input_event_refs: Vec<EvidenceRef>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservedTextSpan {
    pub span_id: String,
    pub text: String,
    pub bounds: ObservationRect,
    pub source_frame_id: String,
    pub observed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scroll_offset_y: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanonicalTextSpan {
    pub canonical_id: String,
    pub text: String,
    pub normalized_text: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub representative_bounds: ObservationRect,
    pub source_span_ids: Vec<String>,
    pub source_frame_ids: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub average_confidence: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservedUiElement {
    pub element_id: String,
    pub role: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
    pub bounds: ObservationRect,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UserEvent {
    pub event_id: String,
    pub event_type: String,
    pub observed_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<ObservationRect>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ObservationProvenanceEdge {
    pub from_id: String,
    pub to_id: String,
    pub relation: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScrollDocument {
    pub schema_version: i32,
    pub document_id: String,
    pub source: CaptureSourceRef,
    pub time_range: ObservationTimeRange,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub visual_mosaic_ref: Option<EvidenceRef>,
    #[serde(default)]
    pub text_spans: Vec<CanonicalTextSpan>,
    #[serde(default)]
    pub ui_elements: Vec<ObservedUiElement>,
    #[serde(default)]
    pub user_events: Vec<UserEvent>,
    #[serde(default)]
    pub provenance_edges: Vec<ObservationProvenanceEdge>,
    #[serde(default)]
    pub evidence: Vec<EvidenceRef>,
    #[serde(default)]
    pub metadata: Value,
}

impl ScrollDocument {
    pub fn from_observed_spans(
        document_id: impl Into<String>,
        source: CaptureSourceRef,
        time_range: ObservationTimeRange,
        observed_spans: &[ObservedTextSpan],
    ) -> Self {
        Self {
            schema_version: 1,
            document_id: document_id.into(),
            source,
            time_range,
            visual_mosaic_ref: None,
            text_spans: canonicalize_text_spans(observed_spans),
            ui_elements: Vec::new(),
            user_events: Vec::new(),
            provenance_edges: Vec::new(),
            evidence: Vec::new(),
            metadata: json!({}),
        }
    }

    pub fn to_capture_envelope(
        &self,
        user_id: impl Into<String>,
        stream_id: impl Into<String>,
        lane_id: impl Into<String>,
        privacy_class: impl Into<String>,
    ) -> CaptureEnvelope {
        let text_preview = self
            .text_spans
            .iter()
            .map(|span| span.text.as_str())
            .take(8)
            .collect::<Vec<_>>()
            .join("\n");
        CaptureEnvelope {
            user_id: user_id.into(),
            stream_id: stream_id.into(),
            lane_id: lane_id.into(),
            kind: "scroll_document".to_string(),
            event_start: self.time_range.start.clone(),
            event_end: self.time_range.end.clone(),
            capture_bundle_id: None,
            payload: to_payload(self),
            blob: None,
            source_clock_id: None,
            source_start_ns: None,
            source_end_ns: None,
            source_sequence: None,
            display_title: Some(format!("Scroll document: {}", self.source.app_name)),
            display_text: if text_preview.is_empty() {
                None
            } else {
                Some(text_preview)
            },
            schema_name: Some(SCROLL_DOCUMENT_SCHEMA.to_string()),
            schema_version: Some(self.schema_version),
            confidence: Some(semantic_confidence_from_spans(&self.text_spans)),
            privacy_class: Some(privacy_class.into()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PacketEvidence {
    pub label: String,
    pub evidence_ref: EvidenceRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentObservationPacket {
    pub schema_version: i32,
    pub packet_id: String,
    pub activity: String,
    pub time_range: ObservationTimeRange,
    pub source: CaptureSourceRef,
    #[serde(default)]
    pub new_information: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<PacketEvidence>,
    #[serde(default)]
    pub askable_evidence: Vec<String>,
    #[serde(default)]
    pub provenance_edges: Vec<ObservationProvenanceEdge>,
    pub salience: SalienceGrade,
    pub privacy_class: String,
    #[serde(default)]
    pub metadata: Value,
}

impl AgentObservationPacket {
    pub fn to_capture_envelope(
        &self,
        user_id: impl Into<String>,
        stream_id: impl Into<String>,
        lane_id: impl Into<String>,
    ) -> CaptureEnvelope {
        CaptureEnvelope {
            user_id: user_id.into(),
            stream_id: stream_id.into(),
            lane_id: lane_id.into(),
            kind: "agent_observation_packet".to_string(),
            event_start: self.time_range.start.clone(),
            event_end: self.time_range.end.clone(),
            capture_bundle_id: None,
            payload: to_payload(self),
            blob: None,
            source_clock_id: None,
            source_start_ns: None,
            source_end_ns: None,
            source_sequence: None,
            display_title: Some(self.activity.clone()),
            display_text: Some(self.new_information.join("\n")),
            schema_name: Some(AGENT_OBSERVATION_PACKET_SCHEMA.to_string()),
            schema_version: Some(self.schema_version),
            confidence: Some(self.salience.confidence_hint()),
            privacy_class: Some(self.privacy_class.clone()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScrollSessionFeatures {
    pub same_window: bool,
    pub same_content_pane: bool,
    pub frame_count: usize,
    pub duration_ms: u64,
    pub dominant_vertical_translation: f32,
    pub stable_chrome_ratio: f32,
    pub new_exposed_strip_ratio: f32,
    pub input_scroll_event_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScrollSessionDecision {
    pub is_scroll_session: bool,
    pub confidence: f32,
    pub reasons: Vec<String>,
}

pub fn detect_scroll_session(features: &ScrollSessionFeatures) -> ScrollSessionDecision {
    let mut reasons = Vec::new();
    let mut score = 0.0_f32;

    if features.same_window {
        score += 0.2;
    } else {
        reasons.push("window changed".to_string());
    }

    if features.same_content_pane {
        score += 0.2;
    } else {
        reasons.push("content pane changed".to_string());
    }

    if features.frame_count >= 2 {
        score += 0.1;
    } else {
        reasons.push("not enough frames".to_string());
    }

    if features.dominant_vertical_translation.abs() >= 8.0 {
        score += 0.2;
    } else {
        reasons.push("no dominant vertical translation".to_string());
    }

    if features.stable_chrome_ratio >= 0.65 {
        score += 0.15;
    } else {
        reasons.push("chrome/header/sidebar not stable enough".to_string());
    }

    if features.new_exposed_strip_ratio >= 0.02 {
        score += 0.1;
    } else {
        reasons.push("little newly exposed content".to_string());
    }

    if features.input_scroll_event_count > 0 {
        score += 0.05;
    }

    let confidence = score.clamp(0.0, 1.0);
    if confidence >= 0.75 {
        reasons.push("same surface with vertical flow and stable chrome".to_string());
    }

    let hard_reject =
        !features.same_window || !features.same_content_pane || features.frame_count < 2;

    ScrollSessionDecision {
        is_scroll_session: !hard_reject && confidence >= 0.75,
        confidence,
        reasons,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SemanticChangeFeatures {
    pub pixel_change_ratio: f32,
    pub text_novelty_ratio: f32,
    pub ax_mutation_count: usize,
    pub dom_mutation_count: usize,
    pub click_count: usize,
    pub keypress_count: usize,
    pub selection_changed: bool,
    pub modal_or_error_visible: bool,
    pub dwell_ms: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SalienceGrade {
    Ignore,
    Compress,
    Preserve,
    Escalate,
}

impl SalienceGrade {
    fn confidence_hint(self) -> f32 {
        match self {
            Self::Ignore => 0.35,
            Self::Compress => 0.65,
            Self::Preserve => 0.85,
            Self::Escalate => 0.95,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SalienceDecision {
    pub grade: SalienceGrade,
    pub preserve_keyframe: bool,
    pub build_agent_packet: bool,
    pub rationale: String,
}

pub fn score_salience(features: &SemanticChangeFeatures) -> SalienceDecision {
    if features.modal_or_error_visible {
        return SalienceDecision {
            grade: SalienceGrade::Escalate,
            preserve_keyframe: true,
            build_agent_packet: true,
            rationale: "modal or error appeared".to_string(),
        };
    }

    if features.click_count > 0
        || features.keypress_count > 0
        || features.selection_changed
        || features.ax_mutation_count > 0
        || features.dom_mutation_count > 0
    {
        return SalienceDecision {
            grade: SalienceGrade::Preserve,
            preserve_keyframe: true,
            build_agent_packet: true,
            rationale: "user action or semantic tree changed".to_string(),
        };
    }

    if features.pixel_change_ratio >= 0.25 && features.text_novelty_ratio <= 0.05 {
        return SalienceDecision {
            grade: SalienceGrade::Compress,
            preserve_keyframe: false,
            build_agent_packet: false,
            rationale: "large visual change with little semantic novelty".to_string(),
        };
    }

    if features.pixel_change_ratio <= 0.08 && features.text_novelty_ratio >= 0.2 {
        return SalienceDecision {
            grade: SalienceGrade::Preserve,
            preserve_keyframe: true,
            build_agent_packet: true,
            rationale: "small visual change with new semantic text".to_string(),
        };
    }

    if features.dwell_ms >= 15_000 && features.text_novelty_ratio > 0.0 {
        return SalienceDecision {
            grade: SalienceGrade::Compress,
            preserve_keyframe: true,
            build_agent_packet: true,
            rationale: "dwell on readable content".to_string(),
        };
    }

    SalienceDecision {
        grade: SalienceGrade::Ignore,
        preserve_keyframe: false,
        build_agent_packet: false,
        rationale: "no meaningful semantic or interaction change".to_string(),
    }
}

pub fn canonicalize_text_spans(spans: &[ObservedTextSpan]) -> Vec<CanonicalTextSpan> {
    let mut groups: Vec<CanonicalTextSpan> = Vec::new();

    for span in spans {
        let normalized = normalize_text_span(&span.text);
        if normalized.is_empty() {
            continue;
        }

        if let Some(existing) = groups.iter_mut().find(|candidate| {
            candidate.normalized_text == normalized
                && !candidate.source_frame_ids.contains(&span.source_frame_id)
                && widths_are_compatible(candidate.representative_bounds.width, span.bounds.width)
        }) {
            existing.source_span_ids.push(span.span_id.clone());
            existing.source_frame_ids.push(span.source_frame_id.clone());
            existing.last_seen_at = max_timestamp(&existing.last_seen_at, &span.observed_at);
            if existing.text.len() < span.text.len() {
                existing.text = span.text.clone();
            }
            existing.average_confidence =
                merge_confidence(existing.average_confidence, span.confidence);
        } else {
            groups.push(CanonicalTextSpan {
                canonical_id: stable_id("text", &format!("{}|{}", normalized, span.span_id)),
                text: span.text.clone(),
                normalized_text: normalized,
                first_seen_at: span.observed_at.clone(),
                last_seen_at: span.observed_at.clone(),
                representative_bounds: span.bounds.clone(),
                source_span_ids: vec![span.span_id.clone()],
                source_frame_ids: vec![span.source_frame_id.clone()],
                average_confidence: span.confidence,
            });
        }
    }

    groups
}

pub fn normalize_text_span(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

fn widths_are_compatible(lhs: f32, rhs: f32) -> bool {
    let largest = lhs.abs().max(rhs.abs()).max(1.0);
    ((lhs - rhs).abs() / largest) <= 0.12 || (lhs - rhs).abs() <= 24.0
}

fn max_timestamp(lhs: &str, rhs: &str) -> String {
    if rhs > lhs {
        rhs.to_string()
    } else {
        lhs.to_string()
    }
}

fn merge_confidence(existing: Option<f32>, incoming: Option<f32>) -> Option<f32> {
    match (existing, incoming) {
        (Some(lhs), Some(rhs)) => Some((lhs + rhs) / 2.0),
        (Some(value), None) | (None, Some(value)) => Some(value),
        (None, None) => None,
    }
}

fn semantic_confidence_from_spans(spans: &[CanonicalTextSpan]) -> f32 {
    if spans.is_empty() {
        return 0.5;
    }
    let confidence_values = spans
        .iter()
        .filter_map(|span| span.average_confidence)
        .collect::<Vec<_>>();
    if confidence_values.is_empty() {
        0.75
    } else {
        let sum: f32 = confidence_values.iter().sum();
        (sum / confidence_values.len() as f32).clamp(0.0, 1.0)
    }
}

fn stable_id(prefix: &str, value: &str) -> String {
    format!("{prefix}:{}", stable_hash(value))
}

fn stable_hash(value: &str) -> String {
    let digest = Sha256::digest(value.as_bytes());
    format!("{digest:x}")
}

fn to_payload<T: Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("semantic observation payload serializes")
}

#[cfg(test)]
mod tests {
    use super::*;

    const USER_ID: &str = "00000000-0000-0000-0000-000000000001";
    const STREAM_ID: &str = "10000000-0000-0000-0000-000000000010";
    const LANE_ID: &str = "20000000-0000-0000-0000-000000000010";

    fn source() -> CaptureSourceRef {
        CaptureSourceRef {
            source_id: "chrome-window-42".to_string(),
            app_name: "Chrome".to_string(),
            bundle_id: Some("com.google.Chrome".to_string()),
            window_id: Some("42".to_string()),
            window_title: Some("ScreenCaptureKit notes".to_string()),
            url: Some("https://example.test/screencapturekit".to_string()),
            frame_ref: None,
            accessibility_ref: None,
            input_event_refs: Vec::new(),
            metadata: json!({}),
        }
    }

    fn span(id: &str, frame: &str, text: &str, y: f32) -> ObservedTextSpan {
        ObservedTextSpan {
            span_id: id.to_string(),
            text: text.to_string(),
            bounds: ObservationRect::new(10.0, y, 420.0, 18.0),
            source_frame_id: frame.to_string(),
            observed_at: "2026-05-24T10:00:00Z".to_string(),
            scroll_offset_y: None,
            confidence: Some(0.92),
            metadata: json!({}),
        }
    }

    #[test]
    fn text_span_deduper_merges_scroll_overlap_without_merging_same_frame_copies() {
        let spans = vec![
            span(
                "a",
                "frame-1",
                "Dirty rects identify updated regions.",
                120.0,
            ),
            span(
                "b",
                "frame-2",
                " Dirty   rects identify updated regions. ",
                80.0,
            ),
            span(
                "c",
                "frame-2",
                "Dirty rects identify updated regions.",
                300.0,
            ),
        ];

        let canonical = canonicalize_text_spans(&spans);

        assert_eq!(canonical.len(), 2);
        assert_eq!(canonical[0].source_span_ids, vec!["a", "b"]);
        assert_eq!(canonical[1].source_span_ids, vec!["c"]);
        assert_ne!(canonical[0].canonical_id, canonical[1].canonical_id);
    }

    #[test]
    fn scroll_detector_requires_same_surface_and_vertical_flow() {
        let decision = detect_scroll_session(&ScrollSessionFeatures {
            same_window: true,
            same_content_pane: true,
            frame_count: 5,
            duration_ms: 1_200,
            dominant_vertical_translation: -42.0,
            stable_chrome_ratio: 0.8,
            new_exposed_strip_ratio: 0.08,
            input_scroll_event_count: 2,
        });

        assert!(decision.is_scroll_session);
        assert!(decision.confidence >= 0.75);

        let not_scroll = detect_scroll_session(&ScrollSessionFeatures {
            same_window: false,
            same_content_pane: true,
            frame_count: 5,
            duration_ms: 1_200,
            dominant_vertical_translation: -42.0,
            stable_chrome_ratio: 0.8,
            new_exposed_strip_ratio: 0.08,
            input_scroll_event_count: 2,
        });

        assert!(!not_scroll.is_scroll_session);
    }

    #[test]
    fn salience_compresses_pixel_motion_but_preserves_semantic_change() {
        let scroll_like = score_salience(&SemanticChangeFeatures {
            pixel_change_ratio: 0.55,
            text_novelty_ratio: 0.01,
            ax_mutation_count: 0,
            dom_mutation_count: 0,
            click_count: 0,
            keypress_count: 0,
            selection_changed: false,
            modal_or_error_visible: false,
            dwell_ms: 500,
        });
        assert_eq!(scroll_like.grade, SalienceGrade::Compress);
        assert!(!scroll_like.preserve_keyframe);

        let semantic_change = score_salience(&SemanticChangeFeatures {
            pixel_change_ratio: 0.02,
            text_novelty_ratio: 0.0,
            ax_mutation_count: 1,
            dom_mutation_count: 0,
            click_count: 0,
            keypress_count: 0,
            selection_changed: false,
            modal_or_error_visible: false,
            dwell_ms: 500,
        });
        assert_eq!(semantic_change.grade, SalienceGrade::Preserve);
        assert!(semantic_change.preserve_keyframe);
    }

    #[test]
    fn scroll_document_and_agent_packet_emit_valid_capture_envelopes() {
        let document = ScrollDocument::from_observed_spans(
            "doc-1",
            source(),
            ObservationTimeRange::new("2026-05-24T10:00:00Z", "2026-05-24T10:00:05Z"),
            &[
                span(
                    "a",
                    "frame-1",
                    "Dirty rects identify updated regions.",
                    120.0,
                ),
                span(
                    "b",
                    "frame-2",
                    "DOMSnapshot returns layout information.",
                    220.0,
                ),
            ],
        );

        let envelope = document.to_capture_envelope(USER_ID, STREAM_ID, LANE_ID, "normal");
        envelope.validate().unwrap();
        assert_eq!(envelope.kind, "scroll_document");
        assert_eq!(
            envelope.schema_name.as_deref(),
            Some(SCROLL_DOCUMENT_SCHEMA)
        );

        let packet = AgentObservationPacket {
            schema_version: 1,
            packet_id: "packet-1".to_string(),
            activity: "User scrolled through a Chrome page about ScreenCaptureKit.".to_string(),
            time_range: ObservationTimeRange::new("2026-05-24T10:00:00Z", "2026-05-24T10:00:05Z"),
            source: source(),
            new_information: vec![
                "Dirty rects identify regions updated from the previous frame.".to_string(),
            ],
            evidence: Vec::new(),
            askable_evidence: vec![
                "show exact frame".to_string(),
                "show full OCR transcript".to_string(),
            ],
            provenance_edges: vec![ObservationProvenanceEdge {
                from_id: "packet-1".to_string(),
                to_id: "doc-1".to_string(),
                relation: "summarizes".to_string(),
                metadata: json!({}),
            }],
            salience: SalienceGrade::Preserve,
            privacy_class: "normal".to_string(),
            metadata: json!({}),
        };

        let packet_envelope = packet.to_capture_envelope(USER_ID, STREAM_ID, LANE_ID);
        packet_envelope.validate().unwrap();
        assert_eq!(packet_envelope.kind, "agent_observation_packet");
        assert_eq!(
            packet_envelope.schema_name.as_deref(),
            Some(AGENT_OBSERVATION_PACKET_SCHEMA)
        );
    }
}
