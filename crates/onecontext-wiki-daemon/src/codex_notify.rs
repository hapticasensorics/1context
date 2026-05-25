use anyhow::{anyhow, Result};
use onecontext_wiki_core::agent_mail::{CodexSteeringPayload, MessagePageRef};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexDispatchRequest<'a> {
    pub payload: &'a CodexSteeringPayload,
    pub runtime_status: CodexRuntimeStatus,
    pub policy: CodexSupervisorPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "state")]
pub enum CodexRuntimeStatus {
    ActiveTurn {
        thread_id: String,
        turn_id: Option<String>,
    },
    LoadedIdle {
        thread_id: String,
    },
    Resumable {
        thread_id: String,
    },
    NoResumableThread,
    UnknownAgent,
    RetiredAgent,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodexSupervisorPolicy {
    pub allow_start_loaded_thread: bool,
    pub allow_resume_thread: bool,
    pub sandbox_mode: Option<String>,
    pub approval_policy: Option<String>,
}

impl Default for CodexSupervisorPolicy {
    fn default() -> Self {
        Self {
            allow_start_loaded_thread: true,
            allow_resume_thread: true,
            sandbox_mode: None,
            approval_policy: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexDispatchDecision {
    pub schema_version: u32,
    pub decision_id: String,
    pub action: CodexDispatchAction,
    pub evidence: CodexDispatchEvidence,
}

impl CodexDispatchDecision {
    pub fn is_dispatchable(&self) -> bool {
        matches!(
            self.action,
            CodexDispatchAction::SteerActiveTurn { .. }
                | CodexDispatchAction::StartLoadedThread { .. }
                | CodexDispatchAction::ResumeThread { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CodexDispatchAction {
    SteerActiveTurn {
        thread_id: String,
        expected_turn_id: Option<String>,
    },
    StartLoadedThread {
        thread_id: String,
    },
    ResumeThread {
        thread_id: String,
    },
    Failed {
        terminal_error: CodexDispatchTerminalError,
    },
}

impl CodexDispatchAction {
    fn control_name(&self) -> &'static str {
        match self {
            Self::SteerActiveTurn { .. } => "turn/steer",
            Self::StartLoadedThread { .. } => "turn/start",
            Self::ResumeThread { .. } => "thread/resume",
            Self::Failed { terminal_error } => terminal_error.code(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "code")]
pub enum CodexDispatchTerminalError {
    UnknownAgent,
    RetiredAgent,
    NoResumableThread,
    ThreadMismatch {
        notification_thread_id: String,
        runtime_thread_id: String,
    },
    PolicyRefused {
        control: String,
    },
    InvalidPayload {
        reason: String,
    },
}

impl CodexDispatchTerminalError {
    fn code(&self) -> &'static str {
        match self {
            Self::UnknownAgent => "unknown_agent",
            Self::RetiredAgent => "retired_agent",
            Self::NoResumableThread => "no_resumable_thread",
            Self::ThreadMismatch { .. } => "thread_mismatch",
            Self::PolicyRefused { .. } => "policy_refused",
            Self::InvalidPayload { .. } => "invalid_payload",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexDispatchEvidence {
    pub schema_version: u32,
    pub notification_id: String,
    pub transport_attempt_id: Option<String>,
    pub agent_id: String,
    pub delivery_id: String,
    pub message_id: String,
    pub message_thread_id: String,
    pub mailbox: String,
    pub kind: String,
    pub subject: String,
    pub page: Option<MessagePageRef>,
    pub target_thread_id: String,
    pub expected_turn_id: Option<String>,
    pub attempted_control: String,
    pub sandbox_mode: Option<String>,
    pub approval_policy: Option<String>,
    pub terminal_error: Option<CodexDispatchTerminalError>,
}

pub fn decide_codex_notification_dispatch(
    request: CodexDispatchRequest<'_>,
) -> Result<CodexDispatchDecision> {
    validate_payload(request.payload)?;

    let action = match request.runtime_status {
        CodexRuntimeStatus::ActiveTurn { thread_id, turn_id } => {
            if let Some(error) = thread_mismatch(request.payload, &thread_id) {
                CodexDispatchAction::Failed {
                    terminal_error: error,
                }
            } else {
                CodexDispatchAction::SteerActiveTurn {
                    thread_id,
                    expected_turn_id: turn_id,
                }
            }
        }
        CodexRuntimeStatus::LoadedIdle { thread_id } => {
            if let Some(error) = thread_mismatch(request.payload, &thread_id) {
                CodexDispatchAction::Failed {
                    terminal_error: error,
                }
            } else if request.policy.allow_start_loaded_thread {
                CodexDispatchAction::StartLoadedThread { thread_id }
            } else {
                CodexDispatchAction::Failed {
                    terminal_error: CodexDispatchTerminalError::PolicyRefused {
                        control: "turn/start".to_string(),
                    },
                }
            }
        }
        CodexRuntimeStatus::Resumable { thread_id } => {
            if let Some(error) = thread_mismatch(request.payload, &thread_id) {
                CodexDispatchAction::Failed {
                    terminal_error: error,
                }
            } else if request.policy.allow_resume_thread {
                CodexDispatchAction::ResumeThread { thread_id }
            } else {
                CodexDispatchAction::Failed {
                    terminal_error: CodexDispatchTerminalError::PolicyRefused {
                        control: "thread/resume".to_string(),
                    },
                }
            }
        }
        CodexRuntimeStatus::NoResumableThread => CodexDispatchAction::Failed {
            terminal_error: CodexDispatchTerminalError::NoResumableThread,
        },
        CodexRuntimeStatus::UnknownAgent => CodexDispatchAction::Failed {
            terminal_error: CodexDispatchTerminalError::UnknownAgent,
        },
        CodexRuntimeStatus::RetiredAgent => CodexDispatchAction::Failed {
            terminal_error: CodexDispatchTerminalError::RetiredAgent,
        },
    };

    let evidence = build_evidence(request.payload, &action, &request.policy);
    Ok(CodexDispatchDecision {
        schema_version: 1,
        decision_id: stable_decision_id(request.payload, &action),
        action,
        evidence,
    })
}

fn validate_payload(payload: &CodexSteeringPayload) -> Result<()> {
    if payload.transport != "codex.steering" {
        return Err(anyhow!(
            "invalid Codex notification payload transport {}; expected codex.steering",
            payload.transport
        ));
    }
    for (label, value) in [
        ("thread_id", payload.thread_id.as_str()),
        ("agent_id", payload.agent_id.as_str()),
        ("notification_id", payload.notification_id.as_str()),
        ("message_id", payload.message_id.as_str()),
        ("delivery_id", payload.delivery_id.as_str()),
        ("message_thread_id", payload.message_thread_id.as_str()),
        ("mailbox", payload.mailbox.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(anyhow!(
                "invalid Codex notification payload: {label} is empty"
            ));
        }
    }
    Ok(())
}

fn thread_mismatch(
    payload: &CodexSteeringPayload,
    runtime_thread_id: &str,
) -> Option<CodexDispatchTerminalError> {
    if payload.thread_id == runtime_thread_id {
        None
    } else {
        Some(CodexDispatchTerminalError::ThreadMismatch {
            notification_thread_id: payload.thread_id.clone(),
            runtime_thread_id: runtime_thread_id.to_string(),
        })
    }
}

fn build_evidence(
    payload: &CodexSteeringPayload,
    action: &CodexDispatchAction,
    policy: &CodexSupervisorPolicy,
) -> CodexDispatchEvidence {
    let (target_thread_id, expected_turn_id, terminal_error) = match action {
        CodexDispatchAction::SteerActiveTurn {
            thread_id,
            expected_turn_id,
        } => (thread_id.clone(), expected_turn_id.clone(), None),
        CodexDispatchAction::StartLoadedThread { thread_id }
        | CodexDispatchAction::ResumeThread { thread_id } => (thread_id.clone(), None, None),
        CodexDispatchAction::Failed { terminal_error } => (
            payload.thread_id.clone(),
            None,
            Some(terminal_error.clone()),
        ),
    };

    CodexDispatchEvidence {
        schema_version: 1,
        notification_id: payload.notification_id.clone(),
        transport_attempt_id: payload.transport_attempt_id.clone(),
        agent_id: payload.agent_id.clone(),
        delivery_id: payload.delivery_id.clone(),
        message_id: payload.message_id.clone(),
        message_thread_id: payload.message_thread_id.clone(),
        mailbox: payload.mailbox.clone(),
        kind: payload.kind.clone(),
        subject: payload.subject.clone(),
        page: payload.page.clone(),
        target_thread_id,
        expected_turn_id,
        attempted_control: action.control_name().to_string(),
        sandbox_mode: policy.sandbox_mode.clone(),
        approval_policy: policy.approval_policy.clone(),
        terminal_error,
    }
}

fn stable_decision_id(payload: &CodexSteeringPayload, action: &CodexDispatchAction) -> String {
    let mut hash = Sha256::new();
    hash.update(payload.notification_id.as_bytes());
    hash.update(b":");
    hash.update(payload.delivery_id.as_bytes());
    hash.update(b":");
    hash.update(payload.thread_id.as_bytes());
    hash.update(b":");
    hash.update(action.control_name().as_bytes());
    if let CodexDispatchAction::SteerActiveTurn {
        expected_turn_id: Some(turn_id),
        ..
    } = action
    {
        hash.update(b":");
        hash.update(turn_id.as_bytes());
    }
    let digest = hash.finalize();
    format!("codex_dispatch_{}", hex_lower(&digest[..8]))
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecontext_wiki_core::agent_mail::DeliveryState;

    #[test]
    fn active_turn_decision_steers_without_body_or_permission_request() {
        let payload = sample_payload();
        let decision = decide_codex_notification_dispatch(CodexDispatchRequest {
            payload: &payload,
            runtime_status: CodexRuntimeStatus::ActiveTurn {
                thread_id: "thread_codex_001".to_string(),
                turn_id: Some("turn_001".to_string()),
            },
            policy: CodexSupervisorPolicy {
                sandbox_mode: Some("workspace-write".to_string()),
                approval_policy: Some("never".to_string()),
                ..Default::default()
            },
        })
        .unwrap();

        assert_eq!(
            decision.action,
            CodexDispatchAction::SteerActiveTurn {
                thread_id: "thread_codex_001".to_string(),
                expected_turn_id: Some("turn_001".to_string())
            }
        );
        assert!(decision.is_dispatchable());
        assert_eq!(decision.evidence.attempted_control, "turn/steer");
        assert_eq!(
            decision.decision_id,
            decide_codex_notification_dispatch(CodexDispatchRequest {
                payload: &payload,
                runtime_status: CodexRuntimeStatus::ActiveTurn {
                    thread_id: "thread_codex_001".to_string(),
                    turn_id: Some("turn_001".to_string()),
                },
                policy: CodexSupervisorPolicy::default(),
            })
            .unwrap()
            .decision_id
        );

        let serialized = serde_json::to_string(&decision.evidence).unwrap();
        assert!(!serialized.contains("full body secret"));
        assert!(!serialized.contains("request_permissions"));
        assert!(!serialized.contains("danger-full-access"));
    }

    #[test]
    fn inactive_loaded_thread_starts_when_supervisor_policy_allows() {
        let payload = sample_payload();
        let decision = decide_codex_notification_dispatch(CodexDispatchRequest {
            payload: &payload,
            runtime_status: CodexRuntimeStatus::LoadedIdle {
                thread_id: "thread_codex_001".to_string(),
            },
            policy: CodexSupervisorPolicy::default(),
        })
        .unwrap();

        assert_eq!(
            decision.action,
            CodexDispatchAction::StartLoadedThread {
                thread_id: "thread_codex_001".to_string()
            }
        );
        assert_eq!(decision.evidence.attempted_control, "turn/start");
        assert!(decision.evidence.terminal_error.is_none());
    }

    #[test]
    fn inactive_resumable_thread_resumes_when_policy_allows() {
        let payload = sample_payload();
        let decision = decide_codex_notification_dispatch(CodexDispatchRequest {
            payload: &payload,
            runtime_status: CodexRuntimeStatus::Resumable {
                thread_id: "thread_codex_001".to_string(),
            },
            policy: CodexSupervisorPolicy::default(),
        })
        .unwrap();

        assert_eq!(
            decision.action,
            CodexDispatchAction::ResumeThread {
                thread_id: "thread_codex_001".to_string()
            }
        );
        assert_eq!(decision.evidence.attempted_control, "thread/resume");
    }

    #[test]
    fn terminal_states_are_explicit_and_not_dispatchable() {
        for (runtime_status, terminal_error) in [
            (
                CodexRuntimeStatus::NoResumableThread,
                CodexDispatchTerminalError::NoResumableThread,
            ),
            (
                CodexRuntimeStatus::UnknownAgent,
                CodexDispatchTerminalError::UnknownAgent,
            ),
            (
                CodexRuntimeStatus::RetiredAgent,
                CodexDispatchTerminalError::RetiredAgent,
            ),
        ] {
            let payload = sample_payload();
            let decision = decide_codex_notification_dispatch(CodexDispatchRequest {
                payload: &payload,
                runtime_status,
                policy: CodexSupervisorPolicy::default(),
            })
            .unwrap();

            assert_eq!(
                decision.action,
                CodexDispatchAction::Failed {
                    terminal_error: terminal_error.clone()
                }
            );
            assert!(!decision.is_dispatchable());
            assert_eq!(decision.evidence.terminal_error, Some(terminal_error));
        }
    }

    #[test]
    fn policy_refusal_and_thread_mismatch_fail_closed() {
        let payload = sample_payload();
        let refused = decide_codex_notification_dispatch(CodexDispatchRequest {
            payload: &payload,
            runtime_status: CodexRuntimeStatus::Resumable {
                thread_id: "thread_codex_001".to_string(),
            },
            policy: CodexSupervisorPolicy {
                allow_resume_thread: false,
                ..Default::default()
            },
        })
        .unwrap();
        assert_eq!(
            refused.action,
            CodexDispatchAction::Failed {
                terminal_error: CodexDispatchTerminalError::PolicyRefused {
                    control: "thread/resume".to_string()
                }
            }
        );

        let mismatch = decide_codex_notification_dispatch(CodexDispatchRequest {
            payload: &payload,
            runtime_status: CodexRuntimeStatus::ActiveTurn {
                thread_id: "thread_codex_other".to_string(),
                turn_id: Some("turn_other".to_string()),
            },
            policy: CodexSupervisorPolicy::default(),
        })
        .unwrap();
        assert_eq!(
            mismatch.action,
            CodexDispatchAction::Failed {
                terminal_error: CodexDispatchTerminalError::ThreadMismatch {
                    notification_thread_id: "thread_codex_001".to_string(),
                    runtime_thread_id: "thread_codex_other".to_string()
                }
            }
        );
    }

    fn sample_payload() -> CodexSteeringPayload {
        CodexSteeringPayload {
            transport: "codex.steering".to_string(),
            thread_id: "thread_codex_001".to_string(),
            transport_attempt_id: Some("notif_attempt_001".to_string()),
            agent_id: "agent_codex_001".to_string(),
            notification_id: "notif_001".to_string(),
            message_id: "mailmsg_001".to_string(),
            delivery_id: "delivery_001".to_string(),
            message_thread_id: "thread_topics_001".to_string(),
            mailbox: "role://topics.curator".to_string(),
            kind: "proposal".to_string(),
            subject: "Review topics".to_string(),
            page: Some(MessagePageRef {
                id: "topics".to_string(),
                route: "/topics".to_string(),
            }),
            delivery_state: DeliveryState::Unread,
            delivery_updated_at: "2026-05-25T12:00:00Z".to_string(),
            message_created_at: "2026-05-25T12:00:00Z".to_string(),
            instruction: "Open the delivery. full body secret must not be evidence.".to_string(),
        }
    }
}
