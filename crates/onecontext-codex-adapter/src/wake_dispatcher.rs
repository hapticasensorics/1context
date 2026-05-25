use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::agent_binding::{
    validate_binding, BindingValidationIssue, CodexAdapterBinding, CodexLoadedState,
    EffectiveLeaseState,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexWakeStrategy {
    SteerActiveTurn,
    InjectEnvelopeThenStartTurn,
    ResumeThenStartTurn,
    StartNewThreadThenRegister,
    PollOnly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexWakeAttemptResult {
    Accepted,
    Failed,
    Suppressed,
    StaleTurn,
    ThreadMissing,
    AgentRetired,
    PolicyDenied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexWakeAttempt {
    pub attempt_id: String,
    pub notification_id: String,
    pub delivery_id: String,
    pub message_id: String,
    pub agent_id: String,
    pub thread_id: Option<String>,
    pub active_turn_id: Option<String>,
    pub strategy: CodexWakeStrategy,
    pub result: CodexWakeAttemptResult,
    pub error_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexWakeSuppression {
    None,
    Suppressed { reason_code: String },
    PollOnly { reason_code: String },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexWakePlanReason {
    ActiveTurnMatched,
    LoadedIdleThread,
    ResumablePersistedThread,
    NewThreadRecovery,
    Suppressed,
    PollOnlyRequested,
    MissingBinding,
    InvalidBinding,
    RetiredAgent,
    StaleLease,
    UnknownLease,
    ExpiredLease,
    WakeAttemptLimitExceeded,
    MissingActiveTurn,
    StaleTurn,
    ResumeDenied,
    StartNewThreadDenied,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexWakePlan {
    pub strategy: CodexWakeStrategy,
    pub result: CodexWakeAttemptResult,
    pub reason: CodexWakePlanReason,
    pub error_code: Option<String>,
    pub thread_id: Option<String>,
    pub active_turn_id: Option<String>,
}

impl CodexWakePlan {
    pub fn dispatchable(&self) -> bool {
        self.result == CodexWakeAttemptResult::Accepted
            && self.strategy != CodexWakeStrategy::PollOnly
    }
}

#[derive(Clone, Debug)]
pub struct CodexWakePlanInput<'a> {
    pub binding: Option<&'a CodexAdapterBinding>,
    pub expected_turn_id: Option<&'a str>,
    pub suppression: CodexWakeSuppression,
    pub attempts_for_delivery: u32,
    pub now: DateTime<Utc>,
}

pub fn plan_wake(input: CodexWakePlanInput<'_>) -> CodexWakePlan {
    let thread_id = input
        .binding
        .and_then(|binding| clean_id(&binding.transport.thread_id));
    let active_turn_id = input
        .binding
        .and_then(|binding| clean_id(&binding.transport.active_turn_id));

    match input.suppression {
        CodexWakeSuppression::Suppressed { reason_code } => {
            return poll_plan(
                CodexWakeAttemptResult::Suppressed,
                CodexWakePlanReason::Suppressed,
                Some(reason_code),
                thread_id,
                active_turn_id,
            );
        }
        CodexWakeSuppression::PollOnly { reason_code } => {
            return poll_plan(
                CodexWakeAttemptResult::Accepted,
                CodexWakePlanReason::PollOnlyRequested,
                Some(reason_code),
                thread_id,
                active_turn_id,
            );
        }
        CodexWakeSuppression::None => {}
    }

    let Some(binding) = input.binding else {
        return poll_plan(
            CodexWakeAttemptResult::ThreadMissing,
            CodexWakePlanReason::MissingBinding,
            Some("binding_missing".to_string()),
            None,
            None,
        );
    };

    let thread_id = clean_id(&binding.transport.thread_id);
    let active_turn_id = clean_id(&binding.transport.active_turn_id);

    match binding.effective_lease_state(input.now) {
        EffectiveLeaseState::Active => {}
        EffectiveLeaseState::Retired => {
            return poll_plan(
                CodexWakeAttemptResult::AgentRetired,
                CodexWakePlanReason::RetiredAgent,
                Some("agent_retired".to_string()),
                thread_id,
                active_turn_id,
            );
        }
        EffectiveLeaseState::Stale => {
            return poll_plan(
                CodexWakeAttemptResult::PolicyDenied,
                CodexWakePlanReason::StaleLease,
                Some("lease_stale".to_string()),
                thread_id,
                active_turn_id,
            );
        }
        EffectiveLeaseState::Unknown => {
            return poll_plan(
                CodexWakeAttemptResult::PolicyDenied,
                CodexWakePlanReason::UnknownLease,
                Some("lease_unknown".to_string()),
                thread_id,
                active_turn_id,
            );
        }
        EffectiveLeaseState::Expired => {
            return poll_plan(
                CodexWakeAttemptResult::PolicyDenied,
                CodexWakePlanReason::ExpiredLease,
                Some("lease_expired".to_string()),
                thread_id,
                active_turn_id,
            );
        }
    }

    if input.attempts_for_delivery >= binding.policy.max_wake_attempts_per_delivery {
        return poll_plan(
            CodexWakeAttemptResult::PolicyDenied,
            CodexWakePlanReason::WakeAttemptLimitExceeded,
            Some("wake_attempt_limit_exceeded".to_string()),
            thread_id,
            active_turn_id,
        );
    }

    let validation = validate_binding(binding, input.now);
    let blocking_issue = validation.issues.iter().find(|issue| {
        !matches!(
            issue,
            BindingValidationIssue::ActiveLeaseExpired
                | BindingValidationIssue::ZeroWakeAttemptLimit
                | BindingValidationIssue::MissingThreadForLoadedState
                | BindingValidationIssue::MissingActiveTurnForLoadedActive
                | BindingValidationIssue::ActiveTurnWithoutThread
        )
    });
    if let Some(issue) = blocking_issue {
        return poll_plan(
            CodexWakeAttemptResult::PolicyDenied,
            CodexWakePlanReason::InvalidBinding,
            Some(format!("invalid_binding:{issue:?}")),
            thread_id,
            active_turn_id,
        );
    }

    match binding.transport.loaded_state {
        CodexLoadedState::LoadedActive => {
            let Some(active_thread_id) = thread_id else {
                return missing_thread_plan(binding, None);
            };
            let Some(active_turn_id) = active_turn_id else {
                return poll_plan(
                    CodexWakeAttemptResult::StaleTurn,
                    CodexWakePlanReason::MissingActiveTurn,
                    Some("missing_active_turn".to_string()),
                    Some(active_thread_id),
                    None,
                );
            };
            if let Some(expected_turn_id) = clean_str(input.expected_turn_id) {
                if expected_turn_id != active_turn_id {
                    return poll_plan(
                        CodexWakeAttemptResult::StaleTurn,
                        CodexWakePlanReason::StaleTurn,
                        Some("active_turn_mismatch".to_string()),
                        Some(active_thread_id),
                        Some(active_turn_id),
                    );
                }
            }

            CodexWakePlan {
                strategy: CodexWakeStrategy::SteerActiveTurn,
                result: CodexWakeAttemptResult::Accepted,
                reason: CodexWakePlanReason::ActiveTurnMatched,
                error_code: None,
                thread_id: Some(active_thread_id),
                active_turn_id: Some(active_turn_id),
            }
        }
        CodexLoadedState::LoadedIdle => {
            let Some(idle_thread_id) = thread_id else {
                return missing_thread_plan(binding, active_turn_id);
            };
            CodexWakePlan {
                strategy: CodexWakeStrategy::InjectEnvelopeThenStartTurn,
                result: CodexWakeAttemptResult::Accepted,
                reason: CodexWakePlanReason::LoadedIdleThread,
                error_code: None,
                thread_id: Some(idle_thread_id),
                active_turn_id: None,
            }
        }
        CodexLoadedState::NotLoaded | CodexLoadedState::Unknown => {
            let Some(persisted_thread_id) = thread_id else {
                return missing_thread_plan(binding, active_turn_id);
            };
            if binding.policy.can_resume_thread {
                CodexWakePlan {
                    strategy: CodexWakeStrategy::ResumeThenStartTurn,
                    result: CodexWakeAttemptResult::Accepted,
                    reason: CodexWakePlanReason::ResumablePersistedThread,
                    error_code: None,
                    thread_id: Some(persisted_thread_id),
                    active_turn_id: None,
                }
            } else {
                poll_plan(
                    CodexWakeAttemptResult::PolicyDenied,
                    CodexWakePlanReason::ResumeDenied,
                    Some("resume_thread_denied".to_string()),
                    Some(persisted_thread_id),
                    active_turn_id,
                )
            }
        }
    }
}

fn missing_thread_plan(
    binding: &CodexAdapterBinding,
    active_turn_id: Option<String>,
) -> CodexWakePlan {
    if binding.policy.can_start_new_thread {
        CodexWakePlan {
            strategy: CodexWakeStrategy::StartNewThreadThenRegister,
            result: CodexWakeAttemptResult::Accepted,
            reason: CodexWakePlanReason::NewThreadRecovery,
            error_code: None,
            thread_id: None,
            active_turn_id,
        }
    } else {
        poll_plan(
            CodexWakeAttemptResult::PolicyDenied,
            CodexWakePlanReason::StartNewThreadDenied,
            Some("start_new_thread_denied".to_string()),
            None,
            active_turn_id,
        )
    }
}

fn poll_plan(
    result: CodexWakeAttemptResult,
    reason: CodexWakePlanReason,
    error_code: Option<String>,
    thread_id: Option<String>,
    active_turn_id: Option<String>,
) -> CodexWakePlan {
    CodexWakePlan {
        strategy: CodexWakeStrategy::PollOnly,
        result,
        reason,
        error_code,
        thread_id,
        active_turn_id,
    }
}

fn clean_id(id: &Option<String>) -> Option<String> {
    clean_str(id.as_deref()).map(ToString::to_string)
}

fn clean_str(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_binding::{
        AgentLeaseProjection, AgentLeaseStateProjection, CodexAdapterPolicyProjection,
        CodexToolsetProjection, CodexTransportProjection,
    };
    use chrono::Duration;

    fn binding(now: DateTime<Utc>) -> CodexAdapterBinding {
        CodexAdapterBinding {
            binding_id: "binding-1".to_string(),
            agent_id: "agent-1".to_string(),
            agent_unit_id: Some("unit-1".to_string()),
            primary_address: "agent-1@1context.local".to_string(),
            transport: CodexTransportProjection {
                kind: "codex-app-server".to_string(),
                codex_home: None,
                app_server_endpoint: Some("http://127.0.0.1:1455".to_string()),
                thread_id: Some("thread-1".to_string()),
                session_id: Some("session-1".to_string()),
                active_turn_id: None,
                loaded_state: CodexLoadedState::LoadedIdle,
            },
            toolsets: CodexToolsetProjection {
                mcp_server_name: "onecontext".to_string(),
                visible_toolsets: vec!["toolset-mail".to_string(), "toolset-wiki".to_string()],
                hidden_host_tools: vec!["thread.inject_items".to_string()],
            },
            lease: AgentLeaseProjection {
                state: AgentLeaseStateProjection::Active,
                expires_at: Some(now + Duration::minutes(5)),
                last_heartbeat_at: Some(now - Duration::seconds(30)),
            },
            policy: CodexAdapterPolicyProjection {
                can_resume_thread: true,
                can_start_new_thread: true,
                can_install_project_hooks: false,
                require_managed_hooks: true,
                max_wake_attempts_per_delivery: 3,
            },
            observed_at: now,
        }
    }

    fn input<'a>(binding: &'a CodexAdapterBinding, now: DateTime<Utc>) -> CodexWakePlanInput<'a> {
        CodexWakePlanInput {
            binding: Some(binding),
            expected_turn_id: None,
            suppression: CodexWakeSuppression::None,
            attempts_for_delivery: 0,
            now,
        }
    }

    #[test]
    fn chooses_active_turn_steering_when_turn_matches() {
        let now = Utc::now();
        let mut binding = binding(now);
        binding.transport.loaded_state = CodexLoadedState::LoadedActive;
        binding.transport.active_turn_id = Some("turn-1".to_string());
        let mut input = input(&binding, now);
        input.expected_turn_id = Some("turn-1");

        let plan = plan_wake(input);

        assert_eq!(plan.strategy, CodexWakeStrategy::SteerActiveTurn);
        assert_eq!(plan.result, CodexWakeAttemptResult::Accepted);
        assert_eq!(plan.reason, CodexWakePlanReason::ActiveTurnMatched);
        assert!(plan.dispatchable());
    }

    #[test]
    fn stale_active_turn_falls_back_to_poll_only() {
        let now = Utc::now();
        let mut binding = binding(now);
        binding.transport.loaded_state = CodexLoadedState::LoadedActive;
        binding.transport.active_turn_id = Some("turn-2".to_string());
        let mut input = input(&binding, now);
        input.expected_turn_id = Some("turn-1");

        let plan = plan_wake(input);

        assert_eq!(plan.strategy, CodexWakeStrategy::PollOnly);
        assert_eq!(plan.result, CodexWakeAttemptResult::StaleTurn);
        assert_eq!(plan.reason, CodexWakePlanReason::StaleTurn);
        assert!(!plan.dispatchable());
    }

    #[test]
    fn chooses_idle_thread_envelope_and_turn_start() {
        let now = Utc::now();
        let binding = binding(now);

        let plan = plan_wake(input(&binding, now));

        assert_eq!(
            plan.strategy,
            CodexWakeStrategy::InjectEnvelopeThenStartTurn
        );
        assert_eq!(plan.result, CodexWakeAttemptResult::Accepted);
        assert_eq!(plan.reason, CodexWakePlanReason::LoadedIdleThread);
    }

    #[test]
    fn chooses_resume_for_persisted_thread() {
        let now = Utc::now();
        let mut binding = binding(now);
        binding.transport.loaded_state = CodexLoadedState::NotLoaded;

        let plan = plan_wake(input(&binding, now));

        assert_eq!(plan.strategy, CodexWakeStrategy::ResumeThenStartTurn);
        assert_eq!(plan.reason, CodexWakePlanReason::ResumablePersistedThread);
    }

    #[test]
    fn chooses_new_thread_recovery_when_thread_is_missing() {
        let now = Utc::now();
        let mut binding = binding(now);
        binding.transport.loaded_state = CodexLoadedState::Unknown;
        binding.transport.thread_id = None;

        let plan = plan_wake(input(&binding, now));

        assert_eq!(plan.strategy, CodexWakeStrategy::StartNewThreadThenRegister);
        assert_eq!(plan.result, CodexWakeAttemptResult::Accepted);
        assert_eq!(plan.reason, CodexWakePlanReason::NewThreadRecovery);
    }

    #[test]
    fn suppression_wins_before_transport_planning() {
        let now = Utc::now();
        let mut binding = binding(now);
        binding.transport.loaded_state = CodexLoadedState::LoadedActive;
        binding.transport.active_turn_id = Some("turn-1".to_string());

        let plan = plan_wake(CodexWakePlanInput {
            binding: Some(&binding),
            expected_turn_id: Some("turn-1"),
            suppression: CodexWakeSuppression::Suppressed {
                reason_code: "claimed_elsewhere".to_string(),
            },
            attempts_for_delivery: 0,
            now,
        });

        assert_eq!(plan.strategy, CodexWakeStrategy::PollOnly);
        assert_eq!(plan.result, CodexWakeAttemptResult::Suppressed);
        assert_eq!(plan.reason, CodexWakePlanReason::Suppressed);
        assert_eq!(plan.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(plan.active_turn_id.as_deref(), Some("turn-1"));
    }

    #[test]
    fn explicit_poll_only_request_stays_accepted_without_dispatch() {
        let now = Utc::now();
        let binding = binding(now);

        let plan = plan_wake(CodexWakePlanInput {
            binding: Some(&binding),
            expected_turn_id: None,
            suppression: CodexWakeSuppression::PollOnly {
                reason_code: "manual_poll_window".to_string(),
            },
            attempts_for_delivery: 0,
            now,
        });

        assert_eq!(plan.strategy, CodexWakeStrategy::PollOnly);
        assert_eq!(plan.result, CodexWakeAttemptResult::Accepted);
        assert_eq!(plan.reason, CodexWakePlanReason::PollOnlyRequested);
        assert_eq!(plan.thread_id.as_deref(), Some("thread-1"));
        assert!(!plan.dispatchable());
    }

    #[test]
    fn retired_or_stale_lease_is_not_dispatched() {
        let now = Utc::now();
        let mut retired = binding(now);
        retired.lease.state = AgentLeaseStateProjection::Retired;

        let retired_plan = plan_wake(input(&retired, now));
        assert_eq!(retired_plan.result, CodexWakeAttemptResult::AgentRetired);
        assert_eq!(retired_plan.reason, CodexWakePlanReason::RetiredAgent);

        let mut stale = binding(now);
        stale.lease.state = AgentLeaseStateProjection::Stale;
        let stale_plan = plan_wake(input(&stale, now));
        assert_eq!(stale_plan.result, CodexWakeAttemptResult::PolicyDenied);
        assert_eq!(stale_plan.reason, CodexWakePlanReason::StaleLease);
    }

    #[test]
    fn policy_denies_resume_start_and_attempt_overflow() {
        let now = Utc::now();
        let mut resume_denied = binding(now);
        resume_denied.transport.loaded_state = CodexLoadedState::NotLoaded;
        resume_denied.policy.can_resume_thread = false;

        let resume_plan = plan_wake(input(&resume_denied, now));
        assert_eq!(resume_plan.result, CodexWakeAttemptResult::PolicyDenied);
        assert_eq!(resume_plan.reason, CodexWakePlanReason::ResumeDenied);

        let mut start_denied = binding(now);
        start_denied.transport.thread_id = None;
        start_denied.policy.can_start_new_thread = false;
        let start_plan = plan_wake(input(&start_denied, now));
        assert_eq!(start_plan.result, CodexWakeAttemptResult::PolicyDenied);
        assert_eq!(start_plan.reason, CodexWakePlanReason::StartNewThreadDenied);

        let limit_binding = binding(now);
        let mut limit = input(&limit_binding, now);
        limit.attempts_for_delivery = 3;
        let limit_plan = plan_wake(limit);
        assert_eq!(limit_plan.result, CodexWakeAttemptResult::PolicyDenied);
        assert_eq!(
            limit_plan.reason,
            CodexWakePlanReason::WakeAttemptLimitExceeded
        );
    }
}
