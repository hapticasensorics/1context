use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Duration, SecondsFormat, Utc};
use image::GenericImageView;
use serde_json::{json, Value};

use crate::{
    events::load_capture_events,
    fixture::AttentionFixture,
    model::{
        CandidateFrameSet, CaptureEvent, DashboardFixtureConfig, DashboardInputsConfig,
        DashboardMediaConfig, DashboardSession, EventRef, FilterOutputRef,
    },
};

const BUNDLE_EVENT_FILES: &[(&str, &str, bool)] = &[
    ("windows", "events/windows.jsonl", true),
    ("displays", "events/displays.jsonl", true),
    ("capture", "events/capture.events.jsonl", true),
    ("ax", "events/ax.events.jsonl", true),
    ("ux", "events/ux.events.jsonl", true),
    (
        "sck-frame-metadata",
        "events/sck-frame-metadata.events.jsonl",
        true,
    ),
    ("browser", "events/browser.events.jsonl", true),
    ("terminal", "events/terminal.events.jsonl", true),
    ("editor", "events/editor.events.jsonl", true),
];
const DASHBOARD_REVIEW_FRAME_MAX_HEIGHT: u32 = 720;
const DERIVED_VISUAL_FRAME_CHANGES_FILE: &str = "derived-visual-frame-changes.events.jsonl";
const VISUAL_FRAME_CHANGE_FULL_DIFF_THRESHOLD: f32 = 0.10;
const VISUAL_FRAME_CHANGE_TOP_DIFF_THRESHOLD: f32 = 0.06;
const VISUAL_DIFF_THUMB_WIDTH: u32 = 96;
const VISUAL_DIFF_THUMB_HEIGHT: u32 = 54;

#[derive(Debug, Clone)]
pub struct BundleRun {
    pub fixture: AttentionFixture,
    pub output_path: PathBuf,
    pub dashboard_session_path: PathBuf,
    pub compatibility_report_path: PathBuf,
}

pub fn prepare_bundle_run(bundle_path: &Path, output_path: Option<&Path>) -> Result<BundleRun> {
    let bundle_path = normalize_existing_path(bundle_path)?;
    validate_ready_bundle_shape(&bundle_path)?;

    let manifest_path = bundle_path.join("manifest.json");
    let manifest: Value = read_json(&manifest_path)?;
    let capture_id = manifest
        .get("capture_id")
        .and_then(Value::as_str)
        .unwrap_or("capture-bundle")
        .to_string();
    let time_start = manifest_time(&manifest, "time_start")
        .or_else(|| manifest_time_path(&manifest, &["time_range", "start"]))
        .ok_or_else(|| anyhow!("manifest is missing time_start/time_range.start"))?;
    let time_end = manifest_time(&manifest, "time_end")
        .or_else(|| manifest_time_path(&manifest, &["time_range", "end"]))
        .ok_or_else(|| anyhow!("manifest is missing time_end/time_range.end"))?;
    let duration_ms = (time_end - time_start).num_milliseconds().max(0) as u64;
    let work_dir = bundle_work_dir(&capture_id, output_path)?;
    fs::create_dir_all(&work_dir).with_context(|| format!("create {}", work_dir.display()))?;
    let output_path = output_path
        .map(Path::to_path_buf)
        .unwrap_or_else(|| work_dir.join("attention-filter-output.json"));
    ensure_output_is_outside_bundle(&bundle_path, &output_path)?;
    let dashboard_session_path = work_dir.join("attention-dashboard-session.json");
    let compatibility_report_path = work_dir.join("bundle-compatibility-report.json");

    let event_refs = event_refs_for_bundle(&bundle_path);
    let events = load_bundle_events(&bundle_path, &event_refs, time_start.timestamp_millis())?;
    let media_index = read_media_index(&bundle_path)?;
    let candidate_frame_sets = candidate_frame_sets_from_media_index(&media_index);

    let session = DashboardSession {
        session_id: capture_id.clone(),
        title: format!("Capture Bundle {capture_id}"),
        created_at: manifest
            .get("created_at")
            .and_then(Value::as_str)
            .unwrap_or_else(|| {
                manifest
                    .get("ready_at")
                    .and_then(Value::as_str)
                    .unwrap_or("1970-01-01T00:00:00Z")
            })
            .to_string(),
        fixture: DashboardFixtureConfig {
            run_id: capture_id.clone(),
            root: bundle_path.display().to_string(),
            duration_ms,
        },
        media: DashboardMediaConfig {
            video_ref: "media/media.index.jsonl".to_string(),
            video_width: 0,
            video_height: 0,
            candidate_frame_sets,
        },
        inputs: DashboardInputsConfig {
            candidate_index_ref: None,
            snapshots_root: None,
            event_refs: event_refs.clone(),
        },
        filter_output: FilterOutputRef {
            path: output_path.display().to_string(),
        },
    };

    let fixture = AttentionFixture {
        session_path: dashboard_session_path.clone(),
        root: bundle_path.clone(),
        session,
        events,
    };

    let report = compatibility_report(&bundle_path, &manifest, &fixture, &media_index);
    write_json(&compatibility_report_path, &report)?;

    Ok(BundleRun {
        fixture,
        output_path,
        dashboard_session_path,
        compatibility_report_path,
    })
}

