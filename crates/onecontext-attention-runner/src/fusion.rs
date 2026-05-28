use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use serde_json::{json, Value};

use crate::{
    fixture::AttentionFixture,
    model::{
        AgentAttentionPacket, AlgorithmRunSummary, AlgorithmVote, AttentionFilterOutput,
        AttentionRegion, AttentionSummary, CandidateState, DecisionExplanation, EvidenceRefSummary,
        JsonMap, ProofBundleSummary, ProvenanceRef, RawBufferItem, Rect, SavedAttentionState,
    },
};

const MAX_SAVED_STATES: usize = 8;
const MIN_ATTENTION_SCORE: f32 = 0.68;
const MIN_MEMORY_VALUE_SCORE: f32 = 0.68;
const MIN_COVERAGE_SCORE: f32 = 0.52;
const HARD_KEEP_SPACING_MS: u64 = 1_200;
const SOFT_KEEP_SPACING_MS: u64 = 2_500;
const SAME_CONTEXT_REDUNDANCY_MS: u64 = 4_000;

pub fn fuse_attention(
    fixture: &AttentionFixture,
    mut candidates: Vec<CandidateState>,
) -> Result<AttentionFilterOutput> {
    candidates.sort_by_key(|candidate| candidate.t_ms);

    let mut ranked_indices = (0..candidates.len()).collect::<Vec<_>>();
    ranked_indices.sort_by(|left, right| {
        let left_candidate = &candidates[*left];
        let right_candidate = &candidates[*right];
        right_candidate
            .signals
            .iter()
            .any(|signal| signal.hard_keep.unwrap_or(false))
            .cmp(
                &left_candidate
                    .signals
                    .iter()
                    .any(|signal| signal.hard_keep.unwrap_or(false)),
            )
            .then_with(|| {
                rank_score(right_candidate)
                    .total_cmp(&rank_score(left_candidate))
                    .then_with(|| {
                        right_candidate
                            .memory_value_score
                            .total_cmp(&left_candidate.memory_value_score)
                    })
                    .then_with(|| {
                        right_candidate
                            .attention_score
                            .total_cmp(&left_candidate.attention_score)
                    })
                    .then_with(|| left_candidate.t_ms.cmp(&right_candidate.t_ms))
            })
    });

    let mut saved_indices = Vec::new();
    let mut drop_reasons = BTreeMap::new();

    for index in ranked_indices {
        if saved_indices.len() >= MAX_SAVED_STATES {
            drop_reasons.insert(
                candidates[index].id.clone(),
                "dropped_budget_exhausted".to_string(),
            );
            continue;
        }

        let candidate = &candidates[index];
        if !passes_save_floor(candidate) {
            drop_reasons.insert(
                candidate.id.clone(),
                "dropped_below_attention_and_memory_floor".to_string(),
            );
            continue;
        }

        if let Some(reason) = redundancy_reason(candidate, &saved_indices, &candidates) {
            drop_reasons.insert(candidate.id.clone(), reason);
            continue;
        }

        saved_indices.push(index);
    }

    saved_indices.sort_by_key(|index| candidates[*index].t_ms);
    let saved_states = saved_indices
        .iter()
        .enumerate()
        .map(|(index, candidate_index)| {
            saved_state_from_candidate(index, &candidates[*candidate_index])
        })
        .collect::<Vec<_>>();
    let saved_lookup = saved_indices
        .iter()
        .zip(saved_states.iter())
        .map(|(candidate_index, state)| (candidates[*candidate_index].id.clone(), state.id.clone()))
        .collect::<BTreeMap<_, _>>();

    let raw_buffer_audit = candidates
        .iter()
        .map(|candidate| {
            let saved_state_id = saved_lookup.get(&candidate.id).map(String::as_str);
            let nearest_saved_state_id = nearest_saved_state_id(candidate, &saved_states);
            let decision = saved_state_id
                .map(|_| "saved".to_string())
                .unwrap_or_else(|| {
                    drop_reasons
                        .get(&candidate.id)
                        .cloned()
                        .unwrap_or_else(|| "dropped_not_selected".to_string())
                });
            raw_item_from_candidate(candidate, &decision, nearest_saved_state_id.as_deref())
        })
        .collect::<Vec<_>>();

    let summary = attention_summary(fixture, &candidates, &saved_states);
    let agent_packet = agent_packet(fixture, &summary, &saved_states);
    let policy = policy_json(candidates.len(), saved_states.len());
    let algorithms = vec![algorithm_summary(
        candidates.len(),
        saved_states.len(),
        candidates.len().saturating_sub(saved_states.len()),
        &policy,
        &saved_states,
    )];

    let source_conflicts = source_conflicts(&candidates);

    Ok(AttentionFilterOutput {
        version: "attention-ledger.v3".to_string(),
        capture_id: fixture.session.session_id.clone(),
        time_range_ms: [0, fixture.session.fixture.duration_ms],
        summary,
        saved_states,
        raw_buffer_audit,
        composites: Vec::new(),
        agent_packet,
        source_conflicts,
        attention_debt: Vec::new(),
        algorithms,
        policy,
        provenance_refs: vec![ProvenanceRef {
            id: Some("dashboard-session".to_string()),
            kind: Some("attention_dashboard_session".to_string()),
            path: Some(fixture.session_path.display().to_string()),
            t_ms: None,
        }],
    })
}

fn rank_score(candidate: &CandidateState) -> f32 {
    let hard_keep_bonus = if is_hard_keep(candidate) { 0.3 } else { 0.0 };
    let event_density_bonus = (candidate.nearby_events.len() as f32 * 0.025).min(0.18);
    let signal_diversity_bonus = (signal_kinds(candidate).len() as f32 * 0.02).min(0.12);
    hard_keep_bonus
        + event_density_bonus
        + signal_diversity_bonus
        + scroll_settle_bonus(candidate)
        + candidate.attention_score * 0.42
        + candidate.memory_value_score * 0.58
}

