use serde_json::Value;

#[derive(Debug, Clone)]
pub struct TimelineEvent {
    pub id: String,
    pub lane_id: String,
    pub t_ms: u64,
    pub duration_ms: Option<u64>,
    pub title: String,
    pub kind: String,
    pub candidate_id: Option<String>,
    pub saved_state_id: Option<String>,
    pub tooltip: String,
    pub source_ref: Option<String>,
}

impl TimelineEvent {
    pub fn end_ms(&self) -> u64 {
        self.duration_ms
            .map(|duration_ms| self.t_ms.saturating_add(duration_ms))
            .unwrap_or(self.t_ms)
    }

    pub fn tooltip_text(&self) -> String {
        let text = if self.tooltip.is_empty() {
            format!(
                "{}\n{} at {:.3}s",
                self.title,
                self.kind,
                self.t_ms as f32 / 1000.0
            )
        } else {
            self.tooltip.clone()
        };
        match &self.source_ref {
            Some(source_ref) if !source_ref.is_empty() => format!("{text}\nsource: {source_ref}"),
            _ => text,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ParsedCaptureEvent {
    pub event: TimelineEvent,
}

pub fn parse_capture_event(
    line: &str,
    line_number: usize,
    base_epoch_ms: Option<i64>,
    source_ref: &str,
) -> Option<ParsedCaptureEvent> {
    let value: Value = serde_json::from_str(line).ok()?;
    let event_type = value.get("eventType")?.as_str()?.to_string();
    let lane_id = lane_for_event_type(&event_type)?;
    let start_epoch_ms = timestamp_from_value(&value, "event_time_start")
        .or_else(|| timestamp_from_value(&value, "eventTimeStart"))
        .or_else(|| timestamp_path(&value, &["payload", "started_at"]))
        .or_else(|| timestamp_from_value(&value, "recordedAt"));
    let end_epoch_ms = timestamp_from_value(&value, "event_time_end")
        .or_else(|| timestamp_from_value(&value, "eventTimeEnd"))
        .or_else(|| timestamp_path(&value, &["payload", "ended_at"]))
        .or_else(|| timestamp_from_value(&value, "recordedAt"))
        .or(start_epoch_ms);
    let t_ms = relative_ms(start_epoch_ms, base_epoch_ms);
    let duration_ms = match (start_epoch_ms, end_epoch_ms) {
        (Some(start), Some(end)) if end > start => Some((end - start) as u64),
        _ => duration_from_payload(&event_type, &value),
    };
    let title = title_for_event(&event_type, &value);
    let tooltip = tooltip_for_event(&event_type, &value, &title, t_ms, duration_ms);
    let id = value
        .get("source_record_id")
        .or_else(|| value.get("sourceRecordID"))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            format!(
                "capture-event-{line_number:04}-{}",
                compact_kind(&event_type)
            )
        });

    Some(ParsedCaptureEvent {
        event: TimelineEvent {
            id,
            lane_id: lane_id.to_string(),
            t_ms,
            duration_ms,
            title,
            kind: compact_kind(&event_type),
            candidate_id: None,
            saved_state_id: None,
            tooltip,
            source_ref: Some(source_ref.to_string()),
        },
    })
}

pub fn timeline_timestamp_from_line(line: &str) -> Option<i64> {
    let value: Value = serde_json::from_str(line).ok()?;
    let event_type = value.get("eventType")?.as_str()?;
    lane_for_event_type(event_type)?;
    timestamp_from_value(&value, "event_time_start")
        .or_else(|| timestamp_from_value(&value, "eventTimeStart"))
        .or_else(|| timestamp_path(&value, &["payload", "started_at"]))
        .or_else(|| timestamp_from_value(&value, "recordedAt"))
}

pub fn timestamp_from_value(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_millis)
}

