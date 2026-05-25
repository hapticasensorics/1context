use eframe::egui;

use crate::schema::{
    AttentionFilterOutput, AttentionRegion, DecisionExplanation, SavedAttentionState,
};

pub(super) fn render_saved_states(
    ui: &mut egui::Ui,
    output: &AttentionFilterOutput,
    selected_saved_state_id: Option<&str>,
) {
    if output.saved_states.is_empty() {
        ui.label(
            "No saved states yet. The placeholder output is waiting for the algorithm harness.",
        );
        return;
    }

    for state in &output.saved_states {
        let selected = selected_saved_state_id == Some(state.id.as_str());
        let title = state_title(state);
        egui::CollapsingHeader::new(format!(
            "{}  {:.2}s  {}",
            decision_label(&state.decision),
            state.time_ms as f32 / 1000.0,
            title
        ))
        .default_open(selected)
        .show(ui, |ui| {
            render_saved_state_detail(ui, state);
        });
    }
}

pub(super) fn render_saved_state_detail(ui: &mut egui::Ui, state: &SavedAttentionState) {
    ui.push_id(("saved-state-detail", &state.id), |ui| {
        render_saved_state_detail_inner(ui, state);
    });
}

fn render_saved_state_detail_inner(ui: &mut egui::Ui, state: &SavedAttentionState) {
    key_value(ui, "id", &state.id);
    if let Some(candidate_id) = &state.candidate_id {
        key_value(ui, "candidate", candidate_id);
    }
    key_value(ui, "decision", &state.decision);
    key_value(
        ui,
        "time",
        &format_duration(state.time_ms, state.duration_ms),
    );

    if let Some(app_name) = &state.app_name {
        key_value(ui, "app", app_name);
    }
    if let Some(window_title) = &state.window_title {
        key_value(ui, "window", window_title);
    }
    if let Some(url) = &state.url {
        key_value(ui, "url", url);
    }
    if let Some(active_file) = &state.active_file {
        key_value(ui, "file", active_file);
    }
    if let Some(command) = &state.terminal_command {
        key_value(ui, "command", command);
    }
    if let Some(path) = &state.thumbnail_ref {
        key_value(ui, "thumbnail", path);
    }
    if let Some(path) = &state.base_screenshot_ref {
        key_value(ui, "screenshot", path);
    }

    if let Some(excerpt) = &state.semantic_excerpt {
        ui.separator();
        ui.strong("Semantic Excerpt");
        ui.label(excerpt);
    }
    if let Some(summary) = &state.redaction_summary {
        ui.separator();
        ui.strong("Redaction");
        ui.label(summary);
    }

    if let Some(explanation) = &state.explanation {
        ui.separator();
        render_explanation(ui, explanation);
    }

    if !state.overlay_regions.is_empty() {
        ui.separator();
        render_overlay_regions(ui, &state.overlay_regions);
    }

    if let Some(proof_bundle) = &state.proof_bundle {
        ui.separator();
        ui.strong("Proof Bundle");
        if let Some(tier) = &proof_bundle.proof_tier {
            key_value(ui, "tier", tier);
        }
        if let Some(explanation) = &proof_bundle.explanation {
            ui.label(explanation);
        }
        for evidence in &proof_bundle.evidence_refs {
            let label = evidence
                .label
                .as_deref()
                .or(evidence.id.as_deref())
                .unwrap_or("evidence");
            let path = evidence.path.as_deref().unwrap_or("");
            key_value(ui, label, path);
        }
        render_string_list(ui, "raw events", &proof_bundle.raw_event_refs);
        render_string_list(ui, "screenshots", &proof_bundle.screenshot_refs);
        render_string_list(ui, "semantic refs", &proof_bundle.semantic_refs);
    }

    render_string_list(ui, "composites", &state.related_composite_ids);
    render_string_list(ui, "object lineage", &state.related_object_lineage_ids);

    if !state.extra.is_empty() {
        ui.separator();
        ui.strong("Extra Fields");
        render_json_map(ui, &state.extra);
    }
}

