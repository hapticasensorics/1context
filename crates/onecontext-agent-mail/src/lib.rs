use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Duration, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use url::Url;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MailAddress {
    Agent { transport: String, id: String },
    Role { scope: String, role: String },
    List { name: String },
    PageMailbox { page_id: String },
    Thread { thread_id: String },
    System { name: String },
}

impl MailAddress {
    pub fn parse(input: &str) -> Result<Self> {
        let value = input.trim();
        if value.is_empty() || value.chars().any(char::is_whitespace) {
            return Err(anyhow!(
                "invalid mail address {input:?}; expected a typed address without whitespace"
            ));
        }
        if value.contains("/../")
            || value.ends_with("/..")
            || value.contains("/./")
            || value.ends_with("/.")
        {
            return Err(anyhow!(
                "invalid mail address {input}; path traversal is not allowed"
            ));
        }

        let parsed = Url::parse(value)
            .with_context(|| format!("invalid mail address {input}; expected typed URI address"))?;
        validate_mail_address_url(&parsed, input)?;

        match parsed.scheme() {
            "agent" => {
                let transport = parsed.host_str().ok_or_else(|| {
                    anyhow!("invalid agent address {input}; expected agent://<transport>/<id>")
                })?;
                let id = single_path_segment(&parsed, input, "agent", "id")?;
                validate_segment(transport, "agent transport")?;
                validate_segment(id, "agent id")?;
                Ok(Self::Agent {
                    transport: transport.to_string(),
                    id: id.to_string(),
                })
            }
            "role" => {
                let rest = parsed.host_str().ok_or_else(|| {
                    anyhow!("invalid role address {input}; expected role://<scope>.<role>")
                })?;
                require_empty_path(&parsed, input, "role")?;
                let Some((scope, role)) = rest.rsplit_once('.') else {
                    return Err(anyhow!(
                        "invalid role address {input}; expected role://<scope>.<role>"
                    ));
                };
                validate_segment(scope, "role scope")?;
                validate_segment(role, "role name")?;
                Ok(Self::Role {
                    scope: scope.to_string(),
                    role: role.to_string(),
                })
            }
            "list" => {
                let name = parsed.host_str().ok_or_else(|| {
                    anyhow!("invalid list address {input}; expected list://<name>")
                })?;
                require_empty_path(&parsed, input, "list")?;
                validate_dotted_name(name, "list address")?;
                Ok(Self::List {
                    name: name.to_string(),
                })
            }
            "mailbox" => {
                if parsed.host_str() != Some("page") {
                    return Err(anyhow!(
                        "invalid mailbox address {input}; expected mailbox://page/<page-id>"
                    ));
                }
                let page_id = single_path_segment(&parsed, input, "mailbox", "page id")?;
                validate_segment(page_id, "page mailbox id")?;
                Ok(Self::PageMailbox {
                    page_id: page_id.to_string(),
                })
            }
            "page" => {
                let page_id = parsed.host_str().ok_or_else(|| {
                    anyhow!("invalid page address {input}; expected page://<page-id>")
                })?;
                require_empty_path(&parsed, input, "page")?;
                validate_segment(page_id, "page mailbox id")?;
                Ok(Self::PageMailbox {
                    page_id: page_id.to_string(),
                })
            }
            "thread" => {
                let thread_id = parsed.host_str().ok_or_else(|| {
                    anyhow!("invalid thread address {input}; expected thread://<thread-id>")
                })?;
                require_empty_path(&parsed, input, "thread")?;
                validate_segment(thread_id, "thread id")?;
                Ok(Self::Thread {
                    thread_id: thread_id.to_string(),
                })
            }
            "system" => {
                let name = parsed.host_str().ok_or_else(|| {
                    anyhow!("invalid system address {input}; expected system://<name>")
                })?;
                require_empty_path(&parsed, input, "system")?;
                validate_dotted_name(name, "system address")?;
                Ok(Self::System {
                    name: name.to_string(),
                })
            }
            _ => Err(anyhow!(
                "invalid mail address {input}; expected agent://, role://, list://, mailbox://page/, page://, thread://, or system://"
            )),
        }
    }

    pub fn canonical(&self) -> String {
        match self {
            Self::Agent { transport, id } => format!("agent://{transport}/{id}"),
            Self::Role { scope, role } => format!("role://{scope}.{role}"),
            Self::List { name } => format!("list://{name}"),
            Self::PageMailbox { page_id } => format!("mailbox://page/{page_id}"),
            Self::Thread { thread_id } => format!("thread://{thread_id}"),
            Self::System { name } => format!("system://{name}"),
        }
    }

    pub fn mailbox_key(&self) -> String {
        let mut hash = Sha256::new();
        hash.update(self.canonical().as_bytes());
        format!("addr-{}", hex_lower(&hash.finalize()))
    }
}

fn validate_mail_address_url(parsed: &Url, input: &str) -> Result<()> {
    if parsed.username() != ""
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
        || parsed.cannot_be_a_base()
    {
        return Err(anyhow!("invalid mail address {input}; expected scheme://host[/segment] without credentials, port, query, or fragment"));
    }
    Ok(())
}

fn require_empty_path(parsed: &Url, input: &str, scheme: &str) -> Result<()> {
    if !matches!(parsed.path(), "" | "/") {
        return Err(anyhow!(
            "invalid {scheme} address {input}; expected scheme://host with no path"
        ));
    }
    Ok(())
}

fn single_path_segment<'a>(
    parsed: &'a Url,
    input: &str,
    scheme: &str,
    label: &str,
) -> Result<&'a str> {
    let mut segments = parsed
        .path_segments()
        .ok_or_else(|| {
            anyhow!("invalid {scheme} address {input}; expected one {label} path segment")
        })?
        .filter(|segment| !segment.is_empty());
    let segment = segments.next().ok_or_else(|| {
        anyhow!("invalid {scheme} address {input}; expected one {label} path segment")
    })?;
    if segments.next().is_some() {
        return Err(anyhow!(
            "invalid {scheme} address {input}; expected one {label} path segment"
        ));
    }
    Ok(segment)
}

