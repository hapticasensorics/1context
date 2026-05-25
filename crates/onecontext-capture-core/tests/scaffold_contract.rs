use chrono::{Duration, Utc};
use onecontext_capture_core::{
    export_ready_bundle, list_bundles, plan_retention_sweep, validate_ready_bundle,
    BundleRelativePath, CaptureTarget, ExportRequest, RetentionPolicy, SourceInventory,
    SourceStatus, SweepActionKind,
};
use serde::de::DeserializeOwned;
use serde_json::json;
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Instant;
use tempfile::tempdir;

#[test]
fn relative_paths_reject_absolute_and_parent_escape() {
    assert!(BundleRelativePath::parse("events/windows.jsonl").is_ok());
    assert!(BundleRelativePath::parse("/tmp/windows.jsonl").is_err());
    assert!(BundleRelativePath::parse("../windows.jsonl").is_err());
    assert!(BundleRelativePath::parse("events/../../windows.jsonl").is_err());
}

#[test]
fn export_creates_ready_bundle_with_required_files() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    assert!(response.bundle_path.join("READY").exists());
    let report = validate_ready_bundle(&response.bundle_path).unwrap();
    assert!(report.ok, "{:#?}", report.findings);
}

#[test]
fn export_copies_2fps_frames_and_debug_video_into_bundle_media() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);
    let frame_dir = temp.path().join("decoder-frames");
    fs::create_dir_all(&frame_dir).unwrap();
    fs::write(frame_dir.join("frame-a.jpg"), "jpeg-a").unwrap();
    fs::write(frame_dir.join("frame-b.png"), "png-b").unwrap();
    let debug_video = temp.path().join("screen-recording.mov");
    fs::write(&debug_video, "debug movie").unwrap();

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: Some(frame_dir),
        debug_video_path: Some(debug_video),
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    assert_eq!(response.state, "ready");
    assert!(response
        .bundle_path
        .join("media/frames-2fps/frame-000001.jpg")
        .exists());
    assert!(response
        .bundle_path
        .join("media/frames-2fps/frame-000002.png")
        .exists());
    assert!(response
        .bundle_path
        .join("media/debug/screen-recording.mov")
        .exists());

    let media = read_jsonl_values(&response.bundle_path.join("media/media.index.jsonl"));
    assert_eq!(
        media
            .iter()
            .filter(|record| record["kind"] == "frame_2fps")
            .count(),
        2
    );
    assert!(media.iter().any(|record| {
        record["kind"] == "debug_screen_recording"
            && record["debug"] == true
            && record["path"] == "media/debug/screen-recording.mov"
    }));

    let sources: SourceInventory = read_json(&response.bundle_path.join("sources.json"));
    let active_frames = sources
        .sources
        .iter()
        .find(|source| source.lane_id == "capture.active_window_frames")
        .unwrap();
    assert_eq!(active_frames.status, SourceStatus::Present);
    assert_eq!(active_frames.record_count, 2);
    assert_eq!(active_frames.extra["frame_2fps_count"], json!(2));

    let report = validate_ready_bundle(&response.bundle_path).unwrap();
    assert!(report.ok, "{:#?}", report.findings);
}

