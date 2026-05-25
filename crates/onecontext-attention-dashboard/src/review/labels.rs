use std::path::PathBuf;

use eframe::egui;
use serde::{Deserialize, Serialize};

use super::{metrics::ReviewMetricsSummary, writer};

const REVIEW_LABEL_SCHEMA_VERSION: &str = "attention-review-label.v1";

pub struct ReviewState {
    labels_path: Option<PathBuf>,
    context: ReviewSessionContext,
    labels: Vec<ReviewLabelEvent>,
    allowed_labels: Vec<String>,
    autosave: bool,
    last_status: Option<String>,
    last_error: Option<String>,
    label_counter: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewLabelEvent {
    pub schema_version: String,
    pub label_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fixture_run_id: Option<String>,
    pub created_at: String,
    pub created_at_ms: u64,
    pub target: ReviewTarget,
    pub label: String,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ReviewTarget {
    #[serde(rename = "candidate")]
    Candidate { candidate_id: String, t_ms: u64 },
    #[serde(rename = "saved_state")]
    SavedState {
        saved_state_id: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        candidate_id: Option<String>,
    },
    #[serde(rename = "time_range")]
    TimeRange { start_ms: u64, end_ms: u64 },
}

#[derive(Debug, Clone)]
struct ReviewSessionContext {
    session_id: String,
    session_title: Option<String>,
    fixture_run_id: Option<String>,
    autosave: bool,
    allowed_labels: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct SessionContextFile {
    session_id: String,
    title: Option<String>,
    fixture: Option<FixtureContextFile>,
    review: Option<ReviewContextFile>,
}

#[derive(Debug, Deserialize)]
struct FixtureContextFile {
    run_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ReviewContextFile {
    autosave: Option<bool>,
    allowed_labels: Option<Vec<String>>,
}

impl ReviewState {
    pub fn new(labels_path: PathBuf) -> Self {
        let context = ReviewSessionContext::from_labels_path(&labels_path);
        let autosave = context.autosave;
        let allowed_labels = context.allowed_labels.clone();
        let (labels, status, error) = match writer::load_labels(&labels_path) {
            Ok(load) => {
                let status = if load.labels.is_empty() {
                    None
                } else {
                    Some(format!("loaded {} review labels", load.labels.len()))
                };
                let error = if load.skipped_errors.is_empty() {
                    None
                } else {
                    Some(format!(
                        "skipped {} malformed review label lines",
                        load.skipped_errors.len()
                    ))
                };
                (load.labels, status, error)
            }
            Err(error) => (
                Vec::new(),
                None,
                Some(format!("could not load labels: {error}")),
            ),
        };
        let label_counter = labels.len() as u64;

        Self {
            labels_path: Some(labels_path),
            context,
            labels,
            allowed_labels,
            autosave,
            last_status: status,
            last_error: error,
            label_counter,
        }
    }

    pub fn metrics(&self) -> ReviewMetricsSummary {
        ReviewMetricsSummary::from_labels(&self.labels)
    }

    pub fn label_buttons(
        &mut self,
        ui: &mut egui::Ui,
        selected_candidate_id: Option<&str>,
        selected_saved_state_id: Option<&str>,
        current_time_ms: u64,
    ) {
        ui.horizontal_wrapped(|ui| {
            for label in self.allowed_labels.clone() {
                if ui.button(&label).clicked() {
                    self.record_label(
                        label,
                        selected_candidate_id,
                        selected_saved_state_id,
                        current_time_ms,
                    );
                }
            }
        });

        if let Some(path) = &self.labels_path {
            ui.small(format!("labels: {}", path.display()));
        }
        ui.small(format!("{} labels recorded", self.labels.len()));
        let metrics = self.metrics();
        ui.small(format!(
            "must-save {} | bad-save {} | missed-save {} | sensitivity {}",
            metrics.must_save_count,
            metrics.bad_save_count,
            metrics.missed_save_count,
            metrics.sensitivity_labels_count
        ));
        if !self.autosave {
            ui.small("autosave disabled: labels are queued in memory");
        }
        if let Some(status) = &self.last_status {
            ui.small(status);
        }
        if let Some(error) = &self.last_error {
            ui.colored_label(egui::Color32::from_rgb(220, 64, 64), error);
        }
    }

    fn record_label(
        &mut self,
        label: String,
        selected_candidate_id: Option<&str>,
        selected_saved_state_id: Option<&str>,
        current_time_ms: u64,
    ) {
        let created_at_ms = current_unix_ms();
        self.label_counter = self.label_counter.saturating_add(1);
        let event = ReviewLabelEvent {
            schema_version: REVIEW_LABEL_SCHEMA_VERSION.to_string(),
            label_id: format!("review-{created_at_ms}-{}", self.label_counter),
            session_id: self.context.session_id.clone(),
            session_title: self.context.session_title.clone(),
            fixture_run_id: self.context.fixture_run_id.clone(),
            created_at: format!("unix-ms:{created_at_ms}"),
            created_at_ms,
            target: target_from_selection(
                selected_candidate_id,
                selected_saved_state_id,
                current_time_ms,
            ),
            label,
            note: None,
        };

        let target_summary = event.target.summary();
        if self.autosave {
            match self.labels_path.as_deref() {
                Some(path) => match writer::append_label(path, &event) {
                    Ok(()) => {
                        self.last_error = None;
                        self.last_status =
                            Some(format!("saved {} -> {target_summary}", event.label));
                        self.labels.push(event);
                    }
                    Err(error) => {
                        self.last_error = Some(format!("could not write label: {error}"));
                        self.last_status = Some(format!("write failed for {}", event.label));
                    }
                },
                None => {
                    self.last_error = Some("no review labels path configured".to_string());
                    self.last_status = Some(format!("write failed for {}", event.label));
                }
            }
        } else {
            self.last_error = None;
            self.last_status = Some(format!("queued {} -> {target_summary}", event.label));
            self.labels.push(event);
        }
    }
}

impl Default for ReviewState {
    fn default() -> Self {
        let context = ReviewSessionContext::default();
        Self {
            labels_path: None,
            allowed_labels: context.allowed_labels.clone(),
            autosave: context.autosave,
            context,
            labels: Vec::new(),
            last_status: None,
            last_error: None,
            label_counter: 0,
        }
    }
}

impl Default for ReviewSessionContext {
    fn default() -> Self {
        Self {
            session_id: "unknown-session".to_string(),
            session_title: None,
            fixture_run_id: None,
            autosave: true,
            allowed_labels: default_allowed_labels(),
        }
    }
}

impl ReviewSessionContext {
    fn from_labels_path(labels_path: &PathBuf) -> Self {
        let Some(session_path) = labels_path
            .parent()
            .map(|parent| parent.join("attention-dashboard-session.json"))
        else {
            return Self::default();
        };

        let Ok(text) = std::fs::read_to_string(session_path) else {
            return Self::default();
        };
        let Ok(file) = serde_json::from_str::<SessionContextFile>(&text) else {
            return Self::default();
        };

        let review = file.review;
        Self {
            session_id: file.session_id,
            session_title: file.title,
            fixture_run_id: file.fixture.and_then(|fixture| fixture.run_id),
            autosave: review
                .as_ref()
                .and_then(|review| review.autosave)
                .unwrap_or(true),
            allowed_labels: review
                .and_then(|review| review.allowed_labels)
                .filter(|labels| !labels.is_empty())
                .unwrap_or_else(default_allowed_labels),
        }
    }
}

impl ReviewTarget {
    fn summary(&self) -> String {
        match self {
            Self::Candidate { candidate_id, .. } => format!("candidate:{candidate_id}"),
            Self::SavedState { saved_state_id, .. } => format!("saved_state:{saved_state_id}"),
            Self::TimeRange { start_ms, end_ms } => {
                format!("time_range:{start_ms}-{end_ms}")
            }
        }
    }
}

fn target_from_selection(
    selected_candidate_id: Option<&str>,
    selected_saved_state_id: Option<&str>,
    current_time_ms: u64,
) -> ReviewTarget {
    if let Some(saved_state_id) = selected_saved_state_id {
        ReviewTarget::SavedState {
            saved_state_id: saved_state_id.to_string(),
            candidate_id: selected_candidate_id.map(str::to_string),
        }
    } else if let Some(candidate_id) = selected_candidate_id {
        ReviewTarget::Candidate {
            candidate_id: candidate_id.to_string(),
            t_ms: current_time_ms,
        }
    } else {
        ReviewTarget::TimeRange {
            start_ms: current_time_ms,
            end_ms: current_time_ms,
        }
    }
}

fn current_unix_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64)
        .unwrap_or(0)
}

fn default_allowed_labels() -> Vec<String> {
    [
        "must_save",
        "good_save",
        "acceptable_drop",
        "bad_save",
        "missed_save",
        "wrong_region",
        "wrong_reason",
        "too_sensitive",
        "not_sensitive",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}