pub fn parse_rfc3339_millis(value: &str) -> Option<i64> {
    let value = value.strip_suffix('Z').unwrap_or(value);
    let (date, time) = value.split_once('T')?;
    let mut date_parts = date.split('-');
    let year = date_parts.next()?.parse::<i32>().ok()?;
    let month = date_parts.next()?.parse::<u32>().ok()?;
    let day = date_parts.next()?.parse::<u32>().ok()?;

    let (clock, millis) = match time.split_once('.') {
        Some((clock, fraction)) => {
            let mut digits = fraction
                .chars()
                .take_while(|char| char.is_ascii_digit())
                .take(3)
                .collect::<String>();
            while digits.len() < 3 {
                digits.push('0');
            }
            (clock, digits.parse::<i64>().ok()?)
        }
        None => (time, 0),
    };
    let mut clock_parts = clock.split(':');
    let hour = clock_parts.next()?.parse::<i64>().ok()?;
    let minute = clock_parts.next()?.parse::<i64>().ok()?;
    let second = clock_parts.next()?.parse::<i64>().ok()?;

    let days = days_from_civil(year, month, day);
    Some(days * 86_400_000 + hour * 3_600_000 + minute * 60_000 + second * 1000 + millis)
}

fn lane_for_event_type(event_type: &str) -> Option<&'static str> {
    match event_type {
        "capture.ux.keyboard_activity.v1"
        | "capture.ux.shortcut.v1"
        | "capture.ux.modifiers.v1" => Some("keyboard"),
        "capture.ux.pointer.v1" => Some("pointer"),
        "capture.ux.scroll_burst.v1" => Some("scroll"),
        "capture.ax_semantic.selected_text_changed.v1" | "capture.ax_semantic.value_changed.v1" => {
            Some("selection")
        }
        "capture.ax_semantic.focused_window_changed.v1" => Some("window-changes"),
        "capture.ux.focus_transition.v1" => Some("focus-transitions"),
        "capture.ax_semantic.focused_element_changed.v1" => Some("focused-elements"),
        "capture.ax_focused_context" => Some("focus-samples"),
        "capture.active_window_frame_metadata" => Some("visual-active-window"),
        "attention.derived.visual_frame_change.v1" => Some("visual-frame-changes"),
        _ => None,
    }
}

fn compact_kind(event_type: &str) -> String {
    event_type
        .strip_prefix("capture.")
        .or_else(|| event_type.strip_prefix("attention."))
        .unwrap_or(event_type)
        .strip_suffix(".v1")
        .unwrap_or_else(|| event_type.strip_prefix("capture.").unwrap_or(event_type))
        .replace('.', "_")
}

fn relative_ms(timestamp_epoch_ms: Option<i64>, base_epoch_ms: Option<i64>) -> u64 {
    match (timestamp_epoch_ms, base_epoch_ms) {
        (Some(timestamp), Some(base)) if timestamp >= base => (timestamp - base) as u64,
        (Some(_), Some(_)) => 0,
        _ => 0,
    }
}

fn timestamp_path(value: &Value, path: &[&str]) -> Option<i64> {
    value_path(value, path)
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_millis)
}

fn value_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.get(*key)?;
    }
    Some(current)
}

fn number_path(value: &Value, path: &[&str]) -> Option<f64> {
    value_path(value, path).and_then(Value::as_f64)
}

fn string_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    value_path(value, path).and_then(Value::as_str)
}

fn duration_from_payload(event_type: &str, value: &Value) -> Option<u64> {
    let key_path = match event_type {
        "capture.ux.keyboard_activity.v1" => &["payload", "keyboard_activity", "duration_ms"][..],
        "capture.ux.pointer.v1" => &["payload", "pointer", "duration_ms"][..],
        "capture.ux.scroll_burst.v1" => &["payload", "scroll", "duration_ms"][..],
        "capture.ux.shortcut.v1" => &["payload", "shortcut", "duration_ms"][..],
        _ => return None,
    };
    number_path(value, key_path)
        .filter(|duration| *duration > 0.0)
        .map(|duration| duration.round() as u64)
}