#[test]
fn export_without_2fps_frames_is_quarantined_before_ready_promotion() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);
    fs::remove_dir_all(capture_root.join("media").join("frames-2fps")).unwrap();

    let response = export_ready_bundle(ExportRequest {
        capture_root: capture_root.clone(),
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    assert_eq!(response.state, "failed");
    assert!(!response.bundle_path.join("READY").exists());
    assert!(response
        .bundle_path
        .starts_with(capture_root.join("bundles").join("failed")));
    let validation = response.validation.as_ref().unwrap();
    assert!(!validation.ok);
    assert!(validation
        .findings
        .iter()
        .any(|finding| finding.code == "missing_frame_2fps_media"));
}

#[test]
fn invalid_export_is_quarantined_before_ready_promotion() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot_without_displays(&capture_root);

    let response = export_ready_bundle(ExportRequest {
        capture_root: capture_root.clone(),
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    assert_eq!(response.state, "failed");
    assert!(!response.bundle_path.join("READY").exists());
    assert!(response
        .bundle_path
        .starts_with(capture_root.join("bundles").join("failed")));
    let validation = response.validation.as_ref().unwrap();
    assert!(!validation.ok);
    assert!(validation
        .findings
        .iter()
        .any(|finding| finding.code == "empty_displays_jsonl"));

    let inventory = list_bundles(&capture_root).unwrap();
    assert!(inventory
        .entries
        .iter()
        .all(|entry| entry.directory_class != onecontext_capture_core::BundleDirectoryClass::Live));
}

#[test]
fn validator_rejects_attention_output_files() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);
    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();
    fs::write(
        response.bundle_path.join("attention-filter-output.json"),
        "{}",
    )
    .unwrap();

    let report = validate_ready_bundle(&response.bundle_path).unwrap();
    assert!(!report.ok);
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == "attention_output_in_bundle"));
}

#[test]
fn validator_rejects_scaffold_alignment_missing_gaps_and_bad_source_status() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);
    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    fs::write(
        response.bundle_path.join("time_alignment.json"),
        r#"{"schema_version":1,"status":"scaffold","time_basis":"system_utc"}"#,
    )
    .unwrap();
    fs::write(response.bundle_path.join("quality/known_gaps.jsonl"), "").unwrap();
    let mut sources: Value = read_json(&response.bundle_path.join("sources.json"));
    let browser = sources["sources"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|source| source["source_id"] == "browser")
        .unwrap();
    browser["status"] = json!("present");
    browser["record_count"] = json!(0);
    fs::write(
        response.bundle_path.join("sources.json"),
        serde_json::to_string_pretty(&sources).unwrap(),
    )
    .unwrap();

    let report = validate_ready_bundle(&response.bundle_path).unwrap();
    assert!(!report.ok);
    for code in [
        "scaffold_time_alignment",
        "missing_known_gap_for_degraded_lane",
        "present_source_has_no_records",
    ] {
        assert!(
            report.findings.iter().any(|finding| finding.code == code),
            "missing finding {code}: {:#?}",
            report.findings
        );
    }
}

#[test]
fn validator_rejects_missing_bundle_relative_provenance_artifact() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);
    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    fs::write(
        response.bundle_path.join("external_refs/source-envelopes.jsonl"),
        r#"{"schema_version":1,"kind":"bundle_relative","path":"external_refs/missing.jsonl","exists":true}"#,
    )
    .unwrap();

    let report = validate_ready_bundle(&response.bundle_path).unwrap();
    assert!(!report.ok);
    assert!(report
        .findings
        .iter()
        .any(|finding| finding.code == "missing_source_envelope_artifact"));
}

