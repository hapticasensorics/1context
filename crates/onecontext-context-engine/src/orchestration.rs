//! Config-driven orchestration primitives.
//!
//! This module is deliberately small: it turns the pack and orchestrator TOMLs
//! into an inspectable execution contract before any model or wiki mutation runs.

use crate::orchestrator::{
    route_for_role, PhaseJobConfig, ResolvedRoute, WikiCompanyOrchestratorConfig,
};
use crate::pack::WikiCompanyPackConfig;
use crate::packet_planner::PacketPlannerPolicy;
use crate::CONTEXT_ENGINE_SCHEMA_VERSION;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutableOrchestrationPlan {
    pub schema_version: u32,
    pub kind: String,
    pub phase_count: usize,
    pub job_count: usize,
    pub phases: Vec<ExecutablePhase>,
    pub receipt_policy: ExecutableReceiptPolicy,
    pub packet_policy: ExecutablePacketPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutablePhase {
    pub id: String,
    pub label: String,
    pub owner: String,
    pub depends_on: Vec<String>,
    pub strategy: String,
    pub completion: String,
    pub reads_raw_history: bool,
    pub durable_receipt: String,
    pub jobs: Vec<ExecutablePhaseJob>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutablePhaseJob {
    pub phase_id: String,
    pub job_id: String,
    pub agent_id: String,
    pub fanout: String,
    pub when: Option<String>,
    pub max_concurrent: Option<u32>,
    pub required_artifacts: Vec<String>,
    pub required_mail: Vec<String>,
    pub bindings: BTreeMap<String, String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub route: ResolvedRoute,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableReceiptPolicy {
    pub require_birth_certificate: bool,
    pub require_turn_start: bool,
    pub require_context_injection: bool,
    pub require_adapter_events: bool,
    pub require_final_message: bool,
    pub require_talk_append: bool,
    pub require_mail_delivery: bool,
    pub require_turn_complete: bool,
    pub final_message_path: String,
    pub final_message_fields: Vec<String>,
    pub talk_delivery_mode: String,
    pub require_non_empty_final_message: bool,
    pub do_not_count_codex_exit_as_done: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutablePacketPolicy {
    pub recent_first_days: u32,
    pub backfill_days: u32,
    pub backfill_order: String,
    pub incremental_unit: Option<String>,
    pub model_context_tokens: u32,
    pub source_context_fraction: f64,
    pub target_source_tokens: u32,
    pub split_when_estimated_tokens_exceed: u32,
    pub raw_history_roles: Vec<String>,
    pub downstream_roles_read_scribe_artifacts: bool,
    pub cache_enabled: bool,
    pub cache_key: Option<String>,
    pub skip_unchanged_scribe_packets: bool,
}

pub fn build_executable_orchestration_plan(
    pack: &WikiCompanyPackConfig,
    config: &WikiCompanyOrchestratorConfig,
) -> Result<ExecutableOrchestrationPlan, String> {
    let jobs_by_id = pack
        .jobs
        .iter()
        .map(|job| (job.id.as_str(), job))
        .collect::<BTreeMap<_, _>>();
    let phase_ids = config
        .phases
        .phases
        .iter()
        .map(|phase| phase.id.as_str())
        .collect::<BTreeSet<_>>();

    let mut phases = Vec::new();
    for phase in &config.phases.phases {
        for depends_on in &phase.depends_on {
            if !phase_ids.contains(depends_on.as_str()) {
                return Err(format!(
                    "phase {} depends on unknown phase {}",
                    phase.id, depends_on
                ));
            }
        }
        let mut jobs = Vec::new();
        for phase_job in config
            .phases
            .phase_jobs
            .iter()
            .filter(|job| job.phase_id == phase.id)
        {
            jobs.push(executable_phase_job(phase_job, &jobs_by_id, config)?);
        }
        phases.push(ExecutablePhase {
            id: phase.id.clone(),
            label: phase.label.clone().unwrap_or_else(|| phase.id.clone()),
            owner: phase.owner.clone(),
            depends_on: phase.depends_on.clone(),
            strategy: phase
                .strategy
                .clone()
                .unwrap_or_else(|| "manual".to_string()),
            completion: phase
                .completion
                .clone()
                .unwrap_or_else(|| "phase_receipt_recorded".to_string()),
            reads_raw_history: phase.reads_raw_history,
            durable_receipt: phase.durable_receipt.clone(),
            jobs,
        });
    }

    let job_count = phases.iter().map(|phase| phase.jobs.len()).sum();
    Ok(ExecutableOrchestrationPlan {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.executable_orchestration_plan.v1".to_string(),
        phase_count: phases.len(),
        job_count,
        phases,
        receipt_policy: receipt_policy(config),
        packet_policy: packet_policy(config),
    })
}

pub fn planner_policy_from_config(
    config: &WikiCompanyOrchestratorConfig,
    request_window_days: u32,
) -> PacketPlannerPolicy {
    let budget = &config.packet_policy.context_budget;
    PacketPlannerPolicy {
        window_days: request_window_days,
        usable_context_tokens: budget.model_context_tokens,
        context_fraction: budget.source_context_fraction,
        target_packet_tokens_override: Some(budget.target_source_tokens),
        ..PacketPlannerPolicy::default()
    }
}

fn executable_phase_job(
    phase_job: &PhaseJobConfig,
    jobs_by_id: &BTreeMap<&str, &crate::pack::JobConfig>,
    config: &WikiCompanyOrchestratorConfig,
) -> Result<ExecutablePhaseJob, String> {
    let job = jobs_by_id
        .get(phase_job.job_id.as_str())
        .ok_or_else(|| format!("phase job references unknown job {}", phase_job.job_id))?;
    Ok(ExecutablePhaseJob {
        phase_id: phase_job.phase_id.clone(),
        job_id: phase_job.job_id.clone(),
        agent_id: job.agent.clone(),
        fanout: phase_job
            .fanout
            .clone()
            .unwrap_or_else(|| "once".to_string()),
        when: phase_job.when.clone(),
        max_concurrent: phase_job.max_concurrent,
        required_artifacts: phase_job.required_artifacts.clone(),
        required_mail: phase_job.required_mail.clone(),
        bindings: phase_job.bindings.clone(),
        inputs: job.inputs.clone(),
        outputs: job.outputs.clone(),
        route: route_for_role(&config.routing, &job.id),
    })
}

fn receipt_policy(config: &WikiCompanyOrchestratorConfig) -> ExecutableReceiptPolicy {
    let required = &config.receipts.required;
    ExecutableReceiptPolicy {
        require_birth_certificate: required.harness_birth_certificate,
        require_turn_start: required.harness_turn_start,
        require_context_injection: required.context_injection_receipt,
        require_adapter_events: required.adapter_events,
        require_final_message: required.final_message,
        require_talk_append: required.talk_append,
        require_mail_delivery: required.mail_delivery,
        require_turn_complete: required.harness_turn_complete,
        final_message_path: config.receipts.final_message.path.clone(),
        final_message_fields: config.receipts.final_message.fields.clone(),
        talk_delivery_mode: config.receipts.talk_append.delivery_mode.clone(),
        require_non_empty_final_message: config.receipts.final_message.non_empty,
        do_not_count_codex_exit_as_done: config.receipts.completion.do_not_count_codex_exit_as_done,
    }
}

fn packet_policy(config: &WikiCompanyOrchestratorConfig) -> ExecutablePacketPolicy {
    let policy = &config.packet_policy;
    ExecutablePacketPolicy {
        recent_first_days: policy.windows.recent_first_days,
        backfill_days: policy.windows.backfill_days,
        backfill_order: policy.windows.backfill_order.clone(),
        incremental_unit: policy.windows.incremental_unit.clone(),
        model_context_tokens: policy.context_budget.model_context_tokens,
        source_context_fraction: policy.context_budget.source_context_fraction,
        target_source_tokens: policy.context_budget.target_source_tokens,
        split_when_estimated_tokens_exceed: policy
            .context_budget
            .split_when_estimated_tokens_exceed,
        raw_history_roles: policy.packet_shape.raw_history_roles.clone(),
        downstream_roles_read_scribe_artifacts: policy
            .packet_shape
            .downstream_roles_read_scribe_artifacts,
        cache_enabled: policy.cache.enabled,
        cache_key: policy.cache.key.clone(),
        skip_unchanged_scribe_packets: policy.cache.skip_unchanged_scribe_packets,
    }
}
