use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use onecontext_capture_core::{
    export_ready_bundle, list_bundles, plan_retention_sweep, read_spool_window, sweep_bundles,
    validate_ready_bundle, BundleDirectoryClass, BundleEntry, BundleState, CaptureRootPaths,
    CaptureTarget, ExportRequest, RetentionPolicy, SpoolQuery, SweepActionKind, CONTRACT_VERSION,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const SURFACE: &str = "capture_bundler";

fn main() {
    if let Err(error) = run() {
        let payload = json!({
            "schema_version": 1,
            "surface": SURFACE,
            "status": "error",
            "error": {
                "code": "capture_bundler_error",
                "message": error.to_string(),
            },
            "repair_hints": [
                "Run onecontext-capture-bundler --help for supported arguments.",
                "Pass --capture-root when testing outside the installed app's Application Support tree."
            ]
        });
        eprintln!("{}", serde_json::to_string_pretty(&payload).unwrap());
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let mut args = env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() || matches!(args[0].as_str(), "--help" | "-h" | "help") {
        print_usage();
        return Ok(());
    }

    let command = args.remove(0);
    match command.as_str() {
        "export" => export_command(args),
        "list" => list_command(args),
        "validate" => validate_command(args),
        "sweep" => sweep_command(args),
        "describe" => describe_command(args),
        "status" => status_command(args),
        other => bail!("unknown command {other:?}"),
    }
}

fn print_usage() {
    eprintln!(
        "usage:\n  onecontext-capture-bundler export [--start <rfc3339>] [--end <rfc3339>] [--visual-recording-json PATH] [--capture-root PATH] [--capture-id ID] [--target active-window|all-windows|custom:<value>] [--frames-2fps-dir PATH] [--debug-pin] [--status-json PATH] [--ux-status-json PATH] [--sampler-json PATH] [--browser-proof-json PATH] [--dry-run]\n  onecontext-capture-bundler list [--capture-root PATH] [--class live|processing|failed|pinned|all]\n  onecontext-capture-bundler validate (--bundle PATH | --capture-id ID [--capture-root PATH]) [--strict]\n  onecontext-capture-bundler sweep [--capture-root PATH] [--processing-max-age-seconds N] [--live-max-age-seconds N] [--failed-max-age-seconds N] [--keep-live N] [--apply]\n  onecontext-capture-bundler status [--capture-root PATH]\n  onecontext-capture-bundler describe"
    );
}

fn export_command(mut args: Vec<String>) -> Result<()> {
    let capture_root = capture_root_from_args(&mut args)?;
    let start_arg = optional_datetime(&mut args, "--start")?;
    let end_arg = optional_datetime(&mut args, "--end")?;
    let visual_recording_json_path = take_option_path(&mut args, "--visual-recording-json");
    let visual_recording = read_visual_recording_json(visual_recording_json_path.as_deref())?;
    let start = match start_arg {
        Some(start) => start,
        None => visual_recording
            .as_ref()
            .map(|recording| recording.capture_started_at)
            .ok_or_else(|| anyhow!("missing --start or --visual-recording-json"))?,
    };
    let end = match end_arg {
        Some(end) => end,
        None => visual_recording
            .as_ref()
            .map(|recording| recording.capture_ended_at)
            .ok_or_else(|| anyhow!("missing --end or --visual-recording-json"))?,
    };
    if end <= start {
        bail!("--end must be later than --start");
    }

    let capture_id_hint = take_option_value(&mut args, "--capture-id")
        .unwrap_or_else(|| default_capture_id(start, end));
    let target = parse_target(
        &take_option_value(&mut args, "--target").unwrap_or_else(|| "active-window".to_string()),
    )?;
    let debug_pin = take_flag(&mut args, "--debug-pin");
    let dry_run = take_flag(&mut args, "--dry-run");
    let status_json_path = take_option_path(&mut args, "--status-json");
    let ux_status_json_path = take_option_path(&mut args, "--ux-status-json");
    let sampler_json_path = take_option_path(&mut args, "--sampler-json");
    let browser_proof_json_path = take_option_path(&mut args, "--browser-proof-json");
    let frames_2fps_dir_arg = take_option_path(&mut args, "--frames-2fps-dir");
    let debug_video_path_arg = take_option_path(&mut args, "--debug-video");
    let frames_2fps_dir_explicit = frames_2fps_dir_arg.is_some();
    if debug_video_path_arg.is_some() {
        bail!("--debug-video is retired; READY bundles use frame_2fps media only");
    }
    let debug_video_path_explicit = false;
    let frames_2fps_dir = frames_2fps_dir_arg.or_else(|| {
        visual_recording
            .as_ref()
            .map(|recording| recording.frames_dir.clone())
    });
    let debug_video_path = None;
    reject_extra_args(&args)?;

    if dry_run {
        let plan = dry_run_export_plan(&capture_root, start, end)?;
        return print_json(&json!({
            "schema_version": 1,
            "surface": SURFACE,
            "status": "ok",
            "operation": "export",
            "mode": "dry_run",
            "dry_run": true,
            "will_write": false,
            "capture_id_hint": capture_id_hint,
            "capture_id_override_supported": false,
            "capture_root": capture_root,
            "bundle_root": CaptureRootPaths::new(&capture_root).bundles_dir,
            "time_range": time_range_json(start, end),
            "target": target_label(&target),
            "debug_pin": debug_pin,
            "contract_version": CONTRACT_VERSION,
            "capability_inputs": capability_inputs_json(
                &status_json_path,
                &ux_status_json_path,
                &sampler_json_path,
                &browser_proof_json_path,
            ),
            "visual_inputs": visual_inputs_json(
                &capture_root,
                &visual_recording_json_path,
                &frames_2fps_dir,
                frames_2fps_dir_explicit,
                &debug_video_path,
                debug_video_path_explicit,
            ),
            "spool_window": plan,
        }));
    }

    let request = ExportRequest {
        capture_root: capture_root.clone(),
        time_start: start,
        time_end: end,
        target: target.clone(),
        debug_pin,
        frames_2fps_dir: frames_2fps_dir.clone(),
        debug_video_path: debug_video_path.clone(),
        status_json: read_optional_json(status_json_path.as_deref(), "--status-json")?,
        ux_status_json: read_optional_json(ux_status_json_path.as_deref(), "--ux-status-json")?,
        sampler_json: read_optional_json(sampler_json_path.as_deref(), "--sampler-json")?,
        browser_proof_json: read_optional_json(
            browser_proof_json_path.as_deref(),
            "--browser-proof-json",
        )?,
        source_envelope_paths: Vec::new(),
    };
    let response = export_ready_bundle(request)?;
    let validation = if let Some(validation) = response.validation.clone() {
        validation
    } else {
        validate_ready_bundle(&response.bundle_path)?
    };
    let status = if response.state == "ready" && validation.ok {
        "ok"
    } else {
        "invalid"
    };

    print_json(&json!({
        "schema_version": 1,
        "surface": SURFACE,
        "status": status,
        "operation": "export",
        "mode": "apply",
        "dry_run": false,
        "capture_id": response.capture_id,
        "capture_id_hint": capture_id_hint,
        "capture_id_override_supported": false,
        "capture_root": capture_root,
        "bundle_root": CaptureRootPaths::new(&capture_root).bundles_dir,
        "time_range": time_range_json(start, end),
        "target": target_label(&target),
        "debug_pin": debug_pin,
        "visual_inputs": visual_inputs_json(
            &capture_root,
            &visual_recording_json_path,
            &frames_2fps_dir,
            frames_2fps_dir_explicit,
            &debug_video_path,
            debug_video_path_explicit,
        ),
        "contract_version": CONTRACT_VERSION,
        "bundle": response,
        "validation": validation,
    }))?;
    if status == "invalid" {
        std::process::exit(1);
    }
    Ok(())
}

fn list_command(mut args: Vec<String>) -> Result<()> {
    let capture_root = capture_root_from_args(&mut args)?;
    let class_filter =
        take_option_value(&mut args, "--class").unwrap_or_else(|| "live".to_string());
    reject_extra_args(&args)?;

    let mut entries = list_bundles(&capture_root)?.entries;
    entries.retain(|entry| class_matches(entry, &class_filter));
    if class_filter != "all" && !valid_class_filter(&class_filter) {
        bail!("unknown --class {class_filter:?}");
    }

    let bundles = entries
        .iter()
        .map(bundle_summary)
        .collect::<Vec<BundleSummary>>();
    print_json(&json!({
        "schema_version": 1,
        "surface": SURFACE,
        "status": "ok",
        "operation": "list",
        "capture_root": capture_root,
        "class": class_filter,
        "bundle_count": bundles.len(),
        "bundles": bundles,
    }))
}

fn validate_command(mut args: Vec<String>) -> Result<()> {
    let strict = take_flag(&mut args, "--strict");
    let bundle_path = if let Some(bundle) = take_option_value(&mut args, "--bundle") {
        PathBuf::from(bundle)
    } else if let Some(capture_id) = take_option_value(&mut args, "--capture-id") {
        let capture_root = capture_root_from_args(&mut args)?;
        find_bundle_by_capture_id(&capture_root, &capture_id)?
    } else {
        bail!("validate requires --bundle PATH or --capture-id ID");
    };
    reject_extra_args(&args)?;

    let report = validate_ready_bundle(&bundle_path)?;
    let status = if report.ok { "ok" } else { "invalid" };
    print_json(&json!({
        "schema_version": 1,
        "surface": SURFACE,
        "status": status,
        "operation": "validate",
        "strict": strict,
        "validator": "onecontext_capture_core::validate_ready_bundle",
        "bundle_path": bundle_path,
        "bundle": report,
    }))?;
    if status == "invalid" {
        std::process::exit(1);
    }
    Ok(())
}

fn sweep_command(mut args: Vec<String>) -> Result<()> {
    let capture_root = capture_root_from_args(&mut args)?;
    let apply = take_flag(&mut args, "--apply");
    let policy = RetentionPolicy {
        processing_stale_after_seconds: option_seconds(
            &mut args,
            "--processing-max-age-seconds",
            15 * 60,
        )?,
        live_ttl_seconds: option_seconds(&mut args, "--live-max-age-seconds", 60 * 60)?,
        failed_ttl_seconds: option_seconds(&mut args, "--failed-max-age-seconds", 72 * 60 * 60)?,
        keep_last_ready: take_option_value(&mut args, "--keep-live")
            .map(|value| {
                value
                    .parse::<usize>()
                    .context("--keep-live must be an integer")
            })
            .transpose()?
            .unwrap_or(10),
        dry_run: !apply,
    };
    reject_extra_args(&args)?;

    let now = Utc::now();
    if apply {
        let report = sweep_bundles(&capture_root, &policy, now)?;
        print_json(&json!({
            "schema_version": 1,
            "surface": SURFACE,
            "status": if report.errors.is_empty() { "ok" } else { "error" },
            "operation": "sweep",
            "capture_root": capture_root,
            "mode": "apply",
            "dry_run": false,
            "report": report,
            "retention": retention_json(&policy),
        }))
    } else {
        let plan = plan_retention_sweep(&capture_root, &policy, now)?;
        let actionable_count = plan
            .actions
            .iter()
            .filter(|action| !matches!(action.kind, SweepActionKind::Preserve))
            .count();
        print_json(&json!({
            "schema_version": 1,
            "surface": SURFACE,
            "status": "ok",
            "operation": "sweep",
            "capture_root": capture_root,
            "mode": "dry_run",
            "dry_run": true,
            "candidate_count": actionable_count,
            "plan": plan,
            "retention": retention_json(&policy),
        }))
    }
}

fn describe_command(args: Vec<String>) -> Result<()> {
    reject_extra_args(&args)?;
    print_json(&json!({
        "schema_version": 1,
        "surface": SURFACE,
        "status": "ok",
        "operation": "describe",
        "contract_version": CONTRACT_VERSION,
        "commands": {
            "export": {
                "args": [
                    "--start <rfc3339>",
                    "--end <rfc3339>",
                    "--visual-recording-json <path>",
                    "--capture-root <path>",
                    "--capture-id <id>",
                    "--target active-window|all-windows|custom:<value>",
                    "--frames-2fps-dir <path>",
                    "--debug-pin",
                    "--status-json <path>",
                    "--ux-status-json <path>",
                    "--sampler-json <path>",
                    "--browser-proof-json <path>",
                    "--dry-run"
                ],
                "status": "implemented",
                "implementation": "onecontext_capture_core::export_ready_bundle",
                "dry_run": "reads spool window and reports the non-mutating plan",
                "visual_recording_json": "derives omitted --start/--end from capture_started_at/capture_ended_at and omitted --frames-2fps-dir from frames_dir; recorder debug video paths are not READY media"
            },
            "list": {
                "args": ["--capture-root <path>", "--class live|processing|failed|pinned|all"],
                "status": "implemented",
                "implementation": "onecontext_capture_core::list_bundles"
            },
            "validate": {
                "args": ["--bundle <path>", "--capture-id <id>", "--capture-root <path>", "--strict"],
                "status": "implemented",
                "implementation": "onecontext_capture_core::validate_ready_bundle"
            },
            "sweep": {
                "args": ["--capture-root <path>", "--processing-max-age-seconds <n>", "--live-max-age-seconds <n>", "--failed-max-age-seconds <n>", "--keep-live <n>", "--apply"],
                "status": "implemented",
                "implementation": "onecontext_capture_core::plan_retention_sweep/sweep_bundles",
                "default_mode": "dry_run"
            },
            "status": {
                "args": ["--capture-root <path>"],
                "status": "implemented"
            }
        },
        "default_capture_root": default_capture_root(),
    }))
}

fn status_command(mut args: Vec<String>) -> Result<()> {
    let capture_root = capture_root_from_args(&mut args)?;
    reject_extra_args(&args)?;

    let paths = CaptureRootPaths::new(&capture_root);
    let inventory = list_bundles(&capture_root)?;
    let mut bundle_counts = BTreeMap::<String, usize>::new();
    for entry in &inventory.entries {
        *bundle_counts
            .entry(directory_class_label(&entry.directory_class).to_string())
            .or_default() += 1;
    }
    print_json(&json!({
        "schema_version": 1,
        "surface": SURFACE,
        "status": "ok",
        "operation": "status",
        "capture_root": capture_root,
        "contract_version": CONTRACT_VERSION,
        "directories": {
            "capture_root": dir_status(&paths.capture_root),
            "events": dir_status(&paths.events_dir),
            "windows": dir_status(&paths.windows_dir),
            "displays": dir_status(&paths.displays_dir),
            "media": dir_status(&paths.media_dir),
            "bundles": dir_status(&paths.bundles_dir),
            "processing": dir_status(&paths.processing_dir),
            "live": dir_status(&paths.live_dir),
            "failed": dir_status(&paths.failed_dir),
            "pinned": dir_status(&paths.pinned_dir),
            "retention": dir_status(&paths.retention_dir),
        },
        "bundle_count": inventory.entries.len(),
        "bundle_counts": bundle_counts,
        "total_bytes": inventory.total_bytes,
        "total_files": inventory.total_files,
        "export": {
            "status": "implemented",
            "implementation": "onecontext_capture_core::export_ready_bundle"
        }
    }))
}

fn capture_root_from_args(args: &mut Vec<String>) -> Result<PathBuf> {
    Ok(take_option_value(args, "--capture-root")
        .map(PathBuf::from)
        .or_else(|| env::var_os("ONECONTEXT_CAPTURE_ROOT").map(PathBuf::from))
        .unwrap_or_else(default_capture_root))
}

fn default_capture_root() -> PathBuf {
    if let Some(runtime_home) = env::var_os("ONECONTEXT_DEV_RUNTIME_HOME") {
        return PathBuf::from(runtime_home)
            .join("Library/Application Support/1Context Dev/capture");
    }
    let home = env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let dev = home.join("Library/Application Support/1Context Dev/capture");
    if dev.exists() {
        dev
    } else {
        home.join("Library/Application Support/1Context/capture")
    }
}

fn optional_datetime(args: &mut Vec<String>, name: &str) -> Result<Option<DateTime<Utc>>> {
    take_option_value(args, name)
        .map(|value| parse_rfc3339_datetime(&value, name))
        .transpose()
}

fn parse_rfc3339_datetime(value: &str, name: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(&value)
        .with_context(|| format!("{name} must be RFC3339"))?
        .with_timezone(&Utc))
}

fn default_capture_id(start: DateTime<Utc>, end: DateTime<Utc>) -> String {
    format!(
        "cap_{}_{}ms",
        start.format("%Y%m%dT%H%M%SZ"),
        (end - start).num_milliseconds()
    )
}

fn parse_target(value: &str) -> Result<CaptureTarget> {
    match value {
        "active-window" => Ok(CaptureTarget::ActiveWindow),
        "all-windows" => Ok(CaptureTarget::AllWindows),
        value if value.starts_with("custom:") => {
            let custom = value.trim_start_matches("custom:");
            if custom.is_empty() {
                bail!("--target custom:<value> requires a non-empty value");
            }
            Ok(CaptureTarget::Custom(custom.to_string()))
        }
        other => bail!(
            "unknown --target {other:?}; expected active-window, all-windows, or custom:<value>"
        ),
    }
}

fn target_label(target: &CaptureTarget) -> String {
    match target {
        CaptureTarget::ActiveWindow => "active-window".to_string(),
        CaptureTarget::AllWindows => "all-windows".to_string(),
        CaptureTarget::Custom(value) => format!("custom:{value}"),
    }
}

fn dry_run_export_plan(
    capture_root: &Path,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Value> {
    let records = read_spool_window(&SpoolQuery {
        capture_root: capture_root.to_path_buf(),
        time_start: start,
        time_end: end,
    })?;
    let mut by_event_type = BTreeMap::<String, usize>::new();
    let mut by_source_path = BTreeMap::<String, usize>::new();
    for record in &records {
        *by_event_type
            .entry(record.envelope.event_type.clone())
            .or_default() += 1;
        *by_source_path
            .entry(record.source_path.display().to_string())
            .or_default() += 1;
    }
    let window_snapshot_count = by_event_type
        .get("capture.window_snapshot")
        .copied()
        .unwrap_or_default();
    Ok(json!({
        "record_count": records.len(),
        "window_snapshot_count": window_snapshot_count,
        "by_event_type": by_event_type,
        "by_source_path": by_source_path,
        "ready_validation": {
            "would_pass_minimum_window_check": window_snapshot_count > 0,
            "required_for_ready": "events/windows.jsonl must contain at least one window snapshot"
        }
    }))
}

fn time_range_json(start: DateTime<Utc>, end: DateTime<Utc>) -> Value {
    json!({
        "start": start.to_rfc3339(),
        "end": end.to_rfc3339(),
    })
}

fn take_option_path(args: &mut Vec<String>, name: &str) -> Option<PathBuf> {
    take_option_value(args, name).map(PathBuf::from)
}

fn read_optional_json(path: Option<&Path>, name: &str) -> Result<Option<Value>> {
    path.map(|path| {
        let bytes = fs::read(path).with_context(|| format!("read {name} {}", path.display()))?;
        serde_json::from_slice(&bytes).with_context(|| format!("parse {name} {}", path.display()))
    })
    .transpose()
}

fn read_visual_recording_json(path: Option<&Path>) -> Result<Option<VisualRecordingExportHints>> {
    path.map(|path| {
        let bytes = fs::read(path)
            .with_context(|| format!("read --visual-recording-json {}", path.display()))?;
        parse_visual_recording_json(&bytes)
            .with_context(|| format!("parse --visual-recording-json {}", path.display()))
    })
    .transpose()
}

fn parse_visual_recording_json(bytes: &[u8]) -> Result<VisualRecordingExportHints> {
    let value: Value = serde_json::from_slice(bytes)?;
    let payload = value.get("result").unwrap_or(&value);
    let raw = VisualRecordingJson::deserialize(payload)?;
    let capture_started_at = parse_rfc3339_datetime(&raw.capture_started_at, "capture_started_at")?;
    let capture_ended_at = parse_rfc3339_datetime(&raw.capture_ended_at, "capture_ended_at")?;
    if raw.frames_dir.as_os_str().is_empty() {
        bail!("frames_dir must not be empty");
    }
    Ok(VisualRecordingExportHints {
        capture_started_at,
        capture_ended_at,
        frames_dir: raw.frames_dir,
    })
}

fn capability_inputs_json(
    status_json_path: &Option<PathBuf>,
    ux_status_json_path: &Option<PathBuf>,
    sampler_json_path: &Option<PathBuf>,
    browser_proof_json_path: &Option<PathBuf>,
) -> Value {
    json!({
        "status_json": optional_path_status(status_json_path),
        "ux_status_json": optional_path_status(ux_status_json_path),
        "sampler_json": optional_path_status(sampler_json_path),
        "browser_proof_json": optional_path_status(browser_proof_json_path),
    })
}

fn visual_inputs_json(
    capture_root: &Path,
    visual_recording_json_path: &Option<PathBuf>,
    frames_2fps_dir: &Option<PathBuf>,
    frames_2fps_dir_explicit: bool,
    debug_video_path: &Option<PathBuf>,
    debug_video_path_explicit: bool,
) -> Value {
    let default_frames_dir = capture_root.join("media").join("frames-2fps");
    let effective_frames_dir = frames_2fps_dir.as_ref().unwrap_or(&default_frames_dir);
    let frame_manifest_path = [
        "frames-2fps-manifest.jsonl",
        "frame-manifest.jsonl",
        "manifest.jsonl",
    ]
    .iter()
    .map(|name| effective_frames_dir.join(name))
    .find(|path| path.is_file());
    json!({
        "visual_recording_json": optional_path_status(visual_recording_json_path),
        "frames_2fps_dir": {
            "path": effective_frames_dir,
            "explicit": frames_2fps_dir_explicit,
            "exists": effective_frames_dir.is_dir(),
            "required_for_ready": true,
            "source": "screen_recording_swift_decoder",
            "timing_manifest": optional_path_status(&frame_manifest_path)
        },
        "debug_video": {
            "path": debug_video_path,
            "explicit": debug_video_path_explicit,
            "exists": debug_video_path.as_ref().is_some_and(|path| path.is_file())
        },
    })
}

fn optional_path_status(path: &Option<PathBuf>) -> Value {
    match path {
        Some(path) => json!({"path": path, "exists": path.is_file()}),
        None => json!({"path": null, "exists": false}),
    }
}

fn valid_class_filter(value: &str) -> bool {
    matches!(value, "processing" | "live" | "failed" | "pinned")
}

fn class_matches(entry: &BundleEntry, filter: &str) -> bool {
    filter == "all" || directory_class_label(&entry.directory_class) == filter
}

fn directory_class_label(class: &BundleDirectoryClass) -> &'static str {
    match class {
        BundleDirectoryClass::Processing => "processing",
        BundleDirectoryClass::Live => "live",
        BundleDirectoryClass::Failed => "failed",
        BundleDirectoryClass::Pinned => "pinned",
    }
}

fn state_label(state: &BundleState) -> &'static str {
    match state {
        BundleState::Partial => "partial",
        BundleState::Ready => "ready",
        BundleState::Failed => "failed",
        BundleState::Expired => "expired",
    }
}