#[test]
fn retention_preserves_pinned_and_plans_expired_live_delete() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);

    let live = export_ready_bundle(ExportRequest {
        capture_root: capture_root.clone(),
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();
    let pinned = export_ready_bundle(ExportRequest {
        capture_root: capture_root.clone(),
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: true,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    let policy = RetentionPolicy {
        keep_last_ready: 0,
        ..RetentionPolicy::default()
    };
    let plan = plan_retention_sweep(&capture_root, &policy, now + Duration::hours(2)).unwrap();
    assert!(plan
        .actions
        .iter()
        .any(|action| action.path == live.bundle_path
            && matches!(action.kind, SweepActionKind::Delete)));
    assert!(plan
        .actions
        .iter()
        .any(|action| action.path == pinned.bundle_path
            && matches!(action.kind, SweepActionKind::Preserve)));

    let inventory = list_bundles(&capture_root).unwrap();
    assert_eq!(inventory.entries.len(), 2);
}

#[test]
fn empty_browser_terminal_and_editor_lanes_are_recorded_as_degraded() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    for lane_file in [
        "events/browser.events.jsonl",
        "events/terminal.events.jsonl",
        "events/editor.events.jsonl",
    ] {
        let text = fs::read_to_string(response.bundle_path.join(lane_file)).unwrap();
        assert!(text.trim().is_empty(), "{lane_file} should be empty");
    }

    let sources: SourceInventory = read_json(&response.bundle_path.join("sources.json"));
    for lane_id in ["capture.browser", "capture.terminal", "capture.editor"] {
        let source = sources
            .sources
            .iter()
            .find(|source| source.lane_id == lane_id)
            .unwrap_or_else(|| panic!("missing source inventory entry for {lane_id}"));
        assert_eq!(source.status, SourceStatus::Degraded);
        assert_eq!(source.record_count, 0);
        assert!(source.required_for_v0);
        assert!(source.degraded_reason.is_some());
    }

    let gaps = read_jsonl_values(&response.bundle_path.join("quality/known_gaps.jsonl"));
    for source_id in ["browser", "terminal", "editor"] {
        assert!(gaps
            .iter()
            .any(|gap| { gap["source_id"] == source_id && gap["code"] == "mandatory_lane_empty" }));
    }
    let browser_proof = sources
        .sources
        .iter()
        .find(|source| source.lane_id == "capabilities/browser-extension-proof.json")
        .expect("browser extension proof should be inventoried");
    assert_eq!(browser_proof.status, SourceStatus::Degraded);
    assert_eq!(browser_proof.record_count, 0);
    assert_eq!(
        browser_proof.degraded_reason.as_deref(),
        Some("browser extension proof not supplied")
    );
    assert!(gaps.iter().any(|gap| {
        gap["source_id"] == "browser_extension_proof" && gap["code"] == "capability_proof_degraded"
    }));
}

#[test]
fn source_envelope_paths_are_surfaced_as_external_refs() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let external_envelope = temp.path().join("external-source-envelope.jsonl");
    fs::write(&external_envelope, "{\"eventType\":\"source.external\"}\n").unwrap();
    let now = seed_window_snapshot(&capture_root);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: vec![external_envelope.clone()],
    })
    .unwrap();

    let refs = read_jsonl_values(
        &response
            .bundle_path
            .join("external_refs/source-envelopes.jsonl"),
    );
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0]["kind"], "external_path_metadata");
    assert_eq!(
        refs[0]["path"].as_str(),
        Some(external_envelope.to_string_lossy().as_ref())
    );
    assert_eq!(refs[0]["absolute"], true);
    assert_eq!(refs[0]["exists"], true);
    assert_eq!(refs[0]["byte_count"], 32);

    let sources: SourceInventory = read_json(&response.bundle_path.join("sources.json"));
    let external_refs = sources
        .sources
        .iter()
        .find(|source| source.lane_id == "capture.external_refs")
        .unwrap();
    assert_eq!(external_refs.status, SourceStatus::Present);
    assert_eq!(external_refs.record_count, 1);
}

#[test]
fn display_records_are_derived_from_window_snapshot_payloads() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    fs::create_dir_all(capture_root.join("windows")).unwrap();
    fs::create_dir_all(capture_root.join("displays")).unwrap();
    let now = Utc::now();
    let line = json!({
        "schemaVersion": 1,
        "eventType": "capture.window_snapshot",
        "recordedAt": now.to_rfc3339(),
        "payload": {
            "windows": [{"windowID": 7}],
            "displays": [{
                "displayID": 1,
                "name": "Built-in Display",
                "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
            }]
        }
    });
    fs::write(
        capture_root.join("windows").join("derived.windows.jsonl"),
        format!("{line}\n"),
    )
    .unwrap();
    seed_frames_2fps(&capture_root, 2);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    let displays = read_jsonl_values(&response.bundle_path.join("events/displays.jsonl"));
    assert_eq!(displays.len(), 1);
    assert_eq!(displays[0]["eventType"], "capture.display_snapshot");
    assert_eq!(displays[0]["payload"]["derived"], true);
    assert!(displays[0]["payload"]["inferred_from"].is_array());

    let sources: SourceInventory = read_json(&response.bundle_path.join("sources.json"));
    let displays_source = sources
        .sources
        .iter()
        .find(|source| source.lane_id == "capture.displays")
        .unwrap();
    assert_eq!(displays_source.status, SourceStatus::Present);
    assert_eq!(displays_source.record_count, 1);
    assert_eq!(displays_source.extra["evidence_quality"], "inferred_only");
}