pub fn write_dashboard_session(bundle_run: &BundleRun) -> Result<()> {
    let session = dashboard_session_json(bundle_run)?;
    write_json(&bundle_run.dashboard_session_path, &session)
}

fn validate_ready_bundle_shape(bundle_path: &Path) -> Result<()> {
    if !bundle_path.is_dir() {
        bail!("bundle path is not a directory: {}", bundle_path.display());
    }
    if !bundle_path.join("READY").is_file() {
        bail!(
            "bundle is not READY: missing {}",
            bundle_path.join("READY").display()
        );
    }
    let manifest: Value = read_json(&bundle_path.join("manifest.json"))?;
    if manifest
        .get("state")
        .and_then(Value::as_str)
        .is_some_and(|state| state != "ready")
    {
        bail!("bundle manifest state is not ready");
    }
    Ok(())
}

fn event_refs_for_bundle(bundle_path: &Path) -> Vec<EventRef> {
    BUNDLE_EVENT_FILES
        .iter()
        .filter(|(_, relative, _)| bundle_path.join(relative).is_file())
        .map(|(id, relative, required)| EventRef {
            id: (*id).to_string(),
            kind: "capture_events".to_string(),
            path: (*relative).to_string(),
            format: "jsonl".to_string(),
            required: *required,
        })
        .collect()
}

fn load_bundle_events(
    bundle_path: &Path,
    event_refs: &[EventRef],
    base_epoch_ms: i64,
) -> Result<Vec<CaptureEvent>> {
    let mut events = Vec::new();
    for event_ref in event_refs {
        events.extend(load_capture_events(
            bundle_path,
            event_ref,
            Some(base_epoch_ms),
        )?);
    }
    Ok(events)
}

fn read_media_index(bundle_path: &Path) -> Result<Vec<Value>> {
    let path = bundle_path.join("media/media.index.jsonl");
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(Vec::new());
    };
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).with_context(|| format!("parse {}", path.display())))
        .collect()
}

fn candidate_frame_sets_from_media_index(media_index: &[Value]) -> Vec<CandidateFrameSet> {
    let frame_2fps_records = media_index
        .iter()
        .filter(|record| record.get("kind").and_then(Value::as_str) == Some("frame_2fps"))
        .filter(|record| media_record_available(record))
        .filter_map(|record| media_path(record).map(|path| (record, path)))
        .filter(|(_, path)| path.starts_with("media/frames-2fps/"))
        .collect::<Vec<_>>();

    if let Some((first_record, first_path)) = frame_2fps_records.first().copied() {
        return vec![CandidateFrameSet {
            id: "2fps".to_string(),
            root: parent_relative_path(first_path)
                .unwrap_or_else(|| "media/frames-2fps".to_string()),
            fps: first_record
                .get("sample_rate_fps")
                .and_then(Value::as_f64)
                .unwrap_or(2.0)
                .max(0.1) as f32,
            count: frame_2fps_records.len(),
            naming: infer_frame_naming(first_path)
                .unwrap_or_else(|| "frame-{index:06}.jpg".to_string()),
        }];
    }

    let event_frame_records = media_index
        .iter()
        .filter(|record| media_record_available(record))
        .filter_map(|record| media_path(record))
        .filter(|path| path.starts_with("media/event-frames/"))
        .collect::<Vec<_>>();

    if event_frame_records.is_empty() {
        return Vec::new();
    }

    vec![CandidateFrameSet {
        id: "bundle-media".to_string(),
        root: "media/event-frames".to_string(),
        fps: 1.0,
        count: event_frame_records.len(),
        naming: event_frame_records
            .first()
            .and_then(|path| infer_frame_naming(path))
            .unwrap_or_else(|| "frame-{index:03}.jpg".to_string()),
    }]
}