fn scroll_settle_bonus(candidate: &CandidateState) -> f32 {
    let has_scroll_signal = candidate
        .signals
        .iter()
        .any(|signal| signal.kind.starts_with("scroll_") && signal.kind != "scroll_noise");
    if !has_scroll_signal {
        return 0.0;
    }

    candidate
        .nearby_events
        .iter()
        .filter(|event| event.event_type.contains("scroll_burst"))
        .filter_map(|event| {
            if candidate.t_ms < event.t_ms {
                return None;
            }
            let duration_ms = event.duration_ms.unwrap_or(0);
            let end_ms = event.t_ms.saturating_add(duration_ms);
            if candidate.t_ms >= end_ms {
                Some(0.16)
            } else if duration_ms > 0 {
                let progress = (candidate.t_ms - event.t_ms) as f32 / duration_ms as f32;
                Some((progress * 0.12).clamp(0.0, 0.12))
            } else {
                Some(0.04)
            }
        })
        .fold(0.0, f32::max)
}

fn passes_save_floor(candidate: &CandidateState) -> bool {
    is_hard_keep(candidate)
        || candidate.attention_score >= MIN_ATTENTION_SCORE
        || candidate.memory_value_score >= MIN_MEMORY_VALUE_SCORE
        || canonical_signal_classes(candidate)
            .iter()
            .any(|class| class == "transition")
        || rank_score(candidate) >= MIN_COVERAGE_SCORE
}

fn redundancy_reason(
    candidate: &CandidateState,
    saved_indices: &[usize],
    candidates: &[CandidateState],
) -> Option<String> {
    let context = candidate_context(candidate);
    let hard_keep = is_hard_keep(candidate);
    let spacing_ms = if hard_keep {
        HARD_KEEP_SPACING_MS
    } else {
        SOFT_KEEP_SPACING_MS
    };

    for saved_index in saved_indices {
        let saved = &candidates[*saved_index];
        let distance = candidate.t_ms.abs_diff(saved.t_ms);
        if distance < spacing_ms {
            return Some(format!(
                "dropped_time_spacing_{}ms_from_{}",
                distance, saved.id
            ));
        }

        if distance < SAME_CONTEXT_REDUNDANCY_MS {
            let saved_context = candidate_context(saved);
            let same_context = !context.app_name.is_empty()
                && context.app_name == saved_context.app_name
                && context.window_title == saved_context.window_title;
            let same_signal = primary_signal_kind(candidate)
                .zip(primary_signal_kind(saved))
                .is_some_and(|(candidate_signal, saved_signal)| candidate_signal == saved_signal);
            if same_context && same_signal {
                return Some(format!(
                    "dropped_redundant_same_context_{}ms_from_{}",
                    distance, saved.id
                ));
            }

            if same_signal {
                if let Some(shared_ref) = shared_causal_event_ref(candidate, saved) {
                    return Some(format!(
                        "dropped_redundant_shared_event_{}ms_from_{}_via_{}",
                        distance, saved.id, shared_ref
                    ));
                }
            }
        }
    }

    None
}

fn shared_causal_event_ref(candidate: &CandidateState, saved: &CandidateState) -> Option<String> {
    let saved_refs = saved
        .nearby_events
        .iter()
        .filter(|event| is_causal_redundancy_event(&event.event_type))
        .map(event_ref_key)
        .collect::<BTreeSet<_>>();

    candidate
        .nearby_events
        .iter()
        .filter(|event| is_causal_redundancy_event(&event.event_type))
        .map(event_ref_key)
        .find(|event_ref| saved_refs.contains(event_ref))
}

fn is_causal_redundancy_event(event_type: &str) -> bool {
    event_type.contains("keyboard_activity")
        || event_type.contains("pointer")
        || event_type.contains("scroll_burst")
        || event_type.contains("selection")
        || event_type.contains("shortcut")
        || event_type.contains("focus_transition")
        || event_type.contains("visual_frame_change")
}

fn event_ref_key(event: &crate::model::CaptureEvent) -> String {
    format!("{}:{}", event.source_ref, event.source_line)
}

fn saved_state_from_candidate(index: usize, candidate: &CandidateState) -> SavedAttentionState {
    let context = candidate_context(candidate);
    let decision = decision_kind(candidate);
    let mut reasons = candidate
        .signals
        .iter()
        .take(5)
        .map(|signal| signal.explanation.clone())
        .collect::<Vec<_>>();
    if let Some(source_resolution) = &context.source_resolution {
        reasons.push(source_resolution.explanation.clone());
    }
    let title = state_title(&decision, candidate, &context);
    let evidence_refs = evidence_refs(candidate);
    let raw_event_refs = raw_event_refs(candidate, 10);
    let semantic_refs = semantic_refs(candidate);

    SavedAttentionState {
        id: format!("saved-{index:03}"),
        candidate_id: Some(candidate.id.clone()),
        decision: decision.clone(),
        title,
        time_ms: candidate.t_ms,
        duration_ms: None,
        app_name: context.app_name,
        window_title: context.window_title,
        url: context.url,
        active_file: context.active_file,
        terminal_command: context.terminal_command,
        base_screenshot_ref: Some(candidate.image_ref.clone()),
        thumbnail_ref: Some(candidate.image_ref.clone()),
        overlay_regions: overlay_regions(candidate),
        semantic_excerpt: context.semantic_excerpt,
        redaction_summary: context.redaction_summary,
        explanation: DecisionExplanation {
            primary_reason: reasons
                .first()
                .cloned()
                .unwrap_or_else(|| "Candidate preserved as first-pass coverage.".to_string()),
            reasons,
            attention_score: candidate.attention_score,
            memory_value_score: candidate.memory_value_score,
            confidence: confidence(candidate),
            score_components: score_components(candidate),
            algorithm_votes: algorithm_votes(candidate, &decision),
        },
        proof_bundle: ProofBundleSummary {
            proof_tier: Some(proof_tier(candidate).to_string()),
            evidence_refs,
            raw_event_refs,
            screenshot_refs: vec![candidate.image_ref.clone()],
            semantic_refs,
            explanation: Some(
                "Saved by first-pass fusion from screenshot receipt plus nearby raw events."
                    .to_string(),
            ),
        },
        related_composite_ids: Vec::new(),
        related_object_lineage_ids: Vec::new(),
        provenance_refs: provenance_refs(candidate, 10),
    }
}

