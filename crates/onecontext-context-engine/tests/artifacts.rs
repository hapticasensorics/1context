use onecontext_context_engine::artifacts::{artifact_id, ArtifactRecord, ArtifactStore};
use onecontext_context_engine::safe_run_id;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;

#[test]
fn artifact_ids_paths_and_content_hashes_are_stable() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(temp.path().join("artifacts"));
    let value = json!({
        "summary": "scribe report",
        "claims": ["typed artifact handoff"]
    });
    let metadata = metadata([("date", "2026-06-07"), ("hour", "09"), ("role", "scribe")]);

    let first = store
        .write_json(
            "run/with spaces",
            "scribe_artifacts",
            "packet/2026-06-07 09",
            value.clone(),
            metadata.clone(),
        )
        .expect("write artifact");
    let second = store
        .write_json(
            "run/with spaces",
            "scribe_artifacts",
            "packet/2026-06-07 09",
            value.clone(),
            metadata,
        )
        .expect("rewrite same artifact");

    assert_eq!(
        first.artifact_id,
        artifact_id("scribe_artifacts", "packet/2026-06-07 09")
    );
    assert_eq!(first.artifact_id, second.artifact_id);
    assert_eq!(first.path, second.path);
    assert_eq!(first.content_sha256, second.content_sha256);
    assert_eq!(first.content_sha256, sha256_json(&value));
    let availability_keys = first.availability_keys();
    assert!(availability_keys.contains(&"scribe_artifacts".to_string()));
    assert!(availability_keys.contains(&format!(
        "scribe_artifacts:key:{}",
        safe_run_id("packet/2026-06-07 09")
    )));
    assert!(availability_keys.contains(&format!("scribe_artifacts:artifact:{}", first.artifact_id)));
    assert!(
        availability_keys.contains(&format!("scribe_artifacts:sha256:{}", first.content_sha256))
    );
    assert!(first.path.contains(&format!(
        "{}/artifacts/scribe_artifacts",
        safe_run_id("run/with spaces")
    )));
    assert!(fs::metadata(&first.path).expect("artifact file").is_file());

    let by_key = store
        .handle_for_key(
            "run/with spaces",
            "scribe_artifacts",
            "packet/2026-06-07 09",
        )
        .expect("lookup by kind/key")
        .expect("artifact handle");
    assert_eq!(by_key.artifact_id, first.artifact_id);

    let record = store.read_json(&by_key).expect("read artifact");
    assert_eq!(record.value, value);
    assert_eq!(record.metadata["date"], "2026-06-07");
}

#[test]
fn artifact_store_lists_by_kind_key_day_and_hour() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(temp.path().join("artifacts"));

    let p1 = write_report(
        &store,
        "wiki-run",
        "scribe_artifacts",
        "packet-a",
        "2026-06-07",
        "09",
    );
    let p2 = write_report(
        &store,
        "wiki-run",
        "scribe_artifacts",
        "packet-b",
        "2026-06-07",
        "10",
    );
    let p3 = write_report(
        &store,
        "wiki-run",
        "scribe_artifacts",
        "packet-c",
        "2026-06-08",
        "09",
    );
    let editor = write_report(
        &store,
        "wiki-run",
        "daily_editorial_artifacts",
        "editor-a",
        "2026-06-07",
        "00",
    );

    let all_scribe = store
        .list_kind("wiki-run", "scribe_artifacts")
        .expect("list scribe artifacts");
    assert_eq!(all_scribe.len(), 3);
    assert!(all_scribe
        .iter()
        .all(|handle| handle.kind == "scribe_artifacts"));
    assert!(!all_scribe
        .iter()
        .any(|handle| handle.artifact_id == editor.artifact_id));

    let by_key = store
        .read_kind_key("wiki-run", "scribe_artifacts", "packet-b")
        .expect("read by kind/key")
        .expect("record");
    assert_eq!(by_key.artifact_id, p2.artifact_id);

    let day = store
        .list_kind_for_day("wiki-run", "scribe_artifacts", "2026-06-07")
        .expect("list by day");
    assert_eq!(ids(day), ids(vec![p1.clone(), p2.clone()]));

    let hour = store
        .list_kind_for_hour("wiki-run", "scribe_artifacts", "2026-06-07", "09")
        .expect("list by hour");
    assert_eq!(ids(hour), ids(vec![p1]));

    let missing = store
        .list_kind_for_hour("wiki-run", "scribe_artifacts", "2026-06-09", "09")
        .expect("missing hour");
    assert!(missing.is_empty());
    assert_ne!(p2.artifact_id, p3.artifact_id);
}