fn media_path(record: &Value) -> Option<&str> {
    record
        .get("path")
        .or_else(|| record.get("uri"))
        .or_else(|| record.get("media_ref"))
        .and_then(Value::as_str)
}

fn parent_relative_path(path: &str) -> Option<String> {
    Path::new(path)
        .parent()
        .map(|parent| parent.to_string_lossy().replace('\\', "/"))
        .filter(|parent| !parent.is_empty())
}

fn infer_frame_naming(path: &str) -> Option<String> {
    let file_name = Path::new(path).file_name()?.to_string_lossy();
    let extension = Path::new(file_name.as_ref())
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("jpg");
    let stem = Path::new(file_name.as_ref()).file_stem()?.to_string_lossy();
    let digits_start = stem
        .char_indices()
        .find_map(|(index, character)| character.is_ascii_digit().then_some(index))?;
    let (prefix, digits) = stem.split_at(digits_start);
    if digits.is_empty() || !digits.chars().all(|character| character.is_ascii_digit()) {
        return None;
    }
    Some(format!("{prefix}{{index:{:02}}}.{extension}", digits.len()))
}

fn candidate_frame_sets_json(frame_sets: &[CandidateFrameSet]) -> Vec<Value> {
    frame_sets
        .iter()
        .map(|frame_set| {
            json!({
                "id": frame_set.id,
                "root": frame_set.root,
                "fps": frame_set.fps,
                "count": frame_set.count,
                "naming": frame_set.naming
            })
        })
        .collect()
}

fn dashboard_frame_cache_json(fixture: &AttentionFixture) -> Value {
    let Some(frame_set) = preferred_dashboard_frame_set(fixture) else {
        return Value::Null;
    };

    let format = naming_extension(&frame_set.naming).unwrap_or("jpg");
    let (frame_width, frame_height) =
        first_frame_dimensions(&fixture.root.join(&frame_set.root), &frame_set.naming)
            .unwrap_or((1, 1));

    json!({
        "root": frame_set.root,
        "index_ref": "media/media.index.jsonl",
        "frame_width": frame_width,
        "frame_height": frame_height,
        "fps": frame_set.fps,
        "format": format
    })
}

fn preferred_dashboard_frame_set(fixture: &AttentionFixture) -> Option<&CandidateFrameSet> {
    fixture
        .session
        .media
        .candidate_frame_sets
        .iter()
        .find(|set| set.id == "2fps")
        .or_else(|| fixture.session.media.candidate_frame_sets.first())
}

fn dashboard_review_frame_cache_json(bundle_run: &BundleRun) -> Option<(Value, Vec<Value>)> {
    let fixture = &bundle_run.fixture;
    let frame_set = preferred_dashboard_frame_set(fixture)?;
    let source_root = fixture.root.join(&frame_set.root);
    let (_, source_height) = first_frame_dimensions(&source_root, &frame_set.naming)?;
    if source_height <= DASHBOARD_REVIEW_FRAME_MAX_HEIGHT {
        return None;
    }

    let work_dir = bundle_run.dashboard_session_path.parent()?;
    let review_root = work_dir.join("review-frame-cache-720p");
    let (frame_width, frame_height) = generate_review_frame_cache(
        &source_root,
        &review_root,
        frame_set,
        DASHBOARD_REVIEW_FRAME_MAX_HEIGHT,
    )
    .ok()?;

    let format = naming_extension(&frame_set.naming).unwrap_or("jpg");
    let mut frame_sets = fixture.session.media.candidate_frame_sets.clone();
    for candidate_set in &mut frame_sets {
        if candidate_set.id == frame_set.id {
            candidate_set.root = review_root.display().to_string();
        }
    }

    Some((
        json!({
            "root": review_root,
            "index_ref": "media/media.index.jsonl",
            "frame_width": frame_width,
            "frame_height": frame_height,
            "fps": frame_set.fps,
            "format": format
        }),
        candidate_frame_sets_json(&frame_sets),
    ))
}

