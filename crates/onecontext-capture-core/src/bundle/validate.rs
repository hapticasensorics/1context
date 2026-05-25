use crate::bundle::schema::{
    BundleState, CaptureBundleManifest, SourceInventory, ValidationFinding, ValidationReport,
    ValidationSeverity,
};
use crate::bundle::writer::walk_files;
use crate::error::{CaptureCoreError, CaptureCoreResult};
use crate::lanes::{mandatory_lane_ids, required_bundle_files};
use crate::paths::BundleRelativePath;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValidationMode {
    Candidate,
    Ready,
}

pub fn validate_ready_bundle(path: impl AsRef<Path>) -> CaptureCoreResult<ValidationReport> {
    validate_bundle(path, ValidationMode::Ready)
}

pub(crate) fn validate_bundle_candidate(
    path: impl AsRef<Path>,
) -> CaptureCoreResult<ValidationReport> {
    validate_bundle(path, ValidationMode::Candidate)
}

fn validate_bundle(
    path: impl AsRef<Path>,
    mode: ValidationMode,
) -> CaptureCoreResult<ValidationReport> {
    let root = path.as_ref();
    let manifest_path = root.join("manifest.json");
    let mut report = if manifest_path.exists() {
        let manifest: CaptureBundleManifest = read_json(&manifest_path)?;
        let mut report = ValidationReport::new(Some(manifest.capture_id.clone()));
        if mode == ValidationMode::Ready && manifest.state != BundleState::Ready {
            report.push(fatal(
                "manifest_not_ready",
                Some("manifest.json"),
                "manifest.state must be ready",
            ));
        } else if mode == ValidationMode::Candidate && manifest.state == BundleState::Failed {
            report.push(fatal(
                "manifest_failed",
                Some("manifest.json"),
                "failed bundles are not valid export candidates",
            ));
        }
        for value in manifest
            .v0_required_files
            .iter()
            .chain(manifest.optional_files.iter())
        {
            if let Err(error) = BundleRelativePath::parse(value) {
                report.push(fatal(
                    "invalid_manifest_path",
                    Some("manifest.json"),
                    &error.to_string(),
                ));
            }
        }
        report
    } else {
        let mut report = ValidationReport::new(None);
        report.push(fatal(
            "missing_manifest",
            Some("manifest.json"),
            "manifest.json is required",
        ));
        report
    };

    if mode == ValidationMode::Ready && !root.join("READY").exists() {
        report.push(fatal(
            "missing_ready",
            Some("READY"),
            "READY sentinel is required",
        ));
    }

    for required in required_bundle_files() {
        let relative = BundleRelativePath::parse(*required)?;
        let file = relative.join_under(root);
        if !file.exists() {
            report.push(fatal(
                "missing_required_file",
                Some(required),
                "required V0 file is missing",
            ));
            continue;
        }
        validate_file(root, required, &file, &mut report)?;
    }

    validate_sources(root, &mut report)?;
    validate_time_alignment(root, &mut report)?;
    validate_media_consistency(root, &mut report)?;
    validate_provenance_refs(root, &mut report)?;
    validate_forbidden_attention_outputs(root, &mut report)?;
    Ok(report)
}

