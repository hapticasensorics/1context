use crate::bundle::schema::{
    BundleState, CaptureBundleManifest, KnownGapRecord, LaneSource, RetentionClass,
    SourceEnvelopeRefKind, SourceEnvelopeRefRecord, SourceInventory, SourceStatus,
    ValidationReport,
};
use crate::bundle::validate::{validate_bundle_candidate, validate_ready_bundle};
use crate::bundle::writer::{compute_tree_totals, AtomicBundleWriter};
use crate::error::{CaptureCoreError, CaptureCoreResult};
use crate::event::RawSpoolRecord;
use crate::lanes::{mandatory_lane_ids, required_bundle_files, CONTRACT_VERSION};
use crate::paths::BundleRelativePath;
use crate::paths::CaptureRootPaths;
use crate::spool::{read_spool_window_report, SpoolQuery, SpoolReadMode, SpoolReadReport};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static CAPTURE_ID_COUNTER: AtomicU64 = AtomicU64::new(1);
const EXPORTER_VERSION: &str = "capture_bundle_exporter.v0.2";
const MAX_BRACKETING_WINDOW_SNAPSHOT_AGE_SECONDS: i64 = 30;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureTarget {
    ActiveWindow,
    AllWindows,
    Custom(String),
}

#[derive(Clone, Debug)]
pub struct ExportRequest {
    pub capture_root: PathBuf,
    pub time_start: DateTime<Utc>,
    pub time_end: DateTime<Utc>,
    pub target: CaptureTarget,
    pub debug_pin: bool,
    pub frames_2fps_dir: Option<PathBuf>,
    pub debug_video_path: Option<PathBuf>,
    pub status_json: Option<Value>,
    pub ux_status_json: Option<Value>,
    pub sampler_json: Option<Value>,
    pub browser_proof_json: Option<Value>,
    pub source_envelope_paths: Vec<PathBuf>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExportResponse {
    pub capture_id: String,
    pub state: String,
    pub bundle_path: PathBuf,
    pub expires_at: Option<DateTime<Utc>>,
    pub byte_count: u64,
    pub file_count: u64,
    pub lane_count: usize,
    pub known_gap_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub validation: Option<ValidationReport>,
}

pub fn export_ready_bundle(request: ExportRequest) -> CaptureCoreResult<ExportResponse> {
    if request.time_end < request.time_start {
        return Err(CaptureCoreError::InvalidTimeRange(
            "time_end must be greater than or equal to time_start".to_string(),
        ));
    }
    if request.debug_video_path.is_some() {
        return Err(CaptureCoreError::InvalidState(
            "debug video is not READY bundle media; keep recordings in dev evidence storage"
                .to_string(),
        ));
    }

    let paths = CaptureRootPaths::new(&request.capture_root);
    paths.ensure_directories()?;
    let capture_id = capture_id(request.time_start);
    let writer = AtomicBundleWriter::create(paths, capture_id.clone())?;
    let optional_files = vec![
        "quality/bracketing_window_snapshot_lookup.json".to_string(),
        "quality/spool_read_report.json".to_string(),
        "quality/raw_provenance.jsonl".to_string(),
    ];
    let mut manifest = CaptureBundleManifest {
        schema_version: 1,
        contract_version: CONTRACT_VERSION.to_string(),
        capture_id: capture_id.clone(),
        state: BundleState::Partial,
        created_at: Utc::now(),
        ready_at: None,
        time_start: request.time_start,
        time_end: request.time_end,
        expires_at: None,
        retention_class: if request.debug_pin {
            RetentionClass::PinnedDebug
        } else {
            RetentionClass::Ephemeral
        },
        pinned: request.debug_pin,
        pin_reason: request.debug_pin.then(|| "debug_pin requested".to_string()),
        source_spool: json!({
            "kind": "onecontext_capture_spool",
            "events": ["capture/events"],
            "windows": ["capture/windows"],
            "displays": ["capture/displays"]
        }),
        v0_required_files: required_bundle_files()
            .iter()
            .map(|path| path.to_string())
            .collect(),
        optional_files,
        byte_count: 0,
        file_count: 0,
        lane_count: 0,
        known_gap_count: 0,
        producer: json!({
            "name": "onecontext-capture-core",
            "role": "bundle_exporter",
            "version": EXPORTER_VERSION
        }),
        app_identity: json!({"capture_root": request.capture_root}),
    };
    writer.write_manifest(&manifest)?;

    let spool_report = read_spool_window_report(
        &SpoolQuery {
            capture_root: request.capture_root.clone(),
            time_start: request.time_start,
            time_end: request.time_end,
        },
        SpoolReadMode::Tolerant,
    )?;
    let records = &spool_report.records;
    let mut routed = RoutedSpool::from_records(records);
    let mut quality = ExportQualityMetadata::default();
    routed.add_bracketing_window_snapshots(
        &request.capture_root,
        request.time_start,
        request.time_end,
        &mut quality,
    )?;
    writer.write_raw_jsonl_lines("events/windows.jsonl", &routed.windows)?;
    writer.write_raw_jsonl_lines("events/displays.jsonl", &routed.displays)?;
    writer.write_raw_jsonl_lines("events/capture.events.jsonl", &routed.capture)?;
    writer.write_raw_jsonl_lines("events/ax.events.jsonl", &routed.ax)?;
    writer.write_raw_jsonl_lines("events/ux.events.jsonl", &routed.ux)?;
    writer.write_raw_jsonl_lines("events/sck-frame-metadata.events.jsonl", &routed.sck)?;
    writer.write_raw_jsonl_lines("events/browser.events.jsonl", &routed.browser)?;
    writer.write_raw_jsonl_lines("events/terminal.events.jsonl", &routed.terminal)?;
    writer.write_raw_jsonl_lines("events/editor.events.jsonl", &routed.editor)?;
    let mut media_index = media_index_for(&routed.sck, &capture_id);
    let (frame_2fps_records, mut media_export_gaps) =
        write_frame_2fps_artifacts(&writer, &request, &capture_id)?;
    let frame_2fps_count = frame_2fps_records.len();
    media_index.extend(frame_2fps_records.clone());
    writer.write_jsonl_values("media/media.index.jsonl", &media_index)?;
    writer.write_json(
        "quality/bracketing_window_snapshot_lookup.json",
        &quality.window_snapshot_lookup,
    )?;
    writer.write_json(
        "quality/spool_read_report.json",
        &spool_read_quality_report(&spool_report),
    )?;
    writer.write_jsonl_values(
        "quality/raw_provenance.jsonl",
        &raw_provenance_for(records, &capture_id),
    )?;
    let mut source_envelope_refs = source_envelope_refs_for(&request.source_envelope_paths);
    if source_envelope_refs.is_empty() {
        source_envelope_refs = inferred_source_envelope_refs_for(&routed);
    }
    writer.write_jsonl_values(
        "external_refs/source-envelopes.jsonl",
        &source_envelope_refs,
    )?;

    let capture_status_raw = request
        .status_json
        .clone()
        .unwrap_or_else(|| degraded_capability("capture.status not supplied"));
    let capture_status = window_bound_capability(capture_status_raw, &request, "capture_status");
    let permissions = permissions_capability_from_capture_status(&capture_status)
        .unwrap_or_else(|| degraded_capability("permissions proof not supplied"));
    let ux_status_raw = request
        .ux_status_json
        .clone()
        .or_else(|| ux_event_tap_capability_from_capture_status(&capture_status))
        .unwrap_or_else(|| degraded_capability("UX status not supplied"));
    let ux_status = window_bound_capability(ux_status_raw, &request, "ux_event_tap");
    let samplers_raw = request
        .sampler_json
        .clone()
        .or_else(|| samplers_capability_from_capture_status(&capture_status))
        .unwrap_or_else(|| degraded_capability("sampler status not supplied"));
    let samplers = window_bound_capability(samplers_raw, &request, "samplers");
    let browser_proof_raw = request
        .browser_proof_json
        .clone()
        .unwrap_or_else(|| degraded_capability("browser extension proof not supplied"));
    let browser_proof =
        window_bound_capability(browser_proof_raw, &request, "browser_extension_proof");
    let capabilities = [
        CapabilityDocument {
            source_id: "capture_status",
            path: "capabilities/capture.status.json",
            value: &capture_status,
        },
        CapabilityDocument {
            source_id: "permissions",
            path: "capabilities/permissions.json",
            value: &permissions,
        },
        CapabilityDocument {
            source_id: "ux_event_tap",
            path: "capabilities/ux-event-tap.json",
            value: &ux_status,
        },
        CapabilityDocument {
            source_id: "samplers",
            path: "capabilities/samplers.json",
            value: &samplers,
        },
        CapabilityDocument {
            source_id: "browser_extension_proof",
            path: "capabilities/browser-extension-proof.json",
            value: &browser_proof,
        },
    ];
    writer.write_json("capabilities/capture.status.json", &capture_status)?;
    writer.write_json("capabilities/permissions.json", &permissions)?;
    writer.write_json("capabilities/ux-event-tap.json", &ux_status)?;
    writer.write_json("capabilities/samplers.json", &samplers)?;
    writer.write_json("capabilities/browser-extension-proof.json", &browser_proof)?;
    writer.write_json(
        "time_alignment.json",
        &time_alignment_for(
            &request,
            &routed,
            &spool_report,
            source_envelope_refs.len(),
            &frame_2fps_records,
        ),
    )?;

    let sources = source_inventory_for(
        &routed,
        source_envelope_refs.len(),
        &capabilities,
        frame_2fps_count,
    );
    let mut known_gaps = known_gaps_for(&sources, &routed, frame_2fps_count);
    known_gaps.append(&mut media_export_gaps);
    known_gaps.extend(media_known_gaps_for(&media_index));
    known_gaps.extend(spool_known_gaps_for(&spool_report));
    known_gaps.extend(quality.known_gap_records());
    writer.write_jsonl_values("quality/known_gaps.jsonl", &known_gaps)?;
    writer.write_json("sources.json", &sources)?;

    let candidate_validation = validate_bundle_candidate(writer.partial_path())?;
    if !candidate_validation.ok {
        let bundle_path =
            finish_failed_export(writer, &mut manifest, candidate_validation.clone())?;
        let (byte_count, file_count) = compute_tree_totals(&bundle_path)?;
        return Ok(ExportResponse {
            capture_id,
            state: "failed".to_string(),
            bundle_path,
            expires_at: manifest.expires_at,
            byte_count,
            file_count,
            lane_count: mandatory_lane_ids().len(),
            known_gap_count: known_gaps.len(),
            validation: Some(candidate_validation),
        });
    }

    manifest.state = BundleState::Ready;
    manifest.ready_at = Some(Utc::now());
    manifest.expires_at = Some(Utc::now() + Duration::minutes(60));
    manifest.lane_count = mandatory_lane_ids().len() as u64;
    manifest.known_gap_count = known_gaps.len() as u64;
    let (bytes, files) = writer.compute_partial_totals()?;
    manifest.byte_count = bytes;
    manifest.file_count = files;
    writer.write_manifest(&manifest)?;
    writer.write_ready_sentinel()?;
    let validation = validate_ready_bundle(writer.partial_path())?;
    if !validation.ok {
        let bundle_path = finish_failed_export(writer, &mut manifest, validation.clone())?;
        let (byte_count, file_count) = compute_tree_totals(&bundle_path)?;
        return Ok(ExportResponse {
            capture_id,
            state: "failed".to_string(),
            bundle_path,
            expires_at: manifest.expires_at,
            byte_count,
            file_count,
            lane_count: mandatory_lane_ids().len(),
            known_gap_count: known_gaps.len(),
            validation: Some(validation),
        });
    }
    let bundle_path = writer.promote()?;
    let (byte_count, file_count) = compute_tree_totals(&bundle_path)?;

    Ok(ExportResponse {
        capture_id,
        state: "ready".to_string(),
        bundle_path,
        expires_at: manifest.expires_at,
        byte_count,
        file_count,
        lane_count: mandatory_lane_ids().len(),
        known_gap_count: known_gaps.len(),
        validation: Some(validation),
    })
}

fn finish_failed_export(
    writer: AtomicBundleWriter,
    manifest: &mut CaptureBundleManifest,
    validation: ValidationReport,
) -> CaptureCoreResult<PathBuf> {
    writer.remove_ready_sentinel()?;
    manifest.state = BundleState::Failed;
    manifest.ready_at = None;
    manifest.retention_class = RetentionClass::FailedAudit;
    manifest.expires_at = Some(Utc::now() + Duration::hours(72));
    writer.write_json(
        "quality/failure.json",
        &json!({
            "schema_version": 1,
            "failed_at": Utc::now(),
            "reason": "bundle validation failed before live promotion",
            "validation": validation,
        }),
    )?;
    writer.write_json("quality/validation-report.json", &validation)?;
    let (bytes, files) = writer.compute_partial_totals()?;
    manifest.byte_count = bytes;
    manifest.file_count = files;
    writer.write_manifest(manifest)?;
    writer.move_to_failed()
}

fn capture_id(time_start: DateTime<Utc>) -> String {
    let counter = CAPTURE_ID_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "cap_{}_{}_{}",
        time_start.format("%Y%m%dT%H%M%S%.3fZ"),
        std::process::id(),
        counter
    )
}