fn generate_review_frame_cache(
    source_root: &Path,
    review_root: &Path,
    frame_set: &CandidateFrameSet,
    max_height: u32,
) -> Result<(u32, u32)> {
    fs::create_dir_all(review_root).with_context(|| format!("create {}", review_root.display()))?;

    let mut output_dimensions = None;
    for frame_index in 1..=frame_set.count {
        let file_name = frame_file_name(&frame_set.naming, frame_index);
        let source_path = source_root.join(&file_name);
        let target_path = review_root.join(&file_name);
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }

        let image = image::open(&source_path)
            .with_context(|| format!("open review source frame {}", source_path.display()))?;
        let (source_width, source_height) = image.dimensions();
        let (target_width, target_height) =
            scaled_dimensions(source_width, source_height, max_height);
        let output = if source_height > max_height {
            image.resize_exact(
                target_width,
                target_height,
                image::imageops::FilterType::Lanczos3,
            )
        } else {
            image
        };
        output
            .save(&target_path)
            .with_context(|| format!("write review frame {}", target_path.display()))?;
        output_dimensions.get_or_insert((target_width, target_height));
    }

    output_dimensions.ok_or_else(|| anyhow!("frame set {} has no frames", frame_set.id))
}

fn scaled_dimensions(source_width: u32, source_height: u32, max_height: u32) -> (u32, u32) {
    if source_height == 0 || source_width == 0 || source_height <= max_height {
        return (source_width.max(1), source_height.max(1));
    }

    let mut target_width = ((source_width as u64 * max_height as u64 + source_height as u64 / 2)
        / source_height as u64)
        .max(1) as u32;
    if target_width % 2 == 1 {
        target_width += 1;
    }
    (target_width, max_height)
}

fn frame_file_name(naming: &str, frame_index: usize) -> String {
    naming
        .replace("{index:06}", &format!("{frame_index:06}"))
        .replace("{index:03}", &format!("{frame_index:03}"))
        .replace("{index}", &frame_index.to_string())
}

fn naming_extension(naming: &str) -> Option<&'static str> {
    match Path::new(naming)
        .extension()
        .and_then(|extension| extension.to_str())
    {
        Some("png") => Some("png"),
        Some("jpeg" | "jpg") => Some("jpg"),
        _ => None,
    }
}

fn first_frame_dimensions(root: &Path, naming: &str) -> Option<(u32, u32)> {
    let first_frame_name = naming
        .replace("{index:06}", "000001")
        .replace("{index:03}", "001")
        .replace("{index}", "1");
    image::image_dimensions(root.join(first_frame_name)).ok()
}

fn compatibility_report(
    bundle_path: &Path,
    manifest: &Value,
    fixture: &AttentionFixture,
    media_index: &[Value],
) -> Value {
    let sources = read_json(&bundle_path.join("sources.json")).unwrap_or_else(|_| json!({}));
    let known_gap_count = fs::read_to_string(bundle_path.join("quality/known_gaps.jsonl"))
        .map(|text| text.lines().filter(|line| !line.trim().is_empty()).count())
        .unwrap_or(0);
    let media_available = media_index
        .iter()
        .filter(|record| media_record_available(record))
        .count();
    let media_omitted = media_index
        .iter()
        .filter(|record| record.get("status").and_then(Value::as_str) == Some("candidate_omitted"))
        .count();

    json!({
        "schema_version": 1,
        "surface": "attention_bundle_compatibility",
        "capture_id": manifest.get("capture_id").and_then(Value::as_str),
        "bundle_path": bundle_path,
        "compatible_for_algorithm_events": !fixture.events.is_empty(),
        "compatible_for_visual_dashboard": media_available > 0,
        "event_count": fixture.events.len(),
        "event_ref_count": fixture.session.inputs.event_refs.len(),
        "candidate_source": if fixture.session.media.candidate_frame_sets.is_empty() {
            "event_times_no_media"
        } else {
            "bundle_media_index"
        },
        "candidate_frame_set_count": fixture.session.media.candidate_frame_sets.len(),
        "media_index_count": media_index.len(),
        "media_available_count": media_available,
        "media_candidate_omitted_count": media_omitted,
        "known_gap_count": known_gap_count,
        "sources": sources,
        "problems": compatibility_problems(fixture, media_index, known_gap_count),
        "migration_notes": [
            "READY bundles are input evidence and intentionally do not contain attention-filter output.",
            "The current dashboard can open generated sessions, but bundle V0 may have no video/frame cache.",
            "When media is absent the runner uses event-time candidates so algorithm development can still exercise AX/UX/SCK fusion."
        ]
    })
}

