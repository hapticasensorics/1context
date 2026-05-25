use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const CODEX_APP_SERVER_TRANSPORT_KIND: &str = "codex-app-server";
pub const ONECONTEXT_MCP_SERVER_NAME: &str = "onecontext";
pub const ALLOWED_VISIBLE_TOOLSETS: [&str; 2] = ["toolset-mail", "toolset-wiki"];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexLoadedState {
    Unknown,
    NotLoaded,
    LoadedIdle,
    LoadedActive,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexTransportProjection {
    pub kind: String,
    pub codex_home: Option<String>,
    pub app_server_endpoint: Option<String>,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub active_turn_id: Option<String>,
    pub loaded_state: CodexLoadedState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLeaseStateProjection {
    Active,
    Stale,
    Retired,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLeaseProjection {
    pub state: AgentLeaseStateProjection,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_heartbeat_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexAdapterPolicyProjection {
    pub can_resume_thread: bool,
    pub can_start_new_thread: bool,
    pub can_install_project_hooks: bool,
    pub require_managed_hooks: bool,
    pub max_wake_attempts_per_delivery: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexToolsetProjection {
    pub mcp_server_name: String,
    pub visible_toolsets: Vec<String>,
    pub hidden_host_tools: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexAdapterBinding {
    pub binding_id: String,
    pub agent_id: String,
    pub agent_unit_id: Option<String>,
    pub primary_address: String,
    pub transport: CodexTransportProjection,
    pub toolsets: CodexToolsetProjection,
    pub lease: AgentLeaseProjection,
    pub policy: CodexAdapterPolicyProjection,
    pub observed_at: DateTime<Utc>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingValidationIssue {
    MissingBindingId,
    MissingAgentId,
    MissingAgentUnitId,
    MissingPrimaryAddress,
    InvalidTransportKind,
    MissingThreadForLoadedState,
    MissingActiveTurnForLoadedActive,
    ActiveTurnWithoutThread,
    MissingMcpServerName,
    MissingVisibleToolset,
    UnknownVisibleToolset,
    EmptyHiddenHostTool,
    ActiveLeaseExpired,
    ZeroWakeAttemptLimit,
    ObservedInFuture,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingValidationReport {
    pub issues: Vec<BindingValidationIssue>,
}

impl BindingValidationReport {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveLeaseState {
    Active,
    Stale,
    Retired,
    Unknown,
    Expired,
}

impl CodexAdapterBinding {
    pub fn effective_lease_state(&self, now: DateTime<Utc>) -> EffectiveLeaseState {
        match self.lease.state {
            AgentLeaseStateProjection::Active => match self.lease.expires_at {
                Some(expires_at) if expires_at <= now => EffectiveLeaseState::Expired,
                _ => EffectiveLeaseState::Active,
            },
            AgentLeaseStateProjection::Stale => EffectiveLeaseState::Stale,
            AgentLeaseStateProjection::Retired => EffectiveLeaseState::Retired,
            AgentLeaseStateProjection::Unknown => EffectiveLeaseState::Unknown,
        }
    }

    pub fn validate(&self, now: DateTime<Utc>) -> BindingValidationReport {
        validate_binding(self, now)
    }

    pub fn has_thread(&self) -> bool {
        has_value(&self.transport.thread_id)
    }

    pub fn has_active_turn(&self) -> bool {
        has_value(&self.transport.active_turn_id)
    }
}

pub fn validate_binding(
    binding: &CodexAdapterBinding,
    now: DateTime<Utc>,
) -> BindingValidationReport {
    let mut issues = Vec::new();

    if binding.binding_id.trim().is_empty() {
        issues.push(BindingValidationIssue::MissingBindingId);
    }
    if binding.agent_id.trim().is_empty() {
        issues.push(BindingValidationIssue::MissingAgentId);
    }
    if matches!(binding.agent_unit_id.as_deref(), Some(unit_id) if unit_id.trim().is_empty()) {
        issues.push(BindingValidationIssue::MissingAgentUnitId);
    }
    if binding.primary_address.trim().is_empty() {
        issues.push(BindingValidationIssue::MissingPrimaryAddress);
    }
    if binding.transport.kind.trim() != CODEX_APP_SERVER_TRANSPORT_KIND {
        issues.push(BindingValidationIssue::InvalidTransportKind);
    }

    let has_thread = binding.has_thread();
    let has_active_turn = binding.has_active_turn();
    match binding.transport.loaded_state {
        CodexLoadedState::LoadedIdle | CodexLoadedState::LoadedActive if !has_thread => {
            issues.push(BindingValidationIssue::MissingThreadForLoadedState);
        }
        _ => {}
    }
    if matches!(
        binding.transport.loaded_state,
        CodexLoadedState::LoadedActive
    ) && !has_active_turn
    {
        issues.push(BindingValidationIssue::MissingActiveTurnForLoadedActive);
    }
    if has_active_turn && !has_thread {
        issues.push(BindingValidationIssue::ActiveTurnWithoutThread);
    }

    if binding.toolsets.mcp_server_name.trim() != ONECONTEXT_MCP_SERVER_NAME {
        issues.push(BindingValidationIssue::MissingMcpServerName);
    }
    if binding
        .toolsets
        .visible_toolsets
        .iter()
        .all(|visible| visible.trim().is_empty())
    {
        issues.push(BindingValidationIssue::MissingVisibleToolset);
    }
    if binding.toolsets.visible_toolsets.iter().any(|visible| {
        !ALLOWED_VISIBLE_TOOLSETS
            .iter()
            .any(|allowed| visible.trim() == *allowed)
    }) {
        issues.push(BindingValidationIssue::UnknownVisibleToolset);
    }
    if binding
        .toolsets
        .hidden_host_tools
        .iter()
        .any(|tool| tool.trim().is_empty())
    {
        issues.push(BindingValidationIssue::EmptyHiddenHostTool);
    }

    if binding.effective_lease_state(now) == EffectiveLeaseState::Expired {
        issues.push(BindingValidationIssue::ActiveLeaseExpired);
    }
    if binding.policy.max_wake_attempts_per_delivery == 0 {
        issues.push(BindingValidationIssue::ZeroWakeAttemptLimit);
    }
    if binding.observed_at > now {
        issues.push(BindingValidationIssue::ObservedInFuture);
    }

    BindingValidationReport { issues }
}

fn has_value(value: &Option<String>) -> bool {
    value
        .as_deref()
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn valid_binding(now: DateTime<Utc>) -> CodexAdapterBinding {
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

    #[test]
    fn validates_complete_binding() {
        let now = Utc::now();
        let report = validate_binding(&valid_binding(now), now);

        assert!(report.is_valid(), "{report:?}");
    }

    #[test]
    fn detects_identity_and_transport_gaps() {
        let now = Utc::now();
        let mut binding = valid_binding(now);
        binding.binding_id = " ".to_string();
        binding.agent_id = String::new();
        binding.agent_unit_id = Some(" ".to_string());
        binding.primary_address = String::new();
        binding.transport.kind = "other".to_string();
        binding.transport.thread_id = None;
        binding.transport.active_turn_id = Some("turn-1".to_string());
        binding.transport.loaded_state = CodexLoadedState::LoadedActive;

        let report = validate_binding(&binding, now);

        assert!(report
            .issues
            .contains(&BindingValidationIssue::MissingBindingId));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::MissingAgentId));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::MissingAgentUnitId));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::MissingPrimaryAddress));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::InvalidTransportKind));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::MissingThreadForLoadedState));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::ActiveTurnWithoutThread));
    }

    #[test]
    fn detects_toolset_lease_policy_and_clock_gaps() {
        let now = Utc::now();
        let mut binding = valid_binding(now);
        binding.toolsets.mcp_server_name = "other".to_string();
        binding.toolsets.visible_toolsets = vec!["toolset-mail".to_string(), "other".to_string()];
        binding.toolsets.hidden_host_tools.push(String::new());
        binding.lease.expires_at = Some(now - Duration::seconds(1));
        binding.policy.max_wake_attempts_per_delivery = 0;
        binding.observed_at = now + Duration::seconds(1);

        let report = validate_binding(&binding, now);

        assert!(report
            .issues
            .contains(&BindingValidationIssue::MissingMcpServerName));
        assert!(!report
            .issues
            .contains(&BindingValidationIssue::MissingVisibleToolset));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::UnknownVisibleToolset));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::EmptyHiddenHostTool));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::ActiveLeaseExpired));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::ZeroWakeAttemptLimit));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::ObservedInFuture));
    }

    #[test]
    fn allows_single_public_toolset_bindings() {
        let now = Utc::now();
        let mut binding = valid_binding(now);
        binding.toolsets.visible_toolsets = vec!["toolset-mail".to_string()];

        let report = validate_binding(&binding, now);

        assert!(report.is_valid(), "{report:?}");
    }

    #[test]
    fn detects_empty_public_toolset_bindings() {
        let now = Utc::now();
        let mut binding = valid_binding(now);
        binding.toolsets.visible_toolsets = vec![" ".to_string()];

        let report = validate_binding(&binding, now);

        assert!(report
            .issues
            .contains(&BindingValidationIssue::MissingVisibleToolset));
        assert!(report
            .issues
            .contains(&BindingValidationIssue::UnknownVisibleToolset));
    }
}
