//! Harness-owned wiki company agent execution.
//!
//! This module owns the first live bridge from Context Engine planning into
//! actual Codex app-server turns. The 1Context harness owns durable agent
//! identity, receipts, mail, and scheduling. Codex runs one bounded worker turn;
//! native Codex multi-agent V2 is not part of the main-branch runtime path.

use crate::artifacts::ArtifactStore;
use crate::conditions::ConditionContext;
use crate::fanout::FanoutContext;
use crate::harness_executor::{
    build_harness_turn_request_for_runtime_turn, evaluate_harness_turn_completion,
    short_harness_id, HarnessTurnCompletion, HarnessTurnReceipts, HarnessTurnRequest,
    HarnessWorkerProfile,
};
use crate::memoryd_client::{ensure_memory_storage_gate, query_recent_source_events};
use crate::orchestration::ExecutableOrchestrationPlan;
use crate::packet_planner::{build_packet_plan, PacketPlannerPolicy};
use crate::receipt_bridge::{
    agent_mail_receipt_for_job, append_scheduler_receipt, apply_appended_receipt_to_state,
    AgentMailReceiptState, ReceiptAuthority,
};
use crate::run_state::{
    artifact_availability_keys, load_or_new_state, save_state, WikiCompanyRunState,
};
use crate::runtime_executor::{build_runtime_execution_scaffold, RuntimeExecutionScaffoldRequest};
use crate::scheduler::{build_scheduler_run_plan, SchedulerRunRequest};
use crate::source_packets::{
    materialize_source_packets, MaterializedSourcePacket, SourcePacketMaterializationRequest,
};
use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use chrono::{Duration as ChronoDuration, SecondsFormat, Utc};
use onecontext_agent_harness_core::{
    AdapterCorrelation, AdapterEventKind, AdapterEventRequest, AdapterEventStatus, AdapterKind,
    AdapterRedaction, AgentCallRequest, AgentHarnessStore, AgentTurnCompleteRequest,
    AgentTurnCompletionState, AgentTurnId, AgentTurnStartRequest, AgentTurnUsage, AgentUnitId,
    AgentVisibility, CapabilityBinding, CapabilityTransport, HarnessError,
};
use onecontext_agent_mail::{
    AgentGrantPolicy, AgentIdentifyRequest, AgentMailStore, DeliveryState, MailAddress,
    MessageBodyRef, MessageEnvelope, MessagePageRef, SendMailOptions,
};
use onecontext_wiki_core::{TalkAppendRequest, TalkDeliveryMode, WikiCore};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const ORCHESTRATOR_UNIT_ID: &str = "wiki-company-orchestrator-v1";
const DEFAULT_WORKER_TIMEOUT_SECS: u64 = 600;
const WORKER_TIMEOUT_ENV: &str = "ONECONTEXT_CONTEXT_ENGINE_WORKER_TIMEOUT_SECS";
const MAIL_AGENT_LEASE_SECONDS: i64 = 3600;
const CODEX_WORKER_DEVELOPER_INSTRUCTIONS: &str = "Complete this single bounded wiki-company turn. Return final-message.md content only. You may read, open, and claim Agent Mail while working, but final-report and follow-up delivery belongs to the 1Context harness; do not call wiki.talk.append or send Agent Mail for those reports yourself.";

