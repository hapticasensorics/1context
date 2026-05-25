pub mod events;
pub mod lanes;

use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
};

use crate::fixture::DashboardFixture;

pub use events::TimelineEvent;
pub use lanes::TimelineLane;

#[derive(Default)]
pub struct TimelineState {
    pub lanes: Vec<TimelineLane>,
    pub events: Vec<TimelineEvent>,
    pub duration_ms: u64,
    pub warnings: Vec<String>,
}

pub struct TimelineViewState {
    pub lane_height: f32,
    pub label_width: f32,
    pub min_event_width: f32,
}

impl TimelineState {
    pub fn from_fixture(fixture: &DashboardFixture) -> Self {
        let lanes: Vec<TimelineLane> = fixture
            .session
            .inputs
            .timeline_lanes
            .iter()
            .map(TimelineLane::from_config)
            .collect();

        let mut events = Vec::new();
        let mut warnings = Vec::new();
        let duration_ms = fixture.duration_ms();
        let base_epoch_ms = manifest_base_epoch_ms(fixture)
            .or_else(|| snapshot_index_base_epoch_ms(fixture))
            .or_else(|| capture_events_base_epoch_ms(fixture));

        if base_epoch_ms.is_none() {
            warnings.push(
                "Could not infer an absolute fixture start time; capture events will pin to 0s."
                    .to_string(),
            );
        }

        add_candidate_frame_events(fixture, &lanes, duration_ms, &mut events);
        for item in &fixture.filter_output.raw_buffer_audit {
            events.push(TimelineEvent {
                id: item.candidate_id.clone(),
                lane_id: "candidate-frames".to_string(),
                t_ms: item.t_ms,
                duration_ms: None,
                title: item.decision.clone(),
                kind: "candidate".to_string(),
                candidate_id: Some(item.candidate_id.clone()),
                saved_state_id: item.nearest_saved_state_id.clone(),
                tooltip: format!(
                    "{}\nframe: {}\ntime: {:.3}s\n{}",
                    item.decision,
                    item.frame_id,
                    item.t_ms as f32 / 1000.0,
                    item.explanation
                ),
                source_ref: Some("attention-filter-output.json".to_string()),
            });
        }
        for state in &fixture.filter_output.saved_states {
            let tooltip = state
                .explanation
                .as_ref()
                .map(|explanation| {
                    let mut lines = vec![
                        state.title.clone(),
                        format!("decision: {}", state.decision),
                        format!("attention: {:.2}", explanation.attention_score),
                        format!("memory: {:.2}", explanation.memory_value_score),
                        explanation.primary_reason.clone(),
                    ];
                    lines.extend(explanation.reasons.iter().cloned());
                    lines.join("\n")
                })
                .unwrap_or_else(|| {
                    format!(
                        "{}\ndecision: {}\ntime: {:.3}s",
                        state.title,
                        state.decision,
                        state.time_ms as f32 / 1000.0
                    )
                });
            events.push(TimelineEvent {
                id: state.id.clone(),
                lane_id: "saved-states".to_string(),
                t_ms: state.time_ms,
                duration_ms: state.duration_ms,
                title: state.title.clone(),
                kind: state.decision.clone(),
                candidate_id: state.candidate_id.clone(),
                saved_state_id: Some(state.id.clone()),
                tooltip,
                source_ref: Some("attention-filter-output.json".to_string()),
            });
        }
        add_attention_debt_events(fixture, &mut events);
        add_capture_event_lanes(fixture, base_epoch_ms, &mut events, &mut warnings);
        add_review_label_events(fixture, &mut events, &mut warnings);

        events.sort_by(|left, right| {
            left.t_ms
                .cmp(&right.t_ms)
                .then_with(|| left.lane_id.cmp(&right.lane_id))
                .then_with(|| left.id.cmp(&right.id))
        });

        Self {
            lanes,
            events,
            duration_ms,
            warnings,
        }
    }

    pub fn lane_event_count(&self, lane_id: &str) -> usize {
        self.events
            .iter()
            .filter(|event| event.lane_id == lane_id)
            .count()
    }

    pub fn visible_lanes(&self) -> impl Iterator<Item = &TimelineLane> {
        self.lanes.iter().filter(|lane| lane.visible)
    }
}

impl Default for TimelineViewState {
    fn default() -> Self {
        Self {
            lane_height: 30.0,
            label_width: 138.0,
            min_event_width: 5.0,
        }
    }
}

