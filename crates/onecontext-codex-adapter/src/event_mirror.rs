use chrono::{DateTime, Utc};
use onecontext_agent_harness_core::{AdapterEventKind, AdapterEventStatus};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexRuntimeEvent {
    ThreadStatusChanged,
    TurnStarted,
    TurnCompleted,
    TurnInterrupted,
    TurnErrored,
    ItemStarted,
    ItemCompleted,
    ToolCallObserved,
    ToolCallDenied,
    ApprovalRequested,
    ApprovalResolved,
    HookRegistryObserved,
    HookDecisionObserved,
    ContextInjectionRequested,
    ContextInjectionExecuted,
    AppServerError,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventMirrorTarget {
    MailControlEvent,
    AgentHarnessAdapterEvent,
    AdapterEventMirrorLog,
    SupervisorMail,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EventMirrorPlan {
    pub runtime_event: CodexRuntimeEvent,
    pub targets: Vec<EventMirrorTarget>,
    pub correlation_id: Option<String>,
    pub mail_control_kind: Option<String>,
    pub harness_event_kind: Option<AdapterEventKind>,
    pub harness_event_status: Option<AdapterEventStatus>,
    pub supervisor_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl EventMirrorPlan {
    pub fn requires_mail_control_event(&self) -> bool {
        self.targets.contains(&EventMirrorTarget::MailControlEvent)
    }

    pub fn requires_harness_adapter_event(&self) -> bool {
        self.targets
            .contains(&EventMirrorTarget::AgentHarnessAdapterEvent)
    }

    pub fn requires_supervisor_mail(&self) -> bool {
        self.targets.contains(&EventMirrorTarget::SupervisorMail)
    }
}

pub fn mirror_runtime_event(
    runtime_event: CodexRuntimeEvent,
    correlation_id: impl Into<Option<String>>,
) -> EventMirrorPlan {
    let (mail_control_kind, harness_event_kind, harness_event_status, supervisor_reason) =
        mirror_projection(&runtime_event);
    let mut targets = vec![EventMirrorTarget::AdapterEventMirrorLog];
    if mail_control_kind.is_some() {
        targets.push(EventMirrorTarget::MailControlEvent);
    }
    if harness_event_kind.is_some() {
        targets.push(EventMirrorTarget::AgentHarnessAdapterEvent);
    }
    if supervisor_reason.is_some() {
        targets.push(EventMirrorTarget::SupervisorMail);
    }

    EventMirrorPlan {
        runtime_event,
        targets,
        correlation_id: correlation_id.into(),
        mail_control_kind,
        harness_event_kind,
        harness_event_status,
        supervisor_reason,
        created_at: Utc::now(),
    }
}

fn mirror_projection(
    runtime_event: &CodexRuntimeEvent,
) -> (
    Option<String>,
    Option<AdapterEventKind>,
    Option<AdapterEventStatus>,
    Option<String>,
) {
    match runtime_event {
        CodexRuntimeEvent::ThreadStatusChanged => (
            Some("codex.thread_status_changed".to_string()),
            Some(AdapterEventKind::TransportIdentityObserved),
            Some(AdapterEventStatus::Observed),
            None,
        ),
        CodexRuntimeEvent::TurnStarted => (
            Some("codex.turn_started".to_string()),
            Some(AdapterEventKind::AgentHeartbeatObserved),
            Some(AdapterEventStatus::Observed),
            None,
        ),
        CodexRuntimeEvent::TurnCompleted => (
            Some("codex.turn_completed".to_string()),
            Some(AdapterEventKind::AgentHeartbeatObserved),
            Some(AdapterEventStatus::Accepted),
            None,
        ),
        CodexRuntimeEvent::TurnInterrupted => (
            Some("codex.turn_interrupted".to_string()),
            Some(AdapterEventKind::RuntimeWakeupFailed),
            Some(AdapterEventStatus::Blocked),
            Some("turn_interrupted".to_string()),
        ),
        CodexRuntimeEvent::TurnErrored => (
            Some("codex.turn_errored".to_string()),
            Some(AdapterEventKind::RuntimeWakeupFailed),
            Some(AdapterEventStatus::Failed),
            Some("turn_error".to_string()),
        ),
        CodexRuntimeEvent::ItemStarted => {
            (Some("codex.item_started".to_string()), None, None, None)
        }
        CodexRuntimeEvent::ItemCompleted => {
            (Some("codex.item_completed".to_string()), None, None, None)
        }
        CodexRuntimeEvent::ToolCallObserved => (
            Some("codex.tool_call_observed".to_string()),
            Some(AdapterEventKind::ToolCallObserved),
            Some(AdapterEventStatus::Observed),
            None,
        ),
        CodexRuntimeEvent::ToolCallDenied => (
            Some("codex.tool_call_denied".to_string()),
            Some(AdapterEventKind::ToolCallDenied),
            Some(AdapterEventStatus::Blocked),
            Some("tool_call_denied".to_string()),
        ),
        CodexRuntimeEvent::ApprovalRequested => (
            Some("codex.approval_requested".to_string()),
            Some(AdapterEventKind::SupervisorDispatchAttempted),
            Some(AdapterEventStatus::Observed),
            Some("approval_requested".to_string()),
        ),
        CodexRuntimeEvent::ApprovalResolved => (
            Some("codex.approval_resolved".to_string()),
            Some(AdapterEventKind::SupervisorDispatchAttempted),
            Some(AdapterEventStatus::Accepted),
            None,
        ),
        CodexRuntimeEvent::HookRegistryObserved => (
            Some("codex.hook_registry_observed".to_string()),
            Some(AdapterEventKind::HookRegistryObserved),
            Some(AdapterEventStatus::Observed),
            None,
        ),
        CodexRuntimeEvent::HookDecisionObserved => (
            Some("codex.hook_decision_observed".to_string()),
            Some(AdapterEventKind::HookDecisionObserved),
            Some(AdapterEventStatus::Observed),
            None,
        ),
        CodexRuntimeEvent::ContextInjectionRequested => (
            Some("codex.context_injection_requested".to_string()),
            Some(AdapterEventKind::ContextInjectionRequested),
            Some(AdapterEventStatus::Observed),
            None,
        ),
        CodexRuntimeEvent::ContextInjectionExecuted => (
            Some("codex.context_injection_executed".to_string()),
            Some(AdapterEventKind::ContextInjectionExecuted),
            Some(AdapterEventStatus::Accepted),
            None,
        ),
        CodexRuntimeEvent::AppServerError => (
            Some("codex.app_server_error".to_string()),
            Some(AdapterEventKind::RuntimeWakeupFailed),
            Some(AdapterEventStatus::Failed),
            Some("app_server_error".to_string()),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mirrors_tool_call_to_mail_harness_and_diagnostic_targets() {
        let plan = mirror_runtime_event(CodexRuntimeEvent::ToolCallObserved, Some("corr-1".into()));

        assert!(plan.requires_mail_control_event());
        assert!(plan.requires_harness_adapter_event());
        assert_eq!(
            plan.mail_control_kind.as_deref(),
            Some("codex.tool_call_observed")
        );
        assert_eq!(
            plan.harness_event_kind,
            Some(AdapterEventKind::ToolCallObserved)
        );
        assert_eq!(
            plan.harness_event_status,
            Some(AdapterEventStatus::Observed)
        );
        assert!(!plan.requires_supervisor_mail());
    }

    #[test]
    fn mirrors_error_family_to_supervisor_target() {
        let plan = mirror_runtime_event(CodexRuntimeEvent::AppServerError, None);

        assert!(plan.requires_mail_control_event());
        assert!(plan.requires_harness_adapter_event());
        assert!(plan.requires_supervisor_mail());
        assert_eq!(plan.supervisor_reason.as_deref(), Some("app_server_error"));
    }

    #[test]
    fn mirrors_injection_family_to_harness_context_proof() {
        let plan = mirror_runtime_event(CodexRuntimeEvent::ContextInjectionExecuted, None);

        assert_eq!(
            plan.harness_event_kind,
            Some(AdapterEventKind::ContextInjectionExecuted)
        );
        assert_eq!(
            plan.harness_event_status,
            Some(AdapterEventStatus::Accepted)
        );
    }
}