fn raw_item_from_candidate(
    candidate: &CandidateState,
    decision: &str,
    nearest_saved_state_id: Option<&str>,
) -> RawBufferItem {
    RawBufferItem {
        candidate_id: candidate.id.clone(),
        frame_id: candidate.frame_id.clone(),
        t_ms: candidate.t_ms,
        decision: decision.to_string(),
        thumbnail_ref: candidate.image_ref.clone(),
        nearest_saved_state_id: nearest_saved_state_id.map(str::to_string),
        explanation: raw_explanation(candidate, decision),
        score_components: score_components(candidate),
        top_signals: candidate.signals.iter().take(5).cloned().collect(),
    }
}

fn score_components(candidate: &CandidateState) -> JsonMap {
    let mut map = JsonMap::new();
    map.insert(
        "attention_score".to_string(),
        json!(candidate.attention_score),
    );
    map.insert(
        "memory_value_score".to_string(),
        json!(candidate.memory_value_score),
    );
    map.insert("rank_score".to_string(), json!(rank_score(candidate)));
    map.insert(
        "nearby_events".to_string(),
        json!(candidate.nearby_events.len()),
    );
    map.insert("hard_keep".to_string(), json!(is_hard_keep(candidate)));
    map.insert("signal_kinds".to_string(), json!(signal_kinds(candidate)));
    map.insert(
        "canonical_signal_classes".to_string(),
        json!(canonical_signal_classes(candidate)),
    );
    map.insert(
        "raw_event_types".to_string(),
        json!(event_types(candidate)
            .into_iter()
            .take(8)
            .collect::<Vec<_>>()),
    );
    if let Some(source_resolution) = source_resolution(candidate) {
        map.insert("source_resolution".to_string(), json!(source_resolution));
    }
    map
}

fn policy_json(candidate_count: usize, saved_count: usize) -> JsonMap {
    let mut policy = JsonMap::new();
    policy.insert("policy_id".to_string(), json!("first-pass-fusion.v1"));
    policy.insert("candidate_count".to_string(), json!(candidate_count));
    policy.insert("saved_count".to_string(), json!(saved_count));
    policy.insert("max_saved_states".to_string(), json!(MAX_SAVED_STATES));
    policy.insert(
        "min_attention_score".to_string(),
        json!(MIN_ATTENTION_SCORE),
    );
    policy.insert(
        "min_memory_value_score".to_string(),
        json!(MIN_MEMORY_VALUE_SCORE),
    );
    policy.insert("min_coverage_score".to_string(), json!(MIN_COVERAGE_SCORE));
    policy.insert(
        "hard_keep_spacing_ms".to_string(),
        json!(HARD_KEEP_SPACING_MS),
    );
    policy.insert(
        "soft_keep_spacing_ms".to_string(),
        json!(SOFT_KEEP_SPACING_MS),
    );
    policy.insert(
        "same_context_redundancy_ms".to_string(),
        json!(SAME_CONTEXT_REDUNDANCY_MS),
    );
    policy.insert(
        "ranking".to_string(),
        json!("hard_keeps_first_then_memory_attention_density"),
    );
    policy
}

fn algorithm_summary(
    candidates_considered: usize,
    saved_count: usize,
    dropped_count: usize,
    policy: &JsonMap,
    saved_states: &[SavedAttentionState],
) -> AlgorithmRunSummary {
    AlgorithmRunSummary {
        id: "first-pass-fusion.v1".to_string(),
        name: "First-pass attention fusion".to_string(),
        version: Some("v1".to_string()),
        enabled: Some(true),
        status: Some("experimental".to_string()),
        summary: Some(format!(
            "Considered {candidates_considered} candidates, saved {saved_count}, dropped {dropped_count} with hard-keep, score, spacing, and redundancy policy."
        )),
        explanation: Some(
            "Separates attention_score from memory_value_score, promotes hard keeps first, and preserves raw audit rows for every candidate."
                .to_string(),
        ),
        candidates_considered: Some(candidates_considered),
        saved_count: Some(saved_count),
        merged_count: Some(0),
        dropped_count: Some(dropped_count),
        runtime_ms: None,
        score_components: policy.clone(),
        votes: saved_states
            .iter()
            .flat_map(|state| state.explanation.algorithm_votes.iter().cloned())
            .take(24)
            .collect(),
    }
}

