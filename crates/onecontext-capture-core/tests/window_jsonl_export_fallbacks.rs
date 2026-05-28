use chrono::{DateTime, Duration, TimeZone, Utc};
use onecontext_capture_core::{
    export_ready_bundle, read_spool_window_strict, read_spool_window_tolerant, CaptureRootPaths,
    CaptureTarget, ExportRequest, SpoolQuery,
};
use serde::de::DeserializeOwned;
use serde_json::{json, Value};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn tolerant_export_reports_malformed_window_lines_and_preserves_raw_provenance_offsets() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let paths = CaptureRootPaths::new(&capture_root);
    paths.ensure_directories().unwrap();
    let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
    let good_line = window_snapshot_line(base, 1);
    let malformed_line = format!(
        "{{\"schemaVersion\":1,\"eventType\":\"capture.window_snapshot\",\"recordedAt\":\"{}\",\"payload\":",
        base.to_rfc3339()
    );
    let malformed_offset = (good_line.len() + 1) as u64;
    fs::write(
        paths.windows_dir.join("2026-05-25.windows.jsonl"),
        format!(
            "{good_line}\n{malformed_line}\n{}\n",
            display_snapshot_line(base)
        ),
    )
    .unwrap();
    seed_frames_2fps(&capture_root, 2);

    let response = export_ready_bundle(export_request(
        capture_root,
        base - Duration::seconds(1),
        base + Duration::seconds(1),
    ))
    .unwrap();

    assert_eq!(response.state, "ready");
    let exported_windows = read_jsonl_values(&response.bundle_path.join("events/windows.jsonl"));
    assert_eq!(exported_windows.len(), 1);
    assert_eq!(exported_windows[0]["payload"]["windows"][0]["windowID"], 1);

    let spool_report: Value =
        read_json(&response.bundle_path.join("quality/spool_read_report.json"));
    assert_eq!(spool_report["tolerant_read"], true);
    assert_eq!(spool_report["malformed_line_count"], 1);
    assert_eq!(spool_report["malformed_lines"][0]["line_number"], 2);
    assert_eq!(
        spool_report["malformed_lines"][0]["byte_offset"],
        malformed_offset
    );
    assert!(spool_report["malformed_lines"][0]["parser_error"]
        .as_str()
        .is_some_and(|error| !error.is_empty()));

    let raw_provenance =
        read_jsonl_values(&response.bundle_path.join("quality/raw_provenance.jsonl"));
    assert_eq!(raw_provenance.len(), 2);
    assert_eq!(raw_provenance[0]["event_type"], "capture.window_snapshot");
    assert_eq!(raw_provenance[0]["raw_line_number"], 1);
    assert_eq!(raw_provenance[0]["raw_byte_offset"], 0);
    assert!(raw_provenance[0]["raw_record_hash"]
        .as_str()
        .is_some_and(|hash| hash.len() == 64));

    let known_gaps = read_jsonl_values(&response.bundle_path.join("quality/known_gaps.jsonl"));
    assert!(known_gaps
        .iter()
        .any(|gap| gap["code"] == "malformed_spool_line_skipped" && gap["line_number"] == 2));
}

#[test]
fn strict_read_rejects_malformed_window_lines_that_tolerant_read_skips() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let paths = CaptureRootPaths::new(&capture_root);
    paths.ensure_directories().unwrap();
    let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
    fs::write(
        paths.windows_dir.join("strictness.windows.jsonl"),
        format!(
            "{}\n{{\"schemaVersion\":1,\"eventType\":\"capture.window_snapshot\",\"recordedAt\":\"{}\",\"payload\":\n",
            window_snapshot_line(base, 1),
            base.to_rfc3339()
        ),
    )
    .unwrap();
    let query = SpoolQuery {
        capture_root,
        time_start: base - Duration::seconds(1),
        time_end: base + Duration::seconds(1),
    };

    let tolerant = read_spool_window_tolerant(&query).unwrap();
    assert_eq!(tolerant.records.len(), 1);
    assert_eq!(tolerant.malformed_lines.len(), 1);

    let strict_error = read_spool_window_strict(&query).unwrap_err();
    assert!(strict_error
        .to_string()
        .contains("strictness.windows.jsonl:2"));
}

