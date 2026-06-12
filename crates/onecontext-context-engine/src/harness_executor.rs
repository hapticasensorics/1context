//! Agent harness request construction.
//!
//! Builds the durable turn request the native `onecontext-agent-harness` /
//! Codex app-server executor owns. Requests are hydrated from a runtime turn
//! descriptor plus a plain worker profile; no pack or orchestrator TOMLs.

use crate::runtime_executor::RuntimeTurnDescriptor;
use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;

const MAX_HARNESS_ID_LEN: usize = 128;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTurnRequest {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub operation_id: String,
    pub unit_id: String,
    pub harness: HarnessTurnHarness,
    pub agent: HarnessTurnAgent,
    pub job: HarnessTurnJob,
    pub prompt_bundle: Vec<HarnessPromptPart>,
    pub prompt_text: String,
    pub mail_context: HarnessMailContext,
    pub wiki_context: HarnessWikiContext,
    pub source_packet: HarnessSourcePacket,
    pub tool_policy: HarnessToolPolicy,
    pub required_receipts: HarnessReceiptRequirements,
    pub talk_report: HarnessTalkReport,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTurnHarness {
    pub id: String,
    pub runner: String,
    pub transport: String,
    pub command: String,
    pub captures: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTurnAgent {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub reasoning_effort: String,
    pub memory_mode: String,
    pub memory_attach: String,
    pub persistent_session: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTurnJob {
    pub id: String,
    pub label: String,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub read_permissions: Vec<String>,
    pub write_permissions: Vec<String>,
    pub deny_permissions: Vec<String>,
    pub completion_done: Vec<String>,
    pub completion_skip: Vec<String>,
    pub completion_failure: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessPromptPart {
    pub path: String,
    pub source: String,
    pub bytes: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessMailContext {
    pub thread_id: String,
    pub mailbox: String,
    pub appendix_enabled: bool,
    pub appendix_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessWikiContext {
    pub user_wiki_root: String,
    pub source_root: String,
    pub talk_root: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessSourcePacket {
    pub packet_id: String,
    pub source: String,
    pub bounded_by_job: bool,
    pub path: Option<String>,
    pub content_sha256: Option<String>,
    pub bytes: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessToolPolicy {
    pub default_tools: Vec<String>,
    pub agent_tools: Vec<String>,
    pub job_tools: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessReceiptRequirements {
    pub final_message_path: String,
    pub require_birth_certificate: bool,
    pub require_turn_start: bool,
    pub require_context_injection: bool,
    pub require_adapter_events: bool,
    pub require_final_message: bool,
    pub require_talk_append: bool,
    pub require_mail_delivery: bool,
    pub require_turn_complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTalkReport {
    pub delivery_mode: String,
    pub thread_id: String,
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTurnReceipts {
    pub codex_exit_status: Option<i32>,
    pub harness_birth_certificate: bool,
    pub harness_turn_start: bool,
    pub context_injection_receipt: bool,
    pub adapter_events: bool,
    pub final_message: Option<String>,
    pub talk_receipt: Option<String>,
    pub mail_receipt: Option<String>,
    pub harness_turn_complete: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTurnCompletion {
    pub status: String,
    pub complete: bool,
    pub issues: Vec<String>,
    pub codex_exit_status: Option<i32>,
}

/// Plain worker profile for one bounded Codex worker turn. The FSM DSL runner
/// supplies these directly; there is no pack or orchestrator config behind it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessWorkerProfile {
    pub model: String,
    pub reasoning_effort: String,
    pub prompt_text: String,
    pub tools: Vec<String>,
}

pub fn build_harness_turn_request_for_runtime_turn(
    paths: &ContextEnginePaths,
    profile: &HarnessWorkerProfile,
    turn: &RuntimeTurnDescriptor,
) -> Result<HarnessTurnRequest, String> {
    let run_id = safe_run_id(&turn.run_id);
    let unit_id = turn.harness_unit_id.clone();
    let mail_context = mail_context_for_unit(&unit_id, true);
    let mut prompt_bundle = vec![HarnessPromptPart {
        path: format!("worker-profile/{}", safe_run_id(&turn.job_id)),
        source: format!("job:{}", turn.job_id),
        bytes: profile.prompt_text.len(),
        text: profile.prompt_text.clone(),
    }];
    if let Some(prompt) = source_packet_prompt_part(turn)? {
        prompt_bundle.push(prompt);
    }
    let prompt_text = render_prompt_text(&prompt_bundle, &mail_context);
    Ok(HarnessTurnRequest {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.agent_harness.turn_request.v1".to_string(),
        run_id,
        operation_id: turn.operation_id.clone(),
        unit_id,
        harness: HarnessTurnHarness {
            id: "codex-app-server".to_string(),
            runner: "onecontext-agent-harness".to_string(),
            transport: "codex-app-server".to_string(),
            command: "codex app-server".to_string(),
            captures: vec![
                "harness_birth_certificate".to_string(),
                "harness_turn_start".to_string(),
                "context_injection_receipt".to_string(),
                "adapter_events".to_string(),
                "final_message".to_string(),
                "talk_receipt".to_string(),
                "mail_receipt".to_string(),
                "harness_turn_complete".to_string(),
            ],
        },
        agent: HarnessTurnAgent {
            id: turn.agent_id.clone(),
            provider: "openai".to_string(),
            model: profile.model.clone(),
            reasoning_effort: profile.reasoning_effort.clone(),
            memory_mode: "episodic".to_string(),
            memory_attach: "none".to_string(),
            persistent_session: false,
        },
        job: HarnessTurnJob {
            id: turn.job_id.clone(),
            label: turn.job_id.clone(),
            inputs: turn.inputs.clone(),
            outputs: turn.outputs.clone(),
            read_permissions: Vec::new(),
            write_permissions: Vec::new(),
            deny_permissions: Vec::new(),
            completion_done: Vec::new(),
            completion_skip: Vec::new(),
            completion_failure: Vec::new(),
        },
        prompt_bundle,
        prompt_text,
        mail_context,
        wiki_context: HarnessWikiContext {
            user_wiki_root: paths.user_wiki.display().to_string(),
            source_root: paths.user_wiki.join("source").display().to_string(),
            talk_root: paths.user_wiki.join("source/talk").display().to_string(),
        },
        source_packet: source_packet_from_runtime_turn(turn)?,
        tool_policy: HarnessToolPolicy {
            default_tools: profile.tools.clone(),
            agent_tools: Vec::new(),
            job_tools: Vec::new(),
        },
        required_receipts: receipt_requirements_from_runtime_turn(turn),
        talk_report: HarnessTalkReport {
            delivery_mode: "mail".to_string(),
            thread_id: turn.route.thread_id.clone(),
            from: turn.route.from_mailbox.clone(),
            to: turn.route.to.clone(),
            cc: turn.route.cc.clone(),
        },
    })
}

fn source_packet_from_runtime_turn(
    turn: &RuntimeTurnDescriptor,
) -> Result<HarnessSourcePacket, String> {
    let packet_id = turn
        .packet_id
        .clone()
        .or_else(|| turn.unit_scope.get("packet.id").cloned())
        .unwrap_or_else(|| "source-packet.pending".to_string());
    let bytes = turn
        .source_packet_path
        .as_ref()
        .map(|path| fs::metadata(path).map(|metadata| metadata.len() as usize))
        .transpose()
        .map_err(|error| format!("failed to stat runtime source packet: {error}"))?;
    Ok(HarnessSourcePacket {
        packet_id,
        source: "materialized_source_packet".to_string(),
        bounded_by_job: true,
        path: turn.source_packet_path.clone(),
        content_sha256: turn.source_packet_hash.clone(),
        bytes,
    })
}

fn source_packet_prompt_part(
    turn: &RuntimeTurnDescriptor,
) -> Result<Option<HarnessPromptPart>, String> {
    let Some(path) = turn.source_packet_path.as_deref() else {
        return Ok(None);
    };
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read runtime source packet {path}: {error}"))?;
    let rendered = format!(
        "## Runtime Source Packet\n\npacket_id: {}\nsource_packet_path: {}\nsource_packet_hash: {}\n\n{}",
        turn.packet_id.as_deref().unwrap_or("source-packet.pending"),
        path,
        turn.source_packet_hash.as_deref().unwrap_or("unknown"),
        text
    );
    Ok(Some(HarnessPromptPart {
        path: path.to_string(),
        source: "runtime-source-packet".to_string(),
        bytes: rendered.len(),
        text: rendered,
    }))
}

fn receipt_requirements_from_runtime_turn(
    turn: &RuntimeTurnDescriptor,
) -> HarnessReceiptRequirements {
    HarnessReceiptRequirements {
        final_message_path: turn.final_message_path.clone(),
        require_birth_certificate: turn.receipt_expectations.require_birth_certificate,
        require_turn_start: turn.receipt_expectations.require_turn_start,
        require_context_injection: turn.receipt_expectations.require_context_injection,
        require_adapter_events: turn.receipt_expectations.require_adapter_events,
        require_final_message: turn.receipt_expectations.require_final_message,
        require_talk_append: turn.receipt_expectations.require_talk_append,
        require_mail_delivery: turn.receipt_expectations.require_mail_delivery,
        require_turn_complete: turn.receipt_expectations.require_turn_complete,
    }
}

fn mail_context_for_unit(unit_id: &str, appendix_enabled: bool) -> HarnessMailContext {
    let mailbox = agent_mail_address_for_unit(unit_id);
    let appendix_text = mail_context_appendix_text(&mailbox);
    HarnessMailContext {
        thread_id: "mail://wiki-company".to_string(),
        mailbox,
        appendix_enabled,
        appendix_text,
    }
}

pub fn agent_mail_address_for_unit(unit_id: &str) -> String {
    format!("agent://context-engine/{}", short_harness_id(unit_id))
}

pub fn short_harness_id(value: &str) -> String {
    let safe = safe_run_id(value);
    if safe.len() <= MAX_HARNESS_ID_LEN {
        return safe;
    }

    let mut hasher = Sha256::new();
    hasher.update(safe.as_bytes());
    let digest = hasher.finalize();
    let suffix = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let prefix_len = MAX_HARNESS_ID_LEN - suffix.len() - 1;
    format!("{}-{suffix}", &safe[..prefix_len])
}

pub fn evaluate_harness_turn_completion(
    request: &HarnessTurnRequest,
    receipts: &HarnessTurnReceipts,
) -> HarnessTurnCompletion {
    let mut issues = Vec::new();
    if request.required_receipts.require_birth_certificate && !receipts.harness_birth_certificate {
        issues.push("missing harness birth certificate".to_string());
    }
    if request.required_receipts.require_turn_start && !receipts.harness_turn_start {
        issues.push("missing harness turn start".to_string());
    }
    if request.required_receipts.require_context_injection && !receipts.context_injection_receipt {
        issues.push("missing context injection receipt".to_string());
    }
    if request.required_receipts.require_adapter_events && !receipts.adapter_events {
        issues.push("missing adapter event receipt".to_string());
    }
    if request.required_receipts.require_final_message
        && receipts
            .final_message
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        issues.push(format!(
            "missing non-empty final message at {}",
            request.required_receipts.final_message_path
        ));
    }
    if request.required_receipts.require_talk_append
        && receipts
            .talk_receipt
            .as_deref()
            .map(str::trim)
            .filter(|receipt| looks_like_talk_receipt(receipt))
            .is_none()
    {
        issues.push("missing wiki.talk.append receipt".to_string());
    }
    if request.required_receipts.require_mail_delivery
        && receipts
            .mail_receipt
            .as_deref()
            .map(str::trim)
            .filter(|receipt| looks_like_agent_mail_receipt(receipt))
            .is_none()
    {
        issues.push("missing Agent Mail delivery receipt".to_string());
    }
    if request.required_receipts.require_turn_complete && !receipts.harness_turn_complete {
        issues.push("missing harness turn complete receipt".to_string());
    }
    if receipts.codex_exit_status == Some(0) && !issues.is_empty() {
        issues.push("codex exit status alone is not a completion receipt".to_string());
    }
    if receipts.codex_exit_status.is_some_and(|status| status != 0) {
        issues.push("codex adapter did not report a successful worker turn".to_string());
    }
    let complete = issues.is_empty();
    HarnessTurnCompletion {
        status: if complete { "complete" } else { "incomplete" }.to_string(),
        complete,
        issues,
        codex_exit_status: receipts.codex_exit_status,
    }
}

fn looks_like_talk_receipt(receipt: &str) -> bool {
    !receipt.is_empty()
        && (receipt.starts_with("wiki.talk.append:") || receipt.starts_with("user-wiki://page/"))
}

fn looks_like_agent_mail_receipt(receipt: &str) -> bool {
    let Some(rest) = receipt.trim().strip_prefix("agent-mail://") else {
        return false;
    };
    let mut parts = rest.split('/');
    let message_id = parts.next().map(str::trim).unwrap_or_default();
    let delivery_id = parts.next().map(str::trim).unwrap_or_default();
    !message_id.is_empty()
        && !delivery_id.is_empty()
        && parts.next().is_none()
        && valid_agent_mail_receipt_segment(message_id)
        && valid_agent_mail_receipt_segment(delivery_id)
}

fn valid_agent_mail_receipt_segment(value: &str) -> bool {
    value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.'))
}

fn render_prompt_text(
    prompt_bundle: &[HarnessPromptPart],
    mail_context: &HarnessMailContext,
) -> String {
    let mut out = String::new();
    for prompt in prompt_bundle {
        out.push_str("\n\n<!-- prompt-source: ");
        out.push_str(&prompt.source);
        out.push_str(" ");
        out.push_str(&prompt.path);
        out.push_str(" -->\n\n");
        out.push_str(&prompt.text);
    }
    if mail_context.appendix_enabled {
        out.push_str("\n\n");
        out.push_str(&mail_context.appendix_text);
    }
    out.push_str("\n\n## Required Final Message Receipt\n\nWrite a non-empty final-message.md with status, evidence, proposed_wiki_talk, next_agent_requests, and next_state_machine_event. Do not send or append this final report yourself; the harness owns the wiki.talk.append and Agent Mail receipts.");
    out
}

fn mail_context_appendix_text(mailbox: &str) -> String {
    format!(
        "## Agent Mail Context\n\nThread: mail://wiki-company\nMailbox: {mailbox}\n\nRead your inbox summary before acting. Do not call wiki.talk.append for your final report. Write your final report as final-message.md content instead; the 1Context harness will append it to wiki talk and Agent Mail with delivery_mode=mail after your turn. Put downstream handoff requests in next_agent_requests."
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn literal_request() -> HarnessTurnRequest {
        HarnessTurnRequest {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            kind: "onecontext.agent_harness.turn_request.v1".to_string(),
            run_id: "literal-run".to_string(),
            operation_id: "literal-run:memory.wiki.for_you_curator".to_string(),
            unit_id: "literal-run-memory.wiki.for_you_curator".to_string(),
            harness: HarnessTurnHarness {
                id: "codex-app-server".to_string(),
                runner: "onecontext-agent-harness".to_string(),
                transport: "codex-app-server".to_string(),
                command: "codex app-server".to_string(),
                captures: vec!["final_message".to_string()],
            },
            agent: HarnessTurnAgent {
                id: "for-you-curator".to_string(),
                provider: "openai".to_string(),
                model: "gpt-5.2".to_string(),
                reasoning_effort: "high".to_string(),
                memory_mode: "episodic".to_string(),
                memory_attach: "none".to_string(),
                persistent_session: false,
            },
            job: HarnessTurnJob {
                id: "memory.wiki.for_you_curator".to_string(),
                label: "Curate For You".to_string(),
                inputs: Vec::new(),
                outputs: Vec::new(),
                read_permissions: Vec::new(),
                write_permissions: Vec::new(),
                deny_permissions: Vec::new(),
                completion_done: Vec::new(),
                completion_skip: Vec::new(),
                completion_failure: Vec::new(),
            },
            prompt_bundle: Vec::new(),
            prompt_text: "curate".to_string(),
            mail_context: mail_context_for_unit("literal-run-memory.wiki.for_you_curator", true),
            wiki_context: HarnessWikiContext {
                user_wiki_root: "/tmp/1Context/user-wiki".to_string(),
                source_root: "/tmp/1Context/user-wiki/source".to_string(),
                talk_root: "/tmp/1Context/user-wiki/source/talk".to_string(),
            },
            source_packet: HarnessSourcePacket {
                packet_id: "source-packet.pending".to_string(),
                source: "perception_db".to_string(),
                bounded_by_job: true,
                path: None,
                content_sha256: None,
                bytes: None,
            },
            tool_policy: HarnessToolPolicy {
                default_tools: vec!["agent.mail.read".to_string()],
                agent_tools: Vec::new(),
                job_tools: Vec::new(),
            },
            required_receipts: HarnessReceiptRequirements {
                final_message_path:
                    "context-engine/live/runs/literal-run/turns/op/attempt-0001/final-message.md"
                        .to_string(),
                require_birth_certificate: true,
                require_turn_start: true,
                require_context_injection: true,
                require_adapter_events: true,
                require_final_message: true,
                require_talk_append: true,
                require_mail_delivery: true,
                require_turn_complete: true,
            },
            talk_report: HarnessTalkReport {
                delivery_mode: "mail".to_string(),
                thread_id: "mail://wiki-company".to_string(),
                from: "agent://context-engine/literal-run-memory.wiki.for_you_curator".to_string(),
                to: vec!["mailbox://page/for-you".to_string()],
                cc: vec!["list://wiki-company".to_string()],
            },
        }
    }

    fn complete_receipts() -> HarnessTurnReceipts {
        HarnessTurnReceipts {
            codex_exit_status: Some(0),
            harness_birth_certificate: true,
            harness_turn_start: true,
            context_injection_receipt: true,
            adapter_events: true,
            final_message: Some("status: completed".to_string()),
            talk_receipt: Some("wiki.talk.append:for-you".to_string()),
            mail_receipt: Some("agent-mail://msg-1/delivery-1".to_string()),
            harness_turn_complete: true,
        }
    }

    #[test]
    fn harness_completion_requires_all_required_receipts() {
        let request = literal_request();
        let completion = evaluate_harness_turn_completion(&request, &complete_receipts());

        assert!(completion.complete);
        assert_eq!(completion.status, "complete");
        assert!(completion.issues.is_empty());
    }

    #[test]
    fn harness_completion_rejects_missing_final_message() {
        let request = literal_request();
        let mut receipts = complete_receipts();
        receipts.final_message = Some("   ".to_string());
        let completion = evaluate_harness_turn_completion(&request, &receipts);

        assert!(!completion.complete);
        assert!(completion
            .issues
            .iter()
            .any(|issue| issue.contains("missing non-empty final message")));
    }

    #[test]
    fn harness_completion_rejects_missing_talk_or_mail_receipt() {
        let request = literal_request();
        let mut receipts = complete_receipts();
        receipts.talk_receipt = None;
        receipts.mail_receipt = None;
        let completion = evaluate_harness_turn_completion(&request, &receipts);

        assert!(!completion.complete);
        assert!(completion
            .issues
            .iter()
            .any(|issue| issue.contains("wiki.talk.append")));
        assert!(completion
            .issues
            .iter()
            .any(|issue| issue.contains("Agent Mail delivery")));
    }

    #[test]
    fn harness_completion_rejects_audit_jsonl_as_agent_mail_receipt() {
        let request = literal_request();
        let mut receipts = complete_receipts();
        receipts.mail_receipt =
            Some("context-engine/live/mail/threads/wiki-company.jsonl".to_string());
        let completion = evaluate_harness_turn_completion(&request, &receipts);

        assert!(!completion.complete);
        assert!(completion
            .issues
            .iter()
            .any(|issue| issue == "missing Agent Mail delivery receipt"));
    }

    #[test]
    fn harness_completion_rejects_mail_thread_uri_as_agent_mail_receipt() {
        let request = literal_request();
        let mut receipts = complete_receipts();
        receipts.mail_receipt = Some("mail://wiki-company".to_string());
        let completion = evaluate_harness_turn_completion(&request, &receipts);

        assert!(!completion.complete);
        assert!(completion
            .issues
            .iter()
            .any(|issue| issue == "missing Agent Mail delivery receipt"));
    }

    #[test]
    fn harness_completion_rejects_codex_exit_only() {
        let request = literal_request();
        let receipts = HarnessTurnReceipts {
            codex_exit_status: Some(0),
            ..HarnessTurnReceipts::default()
        };
        let completion = evaluate_harness_turn_completion(&request, &receipts);

        assert!(!completion.complete);
        assert!(completion
            .issues
            .iter()
            .any(|issue| issue == "codex exit status alone is not a completion receipt"));
    }

    #[test]
    fn harness_completion_rejects_failed_codex_worker_turn() {
        let request = literal_request();
        let mut receipts = complete_receipts();
        receipts.codex_exit_status = Some(1);
        let completion = evaluate_harness_turn_completion(&request, &receipts);

        assert!(!completion.complete);
        assert!(completion
            .issues
            .iter()
            .any(|issue| issue == "codex adapter did not report a successful worker turn"));
    }

    #[test]
    fn runtime_turn_request_uses_worker_profile_and_route() {
        use crate::runtime_executor::RuntimeTurnRoute;
        use crate::scheduler::ScheduledReceiptExpectations;
        use std::collections::BTreeMap;

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
        let turn = RuntimeTurnDescriptor {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            kind: "onecontext.context_engine.runtime_turn_descriptor.v1".to_string(),
            run_id: "profile-run".to_string(),
            status: "runnable".to_string(),
            operation_id: "profile-run:scribe-unit".to_string(),
            harness_unit_id: "profile-run-scribe-unit".to_string(),
            harness_turn_id: "turn-profile-run-scribe-unit".to_string(),
            phase_id: "wake_scribes".to_string(),
            job_id: "memory.hourly.scribe".to_string(),
            agent_id: "hourly-scribe".to_string(),
            unit_id: "scribe-unit".to_string(),
            fanout: "once".to_string(),
            max_concurrent: None,
            packet_id: None,
            date: None,
            hour: None,
            bindings: BTreeMap::new(),
            unit_scope: BTreeMap::new(),
            inputs: vec!["source_packet".to_string()],
            outputs: vec!["scribe_artifacts".to_string()],
            required_artifacts: Vec::new(),
            required_mail: Vec::new(),
            route: RuntimeTurnRoute {
                thread_id: "mail://wiki-company".to_string(),
                from_role: "memory.hourly.scribe".to_string(),
                from_mailbox: agent_mail_address_for_unit("profile-run-scribe-unit"),
                to: vec!["role://memory.wiki.historian".to_string()],
                cc: vec!["list://wiki-company".to_string()],
            },
            receipt_expectations: ScheduledReceiptExpectations {
                durable_receipt: "mail://wiki-company".to_string(),
                require_birth_certificate: true,
                require_turn_start: true,
                require_context_injection: true,
                require_adapter_events: true,
                require_final_message: true,
                require_talk_append: true,
                require_mail_delivery: true,
                require_turn_complete: true,
                final_message_path: "final-message.md".to_string(),
                final_message_fields: Vec::new(),
                talk_delivery_mode: "mail".to_string(),
                require_non_empty_final_message: true,
                do_not_count_codex_exit_as_done: true,
            },
            final_message_path:
                "context-engine/live/runs/profile-run/turns/profile-run-scribe-unit/attempt-0001/final-message.md"
                    .to_string(),
            source_packet_path: None,
            source_packet_hash: None,
            condition: None,
        };
        let profile = HarnessWorkerProfile {
            model: "gpt-5.2".to_string(),
            reasoning_effort: "high".to_string(),
            prompt_text: "SCRIBE_PROFILE_PROMPT".to_string(),
            tools: vec!["agent.mail.read".to_string(), "artifact.write".to_string()],
        };

        let request = build_harness_turn_request_for_runtime_turn(&paths, &profile, &turn)
            .expect("build runtime turn request");

        assert_eq!(request.run_id, "profile-run");
        assert_eq!(request.operation_id, turn.operation_id);
        assert_eq!(request.unit_id, turn.harness_unit_id);
        assert_eq!(request.agent.id, "hourly-scribe");
        assert_eq!(request.agent.model, "gpt-5.2");
        assert_eq!(request.agent.reasoning_effort, "high");
        assert_eq!(request.job.id, "memory.hourly.scribe");
        assert_eq!(request.tool_policy.default_tools, profile.tools);
        assert!(request.prompt_text.contains("SCRIBE_PROFILE_PROMPT"));
        assert!(request.prompt_text.contains("## Agent Mail Context"));
        assert_eq!(request.talk_report.from, turn.route.from_mailbox);
        assert_eq!(request.talk_report.to, turn.route.to);
        assert_eq!(request.talk_report.cc, turn.route.cc);
        assert_eq!(
            request.required_receipts.final_message_path,
            turn.final_message_path
        );
    }
}
