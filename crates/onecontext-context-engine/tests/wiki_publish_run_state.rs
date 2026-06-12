use onecontext_context_engine::artifacts::ArtifactHandle;
use onecontext_context_engine::run_state::{
    load_or_new_state, save_state, state_path, JobReceiptState, WikiCompanyCursors,
};
use onecontext_context_engine::wiki_publish::{
    draft_from_status, write_pages_and_publish_scaffold, WikiPageDraft, WikiPublishRequest,
    WikiPublishResult,
};
use onecontext_context_engine::{safe_run_id, ContextEnginePaths};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

#[test]
fn run_state_resumes_cursors_without_runs_history_or_duplicate_receipts() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");

    let raw_run_id = "state/run 01";
    let safe_run = safe_run_id(raw_run_id);
    let mut state = load_or_new_state(&paths, raw_run_id).expect("new state");
    assert_eq!(state.run_id, safe_run);
    state.set_raw_ingest_cursor("perception-row-42");
    let saved_path = save_state(&paths, &state).expect("save raw cursor");

    assert_eq!(saved_path, state_path(&paths, raw_run_id));
    assert!(saved_path.is_file());
    let current_text =
        fs::read_to_string(&paths.state_current_run_file).expect("current run pointer");
    let current: serde_json::Value =
        serde_json::from_str(&current_text).expect("decode current pointer");
    assert_eq!(current["current_run_id"], safe_run);
    assert_eq!(current["status"], "active");
    assert_eq!(current["time_basis"], "utc");
    assert!(paths.run_json_path(raw_run_id).is_file());

    let mut resumed = load_or_new_state(&paths, raw_run_id).expect("resume state");
    assert_eq!(
        resumed.cursors.raw_ingest_cursor.as_deref(),
        Some("perception-row-42")
    );
    assert_eq!(resumed.cursors.wiki_memory_cursor, None);

    resumed.update_cursors(WikiCompanyCursors {
        raw_ingest_cursor: None,
        wiki_memory_cursor: Some("packet-2026-06-07T09".to_string()),
    });
    resumed.mark_phase_completed("wake_scribes");
    resumed.mark_phase_completed("wake_scribes");

    let artifact = ArtifactHandle {
        artifact_id: "source-packet-a".to_string(),
        kind: "source_packet".to_string(),
        run_id: resumed.run_id.clone(),
        key: "packet-a".to_string(),
        path: "/tmp/source-packet-a.json".to_string(),
        content_sha256: "abc123".to_string(),
        created_at: "2026-06-07T09:00:00Z".to_string(),
    };
    resumed.add_artifact(artifact.clone());
    resumed.add_artifact(artifact);

    let mut receipt = JobReceiptState {
        unit_id: "unit-a".to_string(),
        phase_id: "wake_scribes".to_string(),
        job_id: "memory.hourly.scribe".to_string(),
        channel: "agent_mail".to_string(),
        authority: "agent_mail".to_string(),
        authoritative: true,
        status: "started".to_string(),
        receipt_paths: Vec::new(),
        audit_paths: Vec::new(),
        external_id: None,
        updated_at: "2026-06-07T09:01:00Z".to_string(),
    };
    resumed.record_job_receipt(receipt.clone());
    receipt.status = "complete".to_string();
    receipt
        .receipt_paths
        .push("mail://delivery/source-packet-a".to_string());
    resumed.record_job_receipt(receipt);

    save_state(&paths, &resumed).expect("save resumed state");
    let loaded = load_or_new_state(&paths, "state/run 01").expect("reload state");

    assert_eq!(
        loaded.cursors.raw_ingest_cursor.as_deref(),
        Some("perception-row-42")
    );
    assert_eq!(
        loaded.cursors.wiki_memory_cursor.as_deref(),
        Some("packet-2026-06-07T09")
    );
    assert_eq!(loaded.completed_phase_ids, vec!["wake_scribes"]);
    assert_eq!(loaded.artifacts.len(), 1);
    assert_eq!(loaded.job_receipts.len(), 1);
    assert_eq!(loaded.job_receipts[0].status, "complete");
    assert_eq!(loaded.job_receipts[0].receipt_paths.len(), 1);
    assert_eq!(
        state_path(&paths, "state/run 01"),
        paths.run_json_path("state/run 01")
    );
}

