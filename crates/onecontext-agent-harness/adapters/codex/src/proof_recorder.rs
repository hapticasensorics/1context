use chrono::{DateTime, Utc};
use onecontext_agent_harness_core::{
    AdapterCorrelation, AdapterEventKind, AdapterEventRequest, AdapterEventStatus, AdapterKind,
    AgentUnitId,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::hook_manager::CodexHookIntent;
use crate::injection_bridge::{CodexInjectionJob, InjectionJobStatus};
use crate::policy_bridge::{deny_unredacted_evidence, AdapterPolicyDecision};
use crate::wake_dispatcher::{CodexWakeAttempt, CodexWakeAttemptResult};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofRecordTarget {
    AgentHarness,
    MailControlLedger,
    AdapterDiagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofRecordFamily {
    Wake,
    Injection,
    Hook,
    ToolsetVisibility,
    EventMirror,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofRecordPlan {
    pub target: ProofRecordTarget,
    pub kind: String,
    pub summary: String,
    pub redacted: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessProofRecordPlan {
    pub target: ProofRecordTarget,
    pub family: ProofRecordFamily,
    pub kind: String,
    pub summary: String,
    pub redacted: bool,
    pub policy: AdapterPolicyDecision,
    pub harness_request: Option<AdapterEventRequest>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolsetVisibilityObservation {
    pub visible_toolsets: Vec<String>,
    pub hidden_host_tools: Vec<String>,
    pub unauthorized_visible_toolsets: Vec<String>,
}

impl ToolsetVisibilityObservation {
    pub fn expected_mail_and_wiki(
        visible_toolsets: Vec<String>,
        hidden_host_tools: Vec<String>,
    ) -> Self {
        Self::for_required_toolsets(
            visible_toolsets,
            hidden_host_tools,
            &["toolset-mail", "toolset-wiki"],
        )
    }

    pub fn for_required_toolsets(
        visible_toolsets: Vec<String>,
        hidden_host_tools: Vec<String>,
        required_toolsets: &[&str],
    ) -> Self {
        let unauthorized_visible_toolsets = visible_toolsets
            .iter()
            .filter(|toolset| {
                !required_toolsets
                    .iter()
                    .any(|required| toolset.trim() == *required)
            })
            .cloned()
            .collect();

        Self {
            visible_toolsets,
            hidden_host_tools,
            unauthorized_visible_toolsets,
        }
    }
}

impl HarnessProofRecordPlan {
    pub fn harness_request(&self) -> Option<&AdapterEventRequest> {
        self.harness_request.as_ref()
    }
}

pub fn wake_proof_plan(
    unit_id: impl Into<String>,
    attempt: &CodexWakeAttempt,
) -> HarnessProofRecordPlan {
    let (kind, status) = match attempt.result {
        CodexWakeAttemptResult::Accepted => (
            AdapterEventKind::RuntimeWakeupAccepted,
            AdapterEventStatus::Accepted,
        ),
        CodexWakeAttemptResult::Suppressed => (
            AdapterEventKind::SupervisorDispatchSuppressed,
            AdapterEventStatus::Suppressed,
        ),
        CodexWakeAttemptResult::Failed
        | CodexWakeAttemptResult::StaleTurn
        | CodexWakeAttemptResult::ThreadMissing
        | CodexWakeAttemptResult::AgentRetired
        | CodexWakeAttemptResult::PolicyDenied => (
            AdapterEventKind::RuntimeWakeupFailed,
            AdapterEventStatus::Failed,
        ),
    };

    let evidence = json!({
        "strategy": attempt.strategy,
        "result": attempt.result,
        "error_code": attempt.error_code,
    });
    harness_plan(HarnessPlanInput {
        family: ProofRecordFamily::Wake,
        kind: "codex.wake_attempt".to_string(),
        summary: format!(
            "wake attempt {} for delivery {} recorded as {:?}",
            attempt.attempt_id, attempt.delivery_id, attempt.result
        ),
        unit_id: unit_id.into(),
        adapter_event_kind: kind,
        status,
        correlation: AdapterCorrelation {
            thread_id: attempt.thread_id.clone(),
            turn_id: attempt.active_turn_id.clone(),
            notification_id: Some(attempt.notification_id.clone()),
            transport_attempt_id: Some(attempt.attempt_id.clone()),
            delivery_id: Some(attempt.delivery_id.clone()),
            message_id: Some(attempt.message_id.clone()),
            ..AdapterCorrelation::default()
        },
        evidence,
    })
}

pub fn injection_proof_plan(
    unit_id: impl Into<String>,
    job: &CodexInjectionJob,
) -> HarnessProofRecordPlan {
    let (kind, status) = match job.status {
        InjectionJobStatus::Queued => (
            AdapterEventKind::ContextInjectionRequested,
            AdapterEventStatus::Observed,
        ),
        InjectionJobStatus::Executed => (
            AdapterEventKind::ContextInjectionExecuted,
            AdapterEventStatus::Accepted,
        ),
        InjectionJobStatus::Failed => (
            AdapterEventKind::ContextInjectionExecuted,
            AdapterEventStatus::Failed,
        ),
        InjectionJobStatus::Superseded => (
            AdapterEventKind::ContextInjectionRequested,
            AdapterEventStatus::Suppressed,
        ),
    };

    let evidence = json!({
        "injection_job_id": job.injection_job_id,
        "body_sha256": job.body_sha256,
        "item_count": job.item_count,
        "status": job.status,
    });
    harness_plan(HarnessPlanInput {
        family: ProofRecordFamily::Injection,
        kind: "codex.context_injection".to_string(),
        summary: format!(
            "context injection job {} for delivery {} recorded as {:?}",
            job.injection_job_id, job.delivery_id, job.status
        ),
        unit_id: unit_id.into(),
        adapter_event_kind: kind,
        status,
        correlation: AdapterCorrelation {
            thread_id: Some(job.thread_id.clone()),
            delivery_id: Some(job.delivery_id.clone()),
            message_id: Some(job.message_id.clone()),
            tool_call_id: job.requested_by_tool_call_id.clone(),
            ..AdapterCorrelation::default()
        },
        evidence,
    })
}

pub fn hook_proof_plan(
    unit_id: impl Into<String>,
    intent: &CodexHookIntent,
) -> HarnessProofRecordPlan {
    let evidence = json!({
        "hook_event_name": intent.hook_event_name,
        "action": intent.action,
        "input_sha256": intent.input_sha256,
        "output_sha256": intent.output_sha256,
    });
    harness_plan(HarnessPlanInput {
        family: ProofRecordFamily::Hook,
        kind: "codex.hook_decision".to_string(),
        summary: format!(
            "hook intent {} recorded for {:?}",
            intent.intent_id, intent.hook_event_name
        ),
        unit_id: unit_id.into(),
        adapter_event_kind: AdapterEventKind::HookDecisionObserved,
        status: AdapterEventStatus::Observed,
        correlation: AdapterCorrelation {
            thread_id: intent.codex.thread_id.clone(),
            session_id: intent.codex.session_id.clone(),
            turn_id: intent.codex.turn_id.clone(),
            delivery_id: intent.refs.delivery_id.clone(),
            message_id: intent.refs.message_id.clone(),
            tool_call_id: intent.codex.tool_use_id.clone(),
            hook_key: Some(format!("{:?}", intent.hook_event_name)),
            ..AdapterCorrelation::default()
        },
        evidence,
    })
}

pub fn toolset_visibility_proof_plan(
    unit_id: impl Into<String>,
    observation: ToolsetVisibilityObservation,
) -> HarnessProofRecordPlan {
    toolset_visibility_proof_plan_for_required(
        unit_id,
        observation,
        &["toolset-mail", "toolset-wiki"],
    )
}

pub fn toolset_visibility_proof_plan_for_required(
    unit_id: impl Into<String>,
    observation: ToolsetVisibilityObservation,
    required: &[&str],
) -> HarnessProofRecordPlan {
    let missing_toolsets: Vec<String> = required
        .iter()
        .filter(|required| {
            !observation
                .visible_toolsets
                .iter()
                .any(|visible| visible.trim() == **required)
        })
        .map(|required| (*required).to_string())
        .collect();
    let accepted =
        missing_toolsets.is_empty() && observation.unauthorized_visible_toolsets.is_empty();

    let evidence = json!({
        "visible_toolsets": observation.visible_toolsets,
        "hidden_host_tool_count": observation.hidden_host_tools.len(),
        "missing_toolsets": missing_toolsets,
        "unauthorized_visible_toolsets": observation.unauthorized_visible_toolsets,
    });
    harness_plan(HarnessPlanInput {
        family: ProofRecordFamily::ToolsetVisibility,
        kind: "codex.toolset_visibility".to_string(),
        summary: if accepted {
            "toolset-mail and toolset-wiki are visible and host controls remain hidden".to_string()
        } else {
            "toolset visibility does not match adapter policy".to_string()
        },
        unit_id: unit_id.into(),
        adapter_event_kind: AdapterEventKind::ToolAllowlistChecked,
        status: if accepted {
            AdapterEventStatus::Accepted
        } else {
            AdapterEventStatus::Blocked
        },
        correlation: AdapterCorrelation::default(),
        evidence,
    })
}

struct HarnessPlanInput {
    family: ProofRecordFamily,
    kind: String,
    summary: String,
    unit_id: String,
    adapter_event_kind: AdapterEventKind,
    status: AdapterEventStatus,
    correlation: AdapterCorrelation,
    evidence: Value,
}

fn harness_plan(input: HarnessPlanInput) -> HarnessProofRecordPlan {
    let policy = deny_unredacted_evidence(&input.evidence);
    let redacted = policy.allowed;
    let harness_request = redacted.then(|| AdapterEventRequest {
        unit_id: AgentUnitId(input.unit_id),
        adapter: AdapterKind::CodexAppServer,
        kind: input.adapter_event_kind,
        status: input.status,
        correlation: input.correlation,
        evidence: input.evidence,
        redaction: Default::default(),
    });

    HarnessProofRecordPlan {
        target: ProofRecordTarget::AgentHarness,
        family: input.family,
        kind: input.kind,
        summary: input.summary,
        redacted,
        policy,
        harness_request,
        created_at: Utc::now(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_manager::{
        CodexHookEventName, CodexHookIntentAction, CodexHookIntentCodexRefs, CodexHookIntentRefs,
    };
    use crate::wake_dispatcher::CodexWakeStrategy;

    #[test]
    fn wake_proof_uses_runtime_wakeup_family() {
        let attempt = CodexWakeAttempt {
            attempt_id: "attempt-1".to_string(),
            notification_id: "notification-1".to_string(),
            delivery_id: "delivery-1".to_string(),
            message_id: "message-1".to_string(),
            agent_id: "agent-1".to_string(),
            thread_id: Some("thread-1".to_string()),
            active_turn_id: Some("turn-1".to_string()),
            strategy: CodexWakeStrategy::SteerActiveTurn,
            result: CodexWakeAttemptResult::Accepted,
            error_code: None,
            created_at: Utc::now(),
        };

        let plan = wake_proof_plan("unit-1", &attempt);
        let request = plan.harness_request().expect("harness request");

        assert_eq!(plan.family, ProofRecordFamily::Wake);
        assert!(plan.redacted);
        assert_eq!(request.kind, AdapterEventKind::RuntimeWakeupAccepted);
        assert_eq!(request.status, AdapterEventStatus::Accepted);
        assert_eq!(
            request.correlation.delivery_id.as_deref(),
            Some("delivery-1")
        );
    }

    #[test]
    fn injection_proof_records_hash_without_body() {
        let job = CodexInjectionJob {
            injection_job_id: "injection-1".to_string(),
            delivery_id: "delivery-1".to_string(),
            message_id: "message-1".to_string(),
            agent_id: "agent-1".to_string(),
            thread_id: "thread-1".to_string(),
            requested_by_tool_call_id: Some("tool-1".to_string()),
            body_sha256: "abc123".to_string(),
            item_count: 1,
            status: InjectionJobStatus::Executed,
            created_at: Utc::now(),
        };

        let plan = injection_proof_plan("unit-1", &job);
        let request = plan.harness_request().expect("harness request");

        assert_eq!(plan.family, ProofRecordFamily::Injection);
        assert_eq!(request.kind, AdapterEventKind::ContextInjectionExecuted);
        assert_eq!(request.status, AdapterEventStatus::Accepted);
        assert_eq!(request.evidence["body_sha256"], "abc123");
        assert!(request.evidence.get("body").is_none());
    }

    #[test]
    fn hook_proof_uses_hook_decision_family() {
        let intent = CodexHookIntent {
            intent_id: "hook-1".to_string(),
            hook_event_name: CodexHookEventName::PostToolUse,
            codex: CodexHookIntentCodexRefs {
                session_id: Some("session-1".to_string()),
                thread_id: Some("thread-1".to_string()),
                turn_id: Some("turn-1".to_string()),
                tool_name: Some("toolset-mail.open".to_string()),
                tool_use_id: Some("tool-1".to_string()),
            },
            action: CodexHookIntentAction::QueueInjection,
            refs: CodexHookIntentRefs {
                agent_id: Some("agent-1".to_string()),
                notification_id: None,
                delivery_id: Some("delivery-1".to_string()),
                message_id: Some("message-1".to_string()),
                claim_id: None,
            },
            input_sha256: Some("input-hash".to_string()),
            output_sha256: Some("output-hash".to_string()),
            created_at: Utc::now(),
        };

        let plan = hook_proof_plan("unit-1", &intent);
        let request = plan.harness_request().expect("harness request");

        assert_eq!(plan.family, ProofRecordFamily::Hook);
        assert_eq!(request.kind, AdapterEventKind::HookDecisionObserved);
        assert_eq!(request.correlation.tool_call_id.as_deref(), Some("tool-1"));
        assert!(plan.redacted);
    }

    #[test]
    fn toolset_visibility_proof_checks_mail_wiki_and_hidden_host_controls() {
        let observation = ToolsetVisibilityObservation::expected_mail_and_wiki(
            vec!["toolset-mail".to_string(), "toolset-wiki".to_string()],
            vec!["host.dispatch".to_string(), "host.inject".to_string()],
        );

        let plan = toolset_visibility_proof_plan("unit-1", observation);
        let request = plan.harness_request().expect("harness request");

        assert_eq!(plan.family, ProofRecordFamily::ToolsetVisibility);
        assert_eq!(request.kind, AdapterEventKind::ToolAllowlistChecked);
        assert_eq!(request.status, AdapterEventStatus::Accepted);
        assert_eq!(request.evidence["hidden_host_tool_count"], 2);
    }

    #[test]
    fn toolset_visibility_blocks_extra_visible_host_tool() {
        let observation = ToolsetVisibilityObservation::expected_mail_and_wiki(
            vec![
                "toolset-mail".to_string(),
                "toolset-wiki".to_string(),
                "host".to_string(),
            ],
            vec![],
        );

        let plan = toolset_visibility_proof_plan("unit-1", observation);
        let request = plan.harness_request().expect("harness request");

        assert_eq!(request.status, AdapterEventStatus::Blocked);
        assert_eq!(request.evidence["unauthorized_visible_toolsets"][0], "host");
    }

    #[test]
    fn toolset_visibility_can_prove_single_toolset_bindings() {
        let observation = ToolsetVisibilityObservation::for_required_toolsets(
            vec!["toolset-mail".to_string()],
            vec!["host.dispatch".to_string()],
            &["toolset-mail"],
        );

        let plan =
            toolset_visibility_proof_plan_for_required("unit-1", observation, &["toolset-mail"]);
        let request = plan.harness_request().expect("harness request");

        assert_eq!(request.status, AdapterEventStatus::Accepted);
        assert_eq!(
            request.evidence["missing_toolsets"]
                .as_array()
                .unwrap()
                .len(),
            0
        );
    }
}
