use crate::error::{CaptureCoreError, CaptureCoreResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

const INDEX_SCHEMA_VERSION: u32 = 1;
const SPOOL_ENVELOPE_PREFIX_BYTES: usize = 4096;
const WINDOWS_JSONL_SUFFIX: &str = ".windows.jsonl";
const INDEX_SUFFIX: &str = ".time-index.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WindowsJsonlTimeIndex {
    pub schema_version: u32,
    pub source_path: PathBuf,
    pub source: SourceFileFingerprint,
    pub total_lines: u64,
    pub indexed_lines: u64,
    pub empty_lines: u64,
    pub unindexed_lines: Vec<UnindexedWindowsJsonlLine>,
    pub entries: Vec<WindowsJsonlTimeIndexEntry>,
}

impl WindowsJsonlTimeIndex {
    pub fn is_complete(&self) -> bool {
        self.unindexed_lines.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceFileFingerprint {
    pub byte_count: u64,
    pub modified_unix_seconds: Option<u64>,
    pub modified_subsec_nanos: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsJsonlTimeIndexEntry {
    pub primary_time: DateTime<Utc>,
    pub line_number: u64,
    pub byte_start: u64,
    pub byte_end: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnindexedWindowsJsonlLine {
    pub line_number: u64,
    pub byte_start: u64,
    pub byte_end: u64,
    pub reason: UnindexedWindowsJsonlLineReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum UnindexedWindowsJsonlLineReason {
    MissingPrimaryTime,
    InvalidUtf8Prefix,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsJsonlRangeLookup {
    pub source_path: PathBuf,
    pub sidecar_path: PathBuf,
    pub time_start: DateTime<Utc>,
    pub time_end: DateTime<Utc>,
    pub source_byte_count: u64,
    pub total_lines: u64,
    pub selected_entry_count: u64,
    pub indexed_entry_count: u64,
    pub ranges: Vec<WindowsJsonlIndexedRange>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsJsonlIndexedRange {
    pub byte_start: u64,
    pub byte_end: u64,
    pub line_start: u64,
    pub line_end: u64,
    pub entry_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsJsonlIndexLookup {
    Indexed(WindowsJsonlRangeLookup),
    Fallback(WindowsJsonlIndexFallback),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WindowsJsonlIndexFallback {
    pub reason: WindowsJsonlIndexFallbackReason,
    pub source_path: PathBuf,
    pub sidecar_path: PathBuf,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum WindowsJsonlIndexFallbackReason {
    UnsupportedSourceFile,
    MissingSidecar,
    CorruptSidecar,
    StaleSidecar,
    IncompleteSidecar,
}

pub fn windows_jsonl_time_index_path(source_path: &Path) -> PathBuf {
    PathBuf::from(format!("{}{}", source_path.display(), INDEX_SUFFIX))
}

pub fn rebuild_windows_jsonl_time_index(
    source_path: &Path,
) -> CaptureCoreResult<WindowsJsonlTimeIndex> {
    validate_windows_jsonl_source(source_path)?;
    let source = source_fingerprint(source_path)?;
    let handle = File::open(source_path)
        .map_err(|error| CaptureCoreError::io(Some(source_path.to_path_buf()), error))?;
    let mut reader = BufReader::new(handle);
    let mut line = Vec::new();
    let mut line_number = 0_u64;
    let mut byte_start = 0_u64;
    let mut index = WindowsJsonlTimeIndex {
        schema_version: INDEX_SCHEMA_VERSION,
        source_path: source_path.to_path_buf(),
        source,
        total_lines: 0,
        indexed_lines: 0,
        empty_lines: 0,
        unindexed_lines: Vec::new(),
        entries: Vec::new(),
    };

    loop {
        line.clear();
        let bytes = reader
            .read_until(b'\n', &mut line)
            .map_err(|error| CaptureCoreError::io(Some(source_path.to_path_buf()), error))?;
        if bytes == 0 {
            break;
        }
        line_number += 1;
        index.total_lines += 1;
        let byte_end = byte_start + bytes as u64;
        let trimmed = trim_jsonl_newline(&line);
        if trimmed.is_empty() {
            index.empty_lines += 1;
        } else if let Some(primary_time) = primary_time_from_json_prefix(trimmed) {
            index.indexed_lines += 1;
            index.entries.push(WindowsJsonlTimeIndexEntry {
                primary_time,
                line_number,
                byte_start,
                byte_end,
            });
        } else {
            index.unindexed_lines.push(UnindexedWindowsJsonlLine {
                line_number,
                byte_start,
                byte_end,
                reason: unindexed_reason(trimmed),
            });
        }
        byte_start = byte_end;
    }

    index.entries.sort_by(|left, right| {
        left.primary_time
            .cmp(&right.primary_time)
            .then_with(|| left.line_number.cmp(&right.line_number))
            .then_with(|| left.byte_start.cmp(&right.byte_start))
    });
    Ok(index)
}

pub fn write_windows_jsonl_time_index(
    source_path: &Path,
) -> CaptureCoreResult<WindowsJsonlTimeIndex> {
    let index = rebuild_windows_jsonl_time_index(source_path)?;
    let sidecar_path = windows_jsonl_time_index_path(source_path);
    write_index_sidecar(&sidecar_path, &index)?;
    Ok(index)
}

pub fn query_windows_jsonl_time_index(
    source_path: &Path,
    time_start: DateTime<Utc>,
    time_end: DateTime<Utc>,
) -> CaptureCoreResult<WindowsJsonlIndexLookup> {
    if time_start > time_end {
        return Err(CaptureCoreError::InvalidTimeRange(format!(
            "time_start {time_start} is after time_end {time_end}"
        )));
    }
    let sidecar_path = windows_jsonl_time_index_path(source_path);
    if !is_windows_jsonl_source(source_path) {
        return Ok(fallback(
            WindowsJsonlIndexFallbackReason::UnsupportedSourceFile,
            source_path,
            &sidecar_path,
            "time indexes are only defined for .windows.jsonl files",
        ));
    }

    let current_fingerprint = source_fingerprint(source_path)?;
    let index = match fs::read(&sidecar_path) {
        Ok(bytes) => match serde_json::from_slice::<WindowsJsonlTimeIndex>(&bytes) {
            Ok(index) => index,
            Err(error) => {
                return Ok(fallback(
                    WindowsJsonlIndexFallbackReason::CorruptSidecar,
                    source_path,
                    &sidecar_path,
                    format!("failed to parse sidecar JSON: {error}"),
                ));
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(fallback(
                WindowsJsonlIndexFallbackReason::MissingSidecar,
                source_path,
                &sidecar_path,
                "no sidecar time index exists for the source file",
            ));
        }
        Err(error) => {
            return Err(CaptureCoreError::io(Some(sidecar_path), error));
        }
    };

    if index.schema_version != INDEX_SCHEMA_VERSION {
        return Ok(fallback(
            WindowsJsonlIndexFallbackReason::CorruptSidecar,
            source_path,
            &sidecar_path,
            format!(
                "unsupported sidecar schema version {}; expected {INDEX_SCHEMA_VERSION}",
                index.schema_version
            ),
        ));
    }
    if index.source != current_fingerprint {
        return Ok(fallback(
            WindowsJsonlIndexFallbackReason::StaleSidecar,
            source_path,
            &sidecar_path,
            "source file size or modification time changed since indexing",
        ));
    }
    if let Some(message) = index_shape_error(&index) {
        return Ok(fallback(
            WindowsJsonlIndexFallbackReason::CorruptSidecar,
            source_path,
            &sidecar_path,
            message,
        ));
    }
    if !index.is_complete() {
        return Ok(fallback(
            WindowsJsonlIndexFallbackReason::IncompleteSidecar,
            source_path,
            &sidecar_path,
            format!(
                "sidecar has {} non-empty lines without indexable primary times",
                index.unindexed_lines.len()
            ),
        ));
    }

    Ok(WindowsJsonlIndexLookup::Indexed(index.query(
        source_path,
        &sidecar_path,
        time_start,
        time_end,
    )))
}

impl WindowsJsonlTimeIndex {
    fn query(
        &self,
        source_path: &Path,
        sidecar_path: &Path,
        time_start: DateTime<Utc>,
        time_end: DateTime<Utc>,
    ) -> WindowsJsonlRangeLookup {
        let start = lower_bound_time(&self.entries, time_start);
        let end = upper_bound_time(&self.entries, time_end);
        let mut selected = self.entries[start..end].to_vec();
        selected.sort_by(|left, right| {
            left.byte_start
                .cmp(&right.byte_start)
                .then_with(|| left.line_number.cmp(&right.line_number))
        });
        let selected_entry_count = selected.len() as u64;
        WindowsJsonlRangeLookup {
            source_path: source_path.to_path_buf(),
            sidecar_path: sidecar_path.to_path_buf(),
            time_start,
            time_end,
            source_byte_count: self.source.byte_count,
            total_lines: self.total_lines,
            selected_entry_count,
            indexed_entry_count: self.entries.len() as u64,
            ranges: coalesce_ranges(&selected),
        }
    }
}

fn index_shape_error(index: &WindowsJsonlTimeIndex) -> Option<String> {
    if index.indexed_lines != index.entries.len() as u64 {
        return Some(format!(
            "indexed_lines {} does not match entry count {}",
            index.indexed_lines,
            index.entries.len()
        ));
    }
    let accounted_lines =
        index.indexed_lines + index.empty_lines + index.unindexed_lines.len() as u64;
    if accounted_lines != index.total_lines {
        return Some(format!(
            "accounted line count {accounted_lines} does not match total_lines {}",
            index.total_lines
        ));
    }
    let mut previous_time = None;
    for entry in &index.entries {
        if entry.byte_start >= entry.byte_end {
            return Some(format!(
                "entry for line {} has invalid byte range {}..{}",
                entry.line_number, entry.byte_start, entry.byte_end
            ));
        }
        if let Some(previous_time) = previous_time {
            if entry.primary_time < previous_time {
                return Some("entries are not sorted by primary_time".to_string());
            }
        }
        previous_time = Some(entry.primary_time);
    }
    None
}

fn write_index_sidecar(
    sidecar_path: &Path,
    index: &WindowsJsonlTimeIndex,
) -> CaptureCoreResult<()> {
    let bytes = serde_json::to_vec(index).map_err(|error| {
        CaptureCoreError::InvalidState(format!("failed to serialize time index: {error}"))
    })?;
    let tmp_path = sidecar_path.with_extension("time-index.json.tmp");
    let mut file = File::create(&tmp_path)
        .map_err(|error| CaptureCoreError::io(Some(tmp_path.clone()), error))?;
    file.write_all(&bytes)
        .map_err(|error| CaptureCoreError::io(Some(tmp_path.clone()), error))?;
    file.write_all(b"\n")
        .map_err(|error| CaptureCoreError::io(Some(tmp_path.clone()), error))?;
    file.sync_all()
        .map_err(|error| CaptureCoreError::io(Some(tmp_path.clone()), error))?;
    fs::rename(&tmp_path, sidecar_path)
        .map_err(|error| CaptureCoreError::io(Some(sidecar_path.to_path_buf()), error))?;
    Ok(())
}

fn coalesce_ranges(entries: &[WindowsJsonlTimeIndexEntry]) -> Vec<WindowsJsonlIndexedRange> {
    let mut ranges: Vec<WindowsJsonlIndexedRange> = Vec::new();
    for entry in entries {
        match ranges.last_mut() {
            Some(range)
                if range.byte_end == entry.byte_start
                    && range.line_end + 1 == entry.line_number =>
            {
                range.byte_end = entry.byte_end;
                range.line_end = entry.line_number;
                range.entry_count += 1;
            }
            _ => ranges.push(WindowsJsonlIndexedRange {
                byte_start: entry.byte_start,
                byte_end: entry.byte_end,
                line_start: entry.line_number,
                line_end: entry.line_number,
                entry_count: 1,
            }),
        }
    }
    ranges
}

fn lower_bound_time(entries: &[WindowsJsonlTimeIndexEntry], time: DateTime<Utc>) -> usize {
    let mut left = 0;
    let mut right = entries.len();
    while left < right {
        let mid = left + (right - left) / 2;
        if entries[mid].primary_time < time {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    left
}

fn upper_bound_time(entries: &[WindowsJsonlTimeIndexEntry], time: DateTime<Utc>) -> usize {
    let mut left = 0;
    let mut right = entries.len();
    while left < right {
        let mid = left + (right - left) / 2;
        if entries[mid].primary_time <= time {
            left = mid + 1;
        } else {
            right = mid;
        }
    }
    left
}

fn fallback(
    reason: WindowsJsonlIndexFallbackReason,
    source_path: &Path,
    sidecar_path: &Path,
    message: impl Into<String>,
) -> WindowsJsonlIndexLookup {
    WindowsJsonlIndexLookup::Fallback(WindowsJsonlIndexFallback {
        reason,
        source_path: source_path.to_path_buf(),
        sidecar_path: sidecar_path.to_path_buf(),
        message: message.into(),
    })
}

fn validate_windows_jsonl_source(source_path: &Path) -> CaptureCoreResult<()> {
    if is_windows_jsonl_source(source_path) {
        Ok(())
    } else {
        Err(CaptureCoreError::InvalidPath {
            path: source_path.display().to_string(),
            message: "time indexes are only defined for .windows.jsonl files".to_string(),
        })
    }
}

fn is_windows_jsonl_source(source_path: &Path) -> bool {
    source_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.ends_with(WINDOWS_JSONL_SUFFIX))
}

fn source_fingerprint(source_path: &Path) -> CaptureCoreResult<SourceFileFingerprint> {
    let metadata = fs::metadata(source_path)
        .map_err(|error| CaptureCoreError::io(Some(source_path.to_path_buf()), error))?;
    Ok(SourceFileFingerprint {
        byte_count: metadata.len(),
        modified_unix_seconds: metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_secs()),
        modified_subsec_nanos: metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.subsec_nanos()),
    })
}

fn unindexed_reason(line: &[u8]) -> UnindexedWindowsJsonlLineReason {
    let prefix_len = line.len().min(SPOOL_ENVELOPE_PREFIX_BYTES);
    if std::str::from_utf8(&line[..prefix_len]).is_ok() {
        UnindexedWindowsJsonlLineReason::MissingPrimaryTime
    } else {
        UnindexedWindowsJsonlLineReason::InvalidUtf8Prefix
    }
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

fn trim_jsonl_newline(line: &[u8]) -> &[u8] {
    let without_lf = line.strip_suffix(b"\n").unwrap_or(line);
    without_lf.strip_suffix(b"\r").unwrap_or(without_lf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, TimeZone};
    use serde_json::json;
    use std::io::{Read, Seek, SeekFrom};
    use std::time::Instant;
    use tempfile::TempDir;

    #[test]
    fn maps_time_window_to_coalesced_byte_and_line_ranges() {
        let fixture = window_file_fixture(6, 0);
        write_windows_jsonl_time_index(&fixture.file).unwrap();

        let lookup = query_windows_jsonl_time_index(
            &fixture.file,
            fixture.base + Duration::seconds(2),
            fixture.base + Duration::seconds(4),
        )
        .unwrap();

        let WindowsJsonlIndexLookup::Indexed(indexed) = lookup else {
            panic!("expected indexed lookup");
        };
        assert_eq!(indexed.selected_entry_count, 3);
        assert_eq!(indexed.indexed_entry_count, 6);
        assert_eq!(
            indexed.ranges,
            vec![WindowsJsonlIndexedRange {
                byte_start: fixture.offsets[2].0,
                byte_end: fixture.offsets[4].1,
                line_start: 3,
                line_end: 5,
                entry_count: 3,
            }]
        );
        assert_eq!(
            read_range(&fixture.file, &indexed.ranges[0]),
            fixture.lines[2..=4].join("\n") + "\n"
        );
    }

    #[test]
    fn query_reports_missing_sidecar_fallback_without_building_one() {
        let fixture = window_file_fixture(2, 0);

        let lookup =
            query_windows_jsonl_time_index(&fixture.file, fixture.base, fixture.base).unwrap();

        assert_fallback_reason(lookup, WindowsJsonlIndexFallbackReason::MissingSidecar);
        assert!(!windows_jsonl_time_index_path(&fixture.file).exists());
    }

    #[test]
    fn query_reports_stale_sidecar_when_source_changes() {
        let fixture = window_file_fixture(2, 0);
        write_windows_jsonl_time_index(&fixture.file).unwrap();
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&fixture.file)
            .unwrap();
        writeln!(
            file,
            "{}",
            envelope_line(fixture.base + Duration::seconds(10), 0)
        )
        .unwrap();

        let lookup =
            query_windows_jsonl_time_index(&fixture.file, fixture.base, fixture.base).unwrap();

        assert_fallback_reason(lookup, WindowsJsonlIndexFallbackReason::StaleSidecar);
    }

    #[test]
    fn query_reports_corrupt_sidecar_without_touching_source_ranges() {
        let fixture = window_file_fixture(2, 0);
        fs::write(windows_jsonl_time_index_path(&fixture.file), "{not json").unwrap();

        let lookup =
            query_windows_jsonl_time_index(&fixture.file, fixture.base, fixture.base).unwrap();

        assert_fallback_reason(lookup, WindowsJsonlIndexFallbackReason::CorruptSidecar);
    }

    #[test]
    fn query_reports_incomplete_sidecar_when_lines_could_not_be_indexed() {
        let fixture = window_file_fixture(2, 0);
        fs::write(
            &fixture.file,
            format!(
                "{}\n{{\"eventType\":\"capture.window_snapshot\",\"payload\":{{}}}}\n",
                fixture.lines[0]
            ),
        )
        .unwrap();
        let index = write_windows_jsonl_time_index(&fixture.file).unwrap();
        assert_eq!(index.indexed_lines, 1);
        assert_eq!(index.unindexed_lines.len(), 1);

        let lookup =
            query_windows_jsonl_time_index(&fixture.file, fixture.base, fixture.base).unwrap();

        assert_fallback_reason(lookup, WindowsJsonlIndexFallbackReason::IncompleteSidecar);
    }

    #[test]
    fn indexed_lookup_uses_sidecar_without_source_scan_benchmark_evidence() {
        let fixture = window_file_fixture(20_000, 1024);
        let build_started = Instant::now();
        let index = write_windows_jsonl_time_index(&fixture.file).unwrap();
        let build_elapsed = build_started.elapsed();

        let query_start = fixture.base + Duration::seconds(19_990);
        let query_end = query_start + Duration::seconds(2);
        let lookup_started = Instant::now();
        let lookup = query_windows_jsonl_time_index(&fixture.file, query_start, query_end).unwrap();
        let lookup_elapsed = lookup_started.elapsed();

        let WindowsJsonlIndexLookup::Indexed(indexed) = lookup else {
            panic!("expected indexed lookup");
        };
        assert_eq!(index.total_lines, 20_000);
        assert_eq!(indexed.selected_entry_count, 3);
        assert_eq!(indexed.ranges.len(), 1);
        eprintln!(
            "spool_index_benchmark records={} source_bytes={} sidecar_bytes={} build_ms={} lookup_us={} selected={} ranges={}",
            index.total_lines,
            index.source.byte_count,
            fs::metadata(windows_jsonl_time_index_path(&fixture.file))
                .unwrap()
                .len(),
            build_elapsed.as_millis(),
            lookup_elapsed.as_micros(),
            indexed.selected_entry_count,
            indexed.ranges.len()
        );
    }

    fn assert_fallback_reason(
        lookup: WindowsJsonlIndexLookup,
        expected: WindowsJsonlIndexFallbackReason,
    ) {
        let WindowsJsonlIndexLookup::Fallback(fallback) = lookup else {
            panic!("expected fallback");
        };
        assert_eq!(fallback.reason, expected);
    }

    struct WindowFileFixture {
        _temp_dir: TempDir,
        file: PathBuf,
        base: DateTime<Utc>,
        lines: Vec<String>,
        offsets: Vec<(u64, u64)>,
    }

    fn window_file_fixture(record_count: usize, payload_bytes: usize) -> WindowFileFixture {
        let temp_dir = tempfile::tempdir().unwrap();
        let windows_dir = temp_dir.path().join("capture/windows");
        fs::create_dir_all(&windows_dir).unwrap();
        let file = windows_dir.join("2026-05-25.windows.jsonl");
        let base = Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap();
        let mut bytes = Vec::new();
        let mut lines = Vec::new();
        let mut offsets = Vec::new();
        for index in 0..record_count {
            let line = envelope_line(base + Duration::seconds(index as i64), payload_bytes);
            let byte_start = bytes.len() as u64;
            bytes.extend_from_slice(line.as_bytes());
            bytes.push(b'\n');
            let byte_end = bytes.len() as u64;
            lines.push(line);
            offsets.push((byte_start, byte_end));
        }
        fs::write(&file, bytes).unwrap();
        WindowFileFixture {
            _temp_dir: temp_dir,
            file,
            base,
            lines,
            offsets,
        }
    }

    fn envelope_line(time: DateTime<Utc>, payload_bytes: usize) -> String {
        serde_json::to_string(&json!({
            "schemaVersion": 1,
            "eventType": "capture.window_snapshot",
            "recordedAt": time.to_rfc3339(),
            "eventTimeStart": time.to_rfc3339(),
            "payload": {
                "windows": [{
                    "windowID": time.timestamp(),
                    "title": "x".repeat(payload_bytes)
                }],
                "displays": []
            }
        }))
        .unwrap()
    }

    fn read_range(path: &Path, range: &WindowsJsonlIndexedRange) -> String {
        let mut file = File::open(path).unwrap();
        file.seek(SeekFrom::Start(range.byte_start)).unwrap();
        let mut bytes = vec![0; (range.byte_end - range.byte_start) as usize];
        file.read_exact(&mut bytes).unwrap();
        String::from_utf8(bytes).unwrap()
    }
}