fn title_for_event(event_type: &str, value: &Value) -> String {
    match event_type {
        "capture.ux.keyboard_activity.v1" => {
            let keys = number_path(value, &["payload", "keyboard_activity", "key_down_count"])
                .unwrap_or(0.0) as u64;
            let modified = number_path(
                value,
                &["payload", "keyboard_activity", "modified_key_event_count"],
            )
            .unwrap_or(0.0) as u64;
            if modified > 0 {
                format!("keyboard: {keys} key downs, modified")
            } else {
                format!("keyboard: {keys} key downs")
            }
        }
        "capture.ux.shortcut.v1" => {
            let category = value_path(value, &["payload", "shortcut", "action_categories"])
                .and_then(Value::as_array)
                .and_then(|categories| categories.first())
                .and_then(|category| category.get("category"))
                .and_then(Value::as_str)
                .unwrap_or("shortcut");
            format!("shortcut: {category}")
        }
        "capture.ux.modifiers.v1" => {
            let modifiers = string_list(value, &["payload", "modifiers", "active_modifiers"]);
            if modifiers.is_empty() {
                "modifier released".to_string()
            } else {
                format!("modifier: {}", modifiers.join("+"))
            }
        }
        "capture.ux.pointer.v1" => {
            let action = string_path(value, &["payload", "pointer", "action"]).unwrap_or("pointer");
            let button = string_path(value, &["payload", "pointer", "button"]).unwrap_or("");
            if button.is_empty() {
                format!("pointer: {action}")
            } else {
                format!("pointer: {action} {button}")
            }
        }
        "capture.ux.scroll_burst.v1" => {
            let dy = number_path(value, &["payload", "scroll", "total_dy"]).unwrap_or(0.0);
            format!("scroll dy {dy:.0}")
        }
        "capture.ax_semantic.selected_text_changed.v1" => "selection changed".to_string(),
        "capture.ax_semantic.value_changed.v1" => "value changed".to_string(),
        "capture.ux.focus_transition.v1" => {
            let from = process_label(value, "previous");
            let to = process_label(value, "current");
            format!("focus transition: {from} -> {to}")
        }
        "capture.active_window_frame_metadata" => {
            let app = string_path(value, &["payload", "target", "appName"]).unwrap_or("window");
            format!("sck target: {app}")
        }
        "attention.derived.visual_frame_change.v1" => {
            let full = number_path(value, &["payload", "full_diff_score"]).unwrap_or(0.0);
            let top = number_path(value, &["payload", "top_band_diff_score"]).unwrap_or(0.0);
            format!("visual change: full {full:.2}, top {top:.2}")
        }
        "capture.ax_semantic.focused_window_changed.v1" => {
            format!("window changed: {}", app_window_label(value))
        }
        "capture.ax_semantic.focused_element_changed.v1" => {
            let role = string_path(value, &["payload", "focusedElement", "role"])
                .or_else(|| string_path(value, &["payload", "focusedElement", "subrole"]))
                .unwrap_or("element");
            format!("focused element: {role}")
        }
        "capture.ax_focused_context" => format!("focus sample: {}", app_label(value)),
        _ => compact_kind(event_type),
    }
}