fn worker_timeout_secs() -> u64 {
    std::env::var(WORKER_TIMEOUT_ENV)
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
        .filter(|value| *value >= 30)
        .unwrap_or(DEFAULT_WORKER_TIMEOUT_SECS)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiCompanyAgentExecution {
    pub schema_version: u32,
    pub kind: String,
    pub status: String,
    pub mode: String,
    pub worker_config: Value,
    pub requested_job_count: usize,
    pub completed_job_count: usize,
    pub failed_job_count: usize,
    pub turns: Vec<WikiCompanyAgentTurnResult>,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiCompanyAgentTurnResult {
    pub job_id: String,
    pub agent_id: String,
    pub unit_id: String,
    pub turn_id: String,
    pub codex_thread_id: Option<String>,
    pub codex_turn_id: Option<String>,
    pub codex_thread_resumed: bool,
    pub mail_agent: Option<RegisteredMailAgent>,
    pub worker_config: Value,
    pub final_message_path: String,
    pub final_message_sha256: String,
    pub final_message_origin: String,
    pub talk_receipt_path: String,
    pub mail_receipt_path: String,
    pub follow_up_mail_receipts: Vec<NextAgentRequestReceipt>,
    pub audit_report_path: String,
    pub completion: HarnessTurnCompletion,
    pub assistant_preview: String,
}

impl WikiCompanyAgentTurnResult {
    fn mail_delivery_id(&self) -> Option<String> {
        self.mail_receipt_path
            .rsplit('/')
            .next()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string)
    }

    fn mail_delivery_ids(&self) -> Vec<String> {
        self.mail_delivery_id().into_iter().collect()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TalkMailReceipt {
    pub talk_receipt: String,
    pub mail_receipt: Option<String>,
    pub message_id: String,
    pub delivery_ids: Vec<String>,
    pub result: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NextAgentRequestReceipt {
    pub status: String,
    pub request_text: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub message_id: Option<String>,
    pub delivery_ids: Vec<String>,
    pub mail_receipt: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RegisteredMailAgent {
    pub agent_id: String,
    pub primary_address: String,
    pub transport_kind: String,
    pub transport_thread_id: String,
    pub requested_roles: Vec<String>,
    pub granted_roles: Vec<String>,
    pub requested_capabilities: Vec<String>,
    pub granted_capabilities: Vec<String>,
    pub lease_expires_at: String,
    pub inbox_delivery_count: usize,
    pub thread_message_count: usize,
}

#[derive(Clone, Debug, PartialEq)]
struct MailAgentTurnContext {
    agent: RegisteredMailAgent,
    context_text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct FinalMessageWrite {
    path: PathBuf,
    content_sha256: String,
    origin: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CodexWorkerTurnResult {
    pub status: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub resumed_thread: bool,
    pub mail_agent: Option<RegisteredMailAgent>,
    pub assistant_text: String,
    pub error: Option<String>,
    pub duration_ms: u64,
}

pub fn execute_wiki_company_agents(
    paths: &ContextEnginePaths,
    executable: &ExecutableOrchestrationPlan,
    worker_profiles: &BTreeMap<String, HarnessWorkerProfile>,
    packet_policy: &PacketPlannerPolicy,
    run_id: &str,
    max_agents: u32,
    source_window_days: u32,
) -> Result<WikiCompanyAgentExecution, String> {
    let mut turns = Vec::new();
    let mut issues = Vec::new();
    let mut state = load_or_new_state(paths, run_id)?;

    let live_scaffold = build_live_runtime_scaffold(
        paths,
        executable,
        packet_policy,
        run_id,
        max_agents,
        source_window_days,
        &mut state,
    )?;
    issues.extend(live_scaffold.blocked_reasons.clone());
    let requested_turns = runnable_live_turns(&live_scaffold, max_agents);

    ensure_orchestrator_unit(paths)?;
    let mut completed_operation_ids = BTreeSet::new();
    for turn in &requested_turns {
        let profile = worker_profiles.get(turn.job_id.as_str()).ok_or_else(|| {
            format!("no worker profile provided for job {}", turn.job_id)
        })?;
        let request = build_harness_turn_request_for_runtime_turn(paths, profile, turn)?;
        match execute_harness_turn_request(paths, request) {
            Ok(result) => {
                if !result.completion.complete {
                    issues.extend(result.completion.issues.clone());
                } else {
                    completed_operation_ids.insert(turn.operation_id.clone());
                    if let Err(error) =
                        record_completed_runtime_turn(paths, &mut state, turn, &result)
                    {
                        issues.push(format!("{} state update: {error}", turn.job_id));
                    }
                }
                turns.push(result);
            }
            Err(error) => {
                issues.push(format!("{}: {error}", turn.job_id));
            }
        }
    }
    if let Err(error) =
        complete_batch_phases(paths, &mut state, &live_scaffold, &completed_operation_ids)
    {
        issues.push(format!("phase completion state update: {error}"));
    }

    let completed_job_count = turns.iter().filter(|turn| turn.completion.complete).count();
    let failed_job_count = requested_turns.len().saturating_sub(completed_job_count);
    Ok(WikiCompanyAgentExecution {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.wiki_company_agent_execution".to_string(),
        status: if requested_turns.is_empty() && !issues.is_empty() {
            "blocked"
        } else if failed_job_count == 0 {
            "complete"
        } else {
            "partial"
        }
        .to_string(),
        mode: "tiny_live_harness_first".to_string(),
        worker_config: codex_worker_thread_config(),
        requested_job_count: requested_turns.len(),
        completed_job_count,
        failed_job_count,
        turns,
        issues,
    })
}

pub fn codex_worker_thread_config() -> Value {
    json!({
        "features.multi_agent": false,
        "features.multi_agent_v2": false,
        "features.multi_agent_v2.enabled": false,
    })
}

fn live_job_filter_from_env() -> Option<BTreeSet<String>> {
    let from_env = std::env::var("ONECONTEXT_CONTEXT_ENGINE_LIVE_JOBS")
        .ok()
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>()
        })
        .filter(|jobs| !jobs.is_empty());
    from_env.map(|jobs| jobs.into_iter().collect())
}

fn runnable_live_turns(
    runtime: &crate::runtime_executor::RuntimeExecutionScaffold,
    max_agents: u32,
) -> Vec<crate::runtime_executor::RuntimeTurnDescriptor> {
    let filter = live_job_filter_from_env();
    let mut turns = runtime
        .runnable_turns
        .iter()
        .filter(|turn| {
            filter
                .as_ref()
                .map(|filter| filter.contains(turn.job_id.as_str()))
                .unwrap_or(true)
        })
        .cloned()
        .collect::<Vec<_>>();
    let limit = usize::try_from(max_agents.max(1)).unwrap_or(1);
    turns.truncate(limit.max(1));
    turns
}

fn ensure_orchestrator_unit(paths: &ContextEnginePaths) -> Result<(), String> {
    let store = AgentHarnessStore::new(&paths.root);
    store
        .call(AgentCallRequest {
            unit_id: Some(AgentUnitId(ORCHESTRATOR_UNIT_ID.to_string())),
            parent_unit_id: None,
            spawn_request_id: None,
            role: "1Context wiki company orchestrator".to_string(),
            model: "onecontext-context-engine".to_string(),
            identity: json!({
                "id": ORCHESTRATOR_UNIT_ID,
                "display_name": "Wiki Company Orchestrator"
            }),
            instructions: BTreeMap::from([(
                "charter".to_string(),
                "Own wiki-company agent scheduling, receipts, and mail-first run history."
                    .to_string(),
            )]),
            runtime: json!({
                "runner": "onecontext-context-engine",
                "transport": "agent_harness_core"
            }),
            capabilities: vec![],
            visibility: AgentVisibility::Private,
            metadata: json!({
                "title": "Wiki Company Orchestrator",
                "owner": "context-engine"
            }),
        })
        .map(|_| ())
        .map_err(|error| format!("failed to ensure orchestrator unit: {error}"))
}

fn build_live_runtime_scaffold(
    paths: &ContextEnginePaths,
    executable: &ExecutableOrchestrationPlan,
    packet_policy: &PacketPlannerPolicy,
    run_id: &str,
    max_agents: u32,
    source_window_days: u32,
    state: &mut WikiCompanyRunState,
) -> Result<crate::runtime_executor::RuntimeExecutionScaffold, String> {
    let mut completed_phase_ids = state.completed_phase_ids.clone();
    let mut available_artifacts = state.artifacts.clone();
    let mut selected_packets = load_source_packets_from_artifacts(&available_artifacts);

    if !completed_phase_ids
        .iter()
        .any(|phase| phase == "plan_scribe_packets")
        && selected_packets.is_empty()
    {
        let (packet, source_artifact) =
            materialize_live_source_packet(paths, packet_policy, run_id, source_window_days)?;
        selected_packets.push(packet.clone());
        available_artifacts.push(source_artifact.clone());
        state.add_artifact(source_artifact);
        state.mark_phase_completed("import_perception");
        state.mark_phase_completed("plan_scribe_packets");
        save_state(paths, state)?;
        completed_phase_ids = state.completed_phase_ids.clone();
    }

    let schedule = build_scheduler_run_plan(
        executable,
        SchedulerRunRequest {
            run_id: run_id.to_string(),
            max_concurrent_agents: max_agents,
            completed_phase_ids,
            available_artifacts: artifact_availability_keys(&available_artifacts),
            available_mail: state.available_mail(),
        },
    );
    Ok(build_runtime_execution_scaffold(
        RuntimeExecutionScaffoldRequest {
            run_id: run_id.to_string(),
            schedule,
            fanout_context: FanoutContext {
                run_id: run_id.to_string(),
                selected_packets,
                refreshed_days: Vec::new(),
                available_artifacts,
            },
            condition_context: ConditionContext::with_known_artifacts(state.artifacts.clone()),
        },
    ))
}

fn record_completed_runtime_turn(
    paths: &ContextEnginePaths,
    state: &mut WikiCompanyRunState,
    turn: &crate::runtime_executor::RuntimeTurnDescriptor,
    result: &WikiCompanyAgentTurnResult,
) -> Result<(), String> {
    if let Some(receipt) = agent_mail_scheduler_receipt(turn, result) {
        let append = append_scheduler_receipt(paths, &receipt)?;
        apply_appended_receipt_to_state(state, &receipt, &append);
    }

    let artifact_store = ArtifactStore::from_paths(paths);
    for kind in scheduler_visible_artifact_kinds(turn, result) {
        let key = scheduler_artifact_key(turn, &kind);
        let handle = artifact_store.write_json(
            &turn.run_id,
            &kind,
            &key,
            json!({
                "run_id": turn.run_id,
                "phase_id": turn.phase_id,
                "job_id": turn.job_id,
                "unit_id": turn.unit_id,
                "operation_id": turn.operation_id,
                "final_message_path": result.final_message_path,
                "talk_receipt": result.talk_receipt_path,
                "mail_receipt": result.mail_receipt_path,
                "follow_up_mail_receipts": result.follow_up_mail_receipts,
                "date": turn.date,
                "hour": turn.hour,
                "packet_id": turn.packet_id,
            }),
            BTreeMap::from([
                ("phase_id".to_string(), turn.phase_id.clone()),
                ("job_id".to_string(), turn.job_id.clone()),
                ("unit_id".to_string(), turn.unit_id.clone()),
            ]),
        )?;
        state.add_artifact(handle);
    }

    save_state(paths, state)?;
    Ok(())
}

fn complete_batch_phases(
    paths: &ContextEnginePaths,
    state: &mut WikiCompanyRunState,
    runtime: &crate::runtime_executor::RuntimeExecutionScaffold,
    completed_operation_ids: &BTreeSet<String>,
) -> Result<(), String> {
    if completed_operation_ids.is_empty() {
        return Ok(());
    }

    let mut candidate_phases = runtime
        .runnable_turns
        .iter()
        .filter(|turn| completed_operation_ids.contains(&turn.operation_id))
        .map(|turn| turn.phase_id.clone())
        .collect::<BTreeSet<_>>();
    for phase_id in &state.completed_phase_ids {
        candidate_phases.remove(phase_id);
    }

    for phase_id in candidate_phases {
        let phase_runnable_turns = runtime
            .runnable_turns
            .iter()
            .filter(|turn| turn.phase_id == phase_id)
            .collect::<Vec<_>>();
        let all_runnable_completed = !phase_runnable_turns.is_empty()
            && phase_runnable_turns
                .iter()
                .all(|turn| completed_operation_ids.contains(&turn.operation_id));
        let has_pending_conditional = runtime
            .conditional_turns
            .iter()
            .any(|turn| turn.phase_id == phase_id);
        if all_runnable_completed && !has_pending_conditional {
            state.mark_phase_completed(phase_id);
        }
    }

    save_state(paths, state)?;
    Ok(())
}

fn agent_mail_scheduler_receipt(
    turn: &crate::runtime_executor::RuntimeTurnDescriptor,
    result: &WikiCompanyAgentTurnResult,
) -> Option<crate::receipt_bridge::SchedulerReceipt> {
    let mail_receipt = result.mail_receipt_path.trim();
    if mail_receipt.is_empty() {
        return None;
    }
    let mut receipt = agent_mail_receipt_for_job(
        &turn.run_id,
        &turn.harness_unit_id,
        &turn.phase_id,
        &turn.job_id,
        ReceiptAuthority::AgentMail,
        AgentMailReceiptState::delivered(&turn.route.thread_id, mail_receipt, "wiki-core"),
        json!({
            "talk_receipt": result.talk_receipt_path,
            "mail_receipt": result.mail_receipt_path,
            "mail_delivery_ids": result.mail_delivery_ids(),
            "final_message_path": result.final_message_path,
            "final_message_sha256": result.final_message_sha256,
            "final_message_origin": result.final_message_origin,
        }),
    );
    receipt.path = Some(mail_receipt.to_string());
    receipt.external_id = result.mail_delivery_id();
    Some(receipt)
}

fn scheduler_visible_artifact_kinds(
    turn: &crate::runtime_executor::RuntimeTurnDescriptor,
    result: &WikiCompanyAgentTurnResult,
) -> Vec<String> {
    let mut kinds = BTreeSet::new();
    let skipped = turn_result_status_is_skipped(result);
    match turn.job_id.as_str() {
        "memory.hourly.scribe" => {
            kinds.insert("hourly_talk_entry".to_string());
            kinds.insert("scribe_artifacts".to_string());
        }
        "memory.wiki.historian" => {
            if !skipped {
                for kind in &turn.outputs {
                    if kind != "hourly_answer_questions" {
                        kinds.insert(kind.clone());
                    }
                }
            }
            if result.follow_up_mail_receipts.iter().any(|receipt| {
                receipt.status == "sent"
                    && receipt
                        .to
                        .iter()
                        .any(|to| to == "role://memory.hourly.answerer")
            }) {
                kinds.insert("hourly_answer_questions".to_string());
            }
        }
        "memory.hourly.answerer" => {
            if !skipped {
                kinds.insert("hourly_answer_replies".to_string());
            }
        }
        _ => {
            if !skipped {
                kinds.extend(turn.outputs.iter().cloned());
            }
        }
    }
    kinds.into_iter().collect()
}

fn turn_result_status_is_skipped(result: &WikiCompanyAgentTurnResult) -> bool {
    result
        .assistant_preview
        .lines()
        .next()
        .map(str::trim)
        .is_some_and(|line| line.eq_ignore_ascii_case("status: skipped"))
}

fn scheduler_artifact_key(
    turn: &crate::runtime_executor::RuntimeTurnDescriptor,
    kind: &str,
) -> String {
    if matches!(kind, "scribe_artifacts" | "hourly_talk_entry") {
        if let (Some(date), Some(hour)) = (&turn.date, &turn.hour) {
            return format!("{date}-{hour}");
        }
    }
    if matches!(kind, "hourly_answer_questions" | "hourly_answer_replies") {
        if let (Some(date), Some(hour)) = (&turn.date, &turn.hour) {
            return format!("{date}-{hour}-{}", turn.operation_id);
        }
    }
    turn.date
        .clone()
        .unwrap_or_else(|| turn.operation_id.clone())
}

fn execute_harness_turn_request(
    paths: &ContextEnginePaths,
    mut request: HarnessTurnRequest,
) -> Result<WikiCompanyAgentTurnResult, String> {
    assign_available_final_message_path(paths, &mut request)?;
    let worker_config = codex_worker_thread_config();
    let store = AgentHarnessStore::new(&paths.root);
    let child = store
        .call(child_call_request(&request))
        .map_err(|error| format!("failed to birth child unit {}: {error}", request.unit_id))?;
    let turn_id = AgentTurnId(short_harness_id(&format!("turn-{}", request.operation_id)));
    let start_request = AgentTurnStartRequest {
        unit_id: child.unit_id.clone(),
        turn_id: Some(turn_id.clone()),
        metadata: json!({
            "run_id": request.run_id,
            "job_id": request.job.id,
            "agent_id": request.agent.id,
            "persistent_session": request.agent.persistent_session,
            "context_injection": {
                "mail_context": request.mail_context,
                "wiki_context": request.wiki_context,
                "source_packet": request.source_packet,
            }
        }),
    };
    start_harness_turn_with_stale_recovery(&store, &request, start_request)?;

    let execution_result = (|| -> Result<WikiCompanyAgentTurnResult, String> {
        record_adapter_event(
            &store,
            &child.unit_id,
            AdapterEventKind::ContextInjectionExecuted,
            AdapterEventStatus::Accepted,
            json!({
                "run_id": request.run_id,
                "job_id": request.job.id,
                "prompt_part_count": request.prompt_bundle.len(),
                "prompt_bytes": request.prompt_text.len(),
                "mail_context_appended": request.mail_context.appendix_enabled,
            }),
            AdapterCorrelation::default(),
        )?;

        let worker_result = run_codex_worker_turn(paths, &request, &worker_config)?;
        let adapter_status = if worker_result.status == "completed" {
            AdapterEventStatus::Accepted
        } else {
            AdapterEventStatus::Failed
        };
        record_adapter_event(
            &store,
            &child.unit_id,
            AdapterEventKind::RuntimeWakeupAccepted,
            adapter_status,
            json!({
                "run_id": request.run_id,
                "job_id": request.job.id,
                "codex_status": worker_result.status,
                "codex_error": worker_result.error,
                "assistant_preview": preview(&worker_result.assistant_text, 500),
                "worker_config": worker_config,
                "resumed_thread": worker_result.resumed_thread,
            }),
            AdapterCorrelation {
                thread_id: worker_result.thread_id.clone(),
                turn_id: worker_result.turn_id.clone(),
                ..AdapterCorrelation::default()
            },
        )?;

        let final_message = write_final_message(paths, &request, &worker_result)?;
        let mail_agent = worker_result.mail_agent.as_ref().ok_or_else(|| {
            "Codex worker completed without registered Agent Mail identity".to_string()
        })?;
        let talk_mail_receipt = append_talk_mail_report(
            paths,
            &request,
            &worker_result,
            &final_message.path,
            mail_agent,
        )?;
        let follow_up_mail_receipts = dispatch_next_agent_requests(
            paths,
            &request,
            &worker_result,
            &final_message.path,
            &talk_mail_receipt,
            mail_agent,
        )?;
        let report_path = append_agent_report(
            paths,
            &request,
            &worker_result,
            &final_message,
            &talk_mail_receipt,
            &follow_up_mail_receipts,
        )?;

        store
            .complete_turn(AgentTurnCompleteRequest {
                unit_id: child.unit_id.clone(),
                turn_id: turn_id.clone(),
                usage: AgentTurnUsage {
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: Some(0),
                    duration_ms: worker_result.duration_ms,
                },
                next_state: AgentTurnCompletionState::Ready,
                metadata: json!({
                    "run_id": request.run_id,
                    "job_id": request.job.id,
                    "final_message_path": final_message.path,
                    "final_message_sha256": final_message.content_sha256.clone(),
                    "final_message_origin": final_message.origin.clone(),
                    "talk_receipt": talk_mail_receipt.talk_receipt,
                    "mail_receipt": talk_mail_receipt.mail_receipt,
                    "follow_up_mail_receipts": &follow_up_mail_receipts,
                    "audit_report_path": report_path,
                    "codex_thread_id": worker_result.thread_id,
                    "codex_turn_id": worker_result.turn_id,
                    "codex_thread_resumed": worker_result.resumed_thread,
                    "mail_agent": &worker_result.mail_agent,
                    "worker_config": worker_config,
                }),
            })
            .map_err(|error| format!("failed to complete harness turn {}: {error}", turn_id.0))?;

        let receipts = HarnessTurnReceipts {
            codex_exit_status: Some(if worker_result.status == "completed" {
                0
            } else {
                1
            }),
            harness_birth_certificate: true,
            harness_turn_start: true,
            context_injection_receipt: true,
            adapter_events: true,
            final_message: Some(final_message.path.display().to_string()),
            talk_receipt: Some(talk_mail_receipt.talk_receipt.clone()),
            mail_receipt: talk_mail_receipt.mail_receipt.clone(),
            harness_turn_complete: true,
        };
        let completion = evaluate_harness_turn_completion(&request, &receipts);

        Ok(WikiCompanyAgentTurnResult {
            job_id: request.job.id.clone(),
            agent_id: request.agent.id.clone(),
            unit_id: request.unit_id.clone(),
            turn_id: turn_id.0.clone(),
            codex_thread_id: worker_result.thread_id,
            codex_turn_id: worker_result.turn_id,
            codex_thread_resumed: worker_result.resumed_thread,
            mail_agent: worker_result.mail_agent,
            worker_config: worker_config.clone(),
            final_message_path: final_message.path.display().to_string(),
            final_message_sha256: final_message.content_sha256,
            final_message_origin: final_message.origin,
            talk_receipt_path: talk_mail_receipt.talk_receipt,
            mail_receipt_path: talk_mail_receipt.mail_receipt.unwrap_or_default(),
            follow_up_mail_receipts,
            audit_report_path: report_path.display().to_string(),
            completion,
            assistant_preview: preview(&worker_result.assistant_text, 500),
        })
    })();

    if let Err(error) = &execution_result {
        complete_harness_turn_after_error(&store, &child.unit_id, &turn_id, &request, error)?;
    }

    execution_result
}

fn start_harness_turn_with_stale_recovery(
    store: &AgentHarnessStore,
    request: &HarnessTurnRequest,
    start_request: AgentTurnStartRequest,
) -> Result<(), String> {
    match store.start_turn(start_request.clone()) {
        Ok(_) => Ok(()),
        Err(HarnessError::TurnAlreadyActive {
            unit_id,
            active_turn_id,
        }) => {
            if !complete_stale_harness_turn(store, &unit_id, &active_turn_id, request)? {
                return Err(format!(
                    "failed to start harness turn {}: agent unit {} already has active turn {}",
                    start_request
                        .turn_id
                        .as_ref()
                        .map(|turn_id| turn_id.0.as_str())
                        .unwrap_or("auto"),
                    unit_id.0,
                    active_turn_id.0
                ));
            }
            store
                .start_turn(start_request.clone())
                .map(|_| ())
                .map_err(|error| {
                    format!(
                        "failed to start harness turn {} after stale-turn recovery: {error}",
                        start_request
                            .turn_id
                            .as_ref()
                            .map(|turn_id| turn_id.0.as_str())
                            .unwrap_or("auto")
                    )
                })
        }
        Err(error) => Err(format!(
            "failed to start harness turn {}: {error}",
            start_request
                .turn_id
                .as_ref()
                .map(|turn_id| turn_id.0.as_str())
                .unwrap_or("auto")
        )),
    }
}

fn complete_stale_harness_turn(
    store: &AgentHarnessStore,
    unit_id: &AgentUnitId,
    active_turn_id: &AgentTurnId,
    request: &HarnessTurnRequest,
) -> Result<bool, String> {
    let status = store.agent_status(unit_id).map_err(|error| {
        format!(
            "failed to inspect active harness turn {}: {error}",
            unit_id.0
        )
    })?;
    let age = Utc::now().signed_duration_since(status.unit.metadata.last_active_at);
    let stale_after_secs = worker_timeout_secs();
    let stale_after = ChronoDuration::seconds(stale_after_secs as i64);
    if age < stale_after {
        return Ok(false);
    }
    let duration_ms = age.num_milliseconds().max(0) as u64;
    store
        .complete_turn(AgentTurnCompleteRequest {
            unit_id: unit_id.clone(),
            turn_id: active_turn_id.clone(),
            usage: AgentTurnUsage {
                duration_ms,
                ..AgentTurnUsage::default()
            },
            next_state: AgentTurnCompletionState::Ready,
            metadata: json!({
                "status": "stale_turn_recovered",
                "run_id": request.run_id,
                "job_id": request.job.id,
                "agent_id": request.agent.id,
                "recovered_turn_id": active_turn_id.0,
                "stale_after_seconds": stale_after_secs,
                "age_ms": duration_ms,
            }),
        })
        .map_err(|error| {
            format!(
                "failed to complete stale harness turn {} for unit {}: {error}",
                active_turn_id.0, unit_id.0
            )
        })?;
    Ok(true)
}

fn complete_harness_turn_after_error(
    store: &AgentHarnessStore,
    unit_id: &AgentUnitId,
    turn_id: &AgentTurnId,
    request: &HarnessTurnRequest,
    error: &str,
) -> Result<(), String> {
    store
        .complete_turn(AgentTurnCompleteRequest {
            unit_id: unit_id.clone(),
            turn_id: turn_id.clone(),
            usage: AgentTurnUsage::default(),
            next_state: AgentTurnCompletionState::Ready,
            metadata: json!({
                "status": "worker_turn_failed_cleanup",
                "run_id": request.run_id,
                "job_id": request.job.id,
                "agent_id": request.agent.id,
                "error": error,
            }),
        })
        .map(|_| ())
        .map_err(|cleanup_error| {
            format!(
                "{error}; additionally failed to complete harness turn {} for cleanup: {cleanup_error}",
                turn_id.0
            )
        })
}

fn materialize_live_source_packet(
    paths: &ContextEnginePaths,
    packet_policy: &PacketPlannerPolicy,
    run_id: &str,
    source_window_days: u32,
) -> Result<(MaterializedSourcePacket, crate::artifacts::ArtifactHandle), String> {
    let storage_gate = ensure_memory_storage_gate(paths, source_window_days);
    if !storage_gate.ready_for_source_packets() {
        return Err(format!(
            "tiny live scribe requires ready memory storage before source packet materialization; status={}, storage_ready={}, recent_history_ready={}, message={}, error={}",
            storage_gate.status,
            storage_gate.storage_ready,
            storage_gate.recent_history_ready,
            storage_gate.message.as_deref().unwrap_or("none"),
            storage_gate.error.as_deref().unwrap_or("none")
        ));
    }

    let source_probe = query_recent_source_events(paths, source_window_days);
    if source_probe.status != "ok" || source_probe.source_events.is_empty() {
        return Err(format!(
            "tiny live scribe requires hydrated source events from memoryd; status={}, executable={}, viewport_object_count={}, hydrated_object_count={}, source_event_count={}, density_bucket_count={}, density_object_count={}, error={}",
            source_probe.status,
            source_probe.executable.as_deref().unwrap_or("not found"),
            source_probe.viewport_object_count,
            source_probe.hydrated_object_count,
            source_probe.source_event_count,
            source_probe.density_bucket_count,
            source_probe.density_object_count,
            source_probe.error.as_deref().unwrap_or("none")
        ));
    }

    let mut packet_policy = packet_policy.clone();
    packet_policy.window_days = source_window_days;
    packet_policy.max_packets_per_run = 1;
    let packet_plan =
        build_packet_plan(&source_probe.source_events, packet_policy, &BTreeSet::new());
    let materialized = materialize_source_packets(
        paths,
        &packet_plan,
        &source_probe.source_events,
        SourcePacketMaterializationRequest {
            run_id: run_id.to_string(),
            source_window_days,
            current_pages: Vec::new(),
        },
    )?;
    let packet = materialized.packets.first().cloned().ok_or_else(|| {
        format!(
            "tiny live scribe materialized no source packets; selected_packet_count={}, issues={}",
            packet_plan.selected_packet_count,
            materialized.issues.join("; ")
        )
    })?;

    let artifact_store = ArtifactStore::from_paths(paths);
    let source_artifact = artifact_store.write_json(
        run_id,
        "source_packet",
        &packet.packet_id,
        serde_json::to_value(&packet)
            .map_err(|error| format!("failed to encode source packet artifact: {error}"))?,
        BTreeMap::from([
            ("date".to_string(), packet.date.clone()),
            ("hour".to_string(), packet.hour.clone()),
            ("packet_id".to_string(), packet.packet_id.clone()),
            (
                "source_window_days".to_string(),
                source_window_days.to_string(),
            ),
        ]),
    )?;
    Ok((packet, source_artifact))
}

fn load_source_packets_from_artifacts(
    artifacts: &[crate::artifacts::ArtifactHandle],
) -> Vec<MaterializedSourcePacket> {
    artifacts
        .iter()
        .filter(|artifact| artifact.kind == "source_packet")
        .filter_map(|artifact| {
            let text = fs::read_to_string(&artifact.path).ok()?;
            let record: crate::artifacts::ArtifactRecord = serde_json::from_str(&text).ok()?;
            serde_json::from_value::<MaterializedSourcePacket>(record.value).ok()
        })
        .collect()
}

fn child_call_request(request: &HarnessTurnRequest) -> AgentCallRequest {
    let worker_config = codex_worker_thread_config();
    let spawn_request_id = if request.agent.persistent_session {
        Some(short_harness_id(&format!(
            "spawn-agent-{}",
            request.agent.id
        )))
    } else {
        Some(short_harness_id(&format!("spawn-{}", request.operation_id)))
    };
    let role = if request.agent.persistent_session {
        format!("Wiki company agent: {}", request.agent.id)
    } else {
        request.job.label.clone()
    };
    let identity = if request.agent.persistent_session {
        json!({
            "agent_id": request.agent.id,
            "reasoning_effort": request.agent.reasoning_effort,
            "memory_mode": request.agent.memory_mode,
            "memory_attach": request.agent.memory_attach,
            "persistent_session": true,
        })
    } else {
        json!({
            "agent_id": request.agent.id,
            "job_id": request.job.id,
            "reasoning_effort": request.agent.reasoning_effort,
            "memory_mode": request.agent.memory_mode,
            "memory_attach": request.agent.memory_attach,
            "persistent_session": false,
        })
    };
    let instructions = if request.agent.persistent_session {
        BTreeMap::from([
            (
                "charter".to_string(),
                "You are a persistent 1Context wiki-company agent. Your job-specific prompt arrives on each harness turn; keep continuity through Codex thread resume, harness receipts, and Agent Mail.".to_string(),
            ),
            (
                "final_message_contract".to_string(),
                "Each turn must end with status, evidence, proposed_wiki_talk, next_agent_requests, and next_state_machine_event.".to_string(),
            ),
        ])
    } else {
        BTreeMap::from([
            ("prompt".to_string(), request.prompt_text.clone()),
            (
                "final_message_contract".to_string(),
                "Return a final report with status, evidence, proposed_wiki_talk, next_agent_requests, and next_state_machine_event.".to_string(),
            ),
        ])
    };
    let runtime = if request.agent.persistent_session {
        json!({
            "harness": request.harness,
            "codex_worker_thread_config": worker_config,
            "persistent_codex_thread": true,
        })
    } else {
        json!({
            "harness": request.harness,
            "run_id": request.run_id,
            "operation_id": request.operation_id,
            "codex_worker_thread_config": worker_config,
            "persistent_codex_thread": false,
        })
    };
    let metadata = if request.agent.persistent_session {
        json!({
            "title": format!("Wiki company agent: {}", request.agent.id),
            "agent_id": request.agent.id,
            "persistent_session": true,
        })
    } else {
        json!({
            "title": request.job.label,
            "run_id": request.run_id,
            "job_id": request.job.id,
            "agent_id": request.agent.id,
            "persistent_session": false,
        })
    };
    let tool_names = if request.agent.persistent_session {
        request
            .tool_policy
            .default_tools
            .iter()
            .chain(request.tool_policy.agent_tools.iter())
            .cloned()
            .collect()
    } else {
        request
            .tool_policy
            .default_tools
            .iter()
            .chain(request.tool_policy.agent_tools.iter())
            .chain(request.tool_policy.job_tools.iter())
            .cloned()
            .collect()
    };
    AgentCallRequest {
        unit_id: Some(AgentUnitId(request.unit_id.clone())),
        parent_unit_id: Some(AgentUnitId(ORCHESTRATOR_UNIT_ID.to_string())),
        spawn_request_id,
        role,
        model: request.agent.model.clone(),
        identity,
        instructions,
        runtime,
        capabilities: vec![CapabilityBinding {
            id: "wiki-company-worker-tools".to_string(),
            transport: CapabilityTransport::CodexAppServerDynamicTool,
            tool_names,
            config: json!({}),
            policy: json!({ "bounded_by_job": true }),
            proof_required: vec![
                "context_injection".to_string(),
                "adapter_events".to_string(),
                "final_message".to_string(),
                "talk_append".to_string(),
                "mail_delivery".to_string(),
            ],
        }],
        visibility: AgentVisibility::Private,
        metadata,
    }
}

fn run_codex_worker_turn(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    worker_config: &Value,
) -> Result<CodexWorkerTurnResult, String> {
    let started = Instant::now();
    let mut app = CodexAppServerProcess::spawn()?;
    app.send(
        "init",
        "initialize",
        json!({
            "clientInfo": {
                "name": "onecontext-context-engine",
                "title": "1Context Context Engine",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "experimentalApi": true
            }
        }),
    )?;
    app.read_until_id("init", Duration::from_secs(30))?;

    let cwd = absolute_path(&paths.root)?;
    let base_instructions =
        "You are a harness-born 1Context wiki agent. Do not spawn subagents; the 1Context harness owns orchestration.";
    let thread_open = open_codex_thread(
        paths,
        request,
        &mut app,
        &cwd,
        base_instructions,
        worker_config,
    )?;
    let thread_id = thread_open.thread_id;
    let mail_context = identify_mail_agent_for_turn(paths, request, &thread_id)?;
    let worker_prompt_text = prompt_text_with_registered_mail_context(request, &mail_context);

    app.send(
        "turn",
        "turn/start",
        json!({
            "threadId": thread_id,
            "input": [{
                "type": "text",
                "text": worker_prompt_text,
            }],
            "clientUserMessageId": format!("onecontext-{}", request.operation_id),
            "model": request.agent.model,
            "effort": request.agent.reasoning_effort,
            "additionalContext": {
                "onecontext.mail_context": {
                    "kind": "application",
                    "value": mail_context.context_text.clone(),
                },
                "onecontext.agent_mail_identity": {
                    "kind": "application",
                    "value": serde_json::to_string(&mail_context.agent).unwrap_or_default(),
                },
                "onecontext.harness_receipts": {
                    "kind": "application",
                    "value": serde_json::to_string(&request.required_receipts).unwrap_or_default(),
                }
            }
        }),
    )?;

    let mut assistant_text = String::new();
    let mut codex_turn_id = None;
    let mut final_status = "timeout".to_string();
    let mut error = None;
    let deadline = Duration::from_secs(worker_timeout_secs());
    let start = Instant::now();
    while start.elapsed() < deadline {
        let Some(value) = app.read_next(Duration::from_secs(5))? else {
            continue;
        };
        if value.get("id").and_then(Value::as_str) == Some("turn") {
            codex_turn_id = extract_turn_id(&value).or(codex_turn_id);
        }
        if value.get("method").and_then(Value::as_str) == Some("rawResponseItem/completed") {
            if let Some(text) = extract_assistant_text(&value) {
                if !assistant_text.is_empty() {
                    assistant_text.push('\n');
                }
                assistant_text.push_str(&text);
            }
        }
        if value.get("method").and_then(Value::as_str) == Some("agentMessage/delta") {
            if let Some(delta) = value
                .get("params")
                .and_then(|params| params.get("delta"))
                .and_then(Value::as_str)
            {
                assistant_text.push_str(delta);
            }
        }
        if value.get("method").and_then(Value::as_str) == Some("turn/completed") {
            let turn = value
                .get("params")
                .and_then(|params| params.get("turn"))
                .cloned()
                .unwrap_or(Value::Null);
            codex_turn_id = turn
                .get("id")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
                .or(codex_turn_id);
            final_status = turn
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("completed")
                .to_string();
            error = turn
                .get("error")
                .filter(|error| !error.is_null())
                .map(|error| {
                    serde_json::to_string(error)
                        .unwrap_or_else(|_| "unknown codex error".to_string())
                });
            break;
        }
    }
    app.shutdown();

    if assistant_text.trim().is_empty() && final_status == "completed" {
        assistant_text = "status: completed\n\nevidence:\n- Codex app-server turn completed but did not emit assistant text before completion was observed.\n\nproposed_wiki_talk:\n- No proposed talk text was emitted.\n\nnext_agent_requests:\n- none\n\nnext_state_machine_event: worker_turn_completed".to_string();
    }

    Ok(CodexWorkerTurnResult {
        status: final_status,
        thread_id: Some(thread_id),
        turn_id: codex_turn_id,
        resumed_thread: thread_open.resumed,
        mail_agent: Some(mail_context.agent),
        assistant_text,
        error,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
}

fn identify_mail_agent_for_turn(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    transport_thread_id: &str,
) -> Result<MailAgentTurnContext, String> {
    let store = AgentMailStore::new(&paths.live);
    let requested_roles = requested_mail_roles_for_turn(request)?;
    let requested_capabilities = vec![
        "wiki.mail".to_string(),
        "wiki.talk".to_string(),
        "context_engine.harness".to_string(),
    ];
    let allowed_roles = requested_roles
        .iter()
        .map(|role| {
            MailAddress::parse(role)
                .map(|address| address.canonical())
                .map_err(|error| format!("invalid requested mail role {role:?}: {error:#}"))
        })
        .collect::<Result<BTreeSet<_>, _>>()?;
    let policy = AgentGrantPolicy {
        allowed_roles,
        allowed_capabilities: requested_capabilities.iter().cloned().collect(),
    };
    let now = Utc::now();
    let lease_expires_at = (now + ChronoDuration::seconds(MAIL_AGENT_LEASE_SECONDS))
        .to_rfc3339_opts(SecondsFormat::Secs, true);
    let occurred_at = now.to_rfc3339_opts(SecondsFormat::Secs, true);
    let record = store
        .identify_agent(
            &AgentIdentifyRequest {
                transport_kind: "codex-app-server".to_string(),
                thread_id: transport_thread_id.to_string(),
                requested_roles: requested_roles.clone(),
                requested_capabilities: requested_capabilities.clone(),
                lease_expires_at: lease_expires_at.clone(),
                occurred_at,
            },
            &policy,
        )
        .map_err(|error| {
            format!(
                "failed to identify Agent Mail worker for {} on thread {transport_thread_id}: {error:#}",
                request.operation_id
            )
        })?;

    let inbox_rows = store.agent_inbox(&record.agent_id).map_err(|error| {
        format!(
            "failed to read Agent Mail inbox for {}: {error:#}",
            record.agent_id
        )
    })?;
    let thread_messages = store
        .read_thread(&request.talk_report.thread_id)
        .map_err(|error| {
            format!(
                "failed to read Agent Mail thread {} for {}: {error:#}",
                request.talk_report.thread_id, record.agent_id
            )
        })?;
    let agent = RegisteredMailAgent {
        agent_id: record.agent_id.clone(),
        primary_address: record.primary_address.clone(),
        transport_kind: record.transport.kind.clone(),
        transport_thread_id: record.transport.thread_id.clone(),
        requested_roles: record.requested_roles.clone(),
        granted_roles: record.granted_roles.clone(),
        requested_capabilities: record.requested_capabilities.clone(),
        granted_capabilities: record.granted_capabilities.clone(),
        lease_expires_at,
        inbox_delivery_count: inbox_rows.len(),
        thread_message_count: thread_messages.len(),
    };
    let context_text =
        render_registered_mail_context(&store, request, &agent, &inbox_rows, &thread_messages)?;
    Ok(MailAgentTurnContext {
        agent,
        context_text,
    })
}

fn requested_mail_roles_for_turn(request: &HarnessTurnRequest) -> Result<Vec<String>, String> {
    let mut roles = BTreeSet::new();
    let job_role = format!("role://{}", request.job.id);
    let job_role = MailAddress::parse(&job_role)
        .map_err(|error| {
            format!(
                "job {} does not map to a valid Agent Mail role: {error:#}",
                request.job.id
            )
        })?
        .canonical();
    roles.insert(job_role);

    for address in request
        .talk_report
        .to
        .iter()
        .chain(request.talk_report.cc.iter())
    {
        let parsed = MailAddress::parse(address).map_err(|error| {
            format!(
                "invalid Agent Mail route address {address:?} for {}: {error:#}",
                request.operation_id
            )
        })?;
        match parsed {
            MailAddress::List { .. } | MailAddress::PageMailbox { .. } => {
                roles.insert(parsed.canonical());
            }
            MailAddress::Role { .. } => {}
            _ => {
                return Err(format!(
                    "Agent Mail route address {address:?} for {} is not a schedulable role/list/page recipient",
                    request.operation_id
                ));
            }
        }
    }

    Ok(roles.into_iter().collect())
}

fn prompt_text_with_registered_mail_context(
    request: &HarnessTurnRequest,
    mail_context: &MailAgentTurnContext,
) -> String {
    let mut text = request.prompt_text.clone();
    text.push_str("\n\n");
    text.push_str(&mail_context.context_text);
    text
}

fn render_registered_mail_context(
    store: &AgentMailStore,
    request: &HarnessTurnRequest,
    agent: &RegisteredMailAgent,
    inbox_rows: &[onecontext_agent_mail::InboxRow],
    thread_messages: &[onecontext_agent_mail::HydratedMessage],
) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("## Registered Agent Mail Context\n\n");
    out.push_str("This is the authoritative Agent Mail identity for this harness turn.\n\n");
    out.push_str(&format!("- agent_id: {}\n", agent.agent_id));
    out.push_str(&format!("- primary_address: {}\n", agent.primary_address));
    out.push_str(&format!(
        "- transport: {} / {}\n",
        agent.transport_kind, agent.transport_thread_id
    ));
    out.push_str(&format!("- lease_expires_at: {}\n", agent.lease_expires_at));
    out.push_str(&format!(
        "- granted_roles: {}\n",
        if agent.granted_roles.is_empty() {
            "none".to_string()
        } else {
            agent.granted_roles.join(", ")
        }
    ));
    out.push_str(&format!(
        "- current_thread: {}\n",
        request.talk_report.thread_id
    ));
    out.push_str("\nUse this registered Agent Mail identity for authority. Final reports and follow-up requests are sent by the harness through wiki-core Agent Mail.\n\n");

    out.push_str("### Agent Inbox\n\n");
    let open_rows = inbox_rows
        .iter()
        .filter(|row| delivery_state_is_open(&row.state))
        .take(6)
        .collect::<Vec<_>>();
    if open_rows.is_empty() {
        out.push_str("- no open deliveries\n");
    } else {
        for row in open_rows {
            let message = store.read_message(&row.message_id).map_err(|error| {
                format!(
                    "failed to hydrate inbox message {}: {error:#}",
                    row.message_id
                )
            })?;
            out.push_str(&render_mail_message_summary(
                &row.delivery_id,
                &row.recipient,
                &row.state,
                &message.envelope.subject,
                &message.envelope.from,
                &message.envelope.thread_id,
                &message.body_markdown,
            ));
        }
    }

    out.push_str("\n### Current Thread Messages\n\n");
    if thread_messages.is_empty() {
        out.push_str("- no messages in current thread\n");
    } else {
        for message in thread_messages.iter().rev().take(4).rev() {
            out.push_str(&format!(
                "- message_id: {}\n  from: {}\n  subject: {}\n  body_preview: {}\n",
                message.envelope.message_id,
                message.envelope.from,
                message.envelope.subject,
                single_line_preview(&message.body_markdown, 600)
            ));
        }
    }
    Ok(out)
}

fn delivery_state_is_open(state: &DeliveryState) -> bool {
    matches!(
        state,
        DeliveryState::Unread
            | DeliveryState::Read
            | DeliveryState::Claimed
            | DeliveryState::Snoozed
    )
}

fn render_mail_message_summary(
    delivery_id: &str,
    recipient: &str,
    state: &DeliveryState,
    subject: &str,
    from: &str,
    thread_id: &str,
    body: &str,
) -> String {
    format!(
        "- delivery_id: {delivery_id}\n  recipient: {recipient}\n  state: {state:?}\n  from: {from}\n  subject: {subject}\n  thread_id: {thread_id}\n  body_preview: {}\n",
        single_line_preview(body, 1000)
    )
}

fn single_line_preview(value: &str, max_chars: usize) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct CodexThreadBinding {
    schema_version: u32,
    kind: String,
    agent_id: String,
    unit_id: String,
    thread_id: String,
    model: String,
    cwd: String,
    updated_at: String,
}

#[derive(Clone, Debug, PartialEq)]
struct OpenCodexThread {
    thread_id: String,
    resumed: bool,
}

fn open_codex_thread(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    app: &mut CodexAppServerProcess,
    cwd: &str,
    base_instructions: &str,
    worker_config: &Value,
) -> Result<OpenCodexThread, String> {
    if request.agent.persistent_session {
        if let Some(binding) = read_codex_thread_binding(paths, request)? {
            app.send(
                "thread",
                "thread/resume",
                json!({
                    "threadId": binding.thread_id,
                    "cwd": cwd,
                    "runtimeWorkspaceRoots": [cwd],
                    "model": request.agent.model,
                    "approvalPolicy": "never",
                    "sandbox": "workspace-write",
                    "baseInstructions": base_instructions,
                    "developerInstructions": CODEX_WORKER_DEVELOPER_INSTRUCTIONS,
                    "excludeTurns": true,
                    "config": worker_config,
                }),
            )?;
            if let Ok(thread_response) = app.read_until_id("thread", Duration::from_secs(45)) {
                if let Some(thread_id) = extract_thread_id(&thread_response) {
                    write_codex_thread_binding(paths, request, cwd, &thread_id)?;
                    return Ok(OpenCodexThread {
                        thread_id,
                        resumed: true,
                    });
                }
            }
        }
    }

    app.send(
        "thread",
        "thread/start",
        json!({
            "cwd": cwd,
            "runtimeWorkspaceRoots": [cwd],
            "model": request.agent.model,
            "approvalPolicy": "never",
            "sandbox": "workspace-write",
            "ephemeral": !request.agent.persistent_session,
            "baseInstructions": base_instructions,
            "developerInstructions": CODEX_WORKER_DEVELOPER_INSTRUCTIONS,
            "experimentalRawEvents": true,
            "threadSource": "memory_consolidation",
            "config": worker_config,
        }),
    )?;
    let thread_response = app.read_until_id("thread", Duration::from_secs(45))?;
    let thread_id = extract_thread_id(&thread_response).ok_or_else(|| {
        "codex app-server thread/start response did not include thread.id".to_string()
    })?;
    if request.agent.persistent_session {
        write_codex_thread_binding(paths, request, cwd, &thread_id)?;
    }
    Ok(OpenCodexThread {
        thread_id,
        resumed: false,
    })
}

fn codex_thread_binding_path(paths: &ContextEnginePaths, request: &HarnessTurnRequest) -> PathBuf {
    paths
        .state_codex_app_server_threads
        .join(format!("{}.json", safe_run_id(&request.agent.id)))
}

fn read_codex_thread_binding(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
) -> Result<Option<CodexThreadBinding>, String> {
    let path = codex_thread_binding_path(paths, request);
    if !path.exists() {
        return Ok(None);
    }
    let text = fs::read_to_string(&path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn write_codex_thread_binding(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    cwd: &str,
    thread_id: &str,
) -> Result<PathBuf, String> {
    let path = codex_thread_binding_path(paths, request);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create {}: {error}", parent.display()))?;
    }
    let binding = CodexThreadBinding {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.codex_thread_binding".to_string(),
        agent_id: request.agent.id.clone(),
        unit_id: request.unit_id.clone(),
        thread_id: thread_id.to_string(),
        model: request.agent.model.clone(),
        cwd: cwd.to_string(),
        updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
    };
    let text = serde_json::to_string_pretty(&binding)
        .map_err(|error| format!("failed to encode thread binding: {error}"))?;
    fs::write(&path, format!("{text}\n"))
        .map_err(|error| format!("failed to write {}: {error}", path.display()))?;
    Ok(path)
}

struct CodexAppServerProcess {
    child: Child,
    stdin: ChildStdin,
    lines: Receiver<String>,
}

impl CodexAppServerProcess {
    fn spawn() -> Result<Self, String> {
        let codex_bin =
            std::env::var("ONECONTEXT_CODEX_BIN").unwrap_or_else(|_| "codex".to_string());
        let mut command = Command::new(codex_bin);
        command.arg("app-server");
        let mut child = command
            .arg("--stdio")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("failed to spawn codex app-server: {error}"))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "failed to open codex app-server stdin".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "failed to open codex app-server stdout".to_string())?;
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            use std::io::BufRead;
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            lines: receiver,
        })
    }

    fn send(&mut self, id: &str, method: &str, params: Value) -> Result<(), String> {
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        serde_json::to_writer(&mut self.stdin, &message)
            .map_err(|error| format!("failed to encode app-server request: {error}"))?;
        self.stdin
            .write_all(b"\n")
            .map_err(|error| format!("failed to write app-server request: {error}"))?;
        self.stdin
            .flush()
            .map_err(|error| format!("failed to flush app-server request: {error}"))
    }

    fn read_until_id(&mut self, id: &str, timeout: Duration) -> Result<Value, String> {
        let started = Instant::now();
        while started.elapsed() < timeout {
            if let Some(value) = self.read_next(Duration::from_secs(1))? {
                if value.get("id").and_then(Value::as_str) == Some(id) {
                    if let Some(error) = value.get("error") {
                        return Err(format!(
                            "codex app-server request {id} failed: {}",
                            serde_json::to_string(error).unwrap_or_else(|_| "error".to_string())
                        ));
                    }
                    return Ok(value);
                }
            }
        }
        Err(format!(
            "timed out waiting for codex app-server response {id}"
        ))
    }

    fn read_next(&mut self, timeout: Duration) -> Result<Option<Value>, String> {
        match self.lines.recv_timeout(timeout) {
            Ok(line) => serde_json::from_str::<Value>(&line)
                .map(Some)
                .map_err(|error| format!("failed to parse app-server JSON line: {error}: {line}")),
            Err(mpsc::RecvTimeoutError::Timeout) => Ok(None),
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                Err("codex app-server stdout disconnected".to_string())
            }
        }
    }

    fn shutdown(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn record_adapter_event(
    store: &AgentHarnessStore,
    unit_id: &AgentUnitId,
    kind: AdapterEventKind,
    status: AdapterEventStatus,
    evidence: Value,
    correlation: AdapterCorrelation,
) -> Result<(), String> {
    store
        .record_adapter_event(AdapterEventRequest {
            unit_id: unit_id.clone(),
            adapter: AdapterKind::CodexAppServer,
            kind,
            status,
            correlation,
            evidence,
            redaction: AdapterRedaction::default(),
        })
        .map(|_| ())
        .map_err(|error| format!("failed to record adapter event: {error}"))
}

fn write_final_message(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    worker_result: &CodexWorkerTurnResult,
) -> Result<FinalMessageWrite, String> {
    let requested_path = paths
        .root
        .join(&request.required_receipts.final_message_path);
    let path = next_available_final_message_path(&requested_path)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create final-message dir: {error}"))?;
    }
    let (body, origin) = if worker_result.assistant_text.trim().is_empty() {
        (
            format!(
                "status: {}\n\norigin: harness_synthesized_empty_worker_output\n\nevidence:\n- No assistant text captured.\n\nproposed_wiki_talk:\n- No proposed talk text captured.\n\nnext_agent_requests:\n- none\n\nnext_state_machine_event: worker_turn_{}\n",
                worker_result.status, worker_result.status
            ),
            "harness_synthesized_empty_worker_output".to_string(),
        )
    } else {
        (
            worker_result.assistant_text.clone(),
            "model_output".to_string(),
        )
    };
    let content_sha256 = sha256_text(&body);
    fs::write(&path, body)
        .map_err(|error| format!("failed to write final message {}: {error}", path.display()))?;
    let sha_path = path.with_extension("sha256");
    fs::write(&sha_path, format!("{content_sha256}\n")).map_err(|error| {
        format!(
            "failed to write final message hash {}: {error}",
            sha_path.display()
        )
    })?;
    Ok(FinalMessageWrite {
        path,
        content_sha256,
        origin,
    })
}

fn assign_available_final_message_path(
    paths: &ContextEnginePaths,
    request: &mut HarnessTurnRequest,
) -> Result<(), String> {
    let requested_path = paths
        .root
        .join(&request.required_receipts.final_message_path);
    let path = next_available_final_message_path(&requested_path)?;
    request.required_receipts.final_message_path = path_relative_to_root(paths, &path)?;
    Ok(())
}

fn path_relative_to_root(paths: &ContextEnginePaths, path: &Path) -> Result<String, String> {
    let relative = path.strip_prefix(&paths.root).map_err(|error| {
        format!(
            "final message path {} is outside runtime root {}: {error}",
            path.display(),
            paths.root.display()
        )
    })?;
    Ok(relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

fn next_available_final_message_path(requested_path: &Path) -> Result<PathBuf, String> {
    if !requested_path.exists() {
        return Ok(requested_path.to_path_buf());
    }

    let filename = requested_path.file_name().ok_or_else(|| {
        format!(
            "final message path has no filename: {}",
            requested_path.display()
        )
    })?;
    let Some(attempt_dir) = requested_path.parent() else {
        return Err(format!(
            "final message path has no attempt directory: {}",
            requested_path.display()
        ));
    };
    let Some(turn_dir) = attempt_dir.parent() else {
        return Err(format!(
            "final message path has no turn directory: {}",
            requested_path.display()
        ));
    };

    for attempt in 2..=9999 {
        let candidate = turn_dir
            .join(format!("attempt-{attempt:04}"))
            .join(filename);
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    Err(format!(
        "no free final-message attempt path under {}",
        turn_dir.display()
    ))
}

fn append_talk_mail_report(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    worker_result: &CodexWorkerTurnResult,
    final_message_path: &Path,
    mail_agent: &RegisteredMailAgent,
) -> Result<TalkMailReceipt, String> {
    let body_markdown = fs::read_to_string(final_message_path)
        .map_err(|error| format!("failed to read final message for talk append: {error}"))?;
    let delivery_mode = match request.talk_report.delivery_mode.as_str() {
        "mail" => TalkDeliveryMode::Mail,
        other => {
            return Err(format!(
                "wiki-company talk report requires delivery_mode=mail, got {other:?}"
            ))
        }
    };
    if MailAddress::parse(&mail_agent.primary_address).is_err() {
        return Err(format!(
            "invalid Agent Mail sender address {:?}",
            mail_agent.primary_address
        ));
    }
    let page = talk_page_for_request(request);
    let (to, dropped_to) = normalize_mail_addresses(&request.talk_report.to);
    let (cc, dropped_cc) = normalize_mail_addresses(&request.talk_report.cc);
    if to.is_empty() && cc.is_empty() {
        return Err(format!(
            "no valid Agent Mail recipients for {}; route_to={:?}; route_cc={:?}",
            request.operation_id, request.talk_report.to, request.talk_report.cc
        ));
    }
    let subject = format!("{} final report", request.job.label);
    let result = WikiCore::new(&paths.root)
        .append_talk(TalkAppendRequest {
            page: page.clone(),
            kind: "agent_report".to_string(),
            subject,
            operation_id: Some(harness_talk_report_operation_id(&request.operation_id)),
            delivery_mode,
            thread_id: Some(request.talk_report.thread_id.clone()),
            reply_to: None,
            from: mail_agent.primary_address.clone(),
            to,
            cc,
            body_markdown,
            attachments: Vec::new(),
            allow_tombstoned: false,
        })
        .map_err(|error| {
            format!(
                "failed to append Agent Mail-backed wiki talk report for {} on page {page}: {error:#}",
                request.operation_id
            )
        })?;

    let delivery_ids = result
        .mail_delivery
        .as_ref()
        .map(|delivery| {
            delivery
                .attempts
                .iter()
                .filter(|attempt| {
                    attempt.status == "delivered" || attempt.status == "already_delivered"
                })
                .map(|attempt| attempt.delivery_id.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mail_receipt = delivery_ids
        .first()
        .map(|delivery_id| format!("agent-mail://{}/{}", result.message_id, delivery_id));
    let mut result_value = serde_json::to_value(&result)
        .map_err(|error| format!("failed to encode talk append receipt: {error}"))?;
    if !dropped_to.is_empty() || !dropped_cc.is_empty() {
        result_value["onecontext_dropped_invalid_route_addresses"] = json!({
            "to": dropped_to,
            "cc": dropped_cc,
        });
    }
    if worker_result.status != "completed" {
        result_value["onecontext_worker_status"] = json!(worker_result.status);
    }
    Ok(TalkMailReceipt {
        talk_receipt: result.source.clone(),
        mail_receipt,
        message_id: result.message_id,
        delivery_ids,
        result: result_value,
    })
}

fn dispatch_next_agent_requests(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    worker_result: &CodexWorkerTurnResult,
    final_message_path: &Path,
    parent_receipt: &TalkMailReceipt,
    mail_agent: &RegisteredMailAgent,
) -> Result<Vec<NextAgentRequestReceipt>, String> {
    let requests = parse_next_agent_requests(&worker_result.assistant_text);
    if requests.is_empty() {
        return Ok(Vec::new());
    }
    let store = AgentMailStore::new(&paths.live);
    let mut receipts = Vec::new();
    for (index, request_text) in requests.iter().enumerate() {
        let targets = match resolve_next_agent_request_targets(request_text) {
            Ok(targets) => targets,
            Err(error) => {
                receipts.push(NextAgentRequestReceipt {
                    status: "skipped".to_string(),
                    request_text: request_text.clone(),
                    to: Vec::new(),
                    cc: Vec::new(),
                    message_id: None,
                    delivery_ids: Vec::new(),
                    mail_receipt: None,
                    error: Some(error),
                });
                continue;
            }
        };
        let cc = follow_up_cc_addresses();
        let body_markdown = render_next_agent_request_body(
            request,
            request_text,
            final_message_path,
            parent_receipt,
            mail_agent,
        );
        let message_id = stable_mail_message_id(&format!(
            "{}:{}:{}",
            request.operation_id, index, request_text
        ));
        let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
        let envelope = MessageEnvelope {
            schema_version: 1,
            message_id: message_id.clone(),
            idempotency_key: format!(
                "{}:next-agent-request:{}:{}",
                request.operation_id,
                index,
                short_hash(request_text, 12)
            ),
            kind: "agent_request".to_string(),
            subject: format!("Next agent request from {}", request.job.label),
            from: mail_agent.primary_address.clone(),
            to: targets.clone(),
            cc: cc.clone(),
            page: follow_up_page_ref(&targets),
            thread_id: request.talk_report.thread_id.clone(),
            reply_to: Some(parent_receipt.message_id.clone()),
            body: MessageBodyRef {
                format: "markdown".to_string(),
                sha256: sha256_hex(body_markdown.as_bytes()),
            },
            attachments: Vec::new(),
            created_at,
        };
        match store.send_mail(&envelope, &body_markdown, &SendMailOptions::default()) {
            Ok(send_receipt) => {
                let delivery_ids = send_receipt
                    .attempts
                    .iter()
                    .filter(|attempt| {
                        matches!(
                            attempt.status,
                            onecontext_agent_mail::DeliveryAttemptStatus::Delivered
                                | onecontext_agent_mail::DeliveryAttemptStatus::AlreadyDelivered
                        )
                    })
                    .map(|attempt| attempt.delivery_id.clone())
                    .collect::<Vec<_>>();
                let mail_receipt = delivery_ids
                    .first()
                    .map(|delivery_id| format!("agent-mail://{message_id}/{delivery_id}"));
                receipts.push(NextAgentRequestReceipt {
                    status: if delivery_ids.is_empty() {
                        "deferred".to_string()
                    } else {
                        "sent".to_string()
                    },
                    request_text: request_text.clone(),
                    to: targets,
                    cc,
                    message_id: Some(message_id),
                    delivery_ids,
                    mail_receipt,
                    error: None,
                });
            }
            Err(error) => receipts.push(NextAgentRequestReceipt {
                status: "failed".to_string(),
                request_text: request_text.clone(),
                to: targets,
                cc,
                message_id: Some(message_id),
                delivery_ids: Vec::new(),
                mail_receipt: None,
                error: Some(error.to_string()),
            }),
        }
    }
    Ok(receipts)
}

fn parse_next_agent_requests(text: &str) -> Vec<String> {
    let mut in_section = false;
    let mut requests = Vec::<String>::new();
    for line in text.lines() {
        let trimmed = line.trim();
        if is_next_agent_requests_heading(trimmed) {
            in_section = true;
            continue;
        }
        if !in_section {
            continue;
        }
        if is_section_boundary(trimmed) {
            break;
        }
        if trimmed.is_empty() {
            continue;
        }
        if let Some(item) = strip_list_marker(trimmed) {
            if !is_empty_next_agent_request(item) {
                requests.push(item.to_string());
            }
        } else if let Some(last) = requests.last_mut() {
            last.push(' ');
            last.push_str(trimmed);
        } else if !is_empty_next_agent_request(trimmed) {
            requests.push(trimmed.to_string());
        }
    }
    requests
}

fn is_next_agent_requests_heading(line: &str) -> bool {
    normalized_heading_key(line) == "next_agent_requests"
}

fn is_section_boundary(line: &str) -> bool {
    if line.is_empty() {
        return false;
    }
    let key = normalized_heading_key(line);
    matches!(
        key.as_str(),
        "status"
            | "evidence"
            | "page_change_summary"
            | "link_and_page_intent"
            | "topic_page_decisions"
            | "contradictions_or_stale_claims"
            | "decisions"
            | "proposed_wiki_talk"
            | "next_state_machine_event"
    ) || (line.starts_with('#') && key != "next_agent_requests")
}

fn normalized_heading_key(line: &str) -> String {
    line.trim_start_matches('#')
        .trim()
        .trim_end_matches(':')
        .to_ascii_lowercase()
        .replace([' ', '-'], "_")
}

fn strip_list_marker(line: &str) -> Option<&str> {
    let stripped = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "));
    if stripped.is_some() {
        return stripped.map(str::trim);
    }
    let Some((prefix, rest)) = line.split_once(". ") else {
        return None;
    };
    if prefix.chars().all(|ch| ch.is_ascii_digit()) {
        Some(rest.trim())
    } else {
        None
    }
}

fn is_empty_next_agent_request(value: &str) -> bool {
    let normalized = value
        .trim()
        .trim_matches(|ch: char| matches!(ch, '<' | '>' | '.' | '-'))
        .to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "" | "none" | "no requests" | "n/a" | "na" | "no next agent requests"
    )
}

fn resolve_next_agent_request_targets(request_text: &str) -> Result<Vec<String>, String> {
    let explicit = explicit_mail_addresses(request_text);
    if !explicit.is_empty() {
        return Ok(explicit);
    }
    let lower = request_text.to_ascii_lowercase().replace(['_', '-'], " ");
    let mut targets = BTreeSet::new();
    for (needle, address) in [
        ("for you curator", "role://memory.wiki.for_you_curator"),
        ("context curator", "role://memory.wiki.context_curator"),
        ("librarian", "role://memory.wiki.librarian"),
        ("biographer", "role://memory.wiki.biographer"),
        ("historian", "role://memory.wiki.historian"),
        ("hourly answerer", "role://memory.hourly.answerer"),
        ("answerer", "role://memory.hourly.answerer"),
        ("for you editor", "role://memory.wiki.for_you_editor"),
        ("editor", "role://memory.wiki.for_you_editor"),
        ("scribe", "role://memory.hourly.scribe"),
    ] {
        if lower.contains(needle) {
            targets.insert(address.to_string());
        }
    }
    if lower.contains("curator") && targets.is_empty() {
        return Err("ambiguous next_agent_requests target: curator could mean for_you_curator or context_curator".to_string());
    }
    if targets.is_empty() {
        return Err(
            "next_agent_requests item has no explicit Agent Mail address or known role alias"
                .to_string(),
        );
    }
    Ok(targets.into_iter().collect())
}

fn explicit_mail_addresses(value: &str) -> Vec<String> {
    let mut addresses = BTreeSet::new();
    for token in value.split(|ch: char| {
        ch.is_whitespace() || matches!(ch, ',' | ';' | ')' | '(' | '[' | ']' | '{' | '}')
    }) {
        let candidate = token
            .trim_matches(|ch: char| matches!(ch, '.' | ':' | '"' | '\'' | '`'))
            .trim();
        if !candidate.contains("://") {
            continue;
        }
        if MailAddress::parse(candidate).is_ok() {
            addresses.insert(candidate.to_string());
        }
    }
    addresses.into_iter().collect()
}

fn follow_up_cc_addresses() -> Vec<String> {
    vec!["list://wiki-company".to_string()]
}

fn follow_up_page_ref(targets: &[String]) -> Option<MessagePageRef> {
    targets
        .iter()
        .find_map(|target| target.strip_prefix("mailbox://page/"))
        .map(|page_id| MessagePageRef {
            id: page_id.to_string(),
            route: format!("mailbox://page/{page_id}"),
        })
}

fn render_next_agent_request_body(
    request: &HarnessTurnRequest,
    request_text: &str,
    final_message_path: &Path,
    parent_receipt: &TalkMailReceipt,
    mail_agent: &RegisteredMailAgent,
) -> String {
    format!(
        "## Next Agent Request\n\n{}\n\n## Source\n\n- source_job_id: {}\n- source_agent_id: {}\n- source_mail_agent_id: {}\n- source_primary_address: {}\n- operation_id: {}\n- final_message_path: {}\n- parent_message_id: {}\n- parent_talk_receipt: {}\n- parent_mail_receipt: {}\n",
        request_text,
        request.job.id,
        request.agent.id,
        mail_agent.agent_id,
        mail_agent.primary_address,
        request.operation_id,
        final_message_path.display(),
        parent_receipt.message_id,
        parent_receipt.talk_receipt,
        parent_receipt.mail_receipt.as_deref().unwrap_or("none")
    )
}

fn stable_mail_message_id(value: &str) -> String {
    format!("mailmsg_{}", short_hash(value, 20))
}

fn harness_talk_report_operation_id(operation_id: &str) -> String {
    format!("{operation_id}:harness-final-report")
}

fn short_hash(value: &str, chars: usize) -> String {
    let digest = sha256_hex(value.as_bytes());
    digest.chars().take(chars).collect()
}

fn sha256_text(value: &str) -> String {
    sha256_hex(value.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hash = Sha256::new();
    hash.update(bytes);
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn normalize_mail_addresses(addresses: &[String]) -> (Vec<String>, Vec<String>) {
    let mut accepted = Vec::new();
    let mut dropped = Vec::new();
    for address in addresses {
        if MailAddress::parse(address).is_ok() {
            accepted.push(address.clone());
        } else {
            dropped.push(address.clone());
        }
    }
    (accepted, dropped)
}

fn talk_page_for_request(request: &HarnessTurnRequest) -> String {
    request
        .talk_report
        .to
        .iter()
        .chain(request.talk_report.cc.iter())
        .find_map(|address| address.strip_prefix("mailbox://page/"))
        .map(ToOwned::to_owned)
        .or_else(|| job_default_talk_page(&request.job.id).map(ToOwned::to_owned))
        .unwrap_or_else(|| "for-you".to_string())
}

fn job_default_talk_page(job_id: &str) -> Option<&'static str> {
    if job_id.contains("context_curator") || job_id.contains("biographer") {
        Some("your-context")
    } else if job_id.contains("librarian")
        || job_id.contains("concept")
        || job_id.contains("contradiction")
    {
        Some("topics")
    } else {
        Some("for-you")
    }
}

fn append_agent_report(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    worker_result: &CodexWorkerTurnResult,
    final_message: &FinalMessageWrite,
    talk_mail_receipt: &TalkMailReceipt,
    follow_up_mail_receipts: &[NextAgentRequestReceipt],
) -> Result<PathBuf, String> {
    if let Some(parent) = paths.wiki_company_mail_thread.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create mail thread dir: {error}"))?;
    }
    let receipt = json!({
        "schema_version": CONTEXT_ENGINE_SCHEMA_VERSION,
        "kind": "onecontext.context_engine.wiki_company_agent_report",
        "created_at": Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        "thread_id": "mail://wiki-company",
        "operation_id": request.operation_id,
        "delivery_mode": request.talk_report.delivery_mode,
        "configured_from": request.talk_report.from,
        "from": worker_result
            .mail_agent
            .as_ref()
            .map(|agent| agent.primary_address.as_str())
            .unwrap_or(request.talk_report.from.as_str()),
        "to": request.talk_report.to,
        "cc": request.talk_report.cc,
        "job_id": request.job.id,
        "agent_id": request.agent.id,
        "unit_id": request.unit_id,
        "codex_thread_id": worker_result.thread_id,
        "codex_turn_id": worker_result.turn_id,
        "mail_agent": &worker_result.mail_agent,
        "status": worker_result.status,
        "final_message_path": final_message.path,
        "final_message_sha256": final_message.content_sha256,
        "final_message_origin": final_message.origin,
        "talk_receipt": talk_mail_receipt.talk_receipt,
        "mail_receipt": talk_mail_receipt.mail_receipt,
        "mail_delivery_ids": talk_mail_receipt.delivery_ids,
        "follow_up_mail_receipts": follow_up_mail_receipts,
        "talk_append_result": talk_mail_receipt.result,
        "assistant_preview": preview(&worker_result.assistant_text, 1000),
    });
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.wiki_company_mail_thread)
        .map_err(|error| format!("failed to open mail thread: {error}"))?;
    serde_json::to_writer(&mut file, &receipt)
        .map_err(|error| format!("failed to encode mail receipt: {error}"))?;
    file.write_all(b"\n")
        .map_err(|error| format!("failed to append mail receipt: {error}"))?;
    Ok(paths.wiki_company_mail_thread.clone())
}

fn extract_thread_id(value: &Value) -> Option<String> {
    value
        .get("result")
        .and_then(|result| result.get("thread"))
        .and_then(|thread| thread.get("id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("result")
                .and_then(|result| result.get("threadId"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

fn extract_turn_id(value: &Value) -> Option<String> {
    value
        .get("result")
        .and_then(|result| result.get("turn"))
        .and_then(|turn| turn.get("id"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| {
            value
                .get("result")
                .and_then(|result| result.get("turnId"))
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
}

fn extract_assistant_text(value: &Value) -> Option<String> {
    let item = value.get("params")?.get("item")?;
    if item.get("role").and_then(Value::as_str) != Some("assistant") {
        return None;
    }
    let content = item.get("content")?.as_array()?;
    let mut out = String::new();
    for part in content {
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(text);
        }
    }
    if out.trim().is_empty() {
        None
    } else {
        Some(out)
    }
}

fn absolute_path(path: &Path) -> Result<String, String> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|error| format!("failed to resolve cwd: {error}"))?
            .join(path)
    };
    Ok(absolute.display().to_string())
}

fn preview(value: &str, max_chars: usize) -> String {
    value.chars().take(max_chars).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worker_thread_config_is_harness_only() {
        let config = codex_worker_thread_config();
        assert_eq!(config.get("onecontext_worker_turn"), None);
        assert_eq!(
            config["features.multi_agent"],
            serde_json::Value::Bool(false)
        );
        assert!(multi_agent_v2_disabled(&config));
        assert_eq!(config.get("tools"), None);
        assert_eq!(config.get("enabled_tools"), None);
        assert_eq!(config.get("disabled_tools"), None);
        assert_eq!(
            config.get("features.multi_agent_v2.encrypted_messages"),
            None
        );
        assert_eq!(config.get("features.code_mode"), None);
        assert_eq!(config.get("features.code_mode_only"), None);
    }

    #[test]
    fn codex_worker_developer_instructions_preserve_mail_ownership_boundary() {
        assert!(CODEX_WORKER_DEVELOPER_INSTRUCTIONS.contains("final-message.md content only"));
        assert!(CODEX_WORKER_DEVELOPER_INSTRUCTIONS.contains("read, open, and claim Agent Mail"));
        assert!(CODEX_WORKER_DEVELOPER_INSTRUCTIONS
            .contains("final-report and follow-up delivery belongs to the 1Context harness"));
        assert!(CODEX_WORKER_DEVELOPER_INSTRUCTIONS
            .contains("do not call wiki.talk.append or send Agent Mail"));
    }

    fn multi_agent_v2_disabled(config: &Value) -> bool {
        let values = [
            config
                .get("features.multi_agent_v2")
                .and_then(Value::as_bool),
            config
                .get("features.multi_agent_v2.enabled")
                .and_then(Value::as_bool),
        ];
        values.iter().flatten().any(|value| !*value) && !values.iter().flatten().any(|value| *value)
    }

    #[test]
    fn next_agent_requests_parser_reads_snake_case_section() {
        let text = r#"
status: completed

next_agent_requests:
- Ask the biographer to draft questions about the work history.
- role://memory.wiki.librarian should review topic placement.

next_state_machine_event:
wiki.editor.reported
"#;

        assert_eq!(
            parse_next_agent_requests(text),
            vec![
                "Ask the biographer to draft questions about the work history.",
                "role://memory.wiki.librarian should review topic placement."
            ]
        );
    }

    #[test]
    fn next_agent_requests_parser_reads_markdown_heading_and_continuation() {
        let text = r#"
## Proposed Wiki Talk
Done.

## Next Agent Requests
- Ask the context curator to verify this summary
  against the your-context page.

## Next State Machine Event
wiki.editor.reported
"#;

        assert_eq!(
            parse_next_agent_requests(text),
            vec!["Ask the context curator to verify this summary against the your-context page."]
        );
    }

    #[test]
    fn next_agent_requests_parser_ignores_none_sentinel() {
        assert!(parse_next_agent_requests("next_agent_requests:\n- none\n").is_empty());
    }

    #[test]
    fn next_agent_request_targets_use_explicit_addresses_first() {
        assert_eq!(
            resolve_next_agent_request_targets(
                "Please send this to list://wiki-company and role://memory.wiki.biographer."
            )
            .unwrap(),
            vec![
                "list://wiki-company".to_string(),
                "role://memory.wiki.biographer".to_string()
            ]
        );
    }

    #[test]
    fn next_agent_request_targets_reject_ambiguous_curator() {
        let error =
            resolve_next_agent_request_targets("Ask the curator to look at this").unwrap_err();
        assert!(error.contains("ambiguous"));
    }

    #[test]
    fn next_agent_request_targets_resolve_known_role_aliases() {
        assert_eq!(
            resolve_next_agent_request_targets("Ask the context curator to review").unwrap(),
            vec!["role://memory.wiki.context_curator"]
        );
        assert_eq!(
            resolve_next_agent_request_targets("Ask the hourly answerer to reply").unwrap(),
            vec!["role://memory.hourly.answerer"]
        );
    }

    #[test]
    fn harness_final_report_operation_id_is_namespaced_from_worker_turn_id() {
        assert_eq!(
            harness_talk_report_operation_id(
                "live-historian-loop-20260608-122121:memory.wiki.historian"
            ),
            "live-historian-loop-20260608-122121:memory.wiki.historian:harness-final-report"
        );
    }

    #[test]
    fn final_message_retry_allocates_next_attempt_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let requested = temp
            .path()
            .join("context-engine/live/runs/run-a/turns/op-a/attempt-0001/final-message.md");
        fs::create_dir_all(requested.parent().expect("attempt dir")).expect("mkdir");
        fs::write(&requested, "first attempt").expect("write first");

        let retry = next_available_final_message_path(&requested).expect("retry path");

        assert!(retry.ends_with("turns/op-a/attempt-0002/final-message.md"));
    }

    #[test]
    fn harness_request_reserves_available_final_message_attempt_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
        let mut request = harness_request_for_mail_roles(
            "memory.wiki.historian",
            vec!["role://memory.hourly.answerer"],
            vec![],
        );
        let first_attempt = paths
            .root
            .join(&request.required_receipts.final_message_path);
        fs::create_dir_all(first_attempt.parent().expect("attempt dir")).expect("mkdir");
        fs::write(&first_attempt, "first attempt").expect("write first attempt");

        assign_available_final_message_path(&paths, &mut request).expect("assign path");

        assert_eq!(
            request.required_receipts.final_message_path,
            "context-engine/live/runs/run-a/turns/op-a/attempt-0002/final-message.md"
        );
        assert!(!paths
            .root
            .join(&request.required_receipts.final_message_path)
            .exists());
    }

    #[test]
    fn skipped_historian_does_not_create_answer_question_artifact() {
        let turn = historian_runtime_turn();
        let result = agent_turn_result(
            "status: skipped\n\nnext_agent_requests:\n- role://memory.wiki.for_you_editor: do not promote",
            vec![next_agent_request_receipt(vec!["role://memory.wiki.for_you_editor"])],
        );

        let kinds = scheduler_visible_artifact_kinds(&turn, &result);

        assert!(!kinds.contains(&"hourly_answer_questions".to_string()));
        assert!(!kinds.contains(&"historian_talk_entries".to_string()));
    }

    #[test]
    fn historian_answerer_request_creates_answer_question_artifact() {
        let turn = historian_runtime_turn();
        let result = agent_turn_result(
            "status: completed\n\nnext_agent_requests:\n- role://memory.hourly.answerer: answer the question",
            vec![next_agent_request_receipt(vec!["role://memory.hourly.answerer"])],
        );

        let kinds = scheduler_visible_artifact_kinds(&turn, &result);

        assert!(kinds.contains(&"hourly_answer_questions".to_string()));
        assert!(kinds.contains(&"historian_talk_entries".to_string()));
    }

    #[test]
    fn live_job_filter_is_optional_and_env_driven() {
        std::env::remove_var("ONECONTEXT_CONTEXT_ENGINE_LIVE_JOBS");
        assert_eq!(live_job_filter_from_env(), None);

        std::env::set_var(
            "ONECONTEXT_CONTEXT_ENGINE_LIVE_JOBS",
            "memory.hourly.scribe,memory.hourly.answerer",
        );
        let filter = live_job_filter_from_env().expect("env filter");
        assert!(filter.contains("memory.hourly.scribe"));
        assert!(filter.contains("memory.hourly.answerer"));
        assert!(!filter.contains("memory.wiki.historian"));
        std::env::remove_var("ONECONTEXT_CONTEXT_ENGINE_LIVE_JOBS");
    }

    #[test]
    fn phase_completion_waits_for_all_runnable_turns_in_batch() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
        let mut state = WikiCompanyRunState::new("run-a");
        let mut first = historian_runtime_turn();
        first.phase_id = "wake_scribes".to_string();
        first.operation_id = "op-a".to_string();
        let mut second = first.clone();
        second.operation_id = "op-b".to_string();
        second.unit_id = "memory.hourly.scribe-2026-06-08-10".to_string();
        let runtime = runtime_with_turns(vec![first, second], Vec::new());

        complete_batch_phases(
            &paths,
            &mut state,
            &runtime,
            &BTreeSet::from(["op-a".to_string()]),
        )
        .expect("partial batch state write");

        assert!(!state
            .completed_phase_ids
            .contains(&"wake_scribes".to_string()));

        complete_batch_phases(
            &paths,
            &mut state,
            &runtime,
            &BTreeSet::from(["op-a".to_string(), "op-b".to_string()]),
        )
        .expect("complete batch state write");

        assert!(state
            .completed_phase_ids
            .contains(&"wake_scribes".to_string()));
    }

    #[test]
    fn phase_completion_waits_for_pending_conditional_turns() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
        let mut state = WikiCompanyRunState::new("run-a");
        let mut runnable = historian_runtime_turn();
        runnable.phase_id = "specialists".to_string();
        runnable.operation_id = "op-biographer".to_string();
        let mut conditional = runnable.clone();
        conditional.operation_id = "op-contradiction".to_string();
        conditional.job_id = "memory.wiki.contradiction_flagger".to_string();
        conditional.status = "conditional".to_string();
        let runtime = runtime_with_turns(vec![runnable], vec![conditional]);

        complete_batch_phases(
            &paths,
            &mut state,
            &runtime,
            &BTreeSet::from(["op-biographer".to_string()]),
        )
        .expect("conditional batch state write");

        assert!(!state
            .completed_phase_ids
            .contains(&"specialists".to_string()));
    }

    #[test]
    fn requested_mail_roles_include_contextual_list_and_page_grants_only() {
        let request = harness_request_for_mail_roles(
            "memory.wiki.historian",
            vec!["role://memory.hourly.answerer"],
            vec!["list://wiki-company", "mailbox://page/for-you"],
        );

        let roles = requested_mail_roles_for_turn(&request).expect("mail roles");

        assert!(roles.contains(&"role://memory.wiki.historian".to_string()));
        assert!(roles.contains(&"list://wiki-company".to_string()));
        assert!(roles.contains(&"mailbox://page/for-you".to_string()));
        assert!(!roles.contains(&"role://memory.hourly.answerer".to_string()));
    }

    fn historian_runtime_turn() -> crate::runtime_executor::RuntimeTurnDescriptor {
        crate::runtime_executor::RuntimeTurnDescriptor {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            kind: "onecontext.context_engine.runtime_turn_descriptor.v1".to_string(),
            run_id: "run-a".to_string(),
            status: "runnable".to_string(),
            operation_id: "run-a-memory.wiki.historian-2026-06-08".to_string(),
            harness_unit_id: "run-a-memory.wiki.historian-2026-06-08".to_string(),
            harness_turn_id: "turn-run-a-memory.wiki.historian-2026-06-08".to_string(),
            phase_id: "historian_questions".to_string(),
            job_id: "memory.wiki.historian".to_string(),
            agent_id: "wiki-historian".to_string(),
            unit_id: "memory.wiki.historian-2026-06-08".to_string(),
            fanout: "refreshed_days".to_string(),
            max_concurrent: Some(1),
            packet_id: None,
            date: Some("2026-06-08".to_string()),
            hour: None,
            bindings: BTreeMap::new(),
            unit_scope: BTreeMap::new(),
            inputs: vec![],
            outputs: vec![
                "historian_talk_entries".to_string(),
                "hourly_answer_questions".to_string(),
                "concept_proposals".to_string(),
                "ycx_proposals".to_string(),
            ],
            required_artifacts: vec!["scribe_artifacts".to_string()],
            required_mail: vec![],
            route: crate::runtime_executor::RuntimeTurnRoute {
                thread_id: "mail://wiki-company".to_string(),
                from_role: "memory.wiki.historian".to_string(),
                from_mailbox: "agent://context-engine/historian".to_string(),
                to: vec!["role://memory.hourly.answerer".to_string()],
                cc: vec!["list://wiki-company".to_string()],
            },
            receipt_expectations: crate::scheduler::ScheduledReceiptExpectations {
                durable_receipt: "context-engine/live/mail/threads/wiki-company.jsonl".to_string(),
                require_birth_certificate: true,
                require_turn_start: true,
                require_context_injection: true,
                require_adapter_events: true,
                require_final_message: true,
                require_talk_append: true,
                require_mail_delivery: true,
                require_turn_complete: true,
                final_message_path: "final-message.md".to_string(),
                final_message_fields: vec![],
                talk_delivery_mode: "mail".to_string(),
                require_non_empty_final_message: true,
                do_not_count_codex_exit_as_done: true,
            },
            final_message_path:
                "context-engine/live/runs/run-a/turns/historian/attempt-0001/final-message.md"
                    .to_string(),
            source_packet_path: None,
            source_packet_hash: None,
            condition: None,
        }
    }

    fn runtime_with_turns(
        runnable_turns: Vec<crate::runtime_executor::RuntimeTurnDescriptor>,
        conditional_turns: Vec<crate::runtime_executor::RuntimeTurnDescriptor>,
    ) -> crate::runtime_executor::RuntimeExecutionScaffold {
        crate::runtime_executor::RuntimeExecutionScaffold {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            kind: "onecontext.context_engine.runtime_execution_scaffold.v1".to_string(),
            run_id: "run-a".to_string(),
            ready_phase_ids: vec![],
            runnable_units: vec![],
            conditional_units: vec![],
            runnable_turns,
            conditional_turns,
            blocked_jobs: vec![],
            blocked_reasons: vec![],
        }
    }

    fn harness_request_for_mail_roles(
        job_id: &str,
        to: Vec<&str>,
        cc: Vec<&str>,
    ) -> HarnessTurnRequest {
        HarnessTurnRequest {
            schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
            kind: "onecontext.context_engine.harness_turn_request.v1".to_string(),
            run_id: "run-a".to_string(),
            operation_id: "op-a".to_string(),
            unit_id: "unit-a".to_string(),
            harness: crate::harness_executor::HarnessTurnHarness {
                id: "codex-app-server".to_string(),
                runner: "codex".to_string(),
                transport: "app-server".to_string(),
                command: "codex".to_string(),
                captures: vec![],
            },
            agent: crate::harness_executor::HarnessTurnAgent {
                id: "agent-a".to_string(),
                provider: "openai".to_string(),
                model: "gpt-5".to_string(),
                reasoning_effort: "medium".to_string(),
                memory_mode: "none".to_string(),
                memory_attach: "none".to_string(),
                persistent_session: false,
            },
            job: crate::harness_executor::HarnessTurnJob {
                id: job_id.to_string(),
                label: job_id.to_string(),
                inputs: vec![],
                outputs: vec![],
                read_permissions: vec![],
                write_permissions: vec![],
                deny_permissions: vec![],
                completion_done: vec![],
                completion_skip: vec![],
                completion_failure: vec![],
            },
            prompt_bundle: vec![],
            prompt_text: String::new(),
            mail_context: crate::harness_executor::HarnessMailContext {
                thread_id: "mail://wiki-company".to_string(),
                mailbox: "role://memory.wiki.historian".to_string(),
                appendix_enabled: true,
                appendix_text: String::new(),
            },
            wiki_context: crate::harness_executor::HarnessWikiContext {
                user_wiki_root: "/tmp/user-wiki".to_string(),
                source_root: "/tmp/user-wiki/source".to_string(),
                talk_root: "/tmp/user-wiki/source/talk".to_string(),
            },
            source_packet: crate::harness_executor::HarnessSourcePacket {
                packet_id: "none".to_string(),
                source: "none".to_string(),
                bounded_by_job: true,
                path: None,
                content_sha256: None,
                bytes: None,
            },
            tool_policy: crate::harness_executor::HarnessToolPolicy {
                default_tools: vec![],
                agent_tools: vec![],
                job_tools: vec![],
            },
            required_receipts: crate::harness_executor::HarnessReceiptRequirements {
                final_message_path:
                    "context-engine/live/runs/run-a/turns/op-a/attempt-0001/final-message.md"
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
            talk_report: crate::harness_executor::HarnessTalkReport {
                delivery_mode: "mail".to_string(),
                thread_id: "mail://wiki-company".to_string(),
                from: format!("role://{job_id}"),
                to: to.into_iter().map(ToString::to_string).collect(),
                cc: cc.into_iter().map(ToString::to_string).collect(),
            },
        }
    }

    fn agent_turn_result(
        assistant_preview: &str,
        follow_up_mail_receipts: Vec<NextAgentRequestReceipt>,
    ) -> WikiCompanyAgentTurnResult {
        WikiCompanyAgentTurnResult {
            job_id: "memory.wiki.historian".to_string(),
            agent_id: "wiki-historian".to_string(),
            unit_id: "unit-a".to_string(),
            turn_id: "turn-a".to_string(),
            codex_thread_id: Some("thread-a".to_string()),
            codex_turn_id: Some("codex-turn-a".to_string()),
            codex_thread_resumed: false,
            mail_agent: None,
            worker_config: json!({}),
            final_message_path: "runtime/1Context/context-engine/live/runs/run-a/turns/historian/attempt-0001/final-message.md".to_string(),
            final_message_sha256: "sha256-a".to_string(),
            final_message_origin: "model_output".to_string(),
            talk_receipt_path: "user-wiki://page/for-you/talk/messages/talkmsg-a".to_string(),
            mail_receipt_path: "agent-mail://talkmsg-a/delivery-a".to_string(),
            follow_up_mail_receipts,
            audit_report_path: "runtime/1Context/context-engine/live/mail/threads/wiki-company.jsonl".to_string(),
            completion: HarnessTurnCompletion {
                status: "complete".to_string(),
                complete: true,
                issues: vec![],
                codex_exit_status: Some(0),
            },
            assistant_preview: assistant_preview.to_string(),
        }
    }

    fn next_agent_request_receipt(to: Vec<&str>) -> NextAgentRequestReceipt {
        NextAgentRequestReceipt {
            status: "sent".to_string(),
            request_text: "request".to_string(),
            to: to.into_iter().map(ToString::to_string).collect(),
            cc: vec!["list://wiki-company".to_string()],
            message_id: Some("mailmsg-a".to_string()),
            delivery_ids: vec!["delivery-a".to_string()],
            mail_receipt: Some("agent-mail://mailmsg-a/delivery-a".to_string()),
            error: None,
        }
    }
}
