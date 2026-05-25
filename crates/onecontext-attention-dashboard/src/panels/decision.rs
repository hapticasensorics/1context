use eframe::egui;

use crate::{
    fixture::DashboardFixture,
    review::ReviewState,
    schema::{AttentionFilterOutput, RawBufferItem, SavedAttentionState},
};

use super::{
    agent_packet, raw_audit,
    saved_states::{
        format_time_ms, key_value, render_explanation, render_json_map, render_overlay_regions,
        render_saved_state_detail, score_pill,
    },
};

pub fn decision_panel(
    ui: &mut egui::Ui,
    fixture: &DashboardFixture,
    current_time_ms: u64,
    selected_candidate_id: Option<&str>,
    selected_saved_state_id: Option<&str>,
    review: &mut ReviewState,
) {
    egui::ScrollArea::vertical().show(ui, |ui| {
        render_header(
            ui,
            fixture,
            current_time_ms,
            selected_candidate_id,
            selected_saved_state_id,
        );

        ui.separator();
        egui::CollapsingHeader::new("Current Decision")
            .default_open(true)
            .show(ui, |ui| {
                render_current_decision(
                    ui,
                    &fixture.filter_output,
                    current_time_ms,
                    selected_candidate_id,
                    selected_saved_state_id,
                );
            });

        egui::CollapsingHeader::new("Saved States")
            .default_open(selected_saved_state_id.is_some())
            .show(ui, |ui| {
                super::saved_states::render_saved_states(
                    ui,
                    &fixture.filter_output,
                    selected_saved_state_id,
                );
            });

        egui::CollapsingHeader::new("Raw Buffer Audit")
            .default_open(selected_candidate_id.is_some())
            .show(ui, |ui| {
                raw_audit::render_raw_audit(ui, &fixture.filter_output, selected_candidate_id);
            });

        egui::CollapsingHeader::new("Agent Packet")
            .default_open(false)
            .show(ui, |ui| {
                agent_packet::render_agent_packet(ui, &fixture.filter_output.agent_packet);
            });

        egui::CollapsingHeader::new("Source Conflicts")
            .default_open(false)
            .show(ui, |ui| {
                agent_packet::render_source_conflicts(ui, &fixture.filter_output.source_conflicts);
            });

        egui::CollapsingHeader::new("Attention Debt")
            .default_open(false)
            .show(ui, |ui| {
                agent_packet::render_attention_debt(ui, &fixture.filter_output.attention_debt);
            });

        egui::CollapsingHeader::new("Algorithm Votes")
            .default_open(false)
            .show(ui, |ui| {
                agent_packet::render_algorithms(ui, &fixture.filter_output);
            });

        egui::CollapsingHeader::new("JSON")
            .default_open(false)
            .show(ui, |ui| {
                render_json_panel(ui, &fixture.filter_output);
            });

        ui.separator();
        ui.heading("Review");
        review.label_buttons(
            ui,
            selected_candidate_id,
            selected_saved_state_id,
            current_time_ms,
        );
        egui::CollapsingHeader::new("Review Metrics")
            .default_open(false)
            .show(ui, |ui| {
                super::metrics::metrics_panel(ui, review);
            });
    });
}

fn render_header(
    ui: &mut egui::Ui,
    fixture: &DashboardFixture,
    current_time_ms: u64,
    selected_candidate_id: Option<&str>,
    selected_saved_state_id: Option<&str>,
) {
    ui.heading("Attention Output");
    ui.label(&fixture.session.title);
    if !fixture.filter_output.summary.activity_label.is_empty() {
        ui.small(&fixture.filter_output.summary.activity_label);
    }
    if !fixture.filter_output.summary.activity_summary.is_empty() {
        ui.label(&fixture.filter_output.summary.activity_summary);
    }
    ui.horizontal_wrapped(|ui| {
        score_pill(
            ui,
            "summary confidence",
            fixture.filter_output.summary.confidence,
        );
        ui.label(format!("current: {}", format_time_ms(current_time_ms)));
        ui.label(format!(
            "states: {}",
            fixture.filter_output.saved_states.len()
        ));
        ui.label(format!(
            "audit rows: {}",
            fixture.filter_output.raw_buffer_audit.len()
        ));
    });

    if let Some(candidate_id) = selected_candidate_id {
        key_value(ui, "selected candidate", candidate_id);
    }
    if let Some(saved_state_id) = selected_saved_state_id {
        key_value(ui, "selected saved state", saved_state_id);
    }
}

