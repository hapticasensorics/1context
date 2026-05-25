use chrono::{DateTime, Duration, Utc};
use onecontext_capture_core::{
    export_ready_bundle, seed_demo_capture_spool, validate_ready_bundle, CaptureTarget,
    ExportRequest, SourceInventory, SourceStatus,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use tempfile::tempdir;

#[test]
fn demo_capture_fixture_exports_expected_v0_lanes() {
    let temp = tempdir().unwrap();
    let capture_root = temp.path().join("capture");
    let time_start = DateTime::parse_from_rfc3339("2026-01-02T03:04:05Z")
        .unwrap()
        .with_timezone(&Utc);
    let fixture = seed_demo_capture_spool(&capture_root, time_start).unwrap();

    let response = export_ready_bundle(ExportRequest {
        capture_root: fixture.capture_root.clone(),
        time_start: fixture.time_start - Duration::seconds(1),
        time_end: fixture.time_end + Duration::seconds(1),
        target: CaptureTarget::ActiveWindow,
        debug_pin: false,
        frames_2fps_dir: None,
        debug_video_path: None,
        status_json: Some(capability_ok("capture.status")),
        ux_status_json: Some(capability_ok("ux-event-tap")),
        sampler_json: Some(capability_ok("samplers")),
        browser_proof_json: Some(capability_ok("browser-extension-proof")),
        source_envelope_paths: fixture.source_envelope_paths.clone(),
    })
    .unwrap();

    let report = validate_ready_bundle(&response.bundle_path).unwrap();
    assert!(report.ok, "{:#?}", report.findings);

    let sources = read_sources(&response.bundle_path);
    for lane in &fixture.lanes {
        assert_jsonl_contains_event_type(
            &response.bundle_path.join(&lane.bundle_file),
            &lane.event_type,
        );
        let source = sources
            .get(lane.lane_id.as_str())
            .unwrap_or_else(|| panic!("missing source for {}", lane.lane_id));
        assert_eq!(source.status, SourceStatus::Present);
        assert!(
            source.record_count > 0,
            "{} should have at least one record",
            lane.lane_id
        );
    }

    let external_refs = sources
        .get("capture.external_refs")
        .expect("external refs source should be inventoried for V0");
    assert_eq!(external_refs.status, SourceStatus::Present);
    assert!(external_refs.record_count > 0);
    assert_jsonl_non_empty(
        &response
            .bundle_path
            .join("external_refs/source-envelopes.jsonl"),
    );
}

fn capability_ok(source_id: &str) -> Value {
    json!({
        "schema_version": 1,
        "source_id": source_id,
        "status": "ok"
    })
}

fn assert_jsonl_contains_event_type(path: &Path, event_type: &str) {
    let text = fs::read_to_string(path).unwrap();
    let mut found = false;
    let mut non_empty_lines = 0;
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        non_empty_lines += 1;
        let value: Value = serde_json::from_str(line).unwrap();
        found |= value
            .get("eventType")
            .and_then(Value::as_str)
            .is_some_and(|actual| actual == event_type);
    }
    assert!(
        non_empty_lines > 0,
        "{} should not be empty",
        path.display()
    );
    assert!(found, "{} should contain {event_type}", path.display());
}

fn assert_jsonl_non_empty(path: &Path) {
    let text = fs::read_to_string(path).unwrap();
    assert!(
        text.lines().any(|line| !line.trim().is_empty()),
        "{} should not be empty",
        path.display()
    );
}

fn read_sources(bundle_path: &Path) -> BTreeMap<String, onecontext_capture_core::LaneSource> {
    let text = fs::read_to_string(bundle_path.join("sources.json")).unwrap();
    let inventory: SourceInventory = serde_json::from_str(&text).unwrap();
    inventory
        .sources
        .into_iter()
        .map(|source| (source.lane_id.clone(), source))
        .collect()
}
