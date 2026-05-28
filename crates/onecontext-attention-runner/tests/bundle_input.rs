use std::fs;

use onecontext_attention_runner::run_attention_filter_on_bundle;
use serde_json::{json, Value};
use tempfile::tempdir;

#[test]
fn ready_bundle_runs_with_2fps_bundle_frames_and_external_dashboard_session() {
    let temp = tempdir().unwrap();
    let bundle = temp.path().join("capture/bundles/live/cap_test");
    fs::create_dir_all(bundle.join("events")).unwrap();
    fs::create_dir_all(bundle.join("media/frames-2fps")).unwrap();
    fs::create_dir_all(bundle.join("quality")).unwrap();
    fs::write(bundle.join("READY"), "READY\n").unwrap();
    write_json(
        &bundle.join("manifest.json"),
        &json!({
            "schema_version": 1,
            "contract_version": "capture-window-bundle.v0",
            "capture_id": "cap_test",
            "state": "ready",
            "created_at": "2026-05-25T20:00:00Z",
            "time_start": "2026-05-25T20:00:00Z",
            "time_end": "2026-05-25T20:00:10Z"
        }),
    );
    fs::write(
        bundle.join("events/ax.events.jsonl"),
        format!(
            "{}\n",
            json!({
                "eventType": "capture.ax_semantic.focused_window_changed.v1",
                "recordedAt": "2026-05-25T20:00:01Z",
                "payload": {
                    "kind": "focused_window_changed",
                    "activeApplication": {
                        "appName": "Google Chrome",
                        "bundleID": "com.google.Chrome"
                    },
                    "focusedWindow": { "title": "Composite Screenshot System" }
                }
            })
        ),
    )
    .unwrap();
    fs::write(
        bundle.join("events/sck-frame-metadata.events.jsonl"),
        format!(
            "{}\n",
            json!({
                "eventType": "capture.active_window_frame_metadata",
                "recordedAt": "2026-05-25T20:00:01.050Z",
                "payload": {
                    "target": {
                        "appName": "Code",
                        "bundleID": "com.microsoft.VSCode",
                        "title": "notes.md"
                    },
                    "motionFeatures": {
                        "dirtyAreaRatio": 0,
                        "changedTileRatio": 0,
                        "dirtyRectCount": 0,
                        "estimatedDY": 0,
                        "meanPixelDiff": 0
                    }
                }
            })
        ),
    )
    .unwrap();
    fs::write(bundle.join("media/frames-2fps/frame-000001.jpg"), "frame 1").unwrap();
    fs::write(bundle.join("media/frames-2fps/frame-000002.jpg"), "frame 2").unwrap();
    fs::write(
        bundle.join("media/media.index.jsonl"),
        [
            json!({
                "kind": "frame_2fps",
                "path": "media/frames-2fps/frame-000001.jpg",
                "status": "available",
                "frame_index": 1,
                "sample_rate_fps": 2
            }),
            json!({
                "kind": "frame_2fps",
                "path": "media/frames-2fps/frame-000002.jpg",
                "status": "available",
                "frame_index": 2,
                "sample_rate_fps": 2
            }),
        ]
        .into_iter()
        .map(|record| serde_json::to_string(&record).unwrap())
        .collect::<Vec<_>>()
        .join("\n")
            + "\n",
    )
    .unwrap();
    fs::write(bundle.join("quality/known_gaps.jsonl"), "").unwrap();

    let output_path = temp
        .path()
        .join("attention-run/attention-filter-output.json");
    let summary = run_attention_filter_on_bundle(&bundle, Some(&output_path)).unwrap();

    assert_eq!(summary.candidates, 2);
    assert_eq!(summary.saved, 1);
    assert!(summary.output_path.is_file());
    assert!(summary
        .dashboard_session_path
        .as_ref()
        .expect("dashboard session path")
        .is_file());

    let output = read_json(&output_path);
    assert_eq!(
        output["raw_buffer_audit"][0]["thumbnail_ref"].as_str(),
        Some("media/frames-2fps/frame-000001.jpg")
    );
    assert_eq!(
        output["saved_states"][0]["app_name"].as_str(),
        Some("Google Chrome")
    );
    assert_eq!(
        output["saved_states"][0]["decision"].as_str(),
        Some("save_transition")
    );
    assert!(output["source_conflicts"]
        .as_array()
        .unwrap()
        .iter()
        .any(|conflict| conflict["source_b"] == "sck_active_window"));

    let dashboard_session = read_json(
        summary
            .dashboard_session_path
            .as_ref()
            .expect("dashboard session path"),
    );
    assert_eq!(
        dashboard_session["media"]["playback_mode"].as_str(),
        Some("frame_cache")
    );
    assert_eq!(
        dashboard_session["media"]["frame_cache"]["root"].as_str(),
        Some("media/frames-2fps")
    );
    assert_eq!(
        dashboard_session["media"]["candidate_frame_sets"][0]["naming"].as_str(),
        Some("frame-{index:06}.jpg")
    );
}

#[test]
fn ready_bundle_without_2fps_media_is_rejected() {
    let temp = tempdir().unwrap();
    let bundle = temp.path().join("capture/bundles/live/cap_no_media");
    fs::create_dir_all(bundle.join("events")).unwrap();
    fs::create_dir_all(bundle.join("media")).unwrap();
    fs::write(bundle.join("READY"), "READY\n").unwrap();
    write_json(
        &bundle.join("manifest.json"),
        &json!({
            "schema_version": 1,
            "contract_version": "capture-window-bundle.v0",
            "capture_id": "cap_no_media",
            "state": "ready",
            "created_at": "2026-05-25T20:00:00Z",
            "time_start": "2026-05-25T20:00:00Z",
            "time_end": "2026-05-25T20:00:10Z"
        }),
    );
    fs::write(bundle.join("events/ax.events.jsonl"), "").unwrap();
    fs::write(bundle.join("media/media.index.jsonl"), "").unwrap();

    let error = run_attention_filter_on_bundle(&bundle, None).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("missing required frame_2fps media records"),
        "{error:?}"
    );
}

fn write_json(path: &std::path::Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn read_json(path: &std::path::Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