fn bundle_summary(entry: &BundleEntry) -> BundleSummary {
    BundleSummary {
        class: directory_class_label(&entry.directory_class).to_string(),
        capture_id: entry.capture_id.clone(),
        path: entry.path.clone(),
        state: entry.state.as_ref().map(state_label).map(str::to_string),
        ready: entry.ready,
        pinned: entry.pinned,
        byte_count: entry.byte_count,
        file_count: entry.file_count,
        created_at: entry.created_at.map(|time| time.to_rfc3339()),
        ready_at: entry.ready_at.map(|time| time.to_rfc3339()),
        expires_at: entry.expires_at.map(|time| time.to_rfc3339()),
    }
}

fn find_bundle_by_capture_id(capture_root: &Path, capture_id: &str) -> Result<PathBuf> {
    let inventory = list_bundles(capture_root)?;
    inventory
        .entries
        .into_iter()
        .find(|entry| entry.capture_id == capture_id)
        .map(|entry| entry.path)
        .ok_or_else(|| {
            anyhow!(
                "capture id {capture_id:?} not found under {}",
                capture_root.display()
            )
        })
}

fn option_seconds(args: &mut Vec<String>, name: &str, default_seconds: i64) -> Result<i64> {
    let seconds = take_option_value(args, name)
        .map(|value| {
            value
                .parse::<i64>()
                .with_context(|| format!("{name} must be an integer"))
        })
        .transpose()?
        .unwrap_or(default_seconds);
    if seconds < 0 {
        bail!("{name} must be non-negative");
    }
    Ok(seconds)
}