fn render_current_decision(
    ui: &mut egui::Ui,
    output: &AttentionFilterOutput,
    current_time_ms: u64,
    selected_candidate_id: Option<&str>,
    selected_saved_state_id: Option<&str>,
) {
    let selected_state = selected_saved_state_id
        .and_then(|id| output.saved_states.iter().find(|state| state.id == id))
        .or_else(|| {
            selected_candidate_id.and_then(|id| {
                output
                    .saved_states
                    .iter()
                    .find(|state| state.candidate_id.as_deref() == Some(id))
            })
        });

    let selected_raw = selected_candidate_id
        .and_then(|id| {
            output
                .raw_buffer_audit
                .iter()
                .find(|item| item.candidate_id == id)
        })
        .or_else(|| {
            selected_state.and_then(|state| {
                state.candidate_id.as_deref().and_then(|id| {
                    output
                        .raw_buffer_audit
                        .iter()
                        .find(|item| item.candidate_id == id)
                })
            })
        });

    if let Some(state) = selected_state {
        ui.strong("Selected saved state");
        render_saved_state_detail(ui, state);
        if let Some(raw) = selected_raw {
            ui.separator();
            ui.strong("Source candidate");
            raw_audit::render_raw_audit_item(ui, raw);
        }
        return;
    }

    if let Some(raw) = selected_raw {
        ui.strong("Selected candidate");
        raw_audit::render_raw_audit_item(ui, raw);
        render_nearest_saved_state(ui, output, raw);
        return;
    }

    let nearest_saved = nearest_saved_state(output, current_time_ms);
    let nearest_raw = nearest_raw_item(output, current_time_ms);

    if nearest_saved.is_none() && nearest_raw.is_none() {
        ui.label("No algorithm output is available for this time yet.");
        ui.small("The placeholder fixture still renders, but there is no saved or dropped candidate to inspect.");
        return;
    }

    if let Some(raw) = nearest_raw {
        ui.strong("Nearest candidate decision");
        raw_audit::render_raw_audit_item(ui, raw);
        render_nearest_saved_state(ui, output, raw);
    }

    if let Some(state) = nearest_saved {
        ui.separator();
        ui.strong("Nearest saved state");
        render_saved_state_summary(ui, state);
    }
}

fn render_nearest_saved_state(
    ui: &mut egui::Ui,
    output: &AttentionFilterOutput,
    raw: &RawBufferItem,
) {
    if let Some(id) = &raw.nearest_saved_state_id {
        if let Some(state) = output
            .saved_states
            .iter()
            .find(|state| state.id.as_str() == id.as_str())
        {
            ui.separator();
            ui.strong("Nearest saved state");
            render_saved_state_summary(ui, state);
        }
    }
}

fn render_saved_state_summary(ui: &mut egui::Ui, state: &SavedAttentionState) {
    key_value(ui, "id", &state.id);
    key_value(ui, "decision", &state.decision);
    key_value(ui, "time", &format_time_ms(state.time_ms));
    if !state.title.is_empty() {
        key_value(ui, "title", &state.title);
    }
    if let Some(excerpt) = &state.semantic_excerpt {
        key_value(ui, "excerpt", excerpt);
    }
    if let Some(explanation) = &state.explanation {
        render_explanation(ui, explanation);
    }
    if !state.overlay_regions.is_empty() {
        render_overlay_regions(ui, &state.overlay_regions);
    }
    if !state.extra.is_empty() {
        render_json_map(ui, &state.extra);
    }
}

fn nearest_saved_state<'a>(
    output: &'a AttentionFilterOutput,
    current_time_ms: u64,
) -> Option<&'a SavedAttentionState> {
    output
        .saved_states
        .iter()
        .min_by_key(|state| state.time_ms.abs_diff(current_time_ms))
}

fn nearest_raw_item<'a>(
    output: &'a AttentionFilterOutput,
    current_time_ms: u64,
) -> Option<&'a RawBufferItem> {
    output
        .raw_buffer_audit
        .iter()
        .min_by_key(|item| item.t_ms.abs_diff(current_time_ms))
}

fn render_json_panel(ui: &mut egui::Ui, output: &AttentionFilterOutput) {
    match serde_json::to_string_pretty(output) {
        Ok(json) => {
            ui.monospace(json);
        }
        Err(error) => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 64, 64),
                format!("Could not serialize output JSON: {error}"),
            );
        }
    }
}