#[test]
fn export_includes_appended_windows_records_after_index_creation() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let paths = CaptureRootPaths::new(&capture_root);
    paths.ensure_directories().unwrap();
    let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
    let log_path = paths.windows_dir.join("2026-05-25.windows.jsonl");
    let old_line = window_snapshot_line(base - Duration::minutes(10), 1);
    fs::write(&log_path, format!("{old_line}\n")).unwrap();
    let indexed_byte_count = (old_line.len() + 1) as u64;

    let warmup = read_spool_window_tolerant(&SpoolQuery {
        capture_root: capture_root.clone(),
        time_start: base - Duration::minutes(11),
        time_end: base - Duration::minutes(9),
    })
    .unwrap();
    assert_eq!(warmup.records.len(), 1);
    assert!(warmup.files.iter().any(|file| file.index_built));

    let mut file = OpenOptions::new().append(true).open(&log_path).unwrap();
    writeln!(file, "{}", window_snapshot_line(base, 2)).unwrap();
    writeln!(file, "{}", display_snapshot_line(base)).unwrap();
    seed_frames_2fps(&capture_root, 2);

    let response = export_ready_bundle(export_request(
        capture_root,
        base - Duration::seconds(1),
        base + Duration::seconds(1),
    ))
    .unwrap();

    assert_eq!(response.state, "ready");
    let exported_windows = read_jsonl_values(&response.bundle_path.join("events/windows.jsonl"));
    assert_eq!(exported_windows.len(), 1);
    assert_eq!(exported_windows[0]["payload"]["windows"][0]["windowID"], 2);

    let raw_provenance =
        read_jsonl_values(&response.bundle_path.join("quality/raw_provenance.jsonl"));
    assert_eq!(raw_provenance.len(), 2);
    assert_eq!(raw_provenance[0]["raw_line_number"], 2);
    assert_eq!(raw_provenance[0]["raw_byte_offset"], indexed_byte_count);

    let spool_report: Value =
        read_json(&response.bundle_path.join("quality/spool_read_report.json"));
    assert!(spool_report["files"]
        .as_array()
        .unwrap()
        .iter()
        .any(|file| file["index_refreshed"] == true));
}

#[test]
fn undated_legacy_windows_file_is_not_used_as_bracketing_fallback() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let paths = CaptureRootPaths::new(&capture_root);
    paths.ensure_directories().unwrap();
    let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
    fs::write(
        paths.windows_dir.join("legacy.windows.jsonl"),
        format!("{}\n", window_snapshot_line(base - Duration::hours(1), 1)),
    )
    .unwrap();
    seed_frames_2fps(&capture_root, 2);

    let response = export_ready_bundle(export_request(
        capture_root,
        base - Duration::seconds(1),
        base + Duration::seconds(1),
    ))
    .unwrap();

    let exported_windows = read_jsonl_values(&response.bundle_path.join("events/windows.jsonl"));
    assert!(
        exported_windows.is_empty(),
        "undated legacy window logs should not be compatibility inputs"
    );
    let sources: Value = read_json(&response.bundle_path.join("sources.json"));
    assert!(sources["sources"].as_array().unwrap().iter().any(|source| {
        source["source_id"] == "windows"
            && source["status"] == "degraded"
            && source["record_count"] == 0
    }));
}