#[test]
fn run_state_can_record_interrupted_terminal_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");

    let mut state = load_or_new_state(&paths, "interrupted/run").expect("new state");
    state.mark_interrupted("operator stopped live proof run");
    save_state(&paths, &state).expect("save interrupted state");

    let loaded = load_or_new_state(&paths, "interrupted/run").expect("load interrupted state");
    assert_eq!(loaded.status, "interrupted");
    assert_eq!(
        loaded.terminal_reason.as_deref(),
        Some("operator stopped live proof run")
    );
    let current_text =
        fs::read_to_string(&paths.state_current_run_file).expect("current run pointer");
    assert!(current_text.contains("\"status\": \"interrupted\""));
}

#[test]
fn wiki_publish_writes_configured_page_receipts_and_is_idempotent() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");

    let source_path = paths
        .user_wiki
        .join("source/families/for-you/for-you/source/for-you.md");
    fs::create_dir_all(source_path.parent().expect("source parent")).expect("source parent");
    let old_body = "# For You\n\nOld body.\n";
    fs::write(&source_path, old_body).expect("old source");

    let request = WikiPublishRequest {
        run_id: "publish/run 01".to_string(),
        drafts: vec![draft_from_status("for-you", "For You", "Fresh body.")],
    };
    let safe_run = safe_run_id(&request.run_id);
    let first = write_pages_and_publish_scaffold(&paths, request.clone()).expect("publish");

    assert_eq!(first.run_id, safe_run);
    assert_eq!(first.publish_id, format!("wiki-publish-{safe_run}"));
    assert_eq!(first.status, "complete");
    assert_eq!(first.changed_page_count, 1);
    assert_eq!(first.page_receipts.len(), 1);
    assert_eq!(
        first.site_path,
        paths.user_wiki.join("site").display().to_string()
    );
    assert!(PathBuf::from(&first.publish_proof_path).is_file());
    assert!(first.publish_proof_path.ends_with(&format!(
        "context-engine/live/runs/{safe_run}/publish/publish-proof.json"
    )));

    let receipt = &first.page_receipts[0];
    let expected_body = "# For You\n\nFresh body.\n";
    assert_eq!(
        fs::read_to_string(&source_path).expect("read source"),
        expected_body
    );
    assert_eq!(receipt.slug, "for-you");
    assert_eq!(receipt.page_id, "for-you");
    assert_eq!(receipt.source_path, source_path.display().to_string());
    let old_hash = sha256_hex(old_body);
    assert_eq!(
        receipt.old_content_sha256.as_deref(),
        Some(old_hash.as_str())
    );
    assert_eq!(receipt.new_content_sha256, sha256_hex(expected_body));
    assert_eq!(receipt.content_sha256, receipt.new_content_sha256);
    assert!(receipt.changed);
    assert_eq!(receipt.conflict_status, "none");
    assert!(PathBuf::from(&receipt.write_proof_path).is_file());

    let proof_text = fs::read_to_string(&first.publish_proof_path).expect("publish proof");
    let persisted: WikiPublishResult = serde_json::from_str(&proof_text).expect("proof json");
    assert_eq!(persisted.changed_page_count, 1);
    assert_eq!(persisted.page_receipts[0], *receipt);
    assert!(paths.run_publish(&request.run_id).is_dir());

    let second = write_pages_and_publish_scaffold(&paths, request).expect("republish same body");
    assert_eq!(second.status, "complete");
    assert_eq!(second.changed_page_count, 0);
    assert!(!second.page_receipts[0].changed);
    assert_eq!(
        second.page_receipts[0].old_content_sha256.as_deref(),
        Some(second.page_receipts[0].new_content_sha256.as_str())
    );
}

#[test]
fn wiki_publish_records_partial_proof_when_one_draft_cannot_be_written() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");

    let blocked_parent = temp.path().join("blocked-parent");
    fs::write(&blocked_parent, "not a directory").expect("blocked parent");
    let invalid_source_path = blocked_parent.join("page.md");
    let result = write_pages_and_publish_scaffold(
        &paths,
        WikiPublishRequest {
            run_id: "partial-publish".to_string(),
            drafts: vec![
                draft_from_status("topics", "Topics", "A valid page body."),
                WikiPageDraft {
                    slug: "blocked".to_string(),
                    title: "Blocked".to_string(),
                    body: "This draft cannot be written.".to_string(),
                    source_path: Some(invalid_source_path.display().to_string()),
                },
            ],
        },
    )
    .expect("partial publish proof");

    assert_eq!(result.status, "partial");
    assert_eq!(result.page_receipts.len(), 1);
    assert_eq!(result.changed_page_count, 1);
    assert_eq!(result.issues.len(), 1);
    assert!(PathBuf::from(result.publish_proof_path).is_file());
}

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}
