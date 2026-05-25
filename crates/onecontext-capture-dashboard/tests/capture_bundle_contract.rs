use serde_json::{json, Value};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use tempfile::TempDir;

const REQUIRED_V0_FILES: &[&str] = &[
    "manifest.json",
    "sources.json",
    "time_alignment.json",
    "capabilities/capture.status.json",
    "capabilities/permissions.json",
    "capabilities/ux-event-tap.json",
    "capabilities/samplers.json",
    "capabilities/browser-extension-proof.json",
    "quality/known_gaps.jsonl",
    "events/windows.jsonl",
    "events/displays.jsonl",
    "events/capture.events.jsonl",
    "events/ax.events.jsonl",
    "events/ux.events.jsonl",
    "events/sck-frame-metadata.events.jsonl",
    "events/browser.events.jsonl",
    "events/terminal.events.jsonl",
    "events/editor.events.jsonl",
    "media/media.index.jsonl",
    "media/frames-2fps/",
    "external_refs/source-envelopes.jsonl",
];

const MANDATORY_SOURCE_IDS: &[&str] = &[
    "windows",
    "displays",
    "capture",
    "ax",
    "ux",
    "sck_frame_metadata",
    "browser",
    "terminal",
    "editor",
    "external_refs",
];

#[test]
fn atomic_promotion_hides_partial_until_ready_and_moves_to_live() {
    let temp = TempDir::new().unwrap();
    let bundles = seed_bundle_roots(temp.path());
    let capture_id = "cap_contract_atomic";
    let partial = write_partial_bundle(&bundles, capture_id, false);

    assert!(partial.join("manifest.json").is_file());
    assert_eq!(ready_bundle_ids(&bundles), Vec::<String>::new());
    assert!(!bundles.live.join(capture_id).exists());

    promote_partial_bundle(&bundles, capture_id);

    let live_bundle = bundles.live.join(capture_id);
    assert!(!partial.exists());
    assert!(live_bundle.join("READY").is_file());
    assert_eq!(manifest_state(&live_bundle), "ready");
    assert_eq!(ready_bundle_ids(&bundles), vec![capture_id.to_string()]);
}

