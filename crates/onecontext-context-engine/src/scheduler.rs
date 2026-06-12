//! Deterministic scheduler scaffold for executable orchestration plans.
//!
//! The scheduler builds an inspectable run plan and readiness view. It does not
//! start models, mutate the wiki, or create filesystem run history.

use crate::orchestration::{
    ExecutableOrchestrationPlan, ExecutablePacketPolicy, ExecutablePhaseJob,
    ExecutableReceiptPolicy,
};
use crate::orchestrator::ResolvedRoute;
use crate::scheduler_bindings::{parse_binding_expression, BindingReference};
use crate::{safe_run_id, CONTEXT_ENGINE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SchedulerRunRequest {
    pub run_id: String,
    pub max_concurrent_agents: u32,
    pub completed_phase_ids: Vec<String>,
    pub available_artifacts: Vec<String>,
    pub available_mail: Vec<String>,
}

impl SchedulerRunRequest {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            max_concurrent_agents: 5,
            completed_phase_ids: Vec::new(),
            available_artifacts: Vec::new(),
            available_mail: Vec::new(),
        }
    }

    pub fn with_completed_phases(
        run_id: impl Into<String>,
        completed_phase_ids: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        Self {
            run_id: run_id.into(),
            max_concurrent_agents: 5,
            completed_phase_ids: completed_phase_ids
                .into_iter()
                .map(Into::into)
                .collect::<Vec<_>>(),
            available_artifacts: Vec::new(),
            available_mail: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SchedulerRunPlan {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub status: String,
    pub max_concurrent_agents: u32,
    pub phase_count: usize,
    pub job_count: usize,
    pub ready_job_count: usize,
    pub phase_order: Vec<String>,
    pub ready_phase_ids: Vec<String>,
    pub receipt_policy: ScheduledReceiptExpectations,
    pub packet_policy: ExecutablePacketPolicy,
    pub phases: Vec<ScheduledPhase>,
    pub issues: Vec<SchedulerIssue>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledPhase {
    pub id: String,
    pub label: String,
    pub owner: String,
    pub depends_on: Vec<String>,
    pub blocked_by: Vec<String>,
    pub topological_order: Option<usize>,
    pub status: String,
    pub strategy: String,
    pub completion: String,
    pub reads_raw_history: bool,
    pub receipt_expectations: ScheduledReceiptExpectations,
    pub jobs: Vec<ScheduledJob>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledJob {
    pub phase_id: String,
    pub job_id: String,
    pub agent_id: String,
    pub status: String,
    pub fanout: String,
    pub when: Option<String>,
    pub max_concurrent: Option<u32>,
    pub required_artifacts: Vec<String>,
    pub missing_required_artifacts: Vec<String>,
    pub required_mail: Vec<String>,
    pub missing_required_mail: Vec<String>,
    pub bindings: BTreeMap<String, String>,
    pub binding_references: Vec<ScheduledBindingReference>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub route: ResolvedRoute,
    pub route_thread_id: String,
    pub route_to: Vec<String>,
    pub route_cc: Vec<String>,
    pub receipt_expectations: ScheduledReceiptExpectations,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledBindingReference {
    pub name: String,
    pub expression: String,
    pub references: Vec<BindingReference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScheduledReceiptExpectations {
    pub durable_receipt: String,
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

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerIssue {
    pub code: String,
    pub message: String,
    pub phase_id: Option<String>,
    pub dependency_id: Option<String>,
}

pub fn build_scheduler_run_plan(
    plan: &ExecutableOrchestrationPlan,
    request: SchedulerRunRequest,
) -> SchedulerRunPlan {
    let graph = analyze_phase_graph(plan);
    let max_concurrent_agents = request.max_concurrent_agents.max(1);
    let completed = request
        .completed_phase_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let available_artifacts = request
        .available_artifacts
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let available_mail = request
        .available_mail
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let phase_order_index = graph
        .phase_order
        .iter()
        .enumerate()
        .map(|(idx, phase_id)| (phase_id.as_str(), idx))
        .collect::<BTreeMap<_, _>>();
    let mut ready_phase_ids = Vec::new();
    let mut phases = Vec::new();
    let mut ready_job_count = 0;

    for phase in &plan.phases {
        let blocked_by = blocked_by(phase.id.as_str(), &phase.depends_on, &completed, &graph);
        let is_completed = completed.contains(phase.id.as_str());
        let is_ready = !is_completed && blocked_by.is_empty();
        if is_ready {
            ready_phase_ids.push(phase.id.clone());
        }
        let status = if is_completed {
            "completed"
        } else if is_ready {
            "ready"
        } else {
            "blocked"
        };
        let receipt_expectations = scheduled_receipts(&phase.durable_receipt, &plan.receipt_policy);
        let jobs = phase
            .jobs
            .iter()
            .map(|job| {
                scheduled_job(
                    job,
                    &receipt_expectations,
                    status,
                    is_ready,
                    &available_artifacts,
                    &available_mail,
                )
            })
            .collect::<Vec<_>>();
        ready_job_count += jobs.iter().filter(|job| job.status == "ready").count();
        phases.push(ScheduledPhase {
            id: phase.id.clone(),
            label: phase.label.clone(),
            owner: phase.owner.clone(),
            depends_on: phase.depends_on.clone(),
            blocked_by,
            topological_order: phase_order_index.get(phase.id.as_str()).copied(),
            status: status.to_string(),
            strategy: phase.strategy.clone(),
            completion: phase.completion.clone(),
            reads_raw_history: phase.reads_raw_history,
            jobs,
            receipt_expectations,
        });
    }

    let all_known_phases_completed = plan
        .phases
        .iter()
        .all(|phase| completed.contains(phase.id.as_str()));
    let status = if !graph.issues.is_empty() {
        "invalid"
    } else if all_known_phases_completed {
        "complete"
    } else if ready_phase_ids.is_empty() {
        "blocked"
    } else {
        "ready"
    };

    SchedulerRunPlan {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.scheduler_run_plan.v1".to_string(),
        run_id: safe_run_id(&request.run_id),
        status: status.to_string(),
        max_concurrent_agents,
        phase_count: plan.phase_count,
        job_count: plan.job_count,
        ready_job_count,
        phase_order: graph.phase_order,
        ready_phase_ids,
        receipt_policy: scheduled_receipts(
            "context-engine/live/mail/threads/wiki-company.jsonl",
            &plan.receipt_policy,
        ),
        packet_policy: plan.packet_policy.clone(),
        phases,
        issues: graph.issues,
    }
}

pub fn ready_phase_ids(
    plan: &ExecutableOrchestrationPlan,
    completed_phase_ids: impl IntoIterator<Item = impl Into<String>>,
) -> Vec<String> {
    build_scheduler_run_plan(
        plan,
        SchedulerRunRequest::with_completed_phases("readiness-probe", completed_phase_ids),
    )
    .ready_phase_ids
}

fn scheduled_job(
    job: &ExecutablePhaseJob,
    receipt_expectations: &ScheduledReceiptExpectations,
    phase_status: &str,
    phase_ready: bool,
    available_artifacts: &BTreeSet<String>,
    available_mail: &BTreeSet<String>,
) -> ScheduledJob {
    let missing_required_artifacts = job
        .required_artifacts
        .iter()
        .filter(|artifact| !available_artifacts.contains(artifact.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let missing_required_mail = job
        .required_mail
        .iter()
        .filter(|mail| !available_mail.contains(mail.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let status = if !phase_ready && phase_status == "completed" {
        "phase_completed"
    } else if !phase_ready {
        "phase_blocked"
    } else if !missing_required_artifacts.is_empty() && !missing_required_mail.is_empty() {
        "waiting_for_inputs"
    } else if !missing_required_artifacts.is_empty() {
        "waiting_for_artifacts"
    } else if !missing_required_mail.is_empty() {
        "waiting_for_mail"
    } else if job.when.is_some() {
        "conditional"
    } else {
        "ready"
    };
    ScheduledJob {
        phase_id: job.phase_id.clone(),
        job_id: job.job_id.clone(),
        agent_id: job.agent_id.clone(),
        status: status.to_string(),
        fanout: job.fanout.clone(),
        when: job.when.clone(),
        max_concurrent: job.max_concurrent,
        required_artifacts: job.required_artifacts.clone(),
        missing_required_artifacts,
        required_mail: job.required_mail.clone(),
        missing_required_mail,
        bindings: job.bindings.clone(),
        binding_references: job
            .bindings
            .iter()
            .map(|(name, expression)| {
                let parsed = parse_binding_expression(expression);
                ScheduledBindingReference {
                    name: name.clone(),
                    expression: expression.clone(),
                    references: parsed.references,
                }
            })
            .collect(),
        inputs: job.inputs.clone(),
        outputs: job.outputs.clone(),
        route: job.route.clone(),
        route_thread_id: job.route.thread_id.clone(),
        route_to: job.route.to.clone(),
        route_cc: job.route.cc.clone(),
        receipt_expectations: receipt_expectations.clone(),
    }
}

pub fn ready_jobs(plan: &SchedulerRunPlan) -> Vec<&ScheduledJob> {
    plan.phases
        .iter()
        .flat_map(|phase| phase.jobs.iter())
        .filter(|job| job.status == "ready")
        .collect()
}

fn scheduled_receipts(
    durable_receipt: &str,
    policy: &ExecutableReceiptPolicy,
) -> ScheduledReceiptExpectations {
    ScheduledReceiptExpectations {
        durable_receipt: durable_receipt.to_string(),
        require_birth_certificate: policy.require_birth_certificate,
        require_turn_start: policy.require_turn_start,
        require_context_injection: policy.require_context_injection,
        require_adapter_events: policy.require_adapter_events,
        require_final_message: policy.require_final_message,
        require_talk_append: policy.require_talk_append,
        require_mail_delivery: policy.require_mail_delivery,
        require_turn_complete: policy.require_turn_complete,
        final_message_path: policy.final_message_path.clone(),
        final_message_fields: policy.final_message_fields.clone(),
        talk_delivery_mode: policy.talk_delivery_mode.clone(),
        require_non_empty_final_message: policy.require_non_empty_final_message,
        do_not_count_codex_exit_as_done: policy.do_not_count_codex_exit_as_done,
    }
}

fn blocked_by(
    phase_id: &str,
    depends_on: &[String],
    completed: &BTreeSet<String>,
    graph: &PhaseGraphAnalysis,
) -> Vec<String> {
    let mut blocked = depends_on
        .iter()
        .filter(|dependency| !completed.contains(dependency.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if let Some(missing) = graph.missing_dependencies.get(phase_id) {
        blocked.extend(missing.iter().map(|dep| format!("missing:{dep}")));
    }
    if graph.invalid_dependency_phases.contains(phase_id) {
        blocked.push("invalid_dependency".to_string());
    }
    if graph.cycle_phases.contains(phase_id) {
        blocked.push("cycle".to_string());
    }
    blocked.sort();
    blocked.dedup();
    blocked
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct PhaseGraphAnalysis {
    phase_order: Vec<String>,
    issues: Vec<SchedulerIssue>,
    missing_dependencies: BTreeMap<String, Vec<String>>,
    invalid_dependency_phases: BTreeSet<String>,
    cycle_phases: BTreeSet<String>,
}

fn analyze_phase_graph(plan: &ExecutableOrchestrationPlan) -> PhaseGraphAnalysis {
    let phase_ids = plan
        .phases
        .iter()
        .map(|phase| phase.id.as_str())
        .collect::<BTreeSet<_>>();
    let phase_position = plan
        .phases
        .iter()
        .enumerate()
        .map(|(idx, phase)| (phase.id.as_str(), idx))
        .collect::<BTreeMap<_, _>>();
    let mut known_dep_count = plan
        .phases
        .iter()
        .map(|phase| (phase.id.clone(), 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut dependents = BTreeMap::<String, Vec<String>>::new();
    let mut issues = Vec::new();
    let mut missing_dependencies = BTreeMap::<String, Vec<String>>::new();

    for phase in &plan.phases {
        let mut seen = BTreeSet::new();
        for dependency in &phase.depends_on {
            if !seen.insert(dependency.as_str()) {
                continue;
            }
            if phase_ids.contains(dependency.as_str()) {
                *known_dep_count.entry(phase.id.clone()).or_default() += 1;
                dependents
                    .entry(dependency.clone())
                    .or_default()
                    .push(phase.id.clone());
            } else {
                missing_dependencies
                    .entry(phase.id.clone())
                    .or_default()
                    .push(dependency.clone());
                issues.push(SchedulerIssue {
                    code: "missing_phase_dependency".to_string(),
                    message: format!("phase {} depends on unknown phase {}", phase.id, dependency),
                    phase_id: Some(phase.id.clone()),
                    dependency_id: Some(dependency.clone()),
                });
            }
        }
    }

    let mut queue = plan
        .phases
        .iter()
        .filter(|phase| {
            known_dep_count.get(phase.id.as_str()).copied().unwrap_or(0) == 0
                && !missing_dependencies.contains_key(phase.id.as_str())
        })
        .map(|phase| phase.id.clone())
        .collect::<Vec<_>>();
    let mut phase_order = Vec::new();
    while !queue.is_empty() {
        queue.sort_by_key(|phase_id| phase_position.get(phase_id.as_str()).copied());
        let current = queue.remove(0);
        if phase_order.contains(&current) {
            continue;
        }
        phase_order.push(current.clone());
        if let Some(children) = dependents.get(&current) {
            for child in children {
                let count = known_dep_count.entry(child.clone()).or_default();
                *count = count.saturating_sub(1);
                if *count == 0 && !missing_dependencies.contains_key(child.as_str()) {
                    queue.push(child.clone());
                }
            }
        }
    }

    let mut invalid_dependency_phases = missing_dependencies
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut changed = true;
    while changed {
        changed = false;
        for phase in &plan.phases {
            if invalid_dependency_phases.contains(phase.id.as_str()) {
                continue;
            }
            if phase
                .depends_on
                .iter()
                .any(|dependency| invalid_dependency_phases.contains(dependency.as_str()))
            {
                invalid_dependency_phases.insert(phase.id.clone());
                issues.push(SchedulerIssue {
                    code: "blocked_by_invalid_phase_dependency".to_string(),
                    message: format!("phase {} is blocked by an invalid dependency", phase.id),
                    phase_id: Some(phase.id.clone()),
                    dependency_id: None,
                });
                changed = true;
            }
        }
    }

    let ordered = phase_order
        .iter()
        .map(String::as_str)
        .collect::<BTreeSet<_>>();
    let mut cycle_phases = BTreeSet::new();
    for phase in &plan.phases {
        if !ordered.contains(phase.id.as_str())
            && !invalid_dependency_phases.contains(phase.id.as_str())
        {
            cycle_phases.insert(phase.id.clone());
            issues.push(SchedulerIssue {
                code: "phase_dependency_cycle".to_string(),
                message: format!("phase {} is in a dependency cycle", phase.id),
                phase_id: Some(phase.id.clone()),
                dependency_id: None,
            });
        }
    }

    PhaseGraphAnalysis {
        phase_order,
        issues,
        missing_dependencies,
        invalid_dependency_phases,
        cycle_phases,
    }
}