#[test]
fn fresh_window_snapshot_focused_context_does_not_backfill_ax_lane() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let paths = CaptureRootPaths::new(&capture_root);
    paths.ensure_directories().unwrap();
    let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
    fs::write(
        paths.windows_dir.join("2026-05-25.windows.jsonl"),
        format!("{}\n", window_snapshot_with_focused_context_line(base)),
    )
    .unwrap();
    seed_frames_2fps(&capture_root, 2);

    let response = export_ready_bundle(export_request(
        capture_root,
        base - Duration::seconds(1),
        base + Duration::seconds(1),
    ))
    .unwrap();

    let ax = read_jsonl_values(&response.bundle_path.join("events/ax.events.jsonl"));
    assert!(
        ax.is_empty(),
        "window focusedContext is retained in windows, not manufactured into AX"
    );
    let sources: Value = read_json(&response.bundle_path.join("sources.json"));
    assert!(sources["sources"].as_array().unwrap().iter().any(|source| {
        source["source_id"] == "ax" && source["status"] == "degraded" && source["record_count"] == 0
    }));
}

fn export_request(
    capture_root: impl AsRef<Path>,
    time_start: DateTime<Utc>,
    time_end: DateTime<Utc>,
) -> ExportRequest {
    ExportRequest {
        capture_root: capture_root.as_ref().to_path_buf(),
        time_start,
        time_end,
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    }
}

fn window_snapshot_line(time: DateTime<Utc>, window_id: i64) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": 1,
        "eventType": "capture.window_snapshot",
        "recordedAt": time.to_rfc3339(),
        "eventTimeStart": time.to_rfc3339(),
        "eventTimeEnd": time.to_rfc3339(),
        "laneID": "capture.windows",
        "sourceRecordID": format!("window-{window_id}"),
        "payload": {
            "windows": [{
                "windowID": window_id,
                "ownerName": "Synthetic",
                "bundleID": "com.haptica.synthetic",
                "title": format!("Window {window_id}")
            }],
            "displays": [{
                "displayID": 1,
                "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
            }]
        }
    }))
    .unwrap()
}

fn window_snapshot_with_focused_context_line(time: DateTime<Utc>) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": 1,
        "eventType": "capture.window_snapshot",
        "recordedAt": time.to_rfc3339(),
        "eventTimeStart": time.to_rfc3339(),
        "eventTimeEnd": time.to_rfc3339(),
        "laneID": "capture.windows",
        "sourceRecordID": "window-focused-context",
        "payload": {
            "windows": [{
                "windowID": 7,
                "ownerName": "Synthetic",
                "bundleID": "com.haptica.synthetic",
                "title": "Focused Window"
            }],
            "focusedContext": {
                "schemaVersion": 1,
                "generatedAt": time.to_rfc3339(),
                "isProcessTrusted": true,
                "status": "available",
                "activeApplication": {
                    "appName": "Synthetic",
                    "bundleID": "com.haptica.synthetic",
                    "processID": 42
                },
                "focusedWindow": {
                    "title": "Focused Window",
                    "windowID": 7
                }
            }
        }
    }))
    .unwrap()
}

fn display_snapshot_line(time: DateTime<Utc>) -> String {
    serde_json::to_string(&json!({
        "schemaVersion": 1,
        "eventType": "capture.display_snapshot",
        "recordedAt": time.to_rfc3339(),
        "eventTimeStart": time.to_rfc3339(),
        "eventTimeEnd": time.to_rfc3339(),
        "laneID": "capture.displays",
        "sourceRecordID": "display-1",
        "payload": {
            "displays": [{
                "displayID": 1,
                "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
            }]
        }
    }))
    .unwrap()
}

fn seed_frames_2fps(capture_root: &Path, frame_count: usize) {
    let frame_dir = capture_root.join("media").join("frames-2fps");
    fs::create_dir_all(&frame_dir).unwrap();
    for index in 1..=frame_count {
        fs::write(
            frame_dir.join(format!("frame-{index:06}.jpg")),
            format!("test frame {index}\n"),
        )
        .unwrap();
    }
}

fn read_json<T: DeserializeOwned>(path: &Path) -> T {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}

fn read_jsonl_values(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
