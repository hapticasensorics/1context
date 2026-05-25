use crate::error::{CaptureCoreError, CaptureCoreResult};
use crate::event::{CaptureEventEnvelope, RawSpoolRecord};
use crate::paths::CaptureRootPaths;
use crate::spool_index::{
    query_windows_jsonl_time_index, write_windows_jsonl_time_index,
    WindowsJsonlIndexFallbackReason, WindowsJsonlIndexLookup, WindowsJsonlRangeLookup,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

const MALFORMED_SAMPLE_MAX_BYTES: usize = 256;
const SPOOL_ENVELOPE_PREFIX_BYTES: usize = 4096;

#[derive(Clone, Debug)]
pub struct SpoolQuery {
    pub capture_root: PathBuf,
    pub time_start: DateTime<Utc>,
    pub time_end: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpoolReadMode {
    Tolerant,
    Strict,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpoolReadReport {
    pub schema_version: u32,
    pub tolerant_read: bool,
    pub records: Vec<RawSpoolRecord>,
    pub files: Vec<SpoolFileStats>,
    pub malformed_lines: Vec<MalformedSpoolLine>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SpoolFileStats {
    pub path: PathBuf,
    pub total_lines: u64,
    pub parsed_lines: u64,
    pub full_record_parse_count: u64,
    pub selected_records: u64,
    pub malformed_lines: u64,
    pub byte_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scan_strategy: Option<String>,
    #[serde(default)]
    pub index_used: bool,
    #[serde(default)]
    pub index_built: bool,
    #[serde(default)]
    pub index_refreshed: bool,
    #[serde(default)]
    pub index_checkpoint_count: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed_start_line: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indexed_start_byte_offset: Option<u64>,
    #[serde(default)]
    pub indexed_lines_scanned: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MalformedSpoolLine {
    pub source_path: PathBuf,
    pub line_number: usize,
    pub byte_offset: u64,
    pub raw_record_hash: String,
    pub raw_sample: String,
    pub parser_error: String,
}

pub fn read_spool_window(query: &SpoolQuery) -> CaptureCoreResult<Vec<RawSpoolRecord>> {
    Ok(read_spool_window_report(query, SpoolReadMode::Tolerant)?.records)
}

pub fn read_spool_window_tolerant(query: &SpoolQuery) -> CaptureCoreResult<SpoolReadReport> {
    read_spool_window_report(query, SpoolReadMode::Tolerant)
}

pub fn read_spool_window_strict(query: &SpoolQuery) -> CaptureCoreResult<Vec<RawSpoolRecord>> {
    Ok(read_spool_window_report(query, SpoolReadMode::Strict)?.records)
}

pub fn read_spool_window_report(
    query: &SpoolQuery,
    mode: SpoolReadMode,
) -> CaptureCoreResult<SpoolReadReport> {
    let paths = CaptureRootPaths::new(&query.capture_root);
    let mut files = Vec::new();
    collect_jsonl_files(&paths.events_dir, ".events.jsonl", &mut files)?;
    collect_jsonl_files(&paths.windows_dir, ".windows.jsonl", &mut files)?;
    collect_jsonl_files(&paths.displays_dir, ".displays.jsonl", &mut files)?;
    files.sort();

    let mut report = SpoolReadReport {
        schema_version: 1,
        tolerant_read: mode == SpoolReadMode::Tolerant,
        ..SpoolReadReport::default()
    };
    for file in files {
        read_jsonl_file(&file, query, mode, &mut report)?;
    }
    Ok(report)
}

fn collect_jsonl_files(
    directory: &Path,
    suffix: &str,
    files: &mut Vec<PathBuf>,
) -> CaptureCoreResult<()> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in entries {
        let entry =
            entry.map_err(|error| CaptureCoreError::io(Some(directory.to_path_buf()), error))?;
        let path = entry.path();
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(suffix))
        {
            files.push(path);
        }
    }
    Ok(())
}

fn read_jsonl_file(
    file: &Path,
    query: &SpoolQuery,
    mode: SpoolReadMode,
    report: &mut SpoolReadReport,
) -> CaptureCoreResult<()> {
    if mode == SpoolReadMode::Tolerant && fast_prefix_filter_eligible(file) {
        read_indexed_windows_jsonl_file(file, query, report)?;
        return Ok(());
    }
    read_jsonl_file_full_scan(file, query, mode, report)
}

fn read_jsonl_file_full_scan(
    file: &Path,
    query: &SpoolQuery,
    mode: SpoolReadMode,
    report: &mut SpoolReadReport,
) -> CaptureCoreResult<()> {
    let handle =
        File::open(file).map_err(|error| CaptureCoreError::io(Some(file.to_path_buf()), error))?;
    let mut reader = BufReader::new(handle);
    let mut line = Vec::new();
    let mut line_number = 0_usize;
    let mut byte_offset = 0_u64;
    let mut stats = SpoolFileStats {
        path: file.to_path_buf(),
        ..SpoolFileStats::default()
    };

    loop {
        line.clear();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| CaptureCoreError::io(Some(file.to_path_buf()), error))?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        stats.total_lines += 1;
        stats.byte_count += bytes as u64;
        let trimmed = trim_jsonl_newline(&line);
        if !trimmed.is_empty() {
            if mode == SpoolReadMode::Tolerant && fast_prefix_filter_eligible(file) {
                if let Some(time) = primary_time_from_json_prefix(trimmed) {
                    stats.parsed_lines += 1;
                    if time >= query.time_start && time <= query.time_end {
                        match serde_json::from_slice::<CaptureEventEnvelope>(trimmed) {
                            Ok(envelope) => {
                                stats.full_record_parse_count += 1;
                                stats.selected_records += 1;
                                report.records.push(RawSpoolRecord {
                                    source_path: file.to_path_buf(),
                                    line_number,
                                    byte_offset,
                                    raw_record_hash: sha256_hex(trimmed),
                                    raw_json: String::from_utf8_lossy(trimmed).into_owned(),
                                    envelope,
                                });
                            }
                            Err(error) => {
                                stats.malformed_lines += 1;
                                report.malformed_lines.push(MalformedSpoolLine {
                                    source_path: file.to_path_buf(),
                                    line_number,
                                    byte_offset,
                                    raw_record_hash: sha256_hex(trimmed),
                                    raw_sample: sample_text(trimmed),
                                    parser_error: error.to_string(),
                                });
                            }
                        }
                    }
                    byte_offset += bytes as u64;
                    continue;
                }
            }
            match serde_json::from_slice::<SpoolEnvelopeFilter>(trimmed) {
                Ok(filter) => {
                    stats.parsed_lines += 1;
                    if filter
                        .primary_time()
                        .is_some_and(|time| time >= query.time_start && time <= query.time_end)
                    {
                        let envelope = serde_json::from_slice::<CaptureEventEnvelope>(trimmed)
                            .map_err(|error| {
                                CaptureCoreError::json(
                                    Some(PathBuf::from(format!(
                                        "{}:{line_number}",
                                        file.display()
                                    ))),
                                    error,
                                )
                            })?;
                        stats.full_record_parse_count += 1;
                        stats.selected_records += 1;
                        report.records.push(RawSpoolRecord {
                            source_path: file.to_path_buf(),
                            line_number,
                            byte_offset,
                            raw_record_hash: sha256_hex(trimmed),
                            raw_json: String::from_utf8_lossy(trimmed).into_owned(),
                            envelope,
                        });
                    }
                }
                Err(error) if mode == SpoolReadMode::Tolerant => {
                    stats.malformed_lines += 1;
                    report.malformed_lines.push(MalformedSpoolLine {
                        source_path: file.to_path_buf(),
                        line_number,
                        byte_offset,
                        raw_record_hash: sha256_hex(trimmed),
                        raw_sample: sample_text(trimmed),
                        parser_error: error.to_string(),
                    });
                }
                Err(error) => {
                    return Err(CaptureCoreError::json(
                        Some(PathBuf::from(format!("{}:{line_number}", file.display()))),
                        error,
                    ));
                }
            }
        }
        byte_offset += bytes as u64;
    }
    report.files.push(stats);
    Ok(())
}

fn read_indexed_windows_jsonl_file(
    file: &Path,
    query: &SpoolQuery,
    report: &mut SpoolReadReport,
) -> CaptureCoreResult<()> {
    let Some(indexed) = load_or_refresh_windows_time_range(file, query)? else {
        read_jsonl_file_full_scan(file, query, SpoolReadMode::Tolerant, report)?;
        return Ok(());
    };
    let first_range = indexed.lookup.ranges.first();

    let mut stats = SpoolFileStats {
        path: file.to_path_buf(),
        total_lines: indexed.lookup.total_lines,
        byte_count: indexed.lookup.source_byte_count,
        scan_strategy: Some("windows_spool_time_index".to_string()),
        index_used: true,
        index_built: indexed.built,
        index_refreshed: indexed.refreshed,
        index_checkpoint_count: indexed.lookup.indexed_entry_count,
        indexed_start_line: first_range.map(|range| range.line_start),
        indexed_start_byte_offset: first_range.map(|range| range.byte_start),
        ..SpoolFileStats::default()
    };

    let handle =
        File::open(file).map_err(|error| CaptureCoreError::io(Some(file.to_path_buf()), error))?;
    let mut reader = BufReader::new(handle);

    for range in &indexed.lookup.ranges {
        reader
            .seek(SeekFrom::Start(range.byte_start))
            .map_err(|error| CaptureCoreError::io(Some(file.to_path_buf()), error))?;
        let mut bytes = vec![0; (range.byte_end - range.byte_start) as usize];
        reader
            .read_exact(&mut bytes)
            .map_err(|error| CaptureCoreError::io(Some(file.to_path_buf()), error))?;
        let mut range_reader = std::io::Cursor::new(bytes);
        let mut line = Vec::new();
        let mut line_number = range.line_start.saturating_sub(1) as usize;
        let mut byte_offset = range.byte_start;
        loop {
            line.clear();
            let bytes = range_reader
                .read_until(b'\n', &mut line)
                .map_err(|error| CaptureCoreError::io(Some(file.to_path_buf()), error))?;
            if bytes == 0 {
                break;
            }
            line_number += 1;
            stats.indexed_lines_scanned += 1;
            let trimmed = trim_jsonl_newline(&line);
            if trimmed.is_empty() {
                byte_offset += bytes as u64;
                continue;
            }
            stats.parsed_lines += 1;
            match serde_json::from_slice::<CaptureEventEnvelope>(trimmed) {
                Ok(envelope) => {
                    stats.full_record_parse_count += 1;
                    stats.selected_records += 1;
                    report.records.push(RawSpoolRecord {
                        source_path: file.to_path_buf(),
                        line_number,
                        byte_offset,
                        raw_record_hash: sha256_hex(trimmed),
                        raw_json: String::from_utf8_lossy(trimmed).into_owned(),
                        envelope,
                    });
                }
                Err(error) => {
                    stats.malformed_lines += 1;
                    report.malformed_lines.push(MalformedSpoolLine {
                        source_path: file.to_path_buf(),
                        line_number,
                        byte_offset,
                        raw_record_hash: sha256_hex(trimmed),
                        raw_sample: sample_text(trimmed),
                        parser_error: error.to_string(),
                    });
                }
            }
            byte_offset += bytes as u64;
        }
    }

    report.files.push(stats);
    Ok(())
}

struct LoadedWindowsTimeRange {
    lookup: WindowsJsonlRangeLookup,
    built: bool,
    refreshed: bool,
}

fn load_or_refresh_windows_time_range(
    file: &Path,
    query: &SpoolQuery,
) -> CaptureCoreResult<Option<LoadedWindowsTimeRange>> {
    match query_windows_jsonl_time_index(file, query.time_start, query.time_end)? {
        WindowsJsonlIndexLookup::Indexed(lookup) => Ok(Some(LoadedWindowsTimeRange {
            lookup,
            built: false,
            refreshed: false,
        })),
        WindowsJsonlIndexLookup::Fallback(fallback) => match fallback.reason {
            WindowsJsonlIndexFallbackReason::MissingSidecar => {
                let _ = write_windows_jsonl_time_index(file)?;
                indexed_time_range_after_refresh(file, query, true, false)
            }
            WindowsJsonlIndexFallbackReason::StaleSidecar => {
                let _ = write_windows_jsonl_time_index(file)?;
                indexed_time_range_after_refresh(file, query, false, true)
            }
            WindowsJsonlIndexFallbackReason::UnsupportedSourceFile
            | WindowsJsonlIndexFallbackReason::CorruptSidecar
            | WindowsJsonlIndexFallbackReason::IncompleteSidecar => Ok(None),
        },
    }
}

fn indexed_time_range_after_refresh(
    file: &Path,
    query: &SpoolQuery,
    built: bool,
    refreshed: bool,
) -> CaptureCoreResult<Option<LoadedWindowsTimeRange>> {
    match query_windows_jsonl_time_index(file, query.time_start, query.time_end)? {
        WindowsJsonlIndexLookup::Indexed(lookup) => Ok(Some(LoadedWindowsTimeRange {
            lookup,
            built,
            refreshed,
        })),
        WindowsJsonlIndexLookup::Fallback(_) => Ok(None),
    }
}

fn fast_prefix_filter_eligible(file: &Path) -> bool {
    file.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(".windows.jsonl"))
}

fn primary_time_from_json_prefix(line: &[u8]) -> Option<DateTime<Utc>> {
    let prefix_len = line.len().min(SPOOL_ENVELOPE_PREFIX_BYTES);
    let prefix = std::str::from_utf8(&line[..prefix_len]).ok()?;
    [
        "eventTimeStart",
        "recordedAt",
        "event_time_start",
        "recorded_at",
    ]
    .into_iter()
    .find_map(|key| parse_rfc3339(json_string_field(prefix, key)))
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

fn parse_rfc3339(value: Option<&str>) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value?)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

#[derive(Deserialize)]
struct SpoolEnvelopeFilter {
    #[serde(default, alias = "recordedAt")]
    recorded_at: Option<DateTime<Utc>>,
    #[serde(default, alias = "eventTimeStart")]
    event_time_start: Option<DateTime<Utc>>,
}

impl SpoolEnvelopeFilter {
    fn primary_time(&self) -> Option<DateTime<Utc>> {
        self.event_time_start.or(self.recorded_at)
    }
}

fn trim_jsonl_newline(line: &[u8]) -> &[u8] {
    let without_lf = line.strip_suffix(b"\n").unwrap_or(line);
    without_lf.strip_suffix(b"\r").unwrap_or(without_lf)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

fn sample_text(value: &[u8]) -> String {
    let sample_len = value.len().min(MALFORMED_SAMPLE_MAX_BYTES);
    let mut out = String::from_utf8_lossy(&value[..sample_len]).into_owned();
    if value.len() > MALFORMED_SAMPLE_MAX_BYTES {
        out.push_str("...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::CaptureRootPaths;
    use chrono::{Duration, TimeZone};
    use serde_json::json;
    use std::fs;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn tolerant_read_reports_malformed_lines_without_aborting_window() {
        let (_temp_dir, query, file) = mixed_spool_fixture();

        let report = read_spool_window_tolerant(&query).unwrap();

        assert!(report.tolerant_read);
        assert_eq!(report.schema_version, 1);
        assert_eq!(report.records.len(), 1);
        assert_eq!(report.records[0].line_number, 1);
        assert_eq!(report.records[0].byte_offset, 0);
        assert_eq!(report.records[0].source_path, file);
        assert_eq!(
            report.records[0].envelope.event_type,
            "capture.window_snapshot"
        );

        assert_eq!(report.files.len(), 1);
        assert_eq!(report.files[0].path, file);
        assert_eq!(report.files[0].total_lines, 4);
        assert_eq!(report.files[0].parsed_lines, 2);
        assert_eq!(report.files[0].selected_records, 1);
        assert_eq!(report.files[0].malformed_lines, 2);
        assert!(report.files[0].byte_count > 0);

        assert_eq!(report.malformed_lines.len(), 2);
        assert_eq!(report.malformed_lines[0].source_path, file);
        assert_eq!(report.malformed_lines[0].line_number, 3);
        assert!(report.malformed_lines[0].byte_offset > 0);
        assert_eq!(report.malformed_lines[0].raw_record_hash.len(), 64);
        assert!(report.malformed_lines[0]
            .raw_sample
            .contains("2026-05-25T12:03:00Z"));
        assert!(!report.malformed_lines[0].parser_error.is_empty());

        assert_eq!(report.malformed_lines[1].line_number, 4);
        assert!(report.malformed_lines[1]
            .raw_sample
            .contains("2026-05-25T12:00:30Z"));

        let provenance = report.records[0].provenance();
        assert!(provenance.raw_source_uri.starts_with("file://"));
        assert!(provenance.raw_source_uri.contains("mixed.events.jsonl"));
        assert_eq!(provenance.raw_line_number, 1);
        assert_eq!(provenance.raw_byte_offset, 0);
        assert_eq!(
            provenance.raw_record_hash,
            report.records[0].raw_record_hash
        );
    }

    #[test]
    fn strict_read_fails_on_malformed_line_before_window_filtering() {
        let (_temp_dir, query, _file) = mixed_spool_fixture();

        let error = read_spool_window_strict(&query).unwrap_err();

        assert!(error.to_string().contains("mixed.events.jsonl:3"));
    }

    #[test]
    fn default_read_spool_window_uses_tolerant_mode() {
        let (_temp_dir, query, _file) = mixed_spool_fixture();

        let records = read_spool_window(&query).unwrap();

        assert_eq!(records.len(), 1);
        assert_eq!(records[0].line_number, 1);
    }

    #[test]
    fn indexed_windows_spool_reads_from_checkpoint_instead_of_scanning_whole_file() {
        let temp_dir = tempfile::tempdir().unwrap();
        let capture_root = temp_dir.path().join("capture");
        let paths = CaptureRootPaths::new(&capture_root);
        paths.ensure_directories().unwrap();

        let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
        let file = paths.windows_dir.join("2026-05-25.windows.jsonl");
        write_window_records(&file, base, 1_000);

        let query = SpoolQuery {
            capture_root: capture_root.clone(),
            time_start: base + Duration::seconds(996),
            time_end: base + Duration::seconds(997),
        };
        let first = read_spool_window_tolerant(&query).unwrap();
        let first_stats = first.files.iter().find(|stats| stats.path == file).unwrap();
        assert!(first_stats.index_used);
        assert!(first_stats.index_built);
        assert_eq!(first_stats.total_lines, 1_000);
        assert!(first_stats.indexed_lines_scanned < 130);
        assert_eq!(first.records.len(), 2);
        assert_eq!(first.records[0].line_number, 997);

        let second = read_spool_window_tolerant(&query).unwrap();
        let second_stats = second
            .files
            .iter()
            .find(|stats| stats.path == file)
            .unwrap();
        assert!(second_stats.index_used);
        assert!(!second_stats.index_built);
        assert_eq!(second_stats.total_lines, 1_000);
        assert!(second_stats.indexed_lines_scanned < 130);
        assert_eq!(second.records.len(), 2);
    }

    #[test]
    fn indexed_windows_spool_refreshes_append_only_logs() {
        let temp_dir = tempfile::tempdir().unwrap();
        let capture_root = temp_dir.path().join("capture");
        let paths = CaptureRootPaths::new(&capture_root);
        paths.ensure_directories().unwrap();

        let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
        let file = paths.windows_dir.join("2026-05-25.windows.jsonl");
        write_window_records(&file, base, 4);

        let early_query = SpoolQuery {
            capture_root: capture_root.clone(),
            time_start: base + Duration::seconds(2),
            time_end: base + Duration::seconds(2),
        };
        let early = read_spool_window_tolerant(&early_query).unwrap();
        assert!(early.files[0].index_built);

        let mut append = fs::OpenOptions::new().append(true).open(&file).unwrap();
        for index in 4..8 {
            writeln!(
                append,
                "{}",
                window_record_line(base + Duration::seconds(index), index as u64)
            )
            .unwrap();
        }
        drop(append);

        let later_query = SpoolQuery {
            capture_root,
            time_start: base + Duration::seconds(7),
            time_end: base + Duration::seconds(7),
        };
        let later = read_spool_window_tolerant(&later_query).unwrap();
        assert_eq!(later.records.len(), 1);
        assert_eq!(later.records[0].line_number, 8);
        assert_eq!(later.files[0].total_lines, 8);
        assert!(later.files[0].index_refreshed || later.files[0].index_built);
    }

    fn mixed_spool_fixture() -> (TempDir, SpoolQuery, PathBuf) {
        let temp_dir = tempfile::tempdir().unwrap();
        let capture_root = temp_dir.path().join("capture");
        let paths = CaptureRootPaths::new(&capture_root);
        paths.ensure_directories().unwrap();

        let time_start = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
        let time_end = time_start + Duration::minutes(1);
        let in_window = time_start + Duration::seconds(10);
        let out_of_window = time_start + Duration::minutes(3);

        let file = paths.events_dir.join("mixed.events.jsonl");
        let lines = [
            envelope_line("capture.window_snapshot", in_window),
            envelope_line("capture.ux.input", out_of_window),
            concat!(
                "{\"schemaVersion\":1,\"eventType\":\"capture.ux.input\",",
                "\"eventTimeStart\":\"2026-05-25T12:03:00Z\",\"payload\":"
            )
            .to_string(),
            concat!(
                "{\"schemaVersion\":1,\"eventType\":\"capture.window_snapshot\",",
                "\"eventTimeStart\":\"2026-05-25T12:00:30Z\" \"payload\":{}}"
            )
            .to_string(),
        ]
        .join("\n");
        fs::write(&file, format!("{lines}\n")).unwrap();

        (
            temp_dir,
            SpoolQuery {
                capture_root,
                time_start,
                time_end,
            },
            file,
        )
    }

    fn envelope_line(event_type: &str, time: DateTime<Utc>) -> String {
        serde_json::to_string(&json!({
            "schemaVersion": 1,
            "eventType": event_type,
            "recordedAt": time.to_rfc3339(),
            "eventTimeStart": time.to_rfc3339(),
            "eventTimeEnd": time.to_rfc3339(),
            "laneID": "capture.test",
            "payload": {}
        }))
        .unwrap()
    }

    fn write_window_records(file: &Path, base: DateTime<Utc>, count: i64) {
        let mut output = File::create(file).unwrap();
        for index in 0..count {
            writeln!(
                output,
                "{}",
                window_record_line(base + Duration::seconds(index), index as u64)
            )
            .unwrap();
        }
    }

    fn window_record_line(time: DateTime<Utc>, window_id: u64) -> String {
        serde_json::to_string(&json!({
            "schemaVersion": 1,
            "eventType": "capture.window_snapshot",
            "recordedAt": time.to_rfc3339(),
            "eventTimeStart": time.to_rfc3339(),
            "eventTimeEnd": time.to_rfc3339(),
            "laneID": "capture.windows",
            "payload": {
                "windows": [{
                    "windowID": window_id,
                    "ownerName": "Synthetic",
                    "title": "Synthetic window"
                }]
            }
        }))
        .unwrap()
    }
}
