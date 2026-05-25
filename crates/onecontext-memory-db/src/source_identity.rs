//! Deterministic source identity helpers for Perception DB writes.

use std::collections::{BTreeMap, HashMap};

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use uuid::Uuid;

pub const SOURCE_RECORD_NAMESPACE_LABEL: &str = "source-record";
pub const OBJECT_NAMESPACE_LABEL: &str = "object";

pub fn source_record_id(source_id: Uuid, source_record_key: &str) -> Uuid {
    deterministic_uuid(SOURCE_RECORD_NAMESPACE_LABEL, source_id, source_record_key)
}

pub fn object_id(source_id: Uuid, source_record_key: &str) -> Uuid {
    deterministic_uuid(OBJECT_NAMESPACE_LABEL, source_id, source_record_key)
}

pub fn deterministic_uuid(namespace_label: &str, source_id: Uuid, source_record_key: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(namespace_label.as_bytes());
    hasher.update([0]);
    hasher.update(source_id.as_bytes());
    hasher.update([0]);
    hasher.update(source_record_key.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

pub fn canonical_source_hash(value: &Value) -> String {
    let bytes = canonical_json_bytes(value);
    let digest = Sha256::digest(bytes);
    hex_lower(&digest)
}

pub fn canonical_json_bytes(value: &Value) -> Vec<u8> {
    serde_json::to_vec(&canonicalize_json(value)).expect("canonical JSON serializes")
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIdentityFingerprint {
    pub source_id: Uuid,
    pub source_record_key: String,
    pub source_record_hash: String,
}

impl SourceIdentityFingerprint {
    pub fn new(
        source_id: Uuid,
        source_record_key: impl Into<String>,
        source_record_hash: impl Into<String>,
    ) -> Self {
        Self {
            source_id,
            source_record_key: source_record_key.into(),
            source_record_hash: source_record_hash.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SameBatchDuplicate {
    pub ordinal: usize,
    pub source_id: Uuid,
    pub source_record_key: String,
    pub source_record_hash: String,
    pub source_record_id: Uuid,
    pub object_id: Uuid,
    pub duplicate_of_ordinal: Option<usize>,
    pub dedupe_reason: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceIdentityHashConflict {
    pub source_id: Uuid,
    pub source_record_key: String,
    pub first_ordinal: usize,
    pub conflict_ordinal: usize,
    pub first_hash: String,
    pub conflict_hash: String,
}

impl std::fmt::Display for SourceIdentityHashConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "source identity {}:{} was reused in the same batch with a different canonical hash (first ordinal {}, conflict ordinal {})",
            self.source_id, self.source_record_key, self.first_ordinal, self.conflict_ordinal
        )
    }
}

impl std::error::Error for SourceIdentityHashConflict {}

pub fn detect_same_batch_duplicates(
    records: &[SourceIdentityFingerprint],
) -> Result<Vec<SameBatchDuplicate>, SourceIdentityHashConflict> {
    let mut first_seen = HashMap::<(Uuid, String), (usize, String)>::new();
    let mut decisions = Vec::with_capacity(records.len());

    for (ordinal, record) in records.iter().enumerate() {
        let key = (record.source_id, record.source_record_key.clone());
        let source_record_id = source_record_id(record.source_id, &record.source_record_key);
        let object_id = object_id(record.source_id, &record.source_record_key);
        let duplicate = if let Some((first_ordinal, first_hash)) = first_seen.get(&key) {
            if first_hash != &record.source_record_hash {
                return Err(SourceIdentityHashConflict {
                    source_id: record.source_id,
                    source_record_key: record.source_record_key.clone(),
                    first_ordinal: *first_ordinal,
                    conflict_ordinal: ordinal,
                    first_hash: first_hash.clone(),
                    conflict_hash: record.source_record_hash.clone(),
                });
            }
            Some(*first_ordinal)
        } else {
            first_seen.insert(key, (ordinal, record.source_record_hash.clone()));
            None
        };

        decisions.push(SameBatchDuplicate {
            ordinal,
            source_id: record.source_id,
            source_record_key: record.source_record_key.clone(),
            source_record_hash: record.source_record_hash.clone(),
            source_record_id,
            object_id,
            duplicate_of_ordinal: duplicate,
            dedupe_reason: duplicate.map(|_| "same_batch"),
        });
    }

    Ok(decisions)
}

fn canonicalize_json(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(canonicalize_json).collect()),
        Value::Object(map) => {
            let sorted = map
                .iter()
                .map(|(key, value)| (key.clone(), canonicalize_json(value)))
                .collect::<BTreeMap<_, _>>();
            let mut ordered = Map::new();
            for (key, value) in sorted {
                ordered.insert(key, value);
            }
            Value::Object(ordered)
        }
        value => value.clone(),
    }
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn source_identity_uuid_helpers_are_deterministic_and_namespaced() {
        let source_id = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
        let key = "codex/session/abc/message/42";

        assert_eq!(SOURCE_RECORD_NAMESPACE_LABEL, "source-record");
        assert_eq!(OBJECT_NAMESPACE_LABEL, "object");
        assert_eq!(
            source_record_id(source_id, key),
            source_record_id(source_id, key)
        );
        assert_eq!(object_id(source_id, key), object_id(source_id, key));
        assert_ne!(source_record_id(source_id, key), object_id(source_id, key));
        assert_ne!(
            object_id(source_id, key),
            object_id(source_id, "codex/session/abc/message/43")
        );
    }

    #[test]
    fn source_identity_canonical_hash_is_stable_for_json_key_ordering() {
        let left = json!({
            "source_record_key": "message/42",
            "payload": {
                "b": 2,
                "a": [{"z": true, "y": false}]
            }
        });
        let right = json!({
            "payload": {
                "a": [{"y": false, "z": true}],
                "b": 2
            },
            "source_record_key": "message/42"
        });

        assert_eq!(canonical_json_bytes(&left), canonical_json_bytes(&right));
        assert_eq!(canonical_source_hash(&left), canonical_source_hash(&right));
    }

    #[test]
    fn source_identity_same_batch_duplicate_detection_marks_later_records() {
        let source_id = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
        let records = vec![
            SourceIdentityFingerprint::new(source_id, "message/1", "hash-a"),
            SourceIdentityFingerprint::new(source_id, "message/2", "hash-b"),
            SourceIdentityFingerprint::new(source_id, "message/1", "hash-a"),
        ];

        let decisions = detect_same_batch_duplicates(&records).unwrap();

        assert_eq!(decisions[0].duplicate_of_ordinal, None);
        assert_eq!(decisions[2].duplicate_of_ordinal, Some(0));
        assert_eq!(decisions[2].dedupe_reason, Some("same_batch"));
        assert_eq!(decisions[0].object_id, decisions[2].object_id);
    }

    #[test]
    fn source_identity_same_batch_conflict_rejects_different_hashes() {
        let source_id = Uuid::parse_str("10000000-0000-0000-0000-000000000001").unwrap();
        let records = vec![
            SourceIdentityFingerprint::new(source_id, "message/1", "hash-a"),
            SourceIdentityFingerprint::new(source_id, "message/1", "hash-b"),
        ];

        let error = detect_same_batch_duplicates(&records).unwrap_err();

        assert_eq!(error.first_ordinal, 0);
        assert_eq!(error.conflict_ordinal, 1);
        assert_eq!(error.first_hash, "hash-a");
        assert_eq!(error.conflict_hash, "hash-b");
    }
}