pub(super) fn render_explanation(ui: &mut egui::Ui, explanation: &DecisionExplanation) {
    if !explanation.primary_reason.is_empty() {
        ui.strong(&explanation.primary_reason);
    }
    if !explanation.reasons.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for reason in &explanation.reasons {
                ui.label(format!("[{reason}]"));
            }
        });
    }

    ui.horizontal_wrapped(|ui| {
        score_pill(ui, "attention", explanation.attention_score);
        score_pill(ui, "memory", explanation.memory_value_score);
        score_pill(ui, "confidence", explanation.confidence);
    });

    if !explanation.score_components.is_empty() {
        ui.collapsing("Score Components", |ui| {
            render_json_map(ui, &explanation.score_components);
        });
    }

    if !explanation.algorithm_votes.is_empty() {
        ui.collapsing("Algorithm Votes", |ui| {
            for vote in &explanation.algorithm_votes {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(if vote.algorithm.is_empty() {
                        "algorithm"
                    } else {
                        &vote.algorithm
                    });
                    ui.label(if vote.vote.is_empty() {
                        "vote"
                    } else {
                        &vote.vote
                    });
                    score_pill(ui, "strength", vote.strength);
                });
                if !vote.reason.is_empty() {
                    ui.small(&vote.reason);
                }
            }
        });
    }

    if !explanation.source_conflicts.is_empty() {
        ui.collapsing("Source Conflicts", |ui| {
            for conflict in &explanation.source_conflicts {
                ui.monospace(json_preview(conflict));
            }
        });
    }

    if !explanation.extra.is_empty() {
        ui.collapsing("Extra Explanation Fields", |ui| {
            render_json_map(ui, &explanation.extra);
        });
    }
}

pub(super) fn render_overlay_regions(ui: &mut egui::Ui, regions: &[AttentionRegion]) {
    ui.strong(format!("Overlay Regions ({})", regions.len()));
    for (index, region) in regions.iter().enumerate() {
        let label = if region.label.is_empty() {
            format!("region {}", index + 1)
        } else {
            region.label.clone()
        };
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(label);
                if !region.tint.is_empty() {
                    ui.label(format!("tint: {}", region.tint));
                }
                score_pill(ui, "score", region.score);
                if region.sensitive.unwrap_or(false) {
                    ui.colored_label(egui::Color32::from_rgb(210, 64, 64), "sensitive");
                }
            });
            if let Some(bbox) = &region.bbox {
                key_value(
                    ui,
                    "bbox",
                    &format!(
                        "{:.1}, {:.1}, {:.1} x {:.1} {}",
                        bbox.x, bbox.y, bbox.width, bbox.height, bbox.coordinate_space
                    ),
                );
            }
            render_string_list(ui, "reasons", &region.reasons);
            render_string_list(ui, "evidence", &region.evidence);
            if !region.extra.is_empty() {
                render_json_map(ui, &region.extra);
            }
        });
    }
}

pub(super) fn score_pill(ui: &mut egui::Ui, label: &str, score: f32) {
    let clamped = score.clamp(0.0, 1.0);
    let color = if clamped >= 0.75 {
        egui::Color32::from_rgb(54, 145, 86)
    } else if clamped >= 0.4 {
        egui::Color32::from_rgb(180, 126, 38)
    } else {
        egui::Color32::from_rgb(120, 120, 120)
    };
    ui.colored_label(color, format!("{label}: {:.2}", score));
}

pub(super) fn key_value(ui: &mut egui::Ui, key: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.small(format!("{key}:"));
        ui.label(value);
    });
}

pub(super) fn render_string_list(ui: &mut egui::Ui, label: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    ui.collapsing(format!("{label} ({})", values.len()), |ui| {
        for value in values {
            ui.label(value);
        }
    });
}

pub(super) fn render_json_map(ui: &mut egui::Ui, map: &serde_json::Map<String, serde_json::Value>) {
    for (key, value) in map {
        key_value(ui, key, &json_preview(value));
    }
}

pub(super) fn json_preview(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "<json>".to_string()),
    }
}

pub(super) fn format_time_ms(t_ms: u64) -> String {
    format!("{:.2}s", t_ms as f32 / 1000.0)
}

fn format_duration(time_ms: u64, duration_ms: Option<u64>) -> String {
    if let Some(duration_ms) = duration_ms {
        format!(
            "{} + {}",
            format_time_ms(time_ms),
            format_time_ms(duration_ms)
        )
    } else {
        format_time_ms(time_ms)
    }
}

fn state_title(state: &SavedAttentionState) -> String {
    if !state.title.is_empty() {
        state.title.clone()
    } else if !state.id.is_empty() {
        state.id.clone()
    } else {
        "untitled state".to_string()
    }
}

fn decision_label(decision: &str) -> &str {
    match decision {
        "save_high_attention" => "attention",
        "save_high_memory" => "memory",
        "save_coverage" => "coverage",
        "save_transition" => "transition",
        "save_outcome" => "outcome",
        "save_error" => "error",
        "save_sensitive_redacted" => "redacted",
        _ if decision.is_empty() => "state",
        _ => decision,
    }
}
