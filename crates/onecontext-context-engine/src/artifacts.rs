//! Agent artifact handling.
//!
//! Normalizes final messages, page-change proposals, and proof artifacts before
//! curator promotion and wiki publication.

use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactHandle {
    pub artifact_id: String,
    pub kind: String,
    pub run_id: String,
    pub key: String,
    pub path: String,
    pub content_sha256: String,
    pub created_at: String,
}

impl ArtifactHandle {
    pub fn availability_keys(&self) -> Vec<String> {
        let mut keys = BTreeSet::new();
        keys.insert(self.kind.clone());
        keys.insert(format!("kind:{}", self.kind));
        keys.insert(format!("artifact:{}", self.artifact_id));
        keys.insert(format!("{}:artifact:{}", self.kind, self.artifact_id));
        keys.insert(format!(
            "{}:key:{}",
            self.kind,
            safe_artifact_part(&self.key, 120)
        ));
        keys.insert(format!("{}:sha256:{}", self.kind, self.content_sha256));
        keys.insert(format!("run:{}:{}", self.run_id, self.kind));
        keys.into_iter().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ArtifactRecord {
    pub schema_version: u32,
    pub kind: String,
    pub artifact_id: String,
    pub run_id: String,
    pub key: String,
    pub created_at: String,
    pub content_sha256: String,
    pub metadata: BTreeMap<String, String>,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ContentAddressedArtifactObject {
    pub schema_version: u32,
    pub kind: String,
    pub content_sha256: String,
    pub created_at: String,
    pub value: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArtifactStore {
    root: PathBuf,
}

impl ArtifactStore {
    pub fn from_paths(paths: &ContextEnginePaths) -> Self {
        Self {
            root: paths.runs.clone(),
        }
    }

    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn write_json(
        &self,
        run_id: &str,
        kind: &str,
        key: &str,
        value: Value,
        metadata: BTreeMap<String, String>,
    ) -> Result<ArtifactHandle, String> {
        let run_id = safe_run_id(run_id);
        let artifact_id = artifact_id(kind, key);
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let content_sha256 = content_sha256(&value)?;
        let record = ArtifactRecord {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            kind: kind.to_string(),
            artifact_id: artifact_id.clone(),
            run_id: run_id.clone(),
            key: key.to_string(),
            created_at: created_at.clone(),
            content_sha256: content_sha256.clone(),
            metadata,
            value,
        };
        let bytes = serde_json::to_vec_pretty(&record)
            .map_err(|error| format!("failed to encode artifact {artifact_id}: {error}"))?;
        let path = self.artifact_path(&run_id, kind, key);
        if path.is_file() {
            let existing = self.read_record_path(&path)?;
            if existing.schema_version == record.schema_version
                && existing.kind == record.kind
                && existing.artifact_id == record.artifact_id
                && existing.run_id == record.run_id
                && existing.key == record.key
                && existing.content_sha256 == record.content_sha256
                && existing.metadata == record.metadata
                && existing.value == record.value
            {
                return Ok(ArtifactHandle {
                    artifact_id: existing.artifact_id,
                    kind: existing.kind,
                    run_id: existing.run_id,
                    key: existing.key,
                    path: path.display().to_string(),
                    content_sha256: existing.content_sha256,
                    created_at: existing.created_at,
                });
            }
            return Err(format!(
                "artifact immutability violation for {}; use a new key or turn attempt instead of mutating prior evidence",
                path.display()
            ));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        fs::write(&path, bytes)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        Ok(ArtifactHandle {
            artifact_id,
            kind: kind.to_string(),
            run_id,
            key: key.to_string(),
            path: path.display().to_string(),
            content_sha256,
            created_at,
        })
    }

    pub fn write_content_addressed_json(
        &self,
        run_id: &str,
        value: Value,
    ) -> Result<(PathBuf, String), String> {
        let content_sha256 = content_sha256(&value)?;
        let path = self.content_addressed_json_path(&safe_run_id(run_id), &content_sha256);
        if path.is_file() {
            let text = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
            let existing: ContentAddressedArtifactObject = serde_json::from_str(&text)
                .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
            if existing.content_sha256 == content_sha256 && existing.value == value {
                return Ok((path, content_sha256));
            }
            return Err(format!(
                "content-addressed artifact collision at {}",
                path.display()
            ));
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
        }
        let object = ContentAddressedArtifactObject {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            kind: "onecontext.context_engine.content_addressed_artifact.v1".to_string(),
            content_sha256: content_sha256.clone(),
            created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
            value,
        };
        let bytes = serde_json::to_vec_pretty(&object).map_err(|error| {
            format!("failed to encode content-addressed artifact {content_sha256}: {error}")
        })?;
        fs::write(&path, bytes)
            .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
        Ok((path, content_sha256))
    }

    pub fn read_json(&self, handle: &ArtifactHandle) -> Result<ArtifactRecord, String> {
        let record = self.read_record_path(Path::new(&handle.path))?;
        if handle.content_sha256 != record.content_sha256 {
            return Err(format!(
                "artifact handle hash mismatch for {}: handle={}, record={}",
                handle.path, handle.content_sha256, record.content_sha256
            ));
        }
        Ok(record)
    }

    pub fn handle_for_key(
        &self,
        run_id: &str,
        kind: &str,
        key: &str,
    ) -> Result<Option<ArtifactHandle>, String> {
        let path = self.artifact_path(&safe_run_id(run_id), kind, key);
        if !path.is_file() {
            return Ok(None);
        }
        self.handle_from_path(&path).map(Some)
    }

    pub fn read_kind_key(
        &self,
        run_id: &str,
        kind: &str,
        key: &str,
    ) -> Result<Option<ArtifactRecord>, String> {
        let Some(handle) = self.handle_for_key(run_id, kind, key)? else {
            return Ok(None);
        };
        self.read_json(&handle).map(Some)
    }

    pub fn list_kind(&self, run_id: &str, kind: &str) -> Result<Vec<ArtifactHandle>, String> {
        let dir = self.kind_dir(&safe_run_id(run_id), kind);
        let Ok(entries) = fs::read_dir(&dir) else {
            return Ok(Vec::new());
        };
        let mut handles = Vec::new();
        for entry in entries {
            let entry =
                entry.map_err(|error| format!("failed to read {}: {error}", dir.display()))?;
            let path = entry.path();
            if !path.is_file()
                || path.extension().and_then(|extension| extension.to_str()) != Some("json")
            {
                continue;
            }
            handles.push(self.handle_from_path(&path)?);
        }
        handles.sort_by(|left, right| left.artifact_id.cmp(&right.artifact_id));
        Ok(handles)
    }

    pub fn list_kind_for_day(
        &self,
        run_id: &str,
        kind: &str,
        date: &str,
    ) -> Result<Vec<ArtifactHandle>, String> {
        self.list_kind_matching_metadata(run_id, kind, &[("date", date)])
    }

    pub fn list_kind_for_hour(
        &self,
        run_id: &str,
        kind: &str,
        date: &str,
        hour: &str,
    ) -> Result<Vec<ArtifactHandle>, String> {
        self.list_kind_matching_metadata(run_id, kind, &[("date", date), ("hour", hour)])
    }

    pub fn list_kind_matching_metadata(
        &self,
        run_id: &str,
        kind: &str,
        filters: &[(&str, &str)],
    ) -> Result<Vec<ArtifactHandle>, String> {
        let mut handles = Vec::new();
        for handle in self.list_kind(run_id, kind)? {
            let record = self.read_json(&handle)?;
            if filters.iter().all(|(key, value)| {
                record
                    .metadata
                    .get(*key)
                    .map(|metadata_value| metadata_value.as_str())
                    == Some(*value)
            }) {
                handles.push(handle);
            }
        }
        Ok(handles)
    }

    fn kind_dir(&self, run_id: &str, kind: &str) -> PathBuf {
        self.root
            .join(run_id)
            .join("artifacts")
            .join(safe_artifact_part(kind, 80))
    }

    fn artifact_path(&self, run_id: &str, kind: &str, key: &str) -> PathBuf {
        self.kind_dir(run_id, kind)
            .join(format!("{}.json", artifact_id(kind, key)))
    }

    fn content_addressed_json_path(&self, run_id: &str, content_sha256: &str) -> PathBuf {
        let prefix = content_sha256.chars().take(2).collect::<String>();
        self.root
            .join(run_id)
            .join("artifacts")
            .join("_content")
            .join("sha256")
            .join(prefix)
            .join(format!("{content_sha256}.json"))
    }

    fn handle_from_path(&self, path: &Path) -> Result<ArtifactHandle, String> {
        let record = self.read_record_path(path)?;
        Ok(ArtifactHandle {
            artifact_id: record.artifact_id,
            kind: record.kind,
            run_id: record.run_id,
            key: record.key,
            path: path.display().to_string(),
            content_sha256: record.content_sha256,
            created_at: record.created_at,
        })
    }

    fn read_record_path(&self, path: &Path) -> Result<ArtifactRecord, String> {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
        let record: ArtifactRecord = serde_json::from_str(&text)
            .map_err(|error| format!("failed to parse {}: {error}", path.display()))?;
        let expected = content_sha256(&record.value)?;
        if record.content_sha256 != expected {
            return Err(format!(
                "artifact content hash mismatch for {}: expected={}, actual={}",
                path.display(),
                expected,
                record.content_sha256
            ));
        }
        Ok(record)
    }
}

pub fn artifact_id(kind: &str, key: &str) -> String {
    let digest = short_sha256_hex(format!("{kind}\0{key}").as_bytes());
    format!(
        "{}-{}-{}",
        safe_artifact_part(kind, 48),
        safe_artifact_part(key, 72),
        digest
    )
}

fn safe_artifact_part(value: &str, max_chars: usize) -> String {
    safe_run_id(value).chars().take(max_chars).collect()
}

fn content_sha256(value: &Value) -> Result<String, String> {
    let bytes = serde_json::to_vec(value)
        .map_err(|error| format!("failed to encode artifact value: {error}"))?;
    Ok(sha256_hex(&bytes))
}

fn short_sha256_hex(bytes: &[u8]) -> String {
    let full = sha256_hex(bytes);
    full.chars().take(16).collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
