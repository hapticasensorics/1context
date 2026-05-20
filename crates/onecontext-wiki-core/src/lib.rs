use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, SecondsFormat, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, ErrorKind, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
use toml::Value;

#[derive(Clone, Debug)]
pub struct RuntimePaths {
    pub root: PathBuf,
    pub user_wiki: PathBuf,
    pub context_engine: PathBuf,
    pub source: PathBuf,
    pub templates: PathBuf,
    pub site: PathBuf,
}

impl RuntimePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let user_wiki = root.join("user-wiki");
        let context_engine = root.join("context-engine");
        Self {
            source: user_wiki.join("source"),
            templates: user_wiki.join("templates"),
            site: user_wiki.join("site"),
            user_wiki,
            context_engine,
            root,
        }
    }

    pub fn ensure_v0_dirs(&self) -> Result<()> {
        for path in [
            self.user_wiki.join(".1context"),
            self.source.join("families"),
            self.context_engine.join("agents/directory"),
            self.context_engine.join("agents/subscriptions"),
            self.context_engine.join("mail/mailboxes"),
            self.context_engine.join("notifications/cursors"),
            self.context_engine.join("ledgers"),
            self.context_engine.join("runs"),
            self.context_engine.join("artifacts"),
            self.context_engine.join("proposals"),
            self.context_engine.join("decisions"),
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
    pub agents_summary: AgentsSummary,
    pub mail_summary: MailSummary,
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
    pub mail: PageMailSummary,
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
    pub mailbox: String,
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
    pub mail: PageMailSummary,
    pub files: PageOpenFiles,
    pub hashes: PageOpenHashes,
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
pub struct PageAssetPage {
    pub id: String,
    pub route: String,
    pub title: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageAssetRecord {
    pub id: String,
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
    pub curator_address: String,
    pub page_mailbox: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageMailSummary {
    pub page_mailbox: String,
    pub curator_address: String,
    pub default_watchers_list: String,
    pub associated_lists: Vec<MailListSummary>,
    pub message_count: usize,
    pub actionable_count: usize,
    pub open_delivery_count: usize,
    pub open_thread_count: usize,
    pub unread_count: usize,
    pub watcher_count: usize,
    pub watcher_subscription_count: usize,
    pub active_watcher_count: usize,
    pub inactive_watcher_count: usize,
    pub subscription_liveness_counts: SubscriptionLivenessCounts,
    pub subscriptions: Vec<PageSubscriptionSummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageSubscriptionSummary {
    pub subscription_id: String,
    pub agent_id: String,
    pub subscriber: String,
    pub address: String,
    pub relation: String,
    pub kinds: Vec<String>,
    pub lease_expires_at: String,
    pub agent_liveness: String,
    pub agent_lease_expires_at: Option<String>,
    pub agent_retired_at: Option<String>,
    pub agent_retire_reason: Option<String>,
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

#[derive(Clone, Debug, Default, Serialize)]
pub struct AgentsSummary {
    pub active_count: usize,
    pub stale_count: usize,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct MailSummary {
    pub delivery_count: usize,
    pub unread_count: usize,
    pub notification_count: usize,
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
    pub evidence: Vec<Evidence>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_status: Option<WikiPageStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edit: Option<EditPreconditions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hashes: Option<PageOpenHashes>,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AgentRecord {
    pub schema_version: u32,
    pub event: String,
    pub agent_id: String,
    pub at: String,
    pub addresses: Vec<String>,
    pub roles: Vec<String>,
    pub capabilities: Vec<String>,
    pub lease_expires_at: String,
    pub transport: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentDirectorySummary {
    pub agent_id: String,
    pub liveness: String,
    pub event: String,
    pub registered_at: Option<String>,
    pub last_seen_at: String,
    pub lease_expires_at: String,
    pub retired_at: Option<String>,
    pub retire_reason: Option<String>,
    pub primary_address: Option<String>,
    pub addresses: Vec<String>,
    pub roles: Vec<String>,
    pub capabilities: Vec<String>,
    pub transport: BTreeMap<String, String>,
    pub owned_addresses: Vec<String>,
    pub subscribed_addresses: Vec<String>,
    pub active_subscription_count: usize,
    pub mailbox_count: usize,
    pub unread_count: usize,
    pub actionable_count: usize,
    pub next_action: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct AgentDirectoryCounts {
    pub active_count: usize,
    pub stale_count: usize,
    pub retired_count: usize,
    pub total_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentListResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub counts: AgentDirectoryCounts,
    pub agents: Vec<AgentDirectorySummary>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentStatusResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub exists: bool,
    pub agent: Option<AgentDirectorySummary>,
    pub mailboxes: Vec<MailboxCount>,
    pub subscriptions: Vec<MailSubscriptionSummary>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentIdentifyResult {
    pub schema_version: u32,
    pub status: String,
    pub action: String,
    pub operation: String,
    pub agent_id: String,
    pub thread_id: String,
    pub primary_address: Option<String>,
    pub addresses: Vec<String>,
    pub liveness_before: Option<String>,
    pub liveness_after: String,
    pub agent: Option<AgentDirectorySummary>,
    pub mailboxes: Vec<MailboxCount>,
    pub subscriptions: Vec<MailSubscriptionSummary>,
    pub evidence: Vec<Evidence>,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentWhoamiResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub resolved_by: String,
    pub thread_id: Option<String>,
    pub agent_id: Option<String>,
    pub matches: Vec<AgentDirectorySummary>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentRegisterResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub agent_id: String,
    pub primary_address: String,
    pub addresses: Vec<String>,
    pub mailboxes: Vec<MailboxCount>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MailSubscriptionRecord {
    pub schema_version: u32,
    pub event: String,
    pub subscription_id: String,
    pub agent_id: String,
    pub subscriber: String,
    pub address: String,
    pub relation: String,
    pub kinds: Vec<String>,
    pub state: String,
    pub created_at: String,
    pub lease_expires_at: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct SubscriptionLivenessCounts {
    pub active_agent_count: usize,
    pub stale_agent_count: usize,
    pub retired_agent_count: usize,
    pub unknown_agent_count: usize,
    pub inactive_agent_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailSubscriptionSummary {
    pub schema_version: u32,
    pub event: String,
    pub subscription_id: String,
    pub agent_id: String,
    pub subscriber: String,
    pub address: String,
    pub relation: String,
    pub kinds: Vec<String>,
    pub state: String,
    pub created_at: String,
    pub lease_expires_at: String,
    pub agent_liveness: String,
    pub agent_lease_expires_at: Option<String>,
    pub agent_retired_at: Option<String>,
    pub agent_retire_reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailSubscribeResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub subscription_id: String,
    pub agent_id: String,
    pub subscriber: String,
    pub address: String,
    pub relation: String,
    pub kinds: Vec<String>,
    pub lease_expires_at: String,
    pub backfill: MailSubscribeBackfill,
    pub deduplicated_count: usize,
    pub evidence: Vec<Evidence>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailUnsubscribeResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub agent_id: String,
    pub address: String,
    pub relation: Option<String>,
    pub kinds: Vec<String>,
    pub cancelled_count: usize,
    pub remaining_count: usize,
    pub cancelled: Vec<MailSubscriptionSummary>,
    pub evidence: Vec<Evidence>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailSubscribeBackfill {
    pub surfaced_message_count: usize,
    pub surfaced_unread_count: usize,
    pub notification_count: usize,
    pub notification_policy: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailSubscriptionsResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub subscription_count: usize,
    pub liveness_counts: SubscriptionLivenessCounts,
    pub subscriptions: Vec<MailSubscriptionSummary>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MailListRecord {
    pub schema_version: u32,
    pub event: String,
    pub address: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub page_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    pub state: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailListSummary {
    pub address: String,
    pub title: String,
    pub description: Option<String>,
    pub page_id: Option<String>,
    pub owner: Option<String>,
    pub state: String,
    pub created_at: String,
    pub member_count: usize,
    pub active_member_count: usize,
    pub inactive_member_count: usize,
    pub subscription_liveness_counts: SubscriptionLivenessCounts,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailListCreateResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub list: MailListSummary,
    pub evidence: Vec<Evidence>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailListsResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub list_count: usize,
    pub lists: Vec<MailListSummary>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailListMembersResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub exists: bool,
    pub list: Option<MailListSummary>,
    pub member_count: usize,
    pub active_member_count: usize,
    pub inactive_member_count: usize,
    pub subscriptions: Vec<MailSubscriptionSummary>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailListStatusResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub exists: bool,
    pub include_archived: bool,
    pub include_snoozed: bool,
    pub list: Option<MailListSummary>,
    pub member_count: usize,
    pub active_member_count: usize,
    pub inactive_member_count: usize,
    pub mailbox: MailboxCount,
    pub archived_count: usize,
    pub snoozed_count: usize,
    pub hidden_archived_count: usize,
    pub hidden_snoozed_count: usize,
    pub has_archived: bool,
    pub has_snoozed: bool,
    pub audit_flags: Vec<String>,
    pub subscriptions: Vec<MailSubscriptionSummary>,
    pub messages: Vec<DeliveryRecord>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageParticipantReference {
    pub id: String,
    pub title: String,
    pub route: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageWatchResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub page: PageParticipantReference,
    pub list_create_status: String,
    pub list: MailListSummary,
    pub subscription: MailSubscribeResult,
    pub page_mailbox_subscription: MailSubscribeResult,
    pub unsubscribe_plan: PageWatchUnsubscribePlan,
    pub page_status: WikiPageStatus,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageWatchUnsubscribePlan {
    pub operation: String,
    pub agent_id: String,
    pub page: String,
    pub list_address: String,
    pub page_mailbox_address: String,
    pub relation: String,
    pub kinds: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageUnwatchResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub page: PageParticipantReference,
    pub agent_id: String,
    pub list_address: String,
    pub page_mailbox_address: String,
    pub list_unsubscribe: MailUnsubscribeResult,
    pub page_mailbox_unsubscribe: MailUnsubscribeResult,
    pub cancelled_count: usize,
    pub remaining_count: usize,
    pub page_status: WikiPageStatus,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct PageRoleAssignResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub page: PageParticipantReference,
    pub role_address: String,
    pub subscription: MailSubscribeResult,
    pub page_status: WikiPageStatus,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailboxCount {
    pub address: String,
    pub surface: String,
    pub total_count: usize,
    pub actionable_count: usize,
    pub unread_count: usize,
    pub archived_count: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TalkAppendRequest {
    pub page: String,
    pub kind: String,
    pub subject: String,
    #[serde(default)]
    pub thread_id: Option<String>,
    #[serde(default)]
    pub reply_to: Option<String>,
    pub from: String,
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
    pub body_markdown: String,
    #[serde(default)]
    pub attachments: Vec<TalkAttachmentInput>,
    #[serde(default)]
    pub allow_tombstoned: bool,
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

#[derive(Clone, Debug)]
struct TalkFileMessage {
    message_id: String,
    thread_id: String,
    reply_to: Option<String>,
    page_id: String,
    route: String,
    kind: String,
    subject: String,
    excerpt: String,
    attachments: Vec<TalkAttachmentRecord>,
    created_at: String,
    source: String,
    source_path: String,
    body_markdown: String,
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
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<TalkAttachmentRecord>,
    pub attachment_count: usize,
    pub deliveries: Vec<DeliveryState>,
    pub notifications: Vec<NotificationRecord>,
    pub render_required: bool,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeliveryRecord {
    pub schema_version: u32,
    pub message_id: String,
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub page_id: String,
    pub route: String,
    pub kind: String,
    pub subject: String,
    pub excerpt: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<TalkAttachmentRecord>,
    #[serde(default)]
    pub attachment_count: usize,
    pub recipient: String,
    pub state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snoozed_until: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub claimed_at: Option<String>,
    pub created_at: String,
    pub source: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailClaimResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub message_id: String,
    pub recipient: String,
    pub claimed_by: String,
    pub state: String,
    pub evidence: Vec<Evidence>,
    pub render_required: bool,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailMarkResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub message_id: String,
    pub recipient: String,
    pub state: String,
    pub snoozed_until: Option<String>,
    pub evidence: Vec<Evidence>,
    pub render_required: bool,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailMarkAllResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub message_id: String,
    pub state: String,
    pub recipients: Vec<String>,
    pub changed_delivery_count: usize,
    pub before: MailMarkAllCounts,
    pub after: MailMarkAllCounts,
    pub evidence: Vec<Evidence>,
    pub render_required: bool,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Default)]
pub struct MailMarkAllCounts {
    pub delivery_count: usize,
    pub unread_count: usize,
    pub open_delivery_count: usize,
    pub terminal_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeliveryState {
    pub recipient: String,
    pub state: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct InboxResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub message_count: usize,
    pub actionable_count: usize,
    pub unread_count: usize,
    pub archived_count: usize,
    pub mailbox: MailboxCount,
    pub messages: Vec<DeliveryRecord>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailReadResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub resolved_by: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    pub message_count: usize,
    pub delivery_count: usize,
    pub messages: Vec<MailReadMessage>,
    pub next_action: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct MailReadMessage {
    pub message_id: String,
    pub thread_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub page_id: String,
    pub route: String,
    pub kind: String,
    pub subject: String,
    pub excerpt: String,
    pub attachments: Vec<TalkAttachmentRecord>,
    pub attachment_count: usize,
    pub created_at: String,
    pub source: String,
    pub source_path: String,
    pub body_markdown: String,
    pub deliveries: Vec<DeliveryState>,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentInboxResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub agent_id: String,
    pub message_count: usize,
    pub actionable_count: usize,
    pub claimable_count: usize,
    pub unread_count: usize,
    pub thread_count: usize,
    pub notification_count: usize,
    pub pages_requiring_action: usize,
    pub summary: AgentInboxSummary,
    pub owned_addresses: Vec<String>,
    pub subscribed_addresses: Vec<String>,
    pub effective_mailboxes: Vec<String>,
    pub addresses: Vec<String>,
    pub subscriptions: Vec<MailSubscriptionSummary>,
    pub mailboxes: Vec<MailboxCount>,
    pub pages: Vec<AgentInboxPageSummary>,
    pub threads: Vec<AgentInboxThreadSummary>,
    pub messages: Vec<DeliveryRecord>,
    pub notifications: Vec<NotificationRecord>,
    pub next_action: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentInboxSummary {
    pub actionable_count: usize,
    pub claimable_count: usize,
    pub unread_count: usize,
    pub message_count: usize,
    pub thread_count: usize,
    pub actionable_thread_count: usize,
    pub claimable_thread_count: usize,
    pub notification_count: usize,
    pub notification_thread_count: usize,
    pub pages_with_open_mail_count: usize,
    pub pages_requiring_action: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentInboxPageSummary {
    pub id: String,
    pub title: String,
    pub route: String,
    pub state: String,
    pub next_action: String,
    pub actionable_count: usize,
    pub claimable_count: usize,
    pub unread_count: usize,
    pub message_count: usize,
}

#[derive(Clone, Debug, Serialize)]
pub struct AgentInboxThreadSummary {
    pub thread_id: String,
    pub message_id: String,
    pub page_id: String,
    pub route: String,
    pub kind: String,
    pub subject: String,
    pub excerpt: String,
    pub created_at: String,
    pub delivery_count: usize,
    pub actionable_delivery_count: usize,
    pub claimable_delivery_count: usize,
    pub unread_delivery_count: usize,
    pub notification_count: usize,
    pub attachment_count: usize,
    pub recipients: Vec<DeliveryState>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub schema_version: u32,
    pub notification_id: String,
    pub agent_id: String,
    #[serde(default)]
    pub recipient: String,
    #[serde(default)]
    pub agent_address: String,
    #[serde(default)]
    pub delivery_recipient: String,
    pub mailbox: String,
    pub message_id: String,
    pub thread_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    pub page_id: String,
    #[serde(default)]
    pub route: String,
    pub kind: String,
    #[serde(default)]
    pub subject: String,
    #[serde(default)]
    pub excerpt: String,
    #[serde(default)]
    pub attachment_count: usize,
    pub urgency: String,
    pub cursor: String,
    pub created_at: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NotificationAttempt {
    pub schema_version: u32,
    pub agent_id: String,
    pub notification_id: String,
    pub state: String,
    pub at: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct NotificationPollResult {
    pub schema_version: u32,
    pub status: String,
    pub operation: String,
    pub surface: String,
    pub notification_count: usize,
    pub notifications: Vec<NotificationRecord>,
    pub next_action: String,
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
    pub agents_summary: AgentsSummary,
    pub mail_summary: MailSummary,
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

    fn acquire_mail_mutation_lock(&self) -> Result<FileMutationLock> {
        acquire_file_mutation_lock(
            &self.paths.context_engine.join("mail/.mutation.lock"),
            "mail mutation",
        )
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
            agents_summary: self.agents_summary()?,
            mail_summary: self.mail_summary()?,
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
            mail: status.mail.clone(),
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
                curator_address: format!("role://{}.curator", record.id),
                page_mailbox: format!("mailbox://page/{}", record.id),
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
            agents_summary: inventory.agents_summary,
            mail_summary: inventory.mail_summary,
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
            "title: \"Talk - {}\"\ntalk_for: \"page://{}\"\npage: \"user-wiki://source/families/{}/{}/source/{}.md\"\nroute: \"{}\"\ntalk_route: \"{}\"\nstatus: open\nschema_version: 1\n",
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
        let curator_content = if let Some(template) = &record.talk_curator_template {
            render_template(&self.read_template(template)?, &vars)
        } else {
            default_curator_template(&record)
        };
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
        let entry_template = render_template(
            &self
                .read_template("talk/entry.md")
                .unwrap_or_else(|_| default_entry_template()),
            &vars,
        );
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
                actor,
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
            evidence,
            page_status,
            edit,
            hashes,
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
        let (frontmatter, _body) = split_markdown_frontmatter(&original)?;
        let next = format!("{frontmatter}{body_markdown}");
        self.write_page_source_update(
            record,
            source,
            before_hash,
            next,
            "wiki.page.write_body",
            "page.body_written",
        )
    }

    pub fn patch_page_body(
        &self,
        reference: &str,
        find: &str,
        replace: &str,
        expected_source_sha256: Option<&str>,
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
        let patched_body = body.replacen(find, replace, 1);
        let next = format!("{frontmatter}{patched_body}");
        self.write_page_source_update(
            record,
            source,
            before_hash,
            next,
            "wiki.page.patch_body",
            "page.body_patched",
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
            evidence,
            page_status,
            edit,
            hashes,
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
            evidence,
            page_status,
            edit,
            hashes,
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

    pub fn register_agent(
        &self,
        thread_id: &str,
        roles: Vec<String>,
        capabilities: Vec<String>,
        ttl_seconds: i64,
    ) -> Result<AgentRegisterResult> {
        self.ensure_runtime_dirs()?;
        validate_positive_ttl_seconds("agent", ttl_seconds)?;
        let roles = roles
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let capabilities = capabilities
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        for role in &roles {
            validate_address(role)?;
        }
        let agent_id = agent_id_for_thread(thread_id);
        if let Some(entry) = self.agent_directory_entries()?.get(&agent_id) {
            match entry.liveness.state.as_str() {
                "retired" => {
                    return Err(anyhow!(
                        "agent explicitly retired: {agent_id}; register a new thread/session instead of reviving a retired identity"
                    ));
                }
                _ => {
                    return Err(anyhow!(
                        "agent already registered: {agent_id}; use agent-identify to refresh or merge roles and capabilities"
                    ));
                }
            }
        }
        let primary = agent_primary_address(thread_id);
        let mut addresses = vec![primary];
        addresses.extend(roles.iter().cloned());
        let record = AgentRecord {
            schema_version: 1,
            event: "agent.registered".to_string(),
            agent_id: agent_id.clone(),
            at: now_rfc3339(),
            addresses: addresses.clone(),
            roles,
            capabilities,
            lease_expires_at: (Utc::now() + chrono::Duration::seconds(ttl_seconds))
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            transport: BTreeMap::from([
                ("kind".to_string(), "codex-thread".to_string()),
                ("thread_id".to_string(), thread_id.to_string()),
            ]),
            reason: None,
        };
        append_jsonl(&self.agent_events_path(), &record)?;
        let current = self.current_agents()?;
        write_json_pretty(&self.current_agents_path(), &current)?;
        let mailboxes = addresses
            .iter()
            .map(|address| {
                let messages = self.mailbox(address)?;
                Ok(MailboxCount {
                    address: address.clone(),
                    surface: mailbox_surface(address),
                    total_count: messages.len(),
                    actionable_count: messages
                        .iter()
                        .filter(|message| mail_delivery_is_actionable(message))
                        .count(),
                    unread_count: messages
                        .iter()
                        .filter(|message| message.state == "unread")
                        .count(),
                    archived_count: messages
                        .iter()
                        .filter(|message| message.state == "archived")
                        .count(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(AgentRegisterResult {
            schema_version: 1,
            status: "registered".to_string(),
            operation: "wiki.agent.register".to_string(),
            agent_id,
            primary_address: addresses.first().cloned().unwrap_or_default(),
            addresses,
            mailboxes,
        })
    }

    pub fn identify_agent(
        &self,
        thread_id: &str,
        roles: Vec<String>,
        capabilities: Vec<String>,
        ttl_seconds: i64,
    ) -> Result<AgentIdentifyResult> {
        self.ensure_runtime_dirs()?;
        validate_positive_ttl_seconds("agent", ttl_seconds)?;
        for role in &roles {
            validate_address(role)?;
        }
        let agent_id = agent_id_for_thread(thread_id);
        let entries = self.agent_directory_entries()?;
        let existing = entries.get(&agent_id).cloned();
        let liveness_before = existing.as_ref().map(|entry| entry.liveness.state.clone());
        if existing
            .as_ref()
            .is_some_and(|entry| entry.liveness.state == "retired")
        {
            let status = self.agent_status(&agent_id)?;
            let addresses = status
                .agent
                .as_ref()
                .map(|agent| agent.addresses.clone())
                .unwrap_or_default();
            let primary_address = status
                .agent
                .as_ref()
                .and_then(|agent| agent.primary_address.clone());
            return Ok(AgentIdentifyResult {
                schema_version: 1,
                status: "retired".to_string(),
                action: "retired".to_string(),
                operation: "wiki.agent.identify".to_string(),
                agent_id,
                thread_id: thread_id.to_string(),
                primary_address,
                addresses,
                liveness_before,
                liveness_after: "retired".to_string(),
                agent: status.agent,
                mailboxes: status.mailboxes,
                subscriptions: status.subscriptions,
                evidence: Vec::new(),
                next_action: "agent_register_new_thread".to_string(),
                repair_hints: vec![
                    "This agent was explicitly retired; register a new thread/session instead of silently reviving it.".to_string(),
                ],
            });
        }

        let mut role_set = existing
            .as_ref()
            .map(|entry| entry.record.roles.iter().cloned().collect::<BTreeSet<_>>())
            .unwrap_or_default();
        role_set.extend(roles);
        let roles = role_set.into_iter().collect::<Vec<_>>();
        let mut capability_set = existing
            .as_ref()
            .map(|entry| {
                entry
                    .record
                    .capabilities
                    .iter()
                    .cloned()
                    .collect::<BTreeSet<_>>()
            })
            .unwrap_or_default();
        capability_set.extend(capabilities);
        let capabilities = capability_set.into_iter().collect::<Vec<_>>();
        let mut addresses = vec![agent_primary_address(thread_id)];
        addresses.extend(roles.iter().cloned());
        let record = AgentRecord {
            schema_version: 1,
            event: if existing.is_some() {
                "agent.identified".to_string()
            } else {
                "agent.registered".to_string()
            },
            agent_id: agent_id.clone(),
            at: now_rfc3339(),
            addresses,
            roles,
            capabilities,
            lease_expires_at: (Utc::now() + chrono::Duration::seconds(ttl_seconds))
                .to_rfc3339_opts(SecondsFormat::Secs, true),
            transport: BTreeMap::from([
                ("kind".to_string(), "codex-thread".to_string()),
                ("thread_id".to_string(), thread_id.to_string()),
            ]),
            reason: None,
        };
        append_jsonl(&self.agent_events_path(), &record)?;
        write_json_pretty(&self.current_agents_path(), &self.current_agents()?)?;
        let status = self.agent_status(&agent_id)?;
        let liveness_after = status
            .agent
            .as_ref()
            .map(|agent| agent.liveness.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let next_action = status.next_action.clone();
        let action = match liveness_before.as_deref() {
            None => "registered",
            Some("stale") => "refreshed",
            Some("active") => "identified",
            _ => "updated",
        }
        .to_string();
        let addresses = status
            .agent
            .as_ref()
            .map(|agent| agent.addresses.clone())
            .unwrap_or_default();
        let primary_address = status
            .agent
            .as_ref()
            .and_then(|agent| agent.primary_address.clone());
        Ok(AgentIdentifyResult {
            schema_version: 1,
            status: action.clone(),
            action,
            operation: "wiki.agent.identify".to_string(),
            agent_id,
            thread_id: thread_id.to_string(),
            primary_address,
            addresses,
            liveness_before,
            liveness_after,
            agent: status.agent,
            mailboxes: status.mailboxes,
            subscriptions: status.subscriptions,
            evidence: vec![Evidence {
                path: display_under_root(&self.agent_events_path(), &self.paths.root),
                status: "appended".to_string(),
            }],
            next_action,
            repair_hints: Vec::new(),
        })
    }

    pub fn heartbeat_agent(
        &self,
        agent_id: &str,
        lease_extend_seconds: i64,
    ) -> Result<AgentRegisterResult> {
        validate_positive_ttl_seconds("agent heartbeat", lease_extend_seconds)?;
        let mut record = self.require_active_agent(agent_id)?;
        record.event = "agent.heartbeat".to_string();
        record.at = now_rfc3339();
        record.lease_expires_at = (Utc::now() + chrono::Duration::seconds(lease_extend_seconds))
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        append_jsonl(&self.agent_events_path(), &record)?;
        write_json_pretty(&self.current_agents_path(), &self.current_agents()?)?;
        Ok(AgentRegisterResult {
            schema_version: 1,
            status: "heartbeat".to_string(),
            operation: "wiki.agent.heartbeat".to_string(),
            agent_id: agent_id.to_string(),
            primary_address: record.addresses.first().cloned().unwrap_or_default(),
            addresses: record.addresses,
            mailboxes: Vec::new(),
        })
    }

    pub fn retire_agent(&self, agent_id: &str, reason: &str) -> Result<OperationReceipt> {
        let mut current = self.current_agents_with_stale(true)?;
        let mut record = current
            .remove(agent_id)
            .ok_or_else(|| anyhow!("unknown agent: {agent_id}"))?;
        record.event = "agent.retired".to_string();
        record.at = now_rfc3339();
        record.reason = Some(reason.to_string());
        append_jsonl(&self.agent_events_path(), &record)?;
        write_json_pretty(&self.current_agents_path(), &self.current_agents()?)?;
        Ok(OperationReceipt {
            schema_version: 1,
            status: "retired".to_string(),
            operation: "wiki.agent.retire".to_string(),
            id: agent_id.to_string(),
            route: None,
            page_type: None,
            collection: None,
            evidence: vec![Evidence {
                path: display_under_root(&self.agent_events_path(), &self.paths.root),
                status: "appended".to_string(),
            }],
            page_status: None,
            edit: None,
            hashes: None,
            link_impact: None,
            render_required: false,
            next_action: "none".to_string(),
            repair_hints: Vec::new(),
        })
    }

    pub fn agent_list(
        &self,
        include_stale: bool,
        include_retired: bool,
    ) -> Result<AgentListResult> {
        let subscriptions_by_agent = self.active_subscriptions_by_agent()?;
        let mut agents = self
            .agent_directory_entries()?
            .into_values()
            .filter(|entry| {
                entry.liveness.state == "active"
                    || (include_stale && entry.liveness.state == "stale")
                    || (include_retired && entry.liveness.state == "retired")
            })
            .map(|entry| self.agent_directory_summary(entry, &subscriptions_by_agent))
            .collect::<Result<Vec<_>>>()?;
        agents.sort_by(|a, b| {
            agent_liveness_sort_key(&a.liveness)
                .cmp(&agent_liveness_sort_key(&b.liveness))
                .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
                .then_with(|| a.agent_id.cmp(&b.agent_id))
        });
        Ok(AgentListResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.agent.list".to_string(),
            surface: "agent_list".to_string(),
            counts: agent_directory_counts(&agents),
            agents,
        })
    }

    pub fn agent_status(&self, agent_id: &str) -> Result<AgentStatusResult> {
        let subscriptions_by_agent = self.active_subscriptions_by_agent()?;
        let subscriptions = subscriptions_by_agent
            .get(agent_id)
            .cloned()
            .unwrap_or_default();
        let agent_liveness = self.agent_liveness_index()?;
        let enriched = subscriptions
            .iter()
            .cloned()
            .map(|subscription| enrich_mail_subscription(subscription, &agent_liveness))
            .collect::<Vec<_>>();
        let entries = self.agent_directory_entries()?;
        let Some(entry) = entries.get(agent_id).cloned() else {
            return Ok(AgentStatusResult {
                schema_version: 1,
                status: "missing".to_string(),
                operation: "wiki.agent.status".to_string(),
                surface: "agent_status".to_string(),
                exists: false,
                agent: None,
                mailboxes: Vec::new(),
                subscriptions: enriched,
                next_action: "agent_register".to_string(),
            });
        };
        let summary = self.agent_directory_summary(entry, &subscriptions_by_agent)?;
        let mailboxes = self.agent_mailbox_counts(&summary.owned_addresses, &subscriptions)?;
        let next_action = summary.next_action.clone();
        Ok(AgentStatusResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.agent.status".to_string(),
            surface: "agent_status".to_string(),
            exists: true,
            agent: Some(summary),
            mailboxes,
            subscriptions: enriched,
            next_action,
        })
    }

    pub fn agent_whoami(
        &self,
        thread_id: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<AgentWhoamiResult> {
        if thread_id.is_none() && agent_id.is_none() {
            return Err(anyhow!("whoami requires --thread-id or --agent-id"));
        }
        let subscriptions_by_agent = self.active_subscriptions_by_agent()?;
        let mut matches = self
            .agent_directory_entries()?
            .into_values()
            .filter(|entry| {
                agent_id.is_none_or(|agent_id| entry.record.agent_id == agent_id)
                    && thread_id.is_none_or(|thread_id| {
                        entry
                            .record
                            .transport
                            .get("thread_id")
                            .is_some_and(|candidate| candidate == thread_id)
                    })
            })
            .map(|entry| self.agent_directory_summary(entry, &subscriptions_by_agent))
            .collect::<Result<Vec<_>>>()?;
        matches.sort_by(|a, b| {
            agent_liveness_sort_key(&a.liveness)
                .cmp(&agent_liveness_sort_key(&b.liveness))
                .then_with(|| b.last_seen_at.cmp(&a.last_seen_at))
        });
        let next_action = matches
            .first()
            .map(|agent| agent.next_action.clone())
            .unwrap_or_else(|| "agent_register".to_string());
        Ok(AgentWhoamiResult {
            schema_version: 1,
            status: if matches.is_empty() { "missing" } else { "ok" }.to_string(),
            operation: "wiki.agent.whoami".to_string(),
            surface: "agent_whoami".to_string(),
            resolved_by: if thread_id.is_some() {
                "thread_id".to_string()
            } else {
                "agent_id".to_string()
            },
            thread_id: thread_id.map(ToString::to_string),
            agent_id: agent_id.map(ToString::to_string),
            matches,
            next_action,
        })
    }

    pub fn subscribe_mail(
        &self,
        agent_id: &str,
        address: &str,
        relation: &str,
        kinds: Vec<String>,
        ttl_seconds: i64,
    ) -> Result<MailSubscribeResult> {
        self.ensure_runtime_dirs()?;
        let agent = self.require_active_agent(agent_id)?;
        let address = self.normalize_mail_recipient(address)?;
        let _lock = self.acquire_mail_mutation_lock()?;
        if address.starts_with("list://") && self.mail_lists(None, Some(&address))?.lists.is_empty()
        {
            return Err(anyhow!(
                "unknown list {address}; call wiki.list.create first"
            ));
        }
        if !mail_subscription_relation_is_allowed(relation) {
            return Err(anyhow!(
                "invalid mail subscription relation {relation}; expected watcher, member, assignee, or subscriber"
            ));
        }
        if ttl_seconds <= 0 {
            return Err(anyhow!(
                "invalid mail subscription ttl {ttl_seconds}; expected a positive number of seconds"
            ));
        }
        let kinds = normalize_subscription_kinds(kinds)?;
        let backfill_kind_filter = MailKindFilter::from_kinds(&kinds);
        let existing_mail =
            self.inbox_with_kind_filter(&address, false, false, Some(&backfill_kind_filter))?;
        let created_at = now_rfc3339();
        let subscriber = agent
            .addresses
            .first()
            .cloned()
            .unwrap_or_else(|| agent_id.to_string());
        let lease_expires_at = (Utc::now() + chrono::Duration::seconds(ttl_seconds))
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        let next_action = if existing_mail.mailbox.actionable_count > 0 {
            "agent_inbox"
        } else {
            "none"
        }
        .to_string();

        let mut existing_matches = self
            .active_mail_subscriptions()?
            .into_iter()
            .filter(|subscription| {
                subscription.agent_id == agent_id
                    && subscription.address == address
                    && subscription.relation == relation
                    && subscription_kinds_equal(&subscription.kinds, &kinds)
            })
            .collect::<Vec<_>>();
        existing_matches.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.subscription_id.cmp(&b.subscription_id))
        });

        let path = self.mail_subscriptions_path();
        if let Some(mut record) = existing_matches.first().cloned() {
            let deduplicated_count = existing_matches.len().saturating_sub(1);
            for duplicate in existing_matches.iter().skip(1) {
                let mut cancelled = duplicate.clone();
                cancelled.event = "mail.subscription.cancelled".to_string();
                cancelled.state = "cancelled".to_string();
                cancelled.created_at = created_at.clone();
                cancelled.lease_expires_at = created_at.clone();
                append_jsonl(&path, &cancelled)?;
            }
            record.event = "mail.subscription.renewed".to_string();
            record.subscriber = subscriber.clone();
            record.kinds = kinds.clone();
            record.created_at = created_at;
            record.lease_expires_at = lease_expires_at.clone();
            append_jsonl(&path, &record)?;
            return Ok(MailSubscribeResult {
                schema_version: 1,
                status: "renewed".to_string(),
                operation: "wiki.mail.subscribe".to_string(),
                subscription_id: record.subscription_id,
                agent_id: agent_id.to_string(),
                subscriber,
                address,
                relation: relation.to_string(),
                kinds,
                lease_expires_at,
                backfill: MailSubscribeBackfill {
                    surfaced_message_count: existing_mail.mailbox.total_count,
                    surfaced_unread_count: existing_mail.mailbox.unread_count,
                    notification_count: 0,
                    notification_policy: "future_deliveries_only".to_string(),
                },
                deduplicated_count,
                evidence: vec![Evidence {
                    path: display_under_root(&path, &self.paths.root),
                    status: "appended".to_string(),
                }],
                next_action,
            });
        }

        let id_stamp = now_file_stamp();
        let kind_key = if kinds.is_empty() {
            "all".to_string()
        } else {
            kinds.join(",")
        };
        let subscription_id = format!(
            "sub_{}",
            short_hash(&format!(
                "{agent_id}:{subscriber}:{address}:{relation}:{kind_key}:{id_stamp}"
            ))
        );
        let record = MailSubscriptionRecord {
            schema_version: 1,
            event: "mail.subscription.created".to_string(),
            subscription_id: subscription_id.clone(),
            agent_id: agent_id.to_string(),
            subscriber: subscriber.clone(),
            address: address.clone(),
            relation: relation.to_string(),
            kinds: kinds.clone(),
            state: "active".to_string(),
            created_at,
            lease_expires_at: lease_expires_at.clone(),
        };
        append_jsonl(&path, &record)?;
        Ok(MailSubscribeResult {
            schema_version: 1,
            status: "subscribed".to_string(),
            operation: "wiki.mail.subscribe".to_string(),
            subscription_id,
            agent_id: agent_id.to_string(),
            subscriber,
            address,
            relation: relation.to_string(),
            kinds,
            lease_expires_at,
            backfill: MailSubscribeBackfill {
                surfaced_message_count: existing_mail.mailbox.total_count,
                surfaced_unread_count: existing_mail.mailbox.unread_count,
                notification_count: 0,
                notification_policy: "future_deliveries_only".to_string(),
            },
            deduplicated_count: 0,
            evidence: vec![Evidence {
                path: display_under_root(&path, &self.paths.root),
                status: "appended".to_string(),
            }],
            next_action,
        })
    }

    pub fn unsubscribe_mail(
        &self,
        agent_id: &str,
        address: &str,
        relation: Option<&str>,
        kinds: Vec<String>,
    ) -> Result<MailUnsubscribeResult> {
        self.ensure_runtime_dirs()?;
        self.require_active_agent(agent_id)?;
        let address = self.normalize_mail_recipient(address)?;
        let _lock = self.acquire_mail_mutation_lock()?;
        if let Some(relation) = relation {
            if !mail_subscription_relation_is_allowed(relation) {
                return Err(anyhow!(
                    "invalid mail subscription relation {relation}; expected watcher, member, assignee, or subscriber"
                ));
            }
        }
        let kinds = normalize_subscription_kinds(kinds)?;

        let mut matches = self
            .active_mail_subscriptions()?
            .into_iter()
            .filter(|subscription| {
                subscription.agent_id == agent_id
                    && subscription.address == address
                    && relation.is_none_or(|relation| subscription.relation == relation)
                    && (kinds.is_empty() || subscription_kinds_equal(&subscription.kinds, &kinds))
            })
            .collect::<Vec<_>>();
        matches.sort_by(|a, b| a.created_at.cmp(&b.created_at));

        let path = self.mail_subscriptions_path();
        let cancelled_at = now_rfc3339();
        for subscription in &matches {
            let mut record = subscription.clone();
            record.event = "mail.subscription.cancelled".to_string();
            record.state = "cancelled".to_string();
            record.created_at = cancelled_at.clone();
            record.lease_expires_at = cancelled_at.clone();
            append_jsonl(&path, &record)?;
        }

        let remaining_count = self
            .active_mail_subscriptions()?
            .into_iter()
            .filter(|subscription| {
                subscription.agent_id == agent_id && subscription.address == address
            })
            .count();
        let agent_liveness = self.agent_liveness_index()?;
        let cancelled = matches
            .into_iter()
            .map(|subscription| enrich_mail_subscription(subscription, &agent_liveness))
            .collect::<Vec<_>>();
        let cancelled_count = cancelled.len();

        Ok(MailUnsubscribeResult {
            schema_version: 1,
            status: if cancelled_count == 0 {
                "not_found".to_string()
            } else {
                "unsubscribed".to_string()
            },
            operation: "wiki.mail.unsubscribe".to_string(),
            agent_id: agent_id.to_string(),
            address,
            relation: relation.map(ToString::to_string),
            kinds,
            cancelled_count,
            remaining_count,
            cancelled,
            evidence: if cancelled_count == 0 {
                vec![]
            } else {
                vec![Evidence {
                    path: display_under_root(&path, &self.paths.root),
                    status: "appended".to_string(),
                }]
            },
            next_action: if remaining_count > 0 {
                "mail_subscriptions".to_string()
            } else if cancelled_count == 0 {
                "mail_subscribe".to_string()
            } else {
                "none".to_string()
            },
        })
    }

    pub fn create_list(
        &self,
        address: &str,
        title: Option<String>,
        description: Option<String>,
        page_id: Option<String>,
        owner: Option<String>,
    ) -> Result<MailListCreateResult> {
        self.ensure_runtime_dirs()?;
        validate_list_address(address)?;
        let owner = owner
            .as_deref()
            .map(|owner| self.resolve_owner_address(owner))
            .transpose()?;
        let page_id = page_id
            .map(|page_reference| self.find_page(&page_reference).map(|record| record.id))
            .transpose()?;
        let _lock = self.acquire_mail_mutation_lock()?;
        if let Some(existing) = self
            .current_mail_lists()?
            .into_iter()
            .find(|list| list.address == address)
        {
            let member_counts = self.subscription_liveness_counts_by_address()?;
            return Ok(MailListCreateResult {
                schema_version: 1,
                status: "already_exists".to_string(),
                operation: "wiki.list.create".to_string(),
                list: mail_list_summary(existing, &member_counts),
                evidence: vec![Evidence {
                    path: display_under_root(&self.mail_lists_path(), &self.paths.root),
                    status: "skipped_existing".to_string(),
                }],
            });
        }
        let record = MailListRecord {
            schema_version: 1,
            event: "mail.list.created".to_string(),
            address: address.to_string(),
            title: title.unwrap_or_else(|| title_case(&list_name(address))),
            description,
            page_id,
            owner,
            state: "active".to_string(),
            created_at: now_rfc3339(),
        };
        let path = self.mail_lists_path();
        append_jsonl(&path, &record)?;
        let member_counts = self.subscription_liveness_counts_by_address()?;
        Ok(MailListCreateResult {
            schema_version: 1,
            status: "created".to_string(),
            operation: "wiki.list.create".to_string(),
            list: mail_list_summary(record, &member_counts),
            evidence: vec![Evidence {
                path: display_under_root(&path, &self.paths.root),
                status: "appended".to_string(),
            }],
        })
    }

    pub fn mail_lists(
        &self,
        page_id: Option<&str>,
        address: Option<&str>,
    ) -> Result<MailListsResult> {
        let page_id = page_id
            .map(|page_reference| self.find_page(page_reference).map(|record| record.id))
            .transpose()?;
        if let Some(address) = address {
            validate_list_address(address)?;
        }
        let member_counts = self.subscription_liveness_counts_by_address()?;
        let mut lists = self
            .current_mail_lists()?
            .into_iter()
            .filter(|list| {
                page_id
                    .as_deref()
                    .is_none_or(|page_id| list.page_id.as_deref() == Some(page_id))
                    && address.is_none_or(|address| list.address == address)
            })
            .map(|record| mail_list_summary(record, &member_counts))
            .collect::<Vec<_>>();
        lists.sort_by(|a, b| a.address.cmp(&b.address));
        let list_count = lists.len();
        Ok(MailListsResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.lists".to_string(),
            surface: "mail_lists".to_string(),
            list_count,
            lists,
            next_action: if list_count == 0 {
                "list_create".to_string()
            } else {
                "none".to_string()
            },
        })
    }

    pub fn list_members(&self, address: &str) -> Result<MailListMembersResult> {
        validate_list_address(address)?;
        let agent_liveness = self.agent_liveness_index()?;
        let subscriptions = self
            .active_mail_subscriptions()?
            .into_iter()
            .filter(|subscription| subscription.address == address)
            .map(|subscription| enrich_mail_subscription(subscription, &agent_liveness))
            .collect::<Vec<_>>();
        let member_counts = self.subscription_liveness_counts_by_address()?;
        let list = self
            .current_mail_lists()?
            .into_iter()
            .find(|list| list.address == address)
            .map(|record| mail_list_summary(record, &member_counts));
        let exists = list.is_some();
        let member_count = list.as_ref().map_or(0, |list| list.member_count);
        let active_member_count = list.as_ref().map_or(0, |list| list.active_member_count);
        let inactive_member_count = list.as_ref().map_or(0, |list| list.inactive_member_count);
        Ok(MailListMembersResult {
            schema_version: 1,
            status: if exists { "ok" } else { "missing" }.to_string(),
            operation: "wiki.list.members".to_string(),
            surface: "mail_list_members".to_string(),
            exists,
            list,
            member_count,
            active_member_count,
            inactive_member_count,
            subscriptions,
            next_action: if exists { "none" } else { "list_create" }.to_string(),
        })
    }

    pub fn list_status(&self, address: &str) -> Result<MailListStatusResult> {
        self.list_status_with_options(address, false, false)
    }

    pub fn list_status_with_options(
        &self,
        address: &str,
        include_archived: bool,
        include_snoozed: bool,
    ) -> Result<MailListStatusResult> {
        validate_list_address(address)?;
        let recipient = self.normalize_mail_recipient(address)?;
        let raw_messages = self.mailbox(&recipient)?;
        let archived_count = raw_messages
            .iter()
            .filter(|message| message.state == "archived")
            .count();
        let snoozed_count = raw_messages
            .iter()
            .filter(|message| message.state == "snoozed")
            .count();
        let future_snoozed_count = raw_messages
            .iter()
            .filter(|message| mail_delivery_is_snoozed_until_future(message))
            .count();
        let hidden_archived_count = if include_archived { 0 } else { archived_count };
        let hidden_snoozed_count = if include_snoozed {
            0
        } else {
            future_snoozed_count
        };
        let mut audit_flags = Vec::new();
        if hidden_archived_count > 0 {
            audit_flags.push("archived_hidden".to_string());
        }
        if hidden_snoozed_count > 0 {
            audit_flags.push("snoozed_hidden".to_string());
        }
        let inbox = self.inbox_with_options(&recipient, include_archived, include_snoozed)?;
        let members = self.list_members(address)?;
        let next_action = if !members.exists {
            members.next_action.clone()
        } else if inbox.actionable_count > 0 {
            inbox.next_action.clone()
        } else if hidden_archived_count > 0 || hidden_snoozed_count > 0 {
            "include_hidden_mail".to_string()
        } else {
            "none".to_string()
        };
        Ok(MailListStatusResult {
            schema_version: 1,
            status: members.status.clone(),
            operation: "wiki.list.status".to_string(),
            surface: "mail_list_status".to_string(),
            exists: members.exists,
            include_archived,
            include_snoozed,
            list: members.list,
            member_count: members.member_count,
            active_member_count: members.active_member_count,
            inactive_member_count: members.inactive_member_count,
            mailbox: inbox.mailbox,
            archived_count,
            snoozed_count,
            hidden_archived_count,
            hidden_snoozed_count,
            has_archived: archived_count > 0,
            has_snoozed: snoozed_count > 0,
            audit_flags,
            subscriptions: members.subscriptions,
            messages: inbox.messages,
            next_action,
        })
    }

    pub fn watch_page(
        &self,
        page: &str,
        agent_id: &str,
        list_address: Option<&str>,
        kinds: Vec<String>,
        ttl_seconds: i64,
    ) -> Result<PageWatchResult> {
        let page_status = self.page_status(page)?;
        let agent = self.require_active_agent(agent_id)?;
        let address = list_address
            .map(str::to_string)
            .unwrap_or_else(|| format!("list://{}.watchers", page_status.id));
        validate_list_address(&address)?;
        let owner = agent.addresses.first().cloned();
        let list = self.create_list(
            &address,
            Some(format!("{} Watchers", page_status.title)),
            Some(format!("Agents watching {}.", page_status.title)),
            Some(page_status.id.clone()),
            owner,
        )?;
        let subscription =
            self.subscribe_mail(agent_id, &address, "watcher", kinds.clone(), ttl_seconds)?;
        let page_mailbox = format!("mailbox://page/{}", page_status.id);
        let page_mailbox_subscription = self.subscribe_mail(
            agent_id,
            &page_mailbox,
            "watcher",
            kinds.clone(),
            ttl_seconds,
        )?;
        let refreshed = self.page_status(&page_status.id)?;
        let refreshed_list = self
            .mail_lists(None, Some(&address))?
            .lists
            .into_iter()
            .next()
            .unwrap_or(list.list);
        Ok(PageWatchResult {
            schema_version: 1,
            status: "watching".to_string(),
            operation: "wiki.page.watch".to_string(),
            page: PageParticipantReference {
                id: page_status.id.clone(),
                title: page_status.title.clone(),
                route: page_status.route.clone(),
            },
            list_create_status: list.status,
            list: refreshed_list,
            subscription,
            page_mailbox_subscription,
            unsubscribe_plan: PageWatchUnsubscribePlan {
                operation: "wiki.page.unwatch".to_string(),
                agent_id: agent_id.to_string(),
                page: page_status.id,
                list_address: address,
                page_mailbox_address: page_mailbox,
                relation: "watcher".to_string(),
                kinds,
            },
            page_status: refreshed,
        })
    }

    pub fn unwatch_page(
        &self,
        page: &str,
        agent_id: &str,
        list_address: Option<&str>,
        kinds: Vec<String>,
    ) -> Result<PageUnwatchResult> {
        let page_status = self.page_status(page)?;
        self.require_active_agent(agent_id)?;
        let address = list_address
            .map(str::to_string)
            .unwrap_or_else(|| format!("list://{}.watchers", page_status.id));
        validate_list_address(&address)?;
        let page_mailbox = format!("mailbox://page/{}", page_status.id);

        let list_unsubscribe =
            self.unsubscribe_mail(agent_id, &address, Some("watcher"), kinds.clone())?;
        let page_mailbox_unsubscribe =
            self.unsubscribe_mail(agent_id, &page_mailbox, Some("watcher"), kinds)?;
        let cancelled_count =
            list_unsubscribe.cancelled_count + page_mailbox_unsubscribe.cancelled_count;
        let remaining_count =
            list_unsubscribe.remaining_count + page_mailbox_unsubscribe.remaining_count;
        let refreshed = self.page_status(&page_status.id)?;
        let next_action = if remaining_count > 0 {
            "mail_subscriptions"
        } else if cancelled_count == 0 {
            "page_watch"
        } else {
            "none"
        };
        Ok(PageUnwatchResult {
            schema_version: 1,
            status: if cancelled_count == 0 {
                "not_found".to_string()
            } else {
                "unwatched".to_string()
            },
            operation: "wiki.page.unwatch".to_string(),
            page: PageParticipantReference {
                id: page_status.id,
                title: page_status.title,
                route: page_status.route,
            },
            agent_id: agent_id.to_string(),
            list_address: address,
            page_mailbox_address: page_mailbox,
            list_unsubscribe,
            page_mailbox_unsubscribe,
            cancelled_count,
            remaining_count,
            page_status: refreshed,
            next_action: next_action.to_string(),
        })
    }

    pub fn assign_page_role(
        &self,
        page: &str,
        agent_id: &str,
        role: &str,
        kinds: Vec<String>,
        ttl_seconds: i64,
    ) -> Result<PageRoleAssignResult> {
        let page_status = self.page_status(page)?;
        self.require_active_agent(agent_id)?;
        let role_address = page_role_address(&page_status.id, role)?;
        let subscription =
            self.subscribe_mail(agent_id, &role_address, "assignee", kinds, ttl_seconds)?;
        let refreshed = self.page_status(&page_status.id)?;
        Ok(PageRoleAssignResult {
            schema_version: 1,
            status: "assigned".to_string(),
            operation: "wiki.page.assign_role".to_string(),
            page: PageParticipantReference {
                id: page_status.id,
                title: page_status.title,
                route: page_status.route,
            },
            role_address,
            subscription,
            page_status: refreshed,
        })
    }

    pub fn mail_subscriptions(
        &self,
        agent_id: Option<&str>,
        address: Option<&str>,
    ) -> Result<MailSubscriptionsResult> {
        let address = address
            .map(|address| self.normalize_mail_recipient(address))
            .transpose()?;
        let mut subscriptions = self.active_mail_subscriptions()?;
        if let Some(agent_id) = agent_id {
            subscriptions.retain(|subscription| subscription.agent_id == agent_id);
        }
        if let Some(address) = address.as_deref() {
            subscriptions.retain(|subscription| subscription.address == address);
        }
        let agent_liveness = self.agent_liveness_index()?;
        let mut subscriptions = subscriptions
            .into_iter()
            .map(|subscription| enrich_mail_subscription(subscription, &agent_liveness))
            .collect::<Vec<_>>();
        subscriptions.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let subscription_count = subscriptions.len();
        let liveness_counts = mail_subscription_summary_liveness_counts(&subscriptions);
        Ok(MailSubscriptionsResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.mail.subscriptions".to_string(),
            surface: "mail_subscriptions".to_string(),
            subscription_count,
            liveness_counts,
            subscriptions,
            next_action: if subscription_count == 0 {
                "mail_subscribe".to_string()
            } else {
                "none".to_string()
            },
        })
    }

    pub fn append_talk(&self, request: TalkAppendRequest) -> Result<TalkAppendResult> {
        self.ensure_runtime_dirs()?;
        let _mail_lock = self.acquire_mail_mutation_lock()?;
        validate_address(&request.from)?;
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
        let created_at = now_rfc3339();
        let stamp = now_file_stamp();
        let subject_slug = slugify(&request.subject);
        let message_id = format!(
            "talkmsg_{}_{}",
            stamp.replace('-', ""),
            short_hash(&request.subject)
        );
        let default_thread_id = format!("thread_{}_{}", record.id, subject_slug);
        let thread_target =
            self.resolve_talk_thread_target(&record, &request, default_thread_id)?;
        let thread_id = thread_target.thread_id;
        let reply_to = thread_target.reply_to;
        let filename = format!("{stamp}.{}.{}.md", request.kind, subject_slug);
        let source_path = talk_dir.join(filename);
        let mut recipients = request
            .to
            .iter()
            .chain(request.cc.iter())
            .map(|recipient| self.normalize_talk_recipient(recipient))
            .collect::<Result<Vec<_>>>()?;
        recipients.sort();
        recipients.dedup();
        let attachments =
            self.copy_talk_attachments(&talk_dir, &record.id, &message_id, &request.attachments)?;
        let frontmatter_recipients = recipients
            .iter()
            .map(|recipient| format!("  - \"{recipient}\""))
            .collect::<Vec<_>>()
            .join("\n");
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
            "---\nid: \"{}\"\nkind: \"{}\"\nauthor: \"{}\"\ncreated: \"{}\"\ntalk_for: \"page://{}\"\nthread: \"{}\"\n{}subject: \"{}\"\nstate: open\nrecipients:\n{}\n{}\n---\n\n## {}\n\n{}{}\n",
            message_id,
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
        write_new(&source_path, body)?;

        let source_handle = format!(
            "user-wiki://page/{}/talk/messages/{}",
            record.id, message_id
        );
        let excerpt = excerpt(&request.body_markdown);
        let deliveries = recipients
            .iter()
            .map(|recipient| DeliveryRecord {
                schema_version: 1,
                message_id: message_id.clone(),
                thread_id: thread_id.clone(),
                reply_to: reply_to.clone(),
                page_id: record.id.clone(),
                route: record.route.clone(),
                kind: request.kind.clone(),
                subject: request.subject.clone(),
                excerpt: excerpt.clone(),
                attachments: attachments.clone(),
                attachment_count: attachments.len(),
                recipient: recipient.clone(),
                state: "unread".to_string(),
                snoozed_until: None,
                claimed_by: None,
                claimed_at: None,
                created_at: created_at.clone(),
                source: source_handle.clone(),
            })
            .collect::<Vec<_>>();
        for delivery in &deliveries {
            append_jsonl(&self.deliveries_path(), delivery)?;
            append_jsonl(&self.mailbox_path(&delivery.recipient), delivery)?;
        }

        let notifications = self.enqueue_notifications(&deliveries)?;
        Ok(TalkAppendResult {
            schema_version: 1,
            status: "appended".to_string(),
            operation: "wiki.talk.append".to_string(),
            surface: "page_talk_mailbox".to_string(),
            message_id,
            thread_id,
            reply_to,
            page_id: record.id,
            route: record.route,
            kind: request.kind,
            subject: request.subject,
            created_at,
            source: source_handle,
            attachment_count: attachments.len(),
            attachments,
            deliveries: deliveries
                .iter()
                .map(|d| DeliveryState {
                    recipient: d.recipient.clone(),
                    state: d.state.clone(),
                })
                .collect(),
            notifications,
            render_required: false,
            next_action: "none".to_string(),
            repair_hints: Vec::new(),
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
        let parent = if let Some(reply_to) = reply_to {
            let parent = targets
                .iter()
                .find(|target| target.message_id == reply_to)
                .ok_or_else(|| {
                    anyhow!("talk reply target not found in mailbox or talk files: {reply_to}")
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
            reply_to: reply_to.map(ToString::to_string),
        })
    }

    fn all_talk_message_targets(&self) -> Result<Vec<TalkMessageTarget>> {
        let mut targets = BTreeMap::<String, TalkMessageTarget>::new();
        for delivery in self.all_mailbox_messages()? {
            if talk_target_id_is_valid(&delivery.message_id)
                && talk_target_id_is_valid(&delivery.thread_id)
            {
                targets
                    .entry(delivery.message_id.clone())
                    .or_insert_with(|| TalkMessageTarget {
                        message_id: delivery.message_id,
                        thread_id: delivery.thread_id,
                        page_id: delivery.page_id,
                    });
            }
        }
        for record in self.pages()? {
            for target in self.file_talk_message_targets(&record)? {
                targets.entry(target.message_id.clone()).or_insert(target);
            }
        }
        Ok(targets.into_values().collect())
    }

    fn file_talk_message_targets(&self, record: &PageRecord) -> Result<Vec<TalkMessageTarget>> {
        Ok(self
            .file_talk_messages(record)?
            .into_iter()
            .map(|message| TalkMessageTarget {
                message_id: message.message_id,
                thread_id: message.thread_id,
                page_id: message.page_id,
            })
            .collect())
    }

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
            let body_markdown = body.trim_start_matches(['\n', '\r']).to_string();
            let subject = talk_file_subject(&fields, body, message_id);
            let reply_to = fields
                .get("reply_to")
                .filter(|reply_to| talk_target_id_is_valid(reply_to))
                .cloned();
            messages.push(TalkFileMessage {
                message_id: message_id.clone(),
                thread_id: talk_file_thread_id(record, &fields, body, message_id),
                reply_to,
                page_id: record.id.clone(),
                route: record.route.clone(),
                kind: fields
                    .get("kind")
                    .map(|kind| kind.trim().to_string())
                    .filter(|kind| !kind.is_empty())
                    .unwrap_or_else(|| "talk".to_string()),
                excerpt: excerpt(&body_markdown),
                subject,
                attachments: parse_talk_file_attachments(&text)?,
                created_at: fields
                    .get("created")
                    .or_else(|| fields.get("created_at"))
                    .cloned()
                    .unwrap_or_default(),
                source: format!(
                    "user-wiki://page/{}/talk/messages/{}",
                    record.id, message_id
                ),
                source_path: display_under_root(&path, &self.paths.root),
                body_markdown,
            });
        }
        Ok(messages)
    }

    fn normalize_talk_recipient(&self, recipient: &str) -> Result<String> {
        self.normalize_mail_recipient(recipient)
    }

    fn normalize_mail_recipient(&self, recipient: &str) -> Result<String> {
        if let Some(reference) = recipient.strip_prefix("page://") {
            if reference.trim() != reference
                || reference.is_empty()
                || reference.chars().any(char::is_whitespace)
            {
                return Err(anyhow!(
                    "invalid page recipient {recipient}; expected page://<page-id-or-route>"
                ));
            }
            let page = self.find_page(reference).with_context(|| {
                format!(
                    "invalid page recipient {recipient}; expected page://<page-id-or-route> for a configured page"
                )
            })?;
            return Ok(format!("mailbox://page/{}", page.id));
        }
        validate_address(recipient)?;
        Ok(recipient.to_string())
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

    pub fn inbox(&self, recipient: &str) -> Result<InboxResult> {
        self.inbox_with_options(recipient, false, false)
    }

    pub fn read_mail(
        &self,
        message_id: Option<&str>,
        thread_id: Option<&str>,
    ) -> Result<MailReadResult> {
        if message_id.is_none() && thread_id.is_none() {
            return Err(anyhow!("mail-read requires --message-id or --thread-id"));
        }
        if message_id.is_some() && thread_id.is_some() {
            return Err(anyhow!(
                "mail-read accepts either --message-id or --thread-id, not both"
            ));
        }

        let deliveries = self
            .all_mailbox_messages()?
            .into_iter()
            .filter(|message| {
                message_id.is_some_and(|id| message.message_id == id)
                    || thread_id.is_some_and(|id| message.thread_id == id)
            })
            .collect::<Vec<_>>();
        let delivery_count = deliveries.len();
        let mut messages_by_id = BTreeMap::<String, MailReadMessage>::new();

        let mut deliveries = deliveries;
        deliveries.sort_by(|a, b| {
            a.message_id
                .cmp(&b.message_id)
                .then_with(|| a.recipient.cmp(&b.recipient))
        });

        let mut grouped = BTreeMap::<String, Vec<DeliveryRecord>>::new();
        for delivery in deliveries {
            grouped
                .entry(delivery.message_id.clone())
                .or_default()
                .push(delivery);
        }

        for (_message_id, mut group) in grouped {
            group.sort_by(|a, b| a.recipient.cmp(&b.recipient));
            let first = group
                .first()
                .cloned()
                .ok_or_else(|| anyhow!("mail read group unexpectedly empty"))?;
            let (source_path, body_markdown) =
                self.read_talk_message_source(&first.page_id, &first.message_id)?;
            let attachment_count = first.attachments.len();
            let message = MailReadMessage {
                message_id: first.message_id,
                thread_id: first.thread_id,
                reply_to: first.reply_to,
                page_id: first.page_id,
                route: first.route,
                kind: first.kind,
                subject: first.subject,
                excerpt: first.excerpt,
                attachments: first.attachments,
                attachment_count,
                created_at: first.created_at,
                source: first.source,
                source_path,
                body_markdown,
                deliveries: group
                    .iter()
                    .map(|delivery| DeliveryState {
                        recipient: delivery.recipient.clone(),
                        state: delivery.state.clone(),
                    })
                    .collect(),
            };
            messages_by_id.insert(message.message_id.clone(), message);
        }

        for record in self.pages()? {
            for file_message in self.file_talk_messages(&record)? {
                let matches_target = message_id.is_some_and(|id| file_message.message_id == id)
                    || thread_id.is_some_and(|id| file_message.thread_id == id);
                if matches_target && !messages_by_id.contains_key(&file_message.message_id) {
                    messages_by_id.insert(
                        file_message.message_id.clone(),
                        MailReadMessage {
                            message_id: file_message.message_id,
                            thread_id: file_message.thread_id,
                            reply_to: file_message.reply_to,
                            page_id: file_message.page_id,
                            route: file_message.route,
                            kind: file_message.kind,
                            subject: file_message.subject,
                            excerpt: file_message.excerpt,
                            attachment_count: file_message.attachments.len(),
                            attachments: file_message.attachments,
                            created_at: file_message.created_at,
                            source: file_message.source,
                            source_path: file_message.source_path,
                            body_markdown: file_message.body_markdown,
                            deliveries: Vec::new(),
                        },
                    );
                }
            }
        }

        if messages_by_id.is_empty() {
            let target = message_id.or(thread_id).unwrap_or_default();
            return Err(anyhow!("mail read target not found: {target}"));
        }

        let mut messages = messages_by_id.into_values().collect::<Vec<_>>();
        messages.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.message_id.cmp(&b.message_id))
        });

        let resolved_by = if message_id.is_some() {
            "message_id"
        } else {
            "thread_id"
        };
        Ok(MailReadResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.mail.read".to_string(),
            surface: "mail_thread".to_string(),
            resolved_by: resolved_by.to_string(),
            message_id: message_id.map(ToString::to_string),
            thread_id: thread_id.map(ToString::to_string),
            message_count: messages.len(),
            delivery_count,
            messages,
            next_action: "none".to_string(),
            repair_hints: Vec::new(),
        })
    }

    fn read_talk_message_source(
        &self,
        page_id: &str,
        message_id: &str,
    ) -> Result<(String, String)> {
        let record = self
            .pages()?
            .into_iter()
            .find(|page| page.id == page_id)
            .ok_or_else(|| anyhow!("mail read page not found: {page_id}"))?;
        let talk_dir = self.talk_dir(&record);
        if !talk_dir.is_dir() {
            return Err(anyhow!("talk folder missing for page {page_id}"));
        }
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
            if fields.get("id").is_some_and(|id| id == message_id) {
                let (_, body) = split_markdown_frontmatter(&text)?;
                return Ok((
                    display_under_root(&path, &self.paths.root),
                    body.trim_start_matches(['\n', '\r']).to_string(),
                ));
            }
        }
        Err(anyhow!(
            "talk message source not found for message {message_id}"
        ))
    }

    pub fn inbox_with_archived(
        &self,
        recipient: &str,
        include_archived: bool,
    ) -> Result<InboxResult> {
        self.inbox_with_options(recipient, include_archived, false)
    }

    pub fn inbox_with_options(
        &self,
        recipient: &str,
        include_archived: bool,
        include_snoozed: bool,
    ) -> Result<InboxResult> {
        self.inbox_with_kind_filter(recipient, include_archived, include_snoozed, None)
    }

    fn inbox_with_kind_filter(
        &self,
        recipient: &str,
        include_archived: bool,
        include_snoozed: bool,
        kind_filter: Option<&MailKindFilter>,
    ) -> Result<InboxResult> {
        let recipient = self.normalize_mail_recipient(recipient)?;
        let mut messages = self.mailbox(&recipient)?;
        if let Some(kind_filter) = kind_filter {
            messages.retain(|message| kind_filter.accepts(&message.kind));
        }
        let total_count = messages.len();
        let archived_count = messages
            .iter()
            .filter(|message| message.state == "archived")
            .count();
        if !include_archived {
            messages.retain(|message| message.state != "archived");
        }
        if !include_snoozed {
            messages.retain(|message| !mail_delivery_is_snoozed_until_future(message));
        }
        messages.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let unread_count = messages
            .iter()
            .filter(|message| message.state == "unread")
            .count();
        let actionable_count = messages
            .iter()
            .filter(|message| mail_delivery_is_actionable(message))
            .count();
        let message_count = messages.len();
        Ok(InboxResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.mail.inbox".to_string(),
            surface: mailbox_surface(&recipient),
            message_count,
            actionable_count,
            unread_count,
            archived_count,
            mailbox: MailboxCount {
                address: recipient.to_string(),
                surface: mailbox_surface(&recipient),
                total_count,
                actionable_count,
                unread_count,
                archived_count,
            },
            messages,
            next_action: if actionable_count > 0 {
                "mail_claim_or_mark".to_string()
            } else {
                "none".to_string()
            },
        })
    }

    pub fn agent_inbox(
        &self,
        agent_id: &str,
        include_archived: bool,
        include_snoozed: bool,
    ) -> Result<AgentInboxResult> {
        let agent = self.require_active_agent(agent_id)?;
        let agent_liveness = self.agent_liveness_index()?;
        let subscriptions = self
            .active_mail_subscriptions()?
            .into_iter()
            .filter(|subscription| subscription.agent_id == agent_id)
            .collect::<Vec<_>>();
        let enriched_subscriptions = subscriptions
            .iter()
            .cloned()
            .map(|subscription| enrich_mail_subscription(subscription, &agent_liveness))
            .collect::<Vec<_>>();
        let owned_addresses = agent.addresses.clone();
        let subscribed_addresses = subscriptions
            .iter()
            .map(|subscription| subscription.address.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let effective_mailboxes = owned_addresses
            .iter()
            .chain(
                subscriptions
                    .iter()
                    .map(|subscription| &subscription.address),
            )
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let owned_address_set = owned_addresses.iter().cloned().collect::<BTreeSet<_>>();
        let subscription_kind_filters = mail_kind_filters_by_address(&subscriptions);
        let mut mailboxes = Vec::new();
        let mut messages = Vec::new();
        let mut seen = BTreeSet::new();
        for address in &effective_mailboxes {
            let kind_filter = if owned_address_set.contains(address) {
                None
            } else {
                subscription_kind_filters.get(address)
            };
            let mailbox = self.inbox_with_kind_filter(
                address,
                include_archived,
                include_snoozed,
                kind_filter,
            )?;
            mailboxes.push(mailbox.mailbox.clone());
            for message in mailbox.messages {
                let key = format!("{}:{}", message.message_id, message.recipient);
                if seen.insert(key) {
                    messages.push(message);
                }
            }
        }
        messages.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        let actionable_message_ids = messages
            .iter()
            .filter(|message| mail_delivery_is_actionable(message))
            .map(|message| message.message_id.clone())
            .collect::<BTreeSet<_>>();
        let notifications = self
            .poll_notifications(agent_id)?
            .notifications
            .into_iter()
            .filter(|notification| actionable_message_ids.contains(&notification.message_id))
            .collect::<Vec<_>>();
        let threads = agent_inbox_threads(&messages, &notifications, agent_id);
        let page_counts = messages.iter().fold(
            BTreeMap::<String, (usize, usize, usize, usize)>::new(),
            |mut counts, message| {
                let entry = counts
                    .entry(message.page_id.clone())
                    .or_insert((0, 0, 0, 0));
                entry.0 += 1;
                if message.state == "unread" {
                    entry.1 += 1;
                }
                if mail_delivery_is_actionable(message) {
                    entry.2 += 1;
                }
                if mail_delivery_is_claimable_by(message, agent_id) {
                    entry.3 += 1;
                }
                counts
            },
        );
        let mut pages = Vec::new();
        for (page_id, (message_count, unread_count, actionable_count, claimable_count)) in
            page_counts
        {
            if let Ok(status) = self.page_status(&page_id) {
                pages.push(AgentInboxPageSummary {
                    id: status.id,
                    title: status.title,
                    route: status.route,
                    state: status.state,
                    next_action: status.next_action,
                    actionable_count,
                    claimable_count,
                    unread_count,
                    message_count,
                });
            }
        }
        let unread_count = messages
            .iter()
            .filter(|message| message.state == "unread")
            .count();
        let actionable_count = messages
            .iter()
            .filter(|message| mail_delivery_is_actionable(message))
            .count();
        let claimable_count = messages
            .iter()
            .filter(|message| mail_delivery_is_claimable_by(message, agent_id))
            .count();
        let actionable_thread_count = threads
            .iter()
            .filter(|thread| thread.actionable_delivery_count > 0)
            .count();
        let claimable_thread_count = threads
            .iter()
            .filter(|thread| thread.claimable_delivery_count > 0)
            .count();
        let notification_thread_count = threads
            .iter()
            .filter(|thread| thread.notification_count > 0)
            .count();
        let pages_with_open_mail_count = pages
            .iter()
            .filter(|page| page.actionable_count > 0)
            .count();
        let pages_requiring_action = pages_with_open_mail_count;
        let summary = AgentInboxSummary {
            actionable_count,
            claimable_count,
            unread_count,
            message_count: messages.len(),
            thread_count: threads.len(),
            actionable_thread_count,
            claimable_thread_count,
            notification_count: notifications.len(),
            notification_thread_count,
            pages_with_open_mail_count,
            pages_requiring_action,
        };
        Ok(AgentInboxResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.agent.inbox".to_string(),
            surface: "agent_inbox".to_string(),
            agent_id: agent_id.to_string(),
            message_count: summary.message_count,
            actionable_count: summary.actionable_count,
            claimable_count: summary.claimable_count,
            unread_count: summary.unread_count,
            thread_count: summary.thread_count,
            notification_count: summary.notification_count,
            pages_requiring_action: summary.pages_requiring_action,
            summary,
            owned_addresses,
            subscribed_addresses,
            effective_mailboxes: effective_mailboxes.clone(),
            addresses: effective_mailboxes,
            subscriptions: enriched_subscriptions,
            mailboxes,
            pages,
            threads,
            messages,
            notifications,
            next_action: if claimable_count > 0 {
                "mail_read_or_claim".to_string()
            } else if actionable_count > 0 {
                "mail_read_or_watch_claim".to_string()
            } else {
                "none".to_string()
            },
        })
    }

    pub fn claim_mail_for_agent(
        &self,
        message_id: &str,
        agent_id: &str,
    ) -> Result<MailClaimResult> {
        let inbox = self.agent_inbox(agent_id, false, false)?;
        let relation_by_address = inbox
            .subscriptions
            .iter()
            .map(|subscription| (subscription.address.clone(), subscription.relation.clone()))
            .collect::<BTreeMap<_, _>>();
        let owned_addresses = inbox.owned_addresses.into_iter().collect::<BTreeSet<_>>();
        let mut candidates = inbox
            .messages
            .into_iter()
            .filter(|message| {
                message.message_id == message_id && mail_delivery_is_claimable_by(message, agent_id)
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Err(anyhow!(
                "message {message_id} not found in claimable inbox for agent {agent_id}"
            ));
        }
        candidates.sort_by_key(|message| {
            agent_claim_recipient_priority(
                &message.recipient,
                &owned_addresses,
                &relation_by_address,
            )
        });
        let recipient = candidates
            .first()
            .map(|message| message.recipient.clone())
            .ok_or_else(|| {
                anyhow!("message {message_id} not found in actionable inbox for agent {agent_id}")
            })?;
        let mut receipt = self.claim_mail(message_id, &recipient, agent_id)?;
        receipt.operation = "wiki.agent.claim".to_string();
        Ok(receipt)
    }

    pub fn mark_mail(
        &self,
        message_id: &str,
        recipient: &str,
        state: &str,
        snoozed_until: Option<&str>,
    ) -> Result<MailMarkResult> {
        let requested_recipient = self.normalize_mail_recipient(recipient)?;
        validate_mail_mark_state(state)?;
        let snoozed_until = validate_snooze_until(state, snoozed_until)?;
        let _lock = self.acquire_mail_mutation_lock()?;
        let recipient = if self.mailbox_contains_message(&requested_recipient, message_id)? {
            requested_recipient
        } else {
            self.resolve_agent_inbox_delivery_recipient(
                message_id,
                &requested_recipient,
                None,
                false,
            )?
            .unwrap_or(requested_recipient)
        };
        let path = self.mailbox_path(&recipient);
        let mut messages = self.mailbox_file_records(&recipient)?;
        let mut found = false;
        let mut changed = false;
        for message in &mut messages {
            if message.message_id == message_id && message.recipient == recipient {
                found = true;
                if message.state != state || message.snoozed_until != snoozed_until {
                    message.state = state.to_string();
                    message.snoozed_until = snoozed_until.clone();
                    changed = true;
                }
            }
        }
        if !found {
            return Err(anyhow!(
                "message {message_id} not found in mailbox {recipient}"
            ));
        }
        if changed {
            write_jsonl(&path, &messages)?;
            append_jsonl(
                &self.paths.context_engine.join("mail/claims.jsonl"),
                &json!({
                    "schema_version": 1,
                    "message_id": message_id,
                    "recipient": recipient,
                    "state": state,
                    "snoozed_until": snoozed_until,
                    "at": now_rfc3339()
                }),
            )?;
        }
        Ok(MailMarkResult {
            schema_version: 1,
            status: if changed { "ok" } else { "unchanged" }.to_string(),
            operation: "wiki.mail.mark".to_string(),
            message_id: message_id.to_string(),
            recipient,
            state: state.to_string(),
            snoozed_until,
            evidence: vec![Evidence {
                path: display_under_root(&path, &self.paths.root),
                status: if changed {
                    "updated"
                } else {
                    "skipped_existing"
                }
                .to_string(),
            }],
            render_required: false,
            next_action: "none".to_string(),
            repair_hints: Vec::new(),
        })
    }

    pub fn mark_mail_all_deliveries(
        &self,
        message_id: &str,
        state: &str,
        snoozed_until: Option<&str>,
    ) -> Result<MailMarkAllResult> {
        validate_mail_mark_state(state)?;
        let snoozed_until = validate_snooze_until(state, snoozed_until)?;
        let _lock = self.acquire_mail_mutation_lock()?;
        let mailbox_root = self.paths.context_engine.join("mail/mailboxes");
        let mut evidence = Vec::new();
        let mut recipients = BTreeSet::new();
        let mut before = MailMarkAllCounts::default();
        let mut after = MailMarkAllCounts::default();
        let mut found = false;
        let mut changed_delivery_count = 0;
        if mailbox_root.exists() {
            for entry in fs::read_dir(&mailbox_root)? {
                let entry = entry?;
                let inbox = entry.path().join("inbox.jsonl");
                if !inbox.is_file() {
                    continue;
                }
                let mut messages = read_jsonl::<DeliveryRecord>(&inbox)?;
                refresh_delivery_attachment_counts(&mut messages);
                let mut matched = false;
                let mut changed = false;
                for message in &mut messages {
                    if message.message_id == message_id {
                        found = true;
                        matched = true;
                        count_mail_mark_all_delivery(&mut before, message);
                        if message.state != state || message.snoozed_until != snoozed_until {
                            message.state = state.to_string();
                            message.snoozed_until = snoozed_until.clone();
                            changed = true;
                            changed_delivery_count += 1;
                        }
                        count_mail_mark_all_delivery(&mut after, message);
                        recipients.insert(message.recipient.clone());
                    }
                }
                if changed {
                    write_jsonl(&inbox, &messages)?;
                    evidence.push(Evidence {
                        path: display_under_root(&inbox, &self.paths.root),
                        status: "updated".to_string(),
                    });
                } else if matched {
                    evidence.push(Evidence {
                        path: display_under_root(&inbox, &self.paths.root),
                        status: "unchanged".to_string(),
                    });
                }
            }
        }
        if !found {
            return Err(anyhow!("message {message_id} not found in any mailbox"));
        }
        let recipients = recipients.into_iter().collect::<Vec<_>>();
        if changed_delivery_count > 0 {
            append_jsonl(
                &self.paths.context_engine.join("mail/claims.jsonl"),
                &json!({
                    "schema_version": 1,
                    "message_id": message_id,
                    "recipient": "*",
                    "recipients": recipients,
                    "state": state,
                    "snoozed_until": snoozed_until,
                    "at": now_rfc3339()
                }),
            )?;
        }
        Ok(MailMarkAllResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.mail.mark_all".to_string(),
            message_id: message_id.to_string(),
            state: state.to_string(),
            changed_delivery_count,
            recipients,
            before,
            after,
            evidence,
            render_required: false,
            next_action: "none".to_string(),
            repair_hints: Vec::new(),
        })
    }

    pub fn claim_mail(
        &self,
        message_id: &str,
        recipient: &str,
        agent_id: &str,
    ) -> Result<MailClaimResult> {
        let requested_recipient = self.normalize_mail_recipient(recipient)?;
        self.require_active_agent(agent_id)?;
        let _lock = self.acquire_mail_mutation_lock()?;
        let recipient = if self.mailbox_contains_message(&requested_recipient, message_id)? {
            requested_recipient
        } else {
            self.resolve_agent_inbox_delivery_recipient(
                message_id,
                &requested_recipient,
                Some(agent_id),
                true,
            )?
            .unwrap_or(requested_recipient)
        };
        let path = self.mailbox_path(&recipient);
        let mut messages = self.mailbox_file_records(&recipient)?;
        let claimed_at = now_rfc3339();
        let mut found = false;
        let mut changed = false;
        let mut status = "claimed".to_string();
        for message in &mut messages {
            if message.message_id == message_id && message.recipient == recipient {
                found = true;
                if message.state == "claimed"
                    && message
                        .claimed_by
                        .as_deref()
                        .is_some_and(|claimant| claimant != agent_id)
                {
                    return Err(anyhow!(
                        "message {message_id} in mailbox {recipient} already claimed by {}",
                        message.claimed_by.as_deref().unwrap_or("unknown")
                    ));
                }
                if message.state == "claimed" && message.claimed_by.as_deref() == Some(agent_id) {
                    status = "already_claimed".to_string();
                }
                if mail_delivery_state_is_terminal(message.state.as_str()) {
                    return Err(anyhow!(
                        "message {message_id} in mailbox {recipient} is {} and not claimable",
                        message.state
                    ));
                }
                if status != "already_claimed" {
                    message.state = "claimed".to_string();
                    message.snoozed_until = None;
                    message.claimed_by = Some(agent_id.to_string());
                    message.claimed_at = Some(claimed_at.clone());
                    changed = true;
                }
            }
        }
        if !found {
            return Err(anyhow!(
                "message {message_id} not found in mailbox {recipient}"
            ));
        }
        if changed {
            write_jsonl(&path, &messages)?;
            append_jsonl(
                &self.paths.context_engine.join("mail/claims.jsonl"),
                &json!({
                    "schema_version": 1,
                    "message_id": message_id,
                    "recipient": recipient,
                    "state": "claimed",
                    "claimed_by": agent_id,
                    "at": claimed_at
                }),
            )?;
        }
        Ok(MailClaimResult {
            schema_version: 1,
            status,
            operation: "wiki.mail.claim".to_string(),
            message_id: message_id.to_string(),
            recipient,
            claimed_by: agent_id.to_string(),
            state: "claimed".to_string(),
            evidence: vec![Evidence {
                path: display_under_root(&path, &self.paths.root),
                status: if changed {
                    "updated"
                } else {
                    "skipped_existing"
                }
                .to_string(),
            }],
            render_required: false,
            next_action: "none".to_string(),
            repair_hints: Vec::new(),
        })
    }

    fn resolve_agent_inbox_delivery_recipient(
        &self,
        message_id: &str,
        requested_recipient: &str,
        agent_id: Option<&str>,
        claimable_only: bool,
    ) -> Result<Option<String>> {
        let agents = if let Some(agent_id) = agent_id {
            let agent = self.require_active_agent(agent_id)?;
            if !agent
                .addresses
                .iter()
                .any(|address| address == requested_recipient)
            {
                return Ok(None);
            }
            vec![(agent_id.to_string(), agent)]
        } else {
            self.current_agents()?
                .into_iter()
                .filter(|(_, agent)| {
                    agent
                        .addresses
                        .iter()
                        .any(|address| address == requested_recipient)
                })
                .collect::<Vec<_>>()
        };

        if agents.is_empty() {
            return Ok(None);
        }
        if agents.len() > 1 {
            return Err(anyhow!(
                "mail recipient {requested_recipient} belongs to multiple active agents; use agent-claim or the canonical mailbox recipient"
            ));
        }

        let (agent_id, _) = agents.into_iter().next().unwrap();
        let inbox = self.agent_inbox(&agent_id, true, true)?;
        let relation_by_address = inbox
            .subscriptions
            .iter()
            .map(|subscription| (subscription.address.clone(), subscription.relation.clone()))
            .collect::<BTreeMap<_, _>>();
        let owned_addresses = inbox.owned_addresses.into_iter().collect::<BTreeSet<_>>();
        let mut candidates = inbox
            .messages
            .into_iter()
            .filter(|message| {
                message.message_id == message_id
                    && (!claimable_only || mail_delivery_is_claimable_by(message, &agent_id))
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return Ok(None);
        }
        candidates.sort_by_key(|message| {
            agent_claim_recipient_priority(
                &message.recipient,
                &owned_addresses,
                &relation_by_address,
            )
        });
        Ok(candidates.first().map(|message| message.recipient.clone()))
    }

    pub fn poll_notifications(&self, agent_id: &str) -> Result<NotificationPollResult> {
        let agent = self.require_active_agent(agent_id)?;
        let owned_addresses = agent.addresses.into_iter().collect::<BTreeSet<_>>();
        let active_subscriptions = self
            .active_mail_subscriptions()?
            .into_iter()
            .filter(|subscription| subscription.agent_id == agent_id)
            .collect::<Vec<_>>();
        let subscription_kind_filters = mail_kind_filters_by_address(&active_subscriptions);
        let acknowledged = read_jsonl::<NotificationAttempt>(&self.notifications_attempts_path())?
            .into_iter()
            .filter(|attempt| {
                attempt.agent_id == agent_id && notification_ack_state_is_terminal(&attempt.state)
            })
            .map(|attempt| attempt.notification_id)
            .collect::<std::collections::BTreeSet<_>>();
        let notifications: Vec<NotificationRecord> =
            read_jsonl::<NotificationRecord>(&self.notifications_outbox_path())?
                .into_iter()
                .filter(|notification| {
                    notification.agent_id == agent_id
                        && !acknowledged.contains(&notification.notification_id)
                        && (owned_addresses.contains(&notification.mailbox)
                            || subscription_kind_filters
                                .get(&notification.mailbox)
                                .is_some_and(|filter| filter.accepts(&notification.kind)))
                        && self
                            .notification_delivery_is_actionable(notification)
                            .unwrap_or(false)
                })
                .collect();
        let notification_count = notifications.len();
        Ok(NotificationPollResult {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.notify.poll".to_string(),
            surface: "agent_notification_queue".to_string(),
            notification_count,
            notifications,
            next_action: if notification_count > 0 {
                "notify_ack".to_string()
            } else {
                "none".to_string()
            },
        })
    }

    pub fn ack_notification(
        &self,
        agent_id: &str,
        notification_id: &str,
        state: &str,
    ) -> Result<OperationReceipt> {
        if !notification_ack_state_is_allowed(state) {
            return Err(anyhow!(
                "invalid notification ack state {state}; expected delivered, claimed, read, dismissed, archived, or failed"
            ));
        }
        self.require_active_agent(agent_id)?;
        let _lock = self.acquire_mail_mutation_lock()?;
        let notification_exists =
            read_jsonl::<NotificationRecord>(&self.notifications_outbox_path())?
                .into_iter()
                .any(|notification| {
                    notification.agent_id == agent_id
                        && notification.notification_id == notification_id
                });
        if !notification_exists {
            return Err(anyhow!(
                "notification {notification_id} not found for active agent {agent_id}"
            ));
        }
        let already_acknowledged =
            read_jsonl::<NotificationAttempt>(&self.notifications_attempts_path())?
                .into_iter()
                .any(|attempt| {
                    attempt.agent_id == agent_id
                        && attempt.notification_id == notification_id
                        && notification_ack_state_is_terminal(&attempt.state)
                });
        if already_acknowledged {
            return Ok(OperationReceipt {
                schema_version: 1,
                status: "already_acknowledged".to_string(),
                operation: "wiki.notify.ack".to_string(),
                id: notification_id.to_string(),
                route: None,
                page_type: None,
                collection: None,
                evidence: vec![Evidence {
                    path: display_under_root(&self.notifications_attempts_path(), &self.paths.root),
                    status: "skipped_existing".to_string(),
                }],
                page_status: None,
                edit: None,
                hashes: None,
                link_impact: None,
                render_required: false,
                next_action: "none".to_string(),
                repair_hints: Vec::new(),
            });
        }
        let attempt = NotificationAttempt {
            schema_version: 1,
            agent_id: agent_id.to_string(),
            notification_id: notification_id.to_string(),
            state: state.to_string(),
            at: now_rfc3339(),
        };
        append_jsonl(&self.notifications_attempts_path(), &attempt)?;
        Ok(OperationReceipt {
            schema_version: 1,
            status: "ok".to_string(),
            operation: "wiki.notify.ack".to_string(),
            id: notification_id.to_string(),
            route: None,
            page_type: None,
            collection: None,
            evidence: vec![Evidence {
                path: display_under_root(&self.notifications_attempts_path(), &self.paths.root),
                status: "appended".to_string(),
            }],
            page_status: None,
            edit: None,
            hashes: None,
            link_impact: None,
            render_required: false,
            next_action: "none".to_string(),
            repair_hints: Vec::new(),
        })
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
        let page_mailbox = format!("mailbox://page/{}", record.id);
        let associated_lists = self
            .mail_lists(Some(&record.id), None)
            .map(|result| result.lists)
            .unwrap_or_default();
        let associated_list_addresses = associated_lists
            .iter()
            .map(|list| list.address.clone())
            .collect::<BTreeSet<_>>();
        let page_messages = self
            .all_mailbox_messages()
            .unwrap_or_default()
            .into_iter()
            .filter(|message| {
                message.page_id == record.id
                    && (page_subscription_is_related(&record.id, &page_mailbox, &message.recipient)
                        || associated_list_addresses.contains(&message.recipient))
            })
            .map(|message| {
                (
                    format!("{}:{}", message.message_id, message.recipient),
                    message,
                )
            })
            .collect::<BTreeMap<_, _>>()
            .into_values()
            .collect::<Vec<_>>();
        let unread_count = page_messages
            .iter()
            .filter(|message| message.state == "unread")
            .count();
        let open_delivery_count = page_messages
            .iter()
            .filter(|message| mail_delivery_is_actionable(message))
            .count();
        let open_thread_count = page_messages
            .iter()
            .filter(|message| mail_delivery_is_actionable(message))
            .map(|message| message.thread_id.clone())
            .collect::<BTreeSet<_>>()
            .len();
        let agent_liveness = self.agent_liveness_index().unwrap_or_default();
        let page_subscriptions = self
            .active_mail_subscriptions()
            .unwrap_or_default()
            .into_iter()
            .filter(|subscription| {
                page_subscription_is_related(&record.id, &page_mailbox, &subscription.address)
                    || associated_list_addresses.contains(&subscription.address)
            })
            .map(|subscription| page_subscription_summary(subscription, &agent_liveness))
            .collect::<Vec<_>>();
        let watcher_agents = page_subscriptions
            .iter()
            .filter(|subscription| subscription.relation == "watcher")
            .map(|subscription| {
                (
                    subscription.agent_id.clone(),
                    subscription.agent_liveness.clone(),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let watcher_count = watcher_agents.len();
        let active_watcher_count = watcher_agents
            .values()
            .filter(|liveness| liveness.as_str() == "active")
            .count();
        let inactive_watcher_count = watcher_count.saturating_sub(active_watcher_count);
        let watcher_subscription_count = page_subscriptions
            .iter()
            .filter(|subscription| subscription.relation == "watcher")
            .count();
        let subscription_liveness_counts =
            subscription_liveness_counts(page_subscriptions.iter().map(|subscription| {
                (
                    subscription.agent_id.as_str(),
                    subscription.agent_liveness.as_str(),
                )
            }));
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
                mailbox: format!("mailbox://page/{}", record.id),
                published: format!("app-support://wiki{}", record.route),
            },
            mail: PageMailSummary {
                page_mailbox: page_mailbox.clone(),
                curator_address: format!("role://{}.curator", record.id),
                default_watchers_list: format!("list://{}.watchers", record.id),
                associated_lists,
                message_count: page_messages.len(),
                actionable_count: open_delivery_count,
                open_delivery_count,
                open_thread_count,
                unread_count,
                watcher_count,
                watcher_subscription_count,
                active_watcher_count,
                inactive_watcher_count,
                subscription_liveness_counts,
                subscriptions: page_subscriptions,
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
                mailbox: format!("mailbox://site-page/{}", record.id),
                published: format!("app-support://wiki{}", record.route),
            },
            mail: PageMailSummary {
                page_mailbox: format!("mailbox://site-page/{}", record.id),
                curator_address: "not_applicable".to_string(),
                default_watchers_list: "not_applicable".to_string(),
                associated_lists: Vec::new(),
                message_count: 0,
                actionable_count: 0,
                open_delivery_count: 0,
                open_thread_count: 0,
                unread_count: 0,
                watcher_count: 0,
                watcher_subscription_count: 0,
                active_watcher_count: 0,
                inactive_watcher_count: 0,
                subscription_liveness_counts: SubscriptionLivenessCounts::default(),
                subscriptions: Vec::new(),
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
        Ok(PageAssetRecord {
            id: format!("asset_{}_{}", record.id, slug_token(&filename)),
            filename,
            media_type,
            sha256,
            bytes,
            handle: format!(
                "user-wiki://page/{}/assets/{}",
                record.id,
                path.file_name().unwrap().to_string_lossy()
            ),
            source_path: display_under_root(path, &self.paths.root),
            absolute_path: path.display().to_string(),
            source_relative_href,
            published_href,
            markdown,
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
        _before_hash: String,
        next_content: String,
        operation: &str,
        ledger_event: &str,
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
                evidence,
                page_status,
                edit,
                hashes,
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
            actor: None,
            origin: Some("body_edit".to_string()),
            source_sha256: after_hash,
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
            evidence,
            page_status,
            edit,
            hashes,
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
            talk_curator_template: options.talk_curator_template.clone(),
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

        if let Some(curator_template) = options.talk_curator_template.as_deref() {
            self.read_template(curator_template)?;
        }
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
            ("talk_for_uri".to_string(), format!("page://{}", record.id)),
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
        hasher.update(b"1context-page-publish-fingerprint-v1");
        hasher.update([0]);
        hasher.update(serde_json::to_vec(record)?);
        hasher.update([0]);
        hash_optional_file(&mut hasher, "source", &source)?;
        hash_optional_file(&mut hasher, "tombstone", &tombstone)?;
        hash_optional_tree(&mut hasher, "assets", &self.asset_dir(record))?;
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

    fn agent_events_path(&self) -> PathBuf {
        self.paths
            .context_engine
            .join("agents/directory/agents.jsonl")
    }

    fn current_agents_path(&self) -> PathBuf {
        self.paths
            .context_engine
            .join("agents/directory/current.json")
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

    fn current_agents(&self) -> Result<BTreeMap<String, AgentRecord>> {
        self.current_agents_with_stale(false)
    }

    fn agent_directory_entries(&self) -> Result<BTreeMap<String, AgentDirectoryEntry>> {
        let mut current = BTreeMap::<String, AgentDirectoryEntry>::new();
        for event in read_jsonl::<AgentRecord>(&self.agent_events_path())? {
            let registered_at = current
                .get(&event.agent_id)
                .and_then(|entry| entry.registered_at.clone())
                .or_else(|| {
                    if event.event == "agent.registered" {
                        Some(event.at.clone())
                    } else {
                        None
                    }
                });
            let liveness = if event.event == "agent.retired" {
                AgentLiveness {
                    state: "retired".to_string(),
                    lease_expires_at: Some(event.lease_expires_at.clone()),
                    retired_at: Some(event.at.clone()),
                    retire_reason: event.reason.clone(),
                }
            } else {
                AgentLiveness {
                    state: if agent_lease_is_active(&event) {
                        "active".to_string()
                    } else {
                        "stale".to_string()
                    },
                    lease_expires_at: Some(event.lease_expires_at.clone()),
                    retired_at: None,
                    retire_reason: None,
                }
            };
            current.insert(
                event.agent_id.clone(),
                AgentDirectoryEntry {
                    record: event,
                    liveness,
                    registered_at,
                },
            );
        }
        Ok(current)
    }

    fn active_subscriptions_by_agent(
        &self,
    ) -> Result<BTreeMap<String, Vec<MailSubscriptionRecord>>> {
        let mut by_agent = BTreeMap::<String, Vec<MailSubscriptionRecord>>::new();
        for subscription in self.active_mail_subscriptions()? {
            by_agent
                .entry(subscription.agent_id.clone())
                .or_default()
                .push(subscription);
        }
        Ok(by_agent)
    }

    fn agent_directory_summary(
        &self,
        entry: AgentDirectoryEntry,
        subscriptions_by_agent: &BTreeMap<String, Vec<MailSubscriptionRecord>>,
    ) -> Result<AgentDirectorySummary> {
        let subscriptions = subscriptions_by_agent
            .get(&entry.record.agent_id)
            .cloned()
            .unwrap_or_default();
        let owned_addresses = entry.record.addresses.clone();
        let subscribed_addresses = subscriptions
            .iter()
            .map(|subscription| subscription.address.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let mailbox_counts = self.agent_mailbox_counts(&owned_addresses, &subscriptions)?;
        let mailbox_count = mailbox_counts
            .iter()
            .map(|mailbox| mailbox.total_count)
            .sum();
        let unread_count = mailbox_counts
            .iter()
            .map(|mailbox| mailbox.unread_count)
            .sum();
        let actionable_count = mailbox_counts
            .iter()
            .map(|mailbox| mailbox.actionable_count)
            .sum();
        let next_action = match entry.liveness.state.as_str() {
            "active" if actionable_count > 0 => "check_inbox",
            "active" => "none",
            "stale" => "agent_identify",
            "retired" => "agent_register_new_thread",
            _ => "agent_register",
        };
        Ok(AgentDirectorySummary {
            primary_address: entry.record.addresses.first().cloned(),
            agent_id: entry.record.agent_id,
            liveness: entry.liveness.state,
            event: entry.record.event,
            registered_at: entry.registered_at,
            last_seen_at: entry.record.at,
            lease_expires_at: entry.record.lease_expires_at,
            retired_at: entry.liveness.retired_at,
            retire_reason: entry.liveness.retire_reason,
            addresses: entry.record.addresses,
            roles: entry.record.roles,
            capabilities: entry.record.capabilities,
            transport: entry.record.transport,
            owned_addresses,
            subscribed_addresses,
            active_subscription_count: subscriptions.len(),
            mailbox_count,
            unread_count,
            actionable_count,
            next_action: next_action.to_string(),
        })
    }

    fn agent_mailbox_counts(
        &self,
        owned_addresses: &[String],
        subscriptions: &[MailSubscriptionRecord],
    ) -> Result<Vec<MailboxCount>> {
        let mut addresses = owned_addresses.iter().cloned().collect::<BTreeSet<_>>();
        addresses.extend(
            subscriptions
                .iter()
                .map(|subscription| subscription.address.clone()),
        );
        let owned_address_set = owned_addresses.iter().cloned().collect::<BTreeSet<_>>();
        let subscription_kind_filters = mail_kind_filters_by_address(subscriptions);
        addresses
            .into_iter()
            .map(|address| {
                let kind_filter = if owned_address_set.contains(&address) {
                    None
                } else {
                    subscription_kind_filters.get(&address)
                };
                Ok(self
                    .inbox_with_kind_filter(&address, true, true, kind_filter)?
                    .mailbox)
            })
            .collect()
    }

    fn current_agents_with_stale(
        &self,
        include_stale: bool,
    ) -> Result<BTreeMap<String, AgentRecord>> {
        let mut current = BTreeMap::new();
        for event in read_jsonl::<AgentRecord>(&self.agent_events_path())? {
            if event.event == "agent.retired" {
                current.remove(&event.agent_id);
            } else {
                current.insert(event.agent_id.clone(), event);
            }
        }
        if !include_stale {
            current.retain(|_, agent| agent_lease_is_active(agent));
        }
        Ok(current)
    }

    fn agent_liveness_index(&self) -> Result<BTreeMap<String, AgentLiveness>> {
        let mut current = BTreeMap::new();
        for event in read_jsonl::<AgentRecord>(&self.agent_events_path())? {
            if event.event == "agent.retired" {
                current.insert(
                    event.agent_id.clone(),
                    AgentLiveness {
                        state: "retired".to_string(),
                        lease_expires_at: Some(event.lease_expires_at.clone()),
                        retired_at: Some(event.at.clone()),
                        retire_reason: event.reason.clone(),
                    },
                );
            } else {
                current.insert(
                    event.agent_id.clone(),
                    AgentLiveness {
                        state: if agent_lease_is_active(&event) {
                            "active".to_string()
                        } else {
                            "stale".to_string()
                        },
                        lease_expires_at: Some(event.lease_expires_at.clone()),
                        retired_at: None,
                        retire_reason: None,
                    },
                );
            }
        }
        Ok(current)
    }

    fn require_active_agent(&self, agent_id: &str) -> Result<AgentRecord> {
        if let Some(agent) = self.current_agents()?.remove(agent_id) {
            return Ok(agent);
        }
        if let Some(liveness) = self.agent_liveness_index()?.get(agent_id) {
            if liveness.state == "retired" {
                return Err(anyhow!(
                    "agent explicitly retired: {agent_id}; register a new thread/session instead of using a retired identity"
                ));
            }
            if liveness.state == "stale" {
                let lease = liveness
                    .lease_expires_at
                    .as_deref()
                    .unwrap_or("an earlier time");
                return Err(anyhow!(
                    "agent lease expired: {agent_id}; lease expired at {lease}; call agent-identify with the same thread id to refresh the session before using this command"
                ));
            }
        }
        Err(anyhow!("unknown active agent: {agent_id}"))
    }

    fn resolve_owner_address(&self, owner: &str) -> Result<String> {
        if validate_address(owner).is_ok() {
            return Ok(owner.to_string());
        }
        let agent = self.require_active_agent(owner)?;
        agent
            .addresses
            .first()
            .cloned()
            .ok_or_else(|| anyhow!("agent {owner} has no address to own this list"))
    }

    fn deliveries_path(&self) -> PathBuf {
        self.paths.context_engine.join("mail/deliveries.jsonl")
    }

    fn mail_subscriptions_path(&self) -> PathBuf {
        self.paths.context_engine.join("mail/subscriptions.jsonl")
    }

    fn mail_lists_path(&self) -> PathBuf {
        self.paths.context_engine.join("mail/lists.jsonl")
    }

    fn current_mail_lists(&self) -> Result<Vec<MailListRecord>> {
        let mut current = BTreeMap::new();
        for event in read_jsonl::<MailListRecord>(&self.mail_lists_path())? {
            if event.event == "mail.list.deleted" || event.state == "deleted" {
                current.remove(&event.address);
            } else {
                current.insert(event.address.clone(), event);
            }
        }
        Ok(current
            .into_values()
            .filter(|list| list.state == "active")
            .collect())
    }

    fn subscription_liveness_counts_by_address(
        &self,
    ) -> Result<BTreeMap<String, SubscriptionLivenessCounts>> {
        let agent_liveness = self.agent_liveness_index()?;
        let mut agents_by_address = BTreeMap::<String, BTreeMap<String, String>>::new();
        for subscription in self.active_mail_subscriptions()? {
            let liveness = agent_liveness
                .get(&subscription.agent_id)
                .map(|liveness| liveness.state.clone())
                .unwrap_or_else(|| "unknown".to_string());
            agents_by_address
                .entry(subscription.address)
                .or_default()
                .insert(subscription.agent_id, liveness);
        }
        Ok(agents_by_address
            .into_iter()
            .map(|(address, agents)| {
                let counts = subscription_liveness_counts(
                    agents
                        .iter()
                        .map(|(agent_id, liveness)| (agent_id.as_str(), liveness.as_str())),
                );
                (address, counts)
            })
            .collect())
    }

    fn active_mail_subscriptions(&self) -> Result<Vec<MailSubscriptionRecord>> {
        let mut current = BTreeMap::new();
        for event in read_jsonl::<MailSubscriptionRecord>(&self.mail_subscriptions_path())? {
            if event.event == "mail.subscription.cancelled" || event.state == "cancelled" {
                current.remove(&event.subscription_id);
            } else {
                current.insert(event.subscription_id.clone(), event);
            }
        }
        Ok(current
            .into_values()
            .filter(mail_subscription_lease_is_active)
            .collect())
    }

    fn mailbox_path(&self, recipient: &str) -> PathBuf {
        self.paths
            .context_engine
            .join("mail/mailboxes")
            .join(address_key(recipient))
            .join("inbox.jsonl")
    }

    fn mailbox(&self, recipient: &str) -> Result<Vec<DeliveryRecord>> {
        Ok(self
            .mailbox_file_records(recipient)?
            .into_iter()
            .filter(|delivery| delivery.recipient == recipient)
            .collect())
    }

    fn mailbox_contains_message(&self, recipient: &str, message_id: &str) -> Result<bool> {
        Ok(self
            .mailbox_file_records(recipient)?
            .iter()
            .any(|delivery| delivery.message_id == message_id && delivery.recipient == recipient))
    }

    fn mailbox_file_records(&self, recipient: &str) -> Result<Vec<DeliveryRecord>> {
        let mut messages = read_jsonl(&self.mailbox_path(recipient))?;
        refresh_delivery_attachment_counts(&mut messages);
        Ok(messages)
    }

    fn notifications_outbox_path(&self) -> PathBuf {
        self.paths.context_engine.join("notifications/outbox.jsonl")
    }

    fn notifications_attempts_path(&self) -> PathBuf {
        self.paths
            .context_engine
            .join("notifications/attempts.jsonl")
    }

    fn enqueue_notifications(
        &self,
        deliveries: &[DeliveryRecord],
    ) -> Result<Vec<NotificationRecord>> {
        let current = self.current_agents()?;
        let subscriptions = self.active_mail_subscriptions()?;
        let mut notifications = Vec::new();
        let mut seen = BTreeSet::new();
        for delivery in deliveries {
            for (agent_id, agent) in &current {
                let directly_addressed = agent
                    .addresses
                    .iter()
                    .any(|address| address == &delivery.recipient);
                let subscribed = subscriptions.iter().any(|subscription| {
                    subscription.agent_id == *agent_id
                        && subscription.address == delivery.recipient
                        && subscription_accepts_kind(subscription, &delivery.kind)
                });
                if !directly_addressed && !subscribed {
                    continue;
                }
                let delivery_key = format!(
                    "{}:{}:{}",
                    delivery.message_id, agent_id, delivery.recipient
                );
                if !seen.insert(delivery_key.clone()) {
                    continue;
                }
                let agent_address = agent
                    .addresses
                    .first()
                    .cloned()
                    .unwrap_or_else(|| agent_id.clone());
                let notification = NotificationRecord {
                    schema_version: 1,
                    notification_id: format!(
                        "notif_{}_{}",
                        now_file_stamp().replace('-', ""),
                        short_hash(&delivery_key)
                    ),
                    agent_id: agent_id.clone(),
                    recipient: agent_address.clone(),
                    agent_address,
                    delivery_recipient: delivery.recipient.clone(),
                    mailbox: delivery.recipient.clone(),
                    message_id: delivery.message_id.clone(),
                    thread_id: delivery.thread_id.clone(),
                    reply_to: delivery.reply_to.clone(),
                    page_id: delivery.page_id.clone(),
                    route: delivery.route.clone(),
                    kind: delivery.kind.clone(),
                    subject: delivery.subject.clone(),
                    excerpt: delivery.excerpt.clone(),
                    attachment_count: delivery.attachments.len(),
                    urgency: "normal".to_string(),
                    cursor: format!("notifcur_{}", now_file_stamp().replace('-', "")),
                    created_at: now_rfc3339(),
                };
                append_jsonl(&self.notifications_outbox_path(), &notification)?;
                notifications.push(notification);
            }
        }
        Ok(notifications)
    }

    fn notification_delivery_is_actionable(
        &self,
        notification: &NotificationRecord,
    ) -> Result<bool> {
        Ok(self.mailbox(&notification.mailbox)?.iter().any(|delivery| {
            delivery.message_id == notification.message_id && mail_delivery_is_actionable(delivery)
        }))
    }

    fn agents_summary(&self) -> Result<AgentsSummary> {
        let active_count = self.current_agents()?.len();
        let all_current_count = self.current_agents_with_stale(true)?.len();
        Ok(AgentsSummary {
            active_count,
            stale_count: all_current_count.saturating_sub(active_count),
        })
    }

    fn mail_summary(&self) -> Result<MailSummary> {
        let deliveries = self.all_mailbox_messages()?;
        let notifications = read_jsonl::<NotificationRecord>(&self.notifications_outbox_path())?;
        Ok(MailSummary {
            delivery_count: deliveries.len(),
            unread_count: deliveries
                .iter()
                .filter(|delivery| delivery.state == "unread")
                .count(),
            notification_count: notifications.len(),
        })
    }

    fn all_mailbox_messages(&self) -> Result<Vec<DeliveryRecord>> {
        let mailbox_root = self.paths.context_engine.join("mail/mailboxes");
        if !mailbox_root.exists() {
            return Ok(Vec::new());
        }
        let mut messages = Vec::new();
        for entry in fs::read_dir(mailbox_root)? {
            let entry = entry?;
            let inbox = entry.path().join("inbox.jsonl");
            let mut inbox_messages = read_jsonl::<DeliveryRecord>(&inbox)?;
            refresh_delivery_attachment_counts(&mut inbox_messages);
            messages.extend(inbox_messages);
        }
        Ok(messages)
    }
}

fn refresh_delivery_attachment_counts(messages: &mut [DeliveryRecord]) {
    for message in messages {
        message.attachment_count = message.attachments.len();
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

struct FileMutationLock {
    file: File,
}

impl Drop for FileMutationLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn acquire_file_mutation_lock(path: &Path, label: &str) -> Result<FileMutationLock> {
    const LOCK_TIMEOUT: Duration = Duration::from_secs(10);

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(path)
        .with_context(|| format!("open {label} lock at {}", path.display()))?;
    let started = Instant::now();
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => {
                file.set_len(0)
                    .with_context(|| format!("truncate {label} lock at {}", path.display()))?;
                writeln!(
                    file,
                    "pid={} acquired_at={}",
                    std::process::id(),
                    now_rfc3339()
                )
                .with_context(|| format!("write {label} lock at {}", path.display()))?;
                return Ok(FileMutationLock { file });
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                if started.elapsed() >= LOCK_TIMEOUT {
                    return Err(anyhow!(
                        "timed out waiting for {label} lock at {}",
                        path.display()
                    ));
                }
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => {
                return Err(error)
                    .with_context(|| format!("acquire {label} lock at {}", path.display()));
            }
        }
    }
}

fn write_jsonl<T: Serialize>(path: &Path, values: &[T]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path)?;
    for value in values {
        serde_json::to_writer(&mut file, value)?;
        file.write_all(b"\n")?;
    }
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

fn write_json_pretty<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec_pretty(value)?)?;
    Ok(())
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

fn default_entry_template() -> String {
    "---\nid: \"{{ entry_id }}\"\nkind: \"{{ kind }}\"\nauthor: \"{{ author_id }}\"\ncreated: \"{{ created_at }}\"\ntalk_for: \"{{ talk_for_uri }}\"\nstate: open\n---\n\n## {{ title }}\n\n{{ body }}\n".to_string()
}

fn default_curator_template(record: &PageRecord) -> String {
    format!(
        "# {} Curator\n\nUse this talk folder as the inbox and decision record for `{}`.\n\n- Read new proposals before editing the page.\n- Prefer small, source-backed changes with evidence.\n- Write decisions back to this talk folder.\n- Keep page source, talk entries, and rendered output consistent.\n",
        record.title, record.id
    )
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

fn notification_ack_state_is_terminal(state: &str) -> bool {
    matches!(
        state,
        "delivered" | "claimed" | "read" | "dismissed" | "archived" | "failed"
    )
}

fn notification_ack_state_is_allowed(state: &str) -> bool {
    notification_ack_state_is_terminal(state)
}

fn mail_delivery_state_is_allowed(state: &str) -> bool {
    matches!(
        state,
        "unread" | "read" | "claimed" | "done" | "snoozed" | "archived"
    )
}

fn mail_delivery_state_is_terminal(state: &str) -> bool {
    matches!(state, "done" | "archived")
}

fn mail_delivery_is_actionable(delivery: &DeliveryRecord) -> bool {
    !mail_delivery_state_is_terminal(delivery.state.as_str())
        && !mail_delivery_is_snoozed_until_future(delivery)
}

fn mail_delivery_is_claimable_by(delivery: &DeliveryRecord, agent_id: &str) -> bool {
    mail_delivery_is_actionable(delivery)
        && delivery
            .claimed_by
            .as_deref()
            .map_or(true, |claimant| claimant == agent_id)
}

fn agent_claim_recipient_priority(
    recipient: &str,
    owned_addresses: &BTreeSet<String>,
    relation_by_address: &BTreeMap<String, String>,
) -> (u8, String) {
    let priority = if owned_addresses.contains(recipient) {
        0
    } else {
        match relation_by_address.get(recipient).map(String::as_str) {
            Some("assignee") => 1,
            Some("member") => 2,
            Some("subscriber") => 3,
            Some("watcher") => 4,
            _ if recipient.starts_with("role://") => 5,
            _ if recipient.starts_with("list://") => 6,
            _ if recipient.starts_with("mailbox://page/") => 7,
            _ => 8,
        }
    };
    (priority, recipient.to_string())
}

fn mail_delivery_is_snoozed_until_future(delivery: &DeliveryRecord) -> bool {
    delivery.state == "snoozed"
        && delivery
            .snoozed_until
            .as_deref()
            .and_then(|until| DateTime::parse_from_rfc3339(until).ok())
            .is_some_and(|until| until.with_timezone(&Utc) > Utc::now())
}

fn validate_snooze_until(state: &str, snoozed_until: Option<&str>) -> Result<Option<String>> {
    if state == "snoozed" {
        let until = snoozed_until
            .ok_or_else(|| anyhow!("mail state snoozed requires --until <RFC3339 timestamp>"))?;
        let parsed = DateTime::parse_from_rfc3339(until)
            .with_context(|| format!("invalid snooze until timestamp: {until}"))?
            .with_timezone(&Utc);
        if parsed <= Utc::now() {
            return Err(anyhow!(
                "snooze until timestamp must be in the future: {until}"
            ));
        }
        Ok(Some(parsed.to_rfc3339_opts(SecondsFormat::Secs, true)))
    } else {
        Ok(None)
    }
}

fn validate_mail_mark_state(state: &str) -> Result<()> {
    if !mail_delivery_state_is_allowed(state) {
        return Err(anyhow!(
            "invalid mail state {state}; expected unread, read, done, snoozed, or archived"
        ));
    }
    if state == "claimed" {
        return Err(anyhow!(
            "invalid mail mark state claimed; use wiki.mail.claim or wiki.agent.claim so the claim records an agent_id"
        ));
    }
    Ok(())
}

fn validate_positive_ttl_seconds(label: &str, ttl_seconds: i64) -> Result<()> {
    if ttl_seconds <= 0 {
        return Err(anyhow!(
            "invalid {label} ttl {ttl_seconds}; expected a positive number of seconds"
        ));
    }
    Ok(())
}

fn count_mail_mark_all_delivery(counts: &mut MailMarkAllCounts, delivery: &DeliveryRecord) {
    counts.delivery_count += 1;
    if delivery.state == "unread" {
        counts.unread_count += 1;
    }
    if mail_delivery_state_is_terminal(delivery.state.as_str()) {
        counts.terminal_count += 1;
    } else if mail_delivery_is_actionable(delivery) {
        counts.open_delivery_count += 1;
    }
}

fn mail_subscription_relation_is_allowed(relation: &str) -> bool {
    matches!(relation, "watcher" | "member" | "assignee" | "subscriber")
}

fn validate_kind(kind: &str) -> Result<()> {
    if !kind.is_empty() && kind.trim() == kind && !kind.chars().any(char::is_whitespace) {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid kind {kind}; expected a non-empty token with no whitespace"
        ))
    }
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

fn normalize_subscription_kinds(kinds: Vec<String>) -> Result<Vec<String>> {
    let mut normalized = BTreeSet::new();
    for kind in kinds {
        validate_kind(&kind)?;
        normalized.insert(kind);
    }
    Ok(normalized.into_iter().collect())
}

fn subscription_kinds_equal(left: &[String], right: &[String]) -> bool {
    left.iter().collect::<BTreeSet<_>>() == right.iter().collect::<BTreeSet<_>>()
}

fn validate_address(address: &str) -> Result<()> {
    if address
        .strip_prefix("agent://")
        .is_some_and(|suffix| suffix.starts_with("agent_"))
    {
        return Err(anyhow!(
            "invalid agent address {address}; agent ids are not mail addresses. Use the agent primary_address or addresses[0] value, such as agent://codex/<thread-id>"
        ));
    }
    let valid_prefixes = ["agent://", "role://", "list://", "mailbox://"];
    let has_valid_prefix = valid_prefixes
        .iter()
        .any(|prefix| address.starts_with(prefix) && address.len() > prefix.len());
    if address.trim() == address && has_valid_prefix && !address.chars().any(char::is_whitespace) {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid address {address}; expected agent://, role://, list://, or mailbox:// address"
        ))
    }
}

fn talk_file_thread_id(
    record: &PageRecord,
    fields: &BTreeMap<String, String>,
    body: &str,
    message_id: &str,
) -> String {
    if let Some(thread_id) = fields
        .get("thread")
        .filter(|thread_id| talk_target_id_is_valid(thread_id))
    {
        return thread_id.clone();
    }

    format!(
        "thread_{}_{}",
        record.id,
        slugify(&talk_file_subject(fields, body, message_id)).if_empty("message")
    )
}

fn talk_file_subject(fields: &BTreeMap<String, String>, body: &str, message_id: &str) -> String {
    fields
        .get("subject")
        .map(|subject| subject.trim().to_string())
        .filter(|subject| !subject.is_empty())
        .or_else(|| first_markdown_heading(body))
        .unwrap_or_else(|| message_id.to_string())
}

fn first_markdown_heading(markdown: &str) -> Option<String> {
    markdown.lines().find_map(|line| {
        let trimmed = line.trim_start();
        let heading = trimmed.strip_prefix('#')?;
        let title = heading.trim_start_matches('#').trim();
        if title.is_empty() {
            None
        } else {
            Some(title.to_string())
        }
    })
}

fn talk_target_id_is_valid(value: &str) -> bool {
    value.trim() == value && !value.is_empty() && !value.chars().any(char::is_whitespace)
}

fn validate_talk_target_id<'a>(value: &'a str, flag: &str) -> Result<&'a str> {
    if talk_target_id_is_valid(value) {
        Ok(value)
    } else {
        Err(anyhow!(
            "invalid talk target {value}; expected {flag} with a message or thread id"
        ))
    }
}

fn agent_lease_is_active(agent: &AgentRecord) -> bool {
    DateTime::parse_from_rfc3339(&agent.lease_expires_at)
        .map(|expires_at| expires_at > Utc::now())
        .unwrap_or(true)
}

fn mail_subscription_lease_is_active(subscription: &MailSubscriptionRecord) -> bool {
    subscription.state == "active"
        && DateTime::parse_from_rfc3339(&subscription.lease_expires_at)
            .map(|expires_at| expires_at > Utc::now())
            .unwrap_or(true)
}

fn validate_list_address(address: &str) -> Result<()> {
    validate_address(address)?;
    if address.starts_with("list://") {
        Ok(())
    } else {
        Err(anyhow!(
            "invalid list address {address}; expected list:// address"
        ))
    }
}

fn page_role_address(page_id: &str, role: &str) -> Result<String> {
    let address = if role.starts_with("role://") {
        let role_name = role.trim_start_matches("role://");
        if !role_name.contains(['.', '/'])
            && role_name.trim() == role_name
            && !role_name.is_empty()
            && !role_name.chars().any(char::is_whitespace)
        {
            format!("role://{page_id}.{role_name}")
        } else {
            role.to_string()
        }
    } else if role.trim() == role && !role.is_empty() && !role.chars().any(char::is_whitespace) {
        format!("role://{page_id}.{role}")
    } else {
        return Err(anyhow!(
            "invalid page role {role}; expected a role token or role://{page_id}.<role>"
        ));
    };
    validate_address(&address)?;
    if page_subscription_is_related(page_id, &format!("mailbox://page/{page_id}"), &address) {
        Ok(address)
    } else {
        Err(anyhow!(
            "invalid page role {address}; expected role://{page_id}.<role> or role://{page_id}/<role>"
        ))
    }
}

fn list_name(address: &str) -> String {
    address
        .trim_start_matches("list://")
        .replace(['/', '.', '_'], "-")
}

#[derive(Clone, Debug)]
struct AgentLiveness {
    state: String,
    lease_expires_at: Option<String>,
    retired_at: Option<String>,
    retire_reason: Option<String>,
}

#[derive(Clone, Debug)]
struct AgentDirectoryEntry {
    record: AgentRecord,
    liveness: AgentLiveness,
    registered_at: Option<String>,
}

#[derive(Clone, Debug, Default)]
struct MailKindFilter {
    all: bool,
    kinds: BTreeSet<String>,
}

impl MailKindFilter {
    fn from_kinds(kinds: &[String]) -> Self {
        let mut filter = Self::default();
        filter.add(kinds);
        filter
    }

    fn add(&mut self, kinds: &[String]) {
        if self.all {
            return;
        }
        if kinds.is_empty() {
            self.all = true;
            self.kinds.clear();
        } else {
            self.kinds.extend(kinds.iter().cloned());
        }
    }

    fn accepts(&self, kind: &str) -> bool {
        self.all || self.kinds.contains(kind)
    }
}

fn subscription_accepts_kind(subscription: &MailSubscriptionRecord, kind: &str) -> bool {
    MailKindFilter::from_kinds(&subscription.kinds).accepts(kind)
}

fn mail_kind_filters_by_address(
    subscriptions: &[MailSubscriptionRecord],
) -> BTreeMap<String, MailKindFilter> {
    let mut filters = BTreeMap::<String, MailKindFilter>::new();
    for subscription in subscriptions {
        filters
            .entry(subscription.address.clone())
            .or_default()
            .add(&subscription.kinds);
    }
    filters
}

fn enrich_mail_subscription(
    record: MailSubscriptionRecord,
    agent_liveness: &BTreeMap<String, AgentLiveness>,
) -> MailSubscriptionSummary {
    let liveness = agent_liveness
        .get(&record.agent_id)
        .cloned()
        .unwrap_or_else(|| AgentLiveness {
            state: "unknown".to_string(),
            lease_expires_at: None,
            retired_at: None,
            retire_reason: None,
        });
    MailSubscriptionSummary {
        schema_version: record.schema_version,
        event: record.event,
        subscription_id: record.subscription_id,
        agent_id: record.agent_id,
        subscriber: record.subscriber,
        address: record.address,
        relation: record.relation,
        kinds: record.kinds,
        state: record.state,
        created_at: record.created_at,
        lease_expires_at: record.lease_expires_at,
        agent_liveness: liveness.state,
        agent_lease_expires_at: liveness.lease_expires_at,
        agent_retired_at: liveness.retired_at,
        agent_retire_reason: liveness.retire_reason,
    }
}

fn agent_inbox_threads(
    messages: &[DeliveryRecord],
    notifications: &[NotificationRecord],
    agent_id: &str,
) -> Vec<AgentInboxThreadSummary> {
    let mut notification_counts = BTreeMap::<String, usize>::new();
    for notification in notifications {
        *notification_counts
            .entry(notification.thread_id.clone())
            .or_insert(0) += 1;
    }
    let mut threads = BTreeMap::<String, AgentInboxThreadSummary>::new();
    let mut thread_attachment_keys = BTreeMap::<String, BTreeSet<String>>::new();
    for message in messages {
        let attachment_count = {
            let keys = thread_attachment_keys
                .entry(message.thread_id.clone())
                .or_default();
            for attachment in &message.attachments {
                keys.insert(format!("{}:{}", message.message_id, attachment.path));
            }
            keys.len()
        };
        let thread =
            threads
                .entry(message.thread_id.clone())
                .or_insert_with(|| AgentInboxThreadSummary {
                    thread_id: message.thread_id.clone(),
                    message_id: message.message_id.clone(),
                    page_id: message.page_id.clone(),
                    route: message.route.clone(),
                    kind: message.kind.clone(),
                    subject: message.subject.clone(),
                    excerpt: message.excerpt.clone(),
                    created_at: message.created_at.clone(),
                    delivery_count: 0,
                    actionable_delivery_count: 0,
                    claimable_delivery_count: 0,
                    unread_delivery_count: 0,
                    notification_count: 0,
                    attachment_count: 0,
                    recipients: Vec::new(),
                });
        thread.attachment_count = attachment_count;
        thread.delivery_count += 1;
        if mail_delivery_is_actionable(message) {
            thread.actionable_delivery_count += 1;
        }
        if mail_delivery_is_claimable_by(message, agent_id) {
            thread.claimable_delivery_count += 1;
        }
        if message.state == "unread" {
            thread.unread_delivery_count += 1;
        }
        thread.recipients.push(DeliveryState {
            recipient: message.recipient.clone(),
            state: message.state.clone(),
        });
    }
    let mut threads = threads
        .into_iter()
        .map(|(thread_id, mut thread)| {
            thread.notification_count = notification_counts.get(&thread_id).copied().unwrap_or(0);
            thread
        })
        .collect::<Vec<_>>();
    threads.sort_by(|a, b| {
        b.created_at
            .cmp(&a.created_at)
            .then_with(|| a.thread_id.cmp(&b.thread_id))
    });
    threads
}

fn page_subscription_summary(
    record: MailSubscriptionRecord,
    agent_liveness: &BTreeMap<String, AgentLiveness>,
) -> PageSubscriptionSummary {
    let summary = enrich_mail_subscription(record, agent_liveness);
    PageSubscriptionSummary {
        subscription_id: summary.subscription_id,
        agent_id: summary.agent_id,
        subscriber: summary.subscriber,
        address: summary.address,
        relation: summary.relation,
        kinds: summary.kinds,
        lease_expires_at: summary.lease_expires_at,
        agent_liveness: summary.agent_liveness,
        agent_lease_expires_at: summary.agent_lease_expires_at,
        agent_retired_at: summary.agent_retired_at,
        agent_retire_reason: summary.agent_retire_reason,
    }
}

fn agent_liveness_sort_key(liveness: &str) -> u8 {
    match liveness {
        "active" => 0,
        "stale" => 1,
        "retired" => 2,
        _ => 3,
    }
}

fn agent_directory_counts(agents: &[AgentDirectorySummary]) -> AgentDirectoryCounts {
    let mut counts = AgentDirectoryCounts::default();
    for agent in agents {
        match agent.liveness.as_str() {
            "active" => counts.active_count += 1,
            "stale" => counts.stale_count += 1,
            "retired" => counts.retired_count += 1,
            _ => {}
        }
    }
    counts.total_count = agents.len();
    counts
}

fn subscription_liveness_counts<'a>(
    agents: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> SubscriptionLivenessCounts {
    let mut unique = BTreeMap::new();
    for (agent_id, liveness) in agents {
        unique.insert(agent_id.to_string(), liveness.to_string());
    }
    let mut counts = SubscriptionLivenessCounts::default();
    for liveness in unique.values() {
        match liveness.as_str() {
            "active" => counts.active_agent_count += 1,
            "stale" => counts.stale_agent_count += 1,
            "retired" => counts.retired_agent_count += 1,
            _ => counts.unknown_agent_count += 1,
        }
    }
    counts.inactive_agent_count =
        counts.stale_agent_count + counts.retired_agent_count + counts.unknown_agent_count;
    counts
}

fn mail_subscription_summary_liveness_counts(
    subscriptions: &[MailSubscriptionSummary],
) -> SubscriptionLivenessCounts {
    subscription_liveness_counts(subscriptions.iter().map(|subscription| {
        (
            subscription.agent_id.as_str(),
            subscription.agent_liveness.as_str(),
        )
    }))
}

fn mail_list_summary(
    record: MailListRecord,
    member_counts: &BTreeMap<String, SubscriptionLivenessCounts>,
) -> MailListSummary {
    let counts = member_counts
        .get(&record.address)
        .cloned()
        .unwrap_or_default();
    let member_count = counts.active_agent_count
        + counts.stale_agent_count
        + counts.retired_agent_count
        + counts.unknown_agent_count;
    MailListSummary {
        address: record.address,
        title: record.title,
        description: record.description,
        page_id: record.page_id,
        owner: record.owner,
        state: record.state,
        created_at: record.created_at,
        member_count,
        active_member_count: counts.active_agent_count,
        inactive_member_count: counts.inactive_agent_count,
        subscription_liveness_counts: counts,
    }
}

fn page_subscription_is_related(page_id: &str, page_mailbox: &str, address: &str) -> bool {
    address == page_mailbox
        || address.starts_with(&format!("role://{page_id}."))
        || address.starts_with(&format!("role://{page_id}/"))
        || address == format!("list://{page_id}")
        || address.starts_with(&format!("list://{page_id}."))
        || address.starts_with(&format!("list://{page_id}/"))
        || address.starts_with(&format!("list://page/{page_id}"))
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

fn agent_id_for_thread(thread_id: &str) -> String {
    format!(
        "agent_codex_{}",
        sha256_bytes(thread_id.as_bytes())
            .chars()
            .take(16)
            .collect::<String>()
    )
}

fn agent_primary_address(thread_id: &str) -> String {
    format!("agent://codex/{thread_id}")
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

fn address_key(address: &str) -> String {
    slugify(address).if_empty("mailbox")
}

fn mailbox_surface(address: &str) -> String {
    if address.starts_with("mailbox://page/") {
        "page_thread".to_string()
    } else if address.starts_with("role://") || address.starts_with("agent://") {
        "agent_action_queue".to_string()
    } else {
        "mailbox".to_string()
    }
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

fn excerpt(markdown: &str) -> String {
    markdown
        .split_whitespace()
        .take(32)
        .collect::<Vec<_>>()
        .join(" ")
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
        Some("txt") => "text/plain",
        Some("csv") => "text/csv",
        Some("pdf") => "application/pdf",
        _ => "application/octet-stream",
    }
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
    fn page_create_inventory_and_mail_loop_work_from_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        let before = core.page_status("topics").unwrap();
        assert_eq!(before.state, "source_missing");

        let receipt = core
            .create_page(
                "topics",
                Some(Actor {
                    kind: "test".to_string(),
                    name: Some("unit".to_string()),
                }),
            )
            .unwrap();
        assert_eq!(receipt.status, "ok");
        assert!(root
            .join("user-wiki/source/families/reference/topics/source/topics.md")
            .exists());
        assert!(root
            .join("user-wiki/source/families/reference/topics/talk/topics.talk/_curator.md")
            .exists());

        let thread_id = "019e3f72-3471-7da1-92a8-56e5d25aaa01";
        let agent = core
            .register_agent(
                thread_id,
                vec!["role://topics.curator".to_string()],
                vec!["wiki.mail".to_string()],
                1800,
            )
            .unwrap();
        assert_eq!(agent.status, "registered");
        let duplicate_error = core
            .register_agent(thread_id, Vec::new(), Vec::new(), 1800)
            .unwrap_err()
            .to_string();
        assert!(duplicate_error.contains("agent already registered"));
        let identified = core
            .identify_agent(
                thread_id,
                Vec::new(),
                vec!["wiki.curator.apply".to_string()],
                1800,
            )
            .unwrap();
        assert_eq!(identified.status, "identified");
        assert!(identified
            .agent
            .as_ref()
            .unwrap()
            .roles
            .contains(&"role://topics.curator".to_string()));
        assert!(identified
            .agent
            .as_ref()
            .unwrap()
            .capabilities
            .contains(&"wiki.mail".to_string()));
        assert!(identified
            .agent
            .as_ref()
            .unwrap()
            .capabilities
            .contains(&"wiki.curator.apply".to_string()));
        let whoami = core.agent_whoami(Some(thread_id), None).unwrap();
        assert_eq!(whoami.status, "ok");
        assert_eq!(whoami.matches[0].agent_id, agent.agent_id);

        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Add Rust core topic".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.addresses[0].clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Rust core should own wiki semantics.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();
        assert_eq!(talk.deliveries.len(), 1);
        assert_eq!(talk.notifications.len(), 1);

        let inbox = core.inbox("role://topics.curator").unwrap();
        assert_eq!(inbox.mailbox.unread_count, 1);
        assert_eq!(inbox.messages[0].subject, "Add Rust core topic");

        let notifications = core.poll_notifications(&agent.agent_id).unwrap();
        assert_eq!(notifications.notifications.len(), 1);
        core.ack_notification(
            &agent.agent_id,
            &notifications.notifications[0].notification_id,
            "delivered",
        )
        .unwrap();
        let notifications_after_ack = core.poll_notifications(&agent.agent_id).unwrap();
        assert_eq!(notifications_after_ack.notifications.len(), 0);

        let snoozed_until = (Utc::now() + chrono::Duration::seconds(300))
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        core.mark_mail(
            &talk.message_id,
            "role://topics.curator",
            "snoozed",
            Some(&snoozed_until),
        )
        .unwrap();
        let snoozed_inbox = core.inbox("role://topics.curator").unwrap();
        assert_eq!(snoozed_inbox.messages.len(), 0);
        let snoozed_audit = core
            .inbox_with_options("role://topics.curator", false, true)
            .unwrap();
        assert_eq!(
            snoozed_audit.messages[0].snoozed_until.as_deref(),
            Some(snoozed_until.as_str())
        );

        core.claim_mail(&talk.message_id, "role://topics.curator", &agent.agent_id)
            .unwrap();
        let inbox = core.inbox("role://topics.curator").unwrap();
        assert_eq!(inbox.mailbox.unread_count, 0);

        let agent_status = core.agent_status(&agent.agent_id).unwrap();
        assert_eq!(agent_status.status, "ok");
        assert_eq!(agent_status.agent.as_ref().unwrap().liveness, "active");
        assert_eq!(agent_status.agent.as_ref().unwrap().actionable_count, 1);
        let agent_list = core.agent_list(false, false).unwrap();
        assert_eq!(agent_list.counts.active_count, 1);

        let publish = core.publish_status().unwrap();
        assert!(publish.render_required);
        assert_eq!(publish.next_action, "publish");

        core.retire_agent(&agent.agent_id, "unit done").unwrap();
        let retired_error = core
            .register_agent(thread_id, Vec::new(), Vec::new(), 1800)
            .unwrap_err()
            .to_string();
        assert!(retired_error.contains("agent explicitly retired"));
        let retired_poll_error = core
            .poll_notifications(&agent.agent_id)
            .unwrap_err()
            .to_string();
        assert!(retired_poll_error.contains("agent explicitly retired"));
        let retired_heartbeat_error = core
            .heartbeat_agent(&agent.agent_id, 1800)
            .unwrap_err()
            .to_string();
        assert!(retired_heartbeat_error.contains("agent explicitly retired"));
    }

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
        let agent = core
            .register_agent(
                "same-subject-burst-thread",
                vec!["role://topics.curator".to_string()],
                Vec::new(),
                1800,
            )
            .unwrap();

        let first = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Same subject burst".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.addresses[0].clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "First message.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();
        let second = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Same subject burst".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.addresses[0].clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Second message.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();
        let explicit_reply = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Changed subject should stay in parent thread".to_string(),
                thread_id: None,
                reply_to: Some(first.message_id.clone()),
                from: agent.addresses[0].clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Explicit parent reply.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();
        let explicit_thread = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Explicit thread target".to_string(),
                thread_id: Some(first.thread_id.clone()),
                reply_to: None,
                from: agent.addresses[0].clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Explicit thread id reply.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
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
        let thread = core.read_mail(None, Some(&first.thread_id)).unwrap();
        assert_eq!(thread.messages.len(), 4);
        let explicit_reply_read = thread
            .messages
            .iter()
            .find(|message| message.message_id == explicit_reply.message_id)
            .unwrap();
        assert_eq!(
            explicit_reply_read.reply_to.as_deref(),
            Some(first.message_id.as_str())
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
        let agent = core
            .register_agent(
                "explicit-new-talk-thread",
                vec!["role://topics.curator".to_string()],
                Vec::new(),
                1800,
            )
            .unwrap();
        let thread_id = "agent-chosen-thread-123";

        let started = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Agent chosen thread".to_string(),
                thread_id: Some(thread_id.to_string()),
                reply_to: None,
                from: agent.addresses[0].clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "A sender can choose a new correlation id.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        assert_eq!(started.thread_id, thread_id);
        assert_eq!(started.reply_to, None);
        let read = core.read_mail(None, Some(thread_id)).unwrap();
        assert_eq!(read.message_count, 1);
        assert_eq!(read.messages[0].message_id, started.message_id);
    }

    #[test]
    fn file_only_talk_messages_can_be_reply_and_thread_targets() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent(
                "file-only-talk-target-thread",
                vec!["role://topics.curator".to_string()],
                Vec::new(),
                1800,
            )
            .unwrap();
        let talk_dir = root.join("user-wiki/source/families/reference/topics/talk/topics.talk");
        let parent_id = "talkmsg_file_only_parent";
        let parent_thread = "thread_topics_file_only_parent";
        fs::create_dir_all(talk_dir.join("attachments").join(parent_id)).unwrap();
        fs::write(
            talk_dir
                .join("attachments")
                .join(parent_id)
                .join("legacy-note.txt"),
            "legacy attachment",
        )
        .unwrap();
        fs::write(
            talk_dir.join("2026-05-20T00-00-00-000000000Z.proposal.file-only-parent.md"),
            format!(
                "---\nid: \"{parent_id}\"\nkind: \"proposal\"\nauthor: \"agent://codex/legacy\"\ncreated: \"2026-05-20T00:00:00Z\"\ntalk_for: \"page://topics\"\nthread: \"{parent_thread}\"\nsubject: \"File only parent\"\nstate: open\nrecipients:\n  - \"role://topics.curator\"\nattachments:\n  - filename: \"legacy-note.txt\"\n    media_type: \"text/plain\"\n    path: \"attachments/{parent_id}/legacy-note.txt\"\n    handle: \"user-wiki://page/topics/talk/attachments/{parent_id}/legacy-note.txt\"\n    caption: \"Legacy note\"\n    alt_text: \"Legacy note alt\"\n---\n\n## File only parent\n\nParent message with no delivery rows.\n"
            ),
        )
        .unwrap();
        let direct_thread = "thread_topics_file_only_direct";
        fs::write(
            talk_dir.join("2026-05-20T00-01-00-000000000Z.question.file-only-direct.md"),
            format!(
                "---\nid: \"talkmsg_file_only_direct\"\nkind: \"question\"\nauthor: \"agent://codex/legacy\"\ncreated: \"2026-05-20T00:01:00Z\"\ntalk_for: \"page://topics\"\nthread: \"{direct_thread}\"\nsubject: \"File only direct\"\nstate: open\nrecipients:\n  - \"role://topics.curator\"\nattachments: []\n---\n\n## File only direct\n\nThread target message with no delivery rows.\n"
            ),
        )
        .unwrap();
        assert!(core
            .all_mailbox_messages()
            .unwrap()
            .iter()
            .all(
                |delivery| delivery.message_id != parent_id && delivery.thread_id != direct_thread
            ));

        let reply = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Changed subject file-only reply".to_string(),
                thread_id: None,
                reply_to: Some(parent_id.to_string()),
                from: agent.addresses[0].clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Reply should inherit the file-only parent thread.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();
        let direct = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "reply".to_string(),
                subject: "Direct file-only thread target".to_string(),
                thread_id: Some(direct_thread.to_string()),
                reply_to: None,
                from: agent.addresses[0].clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Explicit thread target should resolve from talk files.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        assert_eq!(reply.thread_id, parent_thread);
        assert_eq!(reply.reply_to.as_deref(), Some(parent_id));
        assert_eq!(direct.thread_id, direct_thread);
        assert_eq!(direct.reply_to, None);

        let parent_thread_read = core.read_mail(None, Some(parent_thread)).unwrap();
        assert_eq!(parent_thread_read.message_count, 2);
        assert_eq!(parent_thread_read.delivery_count, 1);
        let hydrated_parent = parent_thread_read
            .messages
            .iter()
            .find(|message| message.message_id == parent_id)
            .unwrap();
        assert_eq!(hydrated_parent.thread_id, parent_thread);
        assert_eq!(hydrated_parent.subject, "File only parent");
        assert_eq!(hydrated_parent.attachment_count, 1);
        assert_eq!(hydrated_parent.attachments.len(), 1);
        assert_eq!(hydrated_parent.attachments[0].filename, "legacy-note.txt");
        assert_eq!(hydrated_parent.attachments[0].media_type, "text/plain");
        assert_eq!(
            hydrated_parent.attachments[0].path,
            format!("attachments/{parent_id}/legacy-note.txt")
        );
        assert_eq!(
            hydrated_parent.attachments[0].handle,
            format!("user-wiki://page/topics/talk/attachments/{parent_id}/legacy-note.txt")
        );
        assert_eq!(
            hydrated_parent.attachments[0].caption.as_deref(),
            Some("Legacy note")
        );
        assert_eq!(
            hydrated_parent.attachments[0].alt_text.as_deref(),
            Some("Legacy note alt")
        );
        assert!(hydrated_parent.deliveries.is_empty());
        assert!(parent_thread_read
            .messages
            .iter()
            .any(|message| message.message_id == reply.message_id));

        let parent_message_read = core.read_mail(Some(parent_id), None).unwrap();
        assert_eq!(parent_message_read.message_count, 1);
        assert_eq!(parent_message_read.delivery_count, 0);
        assert_eq!(parent_message_read.messages[0].attachment_count, 1);

        let direct_thread_read = core.read_mail(None, Some(direct_thread)).unwrap();
        assert_eq!(direct_thread_read.message_count, 2);
        assert!(direct_thread_read
            .messages
            .iter()
            .any(|message| message.message_id == "talkmsg_file_only_direct"));
        assert!(direct_thread_read
            .messages
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
        assert_eq!(status.mail.open_delivery_count, 0);
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
    fn page_assets_are_agent_addable_listable_and_embed_ready() {
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
        assert_eq!(added.asset.media_type, "image/png");
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
    fn page_status_liveness_counts_include_page_linked_list_members() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let active = core
            .register_agent("page-status-active", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let stale = core
            .register_agent("page-status-stale", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let retired = core
            .register_agent("page-status-retired", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let list = "list://wiki.reviewers";
        core.create_list(
            list,
            Some("Wiki Reviewers".to_string()),
            None,
            Some("topics".to_string()),
            None,
        )
        .unwrap();
        for agent in [&active, &stale, &retired] {
            core.subscribe_mail(&agent.agent_id, list, "member", Vec::new(), 1800)
                .unwrap();
        }

        let mut stale_record = core
            .current_agents_with_stale(true)
            .unwrap()
            .remove(&stale.agent_id)
            .unwrap();
        stale_record.event = "agent.heartbeat".to_string();
        stale_record.at = now_rfc3339();
        stale_record.lease_expires_at =
            (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339_opts(SecondsFormat::Secs, true);
        append_jsonl(&core.agent_events_path(), &stale_record).unwrap();

        core.retire_agent(&retired.agent_id, "unit done").unwrap();

        let status = core.page_status("topics").unwrap();
        assert_eq!(status.mail.watcher_count, 0);
        assert_eq!(
            status.mail.subscription_liveness_counts.active_agent_count,
            1
        );
        assert_eq!(
            status.mail.subscription_liveness_counts.stale_agent_count,
            1
        );
        assert_eq!(
            status.mail.subscription_liveness_counts.retired_agent_count,
            1
        );
        assert_eq!(
            status
                .mail
                .subscription_liveness_counts
                .inactive_agent_count,
            2
        );
    }

    #[test]
    fn stale_agent_can_be_explicitly_retired() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        let agent = core
            .register_agent("stale-retire-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let mut stale_record = core
            .current_agents_with_stale(true)
            .unwrap()
            .remove(&agent.agent_id)
            .unwrap();
        stale_record.event = "agent.heartbeat".to_string();
        stale_record.at = now_rfc3339();
        stale_record.lease_expires_at =
            (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339_opts(SecondsFormat::Secs, true);
        append_jsonl(&core.agent_events_path(), &stale_record).unwrap();

        let stale_status = core.agent_status(&agent.agent_id).unwrap();
        assert_eq!(stale_status.agent.as_ref().unwrap().liveness, "stale");

        let retired = core.retire_agent(&agent.agent_id, "stale cleanup").unwrap();
        assert_eq!(retired.status, "retired");
        let whoami = core.agent_whoami(Some("stale-retire-agent"), None).unwrap();
        assert_eq!(whoami.matches[0].liveness, "retired");
        assert_eq!(whoami.next_action, "agent_register_new_thread");

        let active_error = core
            .poll_notifications(&agent.agent_id)
            .unwrap_err()
            .to_string();
        assert!(active_error.contains("agent explicitly retired"));
    }

    #[test]
    fn stale_agent_control_commands_require_identify_refresh() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        let thread_id = "stale-refresh-agent";
        let agent = core
            .register_agent(thread_id, Vec::new(), Vec::new(), 1800)
            .unwrap();
        let mut stale_record = core
            .current_agents_with_stale(true)
            .unwrap()
            .remove(&agent.agent_id)
            .unwrap();
        stale_record.event = "agent.heartbeat".to_string();
        stale_record.at = now_rfc3339();
        stale_record.lease_expires_at =
            (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339_opts(SecondsFormat::Secs, true);
        append_jsonl(&core.agent_events_path(), &stale_record).unwrap();

        let stale_status = core.agent_status(&agent.agent_id).unwrap();
        assert_eq!(stale_status.agent.as_ref().unwrap().liveness, "stale");
        assert_eq!(stale_status.next_action, "agent_identify");

        for error in [
            core.agent_inbox(&agent.agent_id, false, false)
                .unwrap_err()
                .to_string(),
            core.poll_notifications(&agent.agent_id)
                .unwrap_err()
                .to_string(),
            core.heartbeat_agent(&agent.agent_id, 1800)
                .unwrap_err()
                .to_string(),
        ] {
            assert!(error.contains("agent lease expired"));
            assert!(error.contains("call agent-identify"));
        }

        let refreshed = core
            .identify_agent(thread_id, Vec::new(), Vec::new(), 1800)
            .unwrap();
        assert_eq!(refreshed.status, "refreshed");
        assert_eq!(refreshed.liveness_before.as_deref(), Some("stale"));
        assert_eq!(refreshed.liveness_after, "active");
        core.agent_inbox(&agent.agent_id, false, false).unwrap();
    }

    #[test]
    fn agent_lifecycle_rejects_non_positive_ttls_in_core() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);

        let register_error = core
            .register_agent("bad-register-ttl", Vec::new(), Vec::new(), 0)
            .unwrap_err()
            .to_string();
        assert!(register_error.contains("invalid agent ttl 0"));

        let identify_error = core
            .identify_agent("bad-identify-ttl", Vec::new(), Vec::new(), -1)
            .unwrap_err()
            .to_string();
        assert!(identify_error.contains("invalid agent ttl -1"));
    }

    #[test]
    fn mail_lists_canonicalize_page_route_references_in_core() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();

        let route_list = core
            .create_list(
                "list://topics.route-review",
                Some("Topics Route Review".to_string()),
                None,
                Some("/topics".to_string()),
                None,
            )
            .unwrap();
        assert_eq!(route_list.list.page_id.as_deref(), Some("topics"));

        let id_list = core
            .create_list(
                "list://topics.id-review",
                Some("Topics Id Review".to_string()),
                None,
                Some("topics".to_string()),
                None,
            )
            .unwrap();
        assert_eq!(id_list.list.page_id.as_deref(), Some("topics"));

        let route_created_by_id = core
            .mail_lists(Some("topics"), Some("list://topics.route-review"))
            .unwrap();
        assert_eq!(route_created_by_id.list_count, 1);
        assert_eq!(
            route_created_by_id.lists[0].page_id.as_deref(),
            Some("topics")
        );

        let id_created_by_route = core
            .mail_lists(Some("/topics"), Some("list://topics.id-review"))
            .unwrap();
        assert_eq!(id_created_by_route.list_count, 1);
        assert_eq!(
            id_created_by_route.lists[0].page_id.as_deref(),
            Some("topics")
        );
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
    fn mail_list_summaries_expose_created_at() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("list-created-at-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let address = "list://topics.created-at";
        let created = core
            .create_list(
                address,
                Some("Topics Created At".to_string()),
                None,
                Some("/topics".to_string()),
                Some(agent.agent_id.clone()),
            )
            .unwrap();
        let created_at = created.list.created_at.clone();
        assert!(!created_at.is_empty());

        let existing = core
            .create_list(
                address,
                Some("Ignored Title".to_string()),
                None,
                Some("topics".to_string()),
                None,
            )
            .unwrap();
        assert_eq!(existing.status, "already_exists");
        assert_eq!(existing.list.created_at, created_at);

        let listed = core.mail_lists(Some("topics"), Some(address)).unwrap();
        assert_eq!(listed.lists[0].created_at, created_at);

        let members = core.list_members(address).unwrap();
        assert_eq!(members.list.unwrap().created_at, created_at);

        let status = core.list_status(address).unwrap();
        assert_eq!(status.list.unwrap().created_at, created_at);
    }

    #[test]
    fn page_recipient_alias_normalizes_across_talk_and_mail() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent(
                "probe-b-addressing-agent",
                Vec::new(),
                vec!["probe-b".to_string()],
                1800,
            )
            .unwrap();

        let subscription = core
            .subscribe_mail(
                &agent.agent_id,
                "page://topics",
                "watcher",
                Vec::new(),
                1800,
            )
            .unwrap();
        assert_eq!(subscription.address, "mailbox://page/topics");
        let subscriptions = core
            .mail_subscriptions(None, Some("page://topics"))
            .unwrap();
        assert_eq!(subscriptions.subscription_count, 1);
        assert_eq!(
            subscriptions.subscriptions[0].address,
            "mailbox://page/topics"
        );

        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "review".to_string(),
                subject: "Page alias recipient".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.addresses[0].clone(),
                to: vec!["page://topics".to_string()],
                cc: vec![],
                body_markdown: "Page aliases should behave like page mailboxes.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();
        assert_eq!(talk.deliveries[0].recipient, "mailbox://page/topics");

        let inbox = core.inbox("page://topics").unwrap();
        assert_eq!(inbox.mailbox.address, "mailbox://page/topics");
        assert_eq!(inbox.message_count, 1);
        assert_eq!(inbox.messages[0].message_id, talk.message_id);

        let claim = core
            .claim_mail(&talk.message_id, "page://topics", &agent.agent_id)
            .unwrap();
        assert_eq!(claim.recipient, "mailbox://page/topics");

        let mark = core
            .mark_mail(&talk.message_id, "page://topics", "done", None)
            .unwrap();
        assert_eq!(mark.recipient, "mailbox://page/topics");
        assert_eq!(mark.state, "done");
        let inbox = core
            .inbox_with_options("mailbox://page/topics", true, true)
            .unwrap();
        assert_eq!(inbox.messages[0].state, "done");
    }

    #[test]
    fn mail_subscribe_renews_duplicate_and_reports_backfill_action() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("subscribe-renew-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let list = "list://wiki.reviewers";
        core.create_list(
            list,
            Some("Wiki Reviewers".to_string()),
            None,
            Some("topics".to_string()),
            Some(agent.primary_address.clone()),
        )
        .unwrap();
        core.append_talk(TalkAppendRequest {
            page: "topics".to_string(),
            kind: "proposal".to_string(),
            subject: "Backfill before subscribe".to_string(),
            thread_id: None,
            reply_to: None,
            from: agent.primary_address.clone(),
            to: vec![list.to_string()],
            cc: vec![],
            body_markdown: "A late subscriber should know there is visible list mail.".to_string(),
            attachments: vec![],
            allow_tombstoned: false,
        })
        .unwrap();

        let first = core
            .subscribe_mail(
                &agent.agent_id,
                list,
                "member",
                vec!["review".to_string(), "proposal".to_string()],
                1800,
            )
            .unwrap();
        assert_eq!(first.status, "subscribed");
        assert_eq!(first.kinds, vec!["proposal", "review"]);
        assert_eq!(first.backfill.surfaced_message_count, 1);
        assert_eq!(first.backfill.surfaced_unread_count, 1);
        assert_eq!(first.next_action, "agent_inbox");
        let first_expires = DateTime::parse_from_rfc3339(&first.lease_expires_at).unwrap();

        let renewed = core
            .subscribe_mail(
                &agent.agent_id,
                list,
                "member",
                vec![
                    "proposal".to_string(),
                    "review".to_string(),
                    "proposal".to_string(),
                ],
                3600,
            )
            .unwrap();
        assert_eq!(renewed.status, "renewed");
        assert_eq!(renewed.subscription_id, first.subscription_id);
        assert_eq!(renewed.kinds, vec!["proposal", "review"]);
        assert_eq!(renewed.deduplicated_count, 0);
        assert_eq!(renewed.next_action, "agent_inbox");
        let renewed_expires = DateTime::parse_from_rfc3339(&renewed.lease_expires_at).unwrap();
        assert!(renewed_expires > first_expires);

        let mut duplicate = core.active_mail_subscriptions().unwrap()[0].clone();
        duplicate.subscription_id = "sub_manual_duplicate".to_string();
        duplicate.created_at = now_rfc3339();
        append_jsonl(&core.mail_subscriptions_path(), &duplicate).unwrap();
        assert_eq!(
            core.mail_subscriptions(Some(&agent.agent_id), Some(list))
                .unwrap()
                .subscription_count,
            2
        );

        let deduplicated = core
            .subscribe_mail(
                &agent.agent_id,
                list,
                "member",
                vec!["review".to_string(), "proposal".to_string()],
                3600,
            )
            .unwrap();
        assert_eq!(deduplicated.status, "renewed");
        assert_eq!(deduplicated.subscription_id, first.subscription_id);
        assert_eq!(deduplicated.deduplicated_count, 1);

        let subscriptions = core
            .mail_subscriptions(Some(&agent.agent_id), Some(list))
            .unwrap();
        assert_eq!(subscriptions.subscription_count, 1);
        assert_eq!(
            subscriptions.subscriptions[0].subscription_id,
            first.subscription_id
        );
        assert_eq!(
            subscriptions.subscriptions[0].kinds,
            vec!["proposal", "review"]
        );

        let members = core.list_members(list).unwrap();
        assert_eq!(members.member_count, 1);
        assert_eq!(members.active_member_count, 1);
        assert_eq!(members.subscriptions.len(), 1);
    }

    #[test]
    fn agent_inbox_applies_subscription_kind_filters_without_narrowing_mailbox() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("subscription-kind-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let list = "list://wiki.reviewers";
        core.create_list(
            list,
            Some("Wiki Reviewers".to_string()),
            None,
            Some("topics".to_string()),
            Some(agent.primary_address.clone()),
        )
        .unwrap();

        core.append_talk(TalkAppendRequest {
            page: "topics".to_string(),
            kind: "proposal".to_string(),
            subject: "Proposal before scoped subscribe".to_string(),
            thread_id: None,
            reply_to: None,
            from: agent.primary_address.clone(),
            to: vec![list.to_string()],
            cc: vec![],
            body_markdown: "This should stay durable in the list mailbox.".to_string(),
            attachments: vec![],
            allow_tombstoned: false,
        })
        .unwrap();

        let subscription = core
            .subscribe_mail(
                &agent.agent_id,
                list,
                "member",
                vec!["review".to_string()],
                1800,
            )
            .unwrap();
        assert_eq!(subscription.backfill.surfaced_message_count, 0);
        assert_eq!(subscription.next_action, "none");

        let before_review = core.agent_inbox(&agent.agent_id, false, false).unwrap();
        assert_eq!(before_review.message_count, 0);
        assert_eq!(
            before_review
                .mailboxes
                .iter()
                .find(|mailbox| mailbox.address == list)
                .unwrap()
                .total_count,
            0
        );

        core.append_talk(TalkAppendRequest {
            page: "topics".to_string(),
            kind: "review".to_string(),
            subject: "Review after scoped subscribe".to_string(),
            thread_id: None,
            reply_to: None,
            from: agent.primary_address.clone(),
            to: vec![list.to_string()],
            cc: vec![],
            body_markdown: "This should reach the review subscriber.".to_string(),
            attachments: vec![],
            allow_tombstoned: false,
        })
        .unwrap();

        let list_inbox = core.inbox(list).unwrap();
        assert_eq!(list_inbox.message_count, 2);
        assert_eq!(
            list_inbox
                .messages
                .iter()
                .map(|message| message.kind.as_str())
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["proposal", "review"])
        );

        let agent_inbox = core.agent_inbox(&agent.agent_id, false, false).unwrap();
        assert_eq!(agent_inbox.message_count, 1);
        assert_eq!(
            agent_inbox
                .mailboxes
                .iter()
                .find(|mailbox| mailbox.address == list)
                .unwrap()
                .total_count,
            1
        );
        assert_eq!(agent_inbox.notifications.len(), 1);
        assert_eq!(agent_inbox.messages[0].kind, "review");
        assert_eq!(
            agent_inbox.messages[0].subject,
            "Review after scoped subscribe"
        );

        let status = core.agent_status(&agent.agent_id).unwrap();
        assert_eq!(status.agent.as_ref().unwrap().actionable_count, 1);
        assert_eq!(
            status
                .mailboxes
                .iter()
                .find(|mailbox| mailbox.address == list)
                .unwrap()
                .total_count,
            1
        );
    }

    #[test]
    fn notify_poll_ignores_expired_subscription_wakeups() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("expired-subscription-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let list = "list://wiki.reviewers";
        core.create_list(
            list,
            Some("Wiki Reviewers".to_string()),
            None,
            Some("topics".to_string()),
            Some(agent.primary_address.clone()),
        )
        .unwrap();
        core.subscribe_mail(&agent.agent_id, list, "member", Vec::new(), 1800)
            .unwrap();

        core.append_talk(TalkAppendRequest {
            page: "topics".to_string(),
            kind: "proposal".to_string(),
            subject: "Proposal before subscription expiry".to_string(),
            thread_id: None,
            reply_to: None,
            from: agent.primary_address.clone(),
            to: vec![list.to_string()],
            cc: vec![],
            body_markdown: "This wakeup should disappear when the subscription expires."
                .to_string(),
            attachments: vec![],
            allow_tombstoned: false,
        })
        .unwrap();
        assert_eq!(
            core.poll_notifications(&agent.agent_id)
                .unwrap()
                .notification_count,
            1
        );

        let mut expired = core.active_mail_subscriptions().unwrap()[0].clone();
        expired.event = "mail.subscription.renewed".to_string();
        expired.created_at = now_rfc3339();
        expired.lease_expires_at =
            (Utc::now() - chrono::Duration::seconds(30)).to_rfc3339_opts(SecondsFormat::Secs, true);
        append_jsonl(&core.mail_subscriptions_path(), &expired).unwrap();

        assert_eq!(
            core.mail_subscriptions(Some(&agent.agent_id), Some(list))
                .unwrap()
                .subscription_count,
            0
        );
        assert_eq!(
            core.agent_inbox(&agent.agent_id, false, false)
                .unwrap()
                .message_count,
            0
        );
        assert_eq!(
            core.poll_notifications(&agent.agent_id)
                .unwrap()
                .notification_count,
            0
        );
    }

    #[test]
    fn mail_unsubscribe_cancels_matching_active_subscriptions() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        let agent = core
            .register_agent("unsubscribe-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        core.create_list(
            "list://wiki.reviewers",
            Some("Wiki Reviewers".to_string()),
            None,
            None,
            None,
        )
        .unwrap();

        let proposal = core
            .subscribe_mail(
                &agent.agent_id,
                "list://wiki.reviewers",
                "member",
                vec!["proposal".to_string()],
                1800,
            )
            .unwrap();
        let review = core
            .subscribe_mail(
                &agent.agent_id,
                "list://wiki.reviewers",
                "member",
                vec!["review".to_string()],
                1800,
            )
            .unwrap();
        assert_ne!(proposal.subscription_id, review.subscription_id);
        assert_eq!(
            core.mail_subscriptions(Some(&agent.agent_id), None)
                .unwrap()
                .subscription_count,
            2
        );

        let cancelled = core
            .unsubscribe_mail(
                &agent.agent_id,
                "list://wiki.reviewers",
                Some("member"),
                vec!["proposal".to_string()],
            )
            .unwrap();
        assert_eq!(cancelled.operation, "wiki.mail.unsubscribe");
        assert_eq!(cancelled.status, "unsubscribed");
        assert_eq!(cancelled.cancelled_count, 1);
        assert_eq!(cancelled.remaining_count, 1);
        assert_eq!(cancelled.next_action, "mail_subscriptions");
        assert_eq!(
            cancelled.cancelled[0].subscription_id,
            proposal.subscription_id
        );

        let subscriptions = core
            .mail_subscriptions(Some(&agent.agent_id), Some("list://wiki.reviewers"))
            .unwrap();
        assert_eq!(subscriptions.subscription_count, 1);
        assert_eq!(
            subscriptions.subscriptions[0].subscription_id,
            review.subscription_id
        );
        assert_eq!(subscriptions.subscriptions[0].kinds, vec!["review"]);

        let not_found = core
            .unsubscribe_mail(
                &agent.agent_id,
                "list://wiki.reviewers",
                Some("watcher"),
                Vec::new(),
            )
            .unwrap();
        assert_eq!(not_found.status, "not_found");
        assert_eq!(not_found.cancelled_count, 0);
        assert_eq!(not_found.remaining_count, 1);
        assert_eq!(not_found.next_action, "mail_subscriptions");

        let cancelled_rest = core
            .unsubscribe_mail(
                &agent.agent_id,
                "list://wiki.reviewers",
                Some("member"),
                Vec::new(),
            )
            .unwrap();
        assert_eq!(cancelled_rest.status, "unsubscribed");
        assert_eq!(cancelled_rest.cancelled_count, 1);
        assert_eq!(cancelled_rest.remaining_count, 0);
        assert_eq!(cancelled_rest.next_action, "none");
    }

    #[test]
    fn page_unwatch_cleans_up_page_watch_surfaces() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("page-unwatch-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();

        let watch = core
            .watch_page(
                "topics",
                &agent.agent_id,
                None,
                vec!["proposal".to_string()],
                1800,
            )
            .unwrap();
        assert_eq!(watch.operation, "wiki.page.watch");
        assert_eq!(watch.unsubscribe_plan.operation, "wiki.page.unwatch");
        assert_eq!(watch.unsubscribe_plan.page, "topics");
        assert_eq!(
            watch.unsubscribe_plan.list_address,
            "list://topics.watchers"
        );
        assert_eq!(
            watch.unsubscribe_plan.page_mailbox_address,
            "mailbox://page/topics"
        );
        assert_eq!(watch.subscription.address, "list://topics.watchers");
        assert_eq!(
            watch.page_mailbox_subscription.address,
            "mailbox://page/topics"
        );

        core.subscribe_mail(
            &agent.agent_id,
            "mailbox://page/topics",
            "watcher",
            vec!["proposal".to_string(), "question".to_string()],
            1800,
        )
        .unwrap();
        assert_eq!(
            core.mail_subscriptions(Some(&agent.agent_id), None)
                .unwrap()
                .subscription_count,
            3
        );

        let scoped = core
            .unwatch_page(
                "topics",
                &agent.agent_id,
                None,
                vec!["proposal".to_string()],
            )
            .unwrap();
        assert_eq!(scoped.operation, "wiki.page.unwatch");
        assert_eq!(scoped.status, "unwatched");
        assert_eq!(scoped.cancelled_count, 2);
        assert_eq!(scoped.remaining_count, 1);
        assert_eq!(scoped.next_action, "mail_subscriptions");
        assert_eq!(scoped.list_unsubscribe.cancelled_count, 1);
        assert_eq!(scoped.page_mailbox_unsubscribe.cancelled_count, 1);
        assert_eq!(scoped.page_mailbox_unsubscribe.remaining_count, 1);

        let remaining = core
            .mail_subscriptions(Some(&agent.agent_id), Some("mailbox://page/topics"))
            .unwrap();
        assert_eq!(remaining.subscription_count, 1);
        assert_eq!(
            remaining.subscriptions[0].kinds,
            vec!["proposal", "question"]
        );

        let broad = core
            .unwatch_page("topics", &agent.agent_id, None, Vec::new())
            .unwrap();
        assert_eq!(broad.status, "unwatched");
        assert_eq!(broad.cancelled_count, 1);
        assert_eq!(broad.remaining_count, 0);
        assert_eq!(broad.next_action, "none");
        assert_eq!(
            core.mail_subscriptions(Some(&agent.agent_id), None)
                .unwrap()
                .subscription_count,
            0
        );
        assert_eq!(
            broad
                .page_status
                .mail
                .subscriptions
                .iter()
                .filter(|subscription| subscription.agent_id == agent.agent_id)
                .count(),
            0
        );
    }

    #[test]
    fn list_status_points_at_mail_actions_for_open_list_work() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("list-status-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        core.create_list(
            "list://topics.reviewers",
            Some("Topics Reviewers".to_string()),
            None,
            Some("topics".to_string()),
            Some(agent.primary_address.clone()),
        )
        .unwrap();
        core.subscribe_mail(
            &agent.agent_id,
            "list://topics.reviewers",
            "member",
            vec!["proposal".to_string()],
            1800,
        )
        .unwrap();
        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "List status action".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.primary_address.clone(),
                to: vec!["list://topics.reviewers".to_string()],
                cc: vec![],
                body_markdown: "List status should tell agents what to do next.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        let open = core.list_status("list://topics.reviewers").unwrap();
        assert_eq!(open.mailbox.actionable_count, 1);
        assert_eq!(open.next_action, "mail_claim_or_mark");

        core.claim_mail(&talk.message_id, "list://topics.reviewers", &agent.agent_id)
            .unwrap();
        let claimed = core.list_status("list://topics.reviewers").unwrap();
        assert_eq!(claimed.mailbox.actionable_count, 1);
        assert_eq!(claimed.next_action, "mail_claim_or_mark");

        let snoozed_until = (Utc::now() + chrono::Duration::seconds(300))
            .to_rfc3339_opts(SecondsFormat::Secs, true);
        core.mark_mail(
            &talk.message_id,
            "list://topics.reviewers",
            "snoozed",
            Some(&snoozed_until),
        )
        .unwrap();
        let snoozed = core.list_status("list://topics.reviewers").unwrap();
        assert_eq!(snoozed.mailbox.actionable_count, 0);
        assert!(snoozed.has_snoozed);
        assert_eq!(snoozed.snoozed_count, 1);
        assert_eq!(snoozed.hidden_snoozed_count, 1);
        assert_eq!(snoozed.audit_flags, vec!["snoozed_hidden"]);
        assert!(snoozed.messages.is_empty());
        assert_eq!(snoozed.next_action, "include_hidden_mail");

        let snoozed_audit = core
            .list_status_with_options("list://topics.reviewers", false, true)
            .unwrap();
        assert_eq!(snoozed_audit.hidden_snoozed_count, 0);
        assert_eq!(snoozed_audit.messages.len(), 1);
        assert_eq!(snoozed_audit.messages[0].state, "snoozed");

        core.mark_mail(
            &talk.message_id,
            "list://topics.reviewers",
            "archived",
            None,
        )
        .unwrap();
        let archived = core.list_status("list://topics.reviewers").unwrap();
        assert!(archived.has_archived);
        assert_eq!(archived.archived_count, 1);
        assert_eq!(archived.hidden_archived_count, 1);
        assert_eq!(archived.audit_flags, vec!["archived_hidden"]);
        assert!(archived.messages.is_empty());
        assert_eq!(archived.next_action, "include_hidden_mail");

        let archived_audit = core
            .list_status_with_options("list://topics.reviewers", true, false)
            .unwrap();
        assert_eq!(archived_audit.hidden_archived_count, 0);
        assert_eq!(archived_audit.messages.len(), 1);
        assert_eq!(archived_audit.messages[0].state, "archived");
    }

    #[test]
    fn mail_claim_and_mark_accept_agent_primary_address_for_visible_subscription_delivery() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("primary-mail-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let list_address = "list://topics.reviewers";
        core.create_list(
            list_address,
            Some("Topics Reviewers".to_string()),
            None,
            Some("topics".to_string()),
            Some(agent.primary_address.clone()),
        )
        .unwrap();
        core.subscribe_mail(
            &agent.agent_id,
            list_address,
            "member",
            vec!["proposal".to_string()],
            1800,
        )
        .unwrap();

        let claim_talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Primary claim should resolve".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.primary_address.clone(),
                to: vec![list_address.to_string()],
                cc: vec![],
                body_markdown: "This is visible in agent-inbox through a list.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        assert_eq!(core.inbox(&agent.primary_address).unwrap().message_count, 0);
        let agent_inbox = core.agent_inbox(&agent.agent_id, false, false).unwrap();
        assert_eq!(agent_inbox.message_count, 1);
        assert_eq!(agent_inbox.messages[0].recipient, list_address);

        let claim = core
            .claim_mail(
                &claim_talk.message_id,
                &agent.primary_address,
                &agent.agent_id,
            )
            .unwrap();
        assert_eq!(claim.recipient, list_address);
        assert_eq!(claim.status, "claimed");
        let after_claim = core.inbox(list_address).unwrap();
        let claimed = after_claim
            .messages
            .iter()
            .find(|message| message.message_id == claim_talk.message_id)
            .unwrap();
        assert_eq!(claimed.claimed_by.as_deref(), Some(agent.agent_id.as_str()));

        let mark_talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Primary mark should resolve".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.primary_address.clone(),
                to: vec![list_address.to_string()],
                cc: vec![],
                body_markdown: "This should be markable from the same agent-facing inbox."
                    .to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();
        let mark = core
            .mark_mail(&mark_talk.message_id, &agent.primary_address, "done", None)
            .unwrap();
        assert_eq!(mark.recipient, list_address);
        assert_eq!(mark.status, "ok");
        let after_mark = core.inbox(list_address).unwrap();
        let marked = after_mark
            .messages
            .iter()
            .find(|message| message.message_id == mark_talk.message_id)
            .unwrap();
        assert_eq!(marked.state, "done");
    }

    #[test]
    fn concurrent_claims_have_one_winner_per_delivery() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent_a = core
            .register_agent("concurrent-claim-a", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let agent_b = core
            .register_agent("concurrent-claim-b", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let list_address = "list://topics.concurrent";
        core.create_list(
            list_address,
            Some("Topics Concurrent".to_string()),
            None,
            Some("topics".to_string()),
            Some(agent_a.primary_address.clone()),
        )
        .unwrap();
        for agent in [&agent_a, &agent_b] {
            core.subscribe_mail(
                &agent.agent_id,
                list_address,
                "member",
                vec!["proposal".to_string()],
                1800,
            )
            .unwrap();
        }

        let mut message_ids = Vec::new();
        for index in 0..20 {
            let talk = core
                .append_talk(TalkAppendRequest {
                    page: "topics".to_string(),
                    kind: "proposal".to_string(),
                    subject: format!("Concurrent claim {index}"),
                    thread_id: None,
                    reply_to: None,
                    from: agent_a.primary_address.clone(),
                    to: vec![list_address.to_string()],
                    cc: Vec::new(),
                    body_markdown: "Only one competing agent should win this delivery.".to_string(),
                    attachments: vec![],
                    allow_tombstoned: false,
                })
                .unwrap();
            let message_id = talk.message_id.clone();
            message_ids.push(message_id.clone());

            let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
            let first = {
                let barrier = barrier.clone();
                let root = root.clone();
                let message_id = message_id.clone();
                let agent_id = agent_a.agent_id.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    WikiCore::new(root)
                        .claim_mail(&message_id, list_address, &agent_id)
                        .map(|receipt| receipt.claimed_by)
                        .map_err(|error| error.to_string())
                })
            };
            let second = {
                let barrier = barrier.clone();
                let root = root.clone();
                let message_id = message_id.clone();
                let agent_id = agent_b.agent_id.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    WikiCore::new(root)
                        .claim_mail(&message_id, list_address, &agent_id)
                        .map(|receipt| receipt.claimed_by)
                        .map_err(|error| error.to_string())
                })
            };

            let results = vec![first.join().unwrap(), second.join().unwrap()];
            let winners = results
                .iter()
                .filter_map(|result| result.as_ref().ok())
                .collect::<Vec<_>>();
            assert_eq!(winners.len(), 1, "{results:?}");
            let loser = results
                .iter()
                .filter_map(|result| result.as_ref().err())
                .next()
                .unwrap();
            assert!(loser.contains("already claimed by"), "{loser}");

            let inbox = core.inbox_with_options(list_address, true, true).unwrap();
            let delivery = inbox
                .messages
                .iter()
                .find(|message| message.message_id == message_id)
                .unwrap();
            assert_eq!(delivery.state, "claimed");
            assert_eq!(delivery.claimed_by.as_deref(), Some(winners[0].as_str()));
        }

        let claim_events =
            read_jsonl::<serde_json::Value>(&root.join("context-engine/mail/claims.jsonl"))
                .unwrap();
        for message_id in message_ids {
            let claim_count = claim_events
                .iter()
                .filter(|event| {
                    event.get("message_id").and_then(serde_json::Value::as_str)
                        == Some(message_id.as_str())
                        && event.get("recipient").and_then(serde_json::Value::as_str)
                            == Some(list_address)
                        && event.get("state").and_then(serde_json::Value::as_str) == Some("claimed")
                })
                .count();
            assert_eq!(claim_count, 1, "{message_id}");
        }
    }

    #[test]
    fn concurrent_duplicate_list_and_watch_operations_leave_one_active_record() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("duplicate-watch-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();

        let list_address = "list://topics.duplicate-watch";
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
        let mut workers = Vec::new();
        for _ in 0..12 {
            let barrier = barrier.clone();
            let root = root.clone();
            let owner = agent.primary_address.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                WikiCore::new(root)
                    .create_list(
                        list_address,
                        Some("Duplicate Watch".to_string()),
                        None,
                        Some("topics".to_string()),
                        Some(owner),
                    )
                    .map(|receipt| receipt.status)
                    .map_err(|error| error.to_string())
            }));
        }
        let statuses = workers
            .into_iter()
            .map(|worker| worker.join().unwrap().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            statuses
                .iter()
                .filter(|status| status.as_str() == "created")
                .count(),
            1
        );
        assert_eq!(
            statuses
                .iter()
                .filter(|status| status.as_str() == "already_exists")
                .count(),
            11
        );
        let list_events =
            read_jsonl::<serde_json::Value>(&root.join("context-engine/mail/lists.jsonl")).unwrap();
        assert_eq!(
            list_events
                .iter()
                .filter(|event| {
                    event.get("address").and_then(serde_json::Value::as_str) == Some(list_address)
                        && event.get("event").and_then(serde_json::Value::as_str)
                            == Some("mail.list.created")
                })
                .count(),
            1
        );

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(12));
        let mut workers = Vec::new();
        for _ in 0..12 {
            let barrier = barrier.clone();
            let root = root.clone();
            let agent_id = agent.agent_id.clone();
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                WikiCore::new(root)
                    .watch_page(
                        "topics",
                        &agent_id,
                        Some(list_address),
                        vec!["proposal".to_string()],
                        1800,
                    )
                    .map(|receipt| {
                        (
                            receipt.subscription.status,
                            receipt.page_mailbox_subscription.status,
                        )
                    })
                    .map_err(|error| error.to_string())
            }));
        }
        for worker in workers {
            worker.join().unwrap().unwrap();
        }
        let subscriptions = core
            .mail_subscriptions(Some(&agent.agent_id), None)
            .unwrap();
        let active_by_address = subscriptions
            .subscriptions
            .iter()
            .map(|subscription| subscription.address.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            active_by_address
                .iter()
                .filter(|address| address.as_str() == list_address)
                .count(),
            1
        );
        assert_eq!(
            active_by_address
                .iter()
                .filter(|address| address.as_str() == "mailbox://page/topics")
                .count(),
            1
        );

        let unwatch = core
            .unwatch_page(
                "topics",
                &agent.agent_id,
                Some(list_address),
                vec!["proposal".to_string()],
            )
            .unwrap();
        assert_eq!(unwatch.status, "unwatched");
        assert_eq!(unwatch.cancelled_count, 2);
        let repeated = core
            .unwatch_page(
                "topics",
                &agent.agent_id,
                Some(list_address),
                vec!["proposal".to_string()],
            )
            .unwrap();
        assert_eq!(repeated.status, "not_found");
        assert_eq!(repeated.cancelled_count, 0);
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
                    .write_page_body("topics", "\n# First writer\n", Some(&expected))
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
                    .write_page_body("topics", "\n# Second writer\n", Some(&expected))
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
    fn repeated_mail_claim_mark_and_notification_ack_are_idempotent() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent(
                "idempotent-mail-agent",
                vec!["role://topics.curator".to_string()],
                Vec::new(),
                1800,
            )
            .unwrap();
        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Idempotent mail".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.primary_address.clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: Vec::new(),
                body_markdown: "Retries should not duplicate durable state.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        let notification = core
            .poll_notifications(&agent.agent_id)
            .unwrap()
            .notifications
            .into_iter()
            .next()
            .unwrap();
        let first_ack = core
            .ack_notification(&agent.agent_id, &notification.notification_id, "delivered")
            .unwrap();
        let second_ack = core
            .ack_notification(&agent.agent_id, &notification.notification_id, "delivered")
            .unwrap();
        assert_eq!(first_ack.status, "ok");
        assert_eq!(second_ack.status, "already_acknowledged");

        let first_claim = core
            .claim_mail(&talk.message_id, "role://topics.curator", &agent.agent_id)
            .unwrap();
        let second_claim = core
            .claim_mail(&talk.message_id, "role://topics.curator", &agent.agent_id)
            .unwrap();
        assert_eq!(first_claim.status, "claimed");
        assert_eq!(second_claim.status, "already_claimed");
        assert_eq!(second_claim.evidence[0].status, "skipped_existing");

        let first_mark = core
            .mark_mail(&talk.message_id, "role://topics.curator", "done", None)
            .unwrap();
        let second_mark = core
            .mark_mail(&talk.message_id, "role://topics.curator", "done", None)
            .unwrap();
        assert_eq!(first_mark.status, "ok");
        assert_eq!(second_mark.status, "unchanged");
        let repeated_mark_all = core
            .mark_mail_all_deliveries(&talk.message_id, "done", None)
            .unwrap();
        assert_eq!(repeated_mark_all.changed_delivery_count, 0);

        let claim_events =
            read_jsonl::<serde_json::Value>(&root.join("context-engine/mail/claims.jsonl"))
                .unwrap();
        assert_eq!(
            claim_events
                .iter()
                .filter(|event| {
                    event.get("message_id").and_then(serde_json::Value::as_str)
                        == Some(talk.message_id.as_str())
                        && event.get("state").and_then(serde_json::Value::as_str) == Some("claimed")
                })
                .count(),
            1
        );
        assert_eq!(
            claim_events
                .iter()
                .filter(|event| {
                    event.get("message_id").and_then(serde_json::Value::as_str)
                        == Some(talk.message_id.as_str())
                        && event.get("state").and_then(serde_json::Value::as_str) == Some("done")
                })
                .count(),
            1
        );
        let attempts = read_jsonl::<serde_json::Value>(
            &root.join("context-engine/notifications/attempts.jsonl"),
        )
        .unwrap();
        assert_eq!(
            attempts
                .iter()
                .filter(|event| {
                    event
                        .get("notification_id")
                        .and_then(serde_json::Value::as_str)
                        == Some(notification.notification_id.as_str())
                })
                .count(),
            1
        );
    }

    #[test]
    fn mail_mark_cannot_create_unowned_claims() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("mark-claimed-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Claim must name an agent".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.primary_address.clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: vec![],
                body_markdown: "Generic mark must not leave a claimed message without claimed_by."
                    .to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        let error = core
            .mark_mail(&talk.message_id, "role://topics.curator", "claimed", None)
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid mail mark state claimed"));
        let before_claim = core.inbox("role://topics.curator").unwrap();
        assert_eq!(before_claim.messages[0].state, "unread");
        assert!(before_claim.messages[0].claimed_by.is_none());

        let claim = core
            .claim_mail(&talk.message_id, "role://topics.curator", &agent.agent_id)
            .unwrap();
        assert_eq!(claim.status, "claimed");
        let after_claim = core.inbox("role://topics.curator").unwrap();
        assert_eq!(
            after_claim.messages[0].claimed_by.as_deref(),
            Some(agent.agent_id.as_str())
        );
    }

    #[test]
    fn duplicate_role_assignment_and_register_inputs_are_deduplicated() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent(
                "duplicate-role-agent",
                vec![
                    "role://topics.curator".to_string(),
                    "role://topics.curator".to_string(),
                ],
                vec!["wiki.mail".to_string(), "wiki.mail".to_string()],
                1800,
            )
            .unwrap();
        assert_eq!(
            agent
                .addresses
                .iter()
                .filter(|address| address.as_str() == "role://topics.curator")
                .count(),
            1
        );

        let first = core
            .assign_page_role(
                "topics",
                &agent.agent_id,
                "curator",
                vec!["proposal".to_string()],
                1800,
            )
            .unwrap();
        let second = core
            .assign_page_role(
                "topics",
                &agent.agent_id,
                "curator",
                vec!["proposal".to_string()],
                1800,
            )
            .unwrap();
        assert_eq!(first.subscription.status, "subscribed");
        assert_eq!(second.subscription.status, "renewed");
        let subscriptions = core
            .mail_subscriptions(Some(&agent.agent_id), Some("role://topics.curator"))
            .unwrap();
        assert_eq!(subscriptions.subscription_count, 1);
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
    fn mail_claim_does_not_reopen_terminal_delivery() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("terminal-claim-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Terminal delivery".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.primary_address.clone(),
                to: vec!["role://topics.curator".to_string()],
                cc: Vec::new(),
                body_markdown: "Done mail must stay done.".to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        core.mark_mail_all_deliveries(&talk.message_id, "done", None)
            .unwrap();
        let error = core
            .claim_mail(&talk.message_id, "role://topics.curator", &agent.agent_id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("not claimable"), "{error}");
        let inbox = core
            .inbox_with_options("role://topics.curator", true, true)
            .unwrap();
        assert_eq!(inbox.messages[0].state, "done");
        assert!(inbox.messages[0].claimed_by.is_none());
    }

    #[test]
    fn talk_attachments_copy_media_and_duplicate_names() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent(
                "probe-c-agent",
                vec!["role://topics.curator".to_string()],
                vec!["probe-c".to_string()],
                1800,
            )
            .unwrap();

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
                from: agent.addresses[0].clone(),
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
        assert_eq!(talk.notifications[0].attachment_count, 7);

        let inbox = core.inbox("role://topics.curator").unwrap();
        assert_eq!(inbox.messages[0].attachment_count, 7);
        assert_eq!(inbox.messages[0].attachments.len(), 7);
        assert_eq!(
            inbox.messages[0].attachments[0].filename,
            "agent-facing-note.txt"
        );
        assert_eq!(
            inbox.messages[0].attachments[0].caption.as_deref(),
            Some("Agent-facing caption")
        );
        assert_eq!(
            inbox.messages[0].attachments[0].alt_text.as_deref(),
            Some("Agent-facing alt text")
        );
        assert_eq!(inbox.messages[0].attachments[5].filename, "duplicate-2.txt");
        let agent_inbox = core.agent_inbox(&agent.agent_id, false, false).unwrap();
        assert_eq!(agent_inbox.threads[0].attachment_count, 7);
        assert_eq!(agent_inbox.messages[0].attachment_count, 7);

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
            })
            .unwrap_err()
            .to_string();
        assert!(unsafe_error.contains("invalid attachment filename"));
        assert!(!attachments_dir.exists());
    }

    #[test]
    fn invalid_talk_recipient_with_attachment_leaves_no_orphan_files() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let attachment = temp.path().join("proof.txt");
        fs::write(&attachment, "attachment should not be staged\n").unwrap();
        let talk_dir = root.join("user-wiki/source/families/reference/topics/talk/topics.talk");
        let attachments_dir = talk_dir.join("attachments");

        let error = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Invalid recipient before attachment copy".to_string(),
                thread_id: None,
                reply_to: None,
                from: "agent://worker-cy.sender".to_string(),
                to: vec!["bad address".to_string()],
                cc: vec![],
                body_markdown: "Invalid recipients must fail before attachment staging."
                    .to_string(),
                attachments: vec![attachment_input(attachment)],
                allow_tombstoned: false,
            })
            .unwrap_err()
            .to_string();

        assert!(error.contains("invalid address"), "{error}");
        assert!(!attachments_dir.exists());
    }

    #[test]
    fn mailbox_address_key_collisions_do_not_cross_mark_or_claim() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("1Context");
        seed_fixture(&root);
        let core = WikiCore::new(&root);
        core.create_page("topics", None).unwrap();
        let agent = core
            .register_agent("mailbox-collision-agent", Vec::new(), Vec::new(), 1800)
            .unwrap();
        let dot_recipient = "role://worker-cy.alpha";
        let slash_recipient = "role://worker-cy/alpha";

        let talk = core
            .append_talk(TalkAppendRequest {
                page: "topics".to_string(),
                kind: "proposal".to_string(),
                subject: "Mailbox collision".to_string(),
                thread_id: None,
                reply_to: None,
                from: agent.primary_address.clone(),
                to: vec![dot_recipient.to_string(), slash_recipient.to_string()],
                cc: vec![],
                body_markdown: "These recipients share an address_key but not a mailbox identity."
                    .to_string(),
                attachments: vec![],
                allow_tombstoned: false,
            })
            .unwrap();

        let dot_before = core.inbox(dot_recipient).unwrap();
        let slash_before = core.inbox(slash_recipient).unwrap();
        assert_eq!(dot_before.messages.len(), 1);
        assert_eq!(slash_before.messages.len(), 1);
        assert_eq!(dot_before.messages[0].recipient, dot_recipient);
        assert_eq!(slash_before.messages[0].recipient, slash_recipient);

        core.mark_mail(&talk.message_id, dot_recipient, "done", None)
            .unwrap();
        let dot_after = core.inbox(dot_recipient).unwrap();
        let slash_after = core.inbox(slash_recipient).unwrap();
        assert_eq!(dot_after.messages[0].state, "done");
        assert_eq!(slash_after.messages[0].state, "unread");

        let slash_claim = core
            .claim_mail(&talk.message_id, slash_recipient, &agent.agent_id)
            .unwrap();
        assert_eq!(slash_claim.status, "claimed");
        assert_eq!(
            core.inbox(slash_recipient).unwrap().messages[0]
                .claimed_by
                .as_deref(),
            Some(agent.agent_id.as_str())
        );
        assert!(core.inbox(dot_recipient).unwrap().messages[0]
            .claimed_by
            .is_none());
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
    }
}
