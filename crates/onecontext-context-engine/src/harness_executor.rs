//! Agent harness request construction.
//!
//! This is the first release-side bridge from the wiki-company pack into the
//! native `onecontext-agent-harness` / Codex app-server executor. It does not
//! start a model yet; it builds the durable turn request the executor must own.

use crate::pack::{AgentConfig, HarnessConfig, JobConfig, WikiCompanyPackConfig};
use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

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
    pub native_multi_agent_v2: bool,
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

pub fn build_harness_turn_request(
    paths: &ContextEnginePaths,
    pack: &WikiCompanyPackConfig,
    run_id: &str,
    job_id: &str,
) -> Result<HarnessTurnRequest, String> {
    let job = find_job(pack, job_id)?;
    let agent = find_agent(pack, &job.agent)?;
    let harness = find_harness(pack, &agent.harness)?;
    let run_id = safe_run_id(run_id);
    let memory_mode = agent
        .memory
        .mode
        .clone()
        .unwrap_or_else(|| "episodic".to_string());
    let memory_attach = agent
        .memory
        .attach
        .clone()
        .unwrap_or_else(|| "none".to_string());
    let persistent_session = memory_mode == "persistent";
    let unit_id = if persistent_session {
        short_harness_id(&format!("wiki-{}", agent.id))
    } else {
        short_harness_id(&format!("{}-tmp-{}", safe_run_id(&job.id), run_id))
    };
    let prompt_bundle = load_prompt_bundle(paths, harness, agent, job)?;
    let mail_context = mail_context(&unit_id, harness);
    let prompt_text = render_prompt_text(&prompt_bundle, &mail_context);

    Ok(HarnessTurnRequest {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.agent_harness.turn_request.v1".to_string(),
        operation_id: format!("{}:{}", run_id, job.id),
        run_id,
        unit_id: unit_id.clone(),
        harness: HarnessTurnHarness {
            id: harness.id.clone(),
            runner: harness.runner.clone(),
            transport: harness
                .transport
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            command: harness
                .command
                .clone()
                .unwrap_or_else(|| "onecontext-agent-harness call".to_string()),
            captures: harness.captures.clone(),
        },
        agent: HarnessTurnAgent {
            id: agent.id.clone(),
            provider: agent.provider.clone(),
            model: agent.model.clone(),
            reasoning_effort: agent
                .model_policy
                .reasoning_effort
                .clone()
                .unwrap_or_else(|| "high".to_string()),
            memory_mode,
            memory_attach,
            native_multi_agent_v2: agent
                .runtime
                .native_multi_agent_v2
                .unwrap_or(harness.runtime_policy.default_native_multi_agent_v2),
            persistent_session,
        },
        job: HarnessTurnJob {
            id: job.id.clone(),
            label: job.label.clone().unwrap_or_else(|| job.id.clone()),
            inputs: job.inputs.clone(),
            outputs: job.outputs.clone(),
            read_permissions: job.permissions.read.clone(),
            write_permissions: job.permissions.write.clone(),
            deny_permissions: job.permissions.deny.clone(),
            completion_done: job.completion.done.clone(),
            completion_skip: job.completion.skip.clone(),
            completion_failure: job.completion.failure.clone(),
        },
        prompt_bundle,
        prompt_text,
        mail_context,
        wiki_context: HarnessWikiContext {
            user_wiki_root: paths.user_wiki.display().to_string(),
            source_root: paths.user_wiki.join("source").display().to_string(),
            talk_root: paths.user_wiki.join("source/talk").display().to_string(),
        },
        source_packet: HarnessSourcePacket {
            packet_id: "source-packet.pending".to_string(),
            source: "perception_db".to_string(),
            bounded_by_job: true,
        },
        tool_policy: HarnessToolPolicy {
            default_tools: harness.default_tools.clone(),
            agent_tools: agent.tools.clone(),
            job_tools: job.tools.clone(),
        },
        required_receipts: HarnessReceiptRequirements {
            final_message_path: format!("context-engine/agents/harness/{unit_id}/final-message.md"),
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
            thread_id: "job.run_id".to_string(),
            from: format!("agent://{unit_id}"),
            to: route_to(&job.id),
            cc: route_cc(&job.id),
        },
    })
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
            .unwrap_or_default()
            .is_empty()
    {
        issues.push("missing wiki.talk.append receipt".to_string());
    }
    if request.required_receipts.require_mail_delivery
        && receipts
            .mail_receipt
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
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

fn find_job<'a>(pack: &'a WikiCompanyPackConfig, job_id: &str) -> Result<&'a JobConfig, String> {
    pack.jobs
        .iter()
        .find(|job| job.id == job_id)
        .ok_or_else(|| format!("unknown job {job_id:?}"))
}