fn attention_summary(
    fixture: &AttentionFixture,
    candidates: &[CandidateState],
    saved_states: &[SavedAttentionState],
) -> AttentionSummary {
    let mut apps_seen = Vec::new();
    let mut windows_seen = Vec::new();
    let mut urls_seen = Vec::new();
    let mut files_seen = Vec::new();
    let mut commands_seen = Vec::new();

    for candidate in candidates {
        let context = candidate_context(candidate);
        push_unique_nonempty(&mut apps_seen, context.app_name);
        push_unique_nonempty(&mut windows_seen, context.window_title);
        push_unique_optional(&mut urls_seen, context.url);
        push_unique_optional(&mut files_seen, context.active_file);
        push_unique_optional(&mut commands_seen, context.terminal_command);
    }

    let activity_summary = if saved_states.is_empty() {
        "No candidate crossed the first-pass attention or memory policy floor.".to_string()
    } else {
        format!(
            "Saved {} attention receipts across {} observed apps from {} candidate frames.",
            saved_states.len(),
            apps_seen.len().max(1),
            candidates.len()
        )
    };

    AttentionSummary {
        activity_label: fixture.session.title.clone(),
        activity_summary,
        confidence: if saved_states.is_empty() { 0.25 } else { 0.58 },
        apps_seen,
        windows_seen,
        urls_seen,
        files_seen,
        commands_seen,
    }
}

fn agent_packet(
    fixture: &AttentionFixture,
    summary: &AttentionSummary,
    saved_states: &[SavedAttentionState],
) -> AgentAttentionPacket {
    let important_observations = saved_states
        .iter()
        .map(|state| {
            json!({
                "kind": observation_kind(&state.decision),
                "summary": observation_summary(state),
                "evidence_state_id": state.id,
                "confidence": state.explanation.confidence,
                "proof_tier": state.proof_bundle.proof_tier.clone().unwrap_or_else(|| "semantic_plus_screenshot".to_string()),
                "time_ms": state.time_ms,
                "decision": state.decision,
                "title": state.title,
            })
        })
        .collect::<Vec<_>>();

    let extracted_text = saved_states
        .iter()
        .filter(|state| !state.semantic_excerpt.is_empty())
        .map(|state| {
            json!({
                "source": "accessibility_semantic_excerpt",
                "text": state.semantic_excerpt,
                "state_ids": [state.id.clone()],
                "confidence": state.explanation.confidence,
                "sensitive": state.redaction_summary.is_some(),
            })
        })
        .collect::<Vec<_>>();

    let askable_evidence = saved_states
        .iter()
        .filter_map(|state| {
            state.base_screenshot_ref.as_ref().map(|path| {
                json!({
                    "label": state.title,
                    "ref": path,
                    "proof_tier": state.proof_bundle.proof_tier.clone().unwrap_or_else(|| "semantic_plus_screenshot".to_string()),
                    "state_id": state.id,
                    "time_ms": state.time_ms,
                })
            })
        })
        .collect::<Vec<_>>();

    AgentAttentionPacket {
        time_range_ms: [0, fixture.session.fixture.duration_ms],
        activity_summary: summary.activity_summary.clone(),
        confidence: summary.confidence,
        important_observations,
        extracted_text,
        composites: Vec::new(),
        askable_evidence,
    }
}

fn nearest_saved_state_id(
    candidate: &CandidateState,
    saved_states: &[SavedAttentionState],
) -> Option<String> {
    saved_states
        .iter()
        .min_by_key(|state| candidate.t_ms.abs_diff(state.time_ms))
        .map(|state| state.id.clone())
}

fn decision_kind(candidate: &CandidateState) -> String {
    let kinds = signal_kinds(candidate);
    let classes = canonical_signal_classes(candidate);
    if kinds.iter().any(|kind| kind.contains("error")) {
        "save_error"
    } else if classes
        .iter()
        .any(|class| class == "selection" || class == "command")
    {
        "save_outcome"
    } else if classes.iter().any(|class| class == "transition") {
        "save_transition"
    } else if candidate.memory_value_score >= MIN_MEMORY_VALUE_SCORE
        && candidate.memory_value_score >= candidate.attention_score
    {
        "save_high_memory"
    } else if kinds
        .iter()
        .any(|kind| kind == "scroll_coverage" || kind == "visual_novelty")
    {
        "save_coverage"
    } else {
        "save_high_attention"
    }
    .to_string()
}

fn state_title(decision: &str, candidate: &CandidateState, context: &CandidateContext) -> String {
    let signal = primary_signal_kind(candidate).unwrap_or_else(|| "attention".to_string());
    let surface = if !context.window_title.is_empty() {
        context.window_title.clone()
    } else if !context.app_name.is_empty() {
        context.app_name.clone()
    } else {
        candidate.frame_id.clone()
    };
    format!(
        "{}: {}",
        decision.replace("save_", "").replace('_', " "),
        surface
    )
    .chars()
    .take(96)
    .collect::<String>()
    .trim()
    .trim_end_matches(':')
    .to_string()
    .if_empty(|| signal)
}

fn observation_kind(decision: &str) -> &'static str {
    match decision {
        "save_high_memory" => "high_memory",
        "save_coverage" => "coverage",
        "save_transition" => "transition",
        "save_outcome" => "outcome",
        "save_error" => "error",
        "save_sensitive_redacted" => "sensitive_redacted",
        _ => "high_attention",
    }
}

fn observation_summary(state: &SavedAttentionState) -> String {
    let surface = if !state.window_title.is_empty() {
        state.window_title.as_str()
    } else if !state.app_name.is_empty() {
        state.app_name.as_str()
    } else {
        state.title.as_str()
    };
    format!(
        "{} at {}ms ({:.2} attention, {:.2} memory)",
        surface,
        state.time_ms,
        state.explanation.attention_score,
        state.explanation.memory_value_score
    )
}

fn raw_explanation(candidate: &CandidateState, decision: &str) -> String {
    let top_signal = primary_signal_kind(candidate).unwrap_or_else(|| "no_signal".to_string());
    let source_note = source_resolution(candidate)
        .map(|resolution| format!(", source winner {}", resolution.winning_source))
        .unwrap_or_default();
    format!(
        "{decision}; attention {:.2}, memory {:.2}, rank {:.2}, {} nearby events, top signal {top_signal}{source_note}",
        candidate.attention_score,
        candidate.memory_value_score,
        rank_score(candidate),
        candidate.nearby_events.len()
    )
}