fn validate_file(
    root: &Path,
    relative: &str,
    file: &Path,
    report: &mut ValidationReport,
) -> CaptureCoreResult<()> {
    if relative.ends_with(".json") {
        let _: Value = read_json(file)?;
    } else if relative.ends_with(".jsonl") {
        let text = fs::read_to_string(file)
            .map_err(|error| CaptureCoreError::io(Some(file.to_path_buf()), error))?;
        if relative == "events/windows.jsonl" && text.trim().is_empty() {
            report.push(fatal(
                "empty_windows_jsonl",
                Some(relative),
                "events/windows.jsonl must contain at least one window snapshot",
            ));
        }
        if relative == "events/displays.jsonl" && text.trim().is_empty() {
            report.push(fatal(
                "empty_displays_jsonl",
                Some(relative),
                "events/displays.jsonl must contain display context for V0 READY bundles",
            ));
        }
        let mut has_window_context = false;
        let mut has_display_context = false;
        for (index, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let value = serde_json::from_str::<Value>(line).map_err(|error| {
                CaptureCoreError::json(Some(root.join(format!("{relative}:{}", index + 1))), error)
            })?;
            if relative == "events/windows.jsonl" {
                has_window_context |= has_non_empty_array(&value, &["payload", "windows"])
                    || has_non_empty_array(&value, &["windows"]);
            }
            if relative == "events/displays.jsonl" {
                has_display_context |= has_non_empty_array(&value, &["payload", "displays"])
                    || has_non_empty_array(&value, &["displays"]);
            }
        }
        if relative == "events/windows.jsonl" && !text.trim().is_empty() && !has_window_context {
            report.push(fatal(
                "empty_window_context",
                Some(relative),
                "window snapshots must include a non-empty windows array",
            ));
        }
        if relative == "events/displays.jsonl" && !text.trim().is_empty() && !has_display_context {
            report.push(fatal(
                "empty_display_context",
                Some(relative),
                "display snapshots must include a non-empty displays array",
            ));
        }
    }
    Ok(())
}