fn compatibility_problems(
    fixture: &AttentionFixture,
    media_index: &[Value],
    known_gap_count: usize,
) -> Vec<Value> {
    let mut problems = Vec::new();
    if fixture.events.is_empty() {
        problems.push(json!({
            "severity": "error",
            "code": "no_events",
            "message": "The bundle has no timeline events for the attention runner to score."
        }));
    }
    if fixture.session.media.candidate_frame_sets.is_empty() {
        problems.push(json!({
            "severity": "error",
            "code": "no_fixed_frame_cache",
            "message": "The attention dashboard needs frame_2fps media records from media/frames-2fps/ to review bundle output visually."
        }));
    }
    if media_index
        .iter()
        .any(|record| record.get("status").and_then(Value::as_str) == Some("candidate_omitted"))
    {
        problems.push(json!({
            "severity": "info",
            "code": "media_candidates_omitted",
            "message": "SCK indicated possible keyframes but the exporter did not include media artifacts."
        }));
    }
    if known_gap_count > 0 {
        problems.push(json!({
            "severity": "info",
            "code": "known_gaps_present",
            "message": "The bundle declares degraded or inferred lanes in quality/known_gaps.jsonl.",
            "known_gap_count": known_gap_count
        }));
    }
    problems
}

fn media_record_available(record: &Value) -> bool {
    matches!(
        record.get("status").and_then(Value::as_str),
        Some("available" | "referenced")
    ) || record.get("path").is_some()
        || record.get("uri").is_some()
        || record.get("media_ref").is_some()
}

fn dashboard_session_json(bundle_run: &BundleRun) -> Result<Value> {
    let fixture = &bundle_run.fixture;
    let (frame_cache, candidate_frame_sets) = dashboard_review_frame_cache_json(bundle_run)
        .unwrap_or_else(|| {
            (
                dashboard_frame_cache_json(fixture),
                candidate_frame_sets_json(&fixture.session.media.candidate_frame_sets),
            )
        });
    let playback_mode = if frame_cache.is_null() {
        "bundle_event_candidates"
    } else {
        "frame_cache"
    };
    let mut event_refs = fixture
        .session
        .inputs
        .event_refs
        .iter()
        .map(|event_ref| {
            json!({
                "id": event_ref.id,
                "kind": event_ref.kind,
                "ref": fixture.root.join(&event_ref.path),
                "format": event_ref.format,
                "required": event_ref.required
            })
        })
        .collect::<Vec<_>>();
    if let Some(event_ref) =
        write_visual_frame_change_events(bundle_run, &frame_cache, &candidate_frame_sets)?
    {
        event_refs.push(event_ref);
    }

    Ok(json!({
        "schema_version": "attention-dashboard.v1",
        "session_id": fixture.session.session_id,
        "title": fixture.session.title,
        "created_at": fixture.session.created_at,
        "fixture": {
            "run_id": fixture.session.fixture.run_id,
            "root": fixture.root,
            "duration_ms": fixture.session.fixture.duration_ms,
            "timezone": "UTC",
            "notes": "Generated from a READY capture bundle. The dashboard plays a time-synced review frame cache; original bundle frames remain source evidence.",
            "source_manifest_ref": fixture.root.join("manifest.json"),
            "source_readme_ref": Value::Null
        },
        "media": {
            "video_ref": fixture.root.join("media/media.index.jsonl"),
            "video_width": fixture.session.media.video_width,
            "video_height": fixture.session.media.video_height,
            "video_duration_ms": fixture.session.fixture.duration_ms,
            "video_fps": Value::Null,
            "playback_mode": playback_mode,
            "frame_cache": frame_cache,
            "candidate_frame_sets": candidate_frame_sets
        },
        "inputs": {
            "candidate_index_ref": Value::Null,
            "snapshots_root": Value::Null,
            "event_refs": event_refs,
            "timeline_lanes": timeline_lanes_json()
        },
        "filter_output": {
            "ref": bundle_run.output_path,
            "schema_version": "attention-ledger.v3",
            "generated_by": "onecontext-attention-runner --bundle",
            "generated_at": Utc::now().to_rfc3339()
        },
        "review": {
            "labels_ref": "review-labels.jsonl",
            "autosave": true,
            "allowed_labels": [
                "must_save",
                "good_save",
                "acceptable_drop",
                "bad_save",
                "missed_save",
                "wrong_region",
                "wrong_reason",
                "too_sensitive",
                "not_sensitive"
            ],
            "required_metrics": [
                "must_save_recall",
                "bad_save_rate",
                "compression_ratio",
                "stable_outcome_accuracy",
                "region_quality",
                "reason_quality",
                "sensitivity_quality"
            ]
        },
        "ui": {
            "default_left_panel": "video",
            "default_right_panel": "current_decision",
            "default_bottom_panel": "timeline",
            "enabled_tabs": ["saved_states", "raw_buffer", "agent_packet", "metrics"],
            "feature_flags": {
                "algorithm_toggles": false,
                "ablation_compare": false,
                "label_export": true,
                "overlay_editor": false,
                "side_by_side_runs": false
            }
        },
        "agent_work_packages": []
    }))
}