fn algorithm_votes(candidate: &CandidateState, decision: &str) -> Vec<AlgorithmVote> {
    let mut votes = candidate
        .signals
        .iter()
        .take(6)
        .map(|signal| AlgorithmVote {
            algorithm: signal.algorithm.clone(),
            vote: decision.to_string(),
            reason: signal.explanation.clone(),
            strength: signal.strength,
        })
        .collect::<Vec<_>>();

    votes.push(AlgorithmVote {
        algorithm: "first-pass-fusion.v1".to_string(),
        vote: decision.to_string(),
        reason:
            "Fusion policy selected this candidate after ranking, spacing, and redundancy checks."
                .to_string(),
        strength: rank_score(candidate).min(1.0),
    });
    votes
}

fn confidence(candidate: &CandidateState) -> f32 {
    (0.32
        + candidate.attention_score * 0.25
        + candidate.memory_value_score * 0.3
        + (candidate.nearby_events.len() as f32 * 0.01).min(0.13))
    .min(0.88)
}

fn proof_tier(candidate: &CandidateState) -> &'static str {
    if candidate.nearby_events.is_empty() {
        "visual_required"
    } else if candidate
        .nearby_events
        .iter()
        .any(|event| event.event_type.contains("ax_"))
    {
        "semantic_plus_screenshot"
    } else {
        "visual_required"
    }
}

fn evidence_refs(candidate: &CandidateState) -> Vec<EvidenceRefSummary> {
    let mut refs = vec![EvidenceRefSummary {
        kind: "screenshot".to_string(),
        path: candidate.image_ref.clone(),
        label: Some("candidate frame".to_string()),
        t_ms: Some(candidate.t_ms),
    }];

    for event in candidate.nearby_events.iter().take(4) {
        refs.push(EvidenceRefSummary {
            kind: event.event_type.clone(),
            path: format!("{}:{}", event.source_ref, event.source_line),
            label: Some("nearby raw event".to_string()),
            t_ms: Some(event.t_ms),
        });
    }

    refs
}

fn raw_event_refs(candidate: &CandidateState, limit: usize) -> Vec<String> {
    candidate
        .nearby_events
        .iter()
        .take(limit)
        .map(|event| format!("{}:{}", event.source_ref, event.source_line))
        .collect()
}

fn semantic_refs(candidate: &CandidateState) -> Vec<String> {
    candidate
        .nearby_events
        .iter()
        .filter(|event| event.event_type.contains("ax_"))
        .take(6)
        .map(|event| format!("{}:{}", event.source_ref, event.source_line))
        .collect()
}

fn provenance_refs(candidate: &CandidateState, limit: usize) -> Vec<ProvenanceRef> {
    candidate
        .nearby_events
        .iter()
        .take(limit)
        .map(|event| ProvenanceRef {
            id: Some(event.id.clone()),
            kind: Some(event.event_type.clone()),
            path: Some(format!("{}:{}", event.source_ref, event.source_line)),
            t_ms: Some(event.t_ms),
        })
        .collect()
}

fn overlay_regions(candidate: &CandidateState) -> Vec<AttentionRegion> {
    let signal_regions = candidate
        .signals
        .iter()
        .filter_map(|signal| signal.region.clone())
        .take(4)
        .collect::<Vec<_>>();
    if !signal_regions.is_empty() {
        return signal_regions;
    }

    let focused_regions = candidate
        .nearby_events
        .iter()
        .filter_map(|event| {
            frame_rect_at(&event.payload, &["payload", "focusedElement", "frame"]).map(|bbox| {
                AttentionRegion {
                    bbox,
                    score: candidate.attention_score.max(candidate.memory_value_score),
                    tint: "#35b779".to_string(),
                    label: "focused element".to_string(),
                    explanation: format!("Focused element from {}", event.event_type),
                }
            })
        })
        .take(1)
        .collect::<Vec<_>>();
    if !focused_regions.is_empty() {
        return focused_regions;
    }

    vec![fallback_attention_region(candidate)]
}

fn fallback_attention_region(candidate: &CandidateState) -> AttentionRegion {
    let signal_classes = canonical_signal_classes(candidate);
    let tint = if signal_classes.iter().any(|class| class == "transition") {
        "blue"
    } else if signal_kinds(candidate)
        .iter()
        .any(|kind| kind.contains("selection") || kind.contains("copy"))
    {
        "yellow"
    } else if signal_kinds(candidate)
        .iter()
        .any(|kind| kind.contains("outcome") || kind.contains("command"))
    {
        "purple"
    } else if signal_kinds(candidate)
        .iter()
        .any(|kind| kind.contains("scroll") || kind.contains("coverage"))
    {
        "green"
    } else if signal_kinds(candidate)
        .iter()
        .any(|kind| kind.contains("error"))
    {
        "red"
    } else {
        "orange"
    };

    AttentionRegion {
        bbox: Rect {
            x: 0.04,
            y: 0.08,
            width: 0.92,
            height: 0.84,
        },
        score: candidate.attention_score.max(candidate.memory_value_score),
        tint: tint.to_string(),
        label: "attention receipt".to_string(),
        explanation: "No source-specific region was available; highlight covers the visible content state preserved for end-of-minute review.".to_string(),
    }
}

fn frame_rect_at(value: &Value, path: &[&str]) -> Option<Rect> {
    let frame = value_at(value, path)?;
    Some(Rect {
        x: number_at(frame, &["x"])? as f32,
        y: number_at(frame, &["y"])? as f32,
        width: number_at(frame, &["width"])? as f32,
        height: number_at(frame, &["height"])? as f32,
    })
}