impl fmt::Display for MailAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.canonical())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRecord {
    pub schema_version: u32,
    pub agent_id: String,
    pub primary_address: String,
    pub transport: AgentTransport,
    pub requested_roles: Vec<String>,
    pub granted_roles: Vec<String>,
    pub requested_capabilities: Vec<String>,
    pub granted_capabilities: Vec<String>,
    pub lease_expires_at: String,
    pub state: AgentState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLeaseRecord {
    pub schema_version: u32,
    pub agent_id: String,
    pub lease_expires_at: String,
    pub occurred_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentStatusSnapshot {
    pub record: AgentRecord,
    pub latest_lease: Option<AgentLeaseRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLeaseState {
    Active,
    Stale,
    Retired,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentThreadStatusSnapshot {
    pub schema_version: u32,
    pub thread_id: String,
    pub agent_id: Option<String>,
    pub lease_state: AgentLeaseState,
    pub agent: Option<AgentRecord>,
    pub latest_lease: Option<AgentLeaseRecord>,
    pub active_delivery: Option<InboxRow>,
    pub pending_notifications: Vec<NotificationRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentPushEligibility {
    pub agent_id: String,
    pub eligible: bool,
    pub reason: AgentPushEligibilityReason,
    pub latest_lease: Option<AgentLeaseRecord>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AgentPushEligibilityReason {
    ActiveLease,
    StaleLease,
    RetiredAgent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentIdentifyRequest {
    pub transport_kind: String,
    pub thread_id: String,
    pub requested_roles: Vec<String>,
    pub requested_capabilities: Vec<String>,
    pub lease_expires_at: String,
    pub occurred_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct AgentGrantPolicy {
    pub allowed_roles: BTreeSet<String>,
    pub allowed_capabilities: BTreeSet<String>,
}

impl AgentGrantPolicy {
    pub fn allow_exact(roles: &[&str], capabilities: &[&str]) -> Self {
        Self {
            allowed_roles: roles
                .iter()
                .map(|role| {
                    MailAddress::parse(role)
                        .map(|address| address.canonical())
                        .unwrap_or_else(|_| role.to_string())
                })
                .collect(),
            allowed_capabilities: capabilities
                .iter()
                .map(|capability| capability.to_string())
                .collect(),
        }
    }

    fn grant_roles(&self, requested_roles: &[String]) -> Vec<String> {
        requested_roles
            .iter()
            .filter(|role| self.allowed_roles.contains(*role))
            .cloned()
            .collect()
    }

    fn grant_capabilities(&self, requested_capabilities: &[String]) -> Vec<String> {
        requested_capabilities
            .iter()
            .filter(|capability| self.allowed_capabilities.contains(*capability))
            .cloned()
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTransport {
    pub kind: String,
    pub thread_id: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentState {
    Active,
    Stale,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageEnvelope {
    pub schema_version: u32,
    pub message_id: String,
    pub idempotency_key: String,
    pub kind: String,
    pub subject: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub page: Option<MessagePageRef>,
    pub thread_id: String,
    pub reply_to: Option<String>,
    pub body: MessageBodyRef,
    pub attachments: Vec<MessageAttachmentRef>,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessagePageRef {
    pub id: String,
    pub route: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageBodyRef {
    pub format: String,
    pub sha256: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MessageAttachmentRef {
    pub filename: String,
    pub media_type: String,
    pub sha256: String,
    pub handle: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRecord {
    pub schema_version: u32,
    pub delivery_id: String,
    pub message_id: String,
    pub thread_id: String,
    pub recipient: String,
    pub state: DeliveryState,
    pub claimed_by: Option<String>,
    pub snoozed_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationRecord {
    pub schema_version: u32,
    pub notification_id: String,
    pub agent_id: String,
    pub agent_address: String,
    pub transport: String,
    pub transport_thread_id: String,
    pub delivery_id: String,
    pub message_id: String,
    pub message_thread_id: String,
    pub recipient: String,
    pub state: NotificationState,
    pub created_at: String,
    pub updated_at: String,
    pub acknowledged_at: Option<String>,
    #[serde(default)]
    pub attempt_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_attempt_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_dispatch_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationAttemptRecord {
    pub schema_version: u32,
    pub attempt_id: String,
    pub notification_id: String,
    pub agent_id: String,
    pub transport: String,
    pub status: NotificationAttemptStatus,
    pub occurred_at: String,
    pub payload: CodexSteeringPayload,
    pub steering_text: String,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexSteeringPayload {
    pub transport: String,
    pub thread_id: String,
    pub transport_attempt_id: Option<String>,
    pub agent_id: String,
    pub notification_id: String,
    pub message_id: String,
    pub delivery_id: String,
    pub message_thread_id: String,
    pub mailbox: String,
    pub kind: String,
    pub subject: String,
    pub page: Option<MessagePageRef>,
    pub delivery_state: DeliveryState,
    pub delivery_updated_at: String,
    pub message_created_at: String,
    pub instruction: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationState {
    Pending,
    Acknowledged,
    Suppressed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NotificationAttemptStatus {
    Sent,
    Failed,
    DryRun,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimEvent {
    pub schema_version: u32,
    pub delivery_id: String,
    pub message_id: String,
    pub thread_id: String,
    pub recipient: String,
    pub agent_id: String,
    pub state: DeliveryState,
    pub occurred_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct IdempotencyRecord {
    pub schema_version: u32,
    pub idempotency_key: String,
    pub message_id: String,
    pub payload_sha256: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadLetterRecord {
    pub schema_version: u32,
    pub message_id: Option<String>,
    pub recipient: Option<String>,
    pub code: MailErrorCode,
    pub message: String,
    pub occurred_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessageAcceptance {
    Accepted,
    DuplicateSamePayload { message_id: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SendMailOptions {
    pub max_open_deliveries_per_recipient: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MailSendReceipt {
    pub acceptance: MessageAcceptance,
    pub attempts: Vec<DeliveryAttempt>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryAttempt {
    pub recipient: String,
    pub delivery_id: String,
    pub status: DeliveryAttemptStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DeliveryAttemptStatus {
    Delivered,
    AlreadyDelivered,
    DeferredCapacity,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    Unread,
    Read,
    Claimed,
    Done,
    Archived,
    Rejected,
    Snoozed,
    DeadLetter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InboxRow {
    pub schema_version: u32,
    pub delivery_id: String,
    pub message_id: String,
    pub thread_id: String,
    pub recipient: String,
    pub state: DeliveryState,
    pub claimed_by: Option<String>,
    pub snoozed_until: Option<String>,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecipientMailSummary {
    pub recipient: String,
    pub open_delivery_count: usize,
    pub open_thread_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HydratedMessage {
    pub envelope: MessageEnvelope,
    pub body_markdown: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenedMessageSummary {
    pub envelope: MessageEnvelope,
    pub body_sha256: String,
    pub body_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpenContentDelivery {
    pub schema_version: u32,
    pub transport: String,
    pub method: String,
    pub status: String,
    pub thread_id: String,
    pub items: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OpenDeliveryResult {
    pub delivery: DeliveryRecord,
    pub message: OpenedMessageSummary,
    pub content_delivery: OpenContentDelivery,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailInjectionReceipt {
    pub schema_version: u32,
    pub injection_id: String,
    pub delivery_id: String,
    pub message_id: String,
    pub agent_id: String,
    pub thread_id: String,
    pub body_sha256: String,
    pub item_count: usize,
    pub app_server_method: String,
    pub app_server_result: MailInjectionResult,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailInjectionRecordResult {
    pub receipt: MailInjectionReceipt,
    pub control_event: MailControlEvent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailInjectionResult {
    Ok,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailControlEvent {
    pub schema_version: u32,
    pub control_event_id: String,
    pub created_at: String,
    pub source: MailControlEventSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_event_name: Option<String>,
    pub codex: MailControlCodexRef,
    pub agent: MailControlAgentRef,
    pub mail_refs: MailControlRefs,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<MailControlDecision>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub additional_context_sha256: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailControlEventSource {
    CodexHook,
    CodexAppServer,
    MailSupervisor,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailControlCodexRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission_mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool_use_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailControlAgentRef {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lease_state: Option<AgentLeaseState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_delivery_id: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailControlRefs {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notification_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delivery_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub claim_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub proposal_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailControlDecision {
    pub behavior: MailControlDecisionBehavior,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailControlDecisionBehavior {
    Allow,
    Deny,
    BlockPrompt,
    ContinueTurn,
    InjectContext,
    RecordOnly,
    RewriteToolInput,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MailFailure {
    pub schema_version: u32,
    pub code: MailErrorCode,
    pub message: String,
    pub repair_hints: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MailErrorCode {
    InvalidAddress,
    UnsafeMailboxKey,
    UnknownRoute,
    DuplicateIdempotencyKey,
    MailboxFull,
    StaleAgent,
    RetiredAgent,
    CorruptIndex,
    DeadLetter,
}

#[derive(Clone, Debug)]
pub struct AgentMailPaths {
    pub context_engine: PathBuf,
    pub agents: PathBuf,
    pub mail: PathBuf,
    pub notifications: PathBuf,
}

impl AgentMailPaths {
    pub fn new(context_engine: impl Into<PathBuf>) -> Self {
        let context_engine = context_engine.into();
        Self {
            agents: context_engine.join("agents"),
            mail: context_engine.join("mail"),
            notifications: context_engine.join("notifications"),
            context_engine,
        }
    }

    pub fn ensure_v0_dirs(&self) -> Result<()> {
        for path in [
            self.agents.join("directory"),
            self.mail.join("messages"),
            self.mail.join("bodies"),
            self.mail.join("mailboxes"),
            self.notifications.join("cursors"),
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }

    pub fn message_path(&self, message_id: &str, created_at: &str) -> Result<PathBuf> {
        validate_segment(message_id, "message id")?;
        let (year, month) = year_month(created_at)?;
        Ok(self
            .mail
            .join("messages")
            .join(year)
            .join(month)
            .join(format!("{message_id}.json")))
    }

    pub fn body_path(&self, message_id: &str, created_at: &str) -> Result<PathBuf> {
        validate_segment(message_id, "message id")?;
        let (year, month) = year_month(created_at)?;
        Ok(self
            .mail
            .join("bodies")
            .join(year)
            .join(month)
            .join(format!("{message_id}.md")))
    }

    pub fn deliveries_path(&self) -> PathBuf {
        self.mail.join("deliveries.jsonl")
    }

    pub fn claims_path(&self) -> PathBuf {
        self.mail.join("claims.jsonl")
    }

    pub fn idempotency_path(&self) -> PathBuf {
        self.mail.join("idempotency.jsonl")
    }

    pub fn mutation_lock_path(&self) -> PathBuf {
        self.mail.join(".mutation.lock")
    }

    pub fn dead_letter_path(&self) -> PathBuf {
        self.mail.join("dead-letter.jsonl")
    }

    pub fn control_events_path(&self) -> PathBuf {
        self.mail.join("control-events.jsonl")
    }

    pub fn injection_receipts_path(&self) -> PathBuf {
        self.mail.join("injection-receipts.jsonl")
    }

    pub fn notification_outbox_path(&self) -> PathBuf {
        self.notifications.join("outbox.jsonl")
    }

    pub fn notification_attempts_path(&self) -> PathBuf {
        self.notifications.join("attempts.jsonl")
    }

    pub fn agents_path(&self) -> PathBuf {
        self.agents.join("directory").join("agents.jsonl")
    }

    pub fn leases_path(&self) -> PathBuf {
        self.agents.join("directory").join("leases.jsonl")
    }

    pub fn mailbox_path(&self, address: &MailAddress) -> PathBuf {
        self.mail
            .join("mailboxes")
            .join(address.mailbox_key())
            .join("inbox.jsonl")
    }
}

#[derive(Clone, Debug)]
pub struct AgentMailStore {
    paths: AgentMailPaths,
}

struct MailStoreMutationLock {
    file: File,
}

impl Drop for MailStoreMutationLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

impl AgentMailStore {
    pub fn new(context_engine: impl Into<PathBuf>) -> Self {
        Self {
            paths: AgentMailPaths::new(context_engine),
        }
    }

    pub fn paths(&self) -> &AgentMailPaths {
        &self.paths
    }

    pub fn ensure(&self) -> Result<()> {
        self.paths.ensure_v0_dirs()
    }

    pub fn write_message(&self, envelope: &MessageEnvelope, body_markdown: &str) -> Result<()> {
        self.ensure()?;
        let message_path = self
            .paths
            .message_path(&envelope.message_id, &envelope.created_at)?;
        let body_path = self
            .paths
            .body_path(&envelope.message_id, &envelope.created_at)?;
        write_json_file(&message_path, envelope)?;
        write_text_file(&body_path, body_markdown)?;
        Ok(())
    }

    pub fn accept_message(
        &self,
        envelope: &MessageEnvelope,
        body_markdown: &str,
    ) -> Result<MessageAcceptance> {
        let _lock = self.lock_mutations()?;
        self.accept_message_unlocked(envelope, body_markdown)
    }

    fn accept_message_unlocked(
        &self,
        envelope: &MessageEnvelope,
        body_markdown: &str,
    ) -> Result<MessageAcceptance> {
        self.ensure()?;
        let payload_sha256 = message_payload_sha256(envelope, body_markdown)?;
        for record in read_jsonl::<IdempotencyRecord>(&self.paths.idempotency_path())? {
            if record.idempotency_key == envelope.idempotency_key {
                if record.payload_sha256 == payload_sha256 {
                    return Ok(MessageAcceptance::DuplicateSamePayload {
                        message_id: record.message_id,
                    });
                }
                return Err(anyhow!(
                    "duplicate idempotency key {} with different payload",
                    envelope.idempotency_key
                ));
            }
        }

        self.write_message(envelope, body_markdown)?;
        append_jsonl(
            &self.paths.idempotency_path(),
            &IdempotencyRecord {
                schema_version: 1,
                idempotency_key: envelope.idempotency_key.clone(),
                message_id: envelope.message_id.clone(),
                payload_sha256,
                created_at: envelope.created_at.clone(),
            },
        )?;
        Ok(MessageAcceptance::Accepted)
    }

    pub fn send_mail(
        &self,
        envelope: &MessageEnvelope,
        body_markdown: &str,
        options: &SendMailOptions,
    ) -> Result<MailSendReceipt> {
        MailAddress::parse(&envelope.from)?;
        let recipients = normalized_recipients(envelope)?;
        let _lock = self.lock_mutations()?;
        let acceptance = self.accept_message_unlocked(envelope, body_markdown)?;
        let delivery_envelope;
        let envelope = match &acceptance {
            MessageAcceptance::DuplicateSamePayload { message_id }
                if message_id != &envelope.message_id =>
            {
                delivery_envelope = self.read_message_envelope(message_id)?;
                &delivery_envelope
            }
            _ => envelope,
        };
        let mut attempts = Vec::new();
        // Delivery creation needs a consistent view of the append-only ledger so
        // concurrent send retries cannot both observe "missing" and append the
        // same stable delivery id.
        let mut latest_deliveries = self.latest_deliveries()?;
        for recipient in recipients {
            let delivery_id = stable_delivery_id(&envelope.message_id, &recipient);
            if let Some(delivery) = latest_deliveries.get(&delivery_id) {
                self.ensure_delivery_notifications(delivery)?;
                attempts.push(DeliveryAttempt {
                    recipient,
                    delivery_id,
                    status: DeliveryAttemptStatus::AlreadyDelivered,
                });
                continue;
            }
            if let Some(limit) = options.max_open_deliveries_per_recipient {
                let open_delivery_count = latest_deliveries
                    .values()
                    .filter(|delivery| delivery.recipient == recipient && delivery.state.is_open())
                    .count();
                if open_delivery_count >= limit {
                    self.append_dead_letter(&DeadLetterRecord {
                        schema_version: 1,
                        message_id: Some(envelope.message_id.clone()),
                        recipient: Some(recipient.clone()),
                        code: MailErrorCode::MailboxFull,
                        message: format!(
                            "recipient {recipient} reached open delivery limit {limit}"
                        ),
                        occurred_at: envelope.created_at.clone(),
                    })?;
                    attempts.push(DeliveryAttempt {
                        recipient,
                        delivery_id,
                        status: DeliveryAttemptStatus::DeferredCapacity,
                    });
                    continue;
                }
            }
            let delivery = DeliveryRecord {
                schema_version: 1,
                delivery_id: delivery_id.clone(),
                message_id: envelope.message_id.clone(),
                thread_id: envelope.thread_id.clone(),
                recipient: recipient.clone(),
                state: DeliveryState::Unread,
                claimed_by: None,
                snoozed_until: None,
                created_at: envelope.created_at.clone(),
                updated_at: envelope.created_at.clone(),
            };
            self.append_delivery(&delivery)?;
            self.ensure_delivery_notifications(&delivery)?;
            latest_deliveries.insert(delivery.delivery_id.clone(), delivery);
            attempts.push(DeliveryAttempt {
                recipient,
                delivery_id,
                status: DeliveryAttemptStatus::Delivered,
            });
        }
        Ok(MailSendReceipt {
            acceptance,
            attempts,
        })
    }

    fn lock_mutations(&self) -> Result<MailStoreMutationLock> {
        self.ensure()?;
        let path = self.paths.mutation_lock_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&path)
            .with_context(|| format!("open mail mutation lock {}", path.display()))?;
        file.lock_exclusive()
            .with_context(|| format!("lock mail mutation lock {}", path.display()))?;
        Ok(MailStoreMutationLock { file })
    }

    pub fn append_delivery(&self, delivery: &DeliveryRecord) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.deliveries_path(), delivery)
    }

    pub fn append_claim(&self, claim: &ClaimEvent) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.claims_path(), claim)
    }

    pub fn append_dead_letter(&self, record: &DeadLetterRecord) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.dead_letter_path(), record)
    }

    pub fn append_notification(&self, record: &NotificationRecord) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.notification_outbox_path(), record)
    }

    pub fn append_notification_attempt(&self, record: &NotificationAttemptRecord) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.notification_attempts_path(), record)
    }

    pub fn append_control_event(&self, record: &MailControlEvent) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.control_events_path(), record)
    }

    pub fn append_injection_receipt(&self, record: &MailInjectionReceipt) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.injection_receipts_path(), record)
    }

    pub fn append_agent(&self, record: &AgentRecord) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.agents_path(), record)
    }

    pub fn append_lease(&self, record: &AgentLeaseRecord) -> Result<()> {
        self.ensure()?;
        append_jsonl(&self.paths.leases_path(), record)
    }

    pub fn identify_agent(
        &self,
        request: &AgentIdentifyRequest,
        policy: &AgentGrantPolicy,
    ) -> Result<AgentRecord> {
        validate_segment(&request.transport_kind, "transport kind")?;
        validate_segment(&request.thread_id, "thread id")?;
        let mut requested_route_grants = Vec::new();
        for role in &request.requested_roles {
            let address = MailAddress::parse(role)?;
            match address {
                MailAddress::Role { .. }
                | MailAddress::List { .. }
                | MailAddress::PageMailbox { .. } => {}
                _ => {
                    return Err(anyhow!(
                        "requested routing grant must be role://, list://, mailbox://page/, or page:// address: {role}"
                    ))
                }
            }
            requested_route_grants.push(MailAddress::parse(role)?.canonical());
        }
        for capability in &request.requested_capabilities {
            validate_dotted_name(capability, "capability")?;
        }

        let agent_id = stable_agent_id(&request.transport_kind, &request.thread_id);
        if self
            .latest_agent_record(&agent_id)?
            .is_some_and(|record| record.state == AgentState::Retired)
        {
            return Err(anyhow!(
                "retired agent {agent_id} cannot be silently revived"
            ));
        }
        let record = AgentRecord {
            schema_version: 1,
            agent_id: agent_id.clone(),
            primary_address: format!("agent://{}/{}", request.transport_kind, agent_id),
            transport: AgentTransport {
                kind: request.transport_kind.clone(),
                thread_id: request.thread_id.clone(),
            },
            requested_roles: request.requested_roles.clone(),
            granted_roles: policy.grant_roles(&requested_route_grants),
            requested_capabilities: request.requested_capabilities.clone(),
            granted_capabilities: policy.grant_capabilities(&request.requested_capabilities),
            lease_expires_at: request.lease_expires_at.clone(),
            state: AgentState::Active,
        };
        self.append_agent(&record)?;
        self.append_lease(&AgentLeaseRecord {
            schema_version: 1,
            agent_id,
            lease_expires_at: request.lease_expires_at.clone(),
            occurred_at: request.occurred_at.clone(),
        })?;
        Ok(record)
    }

    pub fn heartbeat_agent(
        &self,
        agent_id: &str,
        lease_expires_at: &str,
        occurred_at: &str,
    ) -> Result<AgentLeaseRecord> {
        validate_segment(agent_id, "agent id")?;
        let Some(record) = self.latest_agent_record(agent_id)? else {
            return Err(anyhow!("unknown agent {agent_id}"));
        };
        if record.state == AgentState::Retired {
            return Err(anyhow!("retired agent {agent_id} cannot heartbeat"));
        }
        if record.state == AgentState::Stale {
            let mut active_record = record;
            active_record.state = AgentState::Active;
            active_record.lease_expires_at = lease_expires_at.to_string();
            self.append_agent(&active_record)?;
        }
        let lease = AgentLeaseRecord {
            schema_version: 1,
            agent_id: agent_id.to_string(),
            lease_expires_at: lease_expires_at.to_string(),
            occurred_at: occurred_at.to_string(),
        };
        self.append_lease(&lease)?;
        Ok(lease)
    }

    pub fn agent_status(&self, agent_id: &str) -> Result<AgentStatusSnapshot> {
        validate_segment(agent_id, "agent id")?;
        let Some(record) = self.latest_agent_record(agent_id)? else {
            return Err(anyhow!("unknown agent {agent_id}"));
        };
        let latest_lease = read_jsonl::<AgentLeaseRecord>(&self.paths.leases_path())?
            .into_iter()
            .filter(|lease| lease.agent_id == agent_id)
            .last();
        Ok(AgentStatusSnapshot {
            record,
            latest_lease,
        })
    }

    pub fn agent_status_by_thread(
        &self,
        thread_id: &str,
        now: &str,
    ) -> Result<AgentThreadStatusSnapshot> {
        validate_segment(thread_id, "thread id")?;
        let Some(agent) = self.latest_agent_record_by_thread(thread_id)? else {
            return Ok(AgentThreadStatusSnapshot {
                schema_version: 1,
                thread_id: thread_id.to_string(),
                agent_id: None,
                lease_state: AgentLeaseState::Unknown,
                agent: None,
                latest_lease: None,
                active_delivery: None,
                pending_notifications: Vec::new(),
            });
        };
        let latest_lease = self.latest_agent_lease(&agent.agent_id)?;
        let lease_state = self.lease_state_for_record(&agent, latest_lease.as_ref(), now)?;
        let active_delivery = self.active_delivery_for_agent(&agent.agent_id)?;
        let pending_notifications =
            self.pending_open_notifications_for_agent(&agent.agent_id, now)?;
        Ok(AgentThreadStatusSnapshot {
            schema_version: 1,
            thread_id: thread_id.to_string(),
            agent_id: Some(agent.agent_id.clone()),
            lease_state,
            agent: Some(agent),
            latest_lease,
            active_delivery,
            pending_notifications,
        })
    }

    pub fn agent_push_eligibility(
        &self,
        agent_id: &str,
        now: &str,
    ) -> Result<AgentPushEligibility> {
        let status = self.agent_status(agent_id)?;
        if status.record.state == AgentState::Retired {
            return Ok(AgentPushEligibility {
                agent_id: agent_id.to_string(),
                eligible: false,
                reason: AgentPushEligibilityReason::RetiredAgent,
                latest_lease: status.latest_lease,
            });
        }
        if status.record.state == AgentState::Stale {
            return Ok(AgentPushEligibility {
                agent_id: agent_id.to_string(),
                eligible: false,
                reason: AgentPushEligibilityReason::StaleLease,
                latest_lease: status.latest_lease,
            });
        }

        let now = parse_rfc3339_utc(now, "now")?;
        let eligible = status
            .latest_lease
            .as_ref()
            .map(|lease| {
                parse_rfc3339_utc(&lease.lease_expires_at, "lease expiration")
                    .map(|lease_expires_at| lease_expires_at > now)
            })
            .transpose()?
            .unwrap_or(false);
        Ok(AgentPushEligibility {
            agent_id: agent_id.to_string(),
            eligible,
            reason: if eligible {
                AgentPushEligibilityReason::ActiveLease
            } else {
                AgentPushEligibilityReason::StaleLease
            },
            latest_lease: status.latest_lease,
        })
    }

    pub fn retire_agent(
        &self,
        agent_id: &str,
        lease_expires_at: &str,
        occurred_at: &str,
    ) -> Result<AgentRecord> {
        validate_segment(agent_id, "agent id")?;
        let Some(mut record) = self.latest_agent_record(agent_id)? else {
            return Err(anyhow!("unknown agent {agent_id}"));
        };
        record.state = AgentState::Retired;
        record.lease_expires_at = lease_expires_at.to_string();
        self.append_agent(&record)?;
        self.append_lease(&AgentLeaseRecord {
            schema_version: 1,
            agent_id: agent_id.to_string(),
            lease_expires_at: lease_expires_at.to_string(),
            occurred_at: occurred_at.to_string(),
        })?;
        Ok(record)
    }

    fn latest_agent_record(&self, agent_id: &str) -> Result<Option<AgentRecord>> {
        Ok(read_jsonl::<AgentRecord>(&self.paths.agents_path())?
            .into_iter()
            .filter(|record| record.agent_id == agent_id)
            .last())
    }

    fn latest_agent_record_by_thread(&self, thread_id: &str) -> Result<Option<AgentRecord>> {
        Ok(read_jsonl::<AgentRecord>(&self.paths.agents_path())?
            .into_iter()
            .filter(|record| record.transport.thread_id == thread_id)
            .last())
    }

    fn latest_agent_lease(&self, agent_id: &str) -> Result<Option<AgentLeaseRecord>> {
        Ok(read_jsonl::<AgentLeaseRecord>(&self.paths.leases_path())?
            .into_iter()
            .filter(|lease| lease.agent_id == agent_id)
            .last())
    }

    fn lease_state_for_record(
        &self,
        agent: &AgentRecord,
        latest_lease: Option<&AgentLeaseRecord>,
        now: &str,
    ) -> Result<AgentLeaseState> {
        if agent.state == AgentState::Retired {
            return Ok(AgentLeaseState::Retired);
        }
        if agent.state == AgentState::Stale {
            return Ok(AgentLeaseState::Stale);
        }
        let now = parse_rfc3339_utc(now, "now")?;
        let Some(latest_lease) = latest_lease else {
            return Ok(AgentLeaseState::Stale);
        };
        let lease_expires_at =
            parse_rfc3339_utc(&latest_lease.lease_expires_at, "lease expiration")?;
        if lease_expires_at > now {
            Ok(AgentLeaseState::Active)
        } else {
            Ok(AgentLeaseState::Stale)
        }
    }

    fn active_delivery_for_agent(&self, agent_id: &str) -> Result<Option<InboxRow>> {
        let mut rows = Vec::new();
        for delivery in self.latest_deliveries()?.into_values() {
            if delivery.claimed_by.as_deref() == Some(agent_id) && delivery.state.is_open() {
                rows.push(InboxRow::from(delivery));
            }
        }
        rows.sort_by(|left, right| {
            left.updated_at
                .cmp(&right.updated_at)
                .then_with(|| left.delivery_id.cmp(&right.delivery_id))
        });
        Ok(rows.into_iter().next())
    }

    fn pending_open_notifications_for_agent(
        &self,
        agent_id: &str,
        now: &str,
    ) -> Result<Vec<NotificationRecord>> {
        let now = parse_rfc3339_utc(now, "now")?;
        let latest_deliveries = self.latest_deliveries()?;
        let mut notifications = Vec::new();
        for notification in self.latest_notifications()?.into_values() {
            if notification.agent_id != agent_id || notification.state != NotificationState::Pending
            {
                continue;
            }
            let Some(delivery) = latest_deliveries.get(&notification.delivery_id) else {
                continue;
            };
            if !delivery.state.is_open() {
                continue;
            }
            if !InboxRow::from(delivery.clone()).is_due_at(now)? {
                continue;
            }
            notifications.push(notification);
        }
        notifications.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.notification_id.cmp(&right.notification_id))
        });
        Ok(notifications)
    }

    pub fn rebuild_mailbox(&self, recipient: &MailAddress) -> Result<Vec<InboxRow>> {
        self.ensure()?;
        let recipient_canonical = recipient.canonical();
        let mut rows = Vec::new();
        for delivery in self.latest_deliveries()?.into_values() {
            if delivery.recipient == recipient_canonical {
                rows.push(InboxRow {
                    schema_version: 1,
                    delivery_id: delivery.delivery_id,
                    message_id: delivery.message_id,
                    thread_id: delivery.thread_id,
                    recipient: delivery.recipient,
                    state: delivery.state,
                    claimed_by: delivery.claimed_by,
                    snoozed_until: delivery.snoozed_until,
                    updated_at: delivery.updated_at,
                });
            }
        }
        let mailbox_path = self.paths.mailbox_path(recipient);
        write_jsonl(&mailbox_path, &rows)?;
        Ok(rows)
    }

    pub fn recipient_summary(&self, recipient: &MailAddress) -> Result<RecipientMailSummary> {
        let recipient_canonical = recipient.canonical();
        let mut open_delivery_count = 0;
        let mut open_threads = BTreeSet::new();
        for delivery in self.latest_deliveries()?.into_values() {
            if delivery.recipient == recipient_canonical && delivery.state.is_open() {
                open_delivery_count += 1;
                open_threads.insert(delivery.thread_id);
            }
        }
        Ok(RecipientMailSummary {
            recipient: recipient_canonical,
            open_delivery_count,
            open_thread_count: open_threads.len(),
        })
    }

    pub fn mail_inbox(&self, recipient: &MailAddress) -> Result<Vec<InboxRow>> {
        self.rebuild_mailbox(recipient)
    }

    pub fn mail_inbox_due(&self, recipient: &MailAddress, now: &str) -> Result<Vec<InboxRow>> {
        let now = parse_rfc3339_utc(now, "now")?;
        self.mail_inbox(recipient)?
            .into_iter()
            .filter(|row| row.state.is_open())
            .filter_map(|row| match row.is_due_at(now) {
                Ok(true) => Some(Ok(row)),
                Ok(false) => None,
                Err(error) => Some(Err(error)),
            })
            .collect()
    }

    pub fn agent_inbox(&self, agent_id: &str) -> Result<Vec<InboxRow>> {
        let agent = self.require_active_agent(agent_id)?;
        let mut rows_by_delivery = BTreeMap::new();
        let mut addresses = vec![MailAddress::parse(&agent.primary_address)?];
        for role in &agent.granted_roles {
            addresses.push(MailAddress::parse(role)?);
        }
        for address in addresses {
            for row in self.mail_inbox(&address)? {
                if row
                    .claimed_by
                    .as_deref()
                    .is_some_and(|claimed_by| claimed_by != agent.agent_id)
                {
                    continue;
                }
                rows_by_delivery.insert(row.delivery_id.clone(), row);
            }
        }
        Ok(rows_by_delivery.into_values().collect())
    }

    pub fn open_delivery(&self, delivery_id: &str, agent_id: &str) -> Result<OpenDeliveryResult> {
        validate_segment(delivery_id, "delivery id")?;
        validate_segment(agent_id, "agent id")?;
        let agent = self.require_active_agent(agent_id)?;
        let delivery = self
            .latest_delivery(delivery_id)?
            .ok_or_else(|| anyhow!("unknown delivery {delivery_id}"))?;
        self.require_delivery_access(&agent, &delivery)?;
        let message = self.read_message(&delivery.message_id)?;
        let content_delivery = codex_open_content_delivery(&agent)?;
        let message = OpenedMessageSummary {
            body_sha256: message.envelope.body.sha256.clone(),
            body_bytes: message.body_markdown.len(),
            envelope: message.envelope,
        };
        Ok(OpenDeliveryResult {
            delivery,
            message,
            content_delivery,
        })
    }

    pub fn record_mail_injection_result(
        &self,
        delivery_id: &str,
        agent_id: &str,
        thread_id: Option<&str>,
        item_count: usize,
        result: MailInjectionResult,
        occurred_at: &str,
        error: Option<String>,
    ) -> Result<MailInjectionRecordResult> {
        validate_segment(delivery_id, "delivery id")?;
        validate_segment(agent_id, "agent id")?;
        if let Some(thread_id) = thread_id {
            validate_segment(thread_id, "thread id")?;
        }
        parse_rfc3339_utc(occurred_at, "mail injection receipt")?;
        let opened = self.open_delivery(delivery_id, agent_id)?;
        let thread_id = thread_id
            .map(str::to_string)
            .unwrap_or_else(|| opened.content_delivery.thread_id.clone());
        if thread_id != opened.content_delivery.thread_id {
            return Err(anyhow!(
                "mail injection thread {} does not match delivery transport thread {}",
                thread_id,
                opened.content_delivery.thread_id
            ));
        }
        let receipt = MailInjectionReceipt {
            schema_version: 1,
            injection_id: stable_injection_id(delivery_id, agent_id, &thread_id, occurred_at),
            delivery_id: delivery_id.to_string(),
            message_id: opened.message.envelope.message_id.clone(),
            agent_id: agent_id.to_string(),
            thread_id: thread_id.clone(),
            body_sha256: opened.message.body_sha256.clone(),
            item_count,
            app_server_method: opened.content_delivery.method.clone(),
            app_server_result: result.clone(),
            created_at: occurred_at.to_string(),
            error,
        };
        self.append_injection_receipt(&receipt)?;
        let agent_status = self.agent_status_by_thread(&thread_id, occurred_at)?;
        let event = MailControlEvent {
            schema_version: 1,
            control_event_id: stable_control_event_id(
                "mail_injection",
                &receipt.injection_id,
                occurred_at,
            ),
            created_at: occurred_at.to_string(),
            source: MailControlEventSource::CodexAppServer,
            hook_event_name: None,
            codex: MailControlCodexRef {
                thread_id: Some(thread_id),
                tool_name: Some("wiki.mail.open".to_string()),
                ..Default::default()
            },
            agent: MailControlAgentRef {
                agent_id: Some(agent_id.to_string()),
                lease_state: Some(agent_status.lease_state),
                active_delivery_id: agent_status
                    .active_delivery
                    .as_ref()
                    .map(|delivery| delivery.delivery_id.clone()),
            },
            mail_refs: MailControlRefs {
                message_id: Some(receipt.message_id.clone()),
                delivery_id: Some(delivery_id.to_string()),
                ..Default::default()
            },
            decision: Some(MailControlDecision {
                behavior: MailControlDecisionBehavior::RecordOnly,
                reason: Some(match &receipt.app_server_result {
                    MailInjectionResult::Ok => "mail body injected into Codex thread".to_string(),
                    MailInjectionResult::Failed => {
                        "mail body injection failed in Codex host adapter".to_string()
                    }
                }),
            }),
            input_sha256: Some(opened.message.body_sha256),
            output_sha256: Some(sha256_json(&receipt)?),
            additional_context_sha256: None,
        };
        self.append_control_event(&event)?;
        Ok(MailInjectionRecordResult {
            receipt,
            control_event: event,
        })
    }

    pub fn notification_poll(
        &self,
        agent_id: &str,
        _cursor: Option<&str>,
        now: &str,
    ) -> Result<Vec<NotificationRecord>> {
        validate_segment(agent_id, "agent id")?;
        let _lock = self.lock_mutations()?;
        let agent = self.require_live_agent(agent_id, now)?;
        self.ensure_agent_notifications(&agent, now)?;
        self.pending_due_notifications_for_agent(agent_id, now)
    }

    pub fn notification_dispatch_queue(
        &self,
        agent_id: &str,
        _cursor: Option<&str>,
        now: &str,
    ) -> Result<Vec<NotificationRecord>> {
        validate_segment(agent_id, "agent id")?;
        let _lock = self.lock_mutations()?;
        let agent = self.require_non_retired_agent(agent_id)?;
        self.ensure_agent_notifications(&agent, now)?;
        self.pending_due_dispatch_notifications_for_agent(agent_id, now)
    }

    pub fn acknowledge_notification(
        &self,
        agent_id: &str,
        notification_id: &str,
        occurred_at: &str,
    ) -> Result<NotificationRecord> {
        validate_segment(agent_id, "agent id")?;
        validate_segment(notification_id, "notification id")?;
        let _lock = self.lock_mutations()?;
        self.require_live_agent(agent_id, occurred_at)?;
        let mut notification = self
            .latest_notifications()?
            .remove(notification_id)
            .ok_or_else(|| anyhow!("unknown notification {notification_id}"))?;
        if notification.agent_id != agent_id {
            return Err(anyhow!(
                "notification {notification_id} belongs to {}",
                notification.agent_id
            ));
        }
        if notification.state == NotificationState::Acknowledged
            || notification.state == NotificationState::Suppressed
        {
            return Ok(notification);
        }
        notification.state = NotificationState::Acknowledged;
        notification.updated_at = occurred_at.to_string();
        notification.acknowledged_at = Some(occurred_at.to_string());
        self.append_notification(&notification)?;
        Ok(notification)
    }

    pub fn codex_steering_payload(
        &self,
        notification: &NotificationRecord,
    ) -> Result<CodexSteeringPayload> {
        self.codex_steering_payload_for_attempt(notification, None)
    }

    fn codex_steering_payload_for_attempt(
        &self,
        notification: &NotificationRecord,
        transport_attempt_id: Option<String>,
    ) -> Result<CodexSteeringPayload> {
        validate_segment(&notification.agent_id, "agent id")?;
        validate_segment(&notification.notification_id, "notification id")?;
        let latest_notification = self
            .latest_notifications()?
            .remove(&notification.notification_id)
            .ok_or_else(|| anyhow!("unknown notification {}", notification.notification_id))?;
        if latest_notification.state != NotificationState::Pending {
            return Err(anyhow!(
                "notification {} is not pending; current state is {:?}",
                notification.notification_id,
                latest_notification.state
            ));
        }
        let envelope = self.read_message_envelope(&notification.message_id)?;
        let delivery = self
            .latest_delivery(&notification.delivery_id)?
            .ok_or_else(|| anyhow!("unknown delivery {}", notification.delivery_id))?;
        if !delivery.state.is_open() {
            return Err(anyhow!(
                "delivery {} is not dispatchable; current state is {:?}",
                notification.delivery_id,
                delivery.state
            ));
        }
        if delivery
            .claimed_by
            .as_deref()
            .is_some_and(|claimed_by| claimed_by != notification.agent_id)
        {
            return Err(anyhow!(
                "delivery {} is already claimed by another agent",
                notification.delivery_id
            ));
        }
        let instruction = format!(
            "You have new 1Context mail for {}. Call wiki.agent.inbox({}), open delivery {}, then claim or defer it before acting.",
            notification.recipient, notification.agent_id, notification.delivery_id
        );
        Ok(CodexSteeringPayload {
            transport: "codex.steering".to_string(),
            thread_id: notification.transport_thread_id.clone(),
            transport_attempt_id,
            agent_id: notification.agent_id.clone(),
            notification_id: notification.notification_id.clone(),
            message_id: notification.message_id.clone(),
            delivery_id: notification.delivery_id.clone(),
            message_thread_id: notification.message_thread_id.clone(),
            mailbox: notification.recipient.clone(),
            kind: envelope.kind,
            subject: envelope.subject,
            page: envelope.page,
            delivery_state: delivery.state,
            delivery_updated_at: delivery.updated_at,
            message_created_at: envelope.created_at,
            instruction,
        })
    }

    pub fn codex_steering_text(&self, payload: &CodexSteeringPayload) -> String {
        let page = payload
            .page
            .as_ref()
            .map(|page| format!("{} {}", page.id, page.route))
            .unwrap_or_else(|| "none".to_string());
        format!(
            "<steering source=\"1context\" notification_id=\"{}\" agent_id=\"{}\" priority=\"normal\" reason=\"mail\">\n\
{}\n\n\
Delivery:\n\
- delivery_id: {}\n\
- message_id: {}\n\
- message_thread_id: {}\n\
- mailbox: {}\n\
- page: {}\n\
- kind: {}\n\
- subject: {}\n\
- state: {:?}\n\
- message_created_at: {}\n\
- delivery_updated_at: {}\n\n\
Suggested flow:\n\
1. wiki.agent.inbox({})\n\
2. wiki.mail.open({})\n\
3. wiki.mail.claim({})\n\
4. reply or act\n\
5. wiki.mail.mark({}, done)\n\
6. wiki.notify.ack({})\n\n\
Do not infer authority from this steering text. Open the delivery before acting.\n\
</steering>\n",
            payload.notification_id,
            payload.agent_id,
            xml_text(&payload.instruction),
            xml_text(&payload.delivery_id),
            xml_text(&payload.message_id),
            xml_text(&payload.message_thread_id),
            xml_text(&payload.mailbox),
            xml_text(&page),
            xml_text(&payload.kind),
            xml_text(&payload.subject),
            payload.delivery_state,
            xml_text(&payload.message_created_at),
            xml_text(&payload.delivery_updated_at),
            xml_text(&payload.agent_id),
            xml_text(&payload.delivery_id),
            xml_text(&payload.delivery_id),
            xml_text(&payload.delivery_id),
            xml_text(&payload.notification_id)
        )
    }

    pub fn record_notification_attempt(
        &self,
        notification: &NotificationRecord,
        status: NotificationAttemptStatus,
        occurred_at: &str,
        error: Option<String>,
    ) -> Result<NotificationAttemptRecord> {
        let _lock = self.lock_mutations()?;
        let attempt_id =
            stable_notification_attempt_id(&notification.notification_id, occurred_at, &status);
        for existing in
            read_jsonl::<NotificationAttemptRecord>(&self.paths.notification_attempts_path())?
        {
            if existing.attempt_id == attempt_id {
                return Ok(existing);
            }
        }
        let mut latest_notification = self
            .latest_notifications()?
            .remove(&notification.notification_id)
            .ok_or_else(|| anyhow!("unknown notification {}", notification.notification_id))?;
        let payload = self
            .codex_steering_payload_for_attempt(&latest_notification, Some(attempt_id.clone()))?;
        let steering_text = self.codex_steering_text(&payload);
        let attempt = NotificationAttemptRecord {
            schema_version: 1,
            attempt_id,
            notification_id: notification.notification_id.clone(),
            agent_id: notification.agent_id.clone(),
            transport: "codex.steering".to_string(),
            status,
            occurred_at: occurred_at.to_string(),
            payload,
            steering_text,
            error,
        };
        self.append_notification_attempt(&attempt)?;
        latest_notification.attempt_count += 1;
        latest_notification.last_attempt_at = Some(occurred_at.to_string());
        latest_notification.updated_at = occurred_at.to_string();
        latest_notification.last_error = attempt.error.clone();
        latest_notification.next_dispatch_at = match &attempt.status {
            NotificationAttemptStatus::Failed => Some(default_next_dispatch_at(occurred_at)?),
            NotificationAttemptStatus::Sent | NotificationAttemptStatus::DryRun => None,
        };
        self.append_notification(&latest_notification)?;
        Ok(attempt)
    }

    pub fn read_message(&self, message_id: &str) -> Result<HydratedMessage> {
        let envelope = self.read_message_envelope(message_id)?;
        let body_path = self
            .paths
            .body_path(&envelope.message_id, &envelope.created_at)?;
        let body_markdown = fs::read_to_string(&body_path)
            .with_context(|| format!("read body {}", body_path.display()))?;
        Ok(HydratedMessage {
            envelope,
            body_markdown,
        })
    }

    fn read_message_envelope(&self, message_id: &str) -> Result<MessageEnvelope> {
        validate_segment(message_id, "message id")?;
        let message_path = self
            .find_message_path(message_id)?
            .ok_or_else(|| anyhow!("unknown message {message_id}"))?;
        serde_json::from_reader(
            File::open(&message_path)
                .with_context(|| format!("open {}", message_path.display()))?,
        )
        .with_context(|| format!("parse message {}", message_path.display()))
    }

    pub fn read_thread(&self, thread_id: &str) -> Result<Vec<HydratedMessage>> {
        validate_segment(thread_id, "thread id")?;
        let mut message_ids = BTreeSet::new();
        for delivery in self.latest_deliveries()?.into_values() {
            if delivery.thread_id == thread_id {
                message_ids.insert(delivery.message_id);
            }
        }
        message_ids
            .into_iter()
            .map(|message_id| self.read_message(&message_id))
            .collect()
    }

    pub fn claim_delivery(
        &self,
        delivery_id: &str,
        agent_id: &str,
        occurred_at: &str,
    ) -> Result<DeliveryRecord> {
        validate_segment(delivery_id, "delivery id")?;
        validate_segment(agent_id, "agent id")?;
        let _lock = self.lock_mutations()?;
        let agent = self.require_live_agent(agent_id, occurred_at)?;
        let delivery = self
            .latest_delivery(delivery_id)?
            .ok_or_else(|| anyhow!("unknown delivery {delivery_id}"))?;
        self.require_delivery_access(&agent, &delivery)?;
        if delivery.state.is_terminal() {
            return Err(anyhow!("delivery {delivery_id} is already terminal"));
        }
        if delivery.state == DeliveryState::Claimed
            && delivery.claimed_by.as_deref() == Some(agent_id)
        {
            return Ok(delivery);
        }
        if let Some(claimed_by) = &delivery.claimed_by {
            if claimed_by != agent_id {
                return Err(anyhow!(
                    "delivery {delivery_id} already claimed by {claimed_by}"
                ));
            }
        }
        self.transition_delivery(
            delivery,
            Some(agent_id.to_string()),
            agent_id,
            DeliveryState::Claimed,
            occurred_at,
        )
    }

    pub fn mark_delivery(
        &self,
        delivery_id: &str,
        agent_id: &str,
        state: DeliveryState,
        occurred_at: &str,
    ) -> Result<DeliveryRecord> {
        validate_segment(delivery_id, "delivery id")?;
        validate_segment(agent_id, "agent id")?;
        let _lock = self.lock_mutations()?;
        let agent = self.require_live_agent(agent_id, occurred_at)?;
        if !state.is_mark_state() {
            return Err(anyhow!("unsupported delivery mark state: {state:?}"));
        }
        let delivery = self
            .latest_delivery(delivery_id)?
            .ok_or_else(|| anyhow!("unknown delivery {delivery_id}"))?;
        self.require_delivery_access(&agent, &delivery)?;
        if delivery.state.is_terminal() {
            if delivery.state == state && delivery.claimed_by.as_deref() == Some(agent_id) {
                return Ok(delivery);
            }
            return Err(anyhow!("delivery {delivery_id} is already terminal"));
        }
        let claimed_by = delivery.claimed_by.clone();
        if let Some(claimed_by) = &claimed_by {
            if claimed_by != agent_id {
                return Err(anyhow!(
                    "delivery {delivery_id} already claimed by {claimed_by}"
                ));
            }
        } else if state.is_terminal() {
            return Err(anyhow!(
                "delivery {delivery_id} must be claimed by {agent_id} before marking {state:?}"
            ));
        }
        let next_claimed_by = if state == DeliveryState::Read {
            claimed_by
        } else {
            Some(agent_id.to_string())
        };
        self.transition_delivery(delivery, next_claimed_by, agent_id, state, occurred_at)
    }

    pub fn snooze_delivery(
        &self,
        delivery_id: &str,
        agent_id: &str,
        snoozed_until: &str,
        occurred_at: &str,
    ) -> Result<DeliveryRecord> {
        parse_rfc3339_utc(snoozed_until, "snooze due")?;
        validate_segment(delivery_id, "delivery id")?;
        validate_segment(agent_id, "agent id")?;
        let _lock = self.lock_mutations()?;
        let agent = self.require_live_agent(agent_id, occurred_at)?;
        let delivery = self
            .latest_delivery(delivery_id)?
            .ok_or_else(|| anyhow!("unknown delivery {delivery_id}"))?;
        self.require_delivery_access(&agent, &delivery)?;
        if delivery.state.is_terminal() {
            return Err(anyhow!("delivery {delivery_id} is already terminal"));
        }
        if let Some(claimed_by) = &delivery.claimed_by {
            if claimed_by != agent_id {
                return Err(anyhow!(
                    "delivery {delivery_id} already claimed by {claimed_by}"
                ));
            }
        }
        self.transition_delivery_with_snooze(
            delivery,
            Some(agent_id.to_string()),
            agent_id,
            DeliveryState::Snoozed,
            Some(snoozed_until.to_string()),
            occurred_at,
        )
    }

    fn transition_delivery(
        &self,
        delivery: DeliveryRecord,
        claimed_by: Option<String>,
        actor_agent_id: &str,
        state: DeliveryState,
        occurred_at: &str,
    ) -> Result<DeliveryRecord> {
        self.transition_delivery_with_snooze(
            delivery,
            claimed_by,
            actor_agent_id,
            state,
            None,
            occurred_at,
        )
    }

    fn transition_delivery_with_snooze(
        &self,
        delivery: DeliveryRecord,
        claimed_by: Option<String>,
        actor_agent_id: &str,
        state: DeliveryState,
        snoozed_until: Option<String>,
        occurred_at: &str,
    ) -> Result<DeliveryRecord> {
        let updated = DeliveryRecord {
            state: state.clone(),
            claimed_by,
            snoozed_until,
            updated_at: occurred_at.to_string(),
            ..delivery
        };
        self.append_delivery(&updated)?;
        self.append_claim(&ClaimEvent {
            schema_version: 1,
            delivery_id: updated.delivery_id.clone(),
            message_id: updated.message_id.clone(),
            thread_id: updated.thread_id.clone(),
            recipient: updated.recipient.clone(),
            agent_id: actor_agent_id.to_string(),
            state,
            occurred_at: occurred_at.to_string(),
        })?;
        if updated.claimed_by.as_deref() == Some(actor_agent_id) {
            self.suppress_competing_notifications(
                &updated.delivery_id,
                actor_agent_id,
                occurred_at,
            )?;
        }
        Ok(updated)
    }

    fn latest_delivery(&self, delivery_id: &str) -> Result<Option<DeliveryRecord>> {
        Ok(self.latest_deliveries()?.remove(delivery_id))
    }

    fn suppress_competing_notifications(
        &self,
        delivery_id: &str,
        winning_agent_id: &str,
        occurred_at: &str,
    ) -> Result<()> {
        for mut notification in self.latest_notifications()?.into_values() {
            if notification.delivery_id != delivery_id
                || notification.agent_id == winning_agent_id
                || notification.state != NotificationState::Pending
            {
                continue;
            }
            notification.state = NotificationState::Suppressed;
            notification.updated_at = occurred_at.to_string();
            self.append_notification(&notification)?;
        }
        Ok(())
    }

    fn ensure_delivery_notifications(&self, delivery: &DeliveryRecord) -> Result<()> {
        let mut latest_notifications = self.latest_notifications()?;
        for agent in self.notification_targets_for_delivery(delivery)? {
            let notification_id = stable_notification_id(&delivery.delivery_id, &agent.agent_id);
            if latest_notifications.contains_key(&notification_id) {
                continue;
            }
            let notification = NotificationRecord {
                schema_version: 1,
                notification_id: notification_id.clone(),
                agent_id: agent.agent_id.clone(),
                agent_address: agent.primary_address.clone(),
                transport: agent.transport.kind.clone(),
                transport_thread_id: agent.transport.thread_id.clone(),
                delivery_id: delivery.delivery_id.clone(),
                message_id: delivery.message_id.clone(),
                message_thread_id: delivery.thread_id.clone(),
                recipient: delivery.recipient.clone(),
                state: NotificationState::Pending,
                created_at: delivery.created_at.clone(),
                updated_at: delivery.created_at.clone(),
                acknowledged_at: None,
                attempt_count: 0,
                last_attempt_at: None,
                next_dispatch_at: None,
                last_error: None,
            };
            self.append_notification(&notification)?;
            latest_notifications.insert(notification_id, notification);
        }
        Ok(())
    }

    fn ensure_agent_notifications(&self, agent: &AgentRecord, occurred_at: &str) -> Result<()> {
        let latest_notifications = self.latest_notifications()?;
        for delivery in self.latest_deliveries()?.into_values() {
            if !delivery.state.is_open() || !delivery_actionable_for_agent(agent, &delivery) {
                continue;
            }
            let notification_id = stable_notification_id(&delivery.delivery_id, &agent.agent_id);
            if latest_notifications.contains_key(&notification_id) {
                continue;
            }
            self.append_notification(&NotificationRecord {
                schema_version: 1,
                notification_id,
                agent_id: agent.agent_id.clone(),
                agent_address: agent.primary_address.clone(),
                transport: agent.transport.kind.clone(),
                transport_thread_id: agent.transport.thread_id.clone(),
                delivery_id: delivery.delivery_id.clone(),
                message_id: delivery.message_id.clone(),
                message_thread_id: delivery.thread_id.clone(),
                recipient: delivery.recipient.clone(),
                state: NotificationState::Pending,
                created_at: occurred_at.to_string(),
                updated_at: occurred_at.to_string(),
                acknowledged_at: None,
                attempt_count: 0,
                last_attempt_at: None,
                next_dispatch_at: None,
                last_error: None,
            })?;
        }
        Ok(())
    }

    fn notification_targets_for_delivery(
        &self,
        delivery: &DeliveryRecord,
    ) -> Result<Vec<AgentRecord>> {
        if !delivery.state.is_open() {
            return Ok(Vec::new());
        }
        let mut targets = Vec::new();
        for agent in self.latest_agent_records()?.into_values() {
            if agent.state == AgentState::Retired {
                continue;
            }
            if !delivery_actionable_for_agent(&agent, delivery) {
                continue;
            }
            targets.push(agent);
        }
        Ok(targets)
    }

    fn pending_due_notifications_for_agent(
        &self,
        agent_id: &str,
        now: &str,
    ) -> Result<Vec<NotificationRecord>> {
        let now = parse_rfc3339_utc(now, "now")?;
        let latest_deliveries = self.latest_deliveries()?;
        let mut notifications = Vec::new();
        for notification in self.latest_notifications()?.into_values() {
            if notification.agent_id != agent_id || notification.state != NotificationState::Pending
            {
                continue;
            }
            let Some(delivery) = latest_deliveries.get(&notification.delivery_id) else {
                continue;
            };
            if !delivery.state.is_open() {
                continue;
            }
            if delivery
                .claimed_by
                .as_deref()
                .is_some_and(|claimed_by| claimed_by != notification.agent_id)
            {
                continue;
            }
            if !InboxRow::from(delivery.clone()).is_due_at(now)? {
                continue;
            }
            notifications.push(notification);
        }
        notifications.sort_by(|left, right| {
            left.created_at
                .cmp(&right.created_at)
                .then_with(|| left.notification_id.cmp(&right.notification_id))
        });
        Ok(notifications)
    }

    fn pending_due_dispatch_notifications_for_agent(
        &self,
        agent_id: &str,
        now: &str,
    ) -> Result<Vec<NotificationRecord>> {
        let now_utc = parse_rfc3339_utc(now, "now")?;
        let mut notifications = self.pending_due_notifications_for_agent(agent_id, now)?;
        notifications.retain(|notification| {
            if notification.attempt_count == 0 {
                return true;
            }
            notification
                .next_dispatch_at
                .as_deref()
                .and_then(|next_dispatch_at| {
                    parse_rfc3339_utc(next_dispatch_at, "next dispatch").ok()
                })
                .is_some_and(|next_dispatch_at| next_dispatch_at <= now_utc)
        });
        Ok(notifications)
    }

    fn require_active_agent(&self, agent_id: &str) -> Result<AgentRecord> {
        let Some(record) = self.latest_agent_record(agent_id)? else {
            return Err(anyhow!("unknown agent {agent_id}"));
        };
        if record.state != AgentState::Active {
            return Err(anyhow!("agent {agent_id} is not active"));
        }
        Ok(record)
    }

    fn require_non_retired_agent(&self, agent_id: &str) -> Result<AgentRecord> {
        let Some(record) = self.latest_agent_record(agent_id)? else {
            return Err(anyhow!("unknown agent {agent_id}"));
        };
        if record.state == AgentState::Retired {
            return Err(anyhow!("agent {agent_id} is retired"));
        }
        Ok(record)
    }

    fn require_live_agent(&self, agent_id: &str, now: &str) -> Result<AgentRecord> {
        let record = self.require_active_agent(agent_id)?;
        let eligibility = self.agent_push_eligibility(agent_id, now)?;
        if eligibility.eligible {
            return Ok(record);
        }
        match eligibility.reason {
            AgentPushEligibilityReason::StaleLease => {
                let expired_at = eligibility
                    .latest_lease
                    .as_ref()
                    .map(|lease| lease.lease_expires_at.as_str())
                    .unwrap_or("unknown");
                Err(anyhow!(
                    "agent {agent_id} has stale lease expired at {expired_at}"
                ))
            }
            AgentPushEligibilityReason::RetiredAgent => Err(anyhow!("agent {agent_id} is retired")),
            AgentPushEligibilityReason::ActiveLease => Ok(record),
        }
    }

    fn require_delivery_access(
        &self,
        agent: &AgentRecord,
        delivery: &DeliveryRecord,
    ) -> Result<()> {
        if delivery_visible_to_agent(agent, delivery) {
            return Ok(());
        }
        Err(anyhow!(
            "agent {} cannot access delivery {} for {}",
            agent.agent_id,
            delivery.delivery_id,
            delivery.recipient
        ))
    }

    fn latest_deliveries(&self) -> Result<BTreeMap<String, DeliveryRecord>> {
        let mut latest = BTreeMap::new();
        for delivery in read_jsonl::<DeliveryRecord>(&self.paths.deliveries_path())? {
            latest.insert(delivery.delivery_id.clone(), delivery);
        }
        Ok(latest)
    }

    fn latest_notifications(&self) -> Result<BTreeMap<String, NotificationRecord>> {
        let mut latest = BTreeMap::new();
        for notification in
            read_jsonl::<NotificationRecord>(&self.paths.notification_outbox_path())?
        {
            latest.insert(notification.notification_id.clone(), notification);
        }
        Ok(latest)
    }

    fn latest_agent_records(&self) -> Result<BTreeMap<String, AgentRecord>> {
        let mut latest = BTreeMap::new();
        for record in read_jsonl::<AgentRecord>(&self.paths.agents_path())? {
            latest.insert(record.agent_id.clone(), record);
        }
        Ok(latest)
    }

    fn find_message_path(&self, message_id: &str) -> Result<Option<PathBuf>> {
        let root = self.paths.mail.join("messages");
        if !root.is_dir() {
            return Ok(None);
        }
        find_named_file(&root, &format!("{message_id}.json"))
    }
}

impl DeliveryState {
    fn is_open(&self) -> bool {
        matches!(
            self,
            Self::Unread | Self::Read | Self::Claimed | Self::Snoozed
        )
    }

    fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Done | Self::Archived | Self::Rejected | Self::DeadLetter
        )
    }

    fn is_mark_state(&self) -> bool {
        matches!(
            self,
            Self::Read | Self::Done | Self::Archived | Self::Rejected
        )
    }
}

impl InboxRow {
    fn is_due_at(&self, now: DateTime<Utc>) -> Result<bool> {
        if self.state != DeliveryState::Snoozed {
            return Ok(true);
        }
        let Some(snoozed_until) = &self.snoozed_until else {
            return Ok(true);
        };
        Ok(parse_rfc3339_utc(snoozed_until, "snooze due")? <= now)
    }
}

impl From<DeliveryRecord> for InboxRow {
    fn from(delivery: DeliveryRecord) -> Self {
        Self {
            schema_version: 1,
            delivery_id: delivery.delivery_id,
            message_id: delivery.message_id,
            thread_id: delivery.thread_id,
            recipient: delivery.recipient,
            state: delivery.state,
            claimed_by: delivery.claimed_by,
            snoozed_until: delivery.snoozed_until,
            updated_at: delivery.updated_at,
        }
    }
}

fn validate_segment(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
        || value.contains('\0')
        || value.chars().any(char::is_whitespace)
        || !value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
    {
        return Err(anyhow!("invalid {label}: {value:?}"));
    }
    Ok(())
}

fn validate_dotted_name(value: &str, label: &str) -> Result<()> {
    if value.is_empty() {
        return Err(anyhow!("invalid {label}: {value:?}"));
    }
    for segment in value.split('.') {
        validate_segment(segment, label)?;
    }
    Ok(())
}

fn year_month(created_at: &str) -> Result<(String, String)> {
    let date = created_at
        .get(0..7)
        .ok_or_else(|| anyhow!("invalid timestamp {created_at:?}; expected YYYY-MM"))?;
    let Some((year, month)) = date.split_once('-') else {
        return Err(anyhow!(
            "invalid timestamp {created_at:?}; expected YYYY-MM"
        ));
    };
    if year.len() != 4 || month.len() != 2 {
        return Err(anyhow!(
            "invalid timestamp {created_at:?}; expected YYYY-MM"
        ));
    }
    validate_segment(year, "timestamp year")?;
    validate_segment(month, "timestamp month")?;
    Ok((year.to_string(), month.to_string()))
}

fn parse_rfc3339_utc(value: &str, label: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(value)
        .with_context(|| format!("parse {label} timestamp {value:?}"))?
        .with_timezone(&Utc))
}

fn default_next_dispatch_at(occurred_at: &str) -> Result<String> {
    Ok(
        (parse_rfc3339_utc(occurred_at, "notification attempt")? + Duration::seconds(60))
            .to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    )
}

fn codex_open_content_delivery(agent: &AgentRecord) -> Result<OpenContentDelivery> {
    Ok(OpenContentDelivery {
        schema_version: 1,
        transport: "codex.thread.inject_items".to_string(),
        method: "thread/inject_items".to_string(),
        status: "requires_host_injection".to_string(),
        thread_id: agent.transport.thread_id.clone(),
        items: Vec::new(),
    })
}

fn write_json_file<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path).with_context(|| format!("create {}", path.display()))?;
    serde_json::to_writer_pretty(&mut file, value)
        .with_context(|| format!("write JSON {}", path.display()))?;
    file.write_all(b"\n")?;
    Ok(())
}

fn write_text_file(path: &Path, value: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, value).with_context(|| format!("write {}", path.display()))
}

fn append_jsonl<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;
    serde_json::to_writer(&mut file, value)
        .with_context(|| format!("append JSONL {}", path.display()))?;
    file.write_all(b"\n")?;
    Ok(())
}

fn write_jsonl<T: Serialize>(path: &Path, values: &[T]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(path).with_context(|| format!("create {}", path.display()))?;
    for value in values {
        serde_json::to_writer(&mut file, value)
            .with_context(|| format!("write JSONL {}", path.display()))?;
        file.write_all(b"\n")?;
    }
    Ok(())
}

fn read_jsonl<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Vec<T>> {
    if !path.is_file() {
        return Ok(Vec::new());
    }
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mut values = Vec::new();
    for (index, line) in BufReader::new(file).lines().enumerate() {
        let line =
            line.with_context(|| format!("read line {} in {}", index + 1, path.display()))?;
        if line.trim().is_empty() {
            continue;
        }
        values
            .push(serde_json::from_str(&line).with_context(|| {
                format!("parse JSONL line {} in {}", index + 1, path.display())
            })?);
    }
    Ok(values)
}

fn xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn find_named_file(root: &Path, filename: &str) -> Result<Option<PathBuf>> {
    for entry in fs::read_dir(root).with_context(|| format!("read directory {}", root.display()))? {
        let entry = entry.with_context(|| format!("read entry in {}", root.display()))?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_named_file(&path, filename)? {
                return Ok(Some(found));
            }
        } else if path.file_name().and_then(|name| name.to_str()) == Some(filename) {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn message_payload_sha256(envelope: &MessageEnvelope, body_markdown: &str) -> Result<String> {
    let mut hash = Sha256::new();
    let mut idempotent_envelope = envelope.clone();
    idempotent_envelope.message_id.clear();
    idempotent_envelope.created_at.clear();
    let envelope_json = serde_json::to_vec(&idempotent_envelope)?;
    hash.update(&envelope_json);
    hash.update(b"\n");
    hash.update(body_markdown.as_bytes());
    Ok(hex_lower(&hash.finalize()))
}

fn sha256_json<T: Serialize>(value: &T) -> Result<String> {
    let bytes = serde_json::to_vec(value)?;
    let mut hash = Sha256::new();
    hash.update(bytes);
    Ok(hex_lower(&hash.finalize()))
}

fn normalized_recipients(envelope: &MessageEnvelope) -> Result<Vec<String>> {
    let mut recipients = BTreeSet::new();
    for value in envelope.to.iter().chain(envelope.cc.iter()) {
        recipients.insert(MailAddress::parse(value)?.canonical());
    }
    Ok(recipients.into_iter().collect())
}

fn stable_agent_id(transport_kind: &str, thread_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(transport_kind.as_bytes());
    hash.update(b":");
    hash.update(thread_id.as_bytes());
    let digest = hex_lower(&hash.finalize());
    format!("agent_{transport_kind}_{}", &digest[..16])
}

fn stable_delivery_id(message_id: &str, recipient: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(message_id.as_bytes());
    hash.update(b":");
    hash.update(recipient.as_bytes());
    let digest = hex_lower(&hash.finalize());
    format!("delivery_{}", &digest[..16])
}

fn stable_notification_id(delivery_id: &str, agent_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(delivery_id.as_bytes());
    hash.update(b":");
    hash.update(agent_id.as_bytes());
    let digest = hex_lower(&hash.finalize());
    format!("notif_{}", &digest[..16])
}

fn stable_notification_attempt_id(
    notification_id: &str,
    occurred_at: &str,
    status: &NotificationAttemptStatus,
) -> String {
    let mut hash = Sha256::new();
    hash.update(notification_id.as_bytes());
    hash.update(b":");
    hash.update(occurred_at.as_bytes());
    hash.update(b":");
    hash.update(format!("{status:?}").as_bytes());
    let digest = hex_lower(&hash.finalize());
    format!("notif_attempt_{}", &digest[..16])
}

fn stable_injection_id(
    delivery_id: &str,
    agent_id: &str,
    thread_id: &str,
    occurred_at: &str,
) -> String {
    let mut hash = Sha256::new();
    hash.update(delivery_id.as_bytes());
    hash.update(b":");
    hash.update(agent_id.as_bytes());
    hash.update(b":");
    hash.update(thread_id.as_bytes());
    hash.update(b":");
    hash.update(occurred_at.as_bytes());
    let digest = hex_lower(&hash.finalize());
    format!("mail_injection_{}", &digest[..16])
}

fn stable_control_event_id(kind: &str, subject_id: &str, occurred_at: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(kind.as_bytes());
    hash.update(b":");
    hash.update(subject_id.as_bytes());
    hash.update(b":");
    hash.update(occurred_at.as_bytes());
    let digest = hex_lower(&hash.finalize());
    format!("mail_control_{}", &digest[..16])
}

fn delivery_visible_to_agent(agent: &AgentRecord, delivery: &DeliveryRecord) -> bool {
    delivery.recipient == agent.primary_address
        || agent
            .granted_roles
            .iter()
            .any(|role| role == &delivery.recipient)
}

fn delivery_actionable_for_agent(agent: &AgentRecord, delivery: &DeliveryRecord) -> bool {
    delivery_visible_to_agent(agent, delivery)
        && delivery
            .claimed_by
            .as_deref()
            .map_or(true, |claimed_by| claimed_by == agent.agent_id)
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
    use std::sync::{Arc, Barrier};
    use std::thread;
    use tempfile::TempDir;

    #[test]
    fn parses_and_canonicalizes_v0_addresses() {
        assert_eq!(
            MailAddress::parse("agent://codex/019e3f72")
                .unwrap()
                .canonical(),
            "agent://codex/019e3f72"
        );
        assert_eq!(
            MailAddress::parse("role://topics.curator")
                .unwrap()
                .canonical(),
            "role://topics.curator"
        );
        assert_eq!(
            MailAddress::parse("mailbox://page/topics")
                .unwrap()
                .canonical(),
            "mailbox://page/topics"
        );
        assert_eq!(
            MailAddress::parse("page://topics").unwrap().canonical(),
            "mailbox://page/topics"
        );
        assert_eq!(
            MailAddress::parse("system://wiki.render")
                .unwrap()
                .canonical(),
            "system://wiki.render"
        );
    }

    #[test]
    fn rejects_path_traversal_and_unsafe_addresses() {
        for value in [
            "",
            "page://../topics",
            "page://topic/one",
            "agent://codex/../../x",
            "role://topics.curator/admin",
            "system://wiki render",
        ] {
            assert!(MailAddress::parse(value).is_err(), "{value} should fail");
        }
    }

    #[test]
    fn mailbox_keys_are_safe_path_segments() {
        let address = MailAddress::parse("role://topics.curator").unwrap();
        let key = address.mailbox_key();
        assert!(key.starts_with("addr-"));
        assert!(key
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-'));
        assert!(!key.contains('/'));
        assert!(!key.contains(".."));
    }

    #[test]
    fn writes_messages_deliveries_and_rebuilds_mailbox_indexes() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        let envelope = MessageEnvelope {
            schema_version: 1,
            message_id: "mailmsg_001".to_string(),
            idempotency_key: "topics-curator-001".to_string(),
            kind: "proposal".to_string(),
            subject: "Review topics".to_string(),
            from: "agent://codex/019e3f72".to_string(),
            to: vec![recipient.canonical()],
            cc: Vec::new(),
            page: Some(MessagePageRef {
                id: "topics".to_string(),
                route: "/topics".to_string(),
            }),
            thread_id: "thread_001".to_string(),
            reply_to: None,
            body: MessageBodyRef {
                format: "markdown".to_string(),
                sha256: "bodysha".to_string(),
            },
            attachments: Vec::new(),
            created_at: "2026-05-21T07:00:00Z".to_string(),
        };
        store
            .write_message(&envelope, "Please review topics.")
            .unwrap();
        store
            .append_delivery(&DeliveryRecord {
                schema_version: 1,
                delivery_id: "delivery_001".to_string(),
                message_id: envelope.message_id.clone(),
                thread_id: envelope.thread_id.clone(),
                recipient: recipient.canonical(),
                state: DeliveryState::Unread,
                claimed_by: None,
                snoozed_until: None,
                created_at: envelope.created_at.clone(),
                updated_at: envelope.created_at.clone(),
            })
            .unwrap();

        let rows = store.rebuild_mailbox(&recipient).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_id, "mailmsg_001");
        assert_eq!(rows[0].state, DeliveryState::Unread);

        assert!(store
            .paths()
            .message_path("mailmsg_001", &envelope.created_at)
            .unwrap()
            .is_file());
        assert!(store
            .paths()
            .body_path("mailmsg_001", &envelope.created_at)
            .unwrap()
            .is_file());
        assert!(store.paths().mailbox_path(&recipient).is_file());
    }

    #[test]
    fn idempotency_accepts_same_payload_and_rejects_different_payload() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_001", "same-key");

        assert_eq!(
            store.accept_message(&envelope, "same body").unwrap(),
            MessageAcceptance::Accepted
        );
        assert_eq!(
            store.accept_message(&envelope, "same body").unwrap(),
            MessageAcceptance::DuplicateSamePayload {
                message_id: "mailmsg_001".to_string()
            }
        );
        assert!(store.accept_message(&envelope, "different body").is_err());
    }

    #[test]
    fn identify_agent_is_deterministic_and_grants_only_policy_allowed_roles() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let request = AgentIdentifyRequest {
            transport_kind: "codex".to_string(),
            thread_id: "019e3f72-3471-7da1-92a8-56e5d25aaa01".to_string(),
            requested_roles: vec![
                "role://topics.curator".to_string(),
                "role://projects.curator".to_string(),
            ],
            requested_capabilities: vec!["wiki.mail".to_string(), "wiki.governance".to_string()],
            lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
            occurred_at: "2026-05-21T07:00:00Z".to_string(),
        };
        let policy = AgentGrantPolicy::allow_exact(&["role://topics.curator"], &["wiki.mail"]);

        let first = store.identify_agent(&request, &policy).unwrap();
        let second = store.identify_agent(&request, &policy).unwrap();

        assert_eq!(first.agent_id, second.agent_id);
        assert_eq!(first.granted_roles, vec!["role://topics.curator"]);
        assert_eq!(first.requested_roles.len(), 2);
        assert_eq!(first.granted_capabilities, vec!["wiki.mail"]);
        assert_eq!(first.requested_capabilities.len(), 2);
        assert!(store.paths().agents_path().is_file());
        assert!(store.paths().leases_path().is_file());
    }

    #[test]
    fn identify_agent_rejects_self_assigned_non_role_addresses() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let request = AgentIdentifyRequest {
            transport_kind: "codex".to_string(),
            thread_id: "019e3f72".to_string(),
            requested_roles: vec!["agent://codex/someone-else".to_string()],
            requested_capabilities: Vec::new(),
            lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
            occurred_at: "2026-05-21T07:00:00Z".to_string(),
        };

        assert!(store
            .identify_agent(&request, &AgentGrantPolicy::default())
            .is_err());
    }

    #[test]
    fn heartbeat_status_and_retire_preserve_agent_lifecycle() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let request = AgentIdentifyRequest {
            transport_kind: "codex".to_string(),
            thread_id: "019e3f72-3471-7da1-92a8-56e5d25aaa01".to_string(),
            requested_roles: Vec::new(),
            requested_capabilities: vec!["wiki.mail".to_string()],
            lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
            occurred_at: "2026-05-21T07:00:00Z".to_string(),
        };
        let agent = store
            .identify_agent(
                &request,
                &AgentGrantPolicy::allow_exact(&[], &["wiki.mail"]),
            )
            .unwrap();

        store
            .heartbeat_agent(
                &agent.agent_id,
                "2026-05-21T08:30:00Z",
                "2026-05-21T07:30:00Z",
            )
            .unwrap();
        let status = store.agent_status(&agent.agent_id).unwrap();
        assert_eq!(status.record.state, AgentState::Active);
        assert_eq!(
            status.latest_lease.unwrap().lease_expires_at,
            "2026-05-21T08:30:00Z"
        );

        let retired = store
            .retire_agent(
                &agent.agent_id,
                "2026-05-21T07:45:00Z",
                "2026-05-21T07:45:00Z",
            )
            .unwrap();
        assert_eq!(retired.state, AgentState::Retired);
        assert!(store
            .heartbeat_agent(
                &agent.agent_id,
                "2026-05-21T09:00:00Z",
                "2026-05-21T08:00:00Z",
            )
            .is_err());
        assert!(store
            .identify_agent(
                &request,
                &AgentGrantPolicy::allow_exact(&[], &["wiki.mail"]),
            )
            .is_err());
    }

    #[test]
    fn stale_lease_blocks_push_eligibility_but_mail_still_delivers() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let request = AgentIdentifyRequest {
            transport_kind: "codex".to_string(),
            thread_id: "019e3f72-3471-7da1-92a8-56e5d25aaa01".to_string(),
            requested_roles: Vec::new(),
            requested_capabilities: vec!["wiki.mail".to_string()],
            lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
            occurred_at: "2026-05-21T07:00:00Z".to_string(),
        };
        let agent = store
            .identify_agent(
                &request,
                &AgentGrantPolicy::allow_exact(&[], &["wiki.mail"]),
            )
            .unwrap();

        let eligibility = store
            .agent_push_eligibility(&agent.agent_id, "2026-05-21T09:00:00Z")
            .unwrap();
        assert!(!eligibility.eligible);
        assert_eq!(eligibility.reason, AgentPushEligibilityReason::StaleLease);

        let mut envelope = sample_envelope("mailmsg_stale_001", "stale-key-001");
        envelope.to = vec![agent.primary_address.clone()];
        envelope.cc = Vec::new();
        let receipt = store
            .send_mail(
                &envelope,
                "Durable mail survives stale push eligibility.",
                &SendMailOptions::default(),
            )
            .unwrap();

        assert_eq!(receipt.attempts[0].status, DeliveryAttemptStatus::Delivered);
        let notifications = store.latest_notifications().unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications.values().next().unwrap().agent_id,
            agent.agent_id
        );
        let recipient = MailAddress::parse(&agent.primary_address).unwrap();
        assert_eq!(
            store
                .recipient_summary(&recipient)
                .unwrap()
                .open_delivery_count,
            1
        );
        assert_eq!(store.rebuild_mailbox(&recipient).unwrap().len(), 1);
    }

    #[test]
    fn appends_agent_lease_claim_and_dead_letter_ledgers() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        store
            .append_agent(&AgentRecord {
                schema_version: 1,
                agent_id: "agent_codex_019e3f72".to_string(),
                primary_address: "agent://codex/019e3f72".to_string(),
                transport: AgentTransport {
                    kind: "codex-thread".to_string(),
                    thread_id: "019e3f72-3471-7da1-92a8-56e5d25aaa01".to_string(),
                },
                requested_roles: vec!["role://topics.curator".to_string()],
                granted_roles: Vec::new(),
                requested_capabilities: vec!["wiki.mail".to_string()],
                granted_capabilities: Vec::new(),
                lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
                state: AgentState::Active,
            })
            .unwrap();
        store
            .append_lease(&AgentLeaseRecord {
                schema_version: 1,
                agent_id: "agent_codex_019e3f72".to_string(),
                lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
                occurred_at: "2026-05-21T07:00:00Z".to_string(),
            })
            .unwrap();
        store
            .append_claim(&ClaimEvent {
                schema_version: 1,
                delivery_id: "delivery_001".to_string(),
                message_id: "mailmsg_001".to_string(),
                thread_id: "thread_001".to_string(),
                recipient: "role://topics.curator".to_string(),
                agent_id: "agent_codex_019e3f72".to_string(),
                state: DeliveryState::Claimed,
                occurred_at: "2026-05-21T07:00:00Z".to_string(),
            })
            .unwrap();
        store
            .append_dead_letter(&DeadLetterRecord {
                schema_version: 1,
                message_id: Some("mailmsg_001".to_string()),
                recipient: Some("role://topics.curator".to_string()),
                code: MailErrorCode::UnknownRoute,
                message: "route not found".to_string(),
                occurred_at: "2026-05-21T07:00:00Z".to_string(),
            })
            .unwrap();

        assert!(store.paths().agents_path().is_file());
        assert!(store.paths().leases_path().is_file());
        assert!(store.paths().claims_path().is_file());
        assert!(store.paths().dead_letter_path().is_file());
    }

    #[test]
    fn claim_and_failure_shapes_are_serializable() {
        let claim = ClaimEvent {
            schema_version: 1,
            delivery_id: "delivery_001".to_string(),
            message_id: "mailmsg_001".to_string(),
            thread_id: "thread_001".to_string(),
            recipient: "role://topics.curator".to_string(),
            agent_id: "agent_codex_019e3f72".to_string(),
            state: DeliveryState::Claimed,
            occurred_at: "2026-05-21T07:00:00Z".to_string(),
        };
        let failure = MailFailure {
            schema_version: 1,
            code: MailErrorCode::InvalidAddress,
            message: "invalid address".to_string(),
            repair_hints: vec![
                "Use a typed agent, role, page, mailbox, or system address.".to_string()
            ],
        };

        assert!(serde_json::to_string(&claim).unwrap().contains("claimed"));
        assert!(serde_json::to_string(&failure)
            .unwrap()
            .contains("invalid_address"));
    }

    #[test]
    fn rebuild_overwrites_corrupted_mailbox_index_from_delivery_truth() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        store
            .append_delivery(&DeliveryRecord {
                schema_version: 1,
                delivery_id: "delivery_001".to_string(),
                message_id: "mailmsg_001".to_string(),
                thread_id: "thread_001".to_string(),
                recipient: recipient.canonical(),
                state: DeliveryState::Unread,
                claimed_by: None,
                snoozed_until: None,
                created_at: "2026-05-21T07:00:00Z".to_string(),
                updated_at: "2026-05-21T07:00:00Z".to_string(),
            })
            .unwrap();
        let mailbox_path = store.paths().mailbox_path(&recipient);
        fs::create_dir_all(mailbox_path.parent().unwrap()).unwrap();
        fs::write(&mailbox_path, "not-json\n").unwrap();

        let rows = store.rebuild_mailbox(&recipient).unwrap();
        assert_eq!(rows.len(), 1);
        let repaired = fs::read_to_string(&mailbox_path).unwrap();
        assert!(repaired.contains("delivery_001"));
        assert!(!repaired.contains("not-json"));
    }

    #[test]
    fn rebuild_ignores_other_recipients() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        let other = MailAddress::parse("role://projects.curator").unwrap();
        for (delivery_id, address) in [
            ("delivery_001", recipient.canonical()),
            ("delivery_002", other.canonical()),
        ] {
            store
                .append_delivery(&DeliveryRecord {
                    schema_version: 1,
                    delivery_id: delivery_id.to_string(),
                    message_id: format!("mailmsg_{delivery_id}"),
                    thread_id: format!("thread_{delivery_id}"),
                    recipient: address,
                    state: DeliveryState::Unread,
                    claimed_by: None,
                    snoozed_until: None,
                    created_at: "2026-05-21T07:00:00Z".to_string(),
                    updated_at: "2026-05-21T07:00:00Z".to_string(),
                })
                .unwrap();
        }

        let rows = store.rebuild_mailbox(&recipient).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].delivery_id, "delivery_001");
    }

    #[test]
    fn send_mail_delivers_to_direct_role_and_page_mailboxes() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let mut envelope = sample_envelope("mailmsg_send_001", "send-key-001");
        envelope.to = vec![
            "agent://codex/agent_codex_019e3f72".to_string(),
            "role://topics.curator".to_string(),
        ];
        envelope.cc = vec!["mailbox://page/topics".to_string()];

        let receipt = store
            .send_mail(
                &envelope,
                "Please review the topics graph.",
                &SendMailOptions::default(),
            )
            .unwrap();

        assert_eq!(receipt.acceptance, MessageAcceptance::Accepted);
        assert_eq!(receipt.attempts.len(), 3);
        assert!(receipt
            .attempts
            .iter()
            .all(|attempt| attempt.status == DeliveryAttemptStatus::Delivered));
        assert!(receipt
            .attempts
            .iter()
            .any(|attempt| attempt.recipient == "mailbox://page/topics"));

        let role = MailAddress::parse("role://topics.curator").unwrap();
        let summary = store.recipient_summary(&role).unwrap();
        assert_eq!(summary.open_delivery_count, 1);
        assert_eq!(summary.open_thread_count, 1);
        let rows = store.rebuild_mailbox(&role).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_id, envelope.message_id);
        assert_eq!(rows[0].thread_id, "thread_001");
    }

    #[test]
    fn send_mail_retry_is_idempotent_and_does_not_duplicate_delivery() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_send_002", "send-key-002");

        let first = store
            .send_mail(&envelope, "Same body.", &SendMailOptions::default())
            .unwrap();
        let retry = store
            .send_mail(&envelope, "Same body.", &SendMailOptions::default())
            .unwrap();

        assert_eq!(first.acceptance, MessageAcceptance::Accepted);
        assert_eq!(
            retry.acceptance,
            MessageAcceptance::DuplicateSamePayload {
                message_id: "mailmsg_send_002".to_string()
            }
        );
        assert_eq!(retry.attempts.len(), 1);
        assert_eq!(
            retry.attempts[0].status,
            DeliveryAttemptStatus::AlreadyDelivered
        );

        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        let rows = store.rebuild_mailbox(&recipient).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(
            store
                .recipient_summary(&recipient)
                .unwrap()
                .open_delivery_count,
            1
        );
    }

    #[test]
    fn concurrent_send_mail_retries_create_one_idempotency_and_delivery_record() {
        let temp = TempDir::new().unwrap();
        let context_engine = temp.path().join("context-engine");
        let worker_count = 8;
        let barrier = Arc::new(Barrier::new(worker_count));
        let mut handles = Vec::new();

        for index in 0..worker_count {
            let barrier = Arc::clone(&barrier);
            let context_engine = context_engine.clone();
            handles.push(thread::spawn(move || {
                let store = AgentMailStore::new(context_engine);
                let envelope = sample_envelope(
                    &format!("mailmsg_concurrent_idem_{index}"),
                    "concurrent-idempotency-key-001",
                );
                barrier.wait();
                store
                    .send_mail(
                        &envelope,
                        "Concurrent idempotent body.",
                        &SendMailOptions::default(),
                    )
                    .unwrap()
            }));
        }

        let receipts: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        let accepted_count = receipts
            .iter()
            .filter(|receipt| receipt.acceptance == MessageAcceptance::Accepted)
            .count();
        assert_eq!(accepted_count, 1);
        assert!(receipts.iter().all(|receipt| receipt.attempts.len() == 1));

        let store = AgentMailStore::new(context_engine);
        let idempotency_records =
            read_jsonl::<IdempotencyRecord>(&store.paths().idempotency_path()).unwrap();
        let delivery_records =
            read_jsonl::<DeliveryRecord>(&store.paths().deliveries_path()).unwrap();
        assert_eq!(idempotency_records.len(), 1);
        assert_eq!(delivery_records.len(), 1);
        assert_eq!(
            store
                .mail_inbox(&MailAddress::parse("role://topics.curator").unwrap())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn send_mail_defers_when_recipient_capacity_is_full() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let options = SendMailOptions {
            max_open_deliveries_per_recipient: Some(1),
        };
        let first = sample_envelope("mailmsg_send_003", "send-key-003");
        let second = sample_envelope("mailmsg_send_004", "send-key-004");

        store.send_mail(&first, "First body.", &options).unwrap();
        let receipt = store.send_mail(&second, "Second body.", &options).unwrap();

        assert_eq!(receipt.acceptance, MessageAcceptance::Accepted);
        assert_eq!(receipt.attempts.len(), 1);
        assert_eq!(
            receipt.attempts[0].status,
            DeliveryAttemptStatus::DeferredCapacity
        );
        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        assert_eq!(store.rebuild_mailbox(&recipient).unwrap().len(), 1);
        let dead_letter = fs::read_to_string(store.paths().dead_letter_path()).unwrap();
        assert!(dead_letter.contains("mailbox_full"));
        assert!(dead_letter.contains("mailmsg_send_004"));
    }

    #[test]
    fn inbox_reads_headers_first_and_hydrates_message_or_thread_on_demand() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_inbox_001", "inbox-key-001");
        store
            .send_mail(
                &envelope,
                "The body should only be loaded when a consumer reads it.",
                &SendMailOptions::default(),
            )
            .unwrap();

        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        let rows = store.mail_inbox(&recipient).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].message_id, "mailmsg_inbox_001");
        assert_eq!(rows[0].state, DeliveryState::Unread);

        let message = store.read_message("mailmsg_inbox_001").unwrap();
        assert_eq!(message.envelope.subject, "Review topics");
        assert!(message.body_markdown.contains("only be loaded"));
        let thread = store.read_thread("thread_001").unwrap();
        assert_eq!(thread.len(), 1);
        assert_eq!(thread[0].envelope.message_id, "mailmsg_inbox_001");
    }

    #[test]
    fn agent_inbox_combines_direct_and_granted_role_mail() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let request = AgentIdentifyRequest {
            transport_kind: "codex".to_string(),
            thread_id: "019e3f72-3471-7da1-92a8-56e5d25aaa01".to_string(),
            requested_roles: vec!["role://topics.curator".to_string()],
            requested_capabilities: vec!["wiki.mail".to_string()],
            lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
            occurred_at: "2026-05-21T07:00:00Z".to_string(),
        };
        let agent = store
            .identify_agent(
                &request,
                &AgentGrantPolicy::allow_exact(&["role://topics.curator"], &["wiki.mail"]),
            )
            .unwrap();
        let mut direct = sample_envelope("mailmsg_agent_001", "agent-key-001");
        direct.to = vec![agent.primary_address.clone()];
        let role = sample_envelope("mailmsg_agent_002", "agent-key-002");

        store
            .send_mail(&direct, "Direct body.", &SendMailOptions::default())
            .unwrap();
        store
            .send_mail(&role, "Role body.", &SendMailOptions::default())
            .unwrap();

        let rows = store.agent_inbox(&agent.agent_id).unwrap();
        let message_ids: BTreeSet<_> = rows.iter().map(|row| row.message_id.as_str()).collect();
        assert_eq!(rows.len(), 2);
        assert!(message_ids.contains("mailmsg_agent_001"));
        assert!(message_ids.contains("mailmsg_agent_002"));

        let direct_delivery_id = rows
            .iter()
            .find(|row| row.message_id == "mailmsg_agent_001")
            .unwrap()
            .delivery_id
            .clone();
        let opened = store
            .open_delivery(&direct_delivery_id, &agent.agent_id)
            .unwrap();
        assert_eq!(opened.delivery.delivery_id, direct_delivery_id);
        assert_eq!(opened.message.envelope.message_id, "mailmsg_agent_001");
        assert_eq!(opened.message.body_bytes, "Direct body.".len());
        assert_eq!(opened.content_delivery.method, "thread/inject_items");
        assert_eq!(opened.content_delivery.thread_id, agent.transport.thread_id);
        assert!(opened.content_delivery.items.is_empty());

        let outsider = identify_test_agent_with_roles(&store, "outsider-open-agent", &[]);
        let error = store
            .open_delivery(&direct_delivery_id, &outsider.agent_id)
            .unwrap_err()
            .to_string();
        assert!(error.contains("cannot access"));
    }

    #[test]
    fn retired_agents_cannot_read_unified_inbox() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let agent = identify_test_agent(&store, "retired-inbox-agent");
        let envelope = sample_envelope("mailmsg_retired_inbox_001", "retired-inbox-key-001");
        store
            .send_mail(
                &envelope,
                "Retired inbox body.",
                &SendMailOptions::default(),
            )
            .unwrap();

        assert_eq!(store.agent_inbox(&agent.agent_id).unwrap().len(), 1);
        store
            .retire_agent(
                &agent.agent_id,
                "2026-05-21T07:10:00Z",
                "2026-05-21T07:10:00Z",
            )
            .unwrap();
        let error = store.agent_inbox(&agent.agent_id).unwrap_err().to_string();
        assert!(error.contains("not active"));
    }

    #[test]
    fn delivered_mail_creates_pollable_codex_steering_notifications() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let agent = store
            .identify_agent(
                &AgentIdentifyRequest {
                    transport_kind: "codex".to_string(),
                    thread_id: "019e3f72-3471-7da1-92a8-56e5d25aaa01".to_string(),
                    requested_roles: vec!["role://topics.curator".to_string()],
                    requested_capabilities: vec!["wiki.mail".to_string()],
                    lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
                    occurred_at: "2026-05-21T07:00:00Z".to_string(),
                },
                &AgentGrantPolicy::allow_exact(&["role://topics.curator"], &["wiki.mail"]),
            )
            .unwrap();
        let envelope = sample_envelope("mailmsg_notify_001", "notify-key-001");

        store
            .send_mail(
                &envelope,
                "Notify this curator without copying the body into steering.",
                &SendMailOptions::default(),
            )
            .unwrap();

        let notifications = store
            .notification_poll(&agent.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].agent_id, agent.agent_id);
        assert_eq!(notifications[0].recipient, "role://topics.curator");
        assert!(store.paths().notification_outbox_path().is_file());

        let payload = store.codex_steering_payload(&notifications[0]).unwrap();
        assert_eq!(payload.transport, "codex.steering");
        assert_eq!(payload.thread_id, "019e3f72-3471-7da1-92a8-56e5d25aaa01");
        assert_eq!(payload.message_id, "mailmsg_notify_001");
        assert_eq!(payload.delivery_id, notifications[0].delivery_id);
        assert_eq!(payload.kind, "proposal");
        assert_eq!(payload.subject, "Review topics");
        assert_eq!(payload.page.as_ref().unwrap().id, "topics");
        assert!(payload.instruction.contains("wiki.agent.inbox"));
        assert!(!payload.instruction.contains("Notify this curator"));
        let steering_text = store.codex_steering_text(&payload);
        assert!(steering_text.starts_with("<steering source=\"1context\""));
        assert!(steering_text.contains("delivery_id:"));
        assert!(steering_text.contains("message_id: mailmsg_notify_001"));
        assert!(steering_text.contains("page: topics /topics"));
        assert!(steering_text.contains("kind: proposal"));
        assert!(steering_text.contains("subject: Review topics"));
        assert!(steering_text.contains("wiki.mail.open("));
        assert!(!steering_text.contains("Notify this curator"));

        let attempt = store
            .record_notification_attempt(
                &notifications[0],
                NotificationAttemptStatus::DryRun,
                "2026-05-21T07:01:05Z",
                None,
            )
            .unwrap();
        assert_eq!(attempt.status, NotificationAttemptStatus::DryRun);
        assert!(store.paths().notification_attempts_path().is_file());

        store
            .acknowledge_notification(
                &agent.agent_id,
                &notifications[0].notification_id,
                "2026-05-21T07:02:00Z",
            )
            .unwrap();
        assert!(store
            .notification_poll(&agent.agent_id, None, "2026-05-21T07:03:00Z")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn shared_role_claim_suppresses_competing_notifications() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let claimant = identify_test_agent(&store, "claimant-thread");
        let watcher = identify_test_agent(&store, "watcher-thread");
        let envelope = sample_envelope("mailmsg_fanout_001", "fanout-key-001");

        let receipt = store
            .send_mail(
                &envelope,
                "Only the claiming agent should keep an actionable wakeup.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let delivery_id = &receipt.attempts[0].delivery_id;

        assert_eq!(
            store
                .notification_poll(&claimant.agent_id, None, "2026-05-21T07:01:00Z")
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .notification_poll(&watcher.agent_id, None, "2026-05-21T07:01:00Z")
                .unwrap()
                .len(),
            1
        );

        store
            .claim_delivery(delivery_id, &claimant.agent_id, "2026-05-21T07:02:00Z")
            .unwrap();

        assert!(store
            .notification_poll(&watcher.agent_id, None, "2026-05-21T07:03:00Z")
            .unwrap()
            .is_empty());
        let notifications = store.latest_notifications().unwrap();
        let watcher_notification = notifications
            .values()
            .find(|notification| {
                notification.delivery_id == *delivery_id
                    && notification.agent_id == watcher.agent_id
            })
            .unwrap();
        assert_eq!(watcher_notification.state, NotificationState::Suppressed);
        assert_eq!(watcher_notification.updated_at, "2026-05-21T07:02:00Z");
    }

    #[test]
    fn stale_or_terminal_deliveries_do_not_poll_as_notifications() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let active = store
            .identify_agent(
                &AgentIdentifyRequest {
                    transport_kind: "codex".to_string(),
                    thread_id: "terminal-agent-thread".to_string(),
                    requested_roles: vec!["role://topics.curator".to_string()],
                    requested_capabilities: vec!["wiki.mail".to_string()],
                    lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
                    occurred_at: "2026-05-21T07:00:00Z".to_string(),
                },
                &AgentGrantPolicy::allow_exact(&["role://topics.curator"], &["wiki.mail"]),
            )
            .unwrap();
        let stale = store
            .identify_agent(
                &AgentIdentifyRequest {
                    transport_kind: "codex".to_string(),
                    thread_id: "stale-agent-thread".to_string(),
                    requested_roles: vec!["role://topics.curator".to_string()],
                    requested_capabilities: vec!["wiki.mail".to_string()],
                    lease_expires_at: "2026-05-21T06:59:00Z".to_string(),
                    occurred_at: "2026-05-21T06:00:00Z".to_string(),
                },
                &AgentGrantPolicy::allow_exact(&["role://topics.curator"], &["wiki.mail"]),
            )
            .unwrap();
        let envelope = sample_envelope("mailmsg_notify_terminal_001", "notify-terminal-key-001");
        let receipt = store
            .send_mail(
                &envelope,
                "Terminal notification body.",
                &SendMailOptions::default(),
            )
            .unwrap();

        assert_eq!(
            store
                .notification_poll(&active.agent_id, None, "2026-05-21T07:01:00Z")
                .unwrap()
                .len(),
            1
        );
        let stale_poll_error = store
            .notification_poll(&stale.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap_err()
            .to_string();
        assert!(stale_poll_error.contains("stale lease"));
        assert_eq!(
            store
                .notification_dispatch_queue(&stale.agent_id, None, "2026-05-21T07:01:00Z")
                .unwrap()
                .len(),
            1
        );
        let outbox = fs::read_to_string(store.paths().notification_outbox_path()).unwrap();
        assert!(outbox.contains("stale-agent-thread"));
        store
            .claim_delivery(
                &receipt.attempts[0].delivery_id,
                &active.agent_id,
                "2026-05-21T07:01:30Z",
            )
            .unwrap();
        store
            .mark_delivery(
                &receipt.attempts[0].delivery_id,
                &active.agent_id,
                DeliveryState::Done,
                "2026-05-21T07:02:00Z",
            )
            .unwrap();
        assert!(store
            .notification_poll(&active.agent_id, None, "2026-05-21T07:03:00Z")
            .unwrap()
            .is_empty());
        assert!(store
            .notification_dispatch_queue(&stale.agent_id, None, "2026-05-21T07:03:00Z")
            .unwrap()
            .is_empty());
    }

    #[test]
    fn claim_and_mark_are_idempotent_conflict_safe_and_terminal() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_claim_001", "claim-key-001");
        let receipt = store
            .send_mail(&envelope, "Claim body.", &SendMailOptions::default())
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();
        let agent_a = identify_test_agent(&store, "claim-agent-a");
        let agent_b = identify_test_agent(&store, "claim-agent-b");

        let claimed = store
            .claim_delivery(delivery_id, &agent_a.agent_id, "2026-05-21T07:01:00Z")
            .unwrap();
        assert_eq!(claimed.state, DeliveryState::Claimed);
        assert_eq!(
            claimed.claimed_by.as_deref(),
            Some(agent_a.agent_id.as_str())
        );
        let claimed_again = store
            .claim_delivery(delivery_id, &agent_a.agent_id, "2026-05-21T07:02:00Z")
            .unwrap();
        assert_eq!(claimed_again.updated_at, claimed.updated_at);
        assert!(store
            .claim_delivery(delivery_id, &agent_b.agent_id, "2026-05-21T07:03:00Z")
            .is_err());

        let done = store
            .mark_delivery(
                delivery_id,
                &agent_a.agent_id,
                DeliveryState::Done,
                "2026-05-21T07:04:00Z",
            )
            .unwrap();
        assert_eq!(done.state, DeliveryState::Done);
        assert!(store
            .mark_delivery(
                delivery_id,
                &agent_a.agent_id,
                DeliveryState::Read,
                "2026-05-21T07:05:00Z",
            )
            .is_err());

        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        assert_eq!(
            store
                .recipient_summary(&recipient)
                .unwrap()
                .open_delivery_count,
            0
        );
        let rows = store.rebuild_mailbox(&recipient).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].state, DeliveryState::Done);
        assert_eq!(
            store
                .mail_inbox_due(&recipient, "2026-05-21T07:06:00Z")
                .unwrap()
                .len(),
            0
        );
        assert!(fs::read_to_string(store.paths().claims_path())
            .unwrap()
            .contains("done"));
    }

    #[test]
    fn concurrent_competing_claims_leave_one_winner() {
        let temp = TempDir::new().unwrap();
        let context_engine = temp.path().join("context-engine");
        let store = AgentMailStore::new(context_engine.clone());
        let envelope = sample_envelope("mailmsg_concurrent_claim_001", "concurrent-claim-key-001");
        let receipt = store
            .send_mail(
                &envelope,
                "Concurrent claim body.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.clone();
        let agents: Vec<_> = (0..8)
            .map(|index| identify_test_agent(&store, &format!("concurrent-claim-agent-{index}")))
            .collect();
        let barrier = Arc::new(Barrier::new(agents.len()));
        let mut handles = Vec::new();

        for agent in agents {
            let barrier = Arc::clone(&barrier);
            let context_engine = context_engine.clone();
            let delivery_id = delivery_id.clone();
            handles.push(thread::spawn(move || {
                let store = AgentMailStore::new(context_engine);
                barrier.wait();
                store.claim_delivery(&delivery_id, &agent.agent_id, "2026-05-21T07:01:00Z")
            }));
        }

        let results: Vec<_> = handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect();
        let winners: Vec<_> = results
            .iter()
            .filter_map(|result| result.as_ref().ok())
            .collect();
        assert_eq!(winners.len(), 1);
        assert_eq!(winners[0].state, DeliveryState::Claimed);

        let store = AgentMailStore::new(context_engine);
        let latest = store.latest_delivery(&delivery_id).unwrap().unwrap();
        assert_eq!(latest.claimed_by, winners[0].claimed_by);
        assert_eq!(
            read_jsonl::<ClaimEvent>(&store.paths().claims_path())
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn claim_mark_and_snooze_require_registered_active_agents() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_claim_guard_001", "claim-guard-key-001");
        let receipt = store
            .send_mail(&envelope, "Guard body.", &SendMailOptions::default())
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();
        let retired = identify_test_agent(&store, "retired-claim-agent");
        store
            .retire_agent(
                &retired.agent_id,
                "2026-05-21T07:10:00Z",
                "2026-05-21T07:10:00Z",
            )
            .unwrap();

        assert!(store
            .claim_delivery(delivery_id, "agent_codex_unknown", "2026-05-21T07:01:00Z")
            .is_err());
        assert!(store
            .mark_delivery(
                delivery_id,
                "agent_codex_unknown",
                DeliveryState::Done,
                "2026-05-21T07:02:00Z",
            )
            .is_err());
        assert!(store
            .snooze_delivery(
                delivery_id,
                "agent_codex_unknown",
                "2026-05-21T08:00:00Z",
                "2026-05-21T07:03:00Z",
            )
            .is_err());
        assert!(store
            .claim_delivery(delivery_id, &retired.agent_id, "2026-05-21T07:04:00Z")
            .is_err());
    }

    #[test]
    fn delivery_mutations_require_recipient_visibility() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_access_guard_001", "access-guard-key-001");
        let receipt = store
            .send_mail(&envelope, "Access guard body.", &SendMailOptions::default())
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();
        let outsider = identify_test_agent_with_roles(
            &store,
            "wrong-role-agent",
            &["role://projects.curator"],
        );
        let authorized = identify_test_agent(&store, "right-role-agent");

        let claim_error = store
            .claim_delivery(delivery_id, &outsider.agent_id, "2026-05-21T07:01:00Z")
            .unwrap_err()
            .to_string();
        assert!(claim_error.contains("cannot access"));
        let mark_error = store
            .mark_delivery(
                delivery_id,
                &outsider.agent_id,
                DeliveryState::Done,
                "2026-05-21T07:02:00Z",
            )
            .unwrap_err()
            .to_string();
        assert!(mark_error.contains("cannot access"));
        let snooze_error = store
            .snooze_delivery(
                delivery_id,
                &outsider.agent_id,
                "2026-05-21T08:00:00Z",
                "2026-05-21T07:03:00Z",
            )
            .unwrap_err()
            .to_string();
        assert!(snooze_error.contains("cannot access"));

        assert!(store
            .claim_delivery(delivery_id, &authorized.agent_id, "2026-05-21T07:04:00Z")
            .is_ok());
    }

    #[test]
    fn notification_poll_and_delivery_mutation_require_live_lease() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let stale = store
            .identify_agent(
                &AgentIdentifyRequest {
                    transport_kind: "codex".to_string(),
                    thread_id: "short-lease-agent".to_string(),
                    requested_roles: vec!["role://topics.curator".to_string()],
                    requested_capabilities: vec!["wiki.mail".to_string()],
                    lease_expires_at: "2026-05-21T07:00:00Z".to_string(),
                    occurred_at: "2026-05-21T06:50:00Z".to_string(),
                },
                &AgentGrantPolicy::allow_exact(&["role://topics.curator"], &["wiki.mail"]),
            )
            .unwrap();
        let mut envelope = sample_envelope("mailmsg_stale_live_001", "stale-live-key-001");
        envelope.created_at = "2026-05-21T06:59:00Z".to_string();
        let receipt = store
            .send_mail(&envelope, "Short lease body.", &SendMailOptions::default())
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();

        assert_eq!(
            store
                .notification_poll(&stale.agent_id, None, "2026-05-21T06:59:30Z")
                .unwrap()
                .len(),
            1
        );
        let poll_error = store
            .notification_poll(&stale.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap_err()
            .to_string();
        assert!(poll_error.contains("stale lease"));
        assert_eq!(
            store
                .notification_dispatch_queue(&stale.agent_id, None, "2026-05-21T07:01:00Z")
                .unwrap()
                .len(),
            1
        );
        let claim_error = store
            .claim_delivery(delivery_id, &stale.agent_id, "2026-05-21T07:01:00Z")
            .unwrap_err()
            .to_string();
        assert!(claim_error.contains("stale lease"));
    }

    #[test]
    fn heartbeat_reconciles_notifications_for_mail_received_while_stale() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let agent = store
            .identify_agent(
                &AgentIdentifyRequest {
                    transport_kind: "codex".to_string(),
                    thread_id: "stale-then-heartbeat-agent".to_string(),
                    requested_roles: vec!["role://topics.curator".to_string()],
                    requested_capabilities: vec!["wiki.mail".to_string()],
                    lease_expires_at: "2026-05-21T07:00:00Z".to_string(),
                    occurred_at: "2026-05-21T06:50:00Z".to_string(),
                },
                &AgentGrantPolicy::allow_exact(&["role://topics.curator"], &["wiki.mail"]),
            )
            .unwrap();
        let mut envelope = sample_envelope(
            "mailmsg_heartbeat_reconcile_001",
            "heartbeat-reconcile-key-001",
        );
        envelope.created_at = "2026-05-21T07:01:00Z".to_string();

        store
            .send_mail(
                &envelope,
                "Mail received while the only matching agent is stale.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let pending_before_heartbeat = store.latest_notifications().unwrap();
        assert_eq!(pending_before_heartbeat.len(), 1);
        assert!(store
            .notification_poll(&agent.agent_id, None, "2026-05-21T07:01:30Z")
            .is_err());

        store
            .heartbeat_agent(
                &agent.agent_id,
                "2026-05-21T08:00:00Z",
                "2026-05-21T07:02:00Z",
            )
            .unwrap();
        let notifications = store
            .notification_poll(&agent.agent_id, None, "2026-05-21T07:02:30Z")
            .unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications[0].message_id,
            "mailmsg_heartbeat_reconcile_001"
        );
    }

    #[test]
    fn generic_mark_rejects_snooze_without_due_time() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_snooze_guard_001", "snooze-guard-key-001");
        let receipt = store
            .send_mail(&envelope, "Snooze guard body.", &SendMailOptions::default())
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();
        let agent = identify_test_agent(&store, "snooze-guard-agent");

        assert!(store
            .mark_delivery(
                delivery_id,
                &agent.agent_id,
                DeliveryState::Snoozed,
                "2026-05-21T07:01:00Z",
            )
            .is_err());
    }

