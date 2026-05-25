use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::Path,
};

use anyhow::{Context, Result};
use serde_json::Value;

use crate::model::{CaptureEvent, EventRef};

pub fn load_capture_events(
    root: &Path,
    event_ref: &EventRef,
    base_epoch_ms: Option<i64>,
) -> Result<Vec<CaptureEvent>> {
    let path = root.join(&event_ref.path);
    let file = File::open(&path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut fallback_base_epoch_ms = None;

    for (index, line) in reader.lines().enumerate() {
        let line = line.with_context(|| format!("read {} line {}", path.display(), index + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(&line)
            .with_context(|| format!("parse {} line {}", path.display(), index + 1))?;
        let epoch_ms = event_epoch_ms(&value);
        fallback_base_epoch_ms = fallback_base_epoch_ms.or(epoch_ms);
        let event_base_epoch_ms = base_epoch_ms.or(fallback_base_epoch_ms);
        let t_ms = match (epoch_ms, event_base_epoch_ms) {
            (Some(epoch), Some(base)) if epoch >= base => (epoch - base) as u64,
            _ => 0,
        };
        let end_ms = event_end_epoch_ms(&value);
        let duration_ms = match (epoch_ms, end_ms) {
            (Some(start), Some(end)) if end > start => Some((end - start) as u64),
            _ => duration_from_payload(&event_type(&value), &value),
        };
        let event_type = event_type(&value);
        let id = value
            .get("source_record_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{}:{}", event_ref.id, index + 1));

        events.push(CaptureEvent {
            id,
            event_type,
            t_ms,
            duration_ms,
            payload: value,
            source_ref: event_ref.path.clone(),
            source_line: index + 1,
        });
    }

    Ok(events)
}

pub fn event_epoch_ms(value: &Value) -> Option<i64> {
    timestamp_from_key(value, "event_time_start")
        .or_else(|| timestamp_from_path(value, &["payload", "started_at"]))
        .or_else(|| timestamp_from_key(value, "recordedAt"))
}

fn event_end_epoch_ms(value: &Value) -> Option<i64> {
    timestamp_from_key(value, "event_time_end")
        .or_else(|| timestamp_from_path(value, &["payload", "ended_at"]))
        .or_else(|| timestamp_from_key(value, "recordedAt"))
}

fn event_type(value: &Value) -> String {
    value
        .get("eventType")
        .and_then(Value::as_str)
        .unwrap_or("capture.unknown")
        .to_string()
}

fn duration_from_payload(event_type: &str, value: &Value) -> Option<u64> {
    if event_type.contains("keyboard_activity") {
        number_from_path(value, &["payload", "keyboard_activity", "duration_ms"])
    } else if event_type.contains("pointer") {
        number_from_path(value, &["payload", "pointer", "duration_ms"])
    } else if event_type.contains("scroll_burst") {
        number_from_path(value, &["payload", "scroll", "duration_ms"])
    } else {
        None
    }
}

fn timestamp_from_key(value: &Value, key: &str) -> Option<i64> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(parse_rfc3339_ms)
}

fn timestamp_from_path(value: &Value, path: &[&str]) -> Option<i64> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_str().and_then(parse_rfc3339_ms)
}

fn number_from_path(value: &Value, path: &[&str]) -> Option<u64> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_u64()
}

pub fn parse_rfc3339_ms(value: &str) -> Option<i64> {
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

    Some(
        days_from_civil(year, month, day) * 86_400_000
            + hour * 3_600_000
            + minute * 60_000
            + second * 1_000
            + millis,
    )
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let mut y = year as i64;
    let m = month as i64;
    let d = day as i64;
    y -= (m <= 2) as i64;
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = m + if m > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}
