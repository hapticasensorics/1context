//! Harness-owned wiki company agent execution.
//!
//! This module owns the first live bridge from Context Engine planning into
//! actual Codex app-server turns. The 1Context harness owns durable agent
//! identity, receipts, mail, and scheduling; each agent profile decides whether
//! its active Codex turn requests native multi-agent V2.

use crate::harness_executor::{
    build_harness_turn_request, evaluate_harness_turn_completion, short_harness_id,
    HarnessTurnCompletion, HarnessTurnReceipts, HarnessTurnRequest,
};
use crate::pack::WikiCompanyPackConfig;
use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use chrono::{SecondsFormat, Utc};
use onecontext_agent_harness_core::{
    AdapterCorrelation, AdapterEventKind, AdapterEventRequest, AdapterEventStatus, AdapterKind,
    AdapterRedaction, AgentCallRequest, AgentHarnessStore, AgentTurnCompleteRequest,
    AgentTurnCompletionState, AgentTurnId, AgentTurnStartRequest, AgentTurnUsage, AgentUnitId,
    AgentVisibility, CapabilityBinding, CapabilityTransport,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

const ORCHESTRATOR_UNIT_ID: &str = "wiki-company-orchestrator-v1";
const DEFAULT_LIVE_JOB_IDS: &[&str] = &["memory.hourly.scribe", "memory.wiki.for_you_editor"];
const WORKER_TIMEOUT_SECS: u64 = 180;

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
    pub worker_config: Value,
    pub final_message_path: String,
    pub talk_receipt_path: String,
    pub mail_receipt_path: String,
    pub completion: HarnessTurnCompletion,
    pub assistant_preview: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CodexWorkerTurnResult {
    pub status: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub resumed_thread: bool,
    pub assistant_text: String,
    pub error: Option<String>,
    pub duration_ms: u64,
}

pub fn execute_wiki_company_agents(
    paths: &ContextEnginePaths,
    pack: &WikiCompanyPackConfig,
    run_id: &str,
    max_agents: u32,
) -> Result<WikiCompanyAgentExecution, String> {
    let job_ids = live_job_ids(max_agents);
    let mut turns = Vec::new();
    let mut issues = Vec::new();

    ensure_orchestrator_unit(paths)?;
    for job_id in &job_ids {
        match execute_one_job(paths, pack, run_id, job_id) {
            Ok(result) => {
                if !result.completion.complete {
                    issues.extend(result.completion.issues.clone());
                }
                turns.push(result);
            }
            Err(error) => {
                issues.push(format!("{job_id}: {error}"));
            }
        }
    }

    let completed_job_count = turns.iter().filter(|turn| turn.completion.complete).count();
    let failed_job_count = job_ids.len().saturating_sub(completed_job_count);
    Ok(WikiCompanyAgentExecution {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.wiki_company_agent_execution".to_string(),
        status: if failed_job_count == 0 {
            "complete"
        } else {
            "partial"
        }
        .to_string(),
        mode: "tiny_live_harness_first".to_string(),
        worker_config: codex_worker_thread_config(false),
        requested_job_count: job_ids.len(),
        completed_job_count,
        failed_job_count,
        turns,
        issues,
    })
}

pub fn codex_worker_thread_config(native_multi_agent_v2: bool) -> Value {
    let mut config = json!({
        "features": {
            "multi_agent_v2": {
                "enabled": native_multi_agent_v2
            }
        },
        "onecontext_worker_turn": true
    });
    if native_multi_agent_v2 {
        config["features"]["multi_agent_v2"]["encrypted_messages"] = Value::Bool(false);
    }
    config
}

fn live_job_ids(max_agents: u32) -> Vec<String> {
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
    let mut jobs = from_env.unwrap_or_else(|| {
        DEFAULT_LIVE_JOB_IDS
            .iter()
            .map(|job| (*job).to_string())
            .collect()
    });
    let limit = usize::try_from(max_agents.max(1)).unwrap_or(1);
    jobs.truncate(limit.max(1));
    jobs
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

fn execute_one_job(
    paths: &ContextEnginePaths,
    pack: &WikiCompanyPackConfig,
    run_id: &str,
    job_id: &str,
) -> Result<WikiCompanyAgentTurnResult, String> {
    let request = build_harness_turn_request(paths, pack, run_id, job_id)?;
    let worker_config = codex_worker_thread_config(request.agent.native_multi_agent_v2);
    let store = AgentHarnessStore::new(&paths.root);
    let child = store
        .call(child_call_request(&request))
        .map_err(|error| format!("failed to birth child unit {}: {error}", request.unit_id))?;
    let turn_id = AgentTurnId(short_harness_id(&format!("turn-{}", request.operation_id)));
    store
        .start_turn(AgentTurnStartRequest {
            unit_id: child.unit_id.clone(),
            turn_id: Some(turn_id.clone()),
            metadata: json!({
                "run_id": request.run_id,
                "job_id": request.job.id,
                "agent_id": request.agent.id,
                "persistent_session": request.agent.persistent_session,
                "native_multi_agent_v2": request.agent.native_multi_agent_v2,
                "context_injection": {
                    "mail_context": request.mail_context,
                    "wiki_context": request.wiki_context,
                    "source_packet": request.source_packet,
                }
            }),
        })
        .map_err(|error| format!("failed to start harness turn {}: {error}", turn_id.0))?;

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
            "native_multi_agent_v2": request.agent.native_multi_agent_v2,
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

    let final_message_path = write_final_message(paths, &request, &worker_result)?;
    let report_path = append_agent_report(paths, &request, &worker_result, &final_message_path)?;

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
                "final_message_path": final_message_path,
                "talk_receipt_path": report_path,
                "mail_receipt_path": report_path,
                "codex_thread_id": worker_result.thread_id,
                "codex_turn_id": worker_result.turn_id,
                "codex_thread_resumed": worker_result.resumed_thread,
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
        final_message: Some(final_message_path.display().to_string()),
        talk_receipt: Some(report_path.display().to_string()),
        mail_receipt: Some(report_path.display().to_string()),
        harness_turn_complete: true,
    };
    let completion = evaluate_harness_turn_completion(&request, &receipts);

    Ok(WikiCompanyAgentTurnResult {
        job_id: request.job.id,
        agent_id: request.agent.id,
        unit_id: request.unit_id,
        turn_id: turn_id.0,
        codex_thread_id: worker_result.thread_id,
        codex_turn_id: worker_result.turn_id,
        codex_thread_resumed: worker_result.resumed_thread,
        worker_config,
        final_message_path: final_message_path.display().to_string(),
        talk_receipt_path: report_path.display().to_string(),
        mail_receipt_path: report_path.display().to_string(),
        completion,
        assistant_preview: preview(&worker_result.assistant_text, 500),
    })
}

fn child_call_request(request: &HarnessTurnRequest) -> AgentCallRequest {
    let worker_config = codex_worker_thread_config(request.agent.native_multi_agent_v2);
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
            "native_multi_agent_v2": request.agent.native_multi_agent_v2,
            "persistent_session": true,
        })
    } else {
        json!({
            "agent_id": request.agent.id,
            "job_id": request.job.id,
            "reasoning_effort": request.agent.reasoning_effort,
            "memory_mode": request.agent.memory_mode,
            "memory_attach": request.agent.memory_attach,
            "native_multi_agent_v2": request.agent.native_multi_agent_v2,
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
    let mut app = CodexAppServerProcess::spawn(request.agent.native_multi_agent_v2)?;
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
    let base_instructions = if request.agent.native_multi_agent_v2 {
        "You are a harness-born 1Context wiki agent. The 1Context harness owns durable identity, receipts, and mail. You may use Codex native multi-agent V2 when it helps this bounded turn."
    } else {
        "You are a harness-born 1Context wiki agent. Do not spawn subagents; the 1Context harness owns orchestration."
    };
    let thread_open = open_codex_thread(
        paths,
        request,
        &mut app,
        &cwd,
        base_instructions,
        worker_config,
    )?;
    let thread_id = thread_open.thread_id;

    app.send(
        "turn",
        "turn/start",
        json!({
            "threadId": thread_id,
            "input": [{
                "type": "text",
                "text": request.prompt_text,
            }],
            "clientUserMessageId": format!("onecontext-{}", request.operation_id),
            "model": request.agent.model,
            "effort": request.agent.reasoning_effort,
            "additionalContext": {
                "onecontext.mail_context": {
                    "kind": "application",
                    "value": request.mail_context.appendix_text,
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
    let deadline = Duration::from_secs(WORKER_TIMEOUT_SECS);
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
        assistant_text,
        error,
        duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
    })
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
                    "developerInstructions": "Complete this single bounded wiki-company turn. Keep the final answer shaped as final-message.md content.",
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
            "developerInstructions": "Complete this single bounded wiki-company turn. Keep the final answer shaped as final-message.md content.",
            "experimentalRawEvents": true,
            "threadSource": "subagent",
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
        .root
        .join("context-engine/agents/harness/codex-app-server/threads")
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
    fn spawn(native_multi_agent_v2: bool) -> Result<Self, String> {
        let codex_bin =
            std::env::var("ONECONTEXT_CODEX_BIN").unwrap_or_else(|_| "codex".to_string());
        let mut command = Command::new(codex_bin);
        command.arg("app-server");
        if !native_multi_agent_v2 {
            command.args(["-c", "features.multi_agent_v2.enabled=false"]);
        }
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
) -> Result<PathBuf, String> {
    let path = paths
        .root
        .join(&request.required_receipts.final_message_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create final-message dir: {error}"))?;
    }
    let body = if worker_result.assistant_text.trim().is_empty() {
        format!(
            "status: {}\n\nevidence:\n- No assistant text captured.\n\nproposed_wiki_talk:\n- No proposed talk text captured.\n\nnext_agent_requests:\n- none\n\nnext_state_machine_event: worker_turn_{}\n",
            worker_result.status, worker_result.status
        )
    } else {
        worker_result.assistant_text.clone()
    };
    fs::write(&path, body)
        .map_err(|error| format!("failed to write final message {}: {error}", path.display()))?;
    Ok(path)
}

fn append_agent_report(
    paths: &ContextEnginePaths,
    request: &HarnessTurnRequest,
    worker_result: &CodexWorkerTurnResult,
    final_message_path: &Path,
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
        "from": request.talk_report.from,
        "to": request.talk_report.to,
        "cc": request.talk_report.cc,
        "job_id": request.job.id,
        "agent_id": request.agent.id,
        "unit_id": request.unit_id,
        "codex_thread_id": worker_result.thread_id,
        "codex_turn_id": worker_result.turn_id,
        "status": worker_result.status,
        "final_message_path": final_message_path,
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
    fn worker_thread_config_defaults_native_multi_agent_off() {
        let config = codex_worker_thread_config(false);
        assert_eq!(
            config["features"]["multi_agent_v2"]["enabled"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(
            config["features"]["multi_agent_v2"].get("encrypted_messages"),
            None
        );
        assert_eq!(
            config["onecontext_worker_turn"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn worker_thread_config_enables_native_multi_agent_when_agent_flag_is_set() {
        let config = codex_worker_thread_config(true);
        assert_eq!(
            config["features"]["multi_agent_v2"]["enabled"],
            serde_json::Value::Bool(true)
        );
        assert_eq!(
            config["features"]["multi_agent_v2"]["encrypted_messages"],
            serde_json::Value::Bool(false)
        );
        assert_eq!(
            config["onecontext_worker_turn"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn live_job_ids_default_to_tiny_harness_first_scope() {
        std::env::remove_var("ONECONTEXT_CONTEXT_ENGINE_LIVE_JOBS");
        assert_eq!(
            live_job_ids(5),
            vec!["memory.hourly.scribe", "memory.wiki.for_you_editor"]
        );
        assert_eq!(live_job_ids(1), vec!["memory.hourly.scribe"]);
    }
}