fn tooltip_for_event(
    event_type: &str,
    value: &Value,
    title: &str,
    t_ms: u64,
    duration_ms: Option<u64>,
) -> String {
    let mut lines = vec![
        title.to_string(),
        format!("kind: {}", compact_kind(event_type)),
        format!("time: {:.3}s", t_ms as f32 / 1000.0),
    ];
    if let Some(duration_ms) = duration_ms {
        lines.push(format!("duration: {duration_ms}ms"));
    }
    if let Some(app) = string_path(value, &["payload", "activeApplication", "appName"])
        .or_else(|| string_path(value, &["payload", "target", "appName"]))
    {
        lines.push(format!("app: {app}"));
    }
    if let Some(pid) = number_path(value, &["payload", "activeApplication", "processID"])
        .or_else(|| number_path(value, &["payload", "target", "appPID"]))
        .or_else(|| number_path(value, &["payload", "focusedApplicationProcessID"]))
    {
        lines.push(format!("pid: {:.0}", pid));
    }
    if let Some(bundle_id) = string_path(value, &["payload", "activeApplication", "bundleID"])
        .or_else(|| string_path(value, &["payload", "target", "bundleID"]))
    {
        lines.push(format!("bundle: {bundle_id}"));
    }
    if let Some(title) = string_path(value, &["payload", "focusedWindow", "title"])
        .or_else(|| string_path(value, &["payload", "target", "title"]))
    {
        lines.push(format!("window: {title}"));
    }
    if let Some(window_id) = number_path(value, &["payload", "matchedWindowID"])
        .or_else(|| number_path(value, &["payload", "target", "windowID"]))
    {
        lines.push(format!("window id: {:.0}", window_id));
    }
    if let Some(role) = string_path(value, &["payload", "focusedElement", "role"]) {
        lines.push(format!("focused role: {role}"));
    }
    if let Some(description) =
        string_path(value, &["payload", "focusedElement", "elementDescription"])
            .or_else(|| string_path(value, &["payload", "focusedElement", "title"]))
            .filter(|description| !description.is_empty())
    {
        lines.push(format!("focused element: {description}"));
    }
    if event_type == "capture.ux.focus_transition.v1" {
        lines.extend(focus_transition_tooltip_lines(value));
    }
    if event_type == "capture.ux.scroll_burst.v1" {
        let events = number_path(value, &["payload", "scroll", "event_count"]).unwrap_or(0.0);
        let momentum =
            number_path(value, &["payload", "scroll", "momentum_event_count"]).unwrap_or(0.0);
        lines.push(format!(
            "scroll events: {events:.0}, momentum: {momentum:.0}"
        ));
    }
    if event_type == "attention.derived.visual_frame_change.v1" {
        if let Some(from_frame) = number_path(value, &["payload", "from_frame"]) {
            lines.push(format!("from frame: {:.0}", from_frame));
        }
        if let Some(to_frame) = number_path(value, &["payload", "to_frame"]) {
            lines.push(format!("to frame: {:.0}", to_frame));
        }
        if let Some(reason) = string_path(value, &["payload", "reason"]) {
            lines.push(format!("reason: {reason}"));
        }
    }
    lines.join("\n")
}

fn app_label(value: &Value) -> String {
    string_path(value, &["payload", "activeApplication", "appName"])
        .or_else(|| string_path(value, &["payload", "target", "appName"]))
        .unwrap_or("focus")
        .to_string()
}

fn app_window_label(value: &Value) -> String {
    let app = string_path(value, &["payload", "activeApplication", "appName"])
        .or_else(|| string_path(value, &["payload", "target", "appName"]))
        .unwrap_or("focus");
    let window = string_path(value, &["payload", "focusedWindow", "title"])
        .or_else(|| string_path(value, &["payload", "target", "title"]))
        .unwrap_or("");
    if window.is_empty() {
        app.to_string()
    } else {
        format!("{app} - {window}")
    }
}

fn process_label(value: &Value, direction: &str) -> String {
    let transition_path = ["payload", "focus_transition"];
    let target_key = format!("{direction}_target");
    let pid_key = format!("{direction}_process_id");
    let target_pid_path = [
        transition_path[0],
        transition_path[1],
        target_key.as_str(),
        "process_id",
    ];
    let target_app_path = [
        transition_path[0],
        transition_path[1],
        target_key.as_str(),
        "appName",
    ];
    let pid_path = [transition_path[0], transition_path[1], pid_key.as_str()];

    if let Some(app) = string_path(value, &target_app_path) {
        return app.to_string();
    }
    number_path(value, &pid_path)
        .or_else(|| number_path(value, &target_pid_path))
        .map(|pid| format!("pid {:.0}", pid))
        .unwrap_or_else(|| "pid unknown".to_string())
}