fn is_hard_keep(candidate: &CandidateState) -> bool {
    candidate
        .signals
        .iter()
        .any(|signal| signal.hard_keep.unwrap_or(false))
}

fn signal_kinds(candidate: &CandidateState) -> Vec<String> {
    let mut kinds = Vec::new();
    for signal in &candidate.signals {
        push_unique_nonempty(&mut kinds, signal.kind.clone());
    }
    kinds
}

fn canonical_signal_classes(candidate: &CandidateState) -> Vec<String> {
    let mut classes = Vec::new();
    for signal in &candidate.signals {
        push_unique_nonempty(&mut classes, canonical_signal_class(&signal.kind));
    }
    classes
}

fn canonical_signal_class(kind: &str) -> String {
    if kind.contains("selection") {
        "selection".to_string()
    } else if kind.starts_with("shortcut_command") || kind.contains("_command_") {
        "command".to_string()
    } else if kind == "ax_focused_window_changed"
        || kind == "ux_focus_transition"
        || kind.contains("app_switch")
    {
        "transition".to_string()
    } else if kind.starts_with("visual_") || kind.starts_with("scroll_") {
        "visual_motion".to_string()
    } else if kind.starts_with("ax_focus") || kind == "ax_focused_element_changed" {
        "semantic_focus".to_string()
    } else if kind.contains("source_conflict") {
        "source_conflict".to_string()
    } else {
        kind.to_string()
    }
}

fn event_types(candidate: &CandidateState) -> Vec<String> {
    let mut types = Vec::new();
    for event in &candidate.nearby_events {
        push_unique_nonempty(&mut types, event.event_type.clone());
    }
    types
}

fn primary_signal_kind(candidate: &CandidateState) -> Option<String> {
    candidate
        .signals
        .iter()
        .max_by(|left, right| left.strength.total_cmp(&right.strength))
        .map(|signal| signal.kind.clone())
}

#[derive(Debug, Clone, Default)]
struct CandidateContext {
    app_name: String,
    window_title: String,
    url: Option<String>,
    active_file: Option<String>,
    terminal_command: Option<String>,
    semantic_excerpt: String,
    redaction_summary: Option<String>,
    source_resolution: Option<SourceResolution>,
}

fn candidate_context(candidate: &CandidateState) -> CandidateContext {
    let mut context = CandidateContext::default();
    context.source_resolution = source_resolution(candidate);
    if let Some(source_resolution) = &context.source_resolution {
        context.app_name = source_resolution.app_name.clone().unwrap_or_default();
        context.window_title = source_resolution.window_title.clone().unwrap_or_default();
    }

    let mut events = candidate.nearby_events.iter().collect::<Vec<_>>();
    events.sort_by_key(|event| event.t_ms.abs_diff(candidate.t_ms));

    for event in events {
        let payload = &event.payload;
        if context.app_name.is_empty() {
            context.app_name = first_string(
                payload,
                &[
                    &["payload", "focusedContext", "activeApplication", "appName"],
                    &["payload", "activeApplication", "appName"],
                    &["payload", "target", "appName"],
                    &["payload", "application", "appName"],
                ],
            )
            .unwrap_or_default();
        }
        if context.window_title.is_empty() {
            context.window_title = first_string(
                payload,
                &[
                    &["payload", "focusedContext", "focusedWindow", "title"],
                    &["payload", "focusedWindow", "title"],
                    &["payload", "target", "title"],
                    &["payload", "window", "title"],
                ],
            )
            .unwrap_or_default();
        }
        if context.url.is_none() {
            context.url = first_string(
                payload,
                &[
                    &["payload", "url"],
                    &["payload", "browser", "url"],
                    &["payload", "target", "url"],
                    &["url"],
                ],
            );
        }
        if context.terminal_command.is_none() {
            context.terminal_command = first_string(
                payload,
                &[
                    &["payload", "command"],
                    &["payload", "terminal", "command"],
                    &["payload", "shell", "command"],
                ],
            );
        }
        if context.active_file.is_none() {
            context.active_file = first_string(
                payload,
                &[
                    &["payload", "activeFile"],
                    &["payload", "active_file"],
                    &["payload", "document", "path"],
                    &["payload", "focusedDocument", "path"],
                ],
            )
            .or_else(|| infer_file_from_title(&context.window_title));
        }
        if context.semantic_excerpt.is_empty() {
            context.semantic_excerpt = semantic_excerpt(payload);
        }
        if context.redaction_summary.is_none()
            && (bool_key(payload, "isSensitive") || bool_key(payload, "selectedTextRedacted"))
        {
            context.redaction_summary = Some(
                "One or more nearby accessibility fields were marked sensitive/redacted."
                    .to_string(),
            );
        }
    }

    context
}

#[derive(Debug, Clone)]
struct SourceClaim {
    source: &'static str,
    field: &'static str,
    t_ms: u64,
    confidence: f32,
    app_name: Option<String>,
    bundle_id: Option<String>,
    window_title: Option<String>,
    event_ref: String,
    explanation: String,
}

#[derive(Debug, Clone, serde::Serialize)]
struct SourceResolution {
    field: String,
    value: String,
    winning_source: String,
    confidence: f32,
    app_name: Option<String>,
    bundle_id: Option<String>,
    window_title: Option<String>,
    supporting_sources: Vec<String>,
    conflicting_sources: Vec<String>,
    explanation: String,
}

