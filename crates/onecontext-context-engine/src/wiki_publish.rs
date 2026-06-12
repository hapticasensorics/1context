//! Wiki page write and publish scaffold.

use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiPageDraft {
    pub slug: String,
    pub title: String,
    pub body: String,
    pub source_path: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiPageWriteReceipt {
    pub schema_version: u32,
    pub kind: String,
    pub slug: String,
    pub page_id: String,
    pub source_path: String,
    pub operation_id: String,
    pub old_content_sha256: Option<String>,
    pub new_content_sha256: String,
    pub content_sha256: String,
    pub changed: bool,
    pub conflict_status: String,
    pub write_proof_path: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiPublishRequest {
    pub run_id: String,
    pub drafts: Vec<WikiPageDraft>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiPublishResult {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub publish_id: String,
    pub status: String,
    pub page_receipts: Vec<WikiPageWriteReceipt>,
    pub changed_page_count: usize,
    pub site_path: String,
    pub publish_proof_path: String,
    pub created_at: String,
    pub issues: Vec<String>,
}

pub fn write_pages_and_publish_scaffold(
    paths: &ContextEnginePaths,
    request: WikiPublishRequest,
) -> Result<WikiPublishResult, String> {
    let run_id = safe_run_id(&request.run_id);
    let mut receipts = Vec::new();
    let mut issues = Vec::new();
    for draft in &request.drafts {
        match write_page_draft(paths, &run_id, draft) {
            Ok(receipt) => receipts.push(receipt),
            Err(error) => issues.push(error),
        }
    }
    let proof_path = paths.run_publish(&run_id).join("publish-proof.json");
    let site_path = paths.user_wiki.join("site");
    if let Some(parent) = proof_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let changed_page_count = receipts.iter().filter(|receipt| receipt.changed).count();
    let result = WikiPublishResult {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.wiki_publish_result.v1".to_string(),
        run_id: run_id.clone(),
        publish_id: format!("wiki-publish-{run_id}"),
        status: if issues.is_empty() {
            "complete"
        } else {
            "partial"
        }
        .to_string(),
        page_receipts: receipts,
        changed_page_count,
        site_path: site_path.display().to_string(),
        publish_proof_path: proof_path.display().to_string(),
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        issues,
    };
    let text = serde_json::to_string_pretty(&result)
        .map_err(|error| format!("failed to encode publish proof: {error}"))?;
    fs::write(&proof_path, text)
        .map_err(|error| format!("failed to write {}: {error}", proof_path.display()))?;
    Ok(result)
}

pub fn draft_from_status(slug: &str, title: &str, body: &str) -> WikiPageDraft {
    WikiPageDraft {
        slug: slug.to_string(),
        title: title.to_string(),
        body: body.to_string(),
        source_path: None,
    }
}

fn write_page_draft(
    paths: &ContextEnginePaths,
    run_id: &str,
    draft: &WikiPageDraft,
) -> Result<WikiPageWriteReceipt, String> {
    let page_id = safe_run_id(&draft.slug);
    let path = draft
        .source_path
        .as_ref()
        .map(PathBuf::from)
        .unwrap_or_else(|| default_page_source_path(paths, &draft.slug));
    let before = fs::read(&path).ok();
    let body = normalize_page_body(draft);
    let old_content_sha256 = before.as_deref().map(sha256_hex);
    let new_content_sha256 = sha256_hex(body.as_bytes());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    fs::write(&path, &body)
        .map_err(|error| format!("failed to write page {}: {error}", path.display()))?;
    let write_proof_path = page_write_proof_path(paths, run_id, &page_id);
    if let Some(parent) = write_proof_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let receipt = WikiPageWriteReceipt {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.wiki_page_write_receipt.v1".to_string(),
        slug: draft.slug.clone(),
        page_id: page_id.clone(),
        source_path: path.display().to_string(),
        operation_id: format!("wiki-page-write-{run_id}-{page_id}"),
        old_content_sha256,
        new_content_sha256: new_content_sha256.clone(),
        content_sha256: new_content_sha256,
        changed: before.as_deref() != Some(body.as_bytes()),
        conflict_status: "none".to_string(),
        write_proof_path: write_proof_path.display().to_string(),
        updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    };
    let text = serde_json::to_string_pretty(&receipt)
        .map_err(|error| format!("failed to encode page write receipt: {error}"))?;
    fs::write(&write_proof_path, text)
        .map_err(|error| format!("failed to write {}: {error}", write_proof_path.display()))?;
    Ok(receipt)
}

fn default_page_source_path(paths: &ContextEnginePaths, slug: &str) -> PathBuf {
    match safe_run_id(slug).as_str() {
        "for-you" => {
            return paths
                .user_wiki
                .join("source/families/for-you/for-you/source/for-you.md");
        }
        "your-context" => {
            return paths
                .user_wiki
                .join("source/families/context/your-context/source/your-context.md");
        }
        "projects" => {
            return paths
                .user_wiki
                .join("source/families/work/projects/source/projects.md");
        }
        "topics" => {
            return paths
                .user_wiki
                .join("source/families/reference/topics/source/topics.md");
        }
        _ => {}
    }
    paths
        .user_wiki
        .join("source")
        .join("families")
        .join("generated")
        .join(safe_run_id(slug))
        .join("source")
        .join(format!("{}.md", safe_run_id(slug)))
}

fn page_write_proof_path(paths: &ContextEnginePaths, run_id: &str, page_id: &str) -> PathBuf {
    paths
        .run_publish(run_id)
        .join(format!("page-write-{}.json", safe_run_id(page_id)))
}

fn normalize_page_body(draft: &WikiPageDraft) -> String {
    if draft.body.trim_start().starts_with("# ") {
        draft.body.trim().to_string() + "\n"
    } else {
        format!("# {}\n\n{}\n", draft.title, draft.body.trim())
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
