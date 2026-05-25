use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvelopeValidationError {
    MissingField(&'static str),
    InvalidTimestamp { field: &'static str, value: String },
    EmptyTimeRange,
    ReversedTimeRange,
    PayloadMustBeObject,
    BlobMissingField(&'static str),
    BlobNegativeByteCount,
    InvalidConfidence,
}

impl std::fmt::Display for EnvelopeValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => write!(formatter, "missing required field {field}"),
            Self::InvalidTimestamp { field, value } => {
                write!(formatter, "invalid RFC3339 timestamp for {field}: {value}")
            }
            Self::EmptyTimeRange => write!(formatter, "event_end must be after event_start"),
            Self::ReversedTimeRange => write!(formatter, "event_end is before event_start"),
            Self::PayloadMustBeObject => write!(formatter, "payload must be a JSON object"),
            Self::BlobMissingField(field) => {
                write!(formatter, "blob is missing required field {field}")
            }
            Self::BlobNegativeByteCount => {
                write!(formatter, "blob byte_count must not be negative")
            }
            Self::InvalidConfidence => write!(formatter, "confidence must be between 0 and 1"),
        }
    }
}

impl std::error::Error for EnvelopeValidationError {}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BlobEnvelope {
    pub uri: String,
    pub content_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CaptureEnvelope {
    pub user_id: String,
    pub stream_id: String,
    pub lane_id: String,
    pub kind: String,

    pub event_start: String,
    pub event_end: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_bundle_id: Option<String>,

    #[serde(default = "empty_payload")]
    pub payload: Value,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobEnvelope>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_clock_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_start_ns: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_end_ns: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_sequence: Option<i64>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<i32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub privacy_class: Option<String>,
}

impl CaptureEnvelope {
    pub fn validate(&self) -> Result<(), EnvelopeValidationError> {
        require_non_empty("user_id", &self.user_id)?;
        require_non_empty("stream_id", &self.stream_id)?;
        require_non_empty("lane_id", &self.lane_id)?;
        require_non_empty("kind", &self.kind)?;
        require_non_empty("event_start", &self.event_start)?;
        require_non_empty("event_end", &self.event_end)?;

        let event_start = parse_timestamp("event_start", &self.event_start)?;
        let event_end = parse_timestamp("event_end", &self.event_end)?;
        if event_end < event_start {
            return Err(EnvelopeValidationError::ReversedTimeRange);
        }
        if event_end == event_start {
            return Err(EnvelopeValidationError::EmptyTimeRange);
        }

        if !self.payload.is_object() {
            return Err(EnvelopeValidationError::PayloadMustBeObject);
        }

        if let Some(blob) = &self.blob {
            blob.validate()?;
        }

        if let Some(confidence) = self.confidence {
            if !(0.0..=1.0).contains(&confidence) {
                return Err(EnvelopeValidationError::InvalidConfidence);
            }
        }

        Ok(())
    }
}

impl BlobEnvelope {
    pub fn validate(&self) -> Result<(), EnvelopeValidationError> {
        if self.uri.trim().is_empty() {
            return Err(EnvelopeValidationError::BlobMissingField("uri"));
        }
        if self.content_type.trim().is_empty() {
            return Err(EnvelopeValidationError::BlobMissingField("content_type"));
        }
        if self.byte_count.is_some_and(|value| value < 0) {
            return Err(EnvelopeValidationError::BlobNegativeByteCount);
        }
        Ok(())
    }
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), EnvelopeValidationError> {
    if value.trim().is_empty() {
        Err(EnvelopeValidationError::MissingField(field))
    } else {
        Ok(())
    }
}

fn parse_timestamp(
    field: &'static str,
    value: &str,
) -> Result<DateTime<Utc>, EnvelopeValidationError> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.with_timezone(&Utc))
        .map_err(|_| EnvelopeValidationError::InvalidTimestamp {
            field,
            value: value.to_string(),
        })
}

fn empty_payload() -> Value {
    Value::Object(Default::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_envelope() -> CaptureEnvelope {
        CaptureEnvelope {
            user_id: "00000000-0000-0000-0000-000000000001".to_string(),
            stream_id: "00000000-0000-0000-0000-000000000002".to_string(),
            lane_id: "00000000-0000-0000-0000-000000000003".to_string(),
            kind: "codex_message".to_string(),
            event_start: "2026-05-21T10:03:25Z".to_string(),
            event_end: "2026-05-21T10:03:25.000001Z".to_string(),
            capture_bundle_id: None,
            payload: json!({"session_id":"abc123","role":"assistant"}),
            blob: None,
            source_clock_id: None,
            source_start_ns: None,
            source_end_ns: None,
            source_sequence: None,
            display_title: Some("Codex message".to_string()),
            display_text: Some("Ship the memory DB.".to_string()),
            schema_name: Some("codex_message".to_string()),
            schema_version: Some(1),
            confidence: Some(1.0),
            privacy_class: Some("normal".to_string()),
        }
    }

    #[test]
    fn valid_capture_envelope_passes() {
        valid_envelope().validate().unwrap();
    }

    #[test]
    fn equal_timestamps_are_rejected_before_insert() {
        let mut envelope = valid_envelope();
        envelope.event_end = envelope.event_start.clone();
        assert_eq!(
            envelope.validate(),
            Err(EnvelopeValidationError::EmptyTimeRange)
        );
    }

    #[test]
    fn payload_must_be_object() {
        let mut envelope = valid_envelope();
        envelope.payload = json!(["not", "an", "object"]);
        assert_eq!(
            envelope.validate(),
            Err(EnvelopeValidationError::PayloadMustBeObject)
        );
    }

    #[test]
    fn blob_metadata_is_validated() {
        let mut envelope = valid_envelope();
        envelope.blob = Some(BlobEnvelope {
            uri: "s3://bucket/frame.png".to_string(),
            content_type: "image/png".to_string(),
            sha256: None,
            byte_count: Some(-1),
        });
        assert_eq!(
            envelope.validate(),
            Err(EnvelopeValidationError::BlobNegativeByteCount)
        );
    }
}