fn source_resolution(candidate: &CandidateState) -> Option<SourceResolution> {
    let claims = source_claims(candidate);
    let winner = claims
        .iter()
        .filter(|claim| claim.field == "app_window_focus" && claim.app_name.is_some())
        .max_by(|left, right| {
            claim_rank(left)
                .cmp(&claim_rank(right))
                .then_with(|| left.confidence.total_cmp(&right.confidence))
                .then_with(|| {
                    right
                        .t_ms
                        .abs_diff(candidate.t_ms)
                        .cmp(&left.t_ms.abs_diff(candidate.t_ms))
                })
        })?;
    let mut supporting_sources = Vec::new();
    let mut conflicting_sources = Vec::new();

    for claim in claims
        .iter()
        .filter(|claim| claim.field == "app_window_focus" && claim.source != winner.source)
    {
        if claims_agree(winner, claim) {
            push_unique_nonempty(&mut supporting_sources, claim.source.to_string());
        } else if claim.confidence >= 0.50 && winner.confidence >= 0.75 {
            push_unique_nonempty(&mut conflicting_sources, claim.source.to_string());
        }
    }

    for claim in claims.iter().filter(|claim| {
        claim.field == "transition_edge" && candidate.t_ms.abs_diff(claim.t_ms) <= 1_500
    }) {
        push_unique_nonempty(&mut supporting_sources, claim.source.to_string());
    }

    let app_name = winner.app_name.clone();
    let window_title = winner.window_title.clone();
    let value = match (&app_name, &window_title) {
        (Some(app), Some(title)) if !title.is_empty() => format!("{app} / {title}"),
        (Some(app), _) => app.clone(),
        _ => "unknown".to_string(),
    };
    let explanation = format!(
        "source_resolution field=app_window_focus winner={} confidence={:.2}; {}",
        winner.source, winner.confidence, winner.explanation
    );

    Some(SourceResolution {
        field: "app_window_focus".to_string(),
        value,
        winning_source: winner.source.to_string(),
        confidence: winner.confidence,
        app_name,
        bundle_id: winner.bundle_id.clone(),
        window_title,
        supporting_sources,
        conflicting_sources,
        explanation,
    })
}

fn source_conflicts(candidates: &[CandidateState]) -> Vec<Value> {
    let mut conflicts = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate in candidates {
        let claims = source_claims(candidate);
        let Some(resolution) = source_resolution(candidate) else {
            continue;
        };
        let Some(winner) = claims
            .iter()
            .find(|claim| claim.source == resolution.winning_source)
        else {
            continue;
        };

        for claim in claims.iter().filter(|claim| {
            claim.field == "app_window_focus"
                && claim.source != winner.source
                && claim.confidence >= 0.50
                && !claims_agree(winner, claim)
        }) {
            let key = format!(
                "{}:{}:{}:{}:{}",
                candidate.t_ms / 2_000,
                winner.source,
                claim.source,
                claim_label(winner),
                claim_label(claim)
            );
            if !seen.insert(key) {
                continue;
            }
            let expected_lag = claim.source == "sck_active_window"
                || (winner.source == "ax_window_changed" && claim.source == "ax_context");
            conflicts.push(json!({
                "id": format!("conflict-{:03}", conflicts.len()),
                "t_ms": candidate.t_ms,
                "candidate_id": candidate.id,
                "source_a": winner.source,
                "source_b": claim.source,
                "conflict": "app_window_focus",
                "resolution": format!("{} wins app/window focus identity; {} retained according to its source role.", winner.source, claim.source),
                "severity": if expected_lag { "info" } else { "warning" },
                "explanation": format!(
                    "{} reported {} while {} reported {}.",
                    winner.source,
                    claim_label(winner),
                    claim.source,
                    claim_label(claim)
                ),
            }));
        }
    }
    conflicts
}

fn source_claims(candidate: &CandidateState) -> Vec<SourceClaim> {
    let mut claims = Vec::new();
    for event in &candidate.nearby_events {
        let source_ref = format!("{}:{}", event.source_ref, event.source_line);
        match event.event_type.as_str() {
            "capture.ax_focused_context" => {
                let app_name = first_string(
                    &event.payload,
                    &[
                        &["payload", "focusedContext", "activeApplication", "appName"],
                        &["payload", "activeApplication", "appName"],
                    ],
                );
                let window_title = first_string(
                    &event.payload,
                    &[
                        &["payload", "focusedContext", "focusedWindow", "title"],
                        &["payload", "focusedWindow", "title"],
                    ],
                );
                let status = first_string(&event.payload, &[&["payload", "status"]])
                    .unwrap_or_else(|| "unknown".to_string());
                if app_name.is_some() || window_title.is_some() {
                    claims.push(SourceClaim {
                        source: "ax_context",
                        field: "app_window_focus",
                        t_ms: event.t_ms,
                        confidence: if status == "success" || status == "usable" {
                            0.90
                        } else {
                            0.82
                        },
                        app_name,
                        bundle_id: first_string(
                            &event.payload,
                            &[
                                &["payload", "focusedContext", "activeApplication", "bundleID"],
                                &["payload", "activeApplication", "bundleID"],
                            ],
                        ),
                        window_title,
                        event_ref: source_ref,
                        explanation:
                            "AX focused context supplies current semantic app/window focus truth."
                                .to_string(),
                    });
                }
            }
            "capture.active_window_frame_metadata" => {
                let app_name = first_string(&event.payload, &[&["payload", "target", "appName"]]);
                let window_title = first_string(&event.payload, &[&["payload", "target", "title"]]);
                if app_name.is_some() || window_title.is_some() {
                    claims.push(SourceClaim {
                        source: "sck_active_window",
                        field: "app_window_focus",
                        t_ms: event.t_ms,
                        confidence: 0.55,
                        app_name,
                        bundle_id: first_string(
                            &event.payload,
                            &[&["payload", "target", "bundleID"]],
                        ),
                        window_title,
                        event_ref: source_ref,
                        explanation: "SCK active-window metadata is visual receipt/geometry, not focus truth."
                            .to_string(),
                    });
                }
            }
            event_type if event_type.contains("focused_window_changed") => {
                let app_name = first_string(
                    &event.payload,
                    &[&["payload", "activeApplication", "appName"]],
                );
                let window_title =
                    first_string(&event.payload, &[&["payload", "focusedWindow", "title"]]);
                if app_name.is_some() || window_title.is_some() {
                    claims.push(SourceClaim {
                        source: "ax_window_changed",
                        field: "app_window_focus",
                        t_ms: event.t_ms,
                        confidence: 0.94,
                        app_name: app_name.clone(),
                        bundle_id: first_string(
                            &event.payload,
                            &[&["payload", "activeApplication", "bundleID"]],
                        ),
                        window_title: window_title.clone(),
                        event_ref: source_ref.clone(),
                        explanation:
                            "AX focused_window_changed is the winning semantic window-change edge."
                                .to_string(),
                    });
                }
                claims.push(SourceClaim {
                    source: "ax_window_changed",
                    field: "transition_edge",
                    t_ms: event.t_ms,
                    confidence: 0.90,
                    app_name,
                    bundle_id: first_string(
                        &event.payload,
                        &[&["payload", "activeApplication", "bundleID"]],
                    ),
                    window_title,
                    event_ref: source_ref,
                    explanation: "AX focused_window_changed marks an app/window transition."
                        .to_string(),
                });
            }
            event_type if event_type.contains("focus_transition") => {
                claims.push(SourceClaim {
                    source: "ux_focus_transition",
                    field: "transition_edge",
                    t_ms: event.t_ms,
                    confidence: 0.70,
                    app_name: None,
                    bundle_id: None,
                    window_title: None,
                    event_ref: source_ref,
                    explanation:
                        "UX focus_transition contributes input/cause timing, not app/window title truth."
                            .to_string(),
                });
            }
            _ => {}
        }
    }
    claims
}