fn add_candidate_frame_events(
    fixture: &DashboardFixture,
    lanes: &[TimelineLane],
    duration_ms: u64,
    events: &mut Vec<TimelineEvent>,
) {
    let source_refs = lanes
        .iter()
        .filter(|lane| lane.kind == "candidate_frames")
        .filter_map(|lane| lane.source_ref.as_deref())
        .collect::<BTreeSet<_>>();
    let fallback_set = fixture
        .session
        .media
        .candidate_frame_sets
        .iter()
        .max_by(|left, right| left.fps.total_cmp(&right.fps))
        .map(|set| set.id.as_str());

    for frame_set in &fixture.session.media.candidate_frame_sets {
        let source_matches = source_refs.is_empty()
            || source_refs.contains(frame_set.id.as_str())
            || source_refs.contains(frame_set.root.as_str());
        if !source_matches && Some(frame_set.id.as_str()) != fallback_set {
            continue;
        }

        for index in 1..=frame_set.count {
            let t_ms = (((index - 1) as f64 / frame_set.fps as f64) * 1000.0).round() as u64;
            if t_ms > duration_ms {
                break;
            }
            let frame_id = format!("{}:frame-{index:03}", frame_set.id);
            events.push(TimelineEvent {
                id: format!("candidate-{frame_id}"),
                lane_id: "candidate-frames".to_string(),
                t_ms,
                duration_ms: None,
                title: format!("{} #{index:03}", frame_set.id),
                kind: "candidate_frame".to_string(),
                candidate_id: Some(frame_id.clone()),
                saved_state_id: None,
                tooltip: format!(
                    "candidate frame\nset: {}\nframe: {index:03}\ntime: {:.3}s\nsource: {}",
                    frame_set.id,
                    t_ms as f32 / 1000.0,
                    frame_set.root
                ),
                source_ref: Some(frame_set.root.clone()),
            });
        }
    }
}

