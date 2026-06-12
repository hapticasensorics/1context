//! Receipt bridge for scheduler-visible evidence.
//!
//! Real Agent Mail delivery is the intended authority when the hosted/native
//! mail tool is available. This module records typed receipt observations and
//! an audit mirror without treating the mirror as proof by itself.

use crate::run_state::{JobReceiptState, WikiCompanyRunState};
use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptAuthority {
    AgentMail,
    WikiTalk,
    Harness,
    AuditMirror,
    ExternalPending,
}

impl ReceiptAuthority {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AgentMail => "agent_mail",
            Self::WikiTalk => "wiki_talk",
            Self::Harness => "harness",
            Self::AuditMirror => "audit_mirror",
            Self::ExternalPending => "external_pending",
        }
    }

    pub fn authoritative_channel(self) -> Option<ReceiptChannel> {
        match self {
            Self::AgentMail => Some(ReceiptChannel::AgentMail),
            Self::WikiTalk => Some(ReceiptChannel::WikiTalk),
            Self::Harness => Some(ReceiptChannel::Harness),
            Self::AuditMirror | Self::ExternalPending => None,
        }
    }

    pub fn is_authoritative_for(self, channel: ReceiptChannel) -> bool {
        self.authoritative_channel() == Some(channel)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptChannel {
    Harness,
    WikiTalk,
    AgentMail,
}

impl ReceiptChannel {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Harness => "harness",
            Self::WikiTalk => "wiki_talk",
            Self::AgentMail => "agent_mail",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptStatus {
    Pending,
    Complete,
    Failed,
}

impl ReceiptStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Complete => "complete",
            Self::Failed => "failed",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessReceiptState {
    pub harness_birth_certificate: bool,
    pub harness_turn_start: bool,
    pub context_injection_receipt: bool,
    pub adapter_events: bool,
    pub final_message_path: Option<String>,
    pub final_message_present: bool,
    pub harness_turn_complete: bool,
    pub codex_exit_status: Option<i32>,
    pub issues: Vec<String>,
}

impl HarnessReceiptState {
    pub fn complete(final_message_path: impl Into<String>) -> Self {
        Self {
            harness_birth_certificate: true,
            harness_turn_start: true,
            context_injection_receipt: true,
            adapter_events: true,
            final_message_path: Some(final_message_path.into()),
            final_message_present: true,
            harness_turn_complete: true,
            codex_exit_status: Some(0),
            issues: Vec::new(),
        }
    }

    pub fn failed(issues: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            issues: issues.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    fn status(&self) -> ReceiptStatus {
        if !self.issues.is_empty() || self.codex_exit_status.is_some_and(|status| status != 0) {
            return ReceiptStatus::Failed;
        }
        if self.harness_birth_certificate
            && self.harness_turn_start
            && self.context_injection_receipt
            && self.adapter_events
            && self.final_message_present
            && self.harness_turn_complete
        {
            ReceiptStatus::Complete
        } else {
            ReceiptStatus::Pending
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiTalkReceiptState {
    pub delivery_mode: String,
    pub thread_id: String,
    pub receipt_id: Option<String>,
    pub appended: bool,
    pub issues: Vec<String>,
}

impl WikiTalkReceiptState {
    pub fn appended_mail(thread_id: impl Into<String>, receipt_id: impl Into<String>) -> Self {
        Self {
            delivery_mode: "mail".to_string(),
            thread_id: thread_id.into(),
            receipt_id: Some(receipt_id.into()),
            appended: true,
            issues: Vec::new(),
        }
    }

    pub fn pending_mail(thread_id: impl Into<String>) -> Self {
        Self {
            delivery_mode: "mail".to_string(),
            thread_id: thread_id.into(),
            receipt_id: None,
            appended: false,
            issues: Vec::new(),
        }
    }

    fn status(&self) -> ReceiptStatus {
        if !self.issues.is_empty() {
            return ReceiptStatus::Failed;
        }
        let has_receipt_id = self
            .receipt_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|receipt| !receipt.is_empty());
        if self.appended && self.delivery_mode == "mail" && has_receipt_id {
            ReceiptStatus::Complete
        } else {
            ReceiptStatus::Pending
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMailReceiptState {
    pub thread_id: String,
    pub mailbox: Option<String>,
    pub receipt_id: Option<String>,
    pub transport: Option<String>,
    pub delivered: bool,
    pub external_pending: bool,
    pub pending_reason: Option<String>,
    pub issues: Vec<String>,
}

impl AgentMailReceiptState {
    pub fn delivered(
        thread_id: impl Into<String>,
        receipt_id: impl Into<String>,
        transport: impl Into<String>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            mailbox: None,
            receipt_id: Some(receipt_id.into()),
            transport: Some(transport.into()),
            delivered: true,
            external_pending: false,
            pending_reason: None,
            issues: Vec::new(),
        }
    }

    pub fn pending_external(
        thread_id: impl Into<String>,
        mailbox: impl Into<String>,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            thread_id: thread_id.into(),
            mailbox: Some(mailbox.into()),
            receipt_id: None,
            transport: None,
            delivered: false,
            external_pending: true,
            pending_reason: Some(reason.into()),
            issues: Vec::new(),
        }
    }

    fn status(&self) -> ReceiptStatus {
        if !self.issues.is_empty() {
            return ReceiptStatus::Failed;
        }
        let has_receipt_id = self
            .receipt_id
            .as_deref()
            .map(str::trim)
            .is_some_and(|receipt| !receipt.is_empty());
        if self.delivered && !self.external_pending && has_receipt_id {
            ReceiptStatus::Complete
        } else {
            ReceiptStatus::Pending
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "channel", content = "state", rename_all = "snake_case")]
pub enum SchedulerReceiptState {
    Harness(HarnessReceiptState),
    WikiTalk(WikiTalkReceiptState),
    AgentMail(AgentMailReceiptState),
}

impl SchedulerReceiptState {
    pub fn channel(&self) -> ReceiptChannel {
        match self {
            Self::Harness(_) => ReceiptChannel::Harness,
            Self::WikiTalk(_) => ReceiptChannel::WikiTalk,
            Self::AgentMail(_) => ReceiptChannel::AgentMail,
        }
    }

    pub fn status(&self) -> ReceiptStatus {
        match self {
            Self::Harness(state) => state.status(),
            Self::WikiTalk(state) => state.status(),
            Self::AgentMail(state) => state.status(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SchedulerReceipt {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub unit_id: String,
    pub phase_id: String,
    pub job_id: String,
    pub authority: ReceiptAuthority,
    pub status: ReceiptStatus,
    pub receipt: SchedulerReceiptState,
    pub path: Option<String>,
    pub external_id: Option<String>,
    pub payload: Value,
    pub created_at: String,
}

impl SchedulerReceipt {
    pub fn has_authoritative_source(&self) -> bool {
        self.authority.is_authoritative_for(self.receipt.channel())
    }

    pub fn completes_job(&self) -> bool {
        self.has_authoritative_source() && self.status == ReceiptStatus::Complete
    }

    pub fn to_job_receipt_state(&self, audit_path: Option<&str>) -> Option<JobReceiptState> {
        if !self.completes_job() {
            return None;
        }
        Some(JobReceiptState {
            unit_id: self.unit_id.clone(),
            phase_id: self.phase_id.clone(),
            job_id: self.job_id.clone(),
            channel: self.receipt.channel().as_str().to_string(),
            authority: self.authority.as_str().to_string(),
            authoritative: true,
            status: self.status.as_str().to_string(),
            receipt_paths: self.path.iter().cloned().collect(),
            audit_paths: audit_path.into_iter().map(ToOwned::to_owned).collect(),
            external_id: self.external_id.clone(),
            updated_at: self.created_at.clone(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptAppendResult {
    pub audit_path: String,
    pub authoritative: bool,
    pub completes_job: bool,
}

pub fn append_scheduler_receipt(
    paths: &ContextEnginePaths,
    receipt: &SchedulerReceipt,
) -> Result<ReceiptAppendResult, String> {
    let path = audit_receipt_path(paths, &receipt.run_id);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|error| format!("failed to open {}: {error}", path.display()))?;
    serde_json::to_writer(&mut file, receipt)
        .map_err(|error| format!("failed to encode scheduler receipt: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("failed to append {}: {error}", path.display()))?;
    Ok(ReceiptAppendResult {
        audit_path: path.display().to_string(),
        authoritative: receipt.has_authoritative_source(),
        completes_job: receipt.completes_job(),
    })
}

pub fn receipt_for_job(
    run_id: &str,
    unit_id: &str,
    phase_id: &str,
    job_id: &str,
    authority: ReceiptAuthority,
    receipt: SchedulerReceiptState,
    payload: Value,
) -> SchedulerReceipt {
    SchedulerReceipt {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.scheduler_receipt.v1".to_string(),
        run_id: safe_run_id(run_id),
        unit_id: unit_id.to_string(),
        phase_id: phase_id.to_string(),
        job_id: job_id.to_string(),
        authority,
        status: receipt.status(),
        receipt,
        path: None,
        external_id: None,
        payload,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    }
}

pub fn harness_receipt_for_job(
    run_id: &str,
    unit_id: &str,
    phase_id: &str,
    job_id: &str,
    state: HarnessReceiptState,
    payload: Value,
) -> SchedulerReceipt {
    receipt_for_job(
        run_id,
        unit_id,
        phase_id,
        job_id,
        ReceiptAuthority::Harness,
        SchedulerReceiptState::Harness(state),
        payload,
    )
}

pub fn wiki_talk_receipt_for_job(
    run_id: &str,
    unit_id: &str,
    phase_id: &str,
    job_id: &str,
    state: WikiTalkReceiptState,
    payload: Value,
) -> SchedulerReceipt {
    receipt_for_job(
        run_id,
        unit_id,
        phase_id,
        job_id,
        ReceiptAuthority::WikiTalk,
        SchedulerReceiptState::WikiTalk(state),
        payload,
    )
}

pub fn agent_mail_receipt_for_job(
    run_id: &str,
    unit_id: &str,
    phase_id: &str,
    job_id: &str,
    authority: ReceiptAuthority,
    state: AgentMailReceiptState,
    payload: Value,
) -> SchedulerReceipt {
    receipt_for_job(
        run_id,
        unit_id,
        phase_id,
        job_id,
        authority,
        SchedulerReceiptState::AgentMail(state),
        payload,
    )
}

pub fn apply_receipt_to_state(state: &mut WikiCompanyRunState, receipt: &SchedulerReceipt) {
    if let Some(job_receipt) = receipt.to_job_receipt_state(None) {
        state.record_job_receipt(job_receipt);
    }
}

pub fn apply_appended_receipt_to_state(
    state: &mut WikiCompanyRunState,
    receipt: &SchedulerReceipt,
    append_result: &ReceiptAppendResult,
) {
    if let Some(job_receipt) = receipt.to_job_receipt_state(Some(&append_result.audit_path)) {
        state.record_job_receipt(job_receipt);
    }
}

pub fn audit_receipt_path(paths: &ContextEnginePaths, run_id: &str) -> PathBuf {
    paths
        .context_engine
        .join("mail")
        .join("threads")
        .join(format!("wiki-company-{}.jsonl", safe_run_id(run_id)))
}
