use std::collections::BTreeMap;

use super::labels::ReviewLabelEvent;

#[derive(Debug, Clone, Default)]
pub struct ReviewMetricsSummary {
    pub total_labels: usize,
    pub counts_by_label: BTreeMap<String, usize>,
    pub bad_save_count: usize,
    pub missed_save_count: usize,
    pub must_save_count: usize,
    pub too_sensitive_count: usize,
    pub not_sensitive_count: usize,
    pub sensitivity_labels_count: usize,
}

impl ReviewMetricsSummary {
    pub fn from_labels(labels: &[ReviewLabelEvent]) -> Self {
        let mut summary = Self {
            total_labels: labels.len(),
            ..Self::default()
        };

        for label in labels {
            *summary
                .counts_by_label
                .entry(label.label.clone())
                .or_default() += 1;

            match label.label.as_str() {
                "bad_save" => summary.bad_save_count += 1,
                "missed_save" => summary.missed_save_count += 1,
                "must_save" => summary.must_save_count += 1,
                "too_sensitive" => {
                    summary.too_sensitive_count += 1;
                    summary.sensitivity_labels_count += 1;
                }
                "not_sensitive" => {
                    summary.not_sensitive_count += 1;
                    summary.sensitivity_labels_count += 1;
                }
                _ => {}
            }
        }

        summary
    }
}