#[test]
fn artifact_reads_reject_content_hash_mismatches() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(temp.path().join("artifacts"));
    let handle = write_report(
        &store,
        "wiki-run",
        "scribe_artifacts",
        "packet-a",
        "2026-06-07",
        "09",
    );

    let text = fs::read_to_string(&handle.path).expect("read artifact file");
    let mut record: ArtifactRecord = serde_json::from_str(&text).expect("parse artifact");
    record.value = json!({ "body": "tampered" });
    fs::write(
        &handle.path,
        serde_json::to_vec_pretty(&record).expect("encode tampered artifact"),
    )
    .expect("write tampered artifact");

    let error = store
        .read_json(&handle)
        .expect_err("tampered artifact should fail hash validation");
    assert!(error.contains("artifact content hash mismatch"));
}

#[test]
fn artifact_store_rejects_mutating_existing_key() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(temp.path().join("artifacts"));
    let metadata = metadata([("date", "2026-06-07"), ("hour", "09")]);

    let first = store
        .write_json(
            "wiki-run",
            "scribe_artifacts",
            "packet-a",
            json!({ "body": "original" }),
            metadata.clone(),
        )
        .expect("write first artifact");
    let same = store
        .write_json(
            "wiki-run",
            "scribe_artifacts",
            "packet-a",
            json!({ "body": "original" }),
            metadata.clone(),
        )
        .expect("same artifact is idempotent");
    assert_eq!(same.artifact_id, first.artifact_id);
    assert_eq!(same.created_at, first.created_at);

    let error = store
        .write_json(
            "wiki-run",
            "scribe_artifacts",
            "packet-a",
            json!({ "body": "changed" }),
            metadata,
        )
        .expect_err("changed artifact must use a new key");
    assert!(error.contains("artifact immutability violation"));
}

#[test]
fn artifact_store_can_write_content_addressed_objects() {
    let temp = tempfile::tempdir().expect("tempdir");
    let store = ArtifactStore::new(temp.path().join("artifacts"));
    let value = json!({
        "kind": "final_message",
        "body": "status: completed\n"
    });

    let (first_path, first_hash) = store
        .write_content_addressed_json("content/run", value.clone())
        .expect("write content-addressed artifact");
    let (second_path, second_hash) = store
        .write_content_addressed_json("content/run", value.clone())
        .expect("same content is idempotent");

    assert_eq!(first_hash, sha256_json(&value));
    assert_eq!(first_hash, second_hash);
    assert_eq!(first_path, second_path);
    assert!(first_path.display().to_string().contains(&format!(
        "{}/artifacts/_content/sha256",
        safe_run_id("content/run")
    )));
    assert!(first_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == format!("{first_hash}.json")));
}

fn write_report(
    store: &ArtifactStore,
    run_id: &str,
    kind: &str,
    key: &str,
    date: &str,
    hour: &str,
) -> onecontext_context_engine::artifacts::ArtifactHandle {
    store
        .write_json(
            run_id,
            kind,
            key,
            json!({ "body": key }),
            metadata([("date", date), ("hour", hour)]),
        )
        .expect("write report")
}

fn metadata<const N: usize>(entries: [(&str, &str); N]) -> BTreeMap<String, String> {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

fn ids(handles: Vec<onecontext_context_engine::artifacts::ArtifactHandle>) -> Vec<String> {
    handles
        .into_iter()
        .map(|handle| handle.artifact_id)
        .collect()
}

fn sha256_json(value: &Value) -> String {
    let mut hasher = Sha256::new();
    hasher.update(serde_json::to_vec(value).expect("json bytes"));
    format!("{:x}", hasher.finalize())
}
