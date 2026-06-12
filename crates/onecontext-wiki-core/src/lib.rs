use anyhow::{anyhow, Context, Result};
use chrono::{SecondsFormat, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use toml::Value;

#[allow(dead_code)]
pub mod agent_mail;

#[derive(Clone, Debug)]
pub struct RuntimePaths {
    pub root: PathBuf,
    pub user_wiki: PathBuf,
    pub context_engine: PathBuf,
    pub context_engine_live: PathBuf,
    pub source: PathBuf,
    pub templates: PathBuf,
    pub site: PathBuf,
}

impl RuntimePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let user_wiki = root.join("user-wiki");
        let context_engine = root.join("context-engine");
        let context_engine_live = context_engine.join("live");
        Self {
            source: user_wiki.join("source"),
            templates: user_wiki.join("templates"),
            site: user_wiki.join("site"),
            user_wiki,
            context_engine,
            context_engine_live,
            root,
        }
    }

    pub fn ensure_v0_dirs(&self) -> Result<()> {
        for path in [
            self.user_wiki.join(".1context"),
            self.source.join("families"),
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageRecord {
    pub id: String,
    pub enabled: bool,
    pub title: String,
    pub slug: String,
    pub route: String,
    pub family_group: String,
    pub family_group_title: String,
    pub family_id: String,
    pub family_title: String,
    #[serde(rename = "type")]
    pub page_type: String,
    pub template: String,
    pub talk_conventions_template: Option<String>,
    pub talk_curator_template: Option<String>,
    pub summary: Option<String>,
    pub nav_section: Option<String>,
    pub nav_order: Option<i64>,
    pub origin: String,
}

#[derive(Clone, Debug)]
struct SitePageRecord {
    id: String,
    enabled: bool,
    title: String,
    route: String,
    kind: String,
    template: String,
    nav_section: Option<String>,
    nav_order: Option<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct PageCreateOptions {
    pub title: Option<String>,
    pub slug: Option<String>,
    pub route: Option<String>,
    pub family_group: Option<String>,
    pub family_group_title: Option<String>,
    pub family_id: Option<String>,
    pub family_title: Option<String>,
    pub page_type: Option<String>,
    pub template: Option<String>,
    pub talk_conventions_template: Option<String>,
    pub talk_curator_template: Option<String>,
    pub summary: Option<String>,
    pub nav_order: Option<i64>,
    pub nav_section: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WikiInventory {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub page_count: usize,
    pub source_page_count: usize,
    pub generated_page_count: usize,
    pub pages: Vec<WikiPageStatus>,
    pub publish_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PagePublishFingerprints {
    pub schema_version: u32,
    pub site_navigation_sha256: String,
    pub pages: Vec<PagePublishFingerprint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PagePublishFingerprint {
    pub page_id: String,
    pub route: String,
    pub sha256: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct WikiPageStatus {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub id: String,
    pub title: String,
    pub route: String,
    pub nav_section: Option<String>,
    pub nav_order: Option<i64>,
    #[serde(rename = "type")]
    pub page_type: String,
    pub collection: String,
    pub kind: String,
    pub state: String,
    pub content_state: String,
    pub origin: String,
    pub template_state: String,
    pub template: PageTemplateSummary,
    pub dirty_since_publish: bool,
    pub talk_state: String,
    pub flags: PageFlags,
    pub handles: PageHandles,
    pub links: PageLinkSummary,
    pub validation: ValidationSummary,
    pub allowed_actions: Vec<String>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageTemplateSummary {
    pub relative_path: String,
    pub path: String,
    pub sha256: Option<String>,
    pub baseline_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageFlags {
    pub configured: bool,
    pub enabled: bool,
    pub source_backed: bool,
    pub rendered: bool,
    pub stale: bool,
    pub tombstoned: bool,
    pub talk_ready: bool,
    pub template_derived: bool,
    pub runtime_default: bool,
    pub custom_created: bool,
    pub user_edited: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageHandles {
    pub source: String,
    pub talk: String,
    pub curator: String,
    pub conventions: String,
    pub published: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageOpenResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub id: String,
    pub title: String,
    pub route: String,
    pub nav_section: Option<String>,
    pub nav_order: Option<i64>,
    #[serde(rename = "type")]
    pub page_type: String,
    pub collection: String,
    pub state: String,
    pub content_state: String,
    pub template_state: String,
    pub talk_state: String,
    pub flags: PageFlags,
    pub page_status: WikiPageStatus,
    pub handles: PageHandles,
    pub files: PageOpenFiles,
    pub hashes: PageOpenHashes,
    pub operator_touched: Vec<OperatorTouchedSpan>,
    pub resources: Vec<PageOpenResource>,
    pub edit: EditPreconditions,
    pub allowed_actions: Vec<String>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageOpenFiles {
    pub source: String,
    pub talk: String,
    pub curator: String,
    pub conventions: String,
    pub tombstone: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageOpenHashes {
    pub source_sha256: Option<String>,
    pub talk_sha256: Option<String>,
    pub curator_sha256: Option<String>,
    pub conventions_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageOpenResource {
    pub surface: String,
    pub uri: String,
    pub path: String,
    pub absolute_path: String,
    pub sha256: Option<String>,
    pub safe_to_edit: bool,
    pub write_mode: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageAssetAddResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub page: PageAssetPage,
    pub asset: PageAssetRecord,
    pub render_required: bool,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageAssetListResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub page: PageAssetPage,
    pub asset_count: usize,
    pub assets: Vec<PageAssetRecord>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ReferenceListResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<PageAssetPage>,
    pub reference_index_path: String,
    pub reference_count: usize,
    pub asset_count: usize,
    pub link_count: usize,
    pub code_block_count: usize,
    pub citation_count: usize,
    pub assets: Vec<serde_json::Value>,
    pub links: Vec<serde_json::Value>,
    pub code_blocks: Vec<serde_json::Value>,
    pub citations: Vec<serde_json::Value>,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct ReferenceIndexFile {
    #[serde(default)]
    assets: Vec<serde_json::Value>,
    #[serde(default)]
    links: Vec<serde_json::Value>,
    #[serde(default)]
    code_blocks: Vec<serde_json::Value>,
    #[serde(default)]
    citations: Vec<serde_json::Value>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageAssetPage {
    pub id: String,
    pub route: String,
    pub title: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageAssetRecord {
    pub id: String,
    pub citation_uri: String,
    pub kind: String,
    pub filename: String,
    pub media_type: String,
    pub sha256: String,
    pub bytes: u64,
    pub handle: String,
    pub source_path: String,
    pub absolute_path: String,
    pub source_relative_href: String,
    pub published_href: String,
    pub markdown: String,
    pub content_role: String,
    pub purpose: String,
    pub caption: Option<String>,
    pub alt_text: Option<String>,
    pub referenced: bool,
    pub published: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct EditPreconditions {
    pub expected_source_sha256: Option<String>,
    pub expected_talk_sha256: Option<String>,
    pub must_preserve_user_edits: bool,
    pub must_check_hash_before_write: bool,
    pub safe_to_edit: bool,
    pub recommended_write_mode: String,
    pub direct_source_write_allowed: bool,
    pub recommended_operation: String,
    pub required_preconditions: Vec<String>,
    pub proposal_required: bool,
    pub policy_reason: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageLinkSummary {
    pub status: String,
    pub broken_internal_count: usize,
    pub broken_internal_targets: Vec<String>,
    pub checked_against: Option<String>,
    pub issues: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub repair_tasks: Vec<LinkRepairTask>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkRepairTask {
    pub page_id: String,
    pub route: String,
    pub markdown_path: Option<String>,
    pub source_path: Option<String>,
    pub route_index_path: Option<String>,
    pub broken_internal_count: usize,
    pub hrefs: Vec<String>,
    pub targets: Vec<String>,
    pub next_action: String,
    pub suggested_operations: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ValidationSummary {
    pub status: String,
    pub issue_count: usize,
    pub blocking_count: usize,
    pub warning_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_severity: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageLedgerEvent {
    pub schema_version: u32,
    pub event: String,
    pub page: String,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origin: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub publish_fingerprint: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Actor {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Actor {
    pub fn operator() -> Self {
        Self {
            kind: "operator".to_string(),
            name: None,
        }
    }

    pub fn agent(address: &str) -> Self {
        Self {
            kind: "agent".to_string(),
            name: Some(address.to_string()),
        }
    }

    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        if value == "operator" {
            return Ok(Self::operator());
        }
        if let Some(unit) = value.strip_prefix("agent://") {
            if !unit.trim().is_empty() {
                return Ok(Self::agent(value));
            }
        }
        Err(anyhow!(
            "invalid actor: {value}; expected operator or agent://<unit>"
        ))
    }

    pub fn is_operator(&self) -> bool {
        self.kind == "operator"
    }

    pub fn label(&self) -> &str {
        self.name.as_deref().unwrap_or(&self.kind)
    }
}

/// One operator-touched span in a page body, delimited by
/// `<!-- operator-touched: <note> -->` ... `<!-- /operator-touched -->`
/// marker lines. An unclosed open marker protects through the end of the
/// body (`closed: false`).
#[derive(Clone, Debug, Serialize)]
pub struct OperatorTouchedSpan {
    pub date: Option<String>,
    pub note: Option<String>,
    pub section_slug: Option<String>,
    /// 1-based inclusive line range in the full source file (frontmatter included).
    pub line_range: [usize; 2],
    pub closed: bool,
}

/// Internal parse result: the public span plus the byte range it occupies in
/// the page body (markers included), used for overlap enforcement.
#[derive(Clone, Debug)]
struct ParsedOperatorSpan {
    span: OperatorTouchedSpan,
    start: usize,
    end: usize,
    closed: bool,
}

/// Typed rejection for agent edits that overlap operator-touched spans.
/// Returned through `anyhow::Error`; callers downcast with
/// `error.downcast_ref::<OperatorTouchedConflict>()` for the structured receipt.
#[derive(Clone, Debug, Serialize)]
pub struct OperatorTouchedConflict {
    pub schema_version: u32,
    pub code: String,
    pub operation: String,
    pub page: String,
    pub actor: Actor,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit_line_range: Option<[usize; 2]>,
    pub conflicting_spans: Vec<OperatorTouchedSpan>,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

impl std::fmt::Display for OperatorTouchedConflict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ranges = self
            .conflicting_spans
            .iter()
            .map(|span| format!("{}-{}", span.line_range[0], span.line_range[1]))
            .collect::<Vec<_>>()
            .join(", ");
        write!(
            f,
            "operator_touched_conflict: {} by {} on {} overlaps operator-touched span(s) at line(s) {}; append after the span or leave the edit to the operator",
            self.operation,
            self.actor.label(),
            self.page,
            ranges
        )
    }
}

impl std::error::Error for OperatorTouchedConflict {}

fn operator_touched_conflict_error(
    operation: &str,
    page: &str,
    actor: &Actor,
    edit_line_range: Option<[usize; 2]>,
    conflicting_spans: Vec<OperatorTouchedSpan>,
) -> anyhow::Error {
    anyhow::Error::new(OperatorTouchedConflict {
        schema_version: 1,
        code: "operator_touched_conflict".to_string(),
        operation: operation.to_string(),
        page: page.to_string(),
        actor: actor.clone(),
        edit_line_range,
        conflicting_spans,
        next_action: "append_after_span_or_defer_to_operator".to_string(),
        repair_hints: vec![
            "Operator-authored content is server-protected. Append new content after the span (page-append-section) or post a talk proposal for the operator.".to_string(),
        ],
    })
}

#[derive(Clone, Debug, Serialize)]
pub struct OperationReceipt {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub page_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actor: Option<Actor>,
    pub evidence: Vec<Evidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_status: Option<WikiPageStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<EditPreconditions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<PageOpenHashes>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub before_source_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub after_source_sha256: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link_impact: Option<LinkImpactSummary>,
    pub render_required: bool,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LinkImpactSummary {
    pub status: String,
    pub deleted_route: String,
    pub deleted_markdown_path: String,
    pub post_publish_expected_next_action: String,
    pub inbound_link_count: usize,
    pub source_page_count: usize,
    pub issues: Vec<InboundLinkIssue>,
}

#[derive(Clone, Debug, Serialize)]
pub struct InboundLinkIssue {
    pub code: String,
    pub severity: String,
    pub phase: String,
    pub source_page_id: String,
    pub source_route: String,
    pub source_path: String,
    pub href: String,
    pub target: String,
    pub target_kind: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageCreateAllResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub page_count: usize,
    pub created_or_checked: Vec<OperationReceipt>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Evidence {
    pub path: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageParticipantReference {
    pub id: String,
    pub title: String,
    pub route: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TalkAppendRequest {
    pub page: String,
    pub kind: String,
    pub subject: String,
    #[serde(default)]
    pub operation_id: Option<String>,
    #[serde(default)]
    pub delivery_mode: TalkDeliveryMode,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
    pub from: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    pub body_markdown: String,
    #[serde(default)]
    pub attachments: Vec<TalkAttachmentInput>,
    #[serde(default)]
    pub allow_tombstoned: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TalkDeliveryMode {
    LabelsOnly,
    Mail,
}

impl Default for TalkDeliveryMode {
    fn default() -> Self {
        Self::LabelsOnly
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TalkAttachmentInput {
    pub path: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub caption: Option<String>,
    #[serde(default)]
    pub alt_text: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TalkAttachmentRecord {
    pub filename: String,
    pub media_type: String,
    pub path: String,
    pub handle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caption: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alt_text: Option<String>,
}

#[derive(Clone, Debug)]
struct TalkThreadTarget {
    thread_id: String,
    reply_to: Option<String>,
}

#[derive(Clone, Debug)]
struct TalkMessageTarget {
    message_id: String,
    thread_id: String,
    page_id: String,
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct TalkFileMessage {
    message_id: String,
    thread_id: String,
    reply_to: Option<String>,
    subject: String,
    attachments: Vec<TalkAttachmentRecord>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TalkAppendResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub message_id: String,
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub page_id: String,
    pub route: String,
    pub kind: String,
    pub subject: String,
    pub created_at: String,
    pub source: String,
    pub delivery_mode: TalkDeliveryMode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mail_delivery: Option<TalkMailDeliveryResult>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<TalkAttachmentRecord>,
    pub attachment_count: usize,
    pub render_required: bool,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TalkMailDeliveryResult {
    pub status: String,
    pub acceptance: Option<String>,
    pub attempt_count: usize,
    pub attempts: Vec<TalkMailDeliveryAttempt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct TalkMailDeliveryAttempt {
    pub recipient: String,
    pub delivery_id: String,
    pub status: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublishStatus {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub config_blocking_count: usize,
    pub render_required: bool,
    pub site_needs_publish: bool,
    pub pages_needing_publish: Vec<String>,
    pub pages_missing_source: Vec<String>,
    pub pages_missing_talk: Vec<String>,
    pub link_health: PublishLinkHealth,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PublishLinkHealth {
    pub status: String,
    pub fresh: bool,
    pub broken_internal_count: usize,
    pub pages_with_broken_links: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub repair_tasks: Vec<LinkRepairTask>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ValidationResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub checked_at: String,
    pub scope: String,
    pub input_fingerprint: Option<String>,
    pub can_publish: bool,
    pub issue_count: usize,
    pub blocking_count: usize,
    pub warning_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub highest_severity: Option<String>,
    pub next_action: String,
    pub repair_hints: Vec<String>,
    pub issues: Vec<ValidationIssue>,
}

#[derive(Clone, Debug, Serialize)]
pub struct ValidationIssue {
    pub code: String,
    pub severity: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page: Option<PageParticipantReference>,
    pub paths: Vec<String>,
    pub message: String,
    pub next_action: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub diagnostics: Vec<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub repair_tasks: Vec<LinkRepairTask>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WikiStatusResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub state: String,
    pub next_action: String,
    pub page_count: usize,
    pub source_page_count: usize,
    pub generated_page_count: usize,
    pub publish: PublishStatus,
    pub validation: WikiStatusValidation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_publish: Option<LastPublishSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct WikiStatusValidation {
    pub status: String,
    pub issue_count: usize,
    pub blocking_count: usize,
    pub warning_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct LastPublishSummary {
    pub status: String,
    pub at: Option<String>,
    pub route_count: Option<u64>,
    pub markdown_twin_count: Option<u64>,
}

pub struct WikiCore {
    paths: RuntimePaths,
}

struct LifecycleWriteLock {
    file: File,
}

impl Drop for LifecycleWriteLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn page_receipt_identity(
    page_status: &Option<WikiPageStatus>,
) -> (Option<String>, Option<String>, Option<String>) {
    page_status
        .as_ref()
        .map(|status| {
            (
                Some(status.route.clone()),
                Some(status.page_type.clone()),
                Some(status.collection.clone()),
            )
        })
        .unwrap_or((None, None, None))
}

impl WikiCore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            paths: RuntimePaths::new(root),
        }
    }

    pub fn ensure_runtime_dirs(&self) -> Result<()> {
        self.paths.ensure_v0_dirs()
    }

    pub fn pages(&self) -> Result<Vec<PageRecord>> {
        let config = read_toml(&self.paths.user_wiki.join("wiki.toml"))?;
        let defaults = config.get("defaults");
        let pages = config
            .get("pages")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("wiki.toml does not contain [[pages]]"))?;
        pages
            .iter()
            .map(|page| page_record(page, defaults))
            .collect()
    }

    fn site_pages(&self) -> Result<Vec<SitePageRecord>> {
        let config = read_toml(&self.paths.user_wiki.join("wiki.toml"))?;
        let Some(site_pages) = config.get("site_pages").and_then(Value::as_array) else {
            return Ok(Vec::new());
        };
        site_pages.iter().map(site_page_record).collect()
    }

    pub fn inventory(&self) -> Result<WikiInventory> {
        self.ensure_runtime_dirs()?;
        let ledger = self.page_ledger()?;
        let fingerprint = self.source_fingerprint().ok();
        let published = read_optional_string(
            &self
                .paths
                .site
                .join(".1context")
                .join("source-fingerprint.txt"),
        )?;
        let site_stale = fingerprint.is_some() && published.is_some() && fingerprint != published;
        let current_page_fingerprints = self
            .page_publish_fingerprints()
            .ok()
            .map(page_fingerprint_map);
        let published_page_fingerprints = self
            .published_page_fingerprints()
            .ok()
            .flatten()
            .map(page_fingerprint_map);
        let pages = self
            .pages()?
            .iter()
            .map(|record| {
                let stale = page_publish_stale(
                    record,
                    current_page_fingerprints.as_ref(),
                    published_page_fingerprints.as_ref(),
                    site_stale,
                );
                self.page_status_from_record(record, &ledger, stale)
            })
            .collect::<Result<Vec<_>>>()?;
        let source_page_count = pages.len();
        let mut pages = pages;
        let generated_pages = self
            .site_pages()?
            .iter()
            .map(|record| self.site_page_status_from_record(record))
            .collect::<Vec<_>>();
        let generated_page_count = generated_pages.len();
        pages.extend(generated_pages);
        let page_count = pages.len();
        Ok(WikiInventory {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.list".to_string(),
            surface: "wiki_inventory".to_string(),
            page_count,
            source_page_count,
            generated_page_count,
            pages,
            publish_fingerprint: fingerprint,
        })
    }

    pub fn publish_fingerprint(&self) -> Result<String> {
        self.source_fingerprint()
    }

    pub fn page_publish_fingerprints(&self) -> Result<PagePublishFingerprints> {
        self.ensure_runtime_dirs()?;
        let pages = self.pages()?;
        let site_navigation_sha256 = self.site_navigation_fingerprint()?;
        let mut fingerprints = Vec::new();
        for page in &pages {
            fingerprints.push(PagePublishFingerprint {
                page_id: page.id.clone(),
                route: page.route.clone(),
                sha256: self.page_publish_fingerprint(page)?,
            });
        }
        Ok(PagePublishFingerprints {
            schema_version: 1,
            site_navigation_sha256,
            pages: fingerprints,
        })
    }

    pub fn page_status(&self, reference: &str) -> Result<WikiPageStatus> {
        let inventory = self.inventory()?;
        let route = if reference.starts_with('/') {
            reference.to_string()
        } else {
            format!("/{reference}")
        };
        inventory
            .pages
            .into_iter()
            .find(|page| page.id == reference || page.route == route)
            .map(Ok)
            .unwrap_or_else(|| self.unknown_source_page_error(reference))
    }

    pub fn open_page(&self, reference: &str) -> Result<PageOpenResult> {
        self.ensure_runtime_dirs()?;
        let record = self.find_page(reference)?;
        let status = self.page_status(reference)?;
        let source = self.source_path(&record);
        let talk = self.talk_dir(&record);
        let curator = talk.join("_curator.md");
        let conventions = talk.join("_conventions.md");
        let tombstone = source.with_extension("tombstone.toml");
        let source_hash = sha256_file(&source).ok();
        let operator_touched = fs::read_to_string(&source)
            .ok()
            .map(|text| operator_touched_spans_for_source(&text))
            .unwrap_or_default();
        let talk_hash = if talk.is_dir() {
            tree_fingerprint(&talk).ok()
        } else {
            None
        };
        let curator_hash = sha256_file(&curator).ok();
        let conventions_hash = sha256_file(&conventions).ok();
        let source_missing = status.state == "source_missing";
        let safe_to_edit =
            !source_missing && status.state != "tombstoned" && status.state != "disabled";
        let recommended_operation = if source_missing {
            "wiki.publish"
        } else if status.state == "tombstoned" || status.state == "disabled" {
            "wiki.page.restore"
        } else if !safe_to_edit {
            "wiki.validate"
        } else if status.content_state == "template_unedited" {
            "wiki.page.write_body"
        } else {
            "wiki.page.patch_body"
        }
        .to_string();
        let write_mode = if source_missing {
            "create_from_template".to_string()
        } else if !safe_to_edit {
            "not_editable".to_string()
        } else if status.content_state == "template_unedited" {
            "hash_checked_direct_edit".to_string()
        } else {
            "hash_checked_patch".to_string()
        };
        let proposal_required = !safe_to_edit && !source_missing;
        let policy_reason = if source_missing {
            "Configured page source is missing. Run wiki.publish to safely backfill configured pages, or wiki.page.create for this page only.".to_string()
        } else if !safe_to_edit {
            "Page is tombstoned or disabled; do not modify source directly.".to_string()
        } else if status.content_state == "template_unedited" {
            "Page still matches its template; a hash-checked body write is allowed, and a talk proposal is optional for broad or uncertain edits.".to_string()
        } else {
            "Page has post-template edits; prefer a narrow hash-checked patch. Use a talk proposal for broad rewrites or unclear ownership.".to_string()
        };
        Ok(PageOpenResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.page.open".to_string(),
            id: record.id.clone(),
            title: status.title.clone(),
            route: status.route.clone(),
            nav_section: status.nav_section.clone(),
            nav_order: status.nav_order,
            page_type: status.page_type.clone(),
            collection: status.collection.clone(),
            state: status.state.clone(),
            content_state: status.content_state.clone(),
            template_state: status.template_state.clone(),
            talk_state: status.talk_state.clone(),
            flags: status.flags.clone(),
            page_status: status.clone(),
            handles: status.handles.clone(),
            files: PageOpenFiles {
                source: display_under_root(&source, &self.paths.root),
                talk: display_under_root(&talk, &self.paths.root),
                curator: display_under_root(&curator, &self.paths.root),
                conventions: display_under_root(&conventions, &self.paths.root),
                tombstone: display_under_root(&tombstone, &self.paths.root),
            },
            hashes: PageOpenHashes {
                source_sha256: source_hash.clone(),
                talk_sha256: talk_hash.clone(),
                curator_sha256: curator_hash.clone(),
                conventions_sha256: conventions_hash.clone(),
            },
            operator_touched,
            resources: vec![
                PageOpenResource {
                    surface: "source".to_string(),
                    uri: format!("user-wiki://page/{}/source", record.id),
                    path: display_under_root(&source, &self.paths.root),
                    absolute_path: source.display().to_string(),
                    sha256: source_hash.clone(),
                    safe_to_edit,
                    write_mode: write_mode.clone(),
                },
                PageOpenResource {
                    surface: "talk".to_string(),
                    uri: format!("user-wiki://page/{}/talk", record.id),
                    path: display_under_root(&talk, &self.paths.root),
                    absolute_path: talk.display().to_string(),
                    sha256: talk_hash.clone(),
                    safe_to_edit: talk.is_dir(),
                    write_mode: if talk.is_dir() {
                        "append_message"
                    } else {
                        "create_from_template"
                    }
                    .to_string(),
                },
                PageOpenResource {
                    surface: "curator".to_string(),
                    uri: format!("user-wiki://page/{}/curator", record.id),
                    path: display_under_root(&curator, &self.paths.root),
                    absolute_path: curator.display().to_string(),
                    sha256: curator_hash,
                    safe_to_edit,
                    write_mode: "proposal".to_string(),
                },
                PageOpenResource {
                    surface: "conventions".to_string(),
                    uri: format!("user-wiki://page/{}/conventions", record.id),
                    path: display_under_root(&conventions, &self.paths.root),
                    absolute_path: conventions.display().to_string(),
                    sha256: conventions_hash,
                    safe_to_edit,
                    write_mode: "proposal".to_string(),
                },
            ],
            edit: EditPreconditions {
                expected_source_sha256: source_hash,
                expected_talk_sha256: talk_hash,
                must_preserve_user_edits: true,
                must_check_hash_before_write: true,
                safe_to_edit,
                recommended_write_mode: write_mode,
                direct_source_write_allowed: safe_to_edit,
                recommended_operation,
                required_preconditions: if safe_to_edit {
                    vec![
                        "expected_source_sha256".to_string(),
                        "preserve_user_edits".to_string(),
                    ]
                } else {
                    Vec::new()
                },
                proposal_required,
                policy_reason,
            },
            allowed_actions: page_allowed_actions(&status.state, status.flags.rendered),
            next_action: status.next_action,
        })
    }

    pub fn add_page_asset(
        &self,
        reference: &str,
        input: &Path,
        filename: Option<&str>,
        purpose: Option<&str>,
        caption: Option<&str>,
        alt_text: Option<&str>,
    ) -> Result<PageAssetAddResult> {
        self.ensure_runtime_dirs()?;
        let _lifecycle_lock = self.lock_lifecycle_writes()?;
        let record = self.find_page(reference)?;
        let status = self.page_status(reference)?;
        if status.state == "source_missing" {
            return Err(anyhow!(
                "page asset target source is missing for {}; create or publish the page before adding assets",
                record.id
            ));
        }
        if status.state == "tombstoned" || status.state == "disabled" {
            return Err(anyhow!(
                "page asset target {} is {}; restore the page before adding assets",
                record.id,
                status.state
            ));
        }
        if !input.is_file() {
            return Err(anyhow!("asset file not found: {}", input.display()));
        }

        let requested_name = filename
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
            .or_else(|| {
                input
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .ok_or_else(|| anyhow!("asset filename missing for {}", input.display()))?;
        let asset_dir = self.asset_dir(&record);
        let mut used = BTreeSet::new();
        if asset_dir.is_dir() {
            for entry in fs::read_dir(&asset_dir)? {
                let entry = entry?;
                if entry.file_type()?.is_file() {
                    if let Some(name) = entry.file_name().to_str() {
                        used.insert(name.to_string());
                    }
                }
            }
        }
        let safe_name =
            unique_attachment_filename(safe_attachment_filename(&requested_name)?, &mut used);
        let destination = asset_dir.join(&safe_name);
        let media_type = infer_media_type(&safe_name);
        let normalized_purpose = purpose
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string())
            .unwrap_or_else(|| {
                if media_type.starts_with("image/") {
                    "inline_image".to_string()
                } else {
                    "download".to_string()
                }
            });
        let caption = caption
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string());
        let alt_text = alt_text
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.trim().to_string());
        if media_type.starts_with("image/")
            && normalized_purpose != "decorative"
            && alt_text.is_none()
        {
            return Err(anyhow!(
                "image asset requires --alt-text unless --purpose decorative is used"
            ));
        }
        if !media_type.starts_with("image/") && caption.is_none() {
            return Err(anyhow!(
                "file asset requires --caption so the reader link has a label"
            ));
        }

        fs::create_dir_all(&asset_dir)
            .with_context(|| format!("create page asset directory {}", asset_dir.display()))?;
        fs::copy(input, &destination).with_context(|| {
            format!(
                "copy page asset {} to {}",
                input.display(),
                destination.display()
            )
        })?;
        let asset_sha256 = sha256_file(&destination).ok();
        self.append_page_event(PageLedgerEvent {
            schema_version: 1,
            event: "page.asset_added".to_string(),
            page: record.id.clone(),
            at: now_rfc3339(),
            actor: None,
            origin: Some(safe_name.clone()),
            source_sha256: asset_sha256,
            template_sha256: None,
            publish_fingerprint: None,
        })?;

        let asset = self.page_asset_record(
            &record,
            &destination,
            &normalized_purpose,
            caption.as_deref(),
            alt_text.as_deref(),
        )?;
        Ok(PageAssetAddResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.asset.add".to_string(),
            page: PageAssetPage {
                id: record.id,
                route: status.route,
                title: status.title,
            },
            asset,
            render_required: true,
            next_action: "insert_markdown".to_string(),
            repair_hints: vec![],
        })
    }

    pub fn list_page_assets(&self, reference: &str) -> Result<PageAssetListResult> {
        self.ensure_runtime_dirs()?;
        let record = self.find_page(reference)?;
        let status = self.page_status(reference)?;
        let asset_dir = self.asset_dir(&record);
        let mut assets = Vec::new();
        if asset_dir.is_dir() {
            let mut files = Vec::new();
            collect_files(&asset_dir, &mut files)?;
            files.sort();
            for file in files {
                if file.is_file() {
                    assets.push(self.page_asset_record(&record, &file, "unknown", None, None)?);
                }
            }
        }
        let next_action = if assets.is_empty() {
            "asset_add"
        } else if assets.iter().any(|asset| !asset.referenced) {
            "insert_markdown"
        } else if assets.iter().any(|asset| !asset.published) {
            "publish"
        } else {
            "none"
        };
        Ok(PageAssetListResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.asset.list".to_string(),
            page: PageAssetPage {
                id: record.id,
                route: status.route,
                title: status.title,
            },
            asset_count: assets.len(),
            next_action: next_action.to_string(),
            assets,
        })
    }

    pub fn list_references(&self, reference: Option<&str>) -> Result<ReferenceListResult> {
        self.ensure_runtime_dirs()?;
        let reference_index_path = self.paths.site.join(".1context/reference-index.json");
        let reference_index_display = display_under_root(&reference_index_path, &self.paths.root);
        let page_status = reference.map(|page| self.page_status(page)).transpose()?;
        let page = page_status.as_ref().map(|status| PageAssetPage {
            id: status.id.clone(),
            route: status.route.clone(),
            title: status.title.clone(),
        });
        if !reference_index_path.is_file() {
            return Ok(ReferenceListResult {
                schema_version: 1,
                status: "missing".to_string(),
                operation: "wiki.reference.list".to_string(),
                surface: "wiki_references".to_string(),
                page,
                reference_index_path: reference_index_display,
                reference_count: 0,
                asset_count: 0,
                link_count: 0,
                code_block_count: 0,
                citation_count: 0,
                assets: vec![],
                links: vec![],
                code_blocks: vec![],
                citations: vec![],
                next_action: "publish".to_string(),
                repair_hints: vec![
                    "Run wiki.publish to generate user-wiki/site/.1context/reference-index.json."
                        .to_string(),
                ],
            });
        }

        let index: ReferenceIndexFile =
            serde_json::from_slice(&fs::read(&reference_index_path)?)
                .with_context(|| format!("parse {}", reference_index_path.display()))?;
        let mut assets = index.assets;
        let mut links = index.links;
        let mut code_blocks = index.code_blocks;
        let mut citations = index.citations;
        if let Some(status) = &page_status {
            assets.retain(|value| reference_record_matches_page(value, &status.id, &status.route));
            links.retain(|value| reference_record_matches_page(value, &status.id, &status.route));
            code_blocks
                .retain(|value| reference_record_matches_page(value, &status.id, &status.route));
            citations
                .retain(|value| reference_record_matches_page(value, &status.id, &status.route));
        }
        let reference_count = assets.len() + links.len() + code_blocks.len() + citations.len();
        Ok(ReferenceListResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.reference.list".to_string(),
            surface: "wiki_references".to_string(),
            page,
            reference_index_path: reference_index_display,
            reference_count,
            asset_count: assets.len(),
            link_count: links.len(),
            code_block_count: code_blocks.len(),
            citation_count: citations.len(),
            assets,
            links,
            code_blocks,
            citations,
            next_action: "none".to_string(),
            repair_hints: vec![],
        })
    }

    pub fn publish_status(&self) -> Result<PublishStatus> {
        let config_issues = self.config_integrity_issues()?;
        let config_blocking_count = config_issues
            .iter()
            .filter(|issue| issue.severity == "error")
            .count();
        let inventory = self.inventory()?;
        let site_needs_publish = self.site_needs_publish()?;
        let pages_needing_publish = inventory
            .pages
            .iter()
            .filter(|page| {
                page.state == "needs_publish"
                    || ((page.state == "tombstoned" || page.state == "disabled")
                        && page.flags.rendered)
            })
            .map(|page| page.id.clone())
            .collect::<Vec<_>>();
        let pages_missing_source = inventory
            .pages
            .iter()
            .filter(|page| page.state == "source_missing")
            .map(|page| page.id.clone())
            .collect::<Vec<_>>();
        let pages_missing_talk = inventory
            .pages
            .iter()
            .filter(|page| page.state == "talk_missing")
            .map(|page| page.id.clone())
            .collect::<Vec<_>>();
        let pages_with_broken_links = inventory
            .pages
            .iter()
            .filter(|page| page.links.broken_internal_count > 0)
            .map(|page| page.id.clone())
            .collect::<Vec<_>>();
        let repair_tasks = inventory
            .pages
            .iter()
            .flat_map(|page| page.links.repair_tasks.clone())
            .collect::<Vec<_>>();
        let broken_internal_count = inventory
            .pages
            .iter()
            .map(|page| page.links.broken_internal_count)
            .sum::<usize>();
        let render_required = site_needs_publish
            || !pages_needing_publish.is_empty()
            || !pages_missing_source.is_empty()
            || !pages_missing_talk.is_empty();
        let mut repair_hints = repair_hints_for_validation_issues(&config_issues);
        let next_action = if config_blocking_count > 0 {
            "repair_wiki_toml"
        } else if !pages_missing_source.is_empty() {
            "publish"
        } else if site_needs_publish
            || !pages_missing_talk.is_empty()
            || !pages_needing_publish.is_empty()
        {
            "publish"
        } else if broken_internal_count > 0 {
            "repair_links"
        } else {
            "none"
        };
        Ok(PublishStatus {
            schema_version: 1,
            status: if config_blocking_count > 0 {
                "blocked"
            } else {
                "ok"
            }
            .to_string(),
            operation: "wiki.publish.status".to_string(),
            surface: "wiki_publish_status".to_string(),
            config_blocking_count,
            render_required,
            site_needs_publish,
            pages_needing_publish,
            pages_missing_source,
            pages_missing_talk,
            link_health: PublishLinkHealth {
                status: if broken_internal_count > 0 {
                    "warning"
                } else {
                    "ok"
                }
                .to_string(),
                fresh: !render_required,
                broken_internal_count,
                pages_with_broken_links,
                repair_tasks,
            },
            next_action: next_action.to_string(),
            repair_hints: std::mem::take(&mut repair_hints),
        })
    }

    pub fn validate(&self) -> Result<ValidationResult> {
        let mut issues = self.config_integrity_issues()?;
        let mut input_fingerprint = self.source_fingerprint().ok();

        if !issues.iter().any(|issue| issue.severity == "error") {
            let inventory = self.inventory()?;
            let publish = self.publish_status()?;
            input_fingerprint = inventory.publish_fingerprint.clone();

            for page in &inventory.pages {
                let page_ref = Some(PageParticipantReference {
                    id: page.id.clone(),
                    title: page.title.clone(),
                    route: page.route.clone(),
                });
                if page.state == "source_missing" {
                    issues.push(ValidationIssue {
                        code: "configured_page_missing_source".to_string(),
                        severity: "warning".to_string(),
                        page: page_ref.clone(),
                        paths: vec!["user-wiki/wiki.toml".to_string()],
                        message: format!(
                            "Configured page {} is missing source; publish will backfill it from the configured template.",
                            page.id
                        ),
                        next_action: "publish".to_string(),
                        diagnostics: Vec::new(),
                        repair_tasks: Vec::new(),
                    });
                }
                if page.state == "talk_missing" {
                    issues.push(ValidationIssue {
                        code: "configured_page_missing_talk".to_string(),
                        severity: "warning".to_string(),
                        page: page_ref.clone(),
                        paths: vec![page.handles.talk.clone()],
                        message: format!(
                            "Configured page {} is missing its talk folder; publish will backfill it from configured talk templates.",
                            page.id
                        ),
                        next_action: "publish".to_string(),
                        diagnostics: Vec::new(),
                        repair_tasks: Vec::new(),
                    });
                }
                if page.state == "needs_publish" || page.dirty_since_publish {
                    issues.push(ValidationIssue {
                        code: "page_needs_publish".to_string(),
                        severity: "warning".to_string(),
                        page: page_ref.clone(),
                        paths: vec![page.handles.source.clone()],
                        message: format!(
                            "Page {} has source changes that are not published.",
                            page.id
                        ),
                        next_action: "publish".to_string(),
                        diagnostics: Vec::new(),
                        repair_tasks: Vec::new(),
                    });
                }
                if page.links.broken_internal_count > 0 {
                    issues.push(ValidationIssue {
                        code: "page_has_broken_internal_links".to_string(),
                        severity: "warning".to_string(),
                        page: page_ref,
                        paths: page.links.broken_internal_targets.clone(),
                        message: format!(
                            "Page {} has {} broken internal link(s).",
                            page.id, page.links.broken_internal_count
                        ),
                        next_action: "repair_links".to_string(),
                        diagnostics: page.links.issues.clone(),
                        repair_tasks: page.links.repair_tasks.clone(),
                    });
                }
            }

            for record in self.pages()? {
                if let Some(issue) = self.page_source_frontmatter_issue(&record)? {
                    issues.push(issue);
                }
            }

            if publish.site_needs_publish && publish.pages_needing_publish.is_empty() {
                issues.push(ValidationIssue {
                    code: "site_needs_publish".to_string(),
                    severity: "warning".to_string(),
                    page: None,
                    paths: vec!["user-wiki/site".to_string()],
                    message: "The rendered site is missing or stale.".to_string(),
                    next_action: "publish".to_string(),
                    diagnostics: Vec::new(),
                    repair_tasks: Vec::new(),
                });
            }
        }

        let blocking_count = issues
            .iter()
            .filter(|issue| issue.severity == "error")
            .count();
        let warning_count = issues
            .iter()
            .filter(|issue| issue.severity == "warning")
            .count();
        let highest_severity = if blocking_count > 0 {
            Some("error".to_string())
        } else if warning_count > 0 {
            Some("warning".to_string())
        } else {
            None
        };
        let status = if blocking_count > 0 {
            "error"
        } else if warning_count > 0 {
            "warning"
        } else {
            "ok"
        };
        let next_action = issues
            .iter()
            .find(|issue| issue.severity == "error")
            .or_else(|| issues.first())
            .map(|issue| issue.next_action.clone())
            .unwrap_or_else(|| "none".to_string());

        Ok(ValidationResult {
            schema_version: 1,
            status: status.to_string(),
            operation: "wiki.validate".to_string(),
            surface: "wiki_validation".to_string(),
            checked_at: now_rfc3339(),
            scope: "site".to_string(),
            input_fingerprint,
            can_publish: blocking_count == 0,
            issue_count: issues.len(),
            blocking_count,
            warning_count,
            highest_severity,
            next_action,
            repair_hints: repair_hints_for_validation_issues(&issues),
            issues,
        })
    }

    fn page_source_frontmatter_issue(
        &self,
        record: &PageRecord,
    ) -> Result<Option<ValidationIssue>> {
        let source = self.source_path(record);
        if !source.is_file() {
            return Ok(None);
        }
        let relative_source = display_under_root(&source, &self.paths.root);
        let text = fs::read_to_string(&source)
            .with_context(|| format!("read page source {}", source.display()))?;
        let fields = match parse_simple_frontmatter_fields(&text) {
            Ok(fields) => fields,
            Err(error) => {
                return Ok(Some(page_frontmatter_issue(
                    record,
                    relative_source,
                    format!("Page {} has invalid frontmatter: {error:#}.", record.id),
                )));
            }
        };
        Ok(
            validate_renderer_frontmatter_fields(&fields).map(|message| {
                page_frontmatter_issue(
                    record,
                    relative_source,
                    format!(
                        "Page {} has renderer-incompatible frontmatter: {message}.",
                        record.id
                    ),
                )
            }),
        )
    }

    fn config_integrity_issues(&self) -> Result<Vec<ValidationIssue>> {
        let config = read_toml(&self.paths.user_wiki.join("wiki.toml"))?;
        let pages = self.pages().unwrap_or_default();
        let mut issues = Vec::new();
        let mut seen_ids = BTreeMap::<String, PageParticipantReference>::new();
        let mut route_owners = reserved_site_routes(&config);
        let mut configured_ids = BTreeSet::new();

        for page in &pages {
            let page_ref = PageParticipantReference {
                id: page.id.clone(),
                title: page.title.clone(),
                route: page.route.clone(),
            };
            configured_ids.insert(page.id.clone());
            if seen_ids.insert(page.id.clone(), page_ref.clone()).is_some() {
                issues.push(config_issue(
                    "duplicate_page_id",
                    Some(page_ref.clone()),
                    format!(
                        "wiki.toml defines page id {} more than once; keep one [[pages]] entry or assign a unique id.",
                        page.id
                    ),
                ));
            }
            if let Err(error) = validate_page_slug(&page.slug) {
                issues.push(config_issue(
                    "invalid_page_slug",
                    Some(page_ref.clone()),
                    error.to_string(),
                ));
            }
            if let Err(error) = validate_page_route(&page.route) {
                issues.push(config_issue(
                    "invalid_page_route",
                    Some(page_ref.clone()),
                    error.to_string(),
                ));
            }
            if let Some(nav_section) = page.nav_section.as_deref() {
                if let Err(error) = validate_nav_section(nav_section) {
                    issues.push(config_issue(
                        "invalid_nav_section",
                        Some(page_ref.clone()),
                        error.to_string(),
                    ));
                }
            }
            push_route_reservation_issue(
                &mut issues,
                &mut route_owners,
                route_conflict_key(&page.route),
                format!("page {}", page.id),
                Some(page_ref.clone()),
            );
            push_route_reservation_issue(
                &mut issues,
                &mut route_owners,
                talk_route_for_route(&page.route),
                format!("talk route for page {}", page.id),
                Some(page_ref),
            );
        }

        if let Some(site_pages) = config.get("site_pages").and_then(Value::as_array) {
            for site_page in site_pages {
                let id = get_opt_str(site_page, "id").unwrap_or_else(|| "unknown".to_string());
                let title = get_opt_str(site_page, "title").unwrap_or_else(|| id.clone());
                let route = get_opt_str(site_page, "route").unwrap_or_else(|| format!("/{id}"));
                configured_ids.insert(id.clone());
                let page_ref = PageParticipantReference {
                    id,
                    title,
                    route: route.clone(),
                };
                if let Err(error) = validate_page_route(&route) {
                    issues.push(config_issue(
                        "invalid_site_page_route",
                        Some(page_ref.clone()),
                        error.to_string(),
                    ));
                }
                if site_page
                    .get("nav_order")
                    .is_some_and(|value| value.as_integer().is_none())
                {
                    issues.push(config_issue(
                        "invalid_nav_order",
                        Some(page_ref),
                        "site page nav_order must be an integer.".to_string(),
                    ));
                }
            }
        }

        if let Some(page_values) = config.get("pages").and_then(Value::as_array) {
            for page in page_values {
                if page
                    .get("nav_order")
                    .is_some_and(|value| value.as_integer().is_none())
                {
                    let id = get_opt_str(page, "id").unwrap_or_else(|| "unknown".to_string());
                    let title = get_opt_str(page, "title").unwrap_or_else(|| id.clone());
                    let route = get_opt_str(page, "route").unwrap_or_else(|| format!("/{id}"));
                    issues.push(config_issue(
                        "invalid_nav_order",
                        Some(PageParticipantReference { id, title, route }),
                        "page nav_order must be an integer.".to_string(),
                    ));
                }
            }
        }

        for nav_key in ["navigation", "primary_navigation", "utility_navigation"] {
            if let Some(values) = site_array(&config, nav_key) {
                for value in values {
                    let Some(id) = value.as_str() else {
                        issues.push(config_issue(
                            "invalid_navigation_entry",
                            None,
                            format!("{nav_key} entries must be page ids."),
                        ));
                        continue;
                    };
                    if !configured_ids.contains(id) {
                        issues.push(config_issue(
                            "unknown_navigation_page",
                            None,
                            format!(
                                "{nav_key} references {id}, but no [[pages]] or [[site_pages]] entry owns that id."
                            ),
                        ));
                    }
                }
            }
        }

        Ok(issues)
    }

    pub fn status(&self) -> Result<WikiStatusResult> {
        let inventory = self.inventory()?;
        let publish = self.publish_status()?;
        let validation = self.validate()?;
        let state = if validation.status == "error" {
            "blocked"
        } else if publish.next_action == "none" && validation.status == "ok" {
            "idle"
        } else {
            "attention"
        };
        Ok(WikiStatusResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.status".to_string(),
            state: state.to_string(),
            next_action: if validation.next_action != "none" {
                validation.next_action.clone()
            } else {
                publish.next_action.clone()
            },
            page_count: inventory.page_count,
            source_page_count: inventory.source_page_count,
            generated_page_count: inventory.generated_page_count,
            publish,
            validation: WikiStatusValidation {
                status: validation.status,
                issue_count: validation.issue_count,
                blocking_count: validation.blocking_count,
                warning_count: validation.warning_count,
            },
            last_publish: self.last_publish_summary().ok().flatten(),
        })
    }

    fn last_publish_summary(&self) -> Result<Option<LastPublishSummary>> {
        let path = self.last_publish_receipt_path();
        if !path.is_file() {
            return Ok(None);
        }
        let value = serde_json::from_slice::<serde_json::Value>(&fs::read(&path)?)
            .with_context(|| format!("parse {}", path.display()))?;
        let render_result = value.get("render_result").unwrap_or(&value);
        Ok(Some(LastPublishSummary {
            status: value
                .get("status")
                .or_else(|| render_result.get("status"))
                .and_then(|value| value.as_str())
                .unwrap_or("unknown")
                .to_string(),
            at: render_result
                .get("rendered_at")
                .and_then(|value| value.as_str())
                .map(ToString::to_string),
            route_count: render_result
                .get("route_count")
                .and_then(|value| value.as_u64()),
            markdown_twin_count: render_result
                .get("markdown_twin_count")
                .and_then(|value| value.as_u64()),
        }))
    }

    pub fn create_page(&self, page_id: &str, actor: Option<Actor>) -> Result<OperationReceipt> {
        self.create_page_with_options(page_id, PageCreateOptions::default(), actor)
    }

    pub fn create_page_with_options(
        &self,
        page_id: &str,
        options: PageCreateOptions,
        actor: Option<Actor>,
    ) -> Result<OperationReceipt> {
        self.ensure_runtime_dirs()?;
        let _lifecycle_lock = self.lock_lifecycle_writes()?;
        let record = match self.find_page(page_id) {
            Ok(record) => {
                let status = self.page_status(&record.id)?;
                if matches!(status.state.as_str(), "tombstoned" | "disabled") {
                    return Err(anyhow!(
                        "page-create refused for {} page {}; use a new page id or an explicit restore operation",
                        status.state,
                        record.id
                    ));
                }
                validate_existing_page_create_options(&record, &options)?;
                record
            }
            Err(error) => {
                if self.site_page_reference(page_id)?.is_some() {
                    return Err(error);
                }
                self.validate_new_page_templates(&options)?;
                self.append_page_registry_entry(page_id, &options)?
            }
        };
        let mut evidence = Vec::new();

        let group_dir = self
            .paths
            .source
            .join("families")
            .join(&record.family_group);
        let family_dir = group_dir.join(&record.family_id);
        let source_dir = family_dir.join("source");
        let talk_dir = family_dir
            .join("talk")
            .join(format!("{}.talk", record.slug));
        let templates_dir = family_dir.join("templates");
        let talk_templates_dir = templates_dir.join("talk");
        for dir in [
            &group_dir,
            &family_dir,
            &source_dir,
            &talk_dir,
            &templates_dir,
            &talk_templates_dir,
        ] {
            fs::create_dir_all(dir)?;
        }

        write_if_missing(
            &self.paths.root,
            &group_dir.join("group.toml"),
            format!(
                "schema_version = 1\ntitle = \"{}\"\n",
                record.family_group_title
            ),
            &mut evidence,
        )?;
        write_if_missing(
            &self.paths.root,
            &family_dir.join("family.toml"),
            format!(
                "schema_version = 1\nid = \"{}\"\ntitle = \"{}\"\npage_id = \"{}\"\nslug = \"{}\"\nroute = \"{}\"\ntype = \"{}\"\ntemplate = \"{}\"\n",
                record.family_id, record.family_title, record.id, record.slug, record.route, record.page_type, record.template
            ),
            &mut evidence,
        )?;

        let vars = self.template_vars(&record);
        let source_template = self.read_template(&record.template)?;
        let source_content = render_template(&source_template, &vars);
        let source_path = source_dir.join(format!("{}.md", record.slug));
        let source_preexisted = source_path.exists();
        if !source_preexisted {
            self.validate_page_source_render_contract(&record)?;
        }
        write_if_missing(
            &self.paths.root,
            &source_path,
            source_content.clone(),
            &mut evidence,
        )?;
        write_if_missing(
            &self.paths.root,
            &templates_dir.join("page.template.md"),
            source_content.clone(),
            &mut evidence,
        )?;

        let talk_route = if record.route == "/" {
            "/talk".to_string()
        } else {
            format!("{}/talk", record.route.trim_end_matches('/'))
        };
        let meta = format!(
            "title: \"Talk - {}\"\ntalk_for: \"mailbox://page/{}\"\npage: \"user-wiki://source/families/{}/{}/source/{}.md\"\nroute: \"{}\"\ntalk_route: \"{}\"\nstatus: open\nschema_version: 1\n",
            record.title,
            record.id,
            record.family_group,
            record.family_id,
            record.slug,
            record.route,
            talk_route
        );
        write_if_missing(
            &self.paths.root,
            &talk_dir.join("_meta.yaml"),
            meta,
            &mut evidence,
        )?;

        if let Some(template) = &record.talk_conventions_template {
            let content = render_template(&self.read_template(template)?, &vars);
            write_if_missing(
                &self.paths.root,
                &talk_dir.join("_conventions.md"),
                content.clone(),
                &mut evidence,
            )?;
            write_if_missing(
                &self.paths.root,
                &talk_templates_dir.join("_conventions.template.md"),
                content,
                &mut evidence,
            )?;
        }
        let curator_template = record
            .talk_curator_template
            .as_deref()
            .ok_or_else(|| anyhow!("page {} missing talk_curator_template; page-create requires an explicit curator template", record.id))?;
        let curator_content = render_template(&self.read_template(curator_template)?, &vars);
        write_if_missing(
            &self.paths.root,
            &talk_dir.join("_curator.md"),
            curator_content.clone(),
            &mut evidence,
        )?;
        write_if_missing(
            &self.paths.root,
            &talk_templates_dir.join("_curator.template.md"),
            curator_content,
            &mut evidence,
        )?;
        let entry_template = render_template(&self.read_template("talk/entry.md")?, &vars);
        write_if_missing(
            &self.paths.root,
            &talk_templates_dir.join("entry.template.md"),
            entry_template,
            &mut evidence,
        )?;

        if !source_preexisted {
            let source_hash = sha256_file(&source_path).ok();
            let template_hash = Some(sha256_bytes(source_template.as_bytes()));
            self.append_page_event(PageLedgerEvent {
                schema_version: 1,
                event: "page.created".to_string(),
                page: record.id.clone(),
                at: now_rfc3339(),
                actor: actor.clone(),
                origin: Some(record.origin.clone()),
                source_sha256: source_hash.clone(),
                template_sha256: None,
                publish_fingerprint: None,
            })?;
            self.append_page_event(PageLedgerEvent {
                schema_version: 1,
                event: "template.baseline".to_string(),
                page: record.id.clone(),
                at: now_rfc3339(),
                actor: actor.clone(),
                origin: None,
                source_sha256: source_hash,
                template_sha256: template_hash,
                publish_fingerprint: None,
            })?;
        }

        let id = record.id.clone();
        let (page_status, edit, hashes) = self.page_receipt_context(&id);
        let (route, page_type, collection) = page_receipt_identity(&page_status);
        Ok(OperationReceipt {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.page.create".to_string(),
            id,
            route,
            page_type,
            collection,
            actor,
            evidence,
            page_status,
            edit,
            hashes,
            before_source_sha256: None,
            after_source_sha256: None,
            link_impact: None,
            render_required: true,
            next_action: "publish".to_string(),
            repair_hints: Vec::new(),
        })
    }

    pub fn create_all_pages(&self) -> Result<PageCreateAllResult> {
        self.create_all_pages_with_operation("wiki.page.create_all")
    }

    pub fn create_all_pages_for_publish_preflight(&self) -> Result<PageCreateAllResult> {
        self.create_all_pages_with_operation("wiki.publish.preflight")
    }

    fn create_all_pages_with_operation(&self, operation: &str) -> Result<PageCreateAllResult> {
        self.ensure_runtime_dirs()?;
        let mut created_or_checked = Vec::new();
        for record in self.pages()? {
            if !record.enabled {
                continue;
            }
            created_or_checked.push(self.create_page(&record.id, None)?);
        }
        Ok(PageCreateAllResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: operation.to_string(),
            page_count: created_or_checked.len(),
            created_or_checked,
        })
    }

    pub fn write_page_body(
        &self,
        reference: &str,
        body_markdown: &str,
        expected_source_sha256: Option<&str>,
        actor: &Actor,
    ) -> Result<OperationReceipt> {
        self.ensure_runtime_dirs()?;
        let _lifecycle_lock = self.lock_lifecycle_writes()?;
        let record = self.find_page(reference)?;
        self.ensure_page_body_editable(&record)?;
        let source = self.source_path(&record);
        let before_hash = sha256_file(&source)?;
        if let Some(expected) = expected_source_sha256 {
            if expected != before_hash {
                return Err(anyhow!(
                    "source hash mismatch for {}; expected {}, found {}",
                    record.id,
                    expected,
                    before_hash
                ));
            }
        }
        let original =
            fs::read_to_string(&source).with_context(|| format!("read {}", source.display()))?;
        let (frontmatter, body) = split_markdown_frontmatter(&original)?;
        // A whole-body rewrite touches every byte, so any operator-touched
        // span on the page rejects an agent write.
        enforce_operator_touched_spans(
            "wiki.page.write_body",
            &record.id,
            actor,
            frontmatter,
            body,
            0,
            body.len(),
        )?;
        let next = format!("{frontmatter}{body_markdown}");
        self.write_page_source_update(
            record,
            source,
            before_hash,
            next,
            "wiki.page.write_body",
            "page.body_written",
            actor,
        )
    }

    pub fn patch_page_body(
        &self,
        reference: &str,
        find: &str,
        replace: &str,
        expected_source_sha256: Option<&str>,
        actor: &Actor,
    ) -> Result<OperationReceipt> {
        if find.is_empty() {
            return Err(anyhow!("page-patch-body requires a non-empty --find value"));
        }
        self.ensure_runtime_dirs()?;
        let _lifecycle_lock = self.lock_lifecycle_writes()?;
        let record = self.find_page(reference)?;
        self.ensure_page_body_editable(&record)?;
        let source = self.source_path(&record);
        let before_hash = sha256_file(&source)?;
        if let Some(expected) = expected_source_sha256 {
            if expected != before_hash {
                return Err(anyhow!(
                    "source hash mismatch for {}; expected {}, found {}",
                    record.id,
                    expected,
                    before_hash
                ));
            }
        }
        let original =
            fs::read_to_string(&source).with_context(|| format!("read {}", source.display()))?;
        let (frontmatter, body) = split_markdown_frontmatter(&original)?;
        let occurrences = body.matches(find).count();
        match occurrences {
            0 => {
                return Err(anyhow!(
                    "patch text not found in body for {}; use page-open to inspect the current source",
                    record.id
                ))
            }
            1 => {}
            _ => {
                return Err(anyhow!(
                    "patch text matched {occurrences} times in body for {}; make --find more specific or use page-write-body",
                    record.id
                ))
            }
        }
        let find_start = body.find(find).expect("single occurrence verified above");
        enforce_operator_touched_spans(
            "wiki.page.patch_body",
            &record.id,
            actor,
            frontmatter,
            body,
            find_start,
            find_start + find.len(),
        )?;
        let patched_body = body.replacen(find, replace, 1);
        let next = format!("{frontmatter}{patched_body}");
        self.write_page_source_update(
            record,
            source,
            before_hash,
            next,
            "wiki.page.patch_body",
            "page.body_patched",
            actor,
        )
    }

    /// First-class additive edit: append a markdown block at the end of one
    /// section (located by its ATX heading text) without rewriting any
    /// existing line. This is the e08 "additive discipline" made mechanical:
    /// the insertion point lands after a section's operator-touched spans, so
    /// agent appends succeed where rewrites are rejected. An unclosed span in
    /// the section still protects through the end of the body.
    pub fn append_to_section(
        &self,
        reference: &str,
        section: &str,
        content: &str,
        expected_source_sha256: Option<&str>,
        actor: &Actor,
    ) -> Result<OperationReceipt> {
        if content.trim().is_empty() {
            return Err(anyhow!(
                "page-append-section requires non-empty --content markdown"
            ));
        }
        self.ensure_runtime_dirs()?;
        let _lifecycle_lock = self.lock_lifecycle_writes()?;
        let record = self.find_page(reference)?;
        self.ensure_page_body_editable(&record)?;
        let source = self.source_path(&record);
        let before_hash = sha256_file(&source)?;
        if let Some(expected) = expected_source_sha256 {
            if expected != before_hash {
                return Err(anyhow!(
                    "source hash mismatch for {}; expected {}, found {}",
                    record.id,
                    expected,
                    before_hash
                ));
            }
        }
        let original =
            fs::read_to_string(&source).with_context(|| format!("read {}", source.display()))?;
        let (frontmatter, body) = split_markdown_frontmatter(&original)?;
        let (_, section_end) = locate_section(body, &record.id, section)?;
        enforce_operator_touched_spans(
            "wiki.page.append_section",
            &record.id,
            actor,
            frontmatter,
            body,
            section_end,
            section_end,
        )?;
        let next_body = append_block_at(body, section_end, content);
        let next = format!("{frontmatter}{next_body}");
        self.write_page_source_update(
            record,
            source,
            before_hash,
            next,
            "wiki.page.append_section",
            "page.section_appended",
            actor,
        )
    }

    pub fn delete_page(&self, reference: &str, mode: &str) -> Result<OperationReceipt> {
        if mode != "tombstone" {
            return Err(anyhow!(
                "unsupported page-delete mode: {mode}; expected tombstone"
            ));
        }
        self.ensure_runtime_dirs()?;
        let _lifecycle_lock = self.lock_lifecycle_writes()?;
        let record = self.find_page(reference)?;
        let link_impact = self.inbound_link_impact_for_delete(&record)?;
        let source = self.source_path(&record);
        let tombstone = source.with_extension("tombstone.toml");
        let source_hash = sha256_file(&source).ok();
        let content = format!(
            "schema_version = 1\npage_id = \"{}\"\nroute = \"{}\"\nmode = \"tombstone\"\ncreated_at = \"{}\"\nsource_sha256 = \"{}\"\n",
            record.id,
            record.route,
            now_rfc3339(),
            source_hash.clone().unwrap_or_default()
        );
        let mut evidence = Vec::new();
        write_if_missing(&self.paths.root, &tombstone, content, &mut evidence)?;
        self.retire_page_from_registry(&record.id, &mut evidence)?;
        let changed = evidence.iter().any(|entry| {
            !matches!(
                entry.status.as_str(),
                "skipped_existing" | "navigation_already_retired"
            )
        });
        if changed {
            self.append_page_event(PageLedgerEvent {
                schema_version: 1,
                event: "page.tombstoned".to_string(),
                page: record.id.clone(),
                at: now_rfc3339(),
                actor: None,
                origin: Some("tombstone".to_string()),
                source_sha256: source_hash,
                template_sha256: None,
                publish_fingerprint: None,
            })?;
        }
        let mut repair_hints =
            vec!["Render after tombstone if the deleted route is still published.".to_string()];
        if link_impact.inbound_link_count > 0 {
            repair_hints.insert(
                0,
                format!(
                    "Repair {} inbound source link(s) to {} before or after publishing the tombstone.",
                    link_impact.inbound_link_count, link_impact.deleted_route
                ),
            );
        }
        let id = record.id.clone();
        let (page_status, edit, hashes) = self.page_receipt_context(&id);
        let (route, page_type, collection) = page_receipt_identity(&page_status);
        let route_still_published = self.route_exists(&record.route);
        let next_action = match (route_still_published, link_impact.inbound_link_count > 0) {
            (true, true) => "publish_then_repair_links",
            (true, false) => "publish",
            (false, true) => "repair_links",
            (false, false) => "none",
        };
        Ok(OperationReceipt {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.page.delete".to_string(),
            id,
            route,
            page_type,
            collection,
            actor: None,
            evidence,
            page_status,
            edit,
            hashes,
            before_source_sha256: None,
            after_source_sha256: None,
            link_impact: Some(link_impact),
            render_required: route_still_published,
            next_action: next_action.to_string(),
            repair_hints,
        })
    }

    pub fn restore_page(&self, reference: &str) -> Result<OperationReceipt> {
        self.ensure_runtime_dirs()?;
        let _lifecycle_lock = self.lock_lifecycle_writes()?;
        let record = self.find_page(reference)?;
        let source = self.source_path(&record);
        if !source.is_file() {
            return Err(anyhow!(
                "page-restore refused for {} because source is missing; recreate with wiki.page.create instead",
                record.id
            ));
        }

        let tombstone = source.with_extension("tombstone.toml");
        let source_hash = sha256_file(&source).ok();
        let mut evidence = Vec::new();
        let mut changed = false;
        if tombstone.is_file() {
            fs::remove_file(&tombstone)
                .with_context(|| format!("remove {}", tombstone.display()))?;
            evidence.push(Evidence {
                path: display_under_root(&tombstone, &self.paths.root),
                status: "removed_tombstone".to_string(),
            });
            changed = true;
        } else {
            evidence.push(Evidence {
                path: display_under_root(&tombstone, &self.paths.root),
                status: "no_tombstone".to_string(),
            });
        }

        if self.restore_page_to_registry(&record, &mut evidence)? {
            changed = true;
        }

        if changed {
            self.append_page_event(PageLedgerEvent {
                schema_version: 1,
                event: "page.restored".to_string(),
                page: record.id.clone(),
                at: now_rfc3339(),
                actor: None,
                origin: Some("restore".to_string()),
                source_sha256: source_hash,
                template_sha256: None,
                publish_fingerprint: None,
            })?;
        }

        let id = record.id.clone();
        let (page_status, edit, hashes) = self.page_receipt_context(&id);
        let (route, page_type, collection) = page_receipt_identity(&page_status);
        Ok(OperationReceipt {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.page.restore".to_string(),
            id,
            route,
            page_type,
            collection,
            actor: None,
            evidence,
            page_status,
            edit,
            hashes,
            before_source_sha256: None,
            after_source_sha256: None,
            link_impact: None,
            render_required: changed,
            next_action: if changed {
                "publish".to_string()
            } else {
                "none".to_string()
            },
            repair_hints: if changed {
                vec!["Publish after restore so the route and navigation return to the rendered site.".to_string()]
            } else {
                Vec::new()
            },
        })
    }

    pub fn append_talk(&self, request: TalkAppendRequest) -> Result<TalkAppendResult> {
        self.ensure_runtime_dirs()?;
        let record = self
            .pages()?
            .into_iter()
            .find(|page| page.id == request.page || page.route == request.page)
            .ok_or_else(|| anyhow!("unknown page: {}", request.page))?;
        let status = self.page_status(&record.id)?;
        if !request.allow_tombstoned && matches!(status.state.as_str(), "tombstoned" | "disabled") {
            return Err(anyhow!(
                "talk append refused for {} page {}; pass --allow-tombstoned to append archive-maintenance talk",
                status.state,
                record.id
            ));
        }
        let talk_dir = self.talk_dir(&record);
        if !talk_dir.exists() {
            return Err(anyhow!(
                "talk folder missing for page {}; call wiki.page.create first",
                record.id
            ));
        }
        let subject_slug = slugify(&request.subject);
        let operation_id = request
            .operation_id
            .as_deref()
            .map(|value| validate_talk_target_id(value, "--operation-id"))
            .transpose()?;
        let stamp = operation_id
            .as_deref()
            .map(|id| format!("op-{}", short_hash(id)))
            .unwrap_or_else(now_file_stamp);
        let message_id = operation_id
            .as_deref()
            .map(|id| format!("talkmsg_op_{}", short_hash(id)))
            .unwrap_or_else(|| {
                format!(
                    "talkmsg_{}_{}",
                    stamp.replace('-', ""),
                    short_hash(&request.subject)
                )
            });
        let default_thread_id = format!("thread_{}_{}", record.id, subject_slug);
        let thread_target =
            self.resolve_talk_thread_target(&record, &request, default_thread_id)?;
        let thread_id = thread_target.thread_id;
        let reply_to = thread_target.reply_to;
        let filename = format!("{stamp}.{}.{}.md", request.kind, subject_slug);
        let source_path = talk_dir.join(filename);
        let created_at = existing_talk_created_at(&source_path)?.unwrap_or_else(now_rfc3339);
        let mut recipients = request
            .to
            .iter()
            .chain(request.cc.iter())
            .map(|recipient| recipient.trim().to_string())
            .filter(|recipient| !recipient.is_empty())
            .collect::<Vec<_>>();
        recipients.sort();
        recipients.dedup();
        let attachments =
            self.copy_talk_attachments(&talk_dir, &record.id, &message_id, &request.attachments)?;
        let frontmatter_recipients = if recipients.is_empty() {
            "recipients: []".to_string()
        } else {
            format!(
                "recipients:\n{}",
                recipients
                    .iter()
                    .map(|recipient| format!("  - \"{}\"", escape_yaml(recipient)))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        let frontmatter_attachments = if attachments.is_empty() {
            "attachments: []".to_string()
        } else {
            format!(
                "attachments:\n{}",
                attachments
                    .iter()
                    .map(|attachment| {
                        let mut lines = vec![
                            format!("  - filename: \"{}\"", escape_yaml(&attachment.filename)),
                            format!(
                                "    media_type: \"{}\"",
                                escape_yaml(&attachment.media_type)
                            ),
                            format!("    path: \"{}\"", escape_yaml(&attachment.path)),
                            format!("    handle: \"{}\"", escape_yaml(&attachment.handle)),
                        ];
                        if let Some(caption) = &attachment.caption {
                            lines.push(format!("    caption: \"{}\"", escape_yaml(caption)));
                        }
                        if let Some(alt_text) = &attachment.alt_text {
                            lines.push(format!("    alt_text: \"{}\"", escape_yaml(alt_text)));
                        }
                        lines.join("\n")
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        let frontmatter_reply_to = reply_to
            .as_ref()
            .map(|target| format!("reply_to: \"{}\"\n", escape_yaml(target)))
            .unwrap_or_default();
        let frontmatter_operation_id = operation_id
            .as_ref()
            .map(|target| format!("operation_id: \"{}\"\n", escape_yaml(target)))
            .unwrap_or_default();
        let attachment_section = if attachments.is_empty() {
            String::new()
        } else {
            format!(
                "\n\n## Attachments\n\n{}",
                attachments
                    .iter()
                    .map(|attachment| {
                        let caption = attachment
                            .caption
                            .as_deref()
                            .map(|value| format!(" - {value}"))
                            .unwrap_or_default();
                        format!(
                            "- [{}]({}) ({}){}",
                            attachment.filename,
                            talk_attachment_href(&record, &attachment.path),
                            attachment.media_type,
                            caption
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        };
        let body = format!(
            "---\nid: \"{}\"\n{}kind: \"{}\"\nauthor: \"{}\"\ncreated: \"{}\"\ntalk_for: \"mailbox://page/{}\"\nthread: \"{}\"\n{}subject: \"{}\"\nstate: open\n{}\n{}\n---\n\n## {}\n\n{}{}\n",
            message_id,
            frontmatter_operation_id,
            request.kind,
            request.from,
            created_at,
            record.id,
            thread_id,
            frontmatter_reply_to,
            escape_yaml(&request.subject),
            frontmatter_recipients,
            frontmatter_attachments,
            request.subject,
            request.body_markdown,
            attachment_section
        );
        if source_path.exists() && operation_id.is_some() {
            // Stable operation ids are idempotent for retry-after-talk-write cases.
        } else {
            write_new(&source_path, body)?;
        }

        let source_handle = format!(
            "user-wiki://page/{}/talk/messages/{}",
            record.id, message_id
        );
        let (status, mail_delivery, repair_hints) = if request.delivery_mode
            == TalkDeliveryMode::Mail
        {
            let mail_result = self.send_talk_mail(
                &request,
                &record,
                &message_id,
                &thread_id,
                reply_to.as_deref(),
                &created_at,
                operation_id.as_deref(),
                &attachments,
                &talk_dir,
            );
            match mail_result {
                Ok(delivery) if delivery.status == "delivered" => {
                    ("appended".to_string(), Some(delivery), Vec::new())
                }
                Ok(delivery) => (
                    "appended_delivery_deferred".to_string(),
                    Some(delivery),
                    vec![
                        "Talk source was preserved; Agent Mail did not create any delivered recipient records yet."
                            .to_string(),
                    ],
                ),
                Err(error) => (
                    "appended_delivery_failed".to_string(),
                    Some(TalkMailDeliveryResult {
                        status: "failed".to_string(),
                        acceptance: None,
                        attempt_count: 0,
                        attempts: Vec::new(),
                        error: Some(format!("{error:#}")),
                    }),
                    vec![
                        "Talk source was preserved; repair the mail request and retry explicit delivery."
                            .to_string(),
                    ],
                ),
            }
        } else {
            ("appended".to_string(), None, Vec::new())
        };
        Ok(TalkAppendResult {
            schema_version: 1,
            status,
            operation: "wiki.talk.append".to_string(),
            surface: "page_talk".to_string(),
            message_id,
            thread_id,
            reply_to,
            page_id: record.id,
            route: record.route,
            kind: request.kind,
            subject: request.subject,
            created_at,
            source: source_handle,
            delivery_mode: request.delivery_mode,
            mail_delivery,
            attachment_count: attachments.len(),
            attachments,
            render_required: false,
            next_action: "none".to_string(),
            repair_hints,
        })
    }

    fn send_talk_mail(
        &self,
        request: &TalkAppendRequest,
        record: &PageRecord,
        message_id: &str,
        thread_id: &str,
        reply_to: Option<&str>,
        created_at: &str,
        operation_id: Option<&str>,
        attachments: &[TalkAttachmentRecord],
        talk_dir: &Path,
    ) -> Result<TalkMailDeliveryResult> {
        let mail_attachments = attachments
            .iter()
            .map(|attachment| {
                let path = talk_dir.join(&attachment.path);
                Ok(agent_mail::MessageAttachmentRef {
                    filename: attachment.filename.clone(),
                    media_type: attachment.media_type.clone(),
                    sha256: sha256_file(&path)?,
                    handle: attachment.handle.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let envelope = agent_mail::MessageEnvelope {
            schema_version: 1,
            message_id: message_id.to_string(),
            idempotency_key: operation_id
                .map(ToString::to_string)
                .unwrap_or_else(|| message_id.to_string()),
            kind: request.kind.clone(),
            subject: request.subject.clone(),
            from: request.from.clone(),
            to: request.to.clone(),
            cc: request.cc.clone(),
            page: Some(agent_mail::MessagePageRef {
                id: record.id.clone(),
                route: record.route.clone(),
            }),
            thread_id: thread_id.to_string(),
            reply_to: reply_to.map(ToString::to_string),
            body: agent_mail::MessageBodyRef {
                format: "markdown".to_string(),
                sha256: sha256_bytes(request.body_markdown.as_bytes()),
            },
            attachments: mail_attachments,
            created_at: created_at.to_string(),
        };
        let receipt = agent_mail::AgentMailStore::new(&self.paths.context_engine_live).send_mail(
            &envelope,
            &request.body_markdown,
            &agent_mail::SendMailOptions::default(),
        )?;
        let delivered_count = receipt
            .attempts
            .iter()
            .filter(|attempt| {
                matches!(
                    attempt.status,
                    agent_mail::DeliveryAttemptStatus::Delivered
                        | agent_mail::DeliveryAttemptStatus::AlreadyDelivered
                )
            })
            .count();
        let deferred_count = receipt
            .attempts
            .iter()
            .filter(|attempt| {
                matches!(
                    attempt.status,
                    agent_mail::DeliveryAttemptStatus::DeferredCapacity
                )
            })
            .count();
        let delivery_status = if delivered_count > 0 {
            "delivered"
        } else if deferred_count > 0 {
            "deferred_capacity"
        } else {
            "no_deliveries"
        };
        Ok(TalkMailDeliveryResult {
            status: delivery_status.to_string(),
            acceptance: Some(match receipt.acceptance {
                agent_mail::MessageAcceptance::Accepted => "accepted".to_string(),
                agent_mail::MessageAcceptance::DuplicateSamePayload { .. } => {
                    "duplicate_same_payload".to_string()
                }
            }),
            attempt_count: receipt.attempts.len(),
            attempts: receipt
                .attempts
                .into_iter()
                .map(|attempt| TalkMailDeliveryAttempt {
                    recipient: attempt.recipient,
                    delivery_id: attempt.delivery_id,
                    status: match attempt.status {
                        agent_mail::DeliveryAttemptStatus::Delivered => "delivered".to_string(),
                        agent_mail::DeliveryAttemptStatus::AlreadyDelivered => {
                            "already_delivered".to_string()
                        }
                        agent_mail::DeliveryAttemptStatus::DeferredCapacity => {
                            "deferred_capacity".to_string()
                        }
                    },
                })
                .collect(),
            error: None,
        })
    }

    fn resolve_talk_thread_target(
        &self,
        record: &PageRecord,
        request: &TalkAppendRequest,
        default_thread_id: String,
    ) -> Result<TalkThreadTarget> {
        let explicit_thread_id = request
            .thread_id
            .as_deref()
            .map(|value| validate_talk_target_id(value, "--thread-id"))
            .transpose()?;
        let reply_to = request
            .reply_to
            .as_deref()
            .map(|value| validate_talk_target_id(value, "--reply-to"))
            .transpose()?;

        if explicit_thread_id.is_none() && reply_to.is_none() {
            return Ok(TalkThreadTarget {
                thread_id: default_thread_id,
                reply_to: None,
            });
        }

        let targets = self.all_talk_message_targets()?;
        let parent = if let Some(reply_to) = reply_to.as_ref() {
            let parent = targets
                .iter()
                .find(|target| target.message_id == reply_to.as_str())
                .ok_or_else(|| {
                    anyhow!("talk reply target not found in page talk files: {reply_to}")
                })?;
            if parent.page_id != record.id {
                return Err(anyhow!(
                    "talk reply target {reply_to} belongs to page {}; expected page {}",
                    parent.page_id,
                    record.id
                ));
            }
            Some(parent)
        } else {
            None
        };

        let thread_id = if let Some(thread_id) = explicit_thread_id {
            let thread_exists_on_current_page = targets
                .iter()
                .any(|target| target.page_id == record.id && target.thread_id == thread_id);
            let thread_exists_on_other_page = targets
                .iter()
                .any(|target| target.page_id != record.id && target.thread_id == thread_id);
            if !thread_exists_on_current_page && thread_exists_on_other_page {
                return Err(anyhow!(
                    "talk thread target belongs to a different page; page {} cannot append to {thread_id}",
                    record.id
                ));
            }
            if let Some(parent) = parent {
                if parent.thread_id != thread_id {
                    return Err(anyhow!(
                        "talk reply target {} belongs to thread {}; expected thread {}",
                        parent.message_id,
                        parent.thread_id,
                        thread_id
                    ));
                }
            }
            thread_id.to_string()
        } else if let Some(parent) = parent {
            parent.thread_id.clone()
        } else {
            default_thread_id
        };

        Ok(TalkThreadTarget {
            thread_id,
            reply_to,
        })
    }

    fn all_talk_message_targets(&self) -> Result<Vec<TalkMessageTarget>> {
        let mut targets = BTreeMap::<String, TalkMessageTarget>::new();
        for record in self.pages()? {
            for target in self.file_talk_message_targets(&record)? {
                targets.entry(target.message_id.clone()).or_insert(target);
            }
        }
        Ok(targets.into_values().collect())
    }

    fn file_talk_message_targets(&self, record: &PageRecord) -> Result<Vec<TalkMessageTarget>> {
        let talk_dir = self.talk_dir(record);
        if !talk_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut targets = Vec::new();
        for entry in fs::read_dir(&talk_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
                continue;
            }
            let text = fs::read_to_string(&path)
                .with_context(|| format!("read talk message {}", path.display()))?;
            let fields = match parse_simple_frontmatter_fields(&text) {
                Ok(fields) => fields,
                Err(_) => continue,
            };
            let Some(message_id) = fields.get("id").filter(|id| talk_target_id_is_valid(id)) else {
                continue;
            };
            let (_, body) = match split_markdown_frontmatter(&text) {
                Ok(parts) => parts,
                Err(_) => continue,
            };
            targets.push(TalkMessageTarget {
                message_id: message_id.clone(),
                thread_id: talk_file_thread_id(record, &fields, body, message_id),
                page_id: record.id.clone(),
            });
        }
        Ok(targets)
    }

    #[cfg(test)]
    fn file_talk_messages(&self, record: &PageRecord) -> Result<Vec<TalkFileMessage>> {
        let talk_dir = self.talk_dir(record);
        if !talk_dir.is_dir() {
            return Ok(Vec::new());
        }

        let mut messages = Vec::new();
        for entry in fs::read_dir(&talk_dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("md") {
                continue;
            }
            let text = fs::read_to_string(&path)
                .with_context(|| format!("read talk message {}", path.display()))?;
            let fields = match parse_simple_frontmatter_fields(&text) {
                Ok(fields) => fields,
                Err(_) => continue,
            };
            let Some(message_id) = fields.get("id").filter(|id| talk_target_id_is_valid(id)) else {
                continue;
            };
            let (_, body) = match split_markdown_frontmatter(&text) {
                Ok(parts) => parts,
                Err(_) => continue,
            };
            let subject = talk_file_subject(&fields, body, message_id);
            let reply_to = fields
                .get("reply_to")
                .filter(|reply_to| talk_target_id_is_valid(reply_to))
                .cloned();
            messages.push(TalkFileMessage {
                message_id: message_id.clone(),
                thread_id: talk_file_thread_id(record, &fields, body, message_id),
                reply_to,
                subject,
                attachments: parse_talk_file_attachments(&text)?,
            });
        }
        Ok(messages)
    }

    fn copy_talk_attachments(
        &self,
        talk_dir: &Path,
        page_id: &str,
        message_id: &str,
        inputs: &[TalkAttachmentInput],
    ) -> Result<Vec<TalkAttachmentRecord>> {
        if inputs.is_empty() {
            return Ok(Vec::new());
        }

        let mut used = BTreeSet::new();
        let mut pending = Vec::new();

        for input in inputs {
            let source = PathBuf::from(&input.path);
            if !source.is_file() {
                return Err(anyhow!("attachment not found: {}", input.path));
            }
            let requested_name = input
                .filename
                .as_deref()
                .or_else(|| source.file_name().and_then(|name| name.to_str()))
                .ok_or_else(|| anyhow!("attachment filename missing for {}", input.path))?;
            let filename =
                unique_attachment_filename(safe_attachment_filename(requested_name)?, &mut used);
            let relative_path = format!("attachments/{message_id}/{filename}");
            pending.push((
                source,
                filename.clone(),
                TalkAttachmentRecord {
                    media_type: infer_media_type(&filename).to_string(),
                    handle: format!(
                        "user-wiki://page/{page_id}/talk/attachments/{message_id}/{filename}"
                    ),
                    filename,
                    path: relative_path,
                    caption: input.caption.clone(),
                    alt_text: input.alt_text.clone(),
                },
            ));
        }

        let attachment_dir = talk_dir.join("attachments").join(message_id);
        fs::create_dir_all(&attachment_dir)
            .with_context(|| format!("create {}", attachment_dir.display()))?;

        for (source, filename, _) in &pending {
            let destination = attachment_dir.join(filename);
            let mut input = File::open(source)
                .with_context(|| format!("open attachment {}", source.display()))?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)
                .with_context(|| format!("create attachment {}", destination.display()))?;
            std::io::copy(&mut input, &mut output).with_context(|| {
                format!(
                    "copy attachment {} to {}",
                    source.display(),
                    destination.display()
                )
            })?;
        }

        Ok(pending.into_iter().map(|(_, _, record)| record).collect())
    }

    fn page_status_from_record(
        &self,
        record: &PageRecord,
        ledger: &[PageLedgerEvent],
        stale: bool,
    ) -> Result<WikiPageStatus> {
        let source = self.source_path(record);
        let tombstone = source.with_extension("tombstone.toml");
        let talk = self.talk_dir(record);
        let source_exists = source.exists();
        let tombstoned = tombstone.exists();
        let talk_ready = talk.join("_meta.yaml").exists()
            && talk.join("_conventions.md").exists()
            && talk.join("_curator.md").exists();
        let rendered = self.route_exists(&record.route);
        let origin = ledger
            .iter()
            .rev()
            .find(|event| event.page == record.id && event.event == "page.created")
            .and_then(|event| event.origin.clone())
            .unwrap_or_else(|| record.origin.clone());
        let baseline_source_sha256 = ledger
            .iter()
            .rev()
            .find(|event| event.page == record.id && event.event == "template.baseline")
            .and_then(|event| event.source_sha256.clone());
        let baseline_template_sha256 = ledger
            .iter()
            .rev()
            .find(|event| event.page == record.id && event.event == "template.baseline")
            .and_then(|event| event.template_sha256.clone());
        let source_hash = if source_exists {
            sha256_file(&source).ok()
        } else {
            None
        };
        let template_content_state = if !source_exists {
            "missing_source"
        } else if source_hash.is_none() {
            "unknown"
        } else if let Some(baseline_source_sha256) = &baseline_source_sha256 {
            if Some(baseline_source_sha256) == source_hash.as_ref() {
                "template_unedited"
            } else {
                "edited"
            }
        } else if page_origin_is_template_derived(&origin) {
            "edited"
        } else {
            "unknown"
        };
        let content_state = if tombstoned {
            "tombstoned"
        } else {
            template_content_state
        };
        let state = if tombstoned {
            "tombstoned"
        } else if !record.enabled {
            "disabled"
        } else if !source_exists {
            "source_missing"
        } else if !talk_ready {
            "talk_missing"
        } else if stale || !rendered {
            "needs_publish"
        } else {
            "rendered"
        };
        let links = self
            .last_link_summary_for_page(record)
            .unwrap_or_else(|_| PageLinkSummary {
                status: "unknown".to_string(),
                broken_internal_count: 0,
                broken_internal_targets: Vec::new(),
                checked_against: None,
                issues: Vec::new(),
                repair_tasks: Vec::new(),
            });
        let state_issue_count = match state {
            "rendered" => 0,
            "tombstoned" | "disabled" => usize::from(rendered),
            _ => 1,
        };
        let blocking_count = 0;
        let link_issue_count = links.broken_internal_count;
        let issue_count = state_issue_count + link_issue_count;
        Ok(WikiPageStatus {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.page.status".to_string(),
            surface: "wiki_page_status".to_string(),
            id: record.id.clone(),
            title: record.title.clone(),
            route: record.route.clone(),
            nav_section: record.nav_section.clone(),
            nav_order: record.nav_order,
            page_type: record.page_type.clone(),
            collection: record.family_group.clone(),
            kind: "source_page".to_string(),
            state: state.to_string(),
            content_state: content_state.to_string(),
            origin: origin.clone(),
            template_state: template_state(&origin, template_content_state),
            template: self.template_summary(&record.template, baseline_template_sha256),
            dirty_since_publish: stale,
            talk_state: if talk_ready { "ready" } else { "missing" }.to_string(),
            flags: PageFlags {
                configured: true,
                enabled: record.enabled,
                source_backed: source_exists,
                rendered,
                stale,
                tombstoned,
                talk_ready,
                template_derived: page_origin_is_template_derived(&origin),
                runtime_default: origin == "runtime_default",
                custom_created: origin == "created_from_template",
                user_edited: template_content_state == "edited",
            },
            handles: PageHandles {
                source: format!("user-wiki://page/{}/source", record.id),
                talk: format!("user-wiki://page/{}/talk", record.id),
                curator: format!("user-wiki://page/{}/curator", record.id),
                conventions: format!("user-wiki://page/{}/conventions", record.id),
                published: format!("app-support://wiki{}", record.route),
            },
            links,
            validation: ValidationSummary {
                status: if issue_count == 0 { "ok" } else { "warning" }.to_string(),
                issue_count,
                blocking_count,
                warning_count: issue_count.saturating_sub(blocking_count),
                highest_severity: (issue_count > 0).then(|| {
                    if blocking_count > 0 {
                        "error".to_string()
                    } else {
                        "warning".to_string()
                    }
                }),
            },
            allowed_actions: page_allowed_actions(state, rendered),
            next_action: match state {
                "source_missing" => "publish",
                "needs_publish" | "talk_missing" => "publish",
                "rendered" if link_issue_count > 0 => "repair_links",
                "rendered" => "none",
                "tombstoned" | "disabled" if rendered => "publish",
                "tombstoned" | "disabled" => "none",
                _ => "validate",
            }
            .to_string(),
        })
    }

    fn site_page_status_from_record(&self, record: &SitePageRecord) -> WikiPageStatus {
        let rendered = self.route_exists(&record.route);
        let state = if !record.enabled {
            "disabled"
        } else if rendered {
            "rendered"
        } else {
            "needs_publish"
        };
        let content_state = if rendered {
            "generated"
        } else {
            "missing_render"
        };
        let next_action = if state == "rendered" {
            "none"
        } else {
            "publish"
        };
        WikiPageStatus {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.page.status".to_string(),
            surface: "wiki_page_status".to_string(),
            id: record.id.clone(),
            title: record.title.clone(),
            route: record.route.clone(),
            nav_section: record.nav_section.clone(),
            nav_order: record.nav_order,
            page_type: record.kind.clone(),
            collection: "site".to_string(),
            kind: "generated_site_page".to_string(),
            state: state.to_string(),
            content_state: content_state.to_string(),
            origin: "generated_site_page".to_string(),
            template_state: "generated_from_template".to_string(),
            template: self.template_summary(&record.template, None),
            dirty_since_publish: state == "needs_publish",
            talk_state: "not_applicable".to_string(),
            flags: PageFlags {
                configured: true,
                enabled: record.enabled,
                source_backed: false,
                rendered,
                stale: false,
                tombstoned: false,
                talk_ready: false,
                template_derived: false,
                runtime_default: false,
                custom_created: false,
                user_edited: false,
            },
            handles: PageHandles {
                source: format!("user-wiki://template/{}", record.template),
                talk: "not_applicable".to_string(),
                curator: "not_applicable".to_string(),
                conventions: "not_applicable".to_string(),
                published: format!("app-support://wiki{}", record.route),
            },
            links: PageLinkSummary {
                status: "unknown".to_string(),
                broken_internal_count: 0,
                broken_internal_targets: Vec::new(),
                checked_against: None,
                issues: Vec::new(),
                repair_tasks: Vec::new(),
            },
            validation: ValidationSummary {
                status: "ok".to_string(),
                issue_count: 0,
                blocking_count: 0,
                warning_count: 0,
                highest_severity: None,
            },
            allowed_actions: vec!["wiki.validate".to_string(), "wiki.publish".to_string()],
            next_action: next_action.to_string(),
        }
    }

    fn template_summary(
        &self,
        relative: &str,
        baseline_sha256: Option<String>,
    ) -> PageTemplateSummary {
        let path = self.paths.templates.join(relative);
        PageTemplateSummary {
            relative_path: relative.to_string(),
            path: display_under_root(&path, &self.paths.root),
            sha256: sha256_file(&path).ok(),
            baseline_sha256,
        }
    }

    fn source_path(&self, record: &PageRecord) -> PathBuf {
        self.paths
            .source
            .join("families")
            .join(&record.family_group)
            .join(&record.family_id)
            .join("source")
            .join(format!("{}.md", record.slug))
    }

    fn asset_dir(&self, record: &PageRecord) -> PathBuf {
        self.paths
            .source
            .join("families")
            .join(&record.family_group)
            .join(&record.family_id)
            .join("source")
            .join(format!("{}.assets", record.slug))
    }

    fn page_asset_record(
        &self,
        record: &PageRecord,
        path: &Path,
        purpose: &str,
        caption: Option<&str>,
        alt_text: Option<&str>,
    ) -> Result<PageAssetRecord> {
        let filename = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| anyhow!("invalid page asset path: {}", path.display()))?
            .to_string();
        let media_type = infer_media_type(&filename).to_string();
        let kind = page_asset_kind(&filename, &media_type);
        let content_role = if purpose == "source_file" || kind == "code_file" {
            "source_file"
        } else if kind == "image" {
            "image"
        } else {
            "download"
        };
        let sha256 = sha256_file(path)?;
        let bytes = path.metadata()?.len();
        let source_relative_href = format!("./{}.assets/{}", record.slug, filename);
        let published_href = page_asset_published_href(&record.route, &filename);
        let source_text = fs::read_to_string(self.source_path(record)).unwrap_or_default();
        let referenced = source_text.contains(&source_relative_href)
            || source_text.contains(source_relative_href.trim_start_matches("./"))
            || source_text.contains(&published_href);
        let published = self
            .paths
            .site
            .join(published_href.trim_start_matches('/'))
            .is_file();
        let markdown = if media_type.starts_with("image/") {
            let alt = alt_text.or(caption).unwrap_or(&filename);
            format!("![{}]({})", alt, source_relative_href)
        } else {
            let label = caption.unwrap_or(&filename);
            format!("[{}]({})", label, source_relative_href)
        };
        let handle = format!("user-wiki://page/{}/assets/{}", record.id, filename);
        Ok(PageAssetRecord {
            id: format!("asset_{}_{}", record.id, slug_token(&filename)),
            citation_uri: handle.clone(),
            kind: kind.to_string(),
            filename,
            media_type,
            sha256,
            bytes,
            handle,
            source_path: display_under_root(path, &self.paths.root),
            absolute_path: path.display().to_string(),
            source_relative_href,
            published_href,
            markdown,
            content_role: content_role.to_string(),
            purpose: purpose.to_string(),
            caption: caption.map(str::to_string),
            alt_text: alt_text.map(str::to_string),
            referenced,
            published,
        })
    }

    fn talk_dir(&self, record: &PageRecord) -> PathBuf {
        self.paths
            .source
            .join("families")
            .join(&record.family_group)
            .join(&record.family_id)
            .join("talk")
            .join(format!("{}.talk", record.slug))
    }

    fn route_exists(&self, route: &str) -> bool {
        let trimmed = route.trim_matches('/');
        if trimmed.is_empty() {
            return self.paths.site.join("index.html").exists();
        }
        self.paths.site.join(format!("{trimmed}.html")).exists()
            || self.paths.site.join(trimmed).join("index.html").exists()
    }

    fn inbound_link_impact_for_delete(&self, target: &PageRecord) -> Result<LinkImpactSummary> {
        let deleted_route = canonical_route_target(&target.route).unwrap_or(target.route.clone());
        let mut issues = Vec::new();
        for record in self.pages()? {
            if record.id == target.id || !record.enabled {
                continue;
            }
            let source = self.source_path(&record);
            if !source.is_file() || source.with_extension("tombstone.toml").exists() {
                continue;
            }
            self.collect_inbound_link_issues(
                &mut issues,
                &source,
                &record.id,
                &record.route,
                "pre_delete_source_link_scan",
                &deleted_route,
            )?;

            let talk_dir = self.talk_dir(&record);
            let mut talk_files = Vec::new();
            collect_files(&talk_dir, &mut talk_files)?;
            talk_files.sort();
            let talk_page_id = format!("{}.talk", record.id);
            let talk_route = if record.route == "/" {
                "/talk".to_string()
            } else {
                format!("{}/talk", record.route.trim_end_matches('/'))
            };
            for talk_file in talk_files {
                if talk_file
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| extension == "md")
                    .unwrap_or(false)
                {
                    self.collect_inbound_link_issues(
                        &mut issues,
                        &talk_file,
                        &talk_page_id,
                        &talk_route,
                        "pre_delete_talk_link_scan",
                        &deleted_route,
                    )?;
                }
            }
        }
        issues.sort_by(|a, b| {
            a.source_page_id
                .cmp(&b.source_page_id)
                .then_with(|| a.href.cmp(&b.href))
        });
        let source_page_count = issues
            .iter()
            .map(|issue| issue.source_page_id.clone())
            .collect::<BTreeSet<_>>()
            .len();
        Ok(LinkImpactSummary {
            status: if issues.is_empty() {
                "ok".to_string()
            } else {
                "warning".to_string()
            },
            deleted_markdown_path: markdown_url_for_route(&deleted_route),
            post_publish_expected_next_action: if issues.is_empty() {
                "none".to_string()
            } else {
                "repair_links".to_string()
            },
            deleted_route,
            inbound_link_count: issues.len(),
            source_page_count,
            issues,
        })
    }

    fn collect_inbound_link_issues(
        &self,
        issues: &mut Vec<InboundLinkIssue>,
        path: &Path,
        source_page_id: &str,
        source_route: &str,
        phase: &str,
        deleted_route: &str,
    ) -> Result<()> {
        let markdown =
            fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
        for href in extract_markdown_link_hrefs(&markdown) {
            let Some(normalized) = normalize_wiki_href(&href, source_route) else {
                continue;
            };
            if !link_points_at_deleted_route(&normalized.route_target, deleted_route) {
                continue;
            }
            issues.push(InboundLinkIssue {
                code: "would_break_internal_link".to_string(),
                severity: "warning".to_string(),
                phase: phase.to_string(),
                source_page_id: source_page_id.to_string(),
                source_route: source_route.to_string(),
                source_path: display_under_root(path, &self.paths.root),
                href,
                target: normalized.target,
                target_kind: normalized.target_kind,
                message: "page-delete would remove this internal link target".to_string(),
            });
        }
        Ok(())
    }

    fn find_page(&self, reference: &str) -> Result<PageRecord> {
        let route = if reference.starts_with('/') {
            reference.to_string()
        } else {
            format!("/{reference}")
        };
        self.pages()?
            .into_iter()
            .find(|page| page.id == reference || page.route == route)
            .map(Ok)
            .unwrap_or_else(|| self.unknown_source_page_error(reference))
    }

    fn unknown_source_page_error<T>(&self, reference: &str) -> Result<T> {
        if let Some(site_page) = self.site_page_reference(reference)? {
            return Err(anyhow!(
                "generated site page {} at {} is not source-backed; use wiki.publish/status or inspect the route manifest instead of wiki.page.status/open",
                site_page.id,
                site_page.route
            ));
        }
        Err(anyhow!("unknown configured page: {reference}"))
    }

    fn site_page_reference(&self, reference: &str) -> Result<Option<PageParticipantReference>> {
        let config = read_toml(&self.paths.user_wiki.join("wiki.toml"))?;
        let route = if reference.starts_with('/') {
            reference.to_string()
        } else {
            format!("/{reference}")
        };
        let Some(site_pages) = config.get("site_pages").and_then(Value::as_array) else {
            return Ok(None);
        };
        for site_page in site_pages {
            let id = get_opt_str(site_page, "id").unwrap_or_else(|| "unknown".to_string());
            let title = get_opt_str(site_page, "title").unwrap_or_else(|| id.clone());
            let site_route = get_opt_str(site_page, "route").unwrap_or_else(|| format!("/{id}"));
            if id == reference || site_route == route || site_route == reference {
                return Ok(Some(PageParticipantReference {
                    id,
                    title,
                    route: site_route,
                }));
            }
        }
        Ok(None)
    }

    fn ensure_page_body_editable(&self, record: &PageRecord) -> Result<()> {
        let status = self.page_status(&record.id)?;
        if matches!(status.state.as_str(), "tombstoned" | "disabled") {
            return Err(anyhow!(
                "page body edit refused for {} page {}; use wiki.page.restore before editing or choose an enabled page",
                status.state,
                record.id
            ));
        }
        let source = self.source_path(record);
        if !source.is_file() {
            return Err(anyhow!(
                "page source missing for {}; call wiki.page.create first",
                record.id
            ));
        }
        Ok(())
    }

    fn page_receipt_context(
        &self,
        page_id: &str,
    ) -> (
        Option<WikiPageStatus>,
        Option<EditPreconditions>,
        Option<PageOpenHashes>,
    ) {
        let page_status = self.page_status(page_id).ok();
        let opened = self.open_page(page_id).ok();
        let edit = opened.as_ref().map(|page| page.edit.clone());
        let hashes = opened.map(|page| page.hashes);
        (page_status, edit, hashes)
    }

    fn write_page_source_update(
        &self,
        record: PageRecord,
        source: PathBuf,
        before_hash: String,
        next_content: String,
        operation: &str,
        ledger_event: &str,
        actor: &Actor,
    ) -> Result<OperationReceipt> {
        let original =
            fs::read_to_string(&source).with_context(|| format!("read {}", source.display()))?;
        let mut evidence = Vec::new();
        if original == next_content {
            evidence.push(Evidence {
                path: display_under_root(&source, &self.paths.root),
                status: "unchanged".to_string(),
            });
            let id = record.id.clone();
            let (page_status, edit, hashes) = self.page_receipt_context(&id);
            let (route, page_type, collection) = page_receipt_identity(&page_status);
            return Ok(OperationReceipt {
                schema_version: 1,
                status: "ok".to_string(),
                operation: operation.to_string(),
                id,
                route,
                page_type,
                collection,
                actor: Some(actor.clone()),
                evidence,
                page_status,
                edit,
                hashes,
                before_source_sha256: Some(before_hash.clone()),
                after_source_sha256: Some(before_hash),
                link_impact: None,
                render_required: false,
                next_action: "none".to_string(),
                repair_hints: Vec::new(),
            });
        }

        fs::write(&source, next_content).with_context(|| format!("write {}", source.display()))?;
        let after_hash = sha256_file(&source).ok();
        evidence.push(Evidence {
            path: display_under_root(&source, &self.paths.root),
            status: "updated".to_string(),
        });
        self.append_page_event(PageLedgerEvent {
            schema_version: 1,
            event: ledger_event.to_string(),
            page: record.id.clone(),
            at: now_rfc3339(),
            actor: Some(actor.clone()),
            origin: Some("body_edit".to_string()),
            source_sha256: after_hash.clone(),
            template_sha256: None,
            publish_fingerprint: None,
        })?;
        let id = record.id.clone();
        let (page_status, edit, hashes) = self.page_receipt_context(&id);
        let (route, page_type, collection) = page_receipt_identity(&page_status);
        Ok(OperationReceipt {
            schema_version: 1,
            status: "ok".to_string(),
            operation: operation.to_string(),
            id,
            route,
            page_type,
            collection,
            actor: Some(actor.clone()),
            evidence,
            page_status,
            edit,
            hashes,
            before_source_sha256: Some(before_hash),
            after_source_sha256: after_hash,
            link_impact: None,
            render_required: true,
            next_action: "publish".to_string(),
            repair_hints: Vec::new(),
        })
    }

    fn append_page_registry_entry(
        &self,
        page_id: &str,
        options: &PageCreateOptions,
    ) -> Result<PageRecord> {
        let config_path = self.paths.user_wiki.join("wiki.toml");
        let mut text = fs::read_to_string(&config_path)
            .with_context(|| format!("read {}", config_path.display()))?;
        let existing = self.pages().unwrap_or_default();
        if existing.iter().any(|page| page.id == page_id) {
            return Err(anyhow!("page already exists: {page_id}"));
        }
        let slug = options
            .slug
            .clone()
            .unwrap_or_else(|| slugify(page_id).if_empty("page"));
        validate_page_slug(&slug)?;
        let title = options.title.clone().unwrap_or_else(|| title_case(&slug));
        let route = options.route.clone().unwrap_or_else(|| format!("/{slug}"));
        validate_page_route(&route)?;
        if let Some(nav_section) = options.nav_section.as_deref() {
            validate_nav_section(nav_section)?;
        }
        let family_group = options
            .family_group
            .clone()
            .or_else(|| options.nav_section.clone())
            .unwrap_or_else(|| "custom".to_string());
        let family_id = options.family_id.clone().unwrap_or_else(|| slug.clone());
        validate_family_path_token(&family_group, "family_group")?;
        validate_family_path_token(&family_id, "family_id")?;
        if let Some(conflict) = existing.iter().find(|page| {
            page.family_group == family_group && page.family_id == family_id && page.slug == slug
        }) {
            return Err(anyhow!(
                "page source path already exists for slug {slug} in family {family_group}/{family_id}; owned by page {}",
                conflict.id
            ));
        }
        let config = read_toml(&config_path)?;
        let reserved_routes = reserved_routes_for_create(&existing, &config);
        let route_key = route_conflict_key(&route);
        if let Some(owner) = reserved_routes.get(&route_key) {
            return Err(anyhow!(
                "route already exists: {route_key}; owned by {owner}"
            ));
        }
        let talk_route = talk_route_for_route(&route);
        if let Some(owner) = reserved_routes.get(&talk_route) {
            return Err(anyhow!(
                "route already exists: {talk_route}; reserved by {owner}. Choose a page route whose talk route does not collide."
            ));
        }
        let record = PageRecord {
            id: page_id.to_string(),
            enabled: true,
            title: title.clone(),
            slug: slug.clone(),
            route: route.clone(),
            family_group: family_group.clone(),
            family_group_title: options
                .family_group_title
                .clone()
                .unwrap_or_else(|| title_case(&family_group)),
            family_id: family_id.clone(),
            family_title: options
                .family_title
                .clone()
                .unwrap_or_else(|| title.clone()),
            page_type: options
                .page_type
                .clone()
                .unwrap_or_else(|| "context-page".to_string()),
            template: options
                .template
                .clone()
                .unwrap_or_else(|| "pages/context-page.md".to_string()),
            talk_conventions_template: Some(
                options
                    .talk_conventions_template
                    .clone()
                    .unwrap_or_else(|| "talk/conventions.md".to_string()),
            ),
            talk_curator_template: Some(
                options
                    .talk_curator_template
                    .clone()
                    .unwrap_or_else(|| "talk/curators/your-context.md".to_string()),
            ),
            summary: options.summary.clone(),
            nav_section: options.nav_section.clone(),
            nav_order: options.nav_order,
            origin: "created_from_template".to_string(),
        };
        self.validate_page_source_render_contract(&record)?;

        if options.nav_section.as_deref() != Some("hidden") {
            text = append_to_toml_string_array(&text, "navigation", &record.id);
            match options.nav_section.as_deref() {
                Some("utility") => {
                    text = append_to_toml_string_array(&text, "utility_navigation", &record.id)
                }
                _ => text = append_to_toml_string_array(&text, "primary_navigation", &record.id),
            }
        }

        text.push_str("\n\n[[pages]]\n");
        text.push_str(&format!("id = {}\n", toml_quote(&record.id)));
        text.push_str("enabled = true\n");
        text.push_str(&format!("title = {}\n", toml_quote(&record.title)));
        text.push_str(&format!("slug = {}\n", toml_quote(&record.slug)));
        text.push_str(&format!("route = {}\n", toml_quote(&record.route)));
        text.push_str(&format!(
            "family_group = {}\n",
            toml_quote(&record.family_group)
        ));
        text.push_str(&format!(
            "family_group_title = {}\n",
            toml_quote(&record.family_group_title)
        ));
        text.push_str(&format!("family_id = {}\n", toml_quote(&record.family_id)));
        text.push_str(&format!(
            "family_title = {}\n",
            toml_quote(&record.family_title)
        ));
        text.push_str(&format!("type = {}\n", toml_quote(&record.page_type)));
        text.push_str(&format!("origin = {}\n", toml_quote(&record.origin)));
        text.push_str(&format!("template = {}\n", toml_quote(&record.template)));
        if let Some(template) = &record.talk_conventions_template {
            text.push_str(&format!(
                "talk_conventions_template = {}\n",
                toml_quote(template)
            ));
        }
        if let Some(template) = &record.talk_curator_template {
            text.push_str(&format!(
                "talk_curator_template = {}\n",
                toml_quote(template)
            ));
        }
        if let Some(summary) = &record.summary {
            text.push_str(&format!("summary = {}\n", toml_quote(summary)));
        }
        if let Some(nav_order) = options.nav_order {
            text.push_str(&format!("nav_order = {nav_order}\n"));
        }
        if let Some(nav_section) = &record.nav_section {
            text.push_str(&format!("nav_section = {}\n", toml_quote(nav_section)));
        }

        write_text_atomic(&config_path, &text)?;
        Ok(record)
    }

    fn validate_new_page_templates(&self, options: &PageCreateOptions) -> Result<()> {
        let source_template = options
            .template
            .as_deref()
            .unwrap_or("pages/context-page.md");
        self.read_template(source_template)?;

        let conventions_template = options
            .talk_conventions_template
            .as_deref()
            .unwrap_or("talk/conventions.md");
        self.read_template(conventions_template)?;

        let curator_template = options
            .talk_curator_template
            .as_deref()
            .unwrap_or("talk/curators/your-context.md");
        self.read_template(curator_template)?;
        Ok(())
    }

    fn validate_page_source_render_contract(&self, record: &PageRecord) -> Result<()> {
        let source_template = self.read_template(&record.template)?;
        let source = render_template(&source_template, &self.template_vars(record));
        let fields = parse_simple_frontmatter_fields(&source).with_context(|| {
            format!(
                "template {} renders invalid markdown for page {}",
                record.template, record.id
            )
        })?;
        for field in ["title", "slug", "section", "access"] {
            if fields
                .get(field)
                .map(|value| value.trim().is_empty())
                .unwrap_or(true)
            {
                return Err(anyhow!(
                    "template {} renders page {} missing required frontmatter field {field:?}; page-create would fail publish",
                    record.template,
                    record.id
                ));
            }
        }
        let section = fields
            .get("section")
            .map(String::as_str)
            .unwrap_or_default();
        if !renderer_section_is_allowed(section) {
            return Err(anyhow!(
                "template {} renders page {} with invalid frontmatter section {section:?}; allowed: for-you, context, project, work, reference, system, site",
                record.template,
                record.id
            ));
        }
        let access = fields.get("access").map(String::as_str).unwrap_or_default();
        if !matches!(access, "public" | "shared" | "private") {
            return Err(anyhow!(
                "template {} renders page {} with invalid frontmatter access {access:?}; allowed: public, shared, private",
                record.template,
                record.id
            ));
        }
        Ok(())
    }

    fn retire_page_from_registry(&self, page_id: &str, evidence: &mut Vec<Evidence>) -> Result<()> {
        let config_path = self.paths.user_wiki.join("wiki.toml");
        let original = fs::read_to_string(&config_path)
            .with_context(|| format!("read {}", config_path.display()))?;
        let mut text = original.clone();
        for key in ["navigation", "primary_navigation", "utility_navigation"] {
            text = remove_from_toml_string_array(&text, key, page_id);
        }
        text = set_page_enabled(&text, page_id, false);
        if text != original {
            write_text_atomic(&config_path, &text)?;
            evidence.push(Evidence {
                path: display_under_root(&config_path, &self.paths.root),
                status: "retired_from_navigation".to_string(),
            });
        } else {
            evidence.push(Evidence {
                path: display_under_root(&config_path, &self.paths.root),
                status: "navigation_already_retired".to_string(),
            });
        }
        Ok(())
    }

    fn restore_page_to_registry(
        &self,
        record: &PageRecord,
        evidence: &mut Vec<Evidence>,
    ) -> Result<bool> {
        let config_path = self.paths.user_wiki.join("wiki.toml");
        let original = fs::read_to_string(&config_path)
            .with_context(|| format!("read {}", config_path.display()))?;
        let mut text = set_page_enabled(&original, &record.id, true);
        if record.nav_section.as_deref() != Some("hidden") {
            text = append_to_toml_string_array(&text, "navigation", &record.id);
            match record.nav_section.as_deref() {
                Some("utility") => {
                    text = append_to_toml_string_array(&text, "utility_navigation", &record.id)
                }
                _ => text = append_to_toml_string_array(&text, "primary_navigation", &record.id),
            }
        }

        if text != original {
            write_text_atomic(&config_path, &text)?;
            evidence.push(Evidence {
                path: display_under_root(&config_path, &self.paths.root),
                status: "restored_to_navigation".to_string(),
            });
            Ok(true)
        } else {
            evidence.push(Evidence {
                path: display_under_root(&config_path, &self.paths.root),
                status: "navigation_already_restored".to_string(),
            });
            Ok(false)
        }
    }

    fn read_template(&self, relative: &str) -> Result<String> {
        if relative.contains("..") || relative.starts_with('/') {
            return Err(anyhow!("template path escapes templates/: {relative}"));
        }
        fs::read_to_string(self.paths.templates.join(relative))
            .with_context(|| format!("read template {relative}"))
    }

    fn lock_lifecycle_writes(&self) -> Result<LifecycleWriteLock> {
        let lock_path = self.paths.user_wiki.join(".1context/page-lifecycle.lock");
        if let Some(parent) = lock_path.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&lock_path)
            .with_context(|| format!("open {}", lock_path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("lock {}", lock_path.display()))?;
        Ok(LifecycleWriteLock { file })
    }

    fn template_vars(&self, record: &PageRecord) -> BTreeMap<String, String> {
        let today = Utc::now().date_naive().to_string();
        let config = read_toml(&self.paths.user_wiki.join("wiki.toml")).ok();
        let defaults = config.as_ref().and_then(|value| value.get("defaults"));
        let default_str = |key: &str, fallback: &str| -> String {
            defaults
                .and_then(|value| get_opt_str(value, key))
                .unwrap_or_else(|| fallback.to_string())
        };
        let talk_route = if record.route == "/" {
            "/talk".to_string()
        } else {
            format!("{}/talk", record.route.trim_end_matches('/'))
        };
        BTreeMap::from([
            ("page_id".to_string(), record.id.clone()),
            ("title".to_string(), record.title.clone()),
            ("slug".to_string(), record.slug.clone()),
            ("page_slug".to_string(), record.slug.clone()),
            ("route".to_string(), record.route.clone()),
            ("md_url".to_string(), markdown_url_for_route(&record.route)),
            ("talk_route".to_string(), talk_route),
            ("page_type".to_string(), record.page_type.clone()),
            ("section".to_string(), renderer_section_for_page(record)),
            (
                "summary".to_string(),
                record.summary.clone().unwrap_or_default(),
            ),
            (
                "operator_name".to_string(),
                default_str("operator_name", "Operator"),
            ),
            (
                "access_tier".to_string(),
                default_str("access_tier", "private"),
            ),
            ("asset_base".to_string(), default_str("asset_base", ".")),
            ("home_href".to_string(), default_str("home_href", "/")),
            ("created_date".to_string(), today.clone()),
            ("last_updated".to_string(), today),
            ("created_at".to_string(), now_rfc3339()),
            ("updated_at".to_string(), now_rfc3339()),
            (
                "talk_for_uri".to_string(),
                format!("mailbox://page/{}", record.id),
            ),
            (
                "article_path".to_string(),
                format!(
                    "user-wiki://source/families/{}/{}/source/{}.md",
                    record.family_group, record.family_id, record.slug
                ),
            ),
            (
                "talk_folder".to_string(),
                format!(
                    "user-wiki://source/families/{}/{}/talk/{}.talk",
                    record.family_group, record.family_id, record.slug
                ),
            ),
            (
                "concept_page_dir".to_string(),
                format!("user-wiki://source/families/{}", record.family_group),
            ),
        ])
    }

    fn page_ledger_path(&self) -> PathBuf {
        self.paths.user_wiki.join(".1context/page-ledger.jsonl")
    }

    fn page_ledger(&self) -> Result<Vec<PageLedgerEvent>> {
        read_jsonl(&self.page_ledger_path())
    }

    fn append_page_event(&self, event: PageLedgerEvent) -> Result<()> {
        append_jsonl(&self.page_ledger_path(), &event)
    }

    fn source_fingerprint(&self) -> Result<String> {
        let mut files = Vec::new();
        let wiki_config = self.paths.user_wiki.join("wiki.toml");
        if wiki_config.is_file() {
            files.push(wiki_config);
        }
        collect_publish_input_files(&self.paths.source, &mut files)?;
        fingerprint_files(&self.paths.user_wiki, files)
    }

    fn page_publish_fingerprint(&self, record: &PageRecord) -> Result<String> {
        let source = self.source_path(record);
        let tombstone = source.with_extension("tombstone.toml");
        let mut hasher = Sha256::new();
        hasher.update(b"1context-page-publish-fingerprint-v2");
        hasher.update([0]);
        hasher.update(serde_json::to_vec(record)?);
        hasher.update([0]);
        hash_optional_file(&mut hasher, "source", &source)?;
        hash_optional_file(&mut hasher, "tombstone", &tombstone)?;
        hash_optional_tree(&mut hasher, "assets", &self.asset_dir(record))?;
        hash_optional_tree(&mut hasher, "talk", &self.talk_dir(record))?;
        Ok(format!("{:x}", hasher.finalize()))
    }

    fn site_navigation_fingerprint(&self) -> Result<String> {
        let path = self.paths.user_wiki.join("wiki.toml");
        Ok(sha256_file(&path).unwrap_or_else(|_| sha256_bytes(b"missing-wiki-toml")))
    }

    fn published_page_fingerprints(&self) -> Result<Option<PagePublishFingerprints>> {
        let path = self
            .paths
            .site
            .join(".1context")
            .join("page-fingerprints.json");
        if !path.is_file() {
            return Ok(None);
        }
        serde_json::from_slice(&fs::read(&path)?)
            .with_context(|| format!("parse {}", path.display()))
            .map(Some)
    }

    fn site_needs_publish(&self) -> Result<bool> {
        let current = self.source_fingerprint().ok();
        let published = read_optional_string(
            &self
                .paths
                .site
                .join(".1context")
                .join("source-fingerprint.txt"),
        )?;
        Ok(match (current, published) {
            (Some(_), None) => true,
            (Some(current), Some(published)) => current != published,
            _ => false,
        })
    }

    fn last_publish_receipt_path(&self) -> PathBuf {
        self.paths
            .context_engine
            .join("runs/wiki-publish-receipt.json")
    }

    fn last_link_summary_for_page(&self, record: &PageRecord) -> Result<PageLinkSummary> {
        let path = self.last_publish_receipt_path();
        if !path.is_file() {
            return Ok(PageLinkSummary {
                status: "unknown".to_string(),
                broken_internal_count: 0,
                broken_internal_targets: Vec::new(),
                checked_against: None,
                issues: Vec::new(),
                repair_tasks: Vec::new(),
            });
        }
        let receipt = serde_json::from_slice::<serde_json::Value>(
            &fs::read(&path).with_context(|| format!("read {}", path.display()))?,
        )?;
        let Some(diagnostics) = receipt.get("link_diagnostics") else {
            return Ok(PageLinkSummary {
                status: "unknown".to_string(),
                broken_internal_count: 0,
                broken_internal_targets: Vec::new(),
                checked_against: None,
                issues: Vec::new(),
                repair_tasks: Vec::new(),
            });
        };
        let markdown_path = format!("{}.md", record.slug);
        let html_path = format!("{}.html", record.slug);
        let issues = diagnostics
            .get("issues")
            .and_then(|value| value.as_array())
            .into_iter()
            .flatten()
            .filter(|issue| {
                issue
                    .get("page_id")
                    .and_then(|value| value.as_str())
                    .is_some_and(|value| value == record.id.as_str())
                    || issue
                        .get("route")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| value == record.route.as_str())
                    || issue
                        .get("markdown_path")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| value == markdown_path.as_str())
                    || issue
                        .get("source_path")
                        .and_then(|value| value.as_str())
                        .is_some_and(|value| value == html_path.as_str())
            })
            .cloned()
            .collect::<Vec<_>>();
        let broken_internal_targets = issues
            .iter()
            .filter_map(|issue| {
                issue
                    .get("target")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string)
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let checked_against = issues
            .iter()
            .find_map(|issue| {
                issue
                    .get("manifest_path")
                    .and_then(|value| value.as_str())
                    .map(ToString::to_string)
            })
            .or_else(|| {
                diagnostics
                    .get("status")
                    .and_then(|value| value.as_str())
                    .map(|_| ".1context/route-manifest.json".to_string())
            });
        Ok(PageLinkSummary {
            status: if issues.is_empty() { "ok" } else { "warning" }.to_string(),
            broken_internal_count: issues.len(),
            broken_internal_targets,
            checked_against,
            repair_tasks: link_repair_tasks_for_page(record, &issues),
            issues,
        })
    }
}

fn page_record(value: &Value, defaults: Option<&Value>) -> Result<PageRecord> {
    let id = get_str(value, "id")?;
    let slug = get_opt_str(value, "slug").unwrap_or_else(|| id.clone());
    let title = get_opt_str(value, "title").unwrap_or_else(|| id.clone());
    let family_group = get_opt_str(value, "family_group").unwrap_or_else(|| "pages".to_string());
    Ok(PageRecord {
        id,
        enabled: get_opt_bool(value, "enabled").unwrap_or(true),
        title: title.clone(),
        slug: slug.clone(),
        route: get_opt_str(value, "route").unwrap_or_else(|| format!("/{slug}")),
        family_group: family_group.clone(),
        family_group_title: get_opt_str(value, "family_group_title")
            .unwrap_or_else(|| title_case(&family_group)),
        family_id: get_opt_str(value, "family_id").unwrap_or_else(|| slug.clone()),
        family_title: get_opt_str(value, "family_title").unwrap_or(title),
        page_type: get_opt_str(value, "type").unwrap_or_else(|| "context-page".to_string()),
        template: get_opt_str(value, "template").unwrap_or_else(|| {
            defaults
                .and_then(|d| get_opt_str(d, "template_pack"))
                .map(|pack| format!("pages/{pack}/{slug}.md"))
                .unwrap_or_else(|| format!("pages/{slug}.md"))
        }),
        talk_conventions_template: get_opt_str(value, "talk_conventions_template"),
        talk_curator_template: get_opt_str(value, "talk_curator_template"),
        summary: get_opt_str(value, "summary"),
        nav_section: get_opt_str(value, "nav_section"),
        nav_order: get_opt_i64(value, "nav_order"),
        origin: get_opt_str(value, "origin").unwrap_or_else(|| "created_from_template".to_string()),
    })
}

fn site_page_record(value: &Value) -> Result<SitePageRecord> {
    let id = get_str(value, "id")?;
    let title = get_opt_str(value, "title").unwrap_or_else(|| id.clone());
    let route = get_opt_str(value, "route").unwrap_or_else(|| format!("/{id}"));
    Ok(SitePageRecord {
        id,
        enabled: get_opt_bool(value, "enabled").unwrap_or(true),
        title,
        route,
        kind: get_opt_str(value, "kind").unwrap_or_else(|| "generated".to_string()),
        template: get_opt_str(value, "template").unwrap_or_else(|| "site/index.md".to_string()),
        nav_section: get_opt_str(value, "nav_section"),
        nav_order: get_opt_i64(value, "nav_order"),
    })
}

fn page_fingerprint_map(fingerprints: PagePublishFingerprints) -> BTreeMap<String, String> {
    fingerprints
        .pages
        .into_iter()
        .map(|page| (page.page_id, page.sha256))
        .collect()
}

fn page_publish_stale(
    record: &PageRecord,
    current: Option<&BTreeMap<String, String>>,
    published: Option<&BTreeMap<String, String>>,
    site_stale: bool,
) -> bool {
    let Some(current) = current else {
        return site_stale;
    };
    let Some(current_hash) = current.get(&record.id) else {
        return site_stale;
    };
    match published.and_then(|published| published.get(&record.id)) {
        Some(published_hash) => published_hash != current_hash,
        None => site_stale,
    }
}

fn read_toml(path: &Path) -> Result<Value> {
    fs::read_to_string(path)
        .with_context(|| format!("read {}", path.display()))?
        .parse::<Value>()
        .with_context(|| format!("parse {}", path.display()))
}

fn get_str(value: &Value, key: &str) -> Result<String> {
    get_opt_str(value, key).ok_or_else(|| anyhow!("missing string field {key}"))
}

fn get_opt_str(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

fn get_opt_bool(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

fn get_opt_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_integer)
}

fn write_if_missing(
    root: &Path,
    path: &Path,
    content: String,
    evidence: &mut Vec<Evidence>,
) -> Result<()> {
    if path.exists() {
        evidence.push(Evidence {
            path: display_under_root(path, root),
            status: "skipped_existing".to_string(),
        });
        return Ok(());
    }
    write_new(path, content)?;
    evidence.push(Evidence {
        path: display_under_root(path, root),
        status: "created".to_string(),
    });
    Ok(())
}

fn write_new(path: &Path, content: String) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("create {}", path.display()))?;
    file.write_all(content.as_bytes())
        .with_context(|| format!("write {}", path.display()))
}

fn existing_talk_created_at(path: &Path) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    Ok(parse_simple_frontmatter_fields(&text)
        .ok()
        .and_then(|fields| fields.get("created").cloned()))
}

fn write_text_atomic(path: &Path, content: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("file");
    let nonce = Utc::now()
        .timestamp_nanos_opt()
        .unwrap_or_else(|| Utc::now().timestamp_micros() * 1_000);
    let temp_path =
        path.with_file_name(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));
    fs::write(&temp_path, content).with_context(|| format!("write {}", temp_path.display()))?;
    fs::rename(&temp_path, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(())
}

fn append_jsonl<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    let mut line = serde_json::to_vec(value)?;
    line.push(b'\n');
    file.write_all(&line)?;
    Ok(())
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let file = File::open(path)?;
    let mut values = Vec::new();
    for line in BufReader::new(file).lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        values.push(
            serde_json::from_str(&line).with_context(|| format!("parse {}", path.display()))?,
        );
    }
    Ok(values)
}

fn read_optional_string(path: &Path) -> Result<Option<String>> {
    if !path.exists() {
        return Ok(None);
    }
    Ok(Some(fs::read_to_string(path)?.trim().to_string()))
}

fn split_markdown_frontmatter(text: &str) -> Result<(&str, &str)> {
    let mut position = if text.starts_with("---\n") {
        4
    } else if text.starts_with("---\r\n") {
        5
    } else {
        return Err(anyhow!(
            "source markdown is missing YAML frontmatter; refusing body-only edit"
        ));
    };

    while position < text.len() {
        let rest = &text[position..];
        let newline = rest.find('\n');
        let (line, next_position) = if let Some(index) = newline {
            (&rest[..index], position + index + 1)
        } else {
            (rest, text.len())
        };
        if line.trim_end_matches('\r') == "---" {
            return Ok(text.split_at(next_position));
        }
        position = next_position;
    }
    Err(anyhow!(
        "source markdown frontmatter is missing closing delimiter; refusing body-only edit"
    ))
}

const OPERATOR_TOUCHED_OPEN: &str = "<!-- operator-touched";
const OPERATOR_TOUCHED_CLOSE: &str = "<!-- /operator-touched";

fn operator_touched_open_note(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let rest = trimmed.strip_prefix(OPERATOR_TOUCHED_OPEN)?;
    let rest = rest.strip_suffix("-->")?;
    if trimmed.starts_with(OPERATOR_TOUCHED_CLOSE) {
        return None;
    }
    let note = rest.trim().trim_start_matches(':').trim();
    Some(note.to_string())
}

fn is_operator_touched_close(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with(OPERATOR_TOUCHED_CLOSE) && trimmed.ends_with("-->")
}

fn looks_like_date(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 10
        && bytes[4] == b'-'
        && bytes[7] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
}

fn atx_heading_text(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim_end_matches(['\r', '\n']);
    let level = trimmed.chars().take_while(|ch| *ch == '#').count();
    if level == 0 || level > 6 {
        return None;
    }
    let rest = &trimmed[level..];
    if !rest.is_empty() && !rest.starts_with(' ') {
        return None;
    }
    Some((level, rest.trim().trim_end_matches('#').trim_end()))
}

/// Parse operator-touched span markers from a page body. Marker grammar
/// (design §6.2; the close marker is this implementation's choice):
/// open  = a line `<!-- operator-touched[: <note>] -->`; if the note starts
///         with a YYYY-MM-DD token it is split out as the span date,
/// close = a line `<!-- /operator-touched -->`.
/// An open marker without a close protects through the end of the body.
fn parse_operator_touched_spans(frontmatter: &str, body: &str) -> Vec<ParsedOperatorSpan> {
    let frontmatter_lines = frontmatter.matches('\n').count();
    let body_lines = body.lines().count().max(1);
    let mut spans = Vec::new();
    let mut section_slug: Option<String> = None;
    let mut open: Option<(usize, usize, Option<String>, Option<String>, Option<String>)> = None;
    let mut offset = 0usize;
    let mut line_number = 0usize;

    for line in body.split_inclusive('\n') {
        line_number += 1;
        let line_start = offset;
        offset += line.len();
        if let Some((_, heading)) = atx_heading_text(line) {
            if open.is_none() {
                section_slug = Some(slugify(heading)).filter(|slug| !slug.is_empty());
            }
            continue;
        }
        if open.is_none() {
            if let Some(note) = operator_touched_open_note(line) {
                let (date, note) = match note.split_once(char::is_whitespace) {
                    Some((first, rest)) if looks_like_date(first) => (
                        Some(first.to_string()),
                        Some(rest.trim().to_string()).filter(|rest| !rest.is_empty()),
                    ),
                    None if looks_like_date(&note) => (Some(note.clone()), None),
                    _ => (None, Some(note.clone()).filter(|note| !note.is_empty())),
                };
                open = Some((line_start, line_number, date, note, section_slug.clone()));
            }
        } else if is_operator_touched_close(line) {
            let (start, start_line, date, note, slug) = open.take().expect("open span");
            spans.push(ParsedOperatorSpan {
                span: OperatorTouchedSpan {
                    date,
                    note,
                    section_slug: slug,
                    line_range: [
                        frontmatter_lines + start_line,
                        frontmatter_lines + line_number,
                    ],
                    closed: true,
                },
                start,
                end: offset,
                closed: true,
            });
        }
    }

    if let Some((start, start_line, date, note, slug)) = open {
        spans.push(ParsedOperatorSpan {
            span: OperatorTouchedSpan {
                date,
                note,
                section_slug: slug,
                line_range: [
                    frontmatter_lines + start_line,
                    frontmatter_lines + body_lines,
                ],
                closed: false,
            },
            start,
            end: body.len(),
            closed: false,
        });
    }

    spans
}

fn operator_touched_spans_for_source(source_text: &str) -> Vec<OperatorTouchedSpan> {
    split_markdown_frontmatter(source_text)
        .map(|(frontmatter, body)| {
            parse_operator_touched_spans(frontmatter, body)
                .into_iter()
                .map(|parsed| parsed.span)
                .collect()
        })
        .unwrap_or_default()
}

fn body_byte_range_to_file_lines(
    frontmatter: &str,
    body: &str,
    start: usize,
    end: usize,
) -> [usize; 2] {
    let frontmatter_lines = frontmatter.matches('\n').count();
    let start_line = frontmatter_lines + body[..start].matches('\n').count() + 1;
    let end_line = frontmatter_lines
        + body[..end.min(body.len())]
            .trim_end_matches('\n')
            .matches('\n')
            .count()
        + 1;
    [start_line, end_line.max(start_line)]
}

/// Locate one section by ATX heading text. Returns the byte range
/// `[heading_line_start, section_end)` in the body, where the section ends at
/// the next heading of the same or higher level (or the end of the body).
fn locate_section(body: &str, page_id: &str, heading: &str) -> Result<(usize, usize)> {
    let wanted = heading.trim().trim_start_matches('#').trim();
    if wanted.is_empty() {
        return Err(anyhow!(
            "page-append-section requires a non-empty --section heading"
        ));
    }
    let mut matches = Vec::new();
    let mut offset = 0usize;
    let mut lines = Vec::new();
    for line in body.split_inclusive('\n') {
        lines.push((offset, line));
        offset += line.len();
    }
    for (index, (line_start, line)) in lines.iter().enumerate() {
        if let Some((level, text)) = atx_heading_text(line) {
            if text == wanted {
                matches.push((index, *line_start, level));
            }
        }
    }
    match matches.len() {
        0 => {
            return Err(anyhow!(
                "section heading not found in body for {page_id}: {wanted}; use page-open to inspect the current source"
            ))
        }
        1 => {}
        count => {
            return Err(anyhow!(
                "section heading matched {count} times in body for {page_id}: {wanted}; make the heading unique before appending"
            ))
        }
    }
    let (heading_index, heading_start, heading_level) = matches[0];
    let section_end = lines
        .iter()
        .skip(heading_index + 1)
        .find_map(|(line_start, line)| {
            atx_heading_text(line)
                .filter(|(level, _)| *level <= heading_level)
                .map(|_| *line_start)
        })
        .unwrap_or(body.len());
    Ok((heading_start, section_end))
}

/// Server-enforced operator-touched guard (design §6.2). Agent actors may not
/// edit a byte range that overlaps an operator-touched span; operator actors
/// bypass because the spans are their own content. Insertions are zero-width
/// ranges (`edit_start == edit_end`), so an append placed at or after a closed
/// span's end passes while one inside the span is rejected. Unclosed spans
/// protect through the end of the body.
fn enforce_operator_touched_spans(
    operation: &str,
    page_id: &str,
    actor: &Actor,
    frontmatter: &str,
    body: &str,
    edit_start: usize,
    edit_end: usize,
) -> Result<()> {
    if actor.is_operator() {
        return Ok(());
    }
    let conflicting_spans = parse_operator_touched_spans(frontmatter, body)
        .into_iter()
        .filter(|parsed| {
            let effective_end = if parsed.closed {
                parsed.end
            } else {
                usize::MAX
            };
            edit_start < effective_end && parsed.start < edit_end
        })
        .map(|parsed| parsed.span)
        .collect::<Vec<_>>();
    if conflicting_spans.is_empty() {
        return Ok(());
    }
    Err(operator_touched_conflict_error(
        operation,
        page_id,
        actor,
        Some(body_byte_range_to_file_lines(
            frontmatter,
            body,
            edit_start,
            edit_end,
        )),
        conflicting_spans,
    ))
}

/// Insert `content` as a standalone markdown block at `insertion` (a byte
/// offset on a line boundary in `body`) without rewriting any existing line:
/// the text before and after the insertion point is preserved verbatim, with
/// blank-line separation added around the new block as needed.
fn append_block_at(body: &str, insertion: usize, content: &str) -> String {
    let before = &body[..insertion];
    let after = &body[insertion..];
    let mut next = String::with_capacity(body.len() + content.len() + 4);
    next.push_str(before);
    if !next.is_empty() && !next.ends_with('\n') {
        next.push('\n');
    }
    if !next.is_empty() && !next.ends_with("\n\n") {
        next.push('\n');
    }
    next.push_str(content.trim_end());
    next.push('\n');
    if !after.is_empty() {
        next.push('\n');
        next.push_str(after);
    }
    next
}

fn parse_simple_frontmatter_fields(text: &str) -> Result<BTreeMap<String, String>> {
    let (frontmatter, _) = split_markdown_frontmatter(text)?;
    let mut fields = BTreeMap::new();
    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() || line == "---" || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        fields.insert(
            key.trim().to_string(),
            unquote_frontmatter_value(value.trim()),
        );
    }
    Ok(fields)
}

#[cfg(test)]
fn parse_talk_file_attachments(text: &str) -> Result<Vec<TalkAttachmentRecord>> {
    let (frontmatter, _) = split_markdown_frontmatter(text)?;
    let mut attachments = Vec::new();
    let mut current = BTreeMap::new();
    let mut in_attachments = false;

    for raw_line in frontmatter.lines() {
        let line = raw_line.trim_end_matches('\r');
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed == "---" || trimmed.starts_with('#') {
            continue;
        }
        if !in_attachments {
            if trimmed == "attachments:" {
                in_attachments = true;
            }
            continue;
        }
        if !line.starts_with(' ') && !line.starts_with('\t') {
            push_talk_attachment_record(&mut attachments, &mut current);
            break;
        }
        if let Some(item) = trimmed.strip_prefix("- ") {
            push_talk_attachment_record(&mut attachments, &mut current);
            if let Some((key, value)) = item.split_once(':') {
                current.insert(
                    key.trim().to_string(),
                    unquote_frontmatter_value(value.trim()),
                );
            }
            continue;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            current.insert(
                key.trim().to_string(),
                unquote_frontmatter_value(value.trim()),
            );
        }
    }
    if in_attachments {
        push_talk_attachment_record(&mut attachments, &mut current);
    }

    Ok(attachments)
}

#[cfg(test)]
fn push_talk_attachment_record(
    attachments: &mut Vec<TalkAttachmentRecord>,
    fields: &mut BTreeMap<String, String>,
) {
    let Some(filename) = fields.remove("filename").filter(|value| !value.is_empty()) else {
        fields.clear();
        return;
    };
    let Some(media_type) = fields
        .remove("media_type")
        .filter(|value| !value.is_empty())
    else {
        fields.clear();
        return;
    };
    let Some(path) = fields.remove("path").filter(|value| !value.is_empty()) else {
        fields.clear();
        return;
    };
    let Some(handle) = fields.remove("handle").filter(|value| !value.is_empty()) else {
        fields.clear();
        return;
    };
    let caption = fields.remove("caption").filter(|value| !value.is_empty());
    let alt_text = fields.remove("alt_text").filter(|value| !value.is_empty());
    fields.clear();

    attachments.push(TalkAttachmentRecord {
        filename,
        media_type,
        path,
        handle,
        caption,
        alt_text,
    });
}

fn validate_talk_target_id(value: &str, flag: &str) -> Result<String> {
    let trimmed = value.trim();
    if talk_target_id_is_valid(trimmed) {
        Ok(trimmed.to_string())
    } else {
        Err(anyhow!(
            "invalid talk target id for {flag}: {value:?}; expected a non-empty id without whitespace or control characters"
        ))
    }
}

fn talk_target_id_is_valid(value: &str) -> bool {
    let trimmed = value.trim();
    !trimmed.is_empty()
        && trimmed.len() <= 200
        && trimmed == value
        && !trimmed
            .chars()
            .any(|ch| ch.is_whitespace() || ch.is_control())
}

fn talk_file_subject(fields: &BTreeMap<String, String>, body: &str, message_id: &str) -> String {
    if let Some(subject) = fields
        .get("subject")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        return subject.to_string();
    }
    body.lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("## ")
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToString::to_string)
        })
        .unwrap_or_else(|| message_id.to_string())
}

fn talk_file_thread_id(
    record: &PageRecord,
    fields: &BTreeMap<String, String>,
    body: &str,
    message_id: &str,
) -> String {
    if let Some(thread_id) = fields
        .get("thread")
        .map(|value| value.trim())
        .filter(|value| talk_target_id_is_valid(value))
    {
        return thread_id.to_string();
    }
    format!(
        "thread_{}_{}",
        record.id,
        slugify(&talk_file_subject(fields, body, message_id))
    )
}

fn unquote_frontmatter_value(value: &str) -> String {
    let trimmed = value.trim();
    if trimmed.len() >= 2 {
        let bytes = trimmed.as_bytes();
        if (bytes[0] == b'"' && bytes[trimmed.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[trimmed.len() - 1] == b'\'')
        {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

fn render_template(template: &str, vars: &BTreeMap<String, String>) -> String {
    let mut rendered = template.to_string();
    for (key, value) in vars {
        rendered = rendered.replace(&format!("{{{{ {key} }}}}"), value);
        rendered = rendered.replace(&format!("{{{{{key}}}}}"), value);
    }
    rendered
}

fn renderer_section_for_page(record: &PageRecord) -> String {
    if renderer_section_is_allowed(&record.family_group) {
        return record.family_group.clone();
    }
    match record.page_type.as_str() {
        "project" | "project-index" => "project",
        "topic" | "topic-index" => "reference",
        "for-you" | "for-you-week" => "for-you",
        "system" => "system",
        "site" => "site",
        "work" => "work",
        _ => "context",
    }
    .to_string()
}

fn renderer_section_is_allowed(section: &str) -> bool {
    matches!(
        section,
        "for-you" | "context" | "project" | "work" | "reference" | "system" | "site"
    )
}

fn validate_renderer_frontmatter_fields(fields: &BTreeMap<String, String>) -> Option<String> {
    for field in ["title", "slug", "section", "access"] {
        if fields
            .get(field)
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
        {
            return Some(format!("missing required field {field:?}"));
        }
    }
    let slug = fields.get("slug").map(String::as_str).unwrap_or_default();
    if !page_slug_is_allowed(slug) {
        return Some(format!(
            "slug {slug:?} must use lowercase alphanumeric characters, hyphens, or dots, with an alphanumeric start and end"
        ));
    }
    let section = fields
        .get("section")
        .map(String::as_str)
        .unwrap_or_default();
    if !renderer_section_is_allowed(section) {
        return Some(format!(
            "field \"section\" has value {section:?}; allowed: for-you, context, project, work, reference, system, site"
        ));
    }
    let access = fields.get("access").map(String::as_str).unwrap_or_default();
    if !matches!(access, "public" | "shared" | "private") {
        return Some(format!(
            "field \"access\" has value {access:?}; allowed: public, shared, private"
        ));
    }
    for (field, allowed) in [
        ("source_type", ["authored", "imported"].as_slice()),
        (
            "status",
            ["draft", "published", "archived", "superseded"].as_slice(),
        ),
        ("audience", ["internal", "public", "both"].as_slice()),
        ("theme_default", ["light", "dark", "auto"].as_slice()),
        ("article_width", ["s", "m", "l"].as_slice()),
        ("font_size", ["s", "m", "l"].as_slice()),
        ("border_radius", ["rounded", "square"].as_slice()),
        ("links_style", ["underline", "color"].as_slice()),
        ("cover_image", ["show", "hide"].as_slice()),
        ("article_style", ["full", "pics", "text"].as_slice()),
    ] {
        let Some(value) = fields.get(field).map(String::as_str) else {
            continue;
        };
        if !allowed.contains(&value) {
            return Some(format!(
                "field {field:?} has value {value:?}; allowed: {}",
                allowed.join(", ")
            ));
        }
    }
    None
}

fn template_state(origin: &str, content_state: &str) -> String {
    if !page_origin_is_template_derived(origin) {
        return "not_template_backed".to_string();
    }
    match content_state {
        "template_unedited" => "template_unedited",
        "edited" => "edited_from_template",
        _ => "unknown",
    }
    .to_string()
}

fn page_origin_is_template_derived(origin: &str) -> bool {
    matches!(origin, "created_from_template" | "runtime_default")
}

fn page_allowed_actions(state: &str, rendered: bool) -> Vec<String> {
    let actions = match state {
        "source_missing" => vec![
            "wiki.publish",
            "wiki.page.create",
            "wiki.page.open",
            "wiki.validate",
        ],
        "tombstoned" | "disabled" if rendered => {
            vec!["wiki.page.restore", "wiki.validate", "wiki.publish"]
        }
        "tombstoned" | "disabled" => vec!["wiki.page.restore", "wiki.validate"],
        _ => vec![
            "wiki.page.open",
            "wiki.page.patch_body",
            "wiki.page.write_body",
            "wiki.asset.add",
            "wiki.asset.list",
            "wiki.talk.append",
            "wiki.validate",
            "wiki.publish",
            "wiki.page.delete",
        ],
    };
    actions.into_iter().map(str::to_string).collect()
}

fn validate_page_slug(slug: &str) -> Result<()> {
    if page_slug_is_allowed(slug) {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid page slug {slug}; expected 1-60 lowercase alphanumeric characters, hyphens, or dots, with an alphanumeric start and end"
        ))
    }
}

fn page_slug_is_allowed(slug: &str) -> bool {
    if slug.is_empty() || slug.len() > 60 || slug.contains("..") {
        return false;
    }
    let mut chars = slug.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let Some(last) = slug.chars().last() else {
        return false;
    };
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    if !last.is_ascii_lowercase() && !last.is_ascii_digit() {
        return false;
    }
    slug.chars()
        .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-' || ch == '.')
}

fn validate_family_path_token(value: &str, label: &str) -> Result<()> {
    if page_slug_is_allowed(value) {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid page {label} {value}; expected a safe filesystem token with lowercase alphanumeric characters, hyphens, or dots, and no path traversal"
        ))
    }
}

fn validate_page_route(route: &str) -> Result<()> {
    if route != route.trim() {
        return Err(anyhow!(
            "invalid page route {route}; route must not have leading or trailing whitespace"
        ));
    }
    if !route.starts_with('/') {
        return Err(anyhow!(
            "invalid page route {route}; expected an absolute route starting with /"
        ));
    }
    if route.contains('\\') || route.contains('?') || route.contains('#') {
        return Err(anyhow!(
            "invalid page route {route}; routes must not contain backslashes, query strings, or fragments"
        ));
    }
    if route != "/" && route.ends_with('/') {
        return Err(anyhow!(
            "invalid page route {route}; omit trailing slashes so every page has one canonical route"
        ));
    }
    if route == "/" {
        return Ok(());
    }
    for segment in route.trim_start_matches('/').split('/') {
        if segment.is_empty() {
            return Err(anyhow!(
                "invalid page route {route}; route segments must not be empty"
            ));
        }
        if segment.ends_with(".md") || segment.ends_with(".html") || !page_slug_is_allowed(segment)
        {
            return Err(anyhow!(
                "invalid page route {route}; each segment must be a safe slug and must not end in .md or .html"
            ));
        }
    }
    Ok(())
}

fn validate_existing_page_create_options(
    record: &PageRecord,
    options: &PageCreateOptions,
) -> Result<()> {
    check_existing_page_option(&record.id, "title", options.title.as_deref(), &record.title)?;
    check_existing_page_option(&record.id, "slug", options.slug.as_deref(), &record.slug)?;
    check_existing_page_option(&record.id, "route", options.route.as_deref(), &record.route)?;
    check_existing_page_option(
        &record.id,
        "family_group",
        options.family_group.as_deref(),
        &record.family_group,
    )?;
    check_existing_page_option(
        &record.id,
        "family_group_title",
        options.family_group_title.as_deref(),
        &record.family_group_title,
    )?;
    check_existing_page_option(
        &record.id,
        "family_id",
        options.family_id.as_deref(),
        &record.family_id,
    )?;
    check_existing_page_option(
        &record.id,
        "family_title",
        options.family_title.as_deref(),
        &record.family_title,
    )?;
    check_existing_page_option(
        &record.id,
        "type",
        options.page_type.as_deref(),
        &record.page_type,
    )?;
    check_existing_page_option(
        &record.id,
        "template",
        options.template.as_deref(),
        &record.template,
    )?;
    check_existing_page_option(
        &record.id,
        "talk_conventions_template",
        options.talk_conventions_template.as_deref(),
        record.talk_conventions_template.as_deref().unwrap_or(""),
    )?;
    check_existing_page_option(
        &record.id,
        "talk_curator_template",
        options.talk_curator_template.as_deref(),
        record.talk_curator_template.as_deref().unwrap_or(""),
    )?;
    if let Some(summary) = options.summary.as_deref() {
        check_existing_page_option(
            &record.id,
            "summary",
            Some(summary),
            record.summary.as_deref().unwrap_or(""),
        )?;
    }
    if let Some(nav_section) = options.nav_section.as_deref() {
        check_existing_page_option(
            &record.id,
            "nav_section",
            Some(nav_section),
            record.nav_section.as_deref().unwrap_or("primary"),
        )?;
    }
    if options.nav_order.is_some() {
        return Err(anyhow!(
            "page already exists: {}; page-create cannot change nav_order for an existing page",
            record.id
        ));
    }
    Ok(())
}

fn check_existing_page_option(
    page_id: &str,
    field: &str,
    requested: Option<&str>,
    existing: &str,
) -> Result<()> {
    if let Some(requested) = requested {
        if requested != existing {
            return Err(anyhow!(
                "page already exists: {page_id}; requested {field} {requested:?} differs from existing {existing:?}"
            ));
        }
    }
    Ok(())
}

fn validate_nav_section(nav_section: &str) -> Result<()> {
    if matches!(nav_section, "primary" | "utility" | "hidden") {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid nav section {nav_section}; expected primary, utility, or hidden"
        ))
    }
}

fn config_issue(
    code: &str,
    page: Option<PageParticipantReference>,
    message: String,
) -> ValidationIssue {
    ValidationIssue {
        code: code.to_string(),
        severity: "error".to_string(),
        page,
        paths: vec!["user-wiki/wiki.toml".to_string()],
        message,
        next_action: "repair_wiki_toml".to_string(),
        diagnostics: Vec::new(),
        repair_tasks: Vec::new(),
    }
}

fn page_frontmatter_issue(
    record: &PageRecord,
    source_path: String,
    message: String,
) -> ValidationIssue {
    ValidationIssue {
        code: "invalid_page_frontmatter".to_string(),
        severity: "error".to_string(),
        page: Some(PageParticipantReference {
            id: record.id.clone(),
            title: record.title.clone(),
            route: record.route.clone(),
        }),
        paths: vec![source_path],
        message,
        next_action: "repair_source".to_string(),
        diagnostics: Vec::new(),
        repair_tasks: Vec::new(),
    }
}

fn repair_hints_for_validation_issues(issues: &[ValidationIssue]) -> Vec<String> {
    issues
        .iter()
        .map(|issue| match issue.code.as_str() {
            "duplicate_page_id" => {
                "Edit user-wiki/wiki.toml so each [[pages]] id is unique, then run wiki.validate."
            }
            "duplicate_route" => {
                "Edit user-wiki/wiki.toml so page, talk, and generated site routes do not collide, then run wiki.validate."
            }
            "invalid_page_slug" => {
                "Fix the page slug in user-wiki/wiki.toml to use lowercase alphanumeric characters, hyphens, or dots."
            }
            "invalid_page_route" | "invalid_site_page_route" => {
                "Fix the route in user-wiki/wiki.toml to be one canonical absolute route such as /topics."
            }
            "invalid_nav_section" => {
                "Set nav_section to primary, utility, or hidden in user-wiki/wiki.toml."
            }
            "invalid_nav_order" => {
                "Set nav_order to an integer in user-wiki/wiki.toml."
            }
            "invalid_navigation_entry" => {
                "Use page ids, not routes or objects, in the site navigation arrays in user-wiki/wiki.toml."
            }
            "unknown_navigation_page" => {
                "Remove the unknown page id from site navigation or add a matching [[pages]] or [[site_pages]] entry."
            }
            "page_has_broken_internal_links" => {
                "Open each repair_tasks entry, patch the listed hrefs, then run wiki.publish and wiki.validate again."
            }
            "invalid_page_frontmatter" => {
                "Repair the page source frontmatter so the renderer can parse it, then run wiki.validate before publishing."
            }
            "configured_page_missing_source" | "configured_page_missing_talk" | "site_needs_publish"
            | "page_needs_publish" => "Run wiki.publish to refresh the rendered site.",
            _ => "",
        })
        .filter(|hint| !hint.is_empty())
        .map(ToString::to_string)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn site_array<'a>(config: &'a Value, key: &str) -> Option<&'a Vec<Value>> {
    config
        .get("site")
        .and_then(|site| site.get(key))
        .or_else(|| config.get(key))
        .and_then(Value::as_array)
}

fn reserved_site_routes(config: &Value) -> BTreeMap<String, String> {
    let mut routes = BTreeMap::new();
    if let Some(site_pages) = config.get("site_pages").and_then(Value::as_array) {
        for site_page in site_pages {
            let id = get_opt_str(site_page, "id").unwrap_or_else(|| "unknown".to_string());
            let route = get_opt_str(site_page, "route").unwrap_or_else(|| format!("/{id}"));
            routes.insert(route_conflict_key(&route), format!("site page {id}"));
        }
    }
    routes
}

fn reserved_routes_for_create(pages: &[PageRecord], config: &Value) -> BTreeMap<String, String> {
    let mut routes = reserved_site_routes(config);
    for page in pages {
        routes.insert(route_conflict_key(&page.route), format!("page {}", page.id));
        routes.insert(
            talk_route_for_route(&page.route),
            format!("talk route for page {}", page.id),
        );
    }
    routes
}

fn push_route_reservation_issue(
    issues: &mut Vec<ValidationIssue>,
    route_owners: &mut BTreeMap<String, String>,
    route: String,
    owner: String,
    page: Option<PageParticipantReference>,
) {
    if let Some(existing_owner) = route_owners.insert(route.clone(), owner.clone()) {
        issues.push(config_issue(
            "duplicate_route",
            page,
            format!(
                "wiki.toml route {route} is claimed by both {existing_owner} and {owner}; choose one owner or assign a unique route."
            ),
        ));
    }
}

fn route_conflict_key(route: &str) -> String {
    canonical_route_target(route).unwrap_or_else(|| {
        let trimmed = route.trim().trim_start_matches('/').trim_end_matches('/');
        if trimmed.is_empty() {
            "/".to_string()
        } else {
            format!("/{trimmed}")
        }
    })
}

fn talk_route_for_route(route: &str) -> String {
    let route = route_conflict_key(route);
    if route == "/" {
        "/talk".to_string()
    } else {
        format!("{}/talk", route)
    }
}

fn collect_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, files)?;
        } else if path.is_file() {
            files.push(path);
        }
    }
    Ok(())
}

fn collect_publish_input_files(root: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    if !root.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(".talk"))
                .unwrap_or(false)
            {
                // Talk trees are publish inputs (design §6.1/R11): a
                // talk-only change must mark the site stale so publish
                // proceeds without --force.
                collect_files(&path, files)?;
                continue;
            }
            collect_publish_input_files(&path, files)?;
        } else if path.is_file() && is_publish_input_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_publish_input_file(path: &Path) -> bool {
    let Some(parent) = path.parent().and_then(|parent| parent.file_name()) else {
        return false;
    };
    if parent
        .to_str()
        .map(|name| name.ends_with(".assets"))
        .unwrap_or(false)
    {
        return true;
    }
    if parent != "source" {
        return false;
    }
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| extension == "md")
        .unwrap_or(false)
        || path
            .file_name()
            .and_then(|name| name.to_str())
            .map(|name| name.ends_with(".tombstone.toml"))
            .unwrap_or(false)
}

fn sha256_file(path: &Path) -> Result<String> {
    Ok(sha256_bytes(&fs::read(path)?))
}

fn hash_optional_file(hasher: &mut Sha256, label: &str, path: &Path) -> Result<()> {
    hasher.update(label.as_bytes());
    hasher.update([0]);
    if path.is_file() {
        hasher.update(b"present");
        hasher.update([0]);
        hasher.update(fs::read(path)?);
    } else {
        hasher.update(b"missing");
    }
    hasher.update([0]);
    Ok(())
}

fn hash_optional_tree(hasher: &mut Sha256, label: &str, path: &Path) -> Result<()> {
    hasher.update(label.as_bytes());
    hasher.update([0]);
    if path.is_dir() {
        hasher.update(b"present");
        hasher.update([0]);
        let mut files = Vec::new();
        collect_files(path, &mut files)?;
        files.sort();
        for file in files {
            hasher.update(display_under_root(&file, path).as_bytes());
            hasher.update([0]);
            hasher.update(fs::read(file)?);
            hasher.update([0]);
        }
    } else {
        hasher.update(b"missing");
    }
    hasher.update([0]);
    Ok(())
}

fn tree_fingerprint(root: &Path) -> Result<String> {
    let mut files = Vec::new();
    collect_files(root, &mut files)?;
    fingerprint_files(root, files)
}

fn fingerprint_files(root: &Path, mut files: Vec<PathBuf>) -> Result<String> {
    files.sort();
    let mut hasher = Sha256::new();
    for file in files {
        hasher.update(display_under_root(&file, root).as_bytes());
        hasher.update([0]);
        hasher.update(fs::read(file)?);
        hasher.update([0]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn short_hash(value: &str) -> String {
    sha256_bytes(value.as_bytes()).chars().take(10).collect()
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn now_file_stamp() -> String {
    Utc::now().format("%Y-%m-%dT%H-%M-%S-%9fZ").to_string()
}

fn slugify(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(64).collect::<String>()
}

fn link_repair_tasks_for_page(
    record: &PageRecord,
    issues: &[serde_json::Value],
) -> Vec<LinkRepairTask> {
    if issues.is_empty() {
        return Vec::new();
    }
    vec![LinkRepairTask {
        page_id: record.id.clone(),
        route: record.route.clone(),
        markdown_path: first_issue_string(issues, "markdown_path")
            .or_else(|| Some(format!("{}.md", record.slug))),
        source_path: first_issue_string(issues, "source_path"),
        route_index_path: first_issue_string(issues, "route_index_path"),
        broken_internal_count: issues.len(),
        hrefs: issue_strings(issues, "href"),
        targets: issue_strings(issues, "target"),
        next_action: "repair_links".to_string(),
        suggested_operations: [
            "wiki.page.open",
            "wiki.page.patch_body",
            "wiki.publish",
            "wiki.validate",
        ]
        .into_iter()
        .map(ToString::to_string)
        .collect(),
    }]
}

fn first_issue_string(issues: &[serde_json::Value], key: &str) -> Option<String> {
    issues.iter().find_map(|issue| {
        issue
            .get(key)
            .and_then(|value| value.as_str())
            .map(ToString::to_string)
    })
}

fn issue_strings(issues: &[serde_json::Value], key: &str) -> Vec<String> {
    issues
        .iter()
        .filter_map(|issue| {
            issue
                .get(key)
                .and_then(|value| value.as_str())
                .map(ToString::to_string)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn extract_markdown_link_hrefs(markdown: &str) -> Vec<String> {
    let mut hrefs = Vec::new();
    let mut offset = 0;
    while let Some(relative) = markdown[offset..].find("](") {
        let marker = offset + relative;
        let start = marker + 2;
        let Some(end_relative) = markdown[start..].find(')') else {
            break;
        };
        let raw = markdown[start..start + end_relative].trim();
        if !raw.is_empty() {
            hrefs.push(markdown_link_destination(raw).to_string());
        }
        offset = start + end_relative + 1;
    }
    hrefs
}

fn markdown_link_destination(raw: &str) -> &str {
    if let Some(inner) = raw
        .strip_prefix('<')
        .and_then(|value| value.split('>').next())
    {
        return inner.trim();
    }
    raw.split_whitespace().next().unwrap_or(raw).trim()
}

struct NormalizedWikiHref {
    target: String,
    route_target: String,
    target_kind: String,
}

fn normalize_wiki_href(href: &str, source_route: &str) -> Option<NormalizedWikiHref> {
    let trimmed = href.trim();
    if trimmed.is_empty()
        || trimmed.starts_with('#')
        || trimmed.starts_with("http://")
        || trimmed.starts_with("https://")
        || trimmed.starts_with("mailto:")
        || trimmed.starts_with("tel:")
        || trimmed.starts_with("data:")
        || trimmed.starts_with("javascript:")
    {
        return None;
    }
    let without_fragment = trimmed.split('#').next().unwrap_or(trimmed);
    let without_query = without_fragment
        .split('?')
        .next()
        .unwrap_or(without_fragment);
    let normalized = if without_query.starts_with('/') {
        normalize_wiki_route_segments(without_query.trim_start_matches('/'))?
    } else {
        let base = source_route
            .trim_matches('/')
            .rsplit_once('/')
            .map(|(parent, _)| parent)
            .unwrap_or("");
        let joined = if base.is_empty() {
            without_query.to_string()
        } else {
            format!("{base}/{without_query}")
        };
        normalize_wiki_route_segments(&joined)?
    };
    let route_target = canonical_route_target(&normalized)?;
    let target_kind = if normalized.ends_with(".md") {
        "markdown_twin"
    } else {
        "route"
    }
    .to_string();
    let target = if target_kind == "markdown_twin" {
        markdown_url_for_route(&route_target)
    } else {
        route_target.clone()
    };
    Some(NormalizedWikiHref {
        target,
        route_target,
        target_kind,
    })
}

fn normalize_wiki_route_segments(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            _ => parts.push(part),
        }
    }
    Some(format!("/{}", parts.join("/")))
}

fn canonical_route_target(route: &str) -> Option<String> {
    let mut value = route.trim();
    if value.is_empty() {
        return None;
    }
    if let Some(stripped) = value.strip_suffix("/index.html") {
        value = stripped;
    }
    if let Some(stripped) = value.strip_suffix("/index.md") {
        value = stripped;
    }
    if let Some(stripped) = value.strip_suffix(".html") {
        value = stripped;
    }
    if let Some(stripped) = value.strip_suffix(".md") {
        value = stripped;
    }
    let normalized = normalize_wiki_route_segments(value.trim_start_matches('/'))?;
    if normalized == "/" {
        Some("/".to_string())
    } else {
        Some(normalized.trim_end_matches('/').to_string())
    }
}

fn link_points_at_deleted_route(target: &str, deleted_route: &str) -> bool {
    if target == deleted_route {
        return true;
    }
    if deleted_route == "/" {
        return false;
    }
    target.starts_with(&format!("{}/", deleted_route.trim_end_matches('/')))
}

fn toml_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn append_to_toml_string_array(text: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key} = [");
    let quoted = toml_quote(value);
    let mut changed = false;
    let lines = text
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if !changed && trimmed.starts_with(&prefix) && trimmed.ends_with(']') {
                changed = true;
                if trimmed.contains(&quoted) {
                    return line.to_string();
                }
                let indent = &line[..line.len() - trimmed.len()];
                let inner = trimmed
                    .trim_start_matches(&prefix)
                    .trim_end_matches(']')
                    .trim();
                let new_inner = if inner.is_empty() {
                    quoted.clone()
                } else {
                    format!("{inner}, {quoted}")
                };
                format!("{indent}{key} = [{new_inner}]")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.ends_with('\n') {
        format!("{lines}\n")
    } else {
        lines
    }
}

fn remove_from_toml_string_array(text: &str, key: &str, value: &str) -> String {
    let prefix = format!("{key} = [");
    let quoted = toml_quote(value);
    let lines = text
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if trimmed.starts_with(&prefix) && trimmed.ends_with(']') {
                let indent = &line[..line.len() - trimmed.len()];
                let inner = trimmed
                    .trim_start_matches(&prefix)
                    .trim_end_matches(']')
                    .trim();
                let values = inner
                    .split(',')
                    .map(str::trim)
                    .filter(|entry| !entry.is_empty() && *entry != quoted)
                    .collect::<Vec<_>>();
                format!("{indent}{key} = [{}]", values.join(", "))
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    if text.ends_with('\n') {
        format!("{lines}\n")
    } else {
        lines
    }
}

fn set_page_enabled(text: &str, page_id: &str, enabled: bool) -> String {
    let enabled_line = format!("enabled = {enabled}");
    let mut out = Vec::new();
    let mut in_page = false;
    let mut target = false;
    let mut saw_enabled = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed == "[[pages]]" {
            if in_page && target && !saw_enabled {
                out.push(enabled_line.clone());
            }
            in_page = true;
            target = false;
            saw_enabled = false;
            out.push(line.to_string());
            continue;
        }
        if in_page && trimmed.starts_with("[[") {
            if target && !saw_enabled {
                out.push(enabled_line.clone());
            }
            in_page = false;
            target = false;
            saw_enabled = false;
            out.push(line.to_string());
            continue;
        }
        if in_page && trimmed.starts_with("id = ") {
            target = parse_toml_string_literal(trimmed.trim_start_matches("id = ").trim())
                .map(|id| id == page_id)
                .unwrap_or(false);
        }
        if in_page && target && trimmed.starts_with("enabled = ") {
            let indent = &line[..line.len() - line.trim_start().len()];
            out.push(format!("{indent}{enabled_line}"));
            saw_enabled = true;
            continue;
        }
        out.push(line.to_string());
    }
    if in_page && target && !saw_enabled {
        out.push(enabled_line);
    }
    let lines = out.join("\n");
    if text.ends_with('\n') {
        format!("{lines}\n")
    } else {
        lines
    }
}

fn parse_toml_string_literal(value: &str) -> Option<&str> {
    value.strip_prefix('"')?.strip_suffix('"')
}

trait IfEmpty {
    fn if_empty(self, fallback: &str) -> String;
}

impl IfEmpty for String {
    fn if_empty(self, fallback: &str) -> String {
        if self.is_empty() {
            fallback.to_string()
        } else {
            self
        }
    }
}

fn safe_attachment_filename(value: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed == "."
        || trimmed == ".."
        || trimmed.contains('/')
        || trimmed.contains('\\')
        || trimmed.contains('\0')
    {
        return Err(anyhow!("invalid attachment filename: {value}"));
    }
    let mut sanitized = String::new();
    let mut previous_was_dash = false;
    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_') {
            sanitized.push(ch);
            previous_was_dash = false;
        } else if ch == '-' {
            if !previous_was_dash {
                sanitized.push('-');
                previous_was_dash = true;
            }
        } else if !previous_was_dash {
            sanitized.push('-');
            previous_was_dash = true;
        }
    }
    while sanitized.contains("-.") {
        sanitized = sanitized.replace("-.", ".");
    }
    let sanitized = sanitized.trim_matches(['.', '-']).to_string();
    if sanitized.is_empty() {
        return Err(anyhow!("invalid attachment filename: {value}"));
    }
    Ok(sanitized)
}

fn unique_attachment_filename(filename: String, used: &mut BTreeSet<String>) -> String {
    if used.insert(filename.clone()) {
        return filename;
    }

    let stem = filename
        .rsplit_once('.')
        .map(|(stem, _)| stem)
        .unwrap_or(filename.as_str())
        .to_string();
    let extension = filename
        .rsplit_once('.')
        .map(|(_, extension)| format!(".{extension}"))
        .unwrap_or_default();
    let mut suffix = 2;
    loop {
        let candidate = format!("{stem}-{suffix}{extension}");
        if used.insert(candidate.clone()) {
            return candidate;
        }
        suffix += 1;
    }
}

fn talk_attachment_href(record: &PageRecord, relative_path: &str) -> String {
    let talk_route = if record.route == "/" {
        "/talk".to_string()
    } else {
        format!("{}/talk", record.route.trim_end_matches('/'))
    };
    format!("{}/{}", talk_route, relative_path.trim_start_matches('/'))
}

fn infer_media_type(filename: &str) -> &'static str {
    match filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("svg") => "image/svg+xml",
        Some("json") => "application/json",
        Some("md") | Some("markdown") => "text/markdown",
        Some("eml") => "text/plain",
        Some("css") => "text/css",
        Some("html") => "text/html",
        Some("js") | Some("mjs") | Some("jsx") | Some("ts") | Some("tsx") => "text/javascript",
        Some("toml") => "text/toml",
        Some("yaml") | Some("yml") => "text/yaml",
        Some("txt") | Some("log") | Some("rs") | Some("swift") | Some("py") | Some("go")
        | Some("java") | Some("kt") | Some("sh") | Some("sql") | Some("c") | Some("h")
        | Some("cc") | Some("cpp") | Some("hpp") | Some("m") | Some("mm") | Some("rb")
        | Some("xml") => "text/plain",
        Some("csv") => "text/csv",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn page_asset_kind(filename: &str, media_type: &str) -> &'static str {
    if media_type.starts_with("image/") {
        return "image";
    }
    match filename
        .rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .as_deref()
    {
        Some("c") | Some("cc") | Some("cpp") | Some("css") | Some("go") | Some("h")
        | Some("hpp") | Some("html") | Some("java") | Some("js") | Some("jsx") | Some("kt")
        | Some("m") | Some("mm") | Some("mjs") | Some("py") | Some("rb") | Some("rs")
        | Some("sh") | Some("sql") | Some("swift") | Some("toml") | Some("ts") | Some("tsx")
        | Some("xml") | Some("yaml") | Some("yml") => "code_file",
        _ => "file",
    }
}

fn reference_record_matches_page(value: &serde_json::Value, page_id: &str, route: &str) -> bool {
    value
        .get("page_id")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|value| value == page_id)
        || value
            .get("route")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| value == route)
}

fn escape_yaml(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn display_under_root(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}

fn markdown_url_for_route(route: &str) -> String {
    let trimmed = route.trim_end_matches('/');
    if trimmed.is_empty() {
        "/index.md".to_string()
    } else {
        format!("{trimmed}.md")
    }
}

fn page_asset_published_href(route: &str, filename: &str) -> String {
    let route = route_conflict_key(route);
    let stem = route.trim_start_matches('/').trim_end_matches('/');
    if stem.is_empty() {
        format!("/index.assets/{filename}")
    } else {
        format!("/{stem}.assets/{filename}")
    }
}

fn slug_token(value: &str) -> String {
    let mut token = String::new();
    let mut previous_was_underscore = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            token.push(ch.to_ascii_lowercase());
            previous_was_underscore = false;
        } else if !previous_was_underscore {
            token.push('_');
            previous_was_underscore = true;
        }
    }
    token.trim_matches('_').to_string()
}

fn title_case(value: &str) -> String {
    value
        .split(['-', '_'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn fresh_configured_pages_point_agents_at_publish() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        let publish = core.publish_status().unwrap();
        assert!(publish.render_required);
        assert_eq!(publish.pages_missing_source, vec!["topics".to_string()]);
        assert_eq!(publish.next_action, "publish");

        let validation = core.validate().unwrap();
        assert_eq!(validation.status, "warning");
        assert_eq!(validation.blocking_count, 0);
        assert!(validation.can_publish);
        assert_eq!(validation.next_action, "publish");
        assert_eq!(validation.issues[0].code, "configured_page_missing_source");
        assert_eq!(validation.issues[0].severity, "warning");

        let status = core.status().unwrap();
        assert_eq!(status.state, "attention");
        assert_eq!(status.next_action, "publish");

        let page = core.page_status("topics").unwrap();
        assert_eq!(page.state, "source_missing");
        assert_eq!(page.next_action, "publish");
        assert_eq!(page.validation.blocking_count, 0);
        assert!(page.allowed_actions.contains(&"wiki.publish".to_string()));

        let opened = core.open_page("topics").unwrap();
        assert_eq!(opened.title, page.title);
        assert_eq!(opened.route, page.route);
        assert_eq!(opened.collection, page.collection);
        assert_eq!(opened.page_type, page.page_type);
        assert_eq!(opened.page_status.route, page.route);
        assert_eq!(opened.edit.recommended_operation, "wiki.publish");
        assert_eq!(opened.edit.recommended_write_mode, "create_from_template");
        assert!(!opened.edit.safe_to_edit);
        assert!(!opened.edit.direct_source_write_allowed);
        assert_eq!(opened.template_state, page.template_state);
        assert_eq!(opened.flags.template_derived, page.flags.template_derived);
        assert_eq!(opened.next_action, "publish");
        assert_eq!(opened.hashes.source_sha256, None);
        assert_eq!(opened.hashes.talk_sha256, None);
    }

    #[test]
    fn page_status_distinguishes_runtime_defaults_from_custom_template_pages() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let wiki_toml = root.join("user-wiki/wiki.toml");
        let text = fs::read_to_string(&wiki_toml).unwrap().replace(
            "type = \"topic-index\"\ntemplate = \"pages/e08/topics.md\"",
            "type = \"topic-index\"\norigin = \"runtime_default\"\ntemplate = \"pages/e08/topics.md\"",
        );
        fs::write(&wiki_toml, text).unwrap();
        let core = WikiCore::new(&root);

        core.create_page("topics", None).unwrap();
        let runtime_default = core.page_status("topics").unwrap();
        assert_eq!(runtime_default.origin, "runtime_default");
        assert!(runtime_default.flags.template_derived);
        assert!(runtime_default.flags.runtime_default);
        assert!(!runtime_default.flags.custom_created);
        assert_eq!(
            runtime_default.template.relative_path,
            "pages/e08/topics.md"
        );
        assert!(runtime_default.template.sha256.is_some());
        assert_eq!(
            runtime_default.template.sha256,
            runtime_default.template.baseline_sha256
        );

        let custom = core
            .create_page_with_options(
                "custom-template-page",
                PageCreateOptions {
                    route: Some("/custom-template-page".to_string()),
                    nav_section: Some("primary".to_string()),
                    nav_order: Some(305),
                    ..test_page_options()
                },
                None,
            )
            .unwrap();
        let custom_status = custom.page_status.unwrap();
        assert_eq!(custom_status.nav_section.as_deref(), Some("primary"));
        assert_eq!(custom_status.nav_order, Some(305));
        assert_eq!(custom_status.origin, "created_from_template");
        assert!(custom_status.flags.template_derived);
        assert!(!custom_status.flags.runtime_default);
        assert!(custom_status.flags.custom_created);
        assert_eq!(custom_status.template.relative_path, "pages/e08/topics.md");
        assert!(custom_status.template.sha256.is_some());
        assert_eq!(
            custom_status.template.sha256,
            custom_status.template.baseline_sha256
        );
        assert!(fs::read_to_string(wiki_toml)
            .unwrap()
            .contains("origin = \"created_from_template\""));
        let list_status = core
            .inventory()
            .unwrap()
            .pages
            .into_iter()
            .find(|page| page.id == "custom-template-page")
            .unwrap();
        assert_eq!(list_status.nav_order, Some(305));
        let opened = core.open_page("custom-template-page").unwrap();
        assert_eq!(opened.nav_order, Some(305));
        assert_eq!(opened.page_status.nav_order, Some(305));
    }

    #[test]
    fn create_all_preserves_preexisting_configured_source_as_user_edited() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let wiki_toml = root.join("user-wiki/wiki.toml");
        let text = fs::read_to_string(&wiki_toml).unwrap().replace(
            "type = \"topic-index\"\ntemplate = \"pages/e08/topics.md\"",
            "type = \"topic-index\"\norigin = \"runtime_default\"\ntemplate = \"pages/e08/topics.md\"",
        );
        fs::write(&wiki_toml, text).unwrap();
        let source = root.join("user-wiki/source/families/reference/topics/source/topics.md");
        let custom_source = "---\ntitle: \"Topics\"\nslug: \"topics\"\nroute: \"/topics\"\nsection: \"Reference\"\naccess: \"private\"\n---\n\n# Topics\n\nOperator-authored source must survive create-all.\n";
        if let Some(parent) = source.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&source, custom_source).unwrap();

        let created = WikiCore::new(&root).create_all_pages().unwrap();
        let receipt = &created.created_or_checked[0];
        assert_eq!(receipt.id, "topics");
        assert!(receipt.evidence.iter().any(|entry| {
            entry.path == "user-wiki/source/families/reference/topics/source/topics.md"
                && entry.status == "skipped_existing"
        }));
        assert_eq!(fs::read_to_string(&source).unwrap(), custom_source);

        let status = receipt.page_status.as_ref().unwrap();
        assert_eq!(status.origin, "runtime_default");
        assert!(status.flags.runtime_default);
        assert!(!status.flags.custom_created);
        assert!(status.flags.user_edited);
        assert_eq!(status.content_state, "edited");
        assert_eq!(status.template_state, "edited_from_template");
        assert_eq!(status.template.relative_path, "pages/e08/topics.md");
        assert!(status.template.sha256.is_some());
        assert_eq!(status.template.baseline_sha256, None);
    }

    #[test]
    fn publish_preflight_create_all_receipt_uses_publish_operation() {
        let direct_temp = TempDir::new().unwrap();
        let direct_root = direct_temp.path().join("1Context");
        seed_fixture(&direct_root);
        let direct = WikiCore::new(&direct_root).create_all_pages().unwrap();
        assert_eq!(direct.operation, "wiki.page.create_all");
        assert_eq!(direct.page_count, 1);

        let preflight_temp = TempDir::new().unwrap();
        let preflight_root = preflight_temp.path().join("1Context");
        seed_fixture(&preflight_root);
        let preflight = WikiCore::new(&preflight_root)
            .create_all_pages_for_publish_preflight()
            .unwrap();
        assert_eq!(preflight.operation, "wiki.publish.preflight");
        assert_eq!(preflight.page_count, 1);
        assert_eq!(
            preflight.created_or_checked[0].operation,
            "wiki.page.create"
        );
    }

    #[test]
    fn same_subject_burst_talk_messages_get_distinct_ids() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let sender = "agent://codex/same-subject-burst-thread".to_string();

        let first = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Same subject burst".to_string(),
                thread_id: None,
                reply_to: None,
                from: sender.clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "First message.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap();
        let second = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Same subject burst".to_string(),
                thread_id: None,
                reply_to: None,
                from: sender.clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Second message.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap();
        let explicit_reply = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Changed subject should stay in parent thread".to_string(),
                thread_id: None,
                reply_to: Some(first.message_id.clone()),
                from: sender.clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Explicit parent reply.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap();
        let explicit_thread = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Explicit thread target".to_string(),
                thread_id: Some(first.thread_id.clone()),
                reply_to: None,
                from: sender.clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Explicit thread id reply.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap();

        assert_ne!(first.message_id, second.message_id);
        assert_eq!(first.thread_id, second.thread_id);
        assert_eq!(explicit_reply.thread_id, first.thread_id);
        assert_eq!(
            explicit_reply.reply_to.as_deref(),
            Some(first.message_id.as_str())
        );
        assert_eq!(explicit_thread.thread_id, first.thread_id);
        assert_eq!(explicit_thread.reply_to, None);
        let record = core.find_page("topics").unwrap();
        let messages = core.file_talk_messages(&record).unwrap();
        assert_eq!(messages.len(), 4);
        let explicit_reply_read = messages
            .iter()
            .find(|message| message.message_id == explicit_reply.message_id)
            .unwrap();
        assert_eq!(
            explicit_reply_read.reply_to.as_deref(),
            Some(first.message_id.as_str())
        );
    }

    #[test]
    fn talk_append_defaults_to_labels_only_without_mail_delivery() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let before_publish = core.publish_status().unwrap();

        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Labels only".to_string(),
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
                thread_id: None,
                reply_to: None,
                from: "agent://codex/labels-only".to_string(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "This should not create mail.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        let after_publish = core.publish_status().unwrap();
        assert_eq!(talk.delivery_mode, TalkDeliveryMode::LabelsOnly);
        assert!(talk.mail_delivery.is_none());
        assert!(!root.join("context-engine/mail/deliveries.jsonl").exists());
        assert!(!root
            .join("context-engine/live/mail/deliveries.jsonl")
            .exists());
        assert_eq!(before_publish.status, after_publish.status);
        assert_eq!(
            before_publish.render_required,
            after_publish.render_required
        );
        assert_eq!(
            before_publish.pages_needing_publish,
            after_publish.pages_needing_publish
        );
    }

    #[test]
    fn talk_append_explicit_mail_creates_message_and_delivery_rows() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Mail delivery".to_string(),
                operation_id: Some("talk-mail-delivery-001".to_string()),
                delivery_mode: TalkDeliveryMode::Mail,
                thread_id: None,
                reply_to: None,
                from: "agent://codex/talk-mail".to_string(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "This should create a mail delivery.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        let delivery = talk.mail_delivery.as_ref().unwrap();
        assert_eq!(delivery.status, "delivered");
        assert_eq!(delivery.acceptance.as_deref(), Some("accepted"));
        assert_eq!(delivery.attempt_count, 1);
        assert_eq!(delivery.attempts[0].recipient, "role://topics.curator");
        let store = agent_mail::AgentMailStore::new(root.join("context-engine/live"));
        let recipient = agent_mail::MailAddress::parse("role://topics.curator").unwrap();
        assert_eq!(
            store
                .recipient_summary(&recipient)
                .unwrap()
                .open_delivery_count,
            1
        );
        assert_eq!(
            store
                .read_message(&talk.message_id)
                .unwrap()
                .envelope
                .page
                .unwrap()
                .id,
            "topics"
        );
    }

    #[test]
    fn talk_append_mail_without_deliveries_is_not_reported_delivered() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "No recipients".to_string(),
                operation_id: Some("talk-mail-no-recipients-001".to_string()),
                delivery_mode: TalkDeliveryMode::Mail,
                thread_id: None,
                reply_to: None,
                from: "agent://codex/talk-mail-empty".to_string(),
                to: vec![],
                cc: vec![],
                body_markdown: "Mail mode without recipients must not look delivered.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        let delivery = talk.mail_delivery.as_ref().unwrap();
        assert_eq!(talk.status, "appended_delivery_deferred");
        assert_eq!(delivery.status, "no_deliveries");
        assert_eq!(delivery.attempt_count, 0);
        assert!(talk
            .repair_hints
            .iter()
            .any(|hint| hint.contains("did not create any delivered recipient records")));
    }

    #[test]
    fn talk_append_mail_retry_uses_stable_operation_id() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let request = TalkAppendRequest {
            page: "topics".to_string(),
            kind: "proposal".to_string(),
            subject: "Mail retry".to_string(),
            operation_id: Some("talk-mail-retry-001".to_string()),
            delivery_mode: TalkDeliveryMode::Mail,
            thread_id: None,
            reply_to: None,
            from: "agent://codex/talk-mail-retry".to_string(),
            to: vec!["role://topics.curator".to_string()],
            cc: vec![],
            body_markdown: "Retry should not duplicate delivery.".to_string(),
            attachments: vec![],
            allow_tombstoned: false,
        };

        let first = core.append_talk(request.clone()).unwrap();
        let retry = core.append_talk(request).unwrap();

        assert_eq!(first.message_id, retry.message_id);
        let retry_delivery = retry.mail_delivery.as_ref().unwrap();
        assert_eq!(
            retry_delivery.acceptance.as_deref(),
            Some("duplicate_same_payload")
        );
        assert_eq!(retry_delivery.attempts[0].status, "already_delivered");
        let record = core.find_page("topics").unwrap();
        let messages = core.file_talk_messages(&record).unwrap();
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn talk_append_delivery_failure_preserves_talk_source() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Bad mail recipient".to_string(),
                operation_id: Some("talk-mail-failure-001".to_string()),
                delivery_mode: TalkDeliveryMode::Mail,
                thread_id: None,
                reply_to: None,
                from: "agent://codex/talk-mail-failure".to_string(),
                to: vec!["not-an-address".to_string()],
                cc: vec![],
                body_markdown: "Talk should remain even when delivery fails.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        assert_eq!(talk.status, "appended_delivery_failed");
        assert_eq!(talk.mail_delivery.as_ref().unwrap().status, "failed");
        assert!(!talk.repair_hints.is_empty());
        let record = core.find_page("topics").unwrap();
        let messages = core.file_talk_messages(&record).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message_id, talk.message_id);
    }

    #[test]
    fn mail_mark_changes_do_not_change_publish_status() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Mark without publish".to_string(),
                operation_id: Some("talk-mail-mark-001".to_string()),
                delivery_mode: TalkDeliveryMode::Mail,
                thread_id: None,
                reply_to: None,
                from: "agent://codex/talk-mail-mark".to_string(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Marking this mail should stay outside publish inputs.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();
        let before_mark = core.publish_status().unwrap();

        let delivery_id = talk.mail_delivery.as_ref().unwrap().attempts[0]
            .delivery_id
            .clone();
        let store = agent_mail::AgentMailStore::new(root.join("context-engine/live"));
        let agent = store
            .identify_agent(
                &agent_mail::AgentIdentifyRequest {
                    transport_kind: "codex".to_string(),
                    thread_id: "mark-without-publish-agent".to_string(),
                    requested_roles: vec!["role://topics.curator".to_string()],
                    requested_capabilities: vec!["wiki.mail".to_string()],
                    lease_expires_at: "2026-05-21T10:00:00Z".to_string(),
                    occurred_at: "2026-05-21T09:00:00Z".to_string(),
                },
                &agent_mail::AgentGrantPolicy::allow_exact(
                    &["role://topics.curator"],
                    &["wiki.mail"],
                ),
            )
            .unwrap();
        store
            .mark_delivery(
                &delivery_id,
                &agent.agent_id,
                agent_mail::DeliveryState::Read,
                "2026-05-21T09:00:00Z",
            )
            .unwrap();
        let after_mark = core.publish_status().unwrap();

        assert_eq!(before_mark.status, after_mark.status);
        assert_eq!(before_mark.render_required, after_mark.render_required);
        assert_eq!(
            before_mark.pages_needing_publish,
            after_mark.pages_needing_publish
        );
    }

    #[test]
    fn write_new_refuses_to_overwrite_existing_talk_source() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("same-stamp.proposal.same-subject.md");
        write_new(&path, "first body\n".to_string()).unwrap();

        let error = write_new(&path, "second body\n".to_string()).unwrap_err();
        let chain = format!("{error:#}");

        assert!(
            chain.contains("File exists") || chain.contains("already exists"),
            "{chain}"
        );
        assert_eq!(fs::read_to_string(path).unwrap(), "first body\n");
    }

    #[test]
    fn talk_append_can_start_with_explicit_thread_id() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let thread_id = "agent-chosen-thread-123";

        let started = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Agent chosen thread".to_string(),
                thread_id: Some(thread_id.to_string()),
                reply_to: None,
                from: "agent://codex/explicit-new-talk-thread".to_string(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "A sender can choose a new correlation id.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap();

        assert_eq!(started.thread_id, thread_id);
        assert_eq!(started.reply_to, None);
        let record = core.find_page("topics").unwrap();
        let messages = core.file_talk_messages(&record).unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message_id, started.message_id);
    }

    #[test]
    fn file_only_talk_messages_can_be_reply_and_thread_targets() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let sender = "agent://codex/file-only-talk-target-thread".to_string();
        let talk_dir = root.join("user-wiki/source/families/reference/topics/talk/topics.talk");
        let parent_id = "talkmsg_file_only_parent";
        let parent_thread = "thread_topics_file_only_parent";
        fs::create_dir_all(talk_dir.join("attachments").join(parent_id)).unwrap();
        fs::write(
            talk_dir
                .join("attachments")
                .join(parent_id)
                .join("file-note.txt"),
            "file attachment",
        )
        .unwrap();
        fs::write(
            talk_dir.join("2026-05-20T00-00-00-000000000Z.proposal.file-only-parent.md"),
            format!(
                "---\nid: \"{parent_id}\"\nkind: \"proposal\"\nauthor: \"agent://codex/fileonly\"\ncreated: \"2026-05-20T00:00:00Z\"\ntalk_for: \"mailbox://page/topics\"\nthread: \"{parent_thread}\"\nsubject: \"File only parent\"\nstate: open\nrecipients:\n  - \"role://topics.curator\"\nattachments:\n  - filename: \"file-note.txt\"\n    media_type: \"text/plain\"\n    path: \"attachments/{parent_id}/file-note.txt\"\n    handle: \"user-wiki://page/topics/talk/attachments/{parent_id}/file-note.txt\"\n    caption: \"File note\"\n    alt_text: \"File note alt\"\n---\n\n## File only parent\n\nParent message with no delivery rows.\n"
            ),
        )
        .unwrap();
        let direct_thread = "thread_topics_file_only_direct";
        fs::write(
            talk_dir.join("2026-05-20T00-01-00-000000000Z.question.file-only-direct.md"),
            format!(
                "---\nid: \"talkmsg_file_only_direct\"\nkind: \"question\"\nauthor: \"agent://codex/fileonly\"\ncreated: \"2026-05-20T00:01:00Z\"\ntalk_for: \"mailbox://page/topics\"\nthread: \"{direct_thread}\"\nsubject: \"File only direct\"\nstate: open\nrecipients:\n  - \"role://topics.curator\"\nattachments: []\n---\n\n## File only direct\n\nThread target message with no delivery rows.\n"
            ),
        )
        .unwrap();

        let reply = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Changed subject file-only reply".to_string(),
                thread_id: None,
                reply_to: Some(parent_id.to_string()),
                from: sender.clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Reply should inherit the file-only parent thread.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap();
        let direct = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Direct file-only thread target".to_string(),
                thread_id: Some(direct_thread.to_string()),
                reply_to: None,
                from: sender,
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Explicit thread target should resolve from talk files.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap();

        assert_eq!(reply.thread_id, parent_thread);
        assert_eq!(reply.reply_to.as_deref(), Some(parent_id));
        assert_eq!(direct.thread_id, direct_thread);
        assert_eq!(direct.reply_to, None);

        let record = core.find_page("topics").unwrap();
        let messages = core.file_talk_messages(&record).unwrap();
        let parent_thread_messages = messages
            .iter()
            .filter(|message| message.thread_id == parent_thread)
            .collect::<Vec<_>>();
        assert_eq!(parent_thread_messages.len(), 2);
        let hydrated_parent = parent_thread_messages
            .iter()
            .copied()
            .find(|message| message.message_id == parent_id)
            .unwrap();
        assert_eq!(hydrated_parent.thread_id, parent_thread);
        assert_eq!(hydrated_parent.subject, "File only parent");
        assert_eq!(hydrated_parent.attachments.len(), 1);
        assert_eq!(hydrated_parent.attachments[0].filename, "file-note.txt");
        assert_eq!(hydrated_parent.attachments[0].media_type, "text/plain");
        assert_eq!(
            hydrated_parent.attachments[0].path,
            format!("attachments/{parent_id}/file-note.txt")
        );
        assert_eq!(
            hydrated_parent.attachments[0].handle,
            format!("user-wiki://page/topics/talk/attachments/{parent_id}/file-note.txt")
        );
        assert_eq!(
            hydrated_parent.attachments[0].caption.as_deref(),
            Some("File note")
        );
        assert_eq!(
            hydrated_parent.attachments[0].alt_text.as_deref(),
            Some("File note alt")
        );
        assert!(messages
            .iter()
            .any(|message| message.message_id == reply.message_id));
        let direct_thread_messages = messages
            .iter()
            .filter(|message| message.thread_id == direct_thread)
            .collect::<Vec<_>>();
        assert_eq!(direct_thread_messages.len(), 2);
        assert!(direct_thread_messages
            .iter()
            .any(|message| message.message_id == "talkmsg_file_only_direct"));
        assert!(direct_thread_messages
            .iter()
            .any(|message| message.message_id == direct.message_id));
    }

    #[test]
    fn rendered_page_without_open_work_has_no_next_action() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        fs::create_dir_all(root.join("user-wiki/site/.1context")).unwrap();
        fs::write(
            root.join("user-wiki/site/topics.html"),
            "<main>Topics</main>",
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/site/.1context/source-fingerprint.txt"),
            format!("{}\n", core.publish_fingerprint().unwrap()),
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/site/.1context/page-fingerprints.json"),
            serde_json::to_vec_pretty(&core.page_publish_fingerprints().unwrap()).unwrap(),
        )
        .unwrap();

        let status = core.page_status("topics").unwrap();
        assert_eq!(status.state, "rendered");
        assert_eq!(status.validation.status, "ok");
        assert_eq!(status.next_action, "none");
        assert!(status
            .allowed_actions
            .contains(&"wiki.page.open".to_string()));
    }

    #[test]
    fn page_mutation_receipts_include_chainable_status_and_edit_preconditions() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        let create = core.create_page("topics", None).unwrap();
        assert_eq!(create.next_action, "publish");
        assert_eq!(create.route.as_deref(), Some("/topics"));
        assert_eq!(create.page_type.as_deref(), Some("topic-index"));
        assert_eq!(create.collection.as_deref(), Some("reference"));
        assert_eq!(create.page_status.as_ref().unwrap().state, "needs_publish");
        let create_hash = create
            .edit
            .as_ref()
            .unwrap()
            .expected_source_sha256
            .clone()
            .unwrap();
        assert_eq!(
            Some(create_hash.as_str()),
            create.hashes.as_ref().unwrap().source_sha256.as_deref()
        );

        let write = core
            .write_page_body(
                "topics",
                "# Topics\n\nFirst receipt body.\n",
                Some(&create_hash),
                &Actor::operator(),
            )
            .unwrap();
        assert_eq!(write.next_action, "publish");
        assert_eq!(write.route.as_deref(), Some("/topics"));
        assert_eq!(write.page_type.as_deref(), Some("topic-index"));
        assert_eq!(write.collection.as_deref(), Some("reference"));
        assert_eq!(write.page_status.as_ref().unwrap().content_state, "edited");
        let write_hash = write
            .edit
            .as_ref()
            .unwrap()
            .expected_source_sha256
            .clone()
            .unwrap();
        assert_ne!(write_hash, create_hash);

        let patch = core
            .patch_page_body(
                "topics",
                "First receipt body",
                "Second receipt body",
                Some(&write_hash),
                &Actor::operator(),
            )
            .unwrap();
        assert_eq!(patch.next_action, "publish");
        assert_eq!(patch.route.as_deref(), Some("/topics"));
        assert_eq!(patch.page_type.as_deref(), Some("topic-index"));
        assert_eq!(patch.collection.as_deref(), Some("reference"));
        assert_eq!(patch.page_status.as_ref().unwrap().state, "needs_publish");
        let patch_hash = patch
            .edit
            .as_ref()
            .unwrap()
            .expected_source_sha256
            .clone()
            .unwrap();
        assert_ne!(patch_hash, write_hash);

        let delete = core.delete_page("topics", "tombstone").unwrap();
        assert_eq!(delete.route.as_deref(), Some("/topics"));
        assert_eq!(delete.page_type.as_deref(), Some("topic-index"));
        assert_eq!(delete.collection.as_deref(), Some("reference"));
        assert_eq!(delete.page_status.as_ref().unwrap().state, "tombstoned");
        assert_eq!(
            delete.page_status.as_ref().unwrap().template_state,
            "edited_from_template"
        );
        assert!(!delete.edit.as_ref().unwrap().safe_to_edit);

        let restore = core.restore_page("topics").unwrap();
        assert_eq!(restore.route.as_deref(), Some("/topics"));
        assert_eq!(restore.page_type.as_deref(), Some("topic-index"));
        assert_eq!(restore.collection.as_deref(), Some("reference"));
        assert_eq!(restore.page_status.as_ref().unwrap().state, "needs_publish");
        assert!(restore.edit.as_ref().unwrap().safe_to_edit);
        assert_eq!(
            restore.edit.as_ref().unwrap().expected_source_sha256,
            Some(patch_hash)
        );
    }

    #[test]
    fn page_assets_are_addable_listable_and_embed_ready() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let input = temp.path().join("topic map.png");
        fs::write(&input, b"fake png bytes").unwrap();

        let added = core
            .add_page_asset(
                "topics",
                &input,
                None,
                None,
                None,
                Some("Current topic taxonomy sketch"),
            )
            .unwrap();
        assert_eq!(added.operation, "wiki.asset.add");
        assert_eq!(added.next_action, "insert_markdown");
        assert!(added.render_required);
        assert_eq!(added.asset.filename, "topic-map.png");
        assert_eq!(
            added.asset.citation_uri,
            "user-wiki://page/topics/assets/topic-map.png"
        );
        assert_eq!(added.asset.media_type, "image/png");
        assert_eq!(added.asset.kind, "image");
        assert_eq!(added.asset.content_role, "image");
        assert_eq!(
            added.asset.citation_uri,
            "user-wiki://page/topics/assets/topic-map.png"
        );
        assert_eq!(added.asset.purpose, "inline_image");
        assert_eq!(
            added.asset.source_relative_href,
            "./topics.assets/topic-map.png"
        );
        assert_eq!(added.asset.published_href, "/topics.assets/topic-map.png");
        assert_eq!(
            added.asset.markdown,
            "![Current topic taxonomy sketch](./topics.assets/topic-map.png)"
        );
        assert!(!added.asset.referenced);
        assert!(!added.asset.published);
        assert!(root
            .join("user-wiki/source/families/reference/topics/source/topics.assets/topic-map.png")
            .is_file());

        core.write_page_body(
            "topics",
            &format!("# Topics\n\n{}\n", added.asset.markdown),
            None,
            &Actor::operator(),
        )
        .unwrap();

        let listed = core.list_page_assets("topics").unwrap();
        assert_eq!(listed.operation, "wiki.asset.list");
        assert_eq!(listed.asset_count, 1);
        assert_eq!(listed.next_action, "publish");
        assert_eq!(listed.assets[0].filename, "topic-map.png");
        assert!(listed.assets[0].referenced);
        assert!(!listed.assets[0].published);
    }

    #[test]
    fn page_assets_classify_source_code_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let input = temp.path().join("Renderer.swift");
        fs::write(
            &input,
            b"let status = try await wiki.publish(trigger: \"agent\")\n",
        )
        .unwrap();

        let added = core
            .add_page_asset(
                "topics",
                &input,
                None,
                Some("source_file"),
                Some("Renderer source excerpt"),
                None,
            )
            .unwrap();
        assert_eq!(added.asset.kind, "code_file");
        assert_eq!(added.asset.content_role, "source_file");
        assert_eq!(added.asset.media_type, "text/plain");
        assert_eq!(
            added.asset.citation_uri,
            "user-wiki://page/topics/assets/Renderer.swift"
        );
        assert_eq!(
            added.asset.markdown,
            "[Renderer source excerpt](./topics.assets/Renderer.swift)"
        );
    }

    #[test]
    fn reference_list_reads_published_reference_index_and_filters_page_scope() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        let missing = core.list_references(None).unwrap();
        assert_eq!(missing.status, "missing");
        assert_eq!(missing.next_action, "publish");

        core.create_page("topics", None).unwrap();
        let metadata_dir = root.join("user-wiki/site/.1context");
        fs::create_dir_all(&metadata_dir).unwrap();
        fs::write(
            metadata_dir.join("reference-index.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "schema_version": "wiki.reference-index.v1",
                "assets": [
                    {
                        "kind": "image",
                        "page_id": "topics",
                        "route": "/topics",
                        "citation_uri": "user-wiki://page/topics/assets/topic-map.png"
                    },
                    {
                        "kind": "file",
                        "page_id": "other",
                        "route": "/other",
                        "citation_uri": "user-wiki://page/other/assets/data.csv"
                    }
                ],
                "links": [
                    {
                        "kind": "link",
                        "page_id": "topics",
                        "route": "/topics",
                        "href": "https://example.com"
                    },
                    {
                        "kind": "link",
                        "page_id": "other",
                        "route": "/other",
                        "href": "/topics"
                    }
                ],
                "code_blocks": [
                    {
                        "kind": "code_block",
                        "page_id": "topics",
                        "route": "/topics",
                        "id": "code-topics-001"
                    }
                ],
                "citations": [
                    {
                        "kind": "citation",
                        "page_id": "topics",
                        "route": "/topics",
                        "id": "cite-note-topics-1",
                        "citation_uri": "user-wiki://page/topics/citations/cite-note-topics-1"
                    }
                ]
            }))
            .unwrap(),
        )
        .unwrap();

        let all = core.list_references(None).unwrap();
        assert_eq!(all.status, "ok");
        assert_eq!(all.reference_count, 6);
        assert_eq!(all.asset_count, 2);
        assert_eq!(all.link_count, 2);
        assert_eq!(all.code_block_count, 1);
        assert_eq!(all.citation_count, 1);

        let topics = core.list_references(Some("/topics")).unwrap();
        assert_eq!(topics.page.as_ref().unwrap().id, "topics");
        assert_eq!(topics.reference_count, 4);
        assert_eq!(topics.asset_count, 1);
        assert_eq!(topics.link_count, 1);
        assert_eq!(topics.code_block_count, 1);
        assert_eq!(topics.citation_count, 1);
        assert_eq!(
            topics.assets[0]
                .get("citation_uri")
                .and_then(serde_json::Value::as_str),
            Some("user-wiki://page/topics/assets/topic-map.png")
        );
    }

    #[test]
    fn page_restore_reopens_tombstoned_page_and_navigation() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let delete = core.delete_page("topics", "tombstone").unwrap();
        assert_eq!(delete.next_action, "none");
        let tombstoned = core.page_status("topics").unwrap();
        assert_eq!(tombstoned.state, "tombstoned");

        let restore = core.restore_page("topics").unwrap();
        assert_eq!(restore.operation, "wiki.page.restore");
        assert!(restore.render_required);
        assert_eq!(restore.next_action, "publish");
        let restored = core.page_status("topics").unwrap();
        assert_eq!(restored.state, "needs_publish");
        assert!(restored.flags.enabled);
        assert!(!restored.flags.tombstoned);

        let config = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        assert!(config.contains("enabled = true"));
        assert!(config.contains("\"topics\""));
    }

    #[test]
    fn tombstoned_edited_page_keeps_user_edited_metadata_in_status_and_list() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        let create = core.create_page("topics", None).unwrap();
        let create_hash = create
            .edit
            .as_ref()
            .unwrap()
            .expected_source_sha256
            .clone()
            .unwrap();
        core.write_page_body(
            "topics",
            "# Topics\n\nOperator-edited body before tombstone.\n",
            Some(&create_hash),
            &Actor::operator(),
        )
        .unwrap();

        let before_delete = core.page_status("topics").unwrap();
        assert_eq!(before_delete.content_state, "edited");
        assert_eq!(before_delete.template_state, "edited_from_template");
        assert!(before_delete.flags.user_edited);

        core.delete_page("topics", "tombstone").unwrap();
        let tombstoned_status = core.page_status("topics").unwrap();
        assert_eq!(tombstoned_status.state, "tombstoned");
        assert_eq!(tombstoned_status.content_state, "tombstoned");
        assert_eq!(tombstoned_status.template_state, "edited_from_template");
        assert!(tombstoned_status.flags.tombstoned);
        assert!(tombstoned_status.flags.user_edited);

        let tombstoned_list_page = core
            .inventory()
            .unwrap()
            .pages
            .into_iter()
            .find(|page| page.id == "topics")
            .unwrap();
        assert_eq!(tombstoned_list_page.state, "tombstoned");
        assert_eq!(tombstoned_list_page.content_state, "tombstoned");
        assert_eq!(tombstoned_list_page.template_state, "edited_from_template");
        assert!(tombstoned_list_page.flags.user_edited);
    }

    #[test]
    fn page_delete_link_impact_includes_talk_markdown_links() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let mut config = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        config.push_str(
            r#"

[[pages]]
id = "target"
enabled = true
title = "Target"
slug = "target"
route = "/target"
family_group = "reference"
family_group_title = "Reference"
family_id = "target"
family_title = "Target"
type = "topic-index"
template = "pages/e08/topics.md"
talk_conventions_template = "talk/conventions/topics.md"
talk_curator_template = "talk/curators/topics.md"
"#,
        );
        fs::write(root.join("user-wiki/wiki.toml"), config).unwrap();
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        core.create_page("target", None).unwrap();
        core.write_page_body(
            "topics",
            "# Topics\n\nArticle link to [Target](/target).\n",
            None,
            &Actor::operator(),
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/source/families/reference/topics/talk/topics.talk/2026-05-20T00-00-00-000000000Z.proposal.target-link.md"),
            "---\nsubject: \"Target link\"\n---\n\nTalk link to [Target](/target).\n",
        )
        .unwrap();

        let delete = core.delete_page("target", "tombstone").unwrap();
        assert_eq!(delete.next_action, "repair_links");
        let impact = delete.link_impact.unwrap();
        assert_eq!(impact.inbound_link_count, 2);
        assert!(impact
            .issues
            .iter()
            .any(|issue| issue.source_page_id == "topics"
                && issue.phase == "pre_delete_source_link_scan"));
        assert!(impact
            .issues
            .iter()
            .any(|issue| issue.source_page_id == "topics.talk"
                && issue.source_route == "/topics/talk"
                && issue.phase == "pre_delete_talk_link_scan"));
    }

    #[test]
    fn page_open_closed_pages_points_agents_at_restore() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        core.delete_page("topics", "tombstone").unwrap();
        let tombstoned_status = core.page_status("topics").unwrap();
        let tombstoned_open = core.open_page("topics").unwrap();
        assert_eq!(tombstoned_open.state, "tombstoned");
        assert_eq!(tombstoned_open.talk_state, tombstoned_status.talk_state);
        assert!(!tombstoned_open.edit.safe_to_edit);
        assert_eq!(
            tombstoned_open.edit.recommended_operation,
            "wiki.page.restore"
        );
        assert!(tombstoned_open
            .allowed_actions
            .contains(&"wiki.page.restore".to_string()));
        assert!(!tombstoned_open
            .allowed_actions
            .contains(&"wiki.talk.append".to_string()));

        core.restore_page("topics").unwrap();
        let config_path = root.join("user-wiki/wiki.toml");
        let disabled_config = fs::read_to_string(&config_path).unwrap().replacen(
            "enabled = true",
            "enabled = false",
            1,
        );
        fs::write(&config_path, disabled_config).unwrap();

        let disabled_status = core.page_status("topics").unwrap();
        let disabled_open = core.open_page("topics").unwrap();
        assert_eq!(disabled_open.state, "disabled");
        assert_eq!(disabled_open.talk_state, disabled_status.talk_state);
        assert!(!disabled_open.edit.safe_to_edit);
        assert_eq!(
            disabled_open.edit.recommended_operation,
            "wiki.page.restore"
        );
        assert!(disabled_open
            .allowed_actions
            .contains(&"wiki.page.restore".to_string()));
        assert!(!disabled_open
            .allowed_actions
            .contains(&"wiki.talk.append".to_string()));
    }

    #[test]
    fn inventory_and_status_include_generated_site_pages() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let mut config = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        config.push_str(
            r#"

[[site_pages]]
id = "home"
enabled = true
title = "Home"
kind = "generated"
route = "/"
template = "site/e08/index.md"

[[site_pages]]
id = "open-questions"
enabled = true
title = "Open Questions"
kind = "generated"
route = "/open-questions"
template = "site/e08/open-questions.md"
"#,
        );
        fs::write(root.join("user-wiki/wiki.toml"), config).unwrap();
        let core = WikiCore::new(&root);

        let inventory = core.inventory().unwrap();
        assert_eq!(inventory.page_count, 3);
        assert_eq!(inventory.source_page_count, 1);
        assert_eq!(inventory.generated_page_count, 2);
        let home = inventory
            .pages
            .iter()
            .find(|page| page.id == "home")
            .unwrap();
        assert_eq!(home.kind, "generated_site_page");
        assert_eq!(home.page_type, "generated");
        assert_eq!(home.route, "/");
        assert_eq!(home.template.relative_path, "site/e08/index.md");
        assert_eq!(home.template.baseline_sha256, None);
        assert!(!home.flags.source_backed);
        assert_eq!(home.next_action, "publish");

        let home_status = core.page_status("home").unwrap();
        assert_eq!(home_status.kind, "generated_site_page");
        let route_status = core.page_status("/open-questions").unwrap();
        assert_eq!(route_status.id, "open-questions");

        let status = core.status().unwrap();
        assert_eq!(status.page_count, 3);
        assert_eq!(status.source_page_count, 1);
        assert_eq!(status.generated_page_count, 2);
    }

    #[test]
    fn page_write_expected_hash_is_checked_under_lifecycle_lock() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let expected = core
            .open_page("topics")
            .unwrap()
            .hashes
            .source_sha256
            .unwrap();

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let first = {
            let barrier = barrier.clone();
            let root = root.clone();
            let expected = expected.clone();
            std::thread::spawn(move || {
                barrier.wait();
                WikiCore::new(root)
                    .write_page_body(
                        "topics",
                        "\n# First writer\n",
                        Some(&expected),
                        &Actor::operator(),
                    )
                    .map(|receipt| receipt.status)
                    .map_err(|error| error.to_string())
            })
        };
        let second = {
            let barrier = barrier.clone();
            let root = root.clone();
            let expected = expected.clone();
            std::thread::spawn(move || {
                barrier.wait();
                WikiCore::new(root)
                    .write_page_body(
                        "topics",
                        "\n# Second writer\n",
                        Some(&expected),
                        &Actor::operator(),
                    )
                    .map(|receipt| receipt.status)
                    .map_err(|error| error.to_string())
            })
        };

        let results = vec![first.join().unwrap(), second.join().unwrap()];
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let stale = results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .next()
            .unwrap();
        assert!(stale.contains("source hash mismatch"), "{stale}");
    }

    #[test]
    fn repeated_tombstone_and_restore_do_not_duplicate_lifecycle_events() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let first_delete = core.delete_page("topics", "tombstone").unwrap();
        let second_delete = core.delete_page("topics", "tombstone").unwrap();
        assert!(first_delete
            .evidence
            .iter()
            .any(|entry| entry.status == "created"));
        assert!(second_delete
            .evidence
            .iter()
            .any(|entry| entry.status == "skipped_existing"));

        let first_restore = core.restore_page("topics").unwrap();
        let second_restore = core.restore_page("topics").unwrap();
        assert_eq!(first_restore.evidence[0].status, "removed_tombstone");
        assert_eq!(second_restore.evidence[0].status, "no_tombstone");

        let page_events =
            read_jsonl::<serde_json::Value>(&root.join("user-wiki/.1context/page-ledger.jsonl"))
                .unwrap();
        assert_eq!(
            page_events
                .iter()
                .filter(|event| {
                    event.get("page").and_then(serde_json::Value::as_str) == Some("topics")
                        && event.get("event").and_then(serde_json::Value::as_str)
                            == Some("page.tombstoned")
                })
                .count(),
            1
        );
        assert_eq!(
            page_events
                .iter()
                .filter(|event| {
                    event.get("page").and_then(serde_json::Value::as_str) == Some("topics")
                        && event.get("event").and_then(serde_json::Value::as_str)
                            == Some("page.restored")
                })
                .count(),
            1
        );
    }

    #[test]
    fn talk_attachments_copy_media_and_duplicate_names() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let inputs = temp.path().join("inputs");
        fs::create_dir_all(inputs.join("a")).unwrap();
        fs::create_dir_all(inputs.join("b")).unwrap();
        fs::write(inputs.join("note.txt"), "plain text").unwrap();
        fs::write(inputs.join("data.json"), "{\"ok\":true}\n").unwrap();
        fs::write(inputs.join("handoff.eml"), "Subject: proof\n\nmessage").unwrap();
        fs::write(inputs.join("photo.PNG"), b"\x89PNG\r\n\x1a\n").unwrap();
        fs::write(inputs.join("a/duplicate.txt"), "first").unwrap();
        fs::write(inputs.join("b/duplicate.txt"), "second").unwrap();
        fs::write(inputs.join("bad name#?.txt"), "punctuation").unwrap();
        let mut named_note = attachment_input(inputs.join("note.txt"));
        named_note.filename = Some("agent-facing-note.txt".to_string());
        named_note.caption = Some("Agent-facing caption".to_string());
        named_note.alt_text = Some("Agent-facing alt text".to_string());

        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Attachment edge success".to_string(),
                thread_id: None,
                reply_to: None,
                from: "agent://codex/probe-c-agent".to_string(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Probe C success body.".to_string(),
                attachments: vec![
                    named_note,
                    attachment_input(inputs.join("data.json")),
                    attachment_input(inputs.join("handoff.eml")),
                    attachment_input(inputs.join("photo.PNG")),
                    attachment_input(inputs.join("a/duplicate.txt")),
                    attachment_input(inputs.join("b/duplicate.txt")),
                    attachment_input(inputs.join("bad name#?.txt")),
                ],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap();

        let filenames = talk
            .attachments
            .iter()
            .map(|attachment| attachment.filename.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            filenames,
            vec![
                "agent-facing-note.txt",
                "data.json",
                "handoff.eml",
                "photo.PNG",
                "duplicate.txt",
                "duplicate-2.txt",
                "bad-name.txt"
            ]
        );

        let media_types = talk
            .attachments
            .iter()
            .map(|attachment| (attachment.filename.as_str(), attachment.media_type.as_str()))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(media_types["agent-facing-note.txt"], "text/plain");
        assert_eq!(media_types["data.json"], "application/json");
        assert_eq!(media_types["handoff.eml"], "text/plain");
        assert_eq!(media_types["photo.PNG"], "image/png");
        assert_eq!(
            talk.attachments[0].caption.as_deref(),
            Some("Agent-facing caption")
        );
        assert_eq!(
            talk.attachments[0].alt_text.as_deref(),
            Some("Agent-facing alt text")
        );
        assert_eq!(talk.attachment_count, 7);

        let talk_dir = root.join("user-wiki/source/families/reference/topics/talk/topics.talk");
        for attachment in &talk.attachments {
            assert!(talk_dir.join(&attachment.path).is_file());
            assert!(attachment.path.starts_with("attachments/"));
            assert!(attachment
                .handle
                .starts_with("user-wiki://page/topics/talk/attachments/"));
        }
        let talk_source = fs::read_dir(&talk_dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .find(|path| {
                path.extension().and_then(|extension| extension.to_str()) == Some("md")
                    && fs::read_to_string(path)
                        .unwrap_or_default()
                        .contains(&talk.message_id)
            })
            .expect("talk message source exists");
        let talk_text = fs::read_to_string(talk_source).unwrap();
        assert!(talk_text.contains("](/topics/talk/attachments/"));
    }

    #[test]
    fn invalid_talk_attachments_do_not_leave_orphan_directories() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let talk_dir = root.join("user-wiki/source/families/reference/topics/talk/topics.talk");
        let attachments_dir = talk_dir.join("attachments");

        let missing_error = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Attachment missing file".to_string(),
                thread_id: None,
                reply_to: None,
                from: "agent://probe-c.sender".to_string(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Should fail missing.".to_string(),
                attachments: vec![attachment_input(temp.path().join("missing.txt"))],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap_err()
            .to_string();
        assert!(missing_error.contains("attachment not found"));
        assert!(!attachments_dir.exists());

        let unsafe_dir = temp.path().join("unsafe");
        fs::create_dir_all(&unsafe_dir).unwrap();
        let unsafe_path = unsafe_dir.join("unsafe\\name.txt");
        fs::write(&unsafe_path, "unsafe filename").unwrap();
        let unsafe_error = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Attachment unsafe filename".to_string(),
                thread_id: None,
                reply_to: None,
                from: "agent://probe-c.sender".to_string(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Should fail unsafe.".to_string(),
                attachments: vec![attachment_input(unsafe_path)],
                allow_tombstoned: false,
                operation_id: None,
                delivery_mode: TalkDeliveryMode::LabelsOnly,
            })
            .unwrap_err()
            .to_string();
        assert!(unsafe_error.contains("invalid attachment filename"));
        assert!(!attachments_dir.exists());
    }

    #[test]
    fn page_create_rejects_bad_sitemap_inputs_before_page_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        for (page_id, options, expected, artifact_slug) in [
            (
                "bad-relative-route",
                PageCreateOptions {
                    route: Some("relative".to_string()),
                    ..test_page_options()
                },
                "invalid page route",
                "bad-relative-route",
            ),
            (
                "bad-trailing-route",
                PageCreateOptions {
                    route: Some("/bad-trailing-route/".to_string()),
                    ..test_page_options()
                },
                "invalid page route",
                "bad-trailing-route",
            ),
            (
                "bad-uppercase-slug",
                PageCreateOptions {
                    slug: Some("BadSlug".to_string()),
                    route: Some("/bad-uppercase-slug".to_string()),
                    ..test_page_options()
                },
                "invalid page slug",
                "BadSlug",
            ),
            (
                "bad-nav-section",
                PageCreateOptions {
                    route: Some("/bad-nav-section".to_string()),
                    nav_section: Some("sidebar".to_string()),
                    ..test_page_options()
                },
                "invalid nav section",
                "bad-nav-section",
            ),
            (
                "bad-family-group",
                PageCreateOptions {
                    family_group: Some("../outside".to_string()),
                    route: Some("/bad-family-group".to_string()),
                    ..test_page_options()
                },
                "invalid page family_group",
                "bad-family-group",
            ),
            (
                "bad-family-id",
                PageCreateOptions {
                    family_id: Some("../outside".to_string()),
                    route: Some("/bad-family-id".to_string()),
                    ..test_page_options()
                },
                "invalid page family_id",
                "bad-family-id",
            ),
        ] {
            let before = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
            let error = core
                .create_page_with_options(page_id, options, None)
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "unexpected error: {error}");
            assert_eq!(
                fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap(),
                before
            );
            assert!(
                !root
                    .join("user-wiki/source/families/custom")
                    .join(artifact_slug)
                    .exists(),
                "{page_id} should not leave page source/talk artifacts"
            );
            assert!(!root.join("user-wiki/source/outside").exists());
            assert!(!root.join("user-wiki/source/families/outside").exists());
        }
    }

    #[test]
    fn page_create_rejects_route_and_source_path_collisions() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        core.create_page_with_options(
            "route-owner",
            PageCreateOptions {
                route: Some("/route-owner".to_string()),
                ..test_page_options()
            },
            None,
        )
        .unwrap();
        let before = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        let duplicate_id_conflict = core
            .create_page_with_options(
                "route-owner",
                PageCreateOptions {
                    route: Some("/different-route-owner".to_string()),
                    ..test_page_options()
                },
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(duplicate_id_conflict.contains("page already exists"));
        assert_eq!(
            fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap(),
            before
        );

        let duplicate_route = core
            .create_page_with_options(
                "route-duplicate",
                PageCreateOptions {
                    route: Some("/route-owner".to_string()),
                    ..test_page_options()
                },
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(duplicate_route.contains("route already exists"));
        assert_eq!(
            fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap(),
            before
        );
        assert!(!root
            .join("user-wiki/source/families/custom/route-duplicate")
            .exists());

        let mut wiki_toml = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        wiki_toml.push_str(
            r#"

[[site_pages]]
id = "home"
enabled = true
title = "Home"
kind = "generated"
route = "/"
template = "site/e08/index.md"
"#,
        );
        fs::write(root.join("user-wiki/wiki.toml"), wiki_toml).unwrap();
        let before_site_page_conflict =
            fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        let site_page_id_conflict = core
            .create_page_with_options("home", test_page_options(), None)
            .unwrap_err()
            .to_string();
        assert!(site_page_id_conflict.contains("generated site page home at /"));
        assert_eq!(
            fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap(),
            before_site_page_conflict
        );
        assert!(!root.join("user-wiki/source/families/custom/home").exists());

        let talk_route_conflict = core
            .create_page_with_options(
                "route-talk-conflict",
                PageCreateOptions {
                    route: Some("/route-owner/talk".to_string()),
                    ..test_page_options()
                },
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(talk_route_conflict.contains("route already exists"));

        core.create_page_with_options(
            "source-owner",
            PageCreateOptions {
                slug: Some("shared-source".to_string()),
                route: Some("/source-owner".to_string()),
                ..test_page_options()
            },
            None,
        )
        .unwrap();
        let before_source_conflict = fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap();
        let source_conflict = core
            .create_page_with_options(
                "source-conflict",
                PageCreateOptions {
                    slug: Some("shared-source".to_string()),
                    route: Some("/source-conflict".to_string()),
                    ..test_page_options()
                },
                None,
            )
            .unwrap_err()
            .to_string();
        assert!(source_conflict.contains("page source path already exists"));
        assert_eq!(
            fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap(),
            before_source_conflict
        );
    }

    #[test]
    fn validate_reports_registry_sitemap_conflicts_as_blocking() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        fs::write(
            root.join("user-wiki/wiki.toml"),
            format!(
                "{}\n\n[site]\nnavigation = [\"topics\", \"missing-nav\"]\nprimary_navigation = [\"topics\"]\nutility_navigation = []\n\n[[pages]]\nid = \"topics\"\nenabled = true\ntitle = \"Duplicate Topics\"\nslug = \"topics-copy\"\nroute = \"/topics\"\nfamily_group = \"reference\"\nfamily_group_title = \"Reference\"\nfamily_id = \"topics-copy\"\nfamily_title = \"Duplicate Topics\"\ntype = \"topic-index\"\ntemplate = \"pages/e08/topics.md\"\ntalk_conventions_template = \"talk/conventions/topics.md\"\ntalk_curator_template = \"talk/curators/topics.md\"\n\n[[pages]]\nid = \"bad-config\"\nenabled = true\ntitle = \"Bad Config\"\nslug = \"BadSlug\"\nroute = \"relative\"\nfamily_group = \"custom\"\nfamily_group_title = \"Custom\"\nfamily_id = \"bad-config\"\nfamily_title = \"Bad Config\"\ntype = \"context-page\"\ntemplate = \"pages/e08/topics.md\"\ntalk_conventions_template = \"talk/conventions/topics.md\"\ntalk_curator_template = \"talk/curators/topics.md\"\nnav_section = \"sidebar\"\nnav_order = \"late\"\n",
                fs::read_to_string(root.join("user-wiki/wiki.toml")).unwrap()
            ),
        )
        .unwrap();

        let validation = WikiCore::new(&root).validate().unwrap();
        let codes = validation
            .issues
            .iter()
            .map(|issue| issue.code.as_str())
            .collect::<BTreeSet<_>>();
        for code in [
            "duplicate_page_id",
            "duplicate_route",
            "invalid_page_slug",
            "invalid_page_route",
            "invalid_nav_section",
            "invalid_nav_order",
            "unknown_navigation_page",
        ] {
            assert!(codes.contains(code), "missing {code}: {codes:?}");
        }
        assert_eq!(validation.status, "error");
        assert!(!validation.can_publish);
        assert_eq!(validation.next_action, "repair_wiki_toml");
        assert!(validation
            .repair_hints
            .iter()
            .any(|hint| hint.contains("each [[pages]] id is unique")));

        let publish = WikiCore::new(&root).publish_status().unwrap();
        assert_eq!(publish.status, "blocked");
        assert_eq!(publish.next_action, "repair_wiki_toml");
        assert!(publish.config_blocking_count >= 1);
        assert!(publish
            .repair_hints
            .iter()
            .any(|hint| hint.contains("unknown page id")));
    }

    #[test]
    fn validate_blocks_renderer_incompatible_page_frontmatter() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let source = root.join("user-wiki/source/families/reference/topics/source/topics.md");
        let text = fs::read_to_string(&source).unwrap().replace(
            "access: \"private\"",
            "access: \"impossible-render-access\"",
        );
        fs::write(&source, text).unwrap();

        let validation = core.validate().unwrap();
        assert_eq!(validation.status, "error");
        assert!(!validation.can_publish);
        assert_eq!(validation.next_action, "repair_source");
        let issue = validation
            .issues
            .iter()
            .find(|issue| issue.code == "invalid_page_frontmatter")
            .expect("invalid frontmatter issue");
        assert_eq!(issue.severity, "error");
        assert!(issue.message.contains("field \"access\""));
        assert!(validation
            .repair_hints
            .iter()
            .any(|hint| hint.contains("Repair the page source frontmatter")));
    }

    fn attachment_input(path: PathBuf) -> TalkAttachmentInput {
        TalkAttachmentInput {
            path: path.to_string_lossy().to_string(),
            filename: None,
            caption: None,
            alt_text: None,
        }
    }

    const SPAN_BODY: &str = "\n# Topics\n\n## Notes\n\n<!-- operator-touched: 2026-06-01 founder pass -->\nFounder paragraph stays exactly as written.\n<!-- /operator-touched -->\n\n## Log\n\n- first entry\n";

    fn seed_topics_with_operator_span(root: &Path) -> WikiCore {
        let core = WikiCore::new(root);
        core.create_page("topics", None).unwrap();
        let expected = core
            .open_page("topics")
            .unwrap()
            .hashes
            .source_sha256
            .unwrap();
        core.write_page_body("topics", SPAN_BODY, Some(&expected), &Actor::operator())
            .unwrap();
        core
    }

    fn topics_source_path(root: &Path) -> PathBuf {
        root.join("user-wiki/source/families/reference/topics/source/topics.md")
    }

    #[test]
    fn page_open_surfaces_operator_touched_spans() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = seed_topics_with_operator_span(&root);

        let opened = core.open_page("topics").unwrap();
        assert_eq!(opened.operator_touched.len(), 1);
        let span = &opened.operator_touched[0];
        assert_eq!(span.date.as_deref(), Some("2026-06-01"));
        assert_eq!(span.note.as_deref(), Some("founder pass"));
        assert_eq!(span.section_slug.as_deref(), Some("notes"));
        assert!(span.closed);
        let source_text = fs::read_to_string(topics_source_path(&root)).unwrap();
        let lines = source_text.lines().collect::<Vec<_>>();
        assert_eq!(
            lines[span.line_range[0] - 1].trim(),
            "<!-- operator-touched: 2026-06-01 founder pass -->"
        );
        assert_eq!(
            lines[span.line_range[1] - 1].trim(),
            "<!-- /operator-touched -->"
        );
    }

    #[test]
    fn agent_patch_overlapping_operator_touched_span_is_rejected_and_page_unchanged() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = seed_topics_with_operator_span(&root);
        let before = fs::read_to_string(topics_source_path(&root)).unwrap();

        let error = core
            .patch_page_body(
                "topics",
                "Founder paragraph stays exactly as written.",
                "Agent rewrite of founder content.",
                None,
                &Actor::agent("agent://wiki/curator"),
            )
            .unwrap_err();
        let conflict = error
            .downcast_ref::<OperatorTouchedConflict>()
            .expect("typed operator_touched_conflict");
        assert_eq!(conflict.code, "operator_touched_conflict");
        assert_eq!(conflict.operation, "wiki.page.patch_body");
        assert_eq!(conflict.page, "topics");
        assert_eq!(conflict.actor.kind, "agent");
        assert_eq!(conflict.actor.name.as_deref(), Some("agent://wiki/curator"));
        assert_eq!(conflict.conflicting_spans.len(), 1);
        assert!(conflict.edit_line_range.is_some());
        assert_eq!(
            fs::read_to_string(topics_source_path(&root)).unwrap(),
            before
        );

        let write_error = core
            .write_page_body(
                "topics",
                "\n# Topics\n\nWholesale agent rewrite.\n",
                None,
                &Actor::agent("agent://wiki/librarian"),
            )
            .unwrap_err();
        assert!(write_error
            .downcast_ref::<OperatorTouchedConflict>()
            .is_some());
        assert_eq!(
            fs::read_to_string(topics_source_path(&root)).unwrap(),
            before
        );
    }

    #[test]
    fn agent_patch_outside_operator_touched_span_is_allowed() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = seed_topics_with_operator_span(&root);

        let receipt = core
            .patch_page_body(
                "topics",
                "- first entry",
                "- first entry (clarified)",
                None,
                &Actor::agent("agent://wiki/curator"),
            )
            .unwrap();
        assert_eq!(receipt.status, "ok");
        let text = fs::read_to_string(topics_source_path(&root)).unwrap();
        assert!(text.contains("- first entry (clarified)"));
        assert!(text.contains("Founder paragraph stays exactly as written."));
    }

    #[test]
    fn operator_actor_bypasses_span_enforcement() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = seed_topics_with_operator_span(&root);

        let receipt = core
            .patch_page_body(
                "topics",
                "Founder paragraph stays exactly as written.",
                "Founder paragraph, revised by the founder.",
                None,
                &Actor::operator(),
            )
            .unwrap();
        assert_eq!(receipt.status, "ok");
        assert_eq!(
            receipt.actor.as_ref().map(|actor| actor.kind.as_str()),
            Some("operator")
        );
        assert!(fs::read_to_string(topics_source_path(&root))
            .unwrap()
            .contains("Founder paragraph, revised by the founder."));
    }

    #[test]
    fn agent_append_after_spans_is_allowed_on_page_with_spans() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = seed_topics_with_operator_span(&root);

        let into_spanned_section = core
            .append_to_section(
                "topics",
                "Notes",
                "Addendum appended after the founder span.",
                None,
                &Actor::agent("agent://wiki/curator"),
            )
            .unwrap();
        assert_eq!(into_spanned_section.status, "ok");
        let at_page_end = core
            .append_to_section(
                "topics",
                "Log",
                "- second entry from agent",
                None,
                &Actor::agent("agent://wiki/curator"),
            )
            .unwrap();
        assert_eq!(at_page_end.status, "ok");

        let text = fs::read_to_string(topics_source_path(&root)).unwrap();
        assert!(text.contains("<!-- operator-touched: 2026-06-01 founder pass -->\nFounder paragraph stays exactly as written.\n<!-- /operator-touched -->"));
        let addendum = text
            .find("Addendum appended after the founder span.")
            .unwrap();
        let close_marker = text.find("<!-- /operator-touched -->").unwrap();
        let log_heading = text.find("## Log").unwrap();
        assert!(close_marker < addendum && addendum < log_heading);
        assert!(text.ends_with("- second entry from agent\n"));
    }

    #[test]
    fn unclosed_operator_span_blocks_agent_appends_through_end_of_body() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        core.write_page_body(
            "topics",
            "\n# Topics\n\n## Notes\n\n<!-- operator-touched -->\nFounder draft in progress; no close marker yet.\n",
            None,
            &Actor::operator(),
        )
        .unwrap();

        let opened = core.open_page("topics").unwrap();
        assert_eq!(opened.operator_touched.len(), 1);
        assert!(!opened.operator_touched[0].closed);

        let error = core
            .append_to_section(
                "topics",
                "Notes",
                "Agent addendum.",
                None,
                &Actor::agent("agent://wiki/curator"),
            )
            .unwrap_err();
        let conflict = error
            .downcast_ref::<OperatorTouchedConflict>()
            .expect("typed operator_touched_conflict");
        assert_eq!(conflict.operation, "wiki.page.append_section");
        assert!(!conflict.conflicting_spans[0].closed);
    }

    #[test]
    fn append_to_section_round_trip_includes_hash_receipts() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = seed_topics_with_operator_span(&root);
        let source = topics_source_path(&root);
        let before_text = fs::read_to_string(&source).unwrap();
        let before_hash = sha256_file(&source).unwrap();

        let receipt = core
            .append_to_section(
                "topics",
                "Log",
                "- appended entry",
                Some(&before_hash),
                &Actor::agent("agent://wiki/scribe"),
            )
            .unwrap();
        assert_eq!(receipt.operation, "wiki.page.append_section");
        assert_eq!(
            receipt.before_source_sha256.as_deref(),
            Some(before_hash.as_str())
        );
        let after_hash = sha256_file(&source).unwrap();
        assert_ne!(after_hash, before_hash);
        assert_eq!(
            receipt.after_source_sha256.as_deref(),
            Some(after_hash.as_str())
        );
        assert_eq!(
            receipt
                .actor
                .as_ref()
                .and_then(|actor| actor.name.as_deref()),
            Some("agent://wiki/scribe")
        );
        assert_eq!(receipt.next_action, "publish");

        let after_text = fs::read_to_string(&source).unwrap();
        assert!(after_text.starts_with(before_text.trim_end_matches('\n')));
        assert!(after_text.ends_with("- appended entry\n"));

        let events =
            read_jsonl::<serde_json::Value>(&root.join("user-wiki/.1context/page-ledger.jsonl"))
                .unwrap();
        let appended = events
            .iter()
            .find(|event| {
                event.get("event").and_then(serde_json::Value::as_str)
                    == Some("page.section_appended")
            })
            .expect("page.section_appended ledger event");
        assert_eq!(
            appended
                .pointer("/actor/name")
                .and_then(serde_json::Value::as_str),
            Some("agent://wiki/scribe")
        );
        assert_eq!(
            appended
                .get("source_sha256")
                .and_then(serde_json::Value::as_str),
            Some(after_hash.as_str())
        );

        let missing = core
            .append_to_section(
                "topics",
                "No Such Section",
                "- entry",
                None,
                &Actor::operator(),
            )
            .unwrap_err();
        assert!(missing
            .to_string()
            .contains("section heading not found in body"));
    }

    #[test]
    fn talk_only_change_marks_publish_required_without_force() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        fs::create_dir_all(root.join("user-wiki/site/.1context")).unwrap();
        fs::write(
            root.join("user-wiki/site/topics.html"),
            "<main>Topics</main>",
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/site/.1context/source-fingerprint.txt"),
            format!("{}\n", core.publish_fingerprint().unwrap()),
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/site/.1context/page-fingerprints.json"),
            serde_json::to_vec_pretty(&core.page_publish_fingerprints().unwrap()).unwrap(),
        )
        .unwrap();

        let published = core.publish_status().unwrap();
        assert!(!published.render_required);
        assert!(!published.site_needs_publish);
        assert_eq!(published.next_action, "none");

        fs::write(
            root.join(
                "user-wiki/source/families/reference/topics/talk/topics.talk/2026-06-11T00-00-00-000000000Z.conversation.talk-only.md",
            ),
            "---\nsubject: \"Talk only\"\n---\n\nTalk-only change must publish without --force.\n",
        )
        .unwrap();

        let stale = core.publish_status().unwrap();
        assert!(stale.site_needs_publish);
        assert!(stale.render_required);
        assert_eq!(stale.next_action, "publish");
        assert_eq!(stale.pages_needing_publish, vec!["topics".to_string()]);
    }

    #[test]
    fn actor_parse_accepts_operator_and_agent_addresses_only() {
        assert!(Actor::parse("operator").unwrap().is_operator());
        let agent = Actor::parse("agent://wiki/curator").unwrap();
        assert!(!agent.is_operator());
        assert_eq!(agent.kind, "agent");
        assert_eq!(agent.name.as_deref(), Some("agent://wiki/curator"));
        assert!(Actor::parse("agent://").is_err());
        assert!(Actor::parse("founder").is_err());
    }

    fn test_page_options() -> PageCreateOptions {
        PageCreateOptions {
            template: Some("pages/e08/topics.md".to_string()),
            talk_conventions_template: Some("talk/conventions/topics.md".to_string()),
            talk_curator_template: Some("talk/curators/topics.md".to_string()),
            ..PageCreateOptions::default()
        }
    }

    fn seed_fixture(root: &Path) {
        fs::create_dir_all(root.join("user-wiki/templates/pages/e08")).unwrap();
        fs::create_dir_all(root.join("user-wiki/templates/talk/conventions")).unwrap();
        fs::create_dir_all(root.join("user-wiki/templates/talk/curators")).unwrap();
        fs::write(
            root.join("user-wiki/wiki.toml"),
            r#"
schema_version = 1

[defaults]
template_pack = "e08"

[[pages]]
id = "topics"
enabled = true
title = "Topics"
slug = "topics"
route = "/topics"
family_group = "reference"
family_group_title = "Reference"
family_id = "topics"
family_title = "Topics"
type = "topic-index"
template = "pages/e08/topics.md"
talk_conventions_template = "talk/conventions/topics.md"
talk_curator_template = "talk/curators/topics.md"
"#,
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/templates/pages/e08/topics.md"),
            "---\npage_id: \"{{ page_id }}\"\ntitle: \"{{ title }}\"\nslug: \"{{ slug }}\"\nroute: \"{{ route }}\"\nsection: \"{{ section }}\"\naccess: \"{{ access_tier }}\"\n---\n\n# {{ title }}\n",
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/templates/talk/conventions/topics.md"),
            "# {{ title }} Talk Conventions\n",
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/templates/talk/curators/topics.md"),
            "# {{ title }} Curator\n",
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/templates/talk/curators/your-context.md"),
            "# {{ title }} Curator\n",
        )
        .unwrap();
        fs::write(
            root.join("user-wiki/templates/talk/entry.md"),
            "---\nid: \"{{ entry_id }}\"\nkind: \"{{ kind }}\"\nauthor: \"{{ author_id }}\"\ncreated: \"{{ created_at }}\"\ntalk_for: \"{{ talk_for_uri }}\"\nstate: open\n---\n\n## {{ title }}\n\n{{ body }}\n",
        )
        .unwrap();
    }
}