fn write_visual_frame_change_events(
    bundle_run: &BundleRun,
    frame_cache: &Value,
    candidate_frame_sets: &[Value],
) -> Result<Option<Value>> {
    let Some(source) = visual_frame_source(frame_cache, candidate_frame_sets) else {
        return Ok(None);
    };
    let Some(start_time) = bundle_manifest_start_time(&bundle_run.fixture.root) else {
        return Ok(None);
    };

    let mut previous: Option<(usize, VisualFrameFingerprint)> = None;
    let mut lines = Vec::new();
    let root = if source.root.is_absolute() {
        source.root.clone()
    } else {
        bundle_run.fixture.root.join(&source.root)
    };

    for frame_index in 1..=source.count {
        let frame_name = frame_file_name(&source.naming, frame_index);
        let frame_path = root.join(&frame_name);
        let Ok(fingerprint) = visual_frame_fingerprint(&frame_path) else {
            continue;
        };

        if let Some((previous_index, previous_fingerprint)) = previous.as_ref() {
            let diff = visual_frame_diff(previous_fingerprint, &fingerprint);
            if diff.full_score >= VISUAL_FRAME_CHANGE_FULL_DIFF_THRESHOLD
                || diff.top_band_score >= VISUAL_FRAME_CHANGE_TOP_DIFF_THRESHOLD
            {
                let t_ms = (((frame_index.saturating_sub(1)) as f64 / source.fps as f64) * 1000.0)
                    .round() as u64;
                let previous_t_ms =
                    (((previous_index.saturating_sub(1)) as f64 / source.fps as f64) * 1000.0)
                        .round() as u64;
                let event_time =
                    start_time + Duration::milliseconds(i64::try_from(t_ms).unwrap_or(i64::MAX));
                let reason = if diff.top_band_score >= VISUAL_FRAME_CHANGE_TOP_DIFF_THRESHOLD {
                    "top/window band changed between adjacent review frames"
                } else {
                    "large visual change between adjacent review frames"
                };
                let record = json!({
                    "eventType": "attention.derived.visual_frame_change.v1",
                    "sourceRecordID": format!("derived-visual-frame-change-{frame_index:06}"),
                    "event_time_start": event_time.to_rfc3339_opts(SecondsFormat::Millis, true),
                    "event_time_end": event_time.to_rfc3339_opts(SecondsFormat::Millis, true),
                    "payload": {
                        "kind": "visual_frame_change",
                        "source": "review_frame_cache",
                        "from_frame": previous_index,
                        "to_frame": frame_index,
                        "from_time_ms": previous_t_ms,
                        "to_time_ms": t_ms,
                        "full_diff_score": rounded_score(diff.full_score),
                        "top_band_diff_score": rounded_score(diff.top_band_score),
                        "full_diff_threshold": VISUAL_FRAME_CHANGE_FULL_DIFF_THRESHOLD,
                        "top_band_diff_threshold": VISUAL_FRAME_CHANGE_TOP_DIFF_THRESHOLD,
                        "reason": reason,
                        "frame_ref": root.join(&frame_name),
                    }
                });
                lines.push(serde_json::to_string(&record)?);
            }
        }

        previous = Some((frame_index, fingerprint));
    }

    if lines.is_empty() {
        return Ok(None);
    }

    let Some(work_dir) = bundle_run.dashboard_session_path.parent() else {
        return Ok(None);
    };
    let path = work_dir.join(DERIVED_VISUAL_FRAME_CHANGES_FILE);
    fs::write(&path, format!("{}\n", lines.join("\n")))
        .with_context(|| format!("write {}", path.display()))?;

    Ok(Some(json!({
        "id": "derived-visual-frame-changes",
        "kind": "capture_events",
        "ref": path,
        "format": "jsonl",
        "required": false
    })))
}