fn target_label(target: &CaptureTarget) -> String {
    match target {
        CaptureTarget::ActiveWindow => "active-window".to_string(),
        CaptureTarget::AllWindows => "all-windows".to_string(),
        CaptureTarget::Custom(value) => format!("custom:{value}"),
    }
}

fn degraded_capability(reason: &str) -> Value {
    json!({
        "schema_version": 1,
        "status": "degraded",
        "reason": reason
    })
}

struct CapabilityDocument<'a> {
    source_id: &'static str,
    path: &'static str,
    value: &'a Value,
}

impl CapabilityDocument<'_> {
    fn status(&self) -> SourceStatus {
        if self
            .value
            .get("ready")
            .and_then(Value::as_bool)
            .is_some_and(|ready| ready)
        {
            return SourceStatus::Present;
        }
        if self
            .value
            .get("ready")
            .and_then(Value::as_bool)
            .is_some_and(|ready| !ready)
        {
            return SourceStatus::Degraded;
        }

        match self.value.get("status").and_then(Value::as_str) {
            Some("ok" | "present" | "ready" | "available" | "granted") => SourceStatus::Present,
            Some("permission_denied" | "denied") => SourceStatus::PermissionDenied,
            Some("source_unavailable" | "unavailable") => SourceStatus::SourceUnavailable,
            Some("disabled_by_policy") => SourceStatus::DisabledByPolicy,
            Some(_) => SourceStatus::Degraded,
            None => SourceStatus::Present,
        }
    }

    fn is_degraded(&self) -> bool {
        self.status() != SourceStatus::Present
    }

    fn reason(&self) -> Option<String> {
        self.value
            .get("reason")
            .and_then(Value::as_str)
            .or_else(|| self.value.get("message").and_then(Value::as_str))
            .or_else(|| self.value.get("status").and_then(Value::as_str))
            .map(str::to_string)
    }
}

fn permissions_capability_from_capture_status(capture_status: &Value) -> Option<Value> {
    let metadata = capture_status
        .get("permission_derived_metadata")
        .or_else(|| {
            capture_status
                .get("capture_status")
                .and_then(|status| status.get("permission_derived_metadata"))
        })?
        .clone();
    let ready = permission_metadata_ready(&metadata);
    let mut capability = json!({
        "schema_version": 1,
        "status": if ready.unwrap_or(true) { "ok" } else { "degraded" },
        "source": "capture.status.permission_derived_metadata",
        "derived_from": "capabilities/capture.status.json",
        "permission_derived_metadata": metadata
    });
    if ready == Some(false) {
        capability["reason"] =
            json!("one or more permission-derived metadata signals are not ready");
    }
    Some(capability)
}

fn ux_event_tap_capability_from_capture_status(capture_status: &Value) -> Option<Value> {
    let status = nested_capture_status_value(capture_status, "ux_event_tap")?;
    let tap_active = status.get("tap_active").and_then(Value::as_bool);
    let lifecycle_running = status
        .get("lifecycle_state")
        .and_then(Value::as_str)
        .is_some_and(|state| state == "running");
    let ready = tap_active == Some(true) && lifecycle_running;
    let mut capability = json!({
        "schema_version": 1,
        "status": if ready { "ok" } else { "degraded" },
        "source": "capture.status.ux_event_tap",
        "derived_from": "capabilities/capture.status.json",
        "ux_event_tap": status
    });
    if !ready {
        capability["reason"] =
            json!("capture status reported the UX event tap as inactive or not running");
    }
    Some(capability)
}