#[test]
fn required_v0_bundle_files_are_present_even_for_empty_degraded_lanes() {
    let temp = TempDir::new().unwrap();
    let bundles = seed_bundle_roots(temp.path());
    let capture_id = "cap_contract_required_files";
    write_partial_bundle(&bundles, capture_id, false);
    promote_partial_bundle(&bundles, capture_id);
    let live_bundle = bundles.live.join(capture_id);

    for required in REQUIRED_V0_FILES {
        assert!(
            live_bundle.join(required).exists(),
            "missing required V0 bundle path: {required}"
        );
    }

    let manifest = read_json(&live_bundle.join("manifest.json"));
    let required_from_manifest = manifest
        .get("v0_required_files")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(required_from_manifest, REQUIRED_V0_FILES);

    let sources = read_json(&live_bundle.join("sources.json"));
    let source_ids = sources
        .get("sources")
        .and_then(Value::as_array)
        .unwrap()
        .iter()
        .map(|source| source.get("source_id").unwrap().as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(source_ids, MANDATORY_SOURCE_IDS);
    assert!(source_ids.contains(&"browser"));
    assert!(source_ids.contains(&"terminal"));
    assert!(source_ids.contains(&"editor"));
}

#[test]
fn relative_path_validation_rejects_absolute_parent_and_prefix_escape_refs() {
    assert!(validate_bundle_relative_path("events/browser.events.jsonl").is_ok());
    assert!(validate_bundle_relative_path("media/event-frames/").is_ok());

    for invalid in [
        "/Users/paul/Desktop/frame.jpg",
        "../../secrets.txt",
        "events/../../secrets.txt",
        "",
    ] {
        assert!(
            validate_bundle_relative_path(invalid).is_err(),
            "expected invalid path rejection for {invalid:?}"
        );
    }

    let temp = TempDir::new().unwrap();
    let bundles = seed_bundle_roots(temp.path());
    let capture_id = "cap_contract_paths";
    write_partial_bundle(&bundles, capture_id, false);
    promote_partial_bundle(&bundles, capture_id);
    let live_bundle = bundles.live.join(capture_id);

    assert!(validate_manifest_paths(&live_bundle).is_ok());

    let manifest_path = live_bundle.join("manifest.json");
    let mut manifest = read_json(&manifest_path);
    manifest["source_spool"]["events"] =
        json!(["capture/events/2026-05-25.events.jsonl", "../raw.jsonl"]);
    write_json(&manifest_path, &manifest);

    assert!(validate_manifest_paths(&live_bundle).is_err());
}

#[test]
fn ttl_sweep_deletes_expired_live_bundles_and_audits() {
    let temp = TempDir::new().unwrap();
    let bundles = seed_bundle_roots(temp.path());
    let capture_id = "cap_contract_expired";
    write_partial_bundle(&bundles, capture_id, false);
    promote_partial_bundle(&bundles, capture_id);

    let report = sweep_expired_live_bundles(&bundles, "2026-05-25T21:00:00.000Z");

    assert!(!bundles.live.join(capture_id).exists());
    assert_eq!(report.deleted_paths, vec![format!("live/{capture_id}")]);
    assert_eq!(report.preserved_paths, Vec::<String>::new());

    let audit_log = fs::read_to_string(bundles.retention.join("sweeps.jsonl")).unwrap();
    assert!(audit_log.contains(&format!(r#""deleted_paths":["live/{capture_id}"]"#)));
}

#[test]
fn pinned_bundle_survives_ttl_sweep() {
    let temp = TempDir::new().unwrap();
    let bundles = seed_bundle_roots(temp.path());
    let capture_id = "cap_contract_pinned";
    write_partial_bundle(&bundles, capture_id, true);
    promote_partial_bundle(&bundles, capture_id);

    let report = sweep_expired_live_bundles(&bundles, "2026-05-25T21:00:00.000Z");

    assert!(bundles.live.join(capture_id).join("READY").is_file());
    assert_eq!(report.deleted_paths, Vec::<String>::new());
    assert_eq!(report.preserved_paths, vec![format!("live/{capture_id}")]);
}

struct BundleRoots {
    processing: PathBuf,
    live: PathBuf,
    retention: PathBuf,
}

struct SweepReport {
    deleted_paths: Vec<String>,
    preserved_paths: Vec<String>,
}

fn seed_bundle_roots(root: &Path) -> BundleRoots {
    let capture_root = root.join("capture");
    let bundles_root = capture_root.join("bundles");
    let roots = BundleRoots {
        processing: bundles_root.join("processing"),
        live: bundles_root.join("live"),
        retention: capture_root.join("retention"),
    };
    fs::create_dir_all(&roots.processing).unwrap();
    fs::create_dir_all(&roots.live).unwrap();
    fs::create_dir_all(&roots.retention).unwrap();
    roots
}

fn write_partial_bundle(bundles: &BundleRoots, capture_id: &str, pinned: bool) -> PathBuf {
    let bundle = bundles.processing.join(format!("{capture_id}.partial"));
    fs::create_dir_all(&bundle).unwrap();
    for required in REQUIRED_V0_FILES {
        let path = bundle.join(required);
        if required.ends_with('/') {
            fs::create_dir_all(&path).unwrap();
            continue;
        }
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        if required.ends_with(".json") {
            write_json(&path, &empty_required_json(required));
        } else {
            fs::write(&path, "").unwrap();
        }
    }
    fs::write(
        bundle.join("media/frames-2fps/frame-000001.jpg"),
        "fixture frame",
    )
    .unwrap();
    write_media_index(&bundle);
    write_json(&bundle.join("sources.json"), &sources_json());
    write_json(
        &bundle.join("manifest.json"),
        &manifest_json(capture_id, "partial", pinned),
    );
    bundle
}

fn promote_partial_bundle(bundles: &BundleRoots, capture_id: &str) {
    let partial = bundles.processing.join(format!("{capture_id}.partial"));
    let ready = partial.join("READY");
    fs::write(&ready, "READY\n").unwrap();

    let manifest_path = partial.join("manifest.json");
    let mut manifest = read_json(&manifest_path);
    manifest["state"] = json!("ready");
    manifest["ready_at"] = json!("2026-05-25T20:01:00.000Z");
    write_json(&manifest_path, &manifest);

    fs::rename(&partial, bundles.live.join(capture_id)).unwrap();
}

fn ready_bundle_ids(bundles: &BundleRoots) -> Vec<String> {
    let mut ids = fs::read_dir(&bundles.live)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| entry.path().join("READY").is_file())
        .filter(|entry| manifest_state(&entry.path()) == "ready")
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

fn sweep_expired_live_bundles(bundles: &BundleRoots, now: &str) -> SweepReport {
    let mut report = SweepReport {
        deleted_paths: Vec::new(),
        preserved_paths: Vec::new(),
    };

    for entry in fs::read_dir(&bundles.live).unwrap().filter_map(Result::ok) {
        let bundle = entry.path();
        if !bundle.is_dir() {
            continue;
        }
        let manifest = read_json(&bundle.join("manifest.json"));
        let capture_id = entry.file_name().to_string_lossy().into_owned();
        let relative_path = format!("live/{capture_id}");
        let pinned = manifest
            .get("pinned")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let expires_at = manifest
            .get("expires_at")
            .and_then(Value::as_str)
            .unwrap_or("");
        if pinned {
            report.preserved_paths.push(relative_path);
        } else if !expires_at.is_empty() && expires_at <= now {
            fs::remove_dir_all(&bundle).unwrap();
            report.deleted_paths.push(relative_path);
        }
    }

    report.deleted_paths.sort();
    report.preserved_paths.sort();
    append_sweep_audit(bundles, &report);
    report
}

fn append_sweep_audit(bundles: &BundleRoots, report: &SweepReport) {
    let line = json!({
        "sweep_id": "sweep_contract",
        "started_at": "2026-05-25T21:00:00.000Z",
        "completed_at": "2026-05-25T21:00:00.010Z",
        "policy_version": "capture-retention.v0",
        "deleted_paths": report.deleted_paths,
        "preserved_paths": report.preserved_paths,
        "errors": []
    });
    let path = bundles.retention.join("sweeps.jsonl");
    let mut existing = fs::read_to_string(&path).unwrap_or_default();
    existing.push_str(&serde_json::to_string(&line).unwrap());
    existing.push('\n');
    fs::write(path, existing).unwrap();
}

fn validate_manifest_paths(bundle: &Path) -> Result<(), String> {
    let manifest = read_json(&bundle.join("manifest.json"));
    let mut refs = Vec::new();
    collect_string_array(&manifest, "v0_required_files", &mut refs);
    collect_string_array(&manifest, "optional_files", &mut refs);
    if let Some(source_spool) = manifest.get("source_spool") {
        for key in ["events", "windows", "browser_extension_captures"] {
            collect_string_array(source_spool, key, &mut refs);
        }
    }

    for path in refs {
        validate_bundle_relative_path(&path)?;
    }
    Ok(())
}

fn validate_bundle_relative_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("path is empty".to_string());
    }
    let path = Path::new(path);
    if path.is_absolute() {
        return Err(format!("path is absolute: {}", path.display()));
    }
    if path.components().any(|component| {
        matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return Err(format!("path escapes bundle root: {}", path.display()));
    }
    Ok(())
}

fn collect_string_array(value: &Value, key: &str, out: &mut Vec<String>) {
    if let Some(values) = value.get(key).and_then(Value::as_array) {
        out.extend(
            values
                .iter()
                .filter_map(Value::as_str)
                .map(ToString::to_string),
        );
    }
}

fn manifest_state(bundle: &Path) -> String {
    read_json(&bundle.join("manifest.json"))
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string()
}

fn manifest_json(capture_id: &str, state: &str, pinned: bool) -> Value {
    json!({
        "schema_version": 1,
        "contract_version": "capture-window-bundle.v0",
        "capture_id": capture_id,
        "state": state,
        "created_at": "2026-05-25T20:00:00.000Z",
        "ready_at": Value::Null,
        "expires_at": "2026-05-25T20:30:00.000Z",
        "retention_class": if pinned { "pinned" } else { "live" },
        "pinned": pinned,
        "pin_reason": if pinned { "contract fixture" } else { "" },
        "time_range": {
            "start": "2026-05-25T20:00:00.000Z",
            "end": "2026-05-25T20:01:00.000Z"
        },
        "duration_ms": 60000,
        "source_spool": {
            "kind": "onecontext_capture_spool",
            "events": ["capture/events/2026-05-25.events.jsonl"],
            "windows": ["capture/windows/2026-05-25.windows.jsonl"],
            "browser_extension_captures": ["browser-extension-captures/20260525-200000"]
        },
        "v0_required_files": REQUIRED_V0_FILES,
        "optional_files": [
            "media/event-frames/",
            "media/thumbs/",
            "media/debug/",
            "replay/replay-manifest.json"
        ]
    })
}

fn write_media_index(bundle: &Path) {
    let record = json!({
        "schema_version": 1,
        "media_id": "fixture_frame_2fps_000001",
        "kind": "frame_2fps",
        "path": "media/frames-2fps/frame-000001.jpg",
        "source": "capture.screen_recording_frame_decoder",
        "sample_rate_fps": 2,
        "debug": false,
        "privacy_class": "visual_evidence",
        "available": true
    });
    fs::write(
        bundle.join("media/media.index.jsonl"),
        format!("{}\n", serde_json::to_string(&record).unwrap()),
    )
    .unwrap();
}

fn sources_json() -> Value {
    let sources = MANDATORY_SOURCE_IDS
        .iter()
        .map(|source_id| {
            let status = match *source_id {
                "browser" | "terminal" | "editor" => "degraded",
                _ => "present",
            };
            json!({
                "source_id": source_id,
                "lane_id": format!("capture.{source_id}"),
                "status": status,
                "required_for_v0": true,
                "record_count": 0,
                "degraded_reason": if status == "degraded" { "fixture_direct_source_unavailable" } else { "" }
            })
        })
        .collect::<Vec<_>>();
    json!({
        "schema_version": 1,
        "sources": sources
    })
}

fn empty_required_json(required: &str) -> Value {
    match required {
        "time_alignment.json" => json!({"schema_version": 1, "clock": "fixture"}),
        "capabilities/capture.status.json" => json!({"schema_version": 1, "status": "fixture"}),
        "capabilities/permissions.json" => {
            json!({"schema_version": 1, "raw_keystrokes_included": false})
        }
        "capabilities/ux-event-tap.json" => json!({"schema_version": 1, "status": "fixture"}),
        "capabilities/samplers.json" => json!({"schema_version": 1, "samplers": []}),
        "capabilities/browser-extension-proof.json" => {
            json!({"schema_version": 1, "status": "degraded"})
        }
        _ => json!({"schema_version": 1}),
    }
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