#[test]
fn degraded_mandatory_lanes_and_not_required_capabilities_are_known_gaps() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot_without_displays(&capture_root);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: Some(json!({
            "schema_version": 1,
            "status": "not_required_yet",
            "reason": "capture status intentionally absent"
        })),
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    let sources: SourceInventory = read_json(&response.bundle_path.join("sources.json"));
    let capture_status = sources
        .sources
        .iter()
        .find(|source| source.lane_id == "capabilities/capture.status.json")
        .unwrap();
    assert_eq!(capture_status.status, SourceStatus::Degraded);
    assert_eq!(capture_status.record_count, 0);

    let gaps = read_jsonl_values(&response.bundle_path.join("quality/known_gaps.jsonl"));
    for source_id in [
        "ax",
        "ux",
        "browser",
        "terminal",
        "editor",
        "capture_status",
        "permissions",
        "ux_event_tap",
        "samplers",
        "browser_extension_proof",
    ] {
        assert!(
            gaps.iter().any(|gap| gap["source_id"] == source_id),
            "missing known gap for {source_id}"
        );
    }
}

#[test]
fn permissions_capability_is_derived_from_capture_status_metadata() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: Some(json!({
            "schema_version": 1,
            "status": "ok",
            "permission_derived_metadata": {
                "schema_version": 1,
                "generated_at": now.to_rfc3339(),
                "signals": {
                    "accessibility": {"ready": true, "status": "granted"},
                    "screen_capture": {"ready": true, "status": "granted"}
                }
            }
        })),
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    let permissions: Value = read_json(&response.bundle_path.join("capabilities/permissions.json"));
    assert_eq!(permissions["status"], "ok");
    assert_eq!(
        permissions["source"],
        "capture.status.permission_derived_metadata"
    );
    assert_eq!(
        permissions["permission_derived_metadata"]["signals"]["accessibility"]["ready"],
        true
    );
}

#[test]
fn time_alignment_contains_capture_window_clocks_and_source_summaries() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let now = seed_window_snapshot(&capture_root);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    let alignment: Value = read_json(&response.bundle_path.join("time_alignment.json"));
    assert_eq!(alignment["status"], "derived");
    assert_eq!(alignment["base_epoch"]["epoch_id"], "unix_epoch_utc");
    assert_eq!(
        alignment["capture_window"]["time_start"],
        (now - Duration::seconds(1)).to_rfc3339()
    );
    assert!(alignment["source_clocks_used"]
        .as_array()
        .unwrap()
        .iter()
        .any(|clock| clock["clock_id"] == "recordedAt"));
    assert!(alignment["segment_timing_summary"]
        .as_array()
        .unwrap()
        .iter()
        .any(|summary| summary["lane_id"] == "capture.windows"));
}