fn samplers_capability_from_capture_status(capture_status: &Value) -> Option<Value> {
    let sampler = nested_capture_status_value(capture_status, "continuous_metadata_sampler")?;
    let enabled = sampler.get("enabled").and_then(Value::as_bool);
    let has_error = sampler
        .get("last_error")
        .is_some_and(|error| !error.is_null());
    let ready = enabled == Some(true) && !has_error;
    let mut capability = json!({
        "schema_version": 1,
        "status": if ready { "ok" } else { "degraded" },
        "source": "capture.status.continuous_metadata_sampler",
        "derived_from": "capabilities/capture.status.json",
        "continuous_metadata_sampler": sampler
    });
    if !ready {
        capability["reason"] =
            json!("capture status reported the metadata sampler as disabled or errored");
    }
    Some(capability)
}

fn nested_capture_status_value(capture_status: &Value, key: &str) -> Option<Value> {
    capture_status
        .get(key)
        .or_else(|| {
            capture_status
                .get("capture_status")
                .and_then(|status| status.get(key))
        })
        .cloned()
}

fn permission_metadata_ready(metadata: &Value) -> Option<bool> {
    let signals = metadata.get("signals")?.as_object()?;
    if signals.is_empty() {
        return None;
    }
    let mut saw_ready_field = false;
    for signal in signals.values() {
        if let Some(ready) = signal.get("ready").and_then(Value::as_bool) {
            saw_ready_field = true;
            if !ready {
                return Some(false);
            }
        }
    }
    saw_ready_field.then_some(true)
}

fn window_bound_capability(mut value: Value, request: &ExportRequest, source_id: &str) -> Value {
    let Some(object) = value.as_object_mut() else {
        return json!({
            "schema_version": 1,
            "status": "degraded",
            "reason": "capability document was not a JSON object",
            "source_id": source_id,
            "collected_at": request.time_start.to_rfc3339(),
            "applies_to_window": true,
            "capture_window": {
                "time_start": request.time_start.to_rfc3339(),
                "time_end": request.time_end.to_rfc3339()
            },
            "original": value
        });
    };
    object
        .entry("schema_version".to_string())
        .or_insert_with(|| json!(1));
    object.insert("source_id".to_string(), json!(source_id));
    object.insert(
        "collected_at".to_string(),
        json!(request.time_start.to_rfc3339()),
    );
    object.insert("applies_to_window".to_string(), json!(true));
    object.insert(
        "capture_window".to_string(),
        json!({
            "time_start": request.time_start.to_rfc3339(),
            "time_end": request.time_end.to_rfc3339()
        }),
    );
    value
}

#[derive(Default)]
struct RoutedSpool {
    windows: Vec<String>,
    displays: Vec<String>,
    capture: Vec<String>,
    ax: Vec<String>,
    ux: Vec<String>,
    sck: Vec<String>,
    browser: Vec<String>,
    terminal: Vec<String>,
    editor: Vec<String>,
}

impl RoutedSpool {
    fn from_records(records: &[RawSpoolRecord]) -> Self {
        let mut routed = Self::default();
        for record in records {
            let event_type = record.envelope.event_type.as_str();
            let raw_json = record.raw_json.clone();
            if event_type == "capture.window_snapshot" {
                routed.windows.push(raw_json.clone());
            }
            if event_type == "capture.display_snapshot" {
                routed.displays.push(raw_json.clone());
            }
            if event_type.starts_with("capture.ax") {
                routed.ax.push(raw_json.clone());
            }
            if event_type.starts_with("capture.ux") {
                routed.ux.push(raw_json.clone());
            }
            if event_type == "capture.active_window_frame_metadata" {
                routed.sck.push(raw_json.clone());
            }
            if event_type.starts_with("capture.browser") {
                routed.browser.push(raw_json.clone());
            }
            if event_type.starts_with("capture.terminal") {
                routed.terminal.push(raw_json.clone());
            }
            if event_type.starts_with("capture.editor") {
                routed.editor.push(raw_json.clone());
            }
            if !event_type.is_empty() {
                routed.capture.push(raw_json);
            }
        }
        routed
    }

    fn add_bracketing_window_snapshots(
        &mut self,
        capture_root: &Path,
        time_start: DateTime<Utc>,
        time_end: DateTime<Utc>,
        quality: &mut ExportQualityMetadata,
    ) -> CaptureCoreResult<()> {
        if !self.windows.is_empty() {
            return Ok(());
        }

        let lookup = select_bracketing_window_snapshots(capture_root, time_end)?;
        quality.window_snapshot_lookup = lookup.metrics.to_json();

        let mut selected = Vec::new();
        if let Some(candidate) = lookup.latest_at_or_before_end.as_ref() {
            selected.push(("latest_at_or_before_time_end", candidate));
        }
        if let Some(candidate) = lookup.nearest_after_end.as_ref() {
            selected.push(("nearest_after_time_end", candidate));
        }

        let mut seen = BTreeSet::new();
        for (reason, candidate) in selected {
            let delta_from_window_ms = if candidate.time < time_start {
                (time_start - candidate.time).num_milliseconds()
            } else if candidate.time > time_end {
                (candidate.time - time_end).num_milliseconds()
            } else {
                0
            };
            if delta_from_window_ms
                > Duration::seconds(MAX_BRACKETING_WINDOW_SNAPSHOT_AGE_SECONDS).num_milliseconds()
            {
                quality.window_snapshot_rejections.push(json!({
                    "selection": reason,
                    "reason": "bracketing_snapshot_too_stale",
                    "snapshot_time": candidate.time.to_rfc3339(),
                    "max_age_ms": Duration::seconds(MAX_BRACKETING_WINDOW_SNAPSHOT_AGE_SECONDS).num_milliseconds(),
                    "delta_ms_from_capture_window": delta_from_window_ms,
                    "capture_window": {
                        "time_start": time_start.to_rfc3339(),
                        "time_end": time_end.to_rfc3339()
                    }
                }));
                continue;
            }
            if seen.insert(candidate.raw.clone()) {
                self.windows.push(candidate.raw.clone());
                self.capture.push(candidate.raw.clone());
                quality.window_snapshot_selections.push(json!({
                    "selection": reason,
                    "snapshot_time": candidate.time.to_rfc3339(),
                    "capture_window": {
                        "time_start": time_start.to_rfc3339(),
                        "time_end": time_end.to_rfc3339()
                    },
                    "delta_ms_from_time_start": (candidate.time - time_start).num_milliseconds(),
                    "delta_ms_from_time_end": (candidate.time - time_end).num_milliseconds()
                }));
            }
        }

        Ok(())
    }
}

#[derive(Default)]
struct ExportQualityMetadata {
    window_snapshot_lookup: Value,
    window_snapshot_selections: Vec<Value>,
    window_snapshot_rejections: Vec<Value>,
}

impl ExportQualityMetadata {
    fn known_gap_records(&self) -> Vec<KnownGapRecord> {
        let mut records = Vec::new();
        for selection in &self.window_snapshot_selections {
            records.push(KnownGapRecord {
                schema_version: 1,
                time: Utc::now(),
                source_id: "windows".to_string(),
                severity: "info".to_string(),
                code: "window_snapshot_bracketed".to_string(),
                message: "no exact in-range window snapshot was available; exporter included a bracketing snapshot".to_string(),
                blocks_ready: false,
                extra: gap_extra([("selection", selection.clone())]),
            });
        }
        for rejection in &self.window_snapshot_rejections {
            records.push(KnownGapRecord {
                schema_version: 1,
                time: Utc::now(),
                source_id: "windows".to_string(),
                severity: "warning".to_string(),
                code: "window_snapshot_bracket_too_stale".to_string(),
                message: "nearest window snapshot was too far from the capture window to use as bundle evidence".to_string(),
                blocks_ready: false,
                extra: gap_extra([("rejection", rejection.clone())]),
            });
        }
        records
    }
}

struct WindowSnapshotCandidate {
    time: DateTime<Utc>,
    raw: String,
}

#[derive(Default)]
struct WindowSnapshotLookup {
    latest_at_or_before_end: Option<WindowSnapshotCandidate>,
    nearest_after_end: Option<WindowSnapshotCandidate>,
    metrics: WindowSnapshotLookupMetrics,
}

#[derive(Default)]
struct WindowSnapshotLookupMetrics {
    schema_version: u32,
    files_scanned: u64,
    lines_scanned: u64,
    minimal_envelope_parse_count: u64,
    full_payload_parse_count: u64,
    window_snapshot_records_seen: u64,
    selected_record_count: u64,
    malformed_line_count: u64,
}