fn visual_frame_source(
    frame_cache: &Value,
    candidate_frame_sets: &[Value],
) -> Option<VisualFrameSource> {
    let root = frame_cache.get("root")?.as_str()?;
    let fps = frame_cache.get("fps")?.as_f64()?.max(0.1) as f32;
    let frame_set = candidate_frame_sets
        .iter()
        .find(|set| set.get("root").and_then(Value::as_str) == Some(root))
        .or_else(|| candidate_frame_sets.first())?;
    let count = frame_set.get("count")?.as_u64()? as usize;
    let naming = frame_set.get("naming")?.as_str()?.to_string();
    Some(VisualFrameSource {
        root: PathBuf::from(root),
        fps,
        count,
        naming,
    })
}

fn bundle_manifest_start_time(bundle_path: &Path) -> Option<DateTime<Utc>> {
    let manifest = read_json(&bundle_path.join("manifest.json")).ok()?;
    manifest_time(&manifest, "time_start")
        .or_else(|| manifest_time_path(&manifest, &["time_range", "start"]))
}

fn visual_frame_fingerprint(path: &Path) -> Result<VisualFrameFingerprint> {
    let image = image::open(path)
        .with_context(|| format!("open visual diff frame {}", path.display()))?
        .resize_exact(
            VISUAL_DIFF_THUMB_WIDTH,
            VISUAL_DIFF_THUMB_HEIGHT,
            image::imageops::FilterType::Triangle,
        )
        .to_luma8();
    Ok(VisualFrameFingerprint {
        pixels: image.into_raw(),
        width: VISUAL_DIFF_THUMB_WIDTH as usize,
        height: VISUAL_DIFF_THUMB_HEIGHT as usize,
    })
}

fn visual_frame_diff(
    previous: &VisualFrameFingerprint,
    current: &VisualFrameFingerprint,
) -> VisualFrameDiff {
    let len = previous.pixels.len().min(current.pixels.len());
    if len == 0 {
        return VisualFrameDiff {
            full_score: 0.0,
            top_band_score: 0.0,
        };
    }

    let full_score = mean_abs_diff(&previous.pixels[..len], &current.pixels[..len]);
    let top_rows = (previous.height / 6).max(1);
    let top_len = (previous.width * top_rows).min(len);
    let top_band_score = mean_abs_diff(&previous.pixels[..top_len], &current.pixels[..top_len]);
    VisualFrameDiff {
        full_score,
        top_band_score,
    }
}

fn mean_abs_diff(previous: &[u8], current: &[u8]) -> f32 {
    let len = previous.len().min(current.len());
    if len == 0 {
        return 0.0;
    }
    let total = previous
        .iter()
        .zip(current.iter())
        .take(len)
        .map(|(left, right)| left.abs_diff(*right) as u64)
        .sum::<u64>();
    total as f32 / (len as f32 * 255.0)
}

fn rounded_score(score: f32) -> f32 {
    (score * 1000.0).round() / 1000.0
}

struct VisualFrameSource {
    root: PathBuf,
    fps: f32,
    count: usize,
    naming: String,
}

struct VisualFrameFingerprint {
    pixels: Vec<u8>,
    width: usize,
    height: usize,
}

