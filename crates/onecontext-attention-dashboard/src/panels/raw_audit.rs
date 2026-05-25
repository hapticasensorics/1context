use eframe::egui;

use crate::schema::{AttentionFilterOutput, DashboardSignal, RawBufferItem};

use super::saved_states::{
    format_time_ms, key_value, render_json_map, render_overlay_regions, score_pill,
};

pub(super) fn render_raw_audit(
    ui: &mut egui::Ui,
    output: &AttentionFilterOutput,
    selected_candidate_id: Option<&str>,
) {
    if output.raw_buffer_audit.is_empty() {
        ui.label("No raw-buffer audit rows yet.");
        return;
    }

    ui.label(format!(
        "{} candidate decisions in the raw-buffer audit",
        output.raw_buffer_audit.len()
    ));

    for item in &output.raw_buffer_audit {
        let selected = selected_candidate_id == Some(item.candidate_id.as_str());
        egui::CollapsingHeader::new(format!(
            "{}  {}  {}",
            format_time_ms(item.t_ms),
            item.candidate_id,
            item.decision
        ))
        .default_open(selected)
        .show(ui, |ui| {
            render_raw_audit_item(ui, item);
        });
    }
}

pub(super) fn render_raw_audit_item(ui: &mut egui::Ui, item: &RawBufferItem) {
    ui.push_id(("raw-audit-item", &item.candidate_id), |ui| {
        render_raw_audit_item_inner(ui, item);
    });
}

fn render_raw_audit_item_inner(ui: &mut egui::Ui, item: &RawBufferItem) {
    key_value(ui, "candidate", &item.candidate_id);
    key_value(ui, "frame", &item.frame_id);
    key_value(ui, "time", &format_time_ms(item.t_ms));
    key_value(ui, "decision", &item.decision);
    key_value(ui, "thumbnail", &item.thumbnail_ref);
    if let Some(nearest) = &item.nearest_saved_state_id {
        key_value(ui, "nearest saved state", nearest);
    }
    if !item.explanation.is_empty() {
        ui.separator();
        ui.label(&item.explanation);
    }

    if !item.score_components.is_empty() {
        ui.collapsing("Score Components", |ui| {
            render_json_map(ui, &item.score_components);
        });
    }

    if !item.top_signals.is_empty() {
        ui.collapsing(format!("Top Signals ({})", item.top_signals.len()), |ui| {
            render_signals(ui, &item.top_signals);
        });
    }

    if !item.extra.is_empty() {
        ui.collapsing("Extra Fields", |ui| {
            render_json_map(ui, &item.extra);
        });
    }
}

pub(super) fn render_signals(ui: &mut egui::Ui, signals: &[DashboardSignal]) {
    for signal in signals {
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(if signal.algorithm.is_empty() {
                    "algorithm"
                } else {
                    &signal.algorithm
                });
                if !signal.kind.is_empty() {
                    ui.label(&signal.kind);
                }
                score_pill(ui, "strength", signal.strength);
                if signal.hard_keep.unwrap_or(false) {
                    ui.colored_label(egui::Color32::from_rgb(54, 145, 86), "hard keep");
                }
            });
            if !signal.explanation.is_empty() {
                ui.small(&signal.explanation);
            }
            if let Some(region) = &signal.region {
                render_overlay_regions(ui, std::slice::from_ref(region));
            }
            if !signal.provenance_refs.is_empty() {
                ui.collapsing("Provenance", |ui| {
                    for provenance in &signal.provenance_refs {
                        let label = provenance
                            .kind
                            .as_deref()
                            .or(provenance.id.as_deref())
                            .unwrap_or("provenance");
                        let value = provenance
                            .path
                            .as_deref()
                            .map(str::to_string)
                            .or_else(|| provenance.t_ms.map(format_time_ms))
                            .unwrap_or_else(|| "-".to_string());
                        key_value(ui, label, &value);
                    }
                });
            }
            if !signal.extra.is_empty() {
                render_json_map(ui, &signal.extra);
            }
        });
    }
}