impl WindowSnapshotLookupMetrics {
    fn to_json(&self) -> Value {
        json!({
            "schema_version": if self.schema_version == 0 { 1 } else { self.schema_version },
            "files_scanned": self.files_scanned,
            "lines_scanned": self.lines_scanned,
            "minimal_envelope_parse_count": self.minimal_envelope_parse_count,
            "full_payload_parse_count": self.full_payload_parse_count,
            "window_snapshot_records_seen": self.window_snapshot_records_seen,
            "selected_record_count": self.selected_record_count,
            "malformed_line_count": self.malformed_line_count,
            "parser": "raw_window_snapshot_envelope",
        })
    }
}

struct WindowSnapshotFile {
    path: PathBuf,
    date: Option<NaiveDate>,
}

fn select_bracketing_window_snapshots(
    capture_root: &Path,
    time_end: DateTime<Utc>,
) -> CaptureCoreResult<WindowSnapshotLookup> {
    let windows_dir = CaptureRootPaths::new(capture_root).windows_dir;
    let Ok(entries) = fs::read_dir(&windows_dir) else {
        return Ok(WindowSnapshotLookup {
            metrics: WindowSnapshotLookupMetrics {
                schema_version: 1,
                ..WindowSnapshotLookupMetrics::default()
            },
            ..WindowSnapshotLookup::default()
        });
    };
    let mut files = Vec::new();
    for entry in entries {
        let entry =
            entry.map_err(|error| CaptureCoreError::io(Some(windows_dir.clone()), error))?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".windows.jsonl"))
        {
            files.push(WindowSnapshotFile {
                date: window_snapshot_file_date(&path),
                path,
            });
        }
    }
    files.sort_by(|left, right| {
        left.date
            .cmp(&right.date)
            .then_with(|| left.path.cmp(&right.path))
    });

    let mut lookup = WindowSnapshotLookup {
        metrics: WindowSnapshotLookupMetrics {
            schema_version: 1,
            ..WindowSnapshotLookupMetrics::default()
        },
        ..WindowSnapshotLookup::default()
    };

    let end_date = time_end.date_naive();
    for file in files
        .iter()
        .rev()
        .filter(|file| file.date.is_some_and(|date| date <= end_date))
    {
        let bracket =
            bracketing_window_snapshots_from_tail(&file.path, time_end, &mut lookup.metrics)?;
        if let Some(candidate) = bracket.nearest_after_end {
            update_nearest_window_snapshot_after(&mut lookup, candidate);
        }
        if let Some(candidate) = bracket.latest_at_or_before_end {
            update_latest_window_snapshot(&mut lookup, candidate);
            break;
        }
    }

    if lookup.nearest_after_end.is_none() {
        for file in files
            .iter()
            .filter(|file| file.date.is_some_and(|date| date > end_date))
        {
            if let Some(candidate) =
                nearest_window_snapshot_after_from_head(&file.path, time_end, &mut lookup.metrics)?
            {
                update_nearest_window_snapshot_after(&mut lookup, candidate);
                break;
            }
        }
    }
    lookup.metrics.selected_record_count = lookup.latest_at_or_before_end.iter().count() as u64
        + lookup.nearest_after_end.iter().count() as u64;
    Ok(lookup)
}