fn add_attention_debt_events(fixture: &DashboardFixture, events: &mut Vec<TimelineEvent>) {
    for (index, debt) in fixture.filter_output.attention_debt.iter().enumerate() {
        let t_ms = debt.t_ms.unwrap_or(0);
        let duration_ms = debt
            .extra
            .get("duration_ms")
            .and_then(serde_json::Value::as_u64)
            .filter(|duration_ms| *duration_ms > 0);
        let title = debt
            .description
            .as_deref()
            .or(debt.explanation.as_deref())
            .or(debt.resolution.as_deref())
            .or_else(|| {
                debt.extra
                    .get("title")
                    .or_else(|| debt.extra.get("reason"))
                    .and_then(serde_json::Value::as_str)
            })
            .filter(|title| !title.is_empty())
            .unwrap_or("attention debt")
            .to_string();
        let id = if debt.id.is_empty() {
            format!("attention-debt-{index:03}")
        } else {
            debt.id.clone()
        };
        let status = if debt.status.is_empty() {
            "open"
        } else {
            debt.status.as_str()
        };
        let kind = if debt.kind.is_empty() {
            "attention_debt"
        } else {
            debt.kind.as_str()
        };
        let reason = debt
            .explanation
            .as_deref()
            .or(debt.resolution.as_deref())
            .or_else(|| debt.extra.get("reason").and_then(serde_json::Value::as_str))
            .unwrap_or("");
        let related_candidate = debt.candidate_id.clone().or_else(|| {
            debt.extra
                .get("candidate_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
        let related_saved_state = debt.saved_state_id.clone().or_else(|| {
            debt.extra
                .get("saved_state_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        });
        let title = format!("{status}: {title}");
        let tooltip_extra = serde_json::to_string(&debt.extra).unwrap_or_default();
        events.push(TimelineEvent {
            id,
            lane_id: "attention-debt".to_string(),
            t_ms,
            duration_ms,
            title: title.clone(),
            kind: kind.to_string(),
            candidate_id: related_candidate,
            saved_state_id: related_saved_state,
            tooltip: format!(
                "attention debt\nkind: {kind}\nstatus: {status}\n{title}\n{reason}\n{tooltip_extra}"
            ),
            source_ref: Some("attention-filter-output.json".to_string()),
        });
    }
}

fn add_capture_event_lanes(
    fixture: &DashboardFixture,
    base_epoch_ms: Option<i64>,
    events: &mut Vec<TimelineEvent>,
    warnings: &mut Vec<String>,
) {
    let mut seen_event_ids = HashSet::new();
    for event_ref in &fixture.session.inputs.event_refs {
        if event_ref.kind != "capture_events" {
            continue;
        }
        let path = fixture.resolve_fixture_asset(&event_ref.path);
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) => {
                if event_ref.required {
                    warnings.push(format!(
                        "Could not read required event ref {}: {error}",
                        path.display()
                    ));
                }
                continue;
            }
        };
        let before_count = events.len();
        let mut process_names = BTreeMap::new();
        for (line_index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value = serde_json::from_str::<serde_json::Value>(line).ok();
            if let Some(mut parsed) =
                events::parse_capture_event(line, line_index + 1, base_epoch_ms, &event_ref.path)
            {
                if !seen_event_ids.insert(parsed.event.id.clone()) {
                    continue;
                }
                if let Some(value) = &value {
                    enrich_focus_transition_label(value, &process_names, &mut parsed.event);
                    remember_process_names(value, &mut process_names);
                }
                events.push(parsed.event);
            }
        }
        if events.len() == before_count && event_ref.required {
            warnings.push(format!(
                "No timeline-compatible events parsed from {}",
                path.display()
            ));
        }
    }
}

fn enrich_focus_transition_label(
    value: &serde_json::Value,
    process_names: &BTreeMap<u64, String>,
    event: &mut TimelineEvent,
) {
    if event.lane_id != "focus-transitions" {
        return;
    }
    let previous_pid = json_number_path(
        value,
        &["payload", "focus_transition", "previous_process_id"],
    );
    let current_pid = json_number_path(
        value,
        &["payload", "focus_transition", "current_process_id"],
    )
    .or_else(|| json_number_path(value, &["payload", "recent_target_process_id"]));
    let Some(current_pid) = current_pid else {
        return;
    };

    let previous_label = previous_pid
        .and_then(|pid| process_names.get(&pid).cloned().map(|name| (pid, name)))
        .map(|(_, name)| name)
        .or_else(|| previous_pid.map(|pid| format!("pid {pid}")))
        .unwrap_or_else(|| "pid unknown".to_string());
    let current_label = process_names
        .get(&current_pid)
        .cloned()
        .unwrap_or_else(|| format!("pid {current_pid}"));
    let previous_pid_line = previous_pid
        .map(|pid| format!("previous pid: {pid}"))
        .unwrap_or_else(|| "previous pid: unknown".to_string());

    event.title = format!("focus transition: {previous_label} -> {current_label}");
    event.tooltip = format!(
        "{}\n{}\ncurrent pid: {}\nprevious app: {}\ncurrent app: {}",
        event.tooltip, previous_pid_line, current_pid, previous_label, current_label
    );
}

fn remember_process_names(value: &serde_json::Value, process_names: &mut BTreeMap<u64, String>) {
    remember_process_name(
        value,
        &["payload", "activeApplication", "processID"],
        &["payload", "activeApplication", "appName"],
        process_names,
    );
    remember_process_name(
        value,
        &["payload", "focusedApplicationProcessID"],
        &["payload", "activeApplication", "appName"],
        process_names,
    );
    remember_process_name(
        value,
        &["payload", "target", "appPID"],
        &["payload", "target", "appName"],
        process_names,
    );
}

fn remember_process_name(
    value: &serde_json::Value,
    pid_path: &[&str],
    app_path: &[&str],
    process_names: &mut BTreeMap<u64, String>,
) {
    let Some(pid) = json_number_path(value, pid_path) else {
        return;
    };
    let Some(app_name) = json_string_path(value, app_path).filter(|app_name| !app_name.is_empty())
    else {
        return;
    };
    process_names.insert(pid, app_name.to_string());
}

fn json_number_path(value: &serde_json::Value, path: &[&str]) -> Option<u64> {
    json_value_path(value, path).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
            .or_else(|| {
                value
                    .as_f64()
                    .filter(|value| value.is_finite() && *value >= 0.0)
                    .map(|value| value.round() as u64)
            })
    })
}

fn json_string_path<'a>(value: &'a serde_json::Value, path: &[&str]) -> Option<&'a str> {
    json_value_path(value, path).and_then(serde_json::Value::as_str)
}

