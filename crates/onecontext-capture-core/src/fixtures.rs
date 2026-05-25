use crate::error::{CaptureCoreError, CaptureCoreResult};
use crate::paths::CaptureRootPaths;
use chrono::{DateTime, Duration, Utc};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemoCaptureLane {
    pub lane_id: String,
    pub bundle_file: String,
    pub event_type: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DemoCaptureFixture {
    pub capture_root: PathBuf,
    pub time_start: DateTime<Utc>,
    pub time_end: DateTime<Utc>,
    pub spool_files: Vec<PathBuf>,
    pub source_envelope_paths: Vec<PathBuf>,
    pub lanes: Vec<DemoCaptureLane>,
}

pub fn seed_demo_capture_spool(
    capture_root: impl AsRef<Path>,
    time_start: DateTime<Utc>,
) -> CaptureCoreResult<DemoCaptureFixture> {
    let capture_root = capture_root.as_ref().to_path_buf();
    let paths = CaptureRootPaths::new(&capture_root);
    paths.ensure_directories()?;

    let window_time = time_start;
    let display_time = time_start + Duration::seconds(1);
    let ax_time = time_start + Duration::seconds(2);
    let ux_time = time_start + Duration::seconds(3);
    let sck_time = time_start + Duration::seconds(4);
    let browser_time = time_start + Duration::seconds(5);
    let terminal_time = time_start + Duration::seconds(6);
    let editor_time = time_start + Duration::seconds(7);
    let time_end = editor_time;

    let window_file = paths.windows_dir.join("demo.windows.jsonl");
    let display_file = paths.displays_dir.join("demo.displays.jsonl");
    let events_file = paths.events_dir.join("demo.events.jsonl");
    let source_envelope_dir = capture_root.join("source-envelopes");
    let source_envelope_file = source_envelope_dir.join("demo-source-envelope.json");

    write_jsonl(
        &window_file,
        &[envelope(
            "capture.window_snapshot",
            "capture.windows",
            window_time,
            json!({
                "activeWindowID": 101,
                "windows": [{
                    "windowID": 101,
                    "ownerName": "Demo Editor",
                    "title": "dashboard-fixture.rs",
                    "bounds": {"x": 48, "y": 72, "width": 1280, "height": 840}
                }]
            }),
        )],
    )?;
    write_jsonl(
        &display_file,
        &[envelope(
            "capture.display_snapshot",
            "capture.displays",
            display_time,
            json!({
                "displays": [{
                    "displayID": 1,
                    "name": "Built-in Retina Display",
                    "scaleFactor": 2.0,
                    "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
                }]
            }),
        )],
    )?;
    write_jsonl(
        &events_file,
        &[
            envelope(
                "capture.ax.snapshot",
                "capture.ax",
                ax_time,
                json!({
                    "windowID": 101,
                    "focusedElement": {
                        "role": "AXTextArea",
                        "title": "dashboard-fixture.rs",
                        "valuePreview": "seed_demo_capture_spool"
                    }
                }),
            ),
            envelope(
                "capture.ux.input",
                "capture.ux",
                ux_time,
                json!({
                    "device": "keyboard",
                    "action": "key_down",
                    "key": "Return",
                    "targetWindowID": 101
                }),
            ),
            envelope(
                "capture.active_window_frame_metadata",
                "capture.active_window_frames",
                sck_time,
                json!({
                    "windowID": 101,
                    "frameID": "demo-frame-0001",
                    "mediaRef": "media://demo-frame-0001",
                    "dimensions": {"width": 1280, "height": 840},
                    "contentHash": "sha256:demo-frame-0001"
                }),
            ),
            envelope(
                "capture.browser.navigation",
                "capture.browser",
                browser_time,
                json!({
                    "browser": "Safari",
                    "tabID": "demo-tab-1",
                    "url": "https://example.test/1context/demo",
                    "title": "1Context demo capture"
                }),
            ),
            envelope(
                "capture.terminal.command",
                "capture.terminal",
                terminal_time,
                json!({
                    "terminal": "Terminal",
                    "sessionID": "demo-shell-1",
                    "cwd": "/Users/demo/project",
                    "command": "cargo test -p onecontext-capture-core"
                }),
            ),
            envelope(
                "capture.editor.buffer_snapshot",
                "capture.editor",
                editor_time,
                json!({
                    "editor": "Demo Editor",
                    "workspace": "/Users/demo/project",
                    "file": "crates/onecontext-capture-core/src/fixtures.rs",
                    "language": "rust",
                    "selection": {"line": 42, "column": 9}
                }),
            ),
        ],
    )?;
    fs::create_dir_all(&source_envelope_dir)
        .map_err(|error| CaptureCoreError::io(Some(source_envelope_dir.clone()), error))?;
    write_json(
        &source_envelope_file,
        &json!({
            "schema_version": 1,
            "kind": "demo_source_envelope",
            "time_start": time_start.to_rfc3339(),
            "time_end": time_end.to_rfc3339(),
            "source_ids": ["windows", "displays", "events"]
        }),
    )?;
    seed_frames_2fps(&paths.media_dir.join("frames-2fps"), 16)?;

    Ok(DemoCaptureFixture {
        capture_root,
        time_start,
        time_end,
        spool_files: vec![window_file, display_file, events_file],
        source_envelope_paths: vec![source_envelope_file],
        lanes: vec![
            lane(
                "capture.windows",
                "events/windows.jsonl",
                "capture.window_snapshot",
            ),
            lane(
                "capture.displays",
                "events/displays.jsonl",
                "capture.display_snapshot",
            ),
            lane(
                "capture.events",
                "events/capture.events.jsonl",
                "capture.ax.snapshot",
            ),
            lane(
                "capture.ax",
                "events/ax.events.jsonl",
                "capture.ax.snapshot",
            ),
            lane("capture.ux", "events/ux.events.jsonl", "capture.ux.input"),
            lane(
                "capture.active_window_frames",
                "events/sck-frame-metadata.events.jsonl",
                "capture.active_window_frame_metadata",
            ),
            lane(
                "capture.browser",
                "events/browser.events.jsonl",
                "capture.browser.navigation",
            ),
            lane(
                "capture.terminal",
                "events/terminal.events.jsonl",
                "capture.terminal.command",
            ),
            lane(
                "capture.editor",
                "events/editor.events.jsonl",
                "capture.editor.buffer_snapshot",
            ),
        ],
    })
}

fn envelope(event_type: &str, lane_id: &str, time: DateTime<Utc>, payload: Value) -> Value {
    json!({
        "schemaVersion": 1,
        "eventType": event_type,
        "recordedAt": time.to_rfc3339(),
        "eventTimeStart": time.to_rfc3339(),
        "eventTimeEnd": time.to_rfc3339(),
        "laneID": lane_id,
        "streamID": "demo-capture-spool",
        "sourceRecordID": format!("demo:{event_type}:{}", time.timestamp_millis()),
        "payload": payload
    })
}

fn lane(lane_id: &str, bundle_file: &str, event_type: &str) -> DemoCaptureLane {
    DemoCaptureLane {
        lane_id: lane_id.to_string(),
        bundle_file: bundle_file.to_string(),
        event_type: event_type.to_string(),
    }
}

fn seed_frames_2fps(frame_dir: &Path, frame_count: usize) -> CaptureCoreResult<()> {
    fs::create_dir_all(frame_dir)
        .map_err(|error| CaptureCoreError::io(Some(frame_dir.to_path_buf()), error))?;
    for index in 1..=frame_count {
        let path = frame_dir.join(format!("frame-{index:06}.jpg"));
        fs::write(&path, format!("demo frame {index}\n"))
            .map_err(|error| CaptureCoreError::io(Some(path), error))?;
    }
    Ok(())
}

fn write_jsonl(path: &Path, records: &[Value]) -> CaptureCoreResult<()> {
    let mut text = String::new();
    for record in records {
        let line = serde_json::to_string(record)
            .map_err(|error| CaptureCoreError::json(Some(path.to_path_buf()), error))?;
        text.push_str(&line);
        text.push('\n');
    }
    fs::write(path, text).map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))
}

fn write_json(path: &Path, value: &Value) -> CaptureCoreResult<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| CaptureCoreError::json(Some(path.to_path_buf()), error))?;
    fs::write(path, format!("{text}\n"))
        .map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))
}