fn window_snapshot_file_date(path: &Path) -> Option<NaiveDate> {
    let stem = path
        .file_name()
        .and_then(|name| name.to_str())?
        .strip_suffix(".windows.jsonl")?;
    if stem.len() != "YYYY-MM-DD".len() {
        return None;
    }
    NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

fn bracketing_window_snapshots_from_tail(
    path: &Path,
    time_end: DateTime<Utc>,
    metrics: &mut WindowSnapshotLookupMetrics,
) -> CaptureCoreResult<WindowSnapshotLookup> {
    metrics.files_scanned += 1;
    let mut nearest_after_end = None;
    let latest_at_or_before_end = scan_lines_from_tail(path, |line| {
        let candidate = window_snapshot_candidate_from_line(line, metrics)?;
        if candidate.time <= time_end {
            Some(candidate)
        } else {
            nearest_after_end = Some(candidate);
            None
        }
    })?;

    Ok(WindowSnapshotLookup {
        latest_at_or_before_end,
        nearest_after_end,
        metrics: WindowSnapshotLookupMetrics::default(),
    })
}

fn nearest_window_snapshot_after_from_head(
    path: &Path,
    time_end: DateTime<Utc>,
    metrics: &mut WindowSnapshotLookupMetrics,
) -> CaptureCoreResult<Option<WindowSnapshotCandidate>> {
    metrics.files_scanned += 1;
    let handle =
        File::open(path).map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
    let mut reader = BufReader::new(handle);
    let mut line = Vec::new();
    loop {
        line.clear();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
        if bytes == 0 {
            return Ok(None);
        }
        if let Some(candidate) =
            window_snapshot_candidate_from_line(trim_jsonl_newline(&line), metrics)
        {
            if candidate.time > time_end {
                return Ok(Some(candidate));
            }
        }
    }
}

fn scan_lines_from_tail<F>(
    path: &Path,
    mut visit: F,
) -> CaptureCoreResult<Option<WindowSnapshotCandidate>>
where
    F: FnMut(&[u8]) -> Option<WindowSnapshotCandidate>,
{
    const BLOCK_SIZE: u64 = 64 * 1024;

    let mut file =
        File::open(path).map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
    let mut position = file
        .seek(SeekFrom::End(0))
        .map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
    let mut remainder = Vec::new();
    while position > 0 {
        let read_len = position.min(BLOCK_SIZE);
        position -= read_len;
        file.seek(SeekFrom::Start(position))
            .map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
        let mut chunk = vec![0_u8; read_len as usize];
        file.read_exact(&mut chunk)
            .map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
        chunk.extend_from_slice(&remainder);

        let mut end = chunk.len();
        while let Some(newline) = chunk[..end].iter().rposition(|byte| *byte == b'\n') {
            let line = trim_jsonl_newline(&chunk[newline + 1..end]);
            if !line.is_empty() {
                if let Some(candidate) = visit(line) {
                    return Ok(Some(candidate));
                }
            }
            end = newline;
        }
        remainder.clear();
        remainder.extend_from_slice(&chunk[..end]);
    }

    let line = trim_jsonl_newline(&remainder);
    if !line.is_empty() {
        if let Some(candidate) = visit(line) {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}

fn window_snapshot_candidate_from_line(
    line: &[u8],
    metrics: &mut WindowSnapshotLookupMetrics,
) -> Option<WindowSnapshotCandidate> {
    if line.is_empty() {
        return None;
    }
    metrics.lines_scanned += 1;
    metrics.minimal_envelope_parse_count += 1;
    let raw = match std::str::from_utf8(line) {
        Ok(raw) => raw,
        Err(_) => {
            metrics.malformed_line_count += 1;
            return None;
        }
    };
    let Some(time) = quick_window_snapshot_time(raw) else {
        return None;
    };
    metrics.window_snapshot_records_seen += 1;
    Some(WindowSnapshotCandidate {
        time,
        raw: raw.to_string(),
    })
}

fn quick_window_snapshot_time(raw: &str) -> Option<DateTime<Utc>> {
    let event_type =
        json_string_field(raw, "eventType").or_else(|| json_string_field(raw, "event_type"))?;
    if event_type != "capture.window_snapshot" {
        return None;
    }
    [
        "eventTimeStart",
        "recordedAt",
        "generatedAt",
        "capturedAt",
        "started_at",
        "event_time_start",
        "recorded_at",
        "generated_at",
        "captured_at",
    ]
    .iter()
    .find_map(|key| parse_rfc3339(json_string_field(raw, key)))
}

fn json_string_field<'a>(raw: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("\"{key}\"");
    let bytes = raw.as_bytes();
    let mut search_from = 0;
    while let Some(relative) = raw[search_from..].find(&needle) {
        let key_start = search_from + relative;
        let mut cursor = key_start + needle.len();
        cursor = skip_json_whitespace(bytes, cursor);
        if bytes.get(cursor) != Some(&b':') {
            search_from = key_start + 1;
            continue;
        }
        cursor += 1;
        cursor = skip_json_whitespace(bytes, cursor);
        if bytes.get(cursor) != Some(&b'"') {
            search_from = key_start + 1;
            continue;
        }
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() {
            match bytes[cursor] {
                b'\\' => return None,
                b'"' => return Some(&raw[value_start..cursor]),
                _ => cursor += 1,
            }
        }
        return None;
    }
    None
}

fn skip_json_whitespace(bytes: &[u8], mut cursor: usize) -> usize {
    while bytes
        .get(cursor)
        .is_some_and(|byte| matches!(byte, b' ' | b'\n' | b'\r' | b'\t'))
    {
        cursor += 1;
    }
    cursor
}

fn update_latest_window_snapshot(
    lookup: &mut WindowSnapshotLookup,
    candidate: WindowSnapshotCandidate,
) {
    if lookup
        .latest_at_or_before_end
        .as_ref()
        .is_none_or(|current| candidate.time > current.time)
    {
        lookup.latest_at_or_before_end = Some(candidate);
    }
}

fn update_nearest_window_snapshot_after(
    lookup: &mut WindowSnapshotLookup,
    candidate: WindowSnapshotCandidate,
) {
    if lookup
        .nearest_after_end
        .as_ref()
        .is_none_or(|current| candidate.time < current.time)
    {
        lookup.nearest_after_end = Some(candidate);
    }
}

fn trim_jsonl_newline(line: &[u8]) -> &[u8] {
    let without_lf = line.strip_suffix(b"\n").unwrap_or(line);
    without_lf.strip_suffix(b"\r").unwrap_or(without_lf)
}

fn event_time(value: &Value) -> Option<DateTime<Utc>> {
    [
        "eventTimeStart",
        "recordedAt",
        "generatedAt",
        "capturedAt",
        "started_at",
    ]
    .iter()
    .find_map(|key| parse_rfc3339(value.get(*key).and_then(Value::as_str)))
    .or_else(|| {
        value.get("payload").and_then(|payload| {
            ["generatedAt", "capturedAt", "started_at"]
                .iter()
                .find_map(|key| parse_rfc3339(payload.get(*key).and_then(Value::as_str)))
        })
    })
}

fn parse_rfc3339(value: Option<&str>) -> Option<DateTime<Utc>> {
    value
        .and_then(|value| DateTime::parse_from_rfc3339(value).ok())
        .map(|time| time.with_timezone(&Utc))
}

fn record_ref(value: &Value, source_file: &str) -> Value {
    json!({
        "source_file": source_file,
        "event_type": value.get("eventType").and_then(Value::as_str),
        "recorded_at": value.get("recordedAt").and_then(Value::as_str),
        "source_record_id": value.get("sourceRecordID").and_then(Value::as_str)
    })
}

fn source_inventory_for(
    routed: &RoutedSpool,
    source_envelope_count: usize,
    capabilities: &[CapabilityDocument<'_>],
    frame_2fps_count: usize,
) -> SourceInventory {
    let counts = [
        ("windows", "capture.windows", routed.windows.len() as u64),
        ("displays", "capture.displays", routed.displays.len() as u64),
        ("capture", "capture.events", routed.capture.len() as u64),
        ("ax", "capture.ax", routed.ax.len() as u64),
        ("ux", "capture.ux", routed.ux.len() as u64),
        (
            "sck_frame_metadata",
            "capture.active_window_frames",
            routed.sck.len() as u64 + frame_2fps_count as u64,
        ),
        ("browser", "capture.browser", routed.browser.len() as u64),
        ("terminal", "capture.terminal", routed.terminal.len() as u64),
        ("editor", "capture.editor", routed.editor.len() as u64),
        (
            "external_refs",
            "capture.external_refs",
            source_envelope_count as u64,
        ),
    ];
    let mut sources: Vec<_> = counts
        .into_iter()
        .map(|(source_id, lane_id, count)| {
            let mut extra = BTreeMap::new();
            let stats = lane_record_stats(routed.records_for_lane_id(lane_id));
            if stats.inferred_count > 0 {
                extra.insert("direct_record_count".to_string(), json!(stats.direct_count));
                extra.insert(
                    "inferred_record_count".to_string(),
                    json!(stats.inferred_count),
                );
                extra.insert(
                    "evidence_quality".to_string(),
                    json!(if stats.direct_count == 0 {
                        "inferred_only"
                    } else {
                        "direct_plus_inferred"
                    }),
                );
            }
            if lane_id == "capture.active_window_frames" {
                extra.insert("frame_2fps_count".to_string(), json!(frame_2fps_count));
                extra.insert(
                    "sck_metadata_record_count".to_string(),
                    json!(routed.sck.len()),
                );
            }
            LaneSource {
                source_id: source_id.to_string(),
                lane_id: lane_id.to_string(),
                status: if count == 0 {
                    SourceStatus::Degraded
                } else {
                    SourceStatus::Present
                },
                required_for_v0: true,
                record_count: count,
                degraded_reason: if count == 0 {
                    Some("mandatory V0 lane is empty in selected spool window".to_string())
                } else {
                    None
                },
                confidence: stats.max_confidence,
                extra,
            }
        })
        .collect();
    sources.extend(capabilities.iter().map(|capability| LaneSource {
        source_id: capability.source_id.to_string(),
        lane_id: capability.path.to_string(),
        status: capability.status(),
        required_for_v0: true,
        record_count: if capability.status() == SourceStatus::Present {
            1
        } else {
            0
        },
        degraded_reason: capability.is_degraded().then(|| {
            capability
                .reason()
                .unwrap_or_else(|| "capability proof is degraded or not present".to_string())
        }),
        confidence: None,
        extra: Default::default(),
    }));
    SourceInventory {
        schema_version: 1,
        sources,
    }
}

impl RoutedSpool {
    fn records_for_lane_id(&self, lane_id: &str) -> &[String] {
        match lane_id {
            "capture.windows" => &self.windows,
            "capture.displays" => &self.displays,
            "capture.events" => &self.capture,
            "capture.ax" => &self.ax,
            "capture.ux" => &self.ux,
            "capture.active_window_frames" => &self.sck,
            "capture.browser" => &self.browser,
            "capture.terminal" => &self.terminal,
            "capture.editor" => &self.editor,
            _ => &[],
        }
    }
}

#[derive(Default)]
struct LaneRecordStats {
    direct_count: u64,
    inferred_count: u64,
    max_confidence: Option<f32>,
}

fn lane_record_stats(records: &[String]) -> LaneRecordStats {
    let mut stats = LaneRecordStats::default();
    for raw in records {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            stats.direct_count += 1;
            continue;
        };
        let is_inferred = value
            .get("sourceRecordID")
            .and_then(Value::as_str)
            .is_some_and(|id| id.starts_with("inferred:") || id.starts_with("derived:"))
            || value
                .get("eventType")
                .and_then(Value::as_str)
                .is_some_and(|event_type| event_type.ends_with(".inferred"));
        if is_inferred {
            stats.inferred_count += 1;
        } else {
            stats.direct_count += 1;
        }
        if let Some(confidence) = value
            .pointer("/payload/confidence")
            .and_then(Value::as_f64)
            .or_else(|| {
                value
                    .pointer("/payload/derived/confidence")
                    .and_then(Value::as_f64)
            })
        {
            let confidence = confidence as f32;
            stats.max_confidence = Some(
                stats
                    .max_confidence
                    .map_or(confidence, |current| current.max(confidence)),
            );
        }
    }
    stats
}

fn known_gaps_for(
    sources: &SourceInventory,
    routed: &RoutedSpool,
    frame_2fps_count: usize,
) -> Vec<KnownGapRecord> {
    let mut gaps = Vec::new();
    for source in &sources.sources {
        let inferred_only = source
            .extra
            .get("evidence_quality")
            .is_some_and(|quality| quality == "inferred_only")
            && source.record_count > 0;
        if source.status == SourceStatus::Present && inferred_only {
            let mut extra = gap_extra([("record_count", json!(source.record_count))]);
            extra.insert("lane_id".to_string(), json!(source.lane_id));
            gaps.push(KnownGapRecord {
                schema_version: 1,
                time: Utc::now(),
                source_id: source.source_id.clone(),
                severity: "warning".to_string(),
                code: "mandatory_lane_inferred_only".to_string(),
                message: "mandatory V0 lane has records, but they are inferred from app/window evidence rather than direct connector events".to_string(),
                blocks_ready: false,
                extra,
            });
        }
        if source.status != SourceStatus::Present {
            let is_capability = source.lane_id.starts_with("capabilities/");
            let code = if is_capability {
                "capability_proof_degraded"
            } else if inferred_only {
                "mandatory_lane_inferred_only"
            } else {
                "mandatory_lane_empty"
            };
            let mut extra = gap_extra([("record_count", json!(source.record_count))]);
            extra.insert("lane_id".to_string(), json!(source.lane_id));
            if is_capability {
                extra.insert("path".to_string(), json!(source.lane_id));
            }
            gaps.push(KnownGapRecord {
                schema_version: 1,
                time: Utc::now(),
                source_id: source.source_id.clone(),
                severity: "warning".to_string(),
                code: code.to_string(),
                message: source.degraded_reason.clone().unwrap_or_else(|| {
                    if is_capability {
                        "capability proof is degraded or not present".to_string()
                    } else {
                        "mandatory V0 lane is empty in selected spool window".to_string()
                    }
                }),
                blocks_ready: false,
                extra,
            });
        }
    }
    if routed
        .sck
        .iter()
        .any(|raw| sck_indicates_media_candidate(raw))
        && frame_2fps_count == 0
    {
        gaps.push(KnownGapRecord {
            schema_version: 1,
            time: Utc::now(),
            source_id: "media".to_string(),
            severity: "warning".to_string(),
            code: "media_candidate_omitted".to_string(),
            message:
                "SCK metadata indicated media/keyframe candidates, but media artifacts were omitted by this exporter"
                    .to_string(),
            blocks_ready: false,
            extra: gap_extra([("record_count", json!(0))]),
        });
    }
    gaps
}

fn write_frame_2fps_artifacts(
    writer: &AtomicBundleWriter,
    request: &ExportRequest,
    capture_id: &str,
) -> CaptureCoreResult<(Vec<Value>, Vec<KnownGapRecord>)> {
    let mut records = Vec::new();
    let mut gaps = Vec::new();

    let frame_paths = match collect_frame_2fps_paths(capture_id, request, &mut gaps)? {
        Some(frame_paths) => frame_paths,
        None => return Ok((records, gaps)),
    };
    let frame_manifest =
        load_frame_2fps_manifest(frame_paths.first().and_then(|path| path.parent()))?;
    if let Some(manifest) = &frame_manifest {
        writer.copy_file_from(
            "media/frames-2fps/frames-2fps-manifest.jsonl",
            &manifest.path,
        )?;
    }
    for (index, source_path) in frame_paths.iter().enumerate() {
        let extension = extension_lower(source_path).unwrap_or_else(|| "jpg".to_string());
        let relative_path = format!("media/frames-2fps/frame-{:06}.{extension}", index + 1);
        let byte_count = writer.copy_file_from(&relative_path, source_path)?;
        let manifest_record = frame_manifest
            .as_ref()
            .and_then(|manifest| manifest.records_by_file.get(&source_file_name(source_path)));
        let synthetic_recorded_at = request.time_start + Duration::milliseconds(index as i64 * 500);
        let recorded_at = manifest_record
            .and_then(|record| parse_rfc3339(record.get("recorded_at").and_then(Value::as_str)))
            .unwrap_or(synthetic_recorded_at);
        let mut record = json!({
            "schema_version": 1,
            "media_id": format!("{capture_id}:frame_2fps:{:06}", index + 1),
            "capture_id": capture_id,
            "kind": "frame_2fps",
            "source": "capture.screen_recording_frame_decoder",
            "path": relative_path.clone(),
            "uri": relative_path,
            "status": "available",
            "state": "available",
            "storage_backend": "bundle_file",
            "content_type": content_type_for(source_path),
            "byte_count": byte_count,
            "frame_index": index + 1,
            "sample_rate_fps": 2,
            "recorded_at": recorded_at.to_rfc3339(),
            "time_range": {
                "start": recorded_at.to_rfc3339(),
                "end": recorded_at.to_rfc3339()
            },
            "timing_source": if manifest_record.is_some() {
                "screen_recording_decoder_manifest"
            } else {
                "synthetic_export_window_sample_rate"
            },
            "debug": false,
            "original_file_name": source_file_name(source_path),
            "privacy_class": "visual_evidence",
            "retention": if request.debug_pin { "pinned_debug" } else { "ephemeral" }
        });
        if let Some(manifest_record) = manifest_record {
            enrich_frame_2fps_record_from_manifest(&mut record, manifest_record);
        }
        records.push(record);
    }
    Ok((records, gaps))
}

struct Frame2fpsManifest {
    path: PathBuf,
    records_by_file: BTreeMap<String, Value>,
}

fn load_frame_2fps_manifest(
    frame_dir: Option<&Path>,
) -> CaptureCoreResult<Option<Frame2fpsManifest>> {
    let Some(frame_dir) = frame_dir else {
        return Ok(None);
    };
    let path = [
        "frames-2fps-manifest.jsonl",
        "frame-manifest.jsonl",
        "manifest.jsonl",
    ]
    .iter()
    .map(|name| frame_dir.join(name))
    .find(|path| path.is_file());
    let Some(path) = path else {
        return Ok(None);
    };

    let file =
        File::open(&path).map_err(|error| CaptureCoreError::io(Some(path.clone()), error))?;
    let reader = BufReader::new(file);
    let mut records_by_file = BTreeMap::new();
    for (line_index, line) in reader.lines().enumerate() {
        let line = line.map_err(|error| CaptureCoreError::io(Some(path.clone()), error))?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let value: Value = serde_json::from_str(trimmed).map_err(|error| {
            CaptureCoreError::InvalidState(format!(
                "invalid 2fps frame manifest JSON at {}:{}: {error}",
                path.display(),
                line_index + 1
            ))
        })?;
        let Some(file_name) = frame_manifest_file_name(&value) else {
            return Err(CaptureCoreError::InvalidState(format!(
                "2fps frame manifest record at {}:{} lacks file_name or path",
                path.display(),
                line_index + 1
            )));
        };
        records_by_file.insert(file_name, value);
    }

    Ok(Some(Frame2fpsManifest {
        path,
        records_by_file,
    }))
}

fn frame_manifest_file_name(value: &Value) -> Option<String> {
    if let Some(file_name) = value.get("file_name").and_then(Value::as_str) {
        return Some(file_name.to_string());
    }
    value
        .get("path")
        .and_then(Value::as_str)
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .map(str::to_string)
}

fn enrich_frame_2fps_record_from_manifest(record: &mut Value, manifest_record: &Value) {
    let Some(object) = record.as_object_mut() else {
        return;
    };
    for key in [
        "requested_at",
        "requested_media_time_seconds",
        "actual_media_time_seconds",
        "requested_to_actual_delta_milliseconds",
        "requested_time_tolerance_before_seconds",
        "requested_time_tolerance_after_seconds",
        "media_zero_time",
        "decoder_source",
        "decoder_implementation",
    ] {
        if let Some(value) = manifest_record.get(key).cloned() {
            object.insert(key.to_string(), value);
        }
    }
    object.insert(
        "frame_timing_manifest_ref".to_string(),
        json!("media/frames-2fps/frames-2fps-manifest.jsonl"),
    );
    object.insert(
        "timing_precision".to_string(),
        json!("actual_decoded_media_time"),
    );
}

fn collect_frame_2fps_paths(
    capture_id: &str,
    request: &ExportRequest,
    gaps: &mut Vec<KnownGapRecord>,
) -> CaptureCoreResult<Option<Vec<PathBuf>>> {
    let frame_dir = request
        .frames_2fps_dir
        .clone()
        .unwrap_or_else(|| request.capture_root.join("media").join("frames-2fps"));
    if !frame_dir.is_dir() {
        gaps.push(missing_frame_2fps_gap(
            capture_id,
            &frame_dir,
            "frame_2fps_dir_missing",
            "2fps frame directory is missing; READY bundles require frames extracted from the screen recording",
        ));
        return Ok(None);
    }
    let mut paths = Vec::new();
    for entry in fs::read_dir(&frame_dir)
        .map_err(|error| CaptureCoreError::io(Some(frame_dir.clone()), error))?
    {
        let entry = entry.map_err(|error| CaptureCoreError::io(Some(frame_dir.clone()), error))?;
        let path = entry.path();
        if path.is_file() && is_frame_2fps_file(&path) {
            paths.push(path);
        }
    }
    paths.sort_by(|left, right| source_file_name(left).cmp(&source_file_name(right)));
    if paths.is_empty() {
        gaps.push(missing_frame_2fps_gap(
            capture_id,
            &frame_dir,
            "frame_2fps_dir_empty",
            "2fps frame directory did not contain jpg, png, or webp frames; READY bundles require screen-recording-derived screenshots",
        ));
        return Ok(None);
    }
    Ok(Some(paths))
}

fn missing_frame_2fps_gap(
    capture_id: &str,
    frame_dir: &Path,
    code: &str,
    message: &str,
) -> KnownGapRecord {
    KnownGapRecord {
        schema_version: 1,
        time: Utc::now(),
        source_id: "media".to_string(),
        severity: "error".to_string(),
        code: code.to_string(),
        message: message.to_string(),
        blocks_ready: true,
        extra: gap_extra([
            ("capture_id", json!(capture_id)),
            ("expected_path", json!(frame_dir.display().to_string())),
            ("required_media_kind", json!("frame_2fps")),
            ("sample_rate_fps", json!(2)),
        ]),
    }
}

fn is_frame_2fps_file(path: &Path) -> bool {
    matches!(
        extension_lower(path).as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp")
    )
}

fn content_type_for(path: &Path) -> &'static str {
    match extension_lower(path).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("mov") => "video/quicktime",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        _ => "application/octet-stream",
    }
}