fn retention_json(policy: &RetentionPolicy) -> Value {
    json!({
        "processing_max_age_seconds": policy.processing_stale_after_seconds,
        "live_max_age_seconds": policy.live_ttl_seconds,
        "failed_max_age_seconds": policy.failed_ttl_seconds,
        "keep_live": policy.keep_last_ready,
    })
}

fn dir_status(path: &Path) -> Value {
    json!({
        "path": path,
        "exists": path.is_dir(),
    })
}

fn take_option_value(args: &mut Vec<String>, name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;
    args.remove(index);
    if index >= args.len() {
        return None;
    }
    Some(args.remove(index))
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.remove(index);
        true
    } else {
        false
    }
}

fn reject_extra_args(args: &[String]) -> Result<()> {
    if let Some(arg) = args.first() {
        bail!("unknown argument {arg:?}");
    }
    Ok(())
}

fn print_json(value: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[derive(Debug, Serialize)]
struct BundleSummary {
    class: String,
    capture_id: String,
    path: PathBuf,
    state: Option<String>,
    ready: bool,
    pinned: bool,
    byte_count: u64,
    file_count: u64,
    created_at: Option<String>,
    ready_at: Option<String>,
    expires_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct VisualRecordingJson {
    capture_started_at: String,
    capture_ended_at: String,
    frames_dir: PathBuf,
}

#[derive(Debug)]
struct VisualRecordingExportHints {
    capture_started_at: DateTime<Utc>,
    capture_ended_at: DateTime<Utc>,
    frames_dir: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecontext_capture_core::ValidationSeverity;

    #[test]
    fn default_capture_id_uses_start_and_duration() {
        let start = DateTime::parse_from_rfc3339("2026-05-25T19:12:12Z")
            .unwrap()
            .with_timezone(&Utc);
        let end = DateTime::parse_from_rfc3339("2026-05-25T19:13:12Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(
            default_capture_id(start, end),
            "cap_20260525T191212Z_60000ms"
        );
    }

    #[test]
    fn target_parser_accepts_supported_targets() {
        assert_eq!(
            target_label(&parse_target("active-window").unwrap()),
            "active-window"
        );
        assert_eq!(
            target_label(&parse_target("all-windows").unwrap()),
            "all-windows"
        );
        assert_eq!(
            target_label(&parse_target("custom:demo").unwrap()),
            "custom:demo"
        );
        assert!(parse_target("custom:").is_err());
        assert!(parse_target("frontmost").is_err());
    }

    #[test]
    fn validation_severity_stays_serializable_for_json_output() {
        let value = serde_json::to_value(ValidationSeverity::Fatal).unwrap();
        assert_eq!(value, json!("fatal"));
    }

    #[test]
    fn visual_recording_json_provides_export_hints() {
        let hints = parse_visual_recording_json(
            br#"{
                "schema_version": 1,
                "surface": "capture_visual_recording",
                "status": "ok",
                "capture_started_at": "2026-05-27T20:15:10.125Z",
                "capture_ended_at": "2026-05-27T20:16:10.625Z",
                "frames_dir": "/tmp/visual-recordings/run/frames-2fps",
                "video_path": "/tmp/visual-recordings/run/screen-recording.mov"
            }"#,
        )
        .unwrap();

        assert_eq!(
            hints.capture_started_at.to_rfc3339(),
            "2026-05-27T20:15:10.125+00:00"
        );
        assert_eq!(
            hints.capture_ended_at.to_rfc3339(),
            "2026-05-27T20:16:10.625+00:00"
        );
        assert_eq!(
            hints.frames_dir,
            PathBuf::from("/tmp/visual-recordings/run/frames-2fps")
        );
    }

    #[test]
    fn visual_recording_json_accepts_jsonrpc_result_wrapper() {
        let hints = parse_visual_recording_json(
            br#"{
                "jsonrpc": "2.0",
                "id": 1,
                "result": {
                    "capture_started_at": "2026-05-27T20:15:10Z",
                    "capture_ended_at": "2026-05-27T20:16:10Z",
                    "frames_dir": "/tmp/visual-recordings/run/frames-2fps"
                }
            }"#,
        )
        .unwrap();

        assert_eq!(
            hints.capture_started_at.to_rfc3339(),
            "2026-05-27T20:15:10+00:00"
        );
    }

    #[test]
    fn visual_recording_json_requires_recorder_fields() {
        assert!(parse_visual_recording_json(
            br#"{
                "capture_started_at": "2026-05-27T20:15:10Z",
                "capture_ended_at": "2026-05-27T20:16:10Z"
            }"#,
        )
        .is_err());
    }
}