fn claim_rank(claim: &SourceClaim) -> u8 {
    match claim.source {
        "ax_window_changed" => 6,
        "ax_context" => 5,
        "snapshot_focused_context" => 4,
        "sck_active_window" => 2,
        "snapshot_active_application" => 1,
        _ => 0,
    }
}

fn claims_agree(left: &SourceClaim, right: &SourceClaim) -> bool {
    match (&left.bundle_id, &right.bundle_id) {
        (Some(left_bundle), Some(right_bundle)) if left_bundle == right_bundle => return true,
        (Some(_), Some(_)) => return false,
        _ => {}
    }

    let app_agrees = match (&left.app_name, &right.app_name) {
        (Some(left_app), Some(right_app)) => names_equal(left_app, right_app),
        _ => true,
    };
    let window_agrees = match (&left.window_title, &right.window_title) {
        (Some(left_window), Some(right_window))
            if !left_window.is_empty() && !right_window.is_empty() =>
        {
            names_equal(left_window, right_window)
        }
        _ => true,
    };
    app_agrees && window_agrees
}

fn claim_label(claim: &SourceClaim) -> String {
    match (&claim.app_name, &claim.window_title) {
        (Some(app), Some(title)) if !title.is_empty() => format!("{app} / {title}"),
        (Some(app), _) => app.clone(),
        (_, Some(title)) => title.clone(),
        _ => claim.event_ref.clone(),
    }
}

fn names_equal(left: &str, right: &str) -> bool {
    left.trim().eq_ignore_ascii_case(right.trim())
}

fn semantic_excerpt(value: &Value) -> String {
    let parts = [
        first_string(
            value,
            &[
                &["payload", "focusedElement", "elementDescription"],
                &["payload", "focusedElement", "title"],
            ],
        ),
        first_string(value, &[&["payload", "focusedWindow", "title"]]),
        selected_text(value),
    ]
    .into_iter()
    .flatten()
    .filter(|part| !part.trim().is_empty())
    .collect::<Vec<_>>();

    parts.join(" | ").chars().take(240).collect()
}

fn selected_text(value: &Value) -> Option<String> {
    if bool_key(value, "selectedTextRedacted") {
        return None;
    }
    first_string(
        value,
        &[
            &["payload", "selection", "selectedText"],
            &["payload", "focusedElement", "selection", "selectedText"],
        ],
    )
}

fn infer_file_from_title(title: &str) -> Option<String> {
    let candidate = title
        .split([' ', '-'])
        .find(|part| {
            part.contains('.')
                && part
                    .chars()
                    .all(|char| char.is_ascii_alphanumeric() || "_-./".contains(char))
        })?
        .trim()
        .trim_matches('.');
    (!candidate.is_empty()).then(|| candidate.to_string())
}

fn first_string(value: &Value, paths: &[&[&str]]) -> Option<String> {
    paths.iter().find_map(|path| string_at(value, path))
}

fn string_at(value: &Value, path: &[&str]) -> Option<String> {
    value_at(value, path)?
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn number_at(value: &Value, path: &[&str]) -> Option<f64> {
    value_at(value, path)?.as_f64()
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    Some(cursor)
}

fn bool_key(value: &Value, key: &str) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(candidate_key, candidate_value)| {
            (candidate_key == key && candidate_value.as_bool().unwrap_or(false))
                || bool_key(candidate_value, key)
        }),
        Value::Array(values) => values.iter().any(|value| bool_key(value, key)),
        _ => false,
    }
}

fn push_unique_nonempty(values: &mut Vec<String>, value: String) {
    if !value.is_empty() && !values.contains(&value) {
        values.push(value);
    }
}

fn push_unique_optional(values: &mut Vec<String>, value: Option<String>) {
    if let Some(value) = value {
        push_unique_nonempty(values, value);
    }
}

trait IfEmpty {
    fn if_empty(self, fallback: impl FnOnce() -> String) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: impl FnOnce() -> String) -> String {
        if self.is_empty() {
            fallback()
        } else {
            self
        }
    }
}