fn extension_lower(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension.to_ascii_lowercase())
}

fn source_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("unknown")
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '_' | '-') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn media_index_for(sck_records: &[String], capture_id: &str) -> Vec<Value> {
    let mut media = Vec::new();
    for (index, raw) in sck_records.iter().enumerate() {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        let payload = value.get("payload").unwrap_or(&value);
        let media_ref = payload
            .get("mediaRef")
            .or_else(|| payload.get("media_ref"))
            .and_then(Value::as_str);
        let content_hash = payload
            .get("contentHash")
            .or_else(|| payload.get("content_hash"))
            .and_then(Value::as_str);
        let frame_id = payload
            .get("frameID")
            .or_else(|| payload.get("frame_id"))
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| format!("{capture_id}:sck:{index}"));
        if media_ref.is_none()
            && content_hash.is_none()
            && !payload_indicates_media_candidate(payload)
        {
            continue;
        }
        media.push(json!({
            "schema_version": 1,
            "media_id": frame_id,
            "capture_id": capture_id,
            "source": "capture.active_window_frame_metadata",
            "recorded_at": value.get("recordedAt").and_then(Value::as_str),
            "media_ref": media_ref,
            "content_hash": content_hash,
            "dimensions": payload.get("dimensions"),
            "status": if media_ref.is_some() || content_hash.is_some() {
                "referenced"
            } else {
                "candidate_omitted"
            },
            "inferred_from": [record_ref(&value, "events/sck-frame-metadata.events.jsonl")]
        }));
    }
    media
}