#[test]
fn out_of_range_window_snapshots_are_bracketed_when_exact_snapshot_is_missing() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    fs::create_dir_all(capture_root.join("windows")).unwrap();
    let now = Utc::now();
    let before = now - Duration::seconds(10);
    let after = now + Duration::seconds(10);
    let lines = [before, after]
        .into_iter()
        .map(|time| {
            json!({
                "schemaVersion": 1,
                "eventType": "capture.window_snapshot",
                "recordedAt": time.to_rfc3339(),
                "payload": {
                    "windows": [{"windowID": time.timestamp()}],
                    "displays": [{
                        "displayID": 1,
                        "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
                    }]
                }
            })
            .to_string()
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(
        capture_root.join("windows").join("bracket.windows.jsonl"),
        format!("{lines}\n"),
    )
    .unwrap();
    seed_frames_2fps(&capture_root, 2);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    let windows = read_jsonl_values(&response.bundle_path.join("events/windows.jsonl"));
    assert_eq!(windows.len(), 2);
    let gaps = read_jsonl_values(&response.bundle_path.join("quality/known_gaps.jsonl"));
    assert!(gaps
        .iter()
        .any(|gap| gap["code"] == "window_snapshot_bracketed"));
}

#[test]
fn large_out_of_range_window_spool_export_avoids_full_payload_parse_regression() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    fs::create_dir_all(capture_root.join("windows")).unwrap();
    let base = Utc::now();
    let record_count = 2_500;
    let payload_padding = "x".repeat(8_192);
    let windows_path = capture_root
        .join("windows")
        .join(base.format("%Y-%m-%d.windows.jsonl").to_string());
    let mut file = fs::File::create(&windows_path).unwrap();
    for index in 0..record_count {
        let time = base + Duration::seconds(index);
        let payload = json!({
            "windows": [{
                "windowID": index,
                "ownerName": "Synthetic",
                "bundleID": "com.haptica.synthetic",
                "title": payload_padding
            }],
            "displays": [{
                "displayID": 1,
                "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
            }]
        });
        writeln!(
            file,
            "{{\"schemaVersion\":1,\"eventType\":\"capture.window_snapshot\",\"recordedAt\":{},\"payload\":{payload}}}",
            serde_json::to_string(&time.to_rfc3339()).unwrap()
        )
        .unwrap();
    }
    seed_frames_2fps(&capture_root, 2);

    let target = base + Duration::seconds(record_count - 3);
    let start = target + Duration::milliseconds(250);
    let end = target + Duration::milliseconds(260);
    let prime_response = export_ready_bundle(ExportRequest {
        capture_root: capture_root.clone(),
        time_start: start,
        time_end: end,
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();
    let prime_spool_report: Value = read_json(
        &prime_response
            .bundle_path
            .join("quality/spool_read_report.json"),
    );
    let prime_large_file = prime_spool_report["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| {
            file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with(".windows.jsonl"))
        })
        .unwrap();
    assert_eq!(prime_large_file["index_used"], true);

    let started = Instant::now();
    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: start,
        time_end: end,
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();
    let elapsed = started.elapsed();
    eprintln!(
        "large window spool export: records={record_count} payload_bytes={} elapsed_ms={}",
        payload_padding.len(),
        elapsed.as_millis()
    );

    let windows = read_jsonl_values(&response.bundle_path.join("events/windows.jsonl"));
    assert_eq!(windows.len(), 2);

    let spool_report: Value =
        read_json(&response.bundle_path.join("quality/spool_read_report.json"));
    let large_file = spool_report["files"]
        .as_array()
        .unwrap()
        .iter()
        .find(|file| {
            file["path"]
                .as_str()
                .is_some_and(|path| path.ends_with(".windows.jsonl"))
        })
        .unwrap();
    assert_eq!(
        large_file["total_lines"].as_u64(),
        Some(record_count as u64)
    );
    assert_eq!(large_file["selected_records"].as_u64(), Some(0));
    assert_eq!(large_file["scan_strategy"], "windows_spool_time_index");
    assert_eq!(large_file["index_used"], true);
    assert_eq!(
        large_file["index_built"], false,
        "measured export should reuse the primed windows spool index"
    );
    let indexed_lines_scanned = large_file["indexed_lines_scanned"].as_u64().unwrap();
    assert!(
        indexed_lines_scanned < record_count as u64 / 10,
        "date-named window spool membership pass should not touch every line; scanned {indexed_lines_scanned}/{record_count}"
    );
    assert!(
        large_file["parsed_lines"].as_u64().unwrap() < record_count as u64 / 10,
        "date-named window spool membership pass should not parse every line"
    );
    assert_eq!(
        large_file["full_record_parse_count"], 0,
        "time filtering must not deserialize every out-of-window payload"
    );

    let lookup: Value = read_json(
        &response
            .bundle_path
            .join("quality/bracketing_window_snapshot_lookup.json"),
    );
    let bracketing_records_seen = lookup["window_snapshot_records_seen"].as_u64().unwrap();
    assert!(
        bracketing_records_seen < record_count as u64 / 10,
        "bracketing lookup should also stay range-bounded for date-named logs; saw {bracketing_records_seen}/{record_count}"
    );
    assert_eq!(lookup["selected_record_count"].as_u64(), Some(2));
    assert_eq!(
        lookup["full_payload_parse_count"], 0,
        "bracketing lookup must not full-parse every window snapshot payload"
    );
}