struct VisualFrameDiff {
    full_score: f32,
    top_band_score: f32,
}

fn timeline_lanes_json() -> Vec<Value> {
    vec![
        lane(
            "candidate-frames",
            "Candidates",
            "candidate_frames",
            true,
            "#7c8cff",
            "event candidates",
        ),
        lane(
            "saved-states",
            "Saved States",
            "saved_states",
            true,
            "#35b779",
            "attention-filter-output.json",
        ),
        lane(
            "keyboard",
            "Keyboard",
            "keyboard",
            true,
            "#f59e0b",
            "events/ux.events.jsonl",
        ),
        lane(
            "pointer",
            "Pointer",
            "pointer",
            true,
            "#38bdf8",
            "events/ux.events.jsonl",
        ),
        lane(
            "scroll",
            "Scroll",
            "scroll",
            true,
            "#a78bfa",
            "events/ux.events.jsonl",
        ),
        lane(
            "selection",
            "Selection",
            "selection",
            true,
            "#facc15",
            "events/ax.events.jsonl",
        ),
        lane(
            "window-changes",
            "Window Changes",
            "window_changes",
            true,
            "#60a5fa",
            "events/ax.events.jsonl",
        ),
        lane(
            "focus-transitions",
            "Focus Transitions",
            "focus_transitions",
            true,
            "#2dd4bf",
            "events/ux.events.jsonl",
        ),
        lane(
            "focused-elements",
            "Focused Elements",
            "focused_elements",
            true,
            "#f472b6",
            "events/ax.events.jsonl",
        ),
        lane(
            "focus-samples",
            "Focus Samples",
            "focus_samples_debug",
            false,
            "#64748b",
            "events/ax.events.jsonl",
        ),
        lane(
            "visual-frame-changes",
            "Visual Changes",
            "visual_frame_changes",
            true,
            "#f97316",
            "derived-visual-frame-changes",
        ),
        lane(
            "visual-active-window",
            "SCK Target",
            "sck_target_debug",
            true,
            "#94a3b8",
            "events/sck-frame-metadata.events.jsonl",
        ),
        lane(
            "attention-debt",
            "Debt",
            "attention_debt",
            true,
            "#fb7185",
            "attention-filter-output.json",
        ),
        lane(
            "review-labels",
            "Review Labels",
            "review_labels",
            true,
            "#e879f9",
            "review-labels.jsonl",
        ),
    ]
}

fn lane(id: &str, title: &str, kind: &str, visible: bool, color: &str, source_ref: &str) -> Value {
    json!({
        "id": id,
        "title": title,
        "kind": kind,
        "visible": visible,
        "color": color,
        "source_ref": source_ref
    })
}

fn bundle_work_dir(capture_id: &str, output_path: Option<&Path>) -> Result<PathBuf> {
    if let Some(output_path) = output_path.and_then(Path::parent) {
        return Ok(output_path.to_path_buf());
    }
    Ok(std::env::current_dir()?
        .join("target")
        .join("attention-bundle-runs")
        .join(capture_id))
}

fn ensure_output_is_outside_bundle(bundle_path: &Path, output_path: &Path) -> Result<()> {
    let output_parent = output_path.parent().unwrap_or_else(|| Path::new("."));
    let output_parent = if output_parent.exists() {
        fs::canonicalize(output_parent).unwrap_or_else(|_| output_parent.to_path_buf())
    } else {
        output_parent.to_path_buf()
    };
    if output_parent.starts_with(bundle_path) {
        bail!(
            "attention output must not be written inside the capture bundle: {}",
            output_path.display()
        );
    }
    Ok(())
}

fn manifest_time(manifest: &Value, key: &str) -> Option<DateTime<Utc>> {
    manifest
        .get(key)
        .and_then(Value::as_str)
        .and_then(parse_datetime)
}

fn manifest_time_path(manifest: &Value, path: &[&str]) -> Option<DateTime<Utc>> {
    let mut cursor = manifest;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    cursor.as_str().and_then(parse_datetime)
}

fn parse_datetime(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn read_json(path: &Path) -> Result<Value> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)
        .with_context(|| format!("write {}", path.display()))
}

fn normalize_existing_path(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path).with_context(|| format!("canonicalize {}", path.display()))
}