    #[test]
    fn snoozed_mail_is_hidden_until_due_and_then_reappears() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_snooze_001", "snooze-key-001");
        let receipt = store
            .send_mail(&envelope, "Snooze body.", &SendMailOptions::default())
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();
        let agent = identify_test_agent(&store, "snooze-agent");

        let snoozed = store
            .snooze_delivery(
                delivery_id,
                &agent.agent_id,
                "2026-05-21T08:00:00Z",
                "2026-05-21T07:05:00Z",
            )
            .unwrap();
        assert_eq!(snoozed.state, DeliveryState::Snoozed);
        assert_eq!(
            snoozed.snoozed_until.as_deref(),
            Some("2026-05-21T08:00:00Z")
        );

        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        assert_eq!(
            store
                .mail_inbox_due(&recipient, "2026-05-21T07:59:59Z")
                .unwrap()
                .len(),
            0
        );
        let due_rows = store
            .mail_inbox_due(&recipient, "2026-05-21T08:00:00Z")
            .unwrap();
        assert_eq!(due_rows.len(), 1);
        assert_eq!(due_rows[0].delivery_id, delivery_id);
        assert_eq!(due_rows[0].state, DeliveryState::Snoozed);
    }

    #[test]
    fn parses_list_and_thread_addresses() {
        assert_eq!(
            MailAddress::parse("list://wiki.reviewers")
                .unwrap()
                .canonical(),
            "list://wiki.reviewers"
        );
        assert_eq!(
            MailAddress::parse("thread://thread_001")
                .unwrap()
                .canonical(),
            "thread://thread_001"
        );
        assert!(MailAddress::parse("list://wiki reviewers").is_err());
        assert!(MailAddress::parse("thread://../thread_001").is_err());
    }

    #[test]
    fn list_and_page_route_grants_receive_inbox_and_notifications() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let agent = identify_test_agent_with_roles(
            &store,
            "list-page-agent",
            &["list://wiki.reviewers", "page://topics"],
        );
        let mut envelope = sample_envelope("mailmsg_list_page_001", "list-page-key-001");
        envelope.to = vec![
            "list://wiki.reviewers".to_string(),
            "page://topics".to_string(),
        ];

        store
            .send_mail(
                &envelope,
                "List and page route body.",
                &SendMailOptions::default(),
            )
            .unwrap();

        let rows = store.agent_inbox(&agent.agent_id).unwrap();
        let recipients: BTreeSet<_> = rows.iter().map(|row| row.recipient.as_str()).collect();
        assert_eq!(rows.len(), 2);
        assert!(recipients.contains("list://wiki.reviewers"));
        assert!(recipients.contains("mailbox://page/topics"));
        assert_eq!(
            store
                .notification_poll(&agent.agent_id, None, "2026-05-21T07:01:00Z")
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn send_mail_duplicate_client_request_id_reuses_original_delivery() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let first = sample_envelope("mailmsg_idem_original", "client-request-001");
        let mut retry = sample_envelope("mailmsg_idem_retry", "client-request-001");
        retry.created_at = "2026-05-21T07:00:30Z".to_string();

        let first_receipt = store
            .send_mail(&first, "Idempotent body.", &SendMailOptions::default())
            .unwrap();
        let retry_receipt = store
            .send_mail(&retry, "Idempotent body.", &SendMailOptions::default())
            .unwrap();

        assert_eq!(
            retry_receipt.acceptance,
            MessageAcceptance::DuplicateSamePayload {
                message_id: "mailmsg_idem_original".to_string()
            }
        );
        assert_eq!(
            retry_receipt.attempts[0].delivery_id,
            first_receipt.attempts[0].delivery_id
        );
        assert_eq!(
            retry_receipt.attempts[0].status,
            DeliveryAttemptStatus::AlreadyDelivered
        );
        let recipient = MailAddress::parse("role://topics.curator").unwrap();
        assert_eq!(store.mail_inbox(&recipient).unwrap().len(), 1);
    }

    #[test]
    fn mark_read_does_not_claim_but_terminal_marks_require_claim() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let envelope = sample_envelope("mailmsg_mark_semantics_001", "mark-semantics-key-001");
        let receipt = store
            .send_mail(
                &envelope,
                "Mark semantics body.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();
        let agent = identify_test_agent(&store, "mark-semantics-agent");

        let read = store
            .mark_delivery(
                delivery_id,
                &agent.agent_id,
                DeliveryState::Read,
                "2026-05-21T07:01:00Z",
            )
            .unwrap();
        assert_eq!(read.state, DeliveryState::Read);
        assert_eq!(read.claimed_by, None);
        let error = store
            .mark_delivery(
                delivery_id,
                &agent.agent_id,
                DeliveryState::Done,
                "2026-05-21T07:02:00Z",
            )
            .unwrap_err()
            .to_string();
        assert!(error.contains("must be claimed"));
        store
            .claim_delivery(delivery_id, &agent.agent_id, "2026-05-21T07:03:00Z")
            .unwrap();
        assert_eq!(
            store
                .mark_delivery(
                    delivery_id,
                    &agent.agent_id,
                    DeliveryState::Done,
                    "2026-05-21T07:04:00Z",
                )
                .unwrap()
                .state,
            DeliveryState::Done
        );
    }

    #[test]
    fn same_role_claim_blocks_future_competing_notifications() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let claimant = identify_test_agent(&store, "future-claimant-agent");
        let envelope = sample_envelope("mailmsg_future_fanout_001", "future-fanout-key-001");
        let receipt = store
            .send_mail(
                &envelope,
                "Future fanout body.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();

        store
            .claim_delivery(delivery_id, &claimant.agent_id, "2026-05-21T07:01:00Z")
            .unwrap();
        let late_agent = identify_test_agent(&store, "future-late-agent");

        assert!(store
            .notification_poll(&late_agent.agent_id, None, "2026-05-21T07:02:00Z")
            .unwrap()
            .is_empty());
        assert!(store
            .notification_dispatch_queue(&late_agent.agent_id, None, "2026-05-21T07:02:00Z")
            .unwrap()
            .is_empty());
        assert!(store.agent_inbox(&late_agent.agent_id).unwrap().is_empty());
    }

    #[test]
    fn explicit_stale_non_retired_agents_keep_durable_notifications() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let agent = identify_test_agent(&store, "explicit-stale-agent");
        let mut stale_record = agent.clone();
        stale_record.state = AgentState::Stale;
        stale_record.lease_expires_at = "2026-05-21T07:00:00Z".to_string();
        store.append_agent(&stale_record).unwrap();
        let envelope = sample_envelope("mailmsg_explicit_stale_001", "explicit-stale-key-001");

        store
            .send_mail(
                &envelope,
                "Explicit stale notification body.",
                &SendMailOptions::default(),
            )
            .unwrap();

        assert!(store
            .notification_poll(&agent.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap_err()
            .to_string()
            .contains("not active"));
        assert_eq!(
            store
                .notification_dispatch_queue(&agent.agent_id, None, "2026-05-21T07:01:00Z")
                .unwrap()
                .len(),
            1
        );
        store
            .heartbeat_agent(
                &agent.agent_id,
                "2026-05-21T08:00:00Z",
                "2026-05-21T07:02:00Z",
            )
            .unwrap();
        assert_eq!(
            store
                .notification_poll(&agent.agent_id, None, "2026-05-21T07:03:00Z")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn suppressed_notification_ack_does_not_unsuppress() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let claimant = identify_test_agent(&store, "ack-claimant-agent");
        let watcher = identify_test_agent(&store, "ack-watcher-agent");
        let envelope = sample_envelope("mailmsg_suppressed_ack_001", "suppressed-ack-key-001");
        let receipt = store
            .send_mail(
                &envelope,
                "Suppressed ack body.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();
        let watcher_notification = store
            .notification_poll(&watcher.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap()
            .pop()
            .unwrap();

        store
            .claim_delivery(delivery_id, &claimant.agent_id, "2026-05-21T07:02:00Z")
            .unwrap();
        let acked = store
            .acknowledge_notification(
                &watcher.agent_id,
                &watcher_notification.notification_id,
                "2026-05-21T07:03:00Z",
            )
            .unwrap();

        assert_eq!(acked.state, NotificationState::Suppressed);
        assert_eq!(
            store
                .latest_notifications()
                .unwrap()
                .get(&watcher_notification.notification_id)
                .unwrap()
                .state,
            NotificationState::Suppressed
        );
    }

    #[test]
    fn dispatch_attempts_are_evidence_backed_and_retry_gated() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let agent = identify_test_agent(&store, "dispatch-attempt-agent");
        let envelope = sample_envelope("mailmsg_dispatch_attempt_001", "dispatch-attempt-key-001");
        store
            .send_mail(
                &envelope,
                "Dispatch attempt body.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let notification = store
            .notification_dispatch_queue(&agent.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap()
            .pop()
            .unwrap();

        let attempt = store
            .record_notification_attempt(
                &notification,
                NotificationAttemptStatus::Sent,
                "2026-05-21T07:01:05Z",
                None,
            )
            .unwrap();
        assert_eq!(
            attempt.payload.transport_attempt_id.as_deref(),
            Some(attempt.attempt_id.as_str())
        );
        assert!(store
            .notification_dispatch_queue(&agent.agent_id, None, "2026-05-21T07:01:06Z")
            .unwrap()
            .is_empty());
        assert_eq!(
            store
                .notification_poll(&agent.agent_id, None, "2026-05-21T07:01:06Z")
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn failed_dispatch_attempt_preserves_pending_work_until_retry_time() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let agent = identify_test_agent(&store, "failed-dispatch-agent");
        let envelope = sample_envelope("mailmsg_failed_dispatch_001", "failed-dispatch-key-001");
        store
            .send_mail(
                &envelope,
                "Failed dispatch body.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let notification = store
            .notification_dispatch_queue(&agent.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap()
            .pop()
            .unwrap();

        store
            .record_notification_attempt(
                &notification,
                NotificationAttemptStatus::Failed,
                "2026-05-21T07:01:05Z",
                Some("turn/steer failed".to_string()),
            )
            .unwrap();
        assert!(store
            .notification_dispatch_queue(&agent.agent_id, None, "2026-05-21T07:01:30Z")
            .unwrap()
            .is_empty());
        let retry_queue = store
            .notification_dispatch_queue(&agent.agent_id, None, "2026-05-21T07:02:05Z")
            .unwrap();
        assert_eq!(retry_queue.len(), 1);
        assert_eq!(retry_queue[0].attempt_count, 1);
        assert_eq!(
            retry_queue[0].last_error.as_deref(),
            Some("turn/steer failed")
        );
        assert_eq!(
            retry_queue[0].next_dispatch_at.as_deref(),
            Some("2026-05-21T07:02:05Z")
        );
    }

    #[test]
    fn record_notification_attempt_rejects_terminal_and_suppressed_work() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let claimant = identify_test_agent(&store, "reject-attempt-claimant");
        let watcher = identify_test_agent(&store, "reject-attempt-watcher");
        let envelope = sample_envelope("mailmsg_reject_attempt_001", "reject-attempt-key-001");
        let receipt = store
            .send_mail(
                &envelope,
                "Reject attempt body.",
                &SendMailOptions::default(),
            )
            .unwrap();
        let delivery_id = receipt.attempts[0].delivery_id.as_str();
        let claimant_notification = store
            .notification_poll(&claimant.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap()
            .pop()
            .unwrap();
        let watcher_notification = store
            .notification_poll(&watcher.agent_id, None, "2026-05-21T07:01:00Z")
            .unwrap()
            .pop()
            .unwrap();

        store
            .claim_delivery(delivery_id, &claimant.agent_id, "2026-05-21T07:02:00Z")
            .unwrap();
        assert!(store
            .record_notification_attempt(
                &watcher_notification,
                NotificationAttemptStatus::Sent,
                "2026-05-21T07:02:30Z",
                None,
            )
            .is_err());
        store
            .mark_delivery(
                delivery_id,
                &claimant.agent_id,
                DeliveryState::Done,
                "2026-05-21T07:03:00Z",
            )
            .unwrap();
        assert!(store
            .record_notification_attempt(
                &claimant_notification,
                NotificationAttemptStatus::Sent,
                "2026-05-21T07:03:30Z",
                None,
            )
            .is_err());
        assert!(!store.paths().notification_attempts_path().is_file());
    }

    #[test]
    fn triad_mail_loop_opens_injects_claims_marks_and_acks() {
        let temp = TempDir::new().unwrap();
        let store = AgentMailStore::new(temp.path().join("context-engine"));
        let pip = identify_test_agent_with_roles(
            &store,
            "triad-pip-thread",
            &["role://triad.promptsmith"],
        );
        let mira = identify_test_agent_with_roles(
            &store,
            "triad-mira-thread",
            &["role://triad.cartographer"],
        );
        let nox =
            identify_test_agent_with_roles(&store, "triad-nox-thread", &["role://triad.archivist"]);

        let first = send_test_mail(
            &store,
            "mailmsg_triad_001",
            "triad-key-001",
            &pip.primary_address,
            "role://triad.cartographer",
            "Frame the route.",
        );
        handle_test_delivery(
            &store,
            &mira,
            "2026-05-21T07:01:00Z",
            "2026-05-21T07:01:10Z",
            &first.attempts[0].delivery_id,
        );

        let second = send_test_mail(
            &store,
            "mailmsg_triad_002",
            "triad-key-002",
            &mira.primary_address,
            "role://triad.archivist",
            "Archive the route.",
        );
        handle_test_delivery(
            &store,
            &nox,
            "2026-05-21T07:02:00Z",
            "2026-05-21T07:02:10Z",
            &second.attempts[0].delivery_id,
        );

        let third = send_test_mail(
            &store,
            "mailmsg_triad_003",
            "triad-key-003",
            &nox.primary_address,
            "role://triad.promptsmith",
            "Close the receipt.",
        );
        handle_test_delivery(
            &store,
            &pip,
            "2026-05-21T07:03:00Z",
            "2026-05-21T07:03:10Z",
            &third.attempts[0].delivery_id,
        );

        let fourth = send_test_mail(
            &store,
            "mailmsg_triad_004",
            "triad-key-004",
            &pip.primary_address,
            "role://triad.promptsmith",
            "Self receipt done.",
        );
        handle_test_delivery(
            &store,
            &pip,
            "2026-05-21T07:04:00Z",
            "2026-05-21T07:04:10Z",
            &fourth.attempts[0].delivery_id,
        );

        assert_eq!(
            read_jsonl::<DeliveryRecord>(&store.paths().deliveries_path())
                .unwrap()
                .len(),
            12
        );
        assert_eq!(
            read_jsonl::<ClaimEvent>(&store.paths().claims_path())
                .unwrap()
                .len(),
            8
        );
        assert_eq!(
            read_jsonl::<MailInjectionReceipt>(&store.paths().injection_receipts_path())
                .unwrap()
                .len(),
            4
        );
        assert_eq!(
            read_jsonl::<MailControlEvent>(&store.paths().control_events_path())
                .unwrap()
                .len(),
            4
        );
        assert_eq!(
            read_jsonl::<NotificationRecord>(&store.paths().notification_outbox_path())
                .unwrap()
                .len(),
            8
        );

        for agent in [&pip, &mira, &nox] {
            assert!(store
                .notification_poll(&agent.agent_id, None, "2026-05-21T07:05:00Z")
                .unwrap()
                .is_empty());
        }
    }

    fn sample_envelope(message_id: &str, idempotency_key: &str) -> MessageEnvelope {
        MessageEnvelope {
            schema_version: 1,
            message_id: message_id.to_string(),
            idempotency_key: idempotency_key.to_string(),
            kind: "proposal".to_string(),
            subject: "Review topics".to_string(),
            from: "agent://codex/019e3f72".to_string(),
            to: vec!["role://topics.curator".to_string()],
            cc: Vec::new(),
            page: Some(MessagePageRef {
                id: "topics".to_string(),
                route: "/topics".to_string(),
            }),
            thread_id: "thread_001".to_string(),
            reply_to: None,
            body: MessageBodyRef {
                format: "markdown".to_string(),
                sha256: "bodysha".to_string(),
            },
            attachments: Vec::new(),
            created_at: "2026-05-21T07:00:00Z".to_string(),
        }
    }

    fn identify_test_agent(store: &AgentMailStore, thread_id: &str) -> AgentRecord {
        identify_test_agent_with_roles(store, thread_id, &["role://topics.curator"])
    }

    fn identify_test_agent_with_roles(
        store: &AgentMailStore,
        thread_id: &str,
        roles: &[&str],
    ) -> AgentRecord {
        store
            .identify_agent(
                &AgentIdentifyRequest {
                    transport_kind: "codex".to_string(),
                    thread_id: thread_id.to_string(),
                    requested_roles: roles.iter().map(|role| role.to_string()).collect(),
                    requested_capabilities: vec!["wiki.mail".to_string()],
                    lease_expires_at: "2026-05-21T08:00:00Z".to_string(),
                    occurred_at: "2026-05-21T07:00:00Z".to_string(),
                },
                &AgentGrantPolicy::allow_exact(roles, &["wiki.mail"]),
            )
            .unwrap()
    }

    fn send_test_mail(
        store: &AgentMailStore,
        message_id: &str,
        idempotency_key: &str,
        from: &str,
        to: &str,
        body: &str,
    ) -> MailSendReceipt {
        let mut envelope = sample_envelope(message_id, idempotency_key);
        envelope.from = from.to_string();
        envelope.to = vec![to.to_string()];
        envelope.cc = Vec::new();
        envelope.subject = format!("Triad message {message_id}");
        envelope.thread_id = format!("thread_{message_id}");
        store
            .send_mail(&envelope, body, &SendMailOptions::default())
            .unwrap()
    }

    fn handle_test_delivery(
        store: &AgentMailStore,
        agent: &AgentRecord,
        poll_at: &str,
        mutate_at: &str,
        delivery_id: &str,
    ) {
        let notifications = store
            .notification_poll(&agent.agent_id, None, poll_at)
            .unwrap();
        let notification = notifications
            .iter()
            .find(|notification| notification.delivery_id == delivery_id)
            .unwrap();
        let rows = store.agent_inbox(&agent.agent_id).unwrap();
        assert!(rows.iter().any(|row| row.delivery_id == delivery_id));

        let opened = store.open_delivery(delivery_id, &agent.agent_id).unwrap();
        assert_eq!(opened.delivery.delivery_id, delivery_id);
        assert_eq!(opened.content_delivery.method, "thread/inject_items");
        assert_eq!(opened.content_delivery.thread_id, agent.transport.thread_id);
        assert!(opened.content_delivery.items.is_empty());

        let recorded = store
            .record_mail_injection_result(
                delivery_id,
                &agent.agent_id,
                None,
                1,
                MailInjectionResult::Ok,
                mutate_at,
                None,
            )
            .unwrap();
        assert_eq!(recorded.receipt.delivery_id, delivery_id);
        assert_eq!(recorded.receipt.item_count, 1);

        let claimed = store
            .claim_delivery(delivery_id, &agent.agent_id, mutate_at)
            .unwrap();
        assert_eq!(claimed.state, DeliveryState::Claimed);
        let done = store
            .mark_delivery(delivery_id, &agent.agent_id, DeliveryState::Done, mutate_at)
            .unwrap();
        assert_eq!(done.state, DeliveryState::Done);
        assert_eq!(done.claimed_by.as_deref(), Some(agent.agent_id.as_str()));
        let acked = store
            .acknowledge_notification(&agent.agent_id, &notification.notification_id, mutate_at)
            .unwrap();
        assert_eq!(acked.state, NotificationState::Acknowledged);
    }
}