#[test]
fn bracketing_window_lookup_skips_unrelated_dated_files() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    fs::create_dir_all(capture_root.join("windows")).unwrap();
    let now = Utc::now();
    let before = now - Duration::seconds(10);
    let after = now + Duration::seconds(10);
    let snapshot_line = |time: chrono::DateTime<Utc>| {
        json!({
            "schemaVersion": 1,
            "eventType": "capture.window_snapshot",
            "recordedAt": time.to_rfc3339(),
            "payload": {
                "windows": [{"windowID": time.timestamp()}],
                "displays": [{
                    "displayID": 1,
                    "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
                }]
            }
        })
        .to_string()
    };

    fs::write(
        capture_root.join("windows").join(
            (now - Duration::days(3))
                .format("%Y-%m-%d.windows.jsonl")
                .to_string(),
        ),
        "{\"eventType\":\"capture.other\",\"recordedAt\":\"2026-05-22T00:00:00Z\",\"payload\":\n",
    )
    .unwrap();
    fs::write(
        capture_root
            .join("windows")
            .join(now.format("%Y-%m-%d.windows.jsonl").to_string()),
        format!("{}\n{}\n", snapshot_line(before), snapshot_line(after)),
    )
    .unwrap();
    fs::write(
        capture_root.join("windows").join(
            (now + Duration::days(3))
                .format("%Y-%m-%d.windows.jsonl")
                .to_string(),
        ),
        "{\"eventType\":\"capture.other\",\"recordedAt\":\"2026-05-28T00:00:00Z\",\"payload\":\n",
    )
    .unwrap();
    seed_frames_2fps(&capture_root, 2);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    let windows = read_jsonl_values(&response.bundle_path.join("events/windows.jsonl"));
    assert_eq!(windows.len(), 2);

    let lookup: Value = read_json(
        &response
            .bundle_path
            .join("quality/bracketing_window_snapshot_lookup.json"),
    );
    assert_eq!(lookup["full_payload_parse_count"], 0);
    assert_eq!(lookup["malformed_line_count"], 0);
    assert_eq!(lookup["files_scanned"], 1);
    assert_eq!(lookup["lines_scanned"], 2);
}

