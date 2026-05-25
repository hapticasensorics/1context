use eframe::egui;

use crate::review::{ReviewMetricsSummary, ReviewState};

pub fn metrics_panel(ui: &mut egui::Ui, review: &ReviewState) {
    ui.heading("Review Metrics");
    metrics_summary(ui, &review.metrics());
}

pub fn metrics_summary(ui: &mut egui::Ui, summary: &ReviewMetricsSummary) {
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("total labels: {}", summary.total_labels));
        ui.label(format!("must-save: {}", summary.must_save_count));
        ui.label(format!("bad-save: {}", summary.bad_save_count));
        ui.label(format!("missed-save: {}", summary.missed_save_count));
        ui.label(format!("sensitivity: {}", summary.sensitivity_labels_count));
    });

    ui.horizontal_wrapped(|ui| {
        ui.label(format!("too-sensitive: {}", summary.too_sensitive_count));
        ui.label(format!("not-sensitive: {}", summary.not_sensitive_count));
    });

    if summary.counts_by_label.is_empty() {
        ui.small("No human labels recorded yet.");
        return;
    }

    ui.separator();
    ui.label("Counts by label");
    egui::Grid::new("review_metrics_counts_by_label")
        .num_columns(2)
        .striped(true)
        .show(ui, |ui| {
            for (label, count) in &summary.counts_by_label {
                ui.label(label);
                ui.label(count.to_string());
                ui.end_row();
            }
        });
}