fn focus_transition_tooltip_lines(value: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(pid) = number_path(
        value,
        &["payload", "focus_transition", "previous_process_id"],
    ) {
        lines.push(format!("previous pid: {:.0}", pid));
    }
    if let Some(pid) = number_path(
        value,
        &["payload", "focus_transition", "current_process_id"],
    )
    .or_else(|| number_path(value, &["payload", "recent_target_process_id"]))
    {
        lines.push(format!("current pid: {:.0}", pid));
    }
    if let Some(trigger) = string_path(value, &["payload", "focus_transition", "trigger"]) {
        lines.push(format!("trigger: {trigger}"));
    }
    if let Some(confidence) = string_path(value, &["payload", "focus_transition", "confidence"]) {
        lines.push(format!("confidence: {confidence}"));
    }
    if let Some(source) = string_path(value, &["payload", "focus_transition", "target_source"]) {
        lines.push(format!("target source: {source}"));
    }
    lines
}

fn string_list(value: &Value, path: &[&str]) -> Vec<String> {
    value_path(value, path)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - i32::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i32;
    let day = day as i32;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    i64::from(era * 146_097 + day_of_era - 719_468)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_event(line: &str) -> TimelineEvent {
        parse_capture_event(line, 1, None, "fixture.events.jsonl")
            .expect("event should parse")
            .event
    }

    #[test]
    fn maps_window_focus_sources_to_split_lanes() {
        let cases = [
            (
                r#"{"eventType":"capture.ax_semantic.focused_window_changed.v1","payload":{"activeApplication":{"appName":"Code","processID":28730},"focusedWindow":{"title":"notes.md"}},"recordedAt":"2026-05-25T00:00:00.000Z"}"#,
                "window-changes",
                "window changed: Code - notes.md",
            ),
            (
                r#"{"eventType":"capture.ux.focus_transition.v1","payload":{"focus_transition":{"previous_process_id":28730,"current_process_id":19780,"trigger":"pointer","confidence":"high"}},"recordedAt":"2026-05-25T00:00:00.000Z"}"#,
                "focus-transitions",
                "focus transition: pid 28730 -> pid 19780",
            ),
            (
                r#"{"eventType":"capture.ax_semantic.focused_element_changed.v1","payload":{"activeApplication":{"appName":"Chrome"},"focusedElement":{"role":"AXTextArea"}},"recordedAt":"2026-05-25T00:00:00.000Z"}"#,
                "focused-elements",
                "focused element: AXTextArea",
            ),
            (
                r#"{"eventType":"capture.ax_focused_context","payload":{"activeApplication":{"appName":"Chrome"}},"recordedAt":"2026-05-25T00:00:00.000Z"}"#,
                "focus-samples",
                "focus sample: Chrome",
            ),
            (
                r#"{"eventType":"capture.active_window_frame_metadata","payload":{"target":{"appName":"Codex","title":"Codex"}},"recordedAt":"2026-05-25T00:00:00.000Z"}"#,
                "visual-active-window",
                "sck target: Codex",
            ),
            (
                r#"{"eventType":"attention.derived.visual_frame_change.v1","payload":{"full_diff_score":0.24,"top_band_diff_score":0.31,"from_frame":66,"to_frame":67},"recordedAt":"2026-05-25T00:00:00.000Z"}"#,
                "visual-frame-changes",
                "visual change: full 0.24, top 0.31",
            ),
        ];

        for (line, expected_lane, expected_title) in cases {
            let event = parse_event(line);
            assert_eq!(event.lane_id, expected_lane);
            assert_eq!(event.title, expected_title);
        }
    }

    #[test]
    fn focus_transition_tooltip_exposes_pid_context() {
        let event = parse_event(
            r#"{"eventType":"capture.ux.focus_transition.v1","payload":{"focus_transition":{"previous_process_id":28730,"current_process_id":19780,"trigger":"pointer","confidence":"high","target_source":"cg_event_target_pid"}},"recordedAt":"2026-05-25T00:00:00.000Z"}"#,
        );

        assert!(event.tooltip.contains("previous pid: 28730"));
        assert!(event.tooltip.contains("current pid: 19780"));
        assert!(event.tooltip.contains("trigger: pointer"));
        assert!(event.tooltip.contains("confidence: high"));
    }
}