#[test]
fn connector_lanes_and_external_refs_are_inferred_from_ax_window_and_sck_evidence() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    fs::create_dir_all(capture_root.join("windows")).unwrap();
    fs::create_dir_all(capture_root.join("events")).unwrap();
    let now = Utc::now();
    fs::write(
        capture_root.join("windows").join("apps.windows.jsonl"),
        format!(
            "{}\n",
            json!({
                "schemaVersion": 1,
                "eventType": "capture.window_snapshot",
                "recordedAt": now.to_rfc3339(),
                "payload": {
                    "windows": [{
                        "windowID": 9,
                        "ownerName": "Terminal",
                        "bundleID": "com.apple.Terminal",
                        "title": "cargo test"
                    }],
                    "displays": [{
                        "displayID": 1,
                        "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
                    }]
                }
            })
        ),
    )
    .unwrap();
    let ax = json!({
        "schemaVersion": 1,
        "eventType": "capture.ax_focused_context",
        "recordedAt": (now + Duration::milliseconds(100)).to_rfc3339(),
        "payload": {
            "activeApplication": {
                "appName": "Google Chrome",
                "bundleID": "com.google.Chrome",
                "processID": 42
            },
            "focusedWindow": {"title": "Example - Google Chrome", "windowID": 11}
        }
    });
    let sck = json!({
        "schemaVersion": 1,
        "eventType": "capture.active_window_frame_metadata",
        "recordedAt": (now + Duration::milliseconds(200)).to_rfc3339(),
        "payload": {
            "target": {
                "appName": "Visual Studio Code",
                "bundleID": "com.microsoft.VSCode",
                "windowID": 12,
                "title": "exporter.rs"
            },
            "displayTime": 12345
        }
    });
    fs::write(
        capture_root.join("events").join("evidence.events.jsonl"),
        format!("{ax}\n{sck}\n"),
    )
    .unwrap();
    seed_frames_2fps(&capture_root, 2);

    let response = export_ready_bundle(ExportRequest {
        capture_root,
        time_start: now - Duration::seconds(1),
        time_end: now + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: None,
        ux_status_json: None,
        sampler_json: None,
        browser_proof_json: None,
        source_envelope_paths: Vec::new(),
    })
    .unwrap();

    for lane_file in [
        "events/browser.events.jsonl",
        "events/editor.events.jsonl",
        "events/terminal.events.jsonl",
    ] {
        let records = read_jsonl_values(&response.bundle_path.join(lane_file));
        assert_eq!(records.len(), 1, "{lane_file}");
        assert_eq!(records[0]["payload"]["inferred"], true);
        assert!(records[0]["payload"]["confidence"].as_f64().unwrap() > 0.6);
        assert!(records[0]["payload"]["inferred_from"].is_array());
    }

    let refs = read_jsonl_values(
        &response
            .bundle_path
            .join("external_refs/source-envelopes.jsonl"),
    );
    assert!(!refs.is_empty());
    assert_eq!(refs[0]["inferred"], true);

    let sources: SourceInventory = read_json(&response.bundle_path.join("sources.json"));
    for lane_id in ["capture.browser", "capture.editor", "capture.terminal"] {
        let source = sources
            .sources
            .iter()
            .find(|source| source.lane_id == lane_id)
            .unwrap();
        assert_eq!(source.status, SourceStatus::Present);
        assert_eq!(source.extra["evidence_quality"], "inferred_only");
    }
}

fn seed_window_snapshot(capture_root: &Path) -> chrono::DateTime<Utc> {
    seed_window_snapshot_with_display_mode(capture_root, true)
}

fn seed_window_snapshot_without_displays(capture_root: &Path) -> chrono::DateTime<Utc> {
    seed_window_snapshot_with_display_mode(capture_root, false)
}

fn seed_window_snapshot_with_display_mode(
    capture_root: &Path,
    include_displays: bool,
) -> chrono::DateTime<Utc> {
    fs::create_dir_all(capture_root.join("windows")).unwrap();
    let now = Utc::now();
    let mut payload = json!({"windows": [{"windowID": 7}]});
    if include_displays {
        payload["displays"] = json!([{
            "displayID": 1,
            "name": "Test Display",
            "bounds": {"x": 0, "y": 0, "width": 1512, "height": 982}
        }]);
    }
    let line = json!({
        "schemaVersion": 1,
        "eventType": "capture.window_snapshot",
        "recordedAt": now.to_rfc3339(),
        "payload": payload
    });
    fs::write(
        capture_root
            .join("windows")
            .join(now.format("%Y-%m-%d.windows.jsonl").to_string()),
        format!("{line}\n"),
    )
    .unwrap();
    seed_frames_2fps(capture_root, 2);
    now
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
    let text = fs::read_to_string(path).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn read_jsonl_values(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