fn json_value_path<'a>(
    value: &'a serde_json::Value,
    path: &[&str],
) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn add_review_label_events(
    fixture: &DashboardFixture,
    events: &mut Vec<TimelineEvent>,
    warnings: &mut Vec<String>,
) {
    let path = fixture.review_labels_path();
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
        Err(error) => {
            warnings.push(format!(
                "Could not read review labels {}: {error}",
                path.display()
            ));
            return;
        }
    };

    for (line_index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            warnings.push(format!(
                "Ignored malformed review label line {}",
                line_index + 1
            ));
            continue;
        };
        let (target_kind, target_id, t_ms) =
            review_target_summary(fixture, &value).unwrap_or_else(|| {
                let fallback_ms = value
                    .get("created_at_ms")
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(0)
                    .min(fixture.duration_ms());
                ("target".to_string(), "unknown".to_string(), fallback_ms)
            });
        let label = value
            .get("label")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("review_label");
        events.push(TimelineEvent {
            id: value
                .get("label_id")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("review-label-{line_index:03}")),
            lane_id: "review-labels".to_string(),
            t_ms,
            duration_ms: None,
            title: label.to_string(),
            kind: "review_label".to_string(),
            candidate_id: (target_kind == "candidate").then(|| target_id.to_string()),
            saved_state_id: (target_kind == "saved_state").then(|| target_id.to_string()),
            tooltip: format!("review label\n{label} -> {target_kind}:{target_id}"),
            source_ref: Some(fixture.session.review.labels_ref.clone()),
        });
    }
}

fn review_target_summary(
    fixture: &DashboardFixture,
    value: &serde_json::Value,
) -> Option<(String, String, u64)> {
    let target = value.get("target")?;
    let kind = target.get("kind")?.as_str()?;
    match kind {
        "candidate" => {
            let candidate_id = target.get("candidate_id")?.as_str()?.to_string();
            let t_ms = target
                .get("t_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .min(fixture.duration_ms());
            Some(("candidate".to_string(), candidate_id, t_ms))
        }
        "saved_state" => {
            let saved_state_id = target.get("saved_state_id")?.as_str()?.to_string();
            let t_ms = fixture
                .filter_output
                .saved_states
                .iter()
                .find(|state| state.id == saved_state_id)
                .map(|state| state.time_ms)
                .unwrap_or(0)
                .min(fixture.duration_ms());
            Some(("saved_state".to_string(), saved_state_id, t_ms))
        }
        "time_range" => {
            let start_ms = target
                .get("start_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0)
                .min(fixture.duration_ms());
            let end_ms = target
                .get("end_ms")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(start_ms)
                .min(fixture.duration_ms());
            Some((
                "time_range".to_string(),
                format!("{start_ms}-{end_ms}"),
                start_ms,
            ))
        }
        _ => None,
    }
}

fn snapshot_index_base_epoch_ms(fixture: &DashboardFixture) -> Option<i64> {
    let index_ref = fixture.session.inputs.candidate_index_ref.as_ref()?;
    let text = fs::read_to_string(fixture.resolve_fixture_asset(index_ref)).ok()?;
    text.lines().find_map(|line| {
        let mut fields = line.split('\t');
        fields.next()?;
        let timestamp = fields.next()?;
        events::parse_rfc3339_millis(timestamp)
    })
}

fn manifest_base_epoch_ms(fixture: &DashboardFixture) -> Option<i64> {
    let manifest_path = fixture
        .session
        .fixture
        .source_manifest_ref
        .as_ref()
        .map(|path| fixture.resolve_fixture_asset(path))
        .unwrap_or_else(|| fixture.fixture_root.join("manifest.json"));
    let value = fs::read_to_string(manifest_path)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())?;
    json_string_path(&value, &["time_start"])
        .or_else(|| json_string_path(&value, &["time_range", "start"]))
        .and_then(events::parse_rfc3339_millis)
}

fn capture_events_base_epoch_ms(fixture: &DashboardFixture) -> Option<i64> {
    fixture
        .session
        .inputs
        .event_refs
        .iter()
        .filter(|event_ref| event_ref.kind == "capture_events")
        .filter_map(|event_ref| {
            fs::read_to_string(fixture.resolve_fixture_asset(&event_ref.path)).ok()
        })
        .flat_map(|text| {
            text.lines()
                .filter_map(events::timeline_timestamp_from_line)
                .collect::<Vec<_>>()
        })
        .min()
}
