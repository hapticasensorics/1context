use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaptureEventEnvelope {
    #[serde(default, alias = "schemaVersion")]
    pub schema_version: Option<u32>,
    #[serde(default, alias = "eventType")]
    pub event_type: String,
    #[serde(default, alias = "recordedAt")]
    pub recorded_at: Option<DateTime<Utc>>,
    #[serde(default, alias = "eventTimeStart")]
    pub event_time_start: Option<DateTime<Utc>>,
    #[serde(default, alias = "eventTimeEnd")]
    pub event_time_end: Option<DateTime<Utc>>,
    #[serde(default, alias = "laneID")]
    pub lane_id: Option<String>,
    #[serde(default, alias = "streamID")]
    pub stream_id: Option<String>,
    #[serde(default, alias = "sourceRecordID")]
    pub source_record_id: Option<String>,
    #[serde(default, alias = "sourceHash")]
    pub source_hash: Option<String>,
    #[serde(default, alias = "sourceSpanID")]
    pub source_span_id: Option<String>,
    #[serde(default, alias = "captureBundleID")]
    pub capture_bundle_id: Option<String>,
    #[serde(default)]
    pub payload: Value,
}

impl CaptureEventEnvelope {
    pub fn primary_time(&self) -> Option<DateTime<Utc>> {
        self.event_time_start.or(self.recorded_at)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawSpoolRecord {
    pub source_path: PathBuf,
    pub line_number: usize,
    pub byte_offset: u64,
    pub raw_record_hash: String,
    pub raw_json: String,
    pub envelope: CaptureEventEnvelope,
}

impl RawSpoolRecord {
    pub fn provenance(&self) -> RawSpoolProvenance {
        RawSpoolProvenance {
            raw_source_uri: file_uri(&self.source_path),
            raw_line_number: self.line_number,
            raw_byte_offset: self.byte_offset,
            raw_record_hash: self.raw_record_hash.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RawSpoolProvenance {
    pub raw_source_uri: String,
    pub raw_line_number: usize,
    pub raw_byte_offset: u64,
    pub raw_record_hash: String,
}

fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for byte in path.to_string_lossy().as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                uri.push(*byte as char)
            }
            _ => {
                use std::fmt::Write;
                let _ = write!(uri, "%{byte:02X}");
            }
        }
    }
    uri
}
