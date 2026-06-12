//! Scheduler-driven runtime execution scaffold.
//!
//! This module coordinates the primitives that sit below the TOML company spec.
//! It expands currently-ready scheduled jobs into concrete units and records the
//! remaining gates. It does not start Codex turns yet.

use crate::conditions::{
    evaluate_condition, ConditionContext, ConditionEvaluation, ConditionStatus,
};
use crate::fanout::{expand_job_fanout, ConcreteJobUnit, FanoutContext};
use crate::harness_executor::{agent_mail_address_for_unit, short_harness_id};
use crate::scheduler::{ScheduledJob, ScheduledReceiptExpectations, SchedulerRunPlan};
use crate::{safe_run_id, CONTEXT_ENGINE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RuntimeExecutionScaffoldRequest {
    pub run_id: String,
    pub schedule: SchedulerRunPlan,
    pub fanout_context: FanoutContext,
    pub condition_context: ConditionContext,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeExecutionScaffold {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub ready_phase_ids: Vec<String>,
    pub runnable_units: Vec<ConcreteJobUnit>,
    pub conditional_units: Vec<ConcreteJobUnit>,
    pub runnable_turns: Vec<RuntimeTurnDescriptor>,
    pub conditional_turns: Vec<RuntimeTurnDescriptor>,
    pub blocked_jobs: Vec<RuntimeBlockedJob>,
    pub blocked_reasons: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTurnDescriptor {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub status: String,
    pub operation_id: String,
    pub harness_unit_id: String,
    pub harness_turn_id: String,
    pub phase_id: String,
    pub job_id: String,
    pub agent_id: String,
    pub unit_id: String,
    pub fanout: String,
    pub max_concurrent: Option<u32>,
    pub packet_id: Option<String>,
    pub date: Option<String>,
    pub hour: Option<String>,
    pub bindings: BTreeMap<String, String>,
    pub unit_scope: BTreeMap<String, String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub required_artifacts: Vec<String>,
    pub required_mail: Vec<String>,
    pub route: RuntimeTurnRoute,
    pub receipt_expectations: ScheduledReceiptExpectations,
    pub final_message_path: String,
    pub source_packet_path: Option<String>,
    pub source_packet_hash: Option<String>,
    pub condition: Option<ConditionEvaluation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTurnRoute {
    pub thread_id: String,
    pub from_role: String,
    pub from_mailbox: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeBlockedJob {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub phase_id: String,
    pub job_id: String,
    pub agent_id: String,
    pub scheduler_status: String,
    pub reason: String,
    pub missing_required_artifacts: Vec<String>,
    pub missing_required_mail: Vec<String>,
    pub route_thread_id: String,
    pub condition: Option<ConditionEvaluation>,
}

pub fn build_runtime_execution_scaffold(
    request: RuntimeExecutionScaffoldRequest,
) -> RuntimeExecutionScaffold {
    let mut runnable_units = Vec::new();
    let mut conditional_units = Vec::new();
    let mut runnable_turns = Vec::new();
    let mut conditional_turns = Vec::new();
    let mut blocked_jobs = Vec::new();
    let mut blocked_reasons = request
        .schedule
        .issues
        .iter()
        .map(|issue| issue.message.clone())
        .collect::<Vec<_>>();
    for job in ready_phase_jobs(&request.schedule) {
        if !job.missing_required_artifacts.is_empty() && !job.missing_required_mail.is_empty() {
            blocked_jobs.push(blocked_job(
                &request.run_id,
                job,
                format!(
                    "waiting for inputs: artifacts [{}]; mail [{}]",
                    job.missing_required_artifacts.join(", "),
                    job.missing_required_mail.join(", ")
                ),
                None,
            ));
            continue;
        }
        if !job.missing_required_artifacts.is_empty() {
            blocked_jobs.push(blocked_job(
                &request.run_id,
                job,
                format!(
                    "waiting for artifacts: {}",
                    job.missing_required_artifacts.join(", ")
                ),
                None,
            ));
            continue;
        }
        if !job.missing_required_mail.is_empty() {
            blocked_jobs.push(blocked_job(
                &request.run_id,
                job,
                format!("waiting for mail: {}", job.missing_required_mail.join(", ")),
                None,
            ));
            continue;
        }

        let condition = evaluate_condition(job.when.as_deref(), &request.condition_context);
        match condition.status {
            ConditionStatus::True => {
                let expansion = expand_job_fanout(job, &request.fanout_context);
                for issue in expansion.issues {
                    blocked_jobs.push(blocked_job(
                        &request.run_id,
                        job,
                        issue,
                        Some(condition.clone()),
                    ));
                }
                for unit in expansion.units {
                    runnable_turns.push(turn_descriptor(
                        &request.run_id,
                        job,
                        &unit,
                        "runnable",
                        condition_for_descriptor(&condition),
                    ));
                    runnable_units.push(unit);
                }
            }
            ConditionStatus::False => {
                blocked_jobs.push(blocked_job(
                    &request.run_id,
                    job,
                    format!("condition false: {}", condition.expression),
                    Some(condition),
                ));
            }
            ConditionStatus::Unknown => {
                let expansion = expand_job_fanout(job, &request.fanout_context);
                for issue in expansion.issues {
                    blocked_jobs.push(blocked_job(
                        &request.run_id,
                        job,
                        issue,
                        Some(condition.clone()),
                    ));
                }
                for unit in expansion.units {
                    conditional_turns.push(turn_descriptor(
                        &request.run_id,
                        job,
                        &unit,
                        "conditional",
                        Some(condition.clone()),
                    ));
                    conditional_units.push(unit);
                }
                blocked_jobs.push(blocked_job(
                    &request.run_id,
                    job,
                    format!(
                        "condition unknown: {} ({})",
                        condition.expression, condition.reason
                    ),
                    Some(condition),
                ));
            }
        }
    }
    blocked_reasons.extend(
        blocked_jobs
            .iter()
            .map(|job| format!("{} {}", job.job_id, job.reason)),
    );
    RuntimeExecutionScaffold {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.runtime_execution_scaffold.v1".to_string(),
        run_id: safe_run_id(&request.run_id),
        ready_phase_ids: request.schedule.ready_phase_ids,
        runnable_units,
        conditional_units,
        runnable_turns,
        conditional_turns,
        blocked_jobs,
        blocked_reasons,
    }
}

fn ready_phase_jobs(plan: &SchedulerRunPlan) -> Vec<&ScheduledJob> {
    plan.phases
        .iter()
        .filter(|phase| phase.status == "ready")
        .flat_map(|phase| phase.jobs.iter())
        .collect()
}

fn turn_descriptor(
    run_id: &str,
    job: &ScheduledJob,
    unit: &ConcreteJobUnit,
    status: &str,
    condition: Option<ConditionEvaluation>,
) -> RuntimeTurnDescriptor {
    let run_id = safe_run_id(run_id);
    let operation_id = short_harness_id(&format!("{}:{}", run_id, unit.unit_id));
    let harness_unit_id = short_harness_id(&format!("{}:{}", run_id, unit.unit_id));
    RuntimeTurnDescriptor {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.runtime_turn_descriptor.v1".to_string(),
        run_id: run_id.clone(),
        status: status.to_string(),
        operation_id: operation_id.clone(),
        harness_turn_id: short_harness_id(&format!("turn-{operation_id}")),
        harness_unit_id: harness_unit_id.clone(),
        phase_id: unit.phase_id.clone(),
        job_id: unit.job_id.clone(),
        agent_id: unit.agent_id.clone(),
        unit_id: unit.unit_id.clone(),
        fanout: unit.fanout.clone(),
        max_concurrent: job.max_concurrent,
        packet_id: unit.packet_id.clone(),
        date: unit.date.clone(),
        hour: unit.hour.clone(),
        bindings: unit.bindings.clone(),
        unit_scope: unit.unit_scope.clone(),
        inputs: job.inputs.clone(),
        outputs: job.outputs.clone(),
        required_artifacts: unit.required_artifacts.clone(),
        required_mail: job.required_mail.clone(),
        route: RuntimeTurnRoute {
            thread_id: job.route_thread_id.clone(),
            from_role: job.route.from_role.clone(),
            from_mailbox: agent_mail_address_for_unit(&harness_unit_id),
            to: job.route_to.clone(),
            cc: job.route_cc.clone(),
        },
        receipt_expectations: job.receipt_expectations.clone(),
        final_message_path: final_message_path(&run_id, &operation_id, &job.receipt_expectations),
        source_packet_path: unit.bindings.get("source_packet_path").cloned(),
        source_packet_hash: unit.bindings.get("source_packet_hash").cloned(),
        condition,
    }
}

fn blocked_job(
    run_id: &str,
    job: &ScheduledJob,
    reason: String,
    condition: Option<ConditionEvaluation>,
) -> RuntimeBlockedJob {
    RuntimeBlockedJob {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.runtime_blocked_job.v1".to_string(),
        run_id: safe_run_id(run_id),
        phase_id: job.phase_id.clone(),
        job_id: job.job_id.clone(),
        agent_id: job.agent_id.clone(),
        scheduler_status: job.status.clone(),
        reason,
        missing_required_artifacts: job.missing_required_artifacts.clone(),
        missing_required_mail: job.missing_required_mail.clone(),
        route_thread_id: job.route_thread_id.clone(),
        condition,
    }
}

fn condition_for_descriptor(condition: &ConditionEvaluation) -> Option<ConditionEvaluation> {
    if condition.expression.is_empty() {
        None
    } else {
        Some(condition.clone())
    }
}

fn final_message_path(
    run_id: &str,
    operation_id: &str,
    receipts: &ScheduledReceiptExpectations,
) -> String {
    let filename = std::path::Path::new(&receipts.final_message_path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("final-message.md");
    format!(
        "context-engine/live/runs/{}/turns/{}/attempt-0001/{}",
        safe_run_id(run_id),
        safe_run_id(operation_id),
        filename
    )
}