fn media_known_gaps_for(media_index: &[Value]) -> Vec<KnownGapRecord> {
    media_index
        .iter()
        .filter(|record| {
            record
                .get("status")
                .and_then(Value::as_str)
                .is_some_and(|status| status == "candidate_omitted")
        })
        .map(|record| KnownGapRecord {
            schema_version: 1,
            time: Utc::now(),
            source_id: "media".to_string(),
            severity: "info".to_string(),
            code: "media_candidate_omitted".to_string(),
            message: "SCK metadata indicated a media candidate, but no media artifact reference was available".to_string(),
            blocks_ready: false,
            extra: gap_extra([(
                "media_id",
                record.get("media_id").cloned().unwrap_or(Value::Null),
            )]),
        })
        .collect()
}

fn sck_indicates_media_candidate(raw: &String) -> bool {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return false;
    };
    let payload = value.get("payload").unwrap_or(&value);
    payload_indicates_media_candidate(payload)
}

fn payload_indicates_media_candidate(payload: &Value) -> bool {
    payload
        .get("attachmentsPresent")
        .or_else(|| payload.get("attachments_present"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || payload
            .get("adaptiveDecision")
            .or_else(|| payload.get("adaptive_decision"))
            .is_some_and(|decision| {
                decision
                    .get("shouldStoreKeyframe")
                    .or_else(|| decision.get("should_store_keyframe"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
            })
        || payload.get("mediaRef").is_some()
        || payload.get("media_ref").is_some()
        || payload.get("contentHash").is_some()
        || payload.get("content_hash").is_some()
}

fn spool_read_quality_report(report: &SpoolReadReport) -> Value {
    json!({
        "schema_version": 1,
        "tolerant_read": report.tolerant_read,
        "record_count": report.records.len(),
        "file_count": report.files.len(),
        "malformed_line_count": report.malformed_lines.len(),
        "files": report.files,
        "malformed_lines": report.malformed_lines
    })
}

fn raw_provenance_for(records: &[RawSpoolRecord], capture_id: &str) -> Vec<Value> {
    records
        .iter()
        .map(|record| {
            let provenance = record.provenance();
            json!({
                "schema_version": 1,
                "capture_id": capture_id,
                "event_type": record.envelope.event_type,
                "recorded_at": record.envelope.recorded_at.map(|time| time.to_rfc3339()),
                "event_time_start": record.envelope.event_time_start.map(|time| time.to_rfc3339()),
                "source_record_id": record.envelope.source_record_id,
                "raw_source_uri": provenance.raw_source_uri,
                "raw_line_number": provenance.raw_line_number,
                "raw_byte_offset": provenance.raw_byte_offset,
                "raw_record_hash": provenance.raw_record_hash
            })
        })
        .collect()
}

fn spool_known_gaps_for(report: &SpoolReadReport) -> Vec<KnownGapRecord> {
    report
        .malformed_lines
        .iter()
        .map(|line| KnownGapRecord {
            schema_version: 1,
            time: Utc::now(),
            source_id: "spool".to_string(),
            severity: "warning".to_string(),
            code: "malformed_spool_line_skipped".to_string(),
            message: "malformed JSONL spool record was skipped during tolerant bundle export"
                .to_string(),
            blocks_ready: false,
            extra: gap_extra([
                ("source_path", json!(line.source_path)),
                ("line_number", json!(line.line_number)),
                ("raw_record_hash", json!(line.raw_record_hash)),
                ("parser_error", json!(line.parser_error)),
            ]),
        })
        .collect()
}

fn source_envelope_refs_for(paths: &[PathBuf]) -> Vec<SourceEnvelopeRefRecord> {
    paths
        .iter()
        .map(|path| {
            let path_text = path.to_string_lossy().replace('\\', "/");
            let is_bundle_relative =
                !path.is_absolute() && BundleRelativePath::parse(&path_text).is_ok();
            let metadata = if is_bundle_relative {
                ExternalPathMetadata::default()
            } else {
                metadata_for_external_path(path)
            };
            SourceEnvelopeRefRecord {
                schema_version: 1,
                kind: if is_bundle_relative {
                    SourceEnvelopeRefKind::BundleRelative
                } else {
                    SourceEnvelopeRefKind::ExternalPathMetadata
                },
                path: path_text,
                absolute: path.is_absolute(),
                exists: metadata.exists,
                file_name: path
                    .file_name()
                    .and_then(|file_name| file_name.to_str())
                    .map(str::to_string),
                byte_count: metadata.byte_count,
                modified_at: metadata.modified_at,
                error: metadata.error,
                extra: Default::default(),
            }
        })
        .collect()
}

fn inferred_source_envelope_refs_for(routed: &RoutedSpool) -> Vec<SourceEnvelopeRefRecord> {
    let mut refs = Vec::new();
    for (path, count) in [
        ("events/windows.jsonl", routed.windows.len()),
        ("events/ax.events.jsonl", routed.ax.len()),
        ("events/sck-frame-metadata.events.jsonl", routed.sck.len()),
    ] {
        if count == 0 {
            continue;
        }
        let mut extra = BTreeMap::new();
        extra.insert("source_id".to_string(), json!("inferred_evidence_ref"));
        extra.insert("inferred".to_string(), json!(true));
        extra.insert("confidence".to_string(), json!(0.65));
        extra.insert("record_count".to_string(), json!(count));
        extra.insert(
            "inferred_from".to_string(),
            json!([{
                "source_file": path,
                "reason": "AX/window/SCK evidence available but no direct source-envelope path was supplied"
            }]),
        );
        refs.push(SourceEnvelopeRefRecord {
            schema_version: 1,
            kind: SourceEnvelopeRefKind::BundleRelative,
            path: path.to_string(),
            absolute: false,
            exists: true,
            file_name: Path::new(path)
                .file_name()
                .and_then(|file_name| file_name.to_str())
                .map(str::to_string),
            byte_count: None,
            modified_at: None,
            error: None,
            extra,
        });
    }
    refs
}

fn time_alignment_for(
    request: &ExportRequest,
    routed: &RoutedSpool,
    spool_report: &SpoolReadReport,
    source_envelope_count: usize,
    frame_2fps_records: &[Value],
) -> Value {
    let mut source_clocks = BTreeSet::new();
    let mut summaries = Vec::new();
    for (source_id, lane_id, records) in [
        ("windows", "capture.windows", &routed.windows),
        ("displays", "capture.displays", &routed.displays),
        ("capture", "capture.events", &routed.capture),
        ("ax", "capture.ax", &routed.ax),
        ("ux", "capture.ux", &routed.ux),
        (
            "sck_frame_metadata",
            "capture.active_window_frames",
            &routed.sck,
        ),
        ("browser", "capture.browser", &routed.browser),
        ("terminal", "capture.terminal", &routed.terminal),
        ("editor", "capture.editor", &routed.editor),
    ] {
        summaries.push(timing_summary(
            source_id,
            lane_id,
            records,
            &mut source_clocks,
        ));
    }
    if source_envelope_count > 0 {
        summaries.push(json!({
            "source_id": "external_refs",
            "lane_id": "capture.external_refs",
            "record_count": source_envelope_count,
            "timed_record_count": 0,
            "first_time": null,
            "last_time": null
        }));
    }
    if !frame_2fps_records.is_empty() {
        let frame_2fps_count = frame_2fps_records.len();
        let frame_times = frame_2fps_records
            .iter()
            .filter_map(|record| parse_rfc3339(record.get("recorded_at").and_then(Value::as_str)))
            .collect::<Vec<_>>();
        let first_time = frame_times.iter().min().copied();
        let last_time = frame_times.iter().max().copied();
        let timing_source = frame_2fps_records
            .iter()
            .filter_map(|record| record.get("timing_source").and_then(Value::as_str))
            .next()
            .unwrap_or("unknown");
        summaries.push(json!({
            "source_id": "frame_2fps",
            "lane_id": "capture.active_window_frames",
            "record_count": frame_2fps_count,
            "timed_record_count": frame_2fps_count,
            "first_time": first_time.unwrap_or(request.time_start).to_rfc3339(),
            "last_time": last_time.unwrap_or(request.time_end).to_rfc3339(),
            "clock_id": "media.frame_2fps.recorded_at",
            "sample_rate_fps": 2,
            "media_kind": "frame_2fps",
            "timing_source": timing_source
        }));
        source_clocks.insert("media.frame_2fps.recorded_at");
    }

    let clocks = source_clocks
        .into_iter()
        .map(|clock| {
            json!({
                "clock_id": clock,
                "time_basis": if clock == "payload.displayTime" {
                    "host_monotonic_sck_display_time"
                } else {
                    "system_utc"
                }
            })
        })
        .collect::<Vec<_>>();

    json!({
        "schema_version": 1,
        "status": "derived",
        "canonical_clock": "system_utc",
        "time_range": {
            "start": request.time_start.to_rfc3339(),
            "end": request.time_end.to_rfc3339()
        },
        "join_keys": [
            "eventTimeStart",
            "eventTimeEnd",
            "event_time_start",
            "event_time_end",
            "recordedAt",
            "monotonic_ns"
        ],
        "base_epoch": {
            "epoch_id": "unix_epoch_utc",
            "time_basis": "system_utc",
            "base_time": "1970-01-01T00:00:00Z"
        },
        "capture_window": {
            "time_start": request.time_start.to_rfc3339(),
            "time_end": request.time_end.to_rfc3339(),
            "duration_ms": (request.time_end - request.time_start).num_milliseconds(),
            "target": target_label(&request.target)
        },
        "spool_window": {
            "files_read": spool_report.files.len(),
            "records_selected": spool_report.records.len(),
            "malformed_lines": spool_report.malformed_lines.len(),
            "tolerant_read": spool_report.tolerant_read
        },
        "source_clocks_used": clocks,
        "inclusion_policy": {
            "primary_time_fields": ["eventTimeStart", "recordedAt"],
            "window_bounds": "inclusive",
            "bracketing_window_snapshots": "when no exact in-range window snapshot exists, include latest <= time_end and nearest > time_end if available",
            "derived_records": "display and connector inference records inherit the evidence timestamp"
        },
        "segment_timing_summary": summaries
    })
}

fn timing_summary(
    source_id: &str,
    lane_id: &str,
    records: &[String],
    source_clocks: &mut BTreeSet<&'static str>,
) -> Value {
    let mut first = None;
    let mut last = None;
    let mut timed = 0_u64;
    for raw in records {
        let Ok(value) = serde_json::from_str::<Value>(raw) else {
            continue;
        };
        for field in ["eventTimeStart", "recordedAt"] {
            if value.get(field).and_then(Value::as_str).is_some() {
                source_clocks.insert(field);
            }
        }
        if value
            .pointer("/payload/capturedAt")
            .and_then(Value::as_str)
            .is_some()
        {
            source_clocks.insert("payload.capturedAt");
        }
        if value
            .pointer("/payload/generatedAt")
            .and_then(Value::as_str)
            .is_some()
        {
            source_clocks.insert("payload.generatedAt");
        }
        if value.pointer("/payload/displayTime").is_some() {
            source_clocks.insert("payload.displayTime");
        }
        if let Some(time) = event_time(&value) {
            timed += 1;
            first = Some(first.map_or(time, |current: DateTime<Utc>| current.min(time)));
            last = Some(last.map_or(time, |current: DateTime<Utc>| current.max(time)));
        }
    }
    json!({
        "source_id": source_id,
        "lane_id": lane_id,
        "record_count": records.len(),
        "timed_record_count": timed,
        "first_time": first.map(|time| time.to_rfc3339()),
        "last_time": last.map(|time| time.to_rfc3339()),
        "duration_ms": first.zip(last).map(|(first, last)| (last - first).num_milliseconds())
    })
}

#[derive(Default)]
struct ExternalPathMetadata {
    exists: bool,
    byte_count: Option<u64>,
    modified_at: Option<DateTime<Utc>>,
    error: Option<String>,
}

fn metadata_for_external_path(path: &PathBuf) -> ExternalPathMetadata {
    match fs::metadata(path) {
        Ok(metadata) => ExternalPathMetadata {
            exists: true,
            byte_count: metadata.is_file().then_some(metadata.len()),
            modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
            error: None,
        },
        Err(error) => ExternalPathMetadata {
            exists: false,
            byte_count: None,
            modified_at: None,
            error: Some(error.to_string()),
        },
    }
}

fn gap_extra<const N: usize>(entries: [(&str, Value); N]) -> BTreeMap<String, Value> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_string(), value))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::io::Write;

    #[test]
    fn dated_window_bracket_lookup_scans_tail_once_for_near_end_exports() {
        let temp = tempfile::tempdir().unwrap();
        let capture_root = temp.path().join("capture");
        let paths = CaptureRootPaths::new(&capture_root);
        paths.ensure_directories().unwrap();

        let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
        let record_count = 2_500;
        let payload_padding = "x".repeat(8_192);
        let file_path = paths.windows_dir.join("2026-05-25.windows.jsonl");
        let mut file = File::create(file_path).unwrap();
        for index in 0..record_count {
            let time = base + Duration::seconds(index as i64);
            writeln!(
                file,
                "{}",
                json!({
                    "schemaVersion": 1,
                    "eventType": "capture.window_snapshot",
                    "recordedAt": time.to_rfc3339(),
                    "payload": {
                        "windows": [{
                            "windowID": index,
                            "ownerName": "Synthetic",
                            "bundleID": "com.haptica.synthetic",
                            "title": payload_padding
                        }]
                    }
                })
            )
            .unwrap();
        }

        let time_end =
            base + Duration::seconds((record_count - 3) as i64) + Duration::milliseconds(500);
        let lookup = select_bracketing_window_snapshots(&capture_root, time_end).unwrap();

        assert!(lookup.latest_at_or_before_end.is_some());
        assert!(lookup.nearest_after_end.is_some());
        assert_eq!(lookup.metrics.files_scanned, 1);
        assert!(
            lookup.metrics.lines_scanned <= 4,
            "tail lookup should not scan the whole dated window log; metrics={}",
            lookup.metrics.to_json()
        );
        assert_eq!(lookup.metrics.full_payload_parse_count, 0);
        assert_eq!(
            lookup.metrics.to_json()["parser"],
            "raw_window_snapshot_envelope"
        );
    }
}