fn validate_sources(root: &Path, report: &mut ValidationReport) -> CaptureCoreResult<()> {
    let sources_path = root.join("sources.json");
    if !sources_path.exists() {
        return Ok(());
    }
    let inventory: SourceInventory = read_json(&sources_path)?;
    let present: BTreeSet<_> = inventory
        .sources
        .iter()
        .map(|source| source.lane_id.as_str())
        .collect();
    let known_gap_source_ids = known_gap_source_ids(root)?;
    for lane in mandatory_lane_ids() {
        if !present.contains(lane) {
            report.push(fatal(
                "missing_mandatory_lane_source",
                Some("sources.json"),
                &format!("sources.json lacks mandatory lane {lane}"),
            ));
        }
    }
    let actual_counts = actual_lane_counts(root)?;
    let lane_files = lane_files();
    for source in &inventory.sources {
        if source.status == crate::bundle::schema::SourceStatus::Present && source.record_count == 0
        {
            report.push(fatal(
                "present_source_has_no_records",
                Some("sources.json"),
                &format!(
                    "source {} is present but record_count is zero",
                    source.source_id
                ),
            ));
        }
        if source.status != crate::bundle::schema::SourceStatus::Present && source.record_count > 0
        {
            report.push(fatal(
                "degraded_source_has_records",
                Some("sources.json"),
                &format!(
                    "source {} is {:?} but record_count is {}",
                    source.source_id, source.status, source.record_count
                ),
            ));
        }
        if source.required_for_v0
            && source.status != crate::bundle::schema::SourceStatus::Present
            && !known_gap_source_ids.contains(source.source_id.as_str())
        {
            report.push(fatal(
                "missing_known_gap_for_degraded_lane",
                Some("quality/known_gaps.jsonl"),
                &format!(
                    "degraded required source {} must have a known-gap record",
                    source.source_id
                ),
            ));
        }
        if let Some((_, relative)) = lane_files.get(source.lane_id.as_str()) {
            if let Some(actual_count) = actual_counts.get(*relative) {
                if source.record_count != *actual_count {
                    report.push(fatal(
                        "source_record_count_mismatch",
                        Some("sources.json"),
                        &format!(
                            "source {} reports {} records but {} contains {}",
                            source.source_id, source.record_count, relative, actual_count
                        ),
                    ));
                }
            }
        } else if source.lane_id.starts_with("capabilities/") {
            let expected_status = capability_status(root, &source.lane_id)?;
            if source.status != expected_status {
                report.push(fatal(
                    "capability_source_status_mismatch",
                    Some("sources.json"),
                    &format!(
                        "source {} status does not match {}",
                        source.source_id, source.lane_id
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_time_alignment(root: &Path, report: &mut ValidationReport) -> CaptureCoreResult<()> {
    let path = root.join("time_alignment.json");
    if !path.exists() {
        return Ok(());
    }
    let value: Value = read_json(&path)?;
    if string_path(&value, &["status"]).is_some_and(|status| status == "scaffold") {
        report.push(fatal(
            "scaffold_time_alignment",
            Some("time_alignment.json"),
            "time_alignment.json must contain concrete V0 clock alignment metadata, not scaffold status",
        ));
    }
    if string_path(&value, &["canonical_clock"]).is_none() {
        report.push(fatal(
            "missing_time_alignment_canonical_clock",
            Some("time_alignment.json"),
            "time_alignment.json must name the canonical clock",
        ));
    }
    if string_path(&value, &["time_range", "start"]).is_none()
        || string_path(&value, &["time_range", "end"]).is_none()
    {
        report.push(fatal(
            "missing_time_alignment_range",
            Some("time_alignment.json"),
            "time_alignment.json must include the exported time range",
        ));
    }
    if !value
        .get("join_keys")
        .and_then(Value::as_array)
        .is_some_and(|keys| !keys.is_empty())
    {
        report.push(fatal(
            "missing_time_alignment_join_keys",
            Some("time_alignment.json"),
            "time_alignment.json must list timestamp join keys",
        ));
    }
    Ok(())
}

fn validate_media_consistency(root: &Path, report: &mut ValidationReport) -> CaptureCoreResult<()> {
    let sck = read_jsonl_values(&root.join("events/sck-frame-metadata.events.jsonl"))?;
    let media = read_jsonl_values(&root.join("media/media.index.jsonl"))?;
    let known_gaps = known_gap_codes(root)?;
    let has_media_candidate = sck.iter().any(sck_indicates_media_candidate);
    let has_media_gap = known_gaps
        .contains(&("media".to_string(), "media_candidate_omitted".to_string()))
        || known_gaps.contains(&("media".to_string(), "keyframe_media_omitted".to_string()))
        || known_gaps.contains(&("media".to_string(), "media_frame_omitted".to_string()));

    if has_media_candidate && media.is_empty() && !has_media_gap {
        report.push(fatal(
            "empty_media_for_keyframe_candidates",
            Some("media/media.index.jsonl"),
            "SCK/keyframe metadata indicates media candidates, but media.index.jsonl is empty and no media known-gap explains the omission",
        ));
    }
    if !media.iter().any(is_available_frame_2fps) {
        report.push(fatal(
            "missing_frame_2fps_media",
            Some("media/media.index.jsonl"),
            "READY capture bundles must include available 2fps screenshot frames under media/frames-2fps/",
        ));
    }

    for (index, record) in media.iter().enumerate() {
        let path = string_path(record, &["path"]).or_else(|| string_path(record, &["uri"]));
        let Some(path) = path else {
            continue;
        };
        if path.contains("://") {
            continue;
        }
        let Ok(relative) = BundleRelativePath::parse(path) else {
            report.push(fatal(
                "invalid_media_path",
                Some("media/media.index.jsonl"),
                &format!(
                    "media record {} has invalid bundle-relative path",
                    index + 1
                ),
            ));
            continue;
        };
        if !relative.join_under(root).exists() {
            report.push(fatal(
                "missing_media_artifact",
                Some("media/media.index.jsonl"),
                &format!(
                    "media record {} references missing artifact {path}",
                    index + 1
                ),
            ));
        }
    }
    Ok(())
}

fn is_available_frame_2fps(record: &Value) -> bool {
    let kind = string_path(record, &["kind"]);
    let status = string_path(record, &["status"]).or_else(|| string_path(record, &["state"]));
    let path = string_path(record, &["path"]).or_else(|| string_path(record, &["uri"]));
    kind == Some("frame_2fps")
        && matches!(status, Some("available" | "present" | "ready"))
        && path.is_some_and(|path| path.starts_with("media/frames-2fps/"))
}

fn validate_provenance_refs(root: &Path, report: &mut ValidationReport) -> CaptureCoreResult<()> {
    let refs = read_jsonl_values(&root.join("external_refs/source-envelopes.jsonl"))?;
    for (index, record) in refs.iter().enumerate() {
        let kind = string_path(record, &["kind"]);
        if kind == Some("bundle_relative") {
            let Some(path) = string_path(record, &["path"]) else {
                report.push(fatal(
                    "missing_source_envelope_path",
                    Some("external_refs/source-envelopes.jsonl"),
                    &format!("source envelope ref {} is missing path", index + 1),
                ));
                continue;
            };
            let Ok(relative) = BundleRelativePath::parse(path) else {
                report.push(fatal(
                    "invalid_source_envelope_path",
                    Some("external_refs/source-envelopes.jsonl"),
                    &format!("source envelope ref {} has invalid path", index + 1),
                ));
                continue;
            };
            if !relative.join_under(root).exists() {
                report.push(fatal(
                    "missing_source_envelope_artifact",
                    Some("external_refs/source-envelopes.jsonl"),
                    &format!(
                        "source envelope ref {} points at missing artifact {path}",
                        index + 1
                    ),
                ));
            }
        }
    }
    Ok(())
}

fn validate_forbidden_attention_outputs(
    root: &Path,
    report: &mut ValidationReport,
) -> CaptureCoreResult<()> {
    for file in walk_files(root)? {
        let relative = file.strip_prefix(root).unwrap_or(&file).to_string_lossy();
        let name = relative.as_ref();
        for forbidden in [
            "attention-filter-output",
            "memory-write",
            "memory_written",
            "keep-drop",
            "composites/",
            "decisions.jsonl",
        ] {
            if name.contains(forbidden) {
                report.push(fatal(
                    "attention_output_in_bundle",
                    Some(name),
                    "capture bundle must not contain attention-filter output",
                ));
            }
        }
    }
    Ok(())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> CaptureCoreResult<T> {
    let text = fs::read_to_string(path)
        .map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
    serde_json::from_str(&text)
        .map_err(|error| CaptureCoreError::json(Some(path.to_path_buf()), error))
}

fn read_jsonl_values(path: &Path) -> CaptureCoreResult<Vec<Value>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let text = fs::read_to_string(path)
        .map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
    let mut values = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        values.push(serde_json::from_str::<Value>(line).map_err(|error| {
            CaptureCoreError::json(
                Some(path.with_file_name(format!(
                "{}:{}",
                path.file_name().and_then(|name| name.to_str()).unwrap_or("jsonl"),
                index + 1
            ))),
                error,
            )
        })?);
    }
    Ok(values)
}

fn known_gap_source_ids(root: &Path) -> CaptureCoreResult<BTreeSet<String>> {
    Ok(read_jsonl_values(&root.join("quality/known_gaps.jsonl"))?
        .into_iter()
        .filter_map(|value| string_path(&value, &["source_id"]).map(str::to_string))
        .collect())
}

fn known_gap_codes(root: &Path) -> CaptureCoreResult<BTreeSet<(String, String)>> {
    Ok(read_jsonl_values(&root.join("quality/known_gaps.jsonl"))?
        .into_iter()
        .filter_map(|value| {
            Some((
                string_path(&value, &["source_id"])?.to_string(),
                string_path(&value, &["code"])?.to_string(),
            ))
        })
        .collect())
}

fn actual_lane_counts(root: &Path) -> CaptureCoreResult<BTreeMap<&'static str, u64>> {
    let mut counts = BTreeMap::new();
    for (_, relative) in lane_files().values() {
        counts.insert(
            *relative,
            read_jsonl_values(&root.join(relative))?.len() as u64,
        );
    }
    if let Some(active_window_frames) = counts.get_mut("events/sck-frame-metadata.events.jsonl") {
        *active_window_frames += read_jsonl_values(&root.join("media/media.index.jsonl"))?
            .iter()
            .filter(|record| is_available_frame_2fps(record))
            .count() as u64;
    }
    Ok(counts)
}

fn lane_files() -> BTreeMap<&'static str, (&'static str, &'static str)> {
    BTreeMap::from([
        ("capture.windows", ("windows", "events/windows.jsonl")),
        ("capture.displays", ("displays", "events/displays.jsonl")),
        ("capture.events", ("capture", "events/capture.events.jsonl")),
        ("capture.ax", ("ax", "events/ax.events.jsonl")),
        ("capture.ux", ("ux", "events/ux.events.jsonl")),
        (
            "capture.active_window_frames",
            (
                "sck_frame_metadata",
                "events/sck-frame-metadata.events.jsonl",
            ),
        ),
        (
            "capture.browser",
            ("browser", "events/browser.events.jsonl"),
        ),
        (
            "capture.terminal",
            ("terminal", "events/terminal.events.jsonl"),
        ),
        ("capture.editor", ("editor", "events/editor.events.jsonl")),
        (
            "capture.external_refs",
            ("external_refs", "external_refs/source-envelopes.jsonl"),
        ),
    ])
}

fn capability_status(
    root: &Path,
    relative: &str,
) -> CaptureCoreResult<crate::bundle::schema::SourceStatus> {
    let value: Value = read_json(&root.join(relative))?;
    if bool_path(&value, &["ready"]) == Some(true) {
        return Ok(crate::bundle::schema::SourceStatus::Present);
    }
    if bool_path(&value, &["ready"]) == Some(false) {
        return Ok(crate::bundle::schema::SourceStatus::Degraded);
    }
    Ok(match string_path(&value, &["status"]) {
        Some("ok" | "present" | "ready" | "available" | "granted") => {
            crate::bundle::schema::SourceStatus::Present
        }
        Some("permission_denied" | "denied") => {
            crate::bundle::schema::SourceStatus::PermissionDenied
        }
        Some("source_unavailable" | "unavailable") => {
            crate::bundle::schema::SourceStatus::SourceUnavailable
        }
        Some("disabled_by_policy") => crate::bundle::schema::SourceStatus::DisabledByPolicy,
        Some(_) => crate::bundle::schema::SourceStatus::Degraded,
        None => crate::bundle::schema::SourceStatus::Present,
    })
}

fn sck_indicates_media_candidate(value: &Value) -> bool {
    bool_path(value, &["attachmentsPresent"]) == Some(true)
        || bool_path(value, &["payload", "attachmentsPresent"]) == Some(true)
        || bool_path(
            value,
            &["payload", "adaptiveDecision", "shouldStoreKeyframe"],
        ) == Some(true)
        || bool_path(
            value,
            &["payload", "adaptiveDecision", "shouldEncodeVideoSegment"],
        ) == Some(true)
        || string_path(value, &["mediaRef"]).is_some()
        || string_path(value, &["payload", "mediaRef"]).is_some()
}

fn has_non_empty_array(value: &Value, path: &[&str]) -> bool {
    value_path(value, path)
        .and_then(Value::as_array)
        .is_some_and(|items| !items.is_empty())
}

fn string_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    value_path(value, path).and_then(Value::as_str)
}

fn bool_path(value: &Value, path: &[&str]) -> Option<bool> {
    value_path(value, path).and_then(Value::as_bool)
}

fn value_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for component in path {
        current = current.get(*component)?;
    }
    Some(current)
}

fn fatal(code: &str, path: Option<&str>, message: &str) -> ValidationFinding {
    ValidationFinding {
        severity: ValidationSeverity::Fatal,
        code: code.to_string(),
        path: path.map(str::to_string),
        message: message.to_string(),
        blocks_ready: true,
    }
}