fn find_agent<'a>(
    pack: &'a WikiCompanyPackConfig,
    agent_id: &str,
) -> Result<&'a AgentConfig, String> {
    pack.agents
        .iter()
        .find(|agent| agent.id == agent_id)
        .ok_or_else(|| format!("job references unknown agent {agent_id:?}"))
}

fn find_harness<'a>(
    pack: &'a WikiCompanyPackConfig,
    harness_id: &str,
) -> Result<&'a HarnessConfig, String> {
    pack.harnesses
        .iter()
        .find(|harness| harness.id == harness_id)
        .ok_or_else(|| format!("agent references unknown harness {harness_id:?}"))
}

fn load_prompt_bundle(
    paths: &ContextEnginePaths,
    harness: &HarnessConfig,
    agent: &AgentConfig,
    job: &JobConfig,
) -> Result<Vec<HarnessPromptPart>, String> {
    let mut refs = Vec::<(String, String)>::new();
    if let Some(prompt) = &harness.prompt_control.model_instructions_file {
        refs.push((prompt.clone(), format!("harness:{}", harness.id)));
    }
    refs.extend(
        agent
            .prompt_paths
            .iter()
            .map(|path| (path.clone(), format!("agent:{}", agent.id))),
    );
    refs.extend(
        agent
            .reference_paths
            .iter()
            .map(|path| (path.clone(), format!("agent-reference:{}", agent.id))),
    );
    refs.extend(
        job.prompt_paths
            .iter()
            .map(|path| (path.clone(), format!("job:{}", job.id))),
    );

    let mut seen = BTreeSet::<String>::new();
    let mut bundle = Vec::new();
    for (path, source) in refs {
        if !seen.insert(path.clone()) {
            continue;
        }
        let text = read_pack_prompt(paths, &path)?;
        bundle.push(HarnessPromptPart {
            bytes: text.len(),
            path,
            source,
            text,
        });
    }
    Ok(bundle)
}

fn read_pack_prompt(paths: &ContextEnginePaths, prompt_path: &str) -> Result<String, String> {
    let relative = prompt_path
        .strip_prefix("prompts/")
        .ok_or_else(|| format!("prompt path {prompt_path:?} must be under prompts/"))?;
    let resolved = paths.pack_prompts.join(relative);
    read_file_to_string(&resolved)
}

fn read_file_to_string(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("failed to read {}: {error}", path.display()))
}

fn mail_context(unit_id: &str, harness: &HarnessConfig) -> HarnessMailContext {
    let appendix_enabled = harness.prompt_control.mail_context_appendix;
    let appendix_text = format!(
        "## Agent Mail Context\n\nThread: mail://wiki-company\nMailbox: agent://{unit_id}\n\nRead your inbox summary before acting. Post your final report through wiki.talk.append with delivery_mode=mail."
    );
    HarnessMailContext {
        thread_id: "mail://wiki-company".to_string(),
        mailbox: format!("agent://{unit_id}"),
        appendix_enabled,
        appendix_text,
    }
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
    out.push_str("\n\n## Required Final Message Receipt\n\nWrite a non-empty final-message.md with status, evidence, proposed_wiki_talk, next_agent_requests, and next_state_machine_event.");
    out
}

fn route_to(job_id: &str) -> Vec<String> {
    match job_id {
        "memory.hourly.scribe"
        | "memory.hourly.block_scribe"
        | "memory.hourly.aggregate_scribe" => vec!["role://memory.wiki.for_you_editor".to_string()],
        "memory.wiki.for_you_editor" => vec!["role://memory.wiki.for_you_curator".to_string()],
        "memory.wiki.biographer" => vec!["role://memory.wiki.context_curator".to_string()],
        "memory.wiki.librarian" | "memory.wiki.librarian_sweep" => vec![
            "role://memory.wiki.for_you_curator".to_string(),
            "role://memory.wiki.context_curator".to_string(),
        ],
        "memory.wiki.for_you_curator" => vec!["mailbox://page/for-you".to_string()],
        "memory.wiki.context_curator" => vec!["mailbox://page/your-context".to_string()],
        _ => vec!["mailbox://wiki-company".to_string()],
    }
}

fn route_cc(job_id: &str) -> Vec<String> {
    match job_id {
        "memory.wiki.for_you_editor" => vec![
            "role://memory.wiki.librarian".to_string(),
            "mailbox://page/for-you".to_string(),
        ],
        "memory.wiki.biographer" => vec![
            "role://memory.wiki.librarian".to_string(),
            "mailbox://wiki-company".to_string(),
        ],
        "memory.wiki.for_you_curator" | "memory.wiki.context_curator" => {
            vec!["mailbox://wiki-company".to_string()]
        }
        _ => vec!["mailbox://wiki-company".to_string()],
    }
}
