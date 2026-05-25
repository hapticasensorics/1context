//! Adapter proof gating primitives.
//!
//! Invariant protected here: proof status is a deterministic summary of
//! declared capability requirements and redacted adapter events; it does not
//! execute, store, or interpret the external capability implementation.

use crate::{
    AdapterEvent, AdapterEventKind, AdapterEventStatus, CapabilityBinding, HarnessLifecycleState,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofCategory {
    TransportIdentity,
    Steering,
    ContextInjection,
    Hooks,
    SkillRegistry,
    PluginRegistry,
    ConnectorRegistry,
    AppRegistry,
    ToolConformance,
    DispatchLeaseLiveness,
}

impl ProofCategory {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::TransportIdentity => "transport_identity",
            Self::Steering => "steering",
            Self::ContextInjection => "context_injection",
            Self::Hooks => "hooks",
            Self::SkillRegistry => "skill_registry",
            Self::PluginRegistry => "plugin_registry",
            Self::ConnectorRegistry => "connector_registry",
            Self::AppRegistry => "app_registry",
            Self::ToolConformance => "tool_conformance",
            Self::DispatchLeaseLiveness => "dispatch_lease_liveness",
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "transport_identity" => Some(Self::TransportIdentity),
            "steering" | "wake_steering" => Some(Self::Steering),
            "context_injection" => Some(Self::ContextInjection),
            "hooks" | "hook_conformance" => Some(Self::Hooks),
            "skill_registry" | "codex_skill_registry" | "skill_registry_observed" => {
                Some(Self::SkillRegistry)
            }
            "plugin_registry" | "codex_plugin_registry" | "plugin_registry_observed" => {
                Some(Self::PluginRegistry)
            }
            "connector_registry" | "codex_connector_registry" | "connector_registry_observed" => {
                Some(Self::ConnectorRegistry)
            }
            "app_registry" | "codex_app_registry" | "app_registry_observed" => {
                Some(Self::AppRegistry)
            }
            "tool_conformance" | "tool_allowlist_conformance" => Some(Self::ToolConformance),
            "dispatch_lease_liveness" | "dispatch_and_lease_liveness" => {
                Some(Self::DispatchLeaseLiveness)
            }
            _ => None,
        }
    }

    pub fn all() -> [Self; 10] {
        [
            Self::TransportIdentity,
            Self::Steering,
            Self::ContextInjection,
            Self::Hooks,
            Self::SkillRegistry,
            Self::PluginRegistry,
            Self::ConnectorRegistry,
            Self::AppRegistry,
            Self::ToolConformance,
            Self::DispatchLeaseLiveness,
        ]
    }
}

impl fmt::Display for ProofCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofGateStatus {
    Satisfied,
    Degraded,
    Blocked,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofWarning {
    pub code: String,
    pub message: String,
    pub category: Option<ProofCategory>,
    pub event_id: Option<String>,
    pub binding_id: Option<String>,
    pub tool_name: Option<String>,
}

impl ProofWarning {
    fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        category: Option<ProofCategory>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            category,
            event_id: None,
            binding_id: None,
            tool_name: None,
        }
    }

    fn with_event(mut self, event: &AdapterEvent) -> Self {
        self.event_id = Some(event.id.0.clone());
        self
    }

    fn with_binding(mut self, binding_id: impl Into<String>) -> Self {
        self.binding_id = Some(binding_id.into());
        self
    }

    fn with_tool(mut self, tool_name: Option<String>) -> Self {
        self.tool_name = tool_name;
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequiredProofSummary {
    pub required: Vec<ProofCategory>,
    pub unknown_required: Vec<String>,
    pub warnings: Vec<ProofWarning>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterEvidenceSummary {
    pub observed: Vec<ProofCategory>,
    pub partial: Vec<ProofCategory>,
    pub event_counts: BTreeMap<String, u64>,
    pub warnings: Vec<ProofWarning>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityProofSummary {
    pub binding_id: String,
    pub required: Vec<ProofCategory>,
    pub observed: Vec<ProofCategory>,
    pub missing: Vec<ProofCategory>,
    pub unknown_required: Vec<String>,
    pub warnings: Vec<ProofWarning>,
    pub gate_status: ProofGateStatus,
    pub lifecycle_hint: HarnessLifecycleState,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProofStatusSummary {
    pub required: Vec<ProofCategory>,
    pub observed: Vec<ProofCategory>,
    pub missing: Vec<ProofCategory>,
    pub unknown_required: Vec<String>,
    pub warnings: Vec<ProofWarning>,
    pub capabilities: Vec<CapabilityProofSummary>,
    pub gate_status: ProofGateStatus,
    pub lifecycle_hint: HarnessLifecycleState,
}

pub fn summarize_required_proof(bindings: &[CapabilityBinding]) -> RequiredProofSummary {
    let mut required = BTreeSet::new();
    let mut unknown_required = BTreeSet::new();
    let mut warnings = Vec::new();

    for binding in bindings {
        for raw_category in &binding.proof_required {
            if let Some(category) = ProofCategory::parse(raw_category) {
                required.insert(category);
            } else {
                unknown_required.insert(raw_category.clone());
                warnings.push(
                    ProofWarning::new(
                        "unknown_required_proof",
                        "capability binding requires an unknown proof category",
                        None,
                    )
                    .with_binding(binding.id.clone()),
                );
            }
        }
    }

    RequiredProofSummary {
        required: required.into_iter().collect(),
        unknown_required: unknown_required.into_iter().collect(),
        warnings,
    }
}

pub fn summarize_adapter_evidence(events: &[AdapterEvent]) -> AdapterEvidenceSummary {
    let mut families = AdapterEvidenceFamilies::default();
    let mut event_counts = BTreeMap::new();
    let mut warnings = Vec::new();

    for event in events {
        *event_counts
            .entry(event.kind_name().to_string())
            .or_insert(0) += 1;
        apply_adapter_event(event, &mut families, &mut warnings);
    }

    let observed = ProofCategory::all()
        .into_iter()
        .filter(|category| families.satisfies(category))
        .collect();
    let partial = ProofCategory::all()
        .into_iter()
        .filter(|category| families.has_partial(category) && !families.satisfies(category))
        .collect();

    AdapterEvidenceSummary {
        observed,
        partial,
        event_counts,
        warnings,
    }
}

pub fn summarize_proof_status(
    bindings: &[CapabilityBinding],
    events: &[AdapterEvent],
) -> ProofStatusSummary {
    let required_summary = summarize_required_proof(bindings);
    let evidence_summary = summarize_adapter_evidence(events);

    let required: BTreeSet<_> = required_summary.required.iter().cloned().collect();
    let observed: BTreeSet<_> = evidence_summary.observed.iter().cloned().collect();
    let missing: Vec<_> = required.difference(&observed).cloned().collect();

    let mut warnings = required_summary.warnings;
    warnings.extend(evidence_summary.warnings.clone());

    let capabilities = bindings
        .iter()
        .map(|binding| summarize_capability_proof(binding, &observed, events))
        .collect::<Vec<_>>();

    let gate_status = gate_status(
        missing.is_empty(),
        required_summary.unknown_required.is_empty(),
        &warnings,
    );

    ProofStatusSummary {
        required: required.into_iter().collect(),
        observed: observed.into_iter().collect(),
        missing,
        unknown_required: required_summary.unknown_required,
        warnings,
        capabilities,
        lifecycle_hint: lifecycle_hint(&gate_status),
        gate_status,
    }
}

fn summarize_capability_proof(
    binding: &CapabilityBinding,
    observed: &BTreeSet<ProofCategory>,
    events: &[AdapterEvent],
) -> CapabilityProofSummary {
    let mut required = BTreeSet::new();
    let mut unknown_required = Vec::new();
    let mut warnings = Vec::new();

    for raw_category in &binding.proof_required {
        if let Some(category) = ProofCategory::parse(raw_category) {
            required.insert(category);
        } else {
            unknown_required.push(raw_category.clone());
            warnings.push(
                ProofWarning::new(
                    "unknown_required_proof",
                    "capability binding requires an unknown proof category",
                    None,
                )
                .with_binding(binding.id.clone()),
            );
        }
    }

    let required_vec: Vec<_> = required.iter().cloned().collect();
    let observed_vec: Vec<_> = required.intersection(observed).cloned().collect();
    let missing: Vec<_> = required.difference(observed).cloned().collect();

    for event in events {
        if event.kind == AdapterEventKind::NativeExtraToolObserved {
            warnings.push(
                native_extra_tool_warning(event)
                    .with_binding(binding.id.clone())
                    .with_tool(native_tool_name(event)),
            );
        }
    }

    let gate_status = gate_status(missing.is_empty(), unknown_required.is_empty(), &warnings);

    CapabilityProofSummary {
        binding_id: binding.id.clone(),
        required: required_vec,
        observed: observed_vec,
        missing,
        unknown_required,
        warnings,
        lifecycle_hint: lifecycle_hint(&gate_status),
        gate_status,
    }
}

fn gate_status(
    no_missing_known_requirements: bool,
    no_unknown_requirements: bool,
    warnings: &[ProofWarning],
) -> ProofGateStatus {
    if !no_missing_known_requirements || !no_unknown_requirements {
        ProofGateStatus::Blocked
    } else if warnings.is_empty() {
        ProofGateStatus::Satisfied
    } else {
        ProofGateStatus::Degraded
    }
}

fn lifecycle_hint(status: &ProofGateStatus) -> HarnessLifecycleState {
    match status {
        ProofGateStatus::Satisfied => HarnessLifecycleState::Ready,
        ProofGateStatus::Degraded => HarnessLifecycleState::Waiting,
        ProofGateStatus::Blocked => HarnessLifecycleState::Blocked,
    }
}

#[derive(Default)]
struct AdapterEvidenceFamilies {
    transport_identity_complete: bool,
    transport_identity_partial: bool,
    steering: bool,
    context_injection_requested: bool,
    context_injection_executed: bool,
    hook_registry_observed: bool,
    hook_decision_observed: bool,
    skill_registry_observed: bool,
    plugin_registry_observed: bool,
    connector_registry_observed: bool,
    app_registry_observed: bool,
    tool_allowlist_checked: bool,
    tool_activity_observed: bool,
    dispatch_or_liveness: bool,
}

impl AdapterEvidenceFamilies {
    fn satisfies(&self, category: &ProofCategory) -> bool {
        match category {
            ProofCategory::TransportIdentity => self.transport_identity_complete,
            ProofCategory::Steering => self.steering,
            ProofCategory::ContextInjection => self.context_injection_executed,
            ProofCategory::Hooks => self.hook_registry_observed && self.hook_decision_observed,
            ProofCategory::SkillRegistry => self.skill_registry_observed,
            ProofCategory::PluginRegistry => self.plugin_registry_observed,
            ProofCategory::ConnectorRegistry => self.connector_registry_observed,
            ProofCategory::AppRegistry => self.app_registry_observed,
            ProofCategory::ToolConformance => self.tool_allowlist_checked,
            ProofCategory::DispatchLeaseLiveness => self.dispatch_or_liveness,
        }
    }

    fn has_partial(&self, category: &ProofCategory) -> bool {
        match category {
            ProofCategory::TransportIdentity => self.transport_identity_partial,
            ProofCategory::Steering => self.steering,
            ProofCategory::ContextInjection => {
                self.context_injection_requested || self.context_injection_executed
            }
            ProofCategory::Hooks => self.hook_registry_observed || self.hook_decision_observed,
            ProofCategory::SkillRegistry => self.skill_registry_observed,
            ProofCategory::PluginRegistry => self.plugin_registry_observed,
            ProofCategory::ConnectorRegistry => self.connector_registry_observed,
            ProofCategory::AppRegistry => self.app_registry_observed,
            ProofCategory::ToolConformance => {
                self.tool_allowlist_checked || self.tool_activity_observed
            }
            ProofCategory::DispatchLeaseLiveness => self.dispatch_or_liveness,
        }
    }
}

fn apply_adapter_event(
    event: &AdapterEvent,
    families: &mut AdapterEvidenceFamilies,
    warnings: &mut Vec<ProofWarning>,
) {
    match event.kind {
        AdapterEventKind::TransportIdentityObserved if event.status.is_positive() => {
            families.transport_identity_partial = true;
            if has_complete_transport_identity(event) {
                families.transport_identity_complete = true;
            } else {
                warnings.push(
                    ProofWarning::new(
                        "incomplete_transport_identity",
                        "transport identity proof must cite thread, session, turn, and generated ids",
                        Some(ProofCategory::TransportIdentity),
                    )
                    .with_event(event),
                );
            }
        }
        AdapterEventKind::RuntimeWakeupAttempted
        | AdapterEventKind::RuntimeWakeupAccepted
        | AdapterEventKind::RuntimeWakeupFailed
            if !event.status.is_missing() =>
        {
            families.steering = true;
        }
        AdapterEventKind::SupervisorDispatchSuppressed if !event.status.is_missing() => {
            families.steering = true;
            families.dispatch_or_liveness = true;
        }
        AdapterEventKind::ContextInjectionRequested if event.status.is_positive() => {
            families.context_injection_requested = true;
        }
        AdapterEventKind::ContextInjectionExecuted if event.status.is_positive() => {
            families.context_injection_executed = true;
        }
        AdapterEventKind::HookRegistryObserved if event.status.is_positive() => {
            families.hook_registry_observed = true;
        }
        AdapterEventKind::HookDecisionObserved if event.status.is_positive() => {
            families.hook_decision_observed = true;
        }
        AdapterEventKind::SkillRegistryObserved if event.status.is_positive() => {
            families.skill_registry_observed = true;
        }
        AdapterEventKind::PluginRegistryObserved if event.status.is_positive() => {
            families.plugin_registry_observed = true;
        }
        AdapterEventKind::ConnectorRegistryObserved if event.status.is_positive() => {
            families.connector_registry_observed = true;
        }
        AdapterEventKind::AppRegistryObserved if event.status.is_positive() => {
            families.app_registry_observed = true;
        }
        AdapterEventKind::ToolAllowlistChecked if event.status.is_positive() => {
            families.tool_allowlist_checked = true;
        }
        AdapterEventKind::ToolCallObserved | AdapterEventKind::ToolCallDenied
            if !event.status.is_missing() =>
        {
            families.tool_activity_observed = true;
        }
        AdapterEventKind::NativeExtraToolObserved if !event.status.is_missing() => {
            families.tool_activity_observed = true;
            warnings.push(native_extra_tool_warning(event).with_tool(native_tool_name(event)));
        }
        AdapterEventKind::SupervisorDispatchAttempted
        | AdapterEventKind::AgentLeaseExpired
        | AdapterEventKind::AgentHeartbeatObserved
            if !event.status.is_missing() =>
        {
            families.dispatch_or_liveness = true;
        }
        _ => {}
    }
}

fn native_extra_tool_warning(event: &AdapterEvent) -> ProofWarning {
    ProofWarning::new(
        "native_extra_tool_observed",
        "adapter observed a native extra tool outside the harness capability binding",
        Some(ProofCategory::ToolConformance),
    )
    .with_event(event)
}

fn has_complete_transport_identity(event: &AdapterEvent) -> bool {
    event
        .correlation
        .thread_id
        .as_deref()
        .is_some_and(not_empty)
        && event
            .correlation
            .session_id
            .as_deref()
            .is_some_and(not_empty)
        && event.correlation.turn_id.as_deref().is_some_and(not_empty)
        && generated_ids_present(&event.evidence)
}

fn generated_ids_present(evidence: &Value) -> bool {
    evidence
        .get("generated_ids")
        .and_then(Value::as_object)
        .is_some_and(|ids| !ids.is_empty())
}

fn native_tool_name(event: &AdapterEvent) -> Option<String> {
    ["tool_name", "native_tool_name", "name"]
        .iter()
        .find_map(|key| event.evidence.get(*key).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

fn not_empty(value: &str) -> bool {
    !value.trim().is_empty()
}

trait AdapterEventKindName {
    fn kind_name(&self) -> &'static str;
}

impl AdapterEventKindName for AdapterEvent {
    fn kind_name(&self) -> &'static str {
        match self.kind {
            AdapterEventKind::TransportIdentityObserved => "transport_identity_observed",
            AdapterEventKind::RuntimeWakeupAttempted => "runtime_wakeup_attempted",
            AdapterEventKind::RuntimeWakeupAccepted => "runtime_wakeup_accepted",
            AdapterEventKind::RuntimeWakeupFailed => "runtime_wakeup_failed",
            AdapterEventKind::ContextInjectionRequested => "context_injection_requested",
            AdapterEventKind::ContextInjectionExecuted => "context_injection_executed",
            AdapterEventKind::HookRegistryObserved => "hook_registry_observed",
            AdapterEventKind::HookDecisionObserved => "hook_decision_observed",
            AdapterEventKind::SkillRegistryObserved => "skill_registry_observed",
            AdapterEventKind::PluginRegistryObserved => "plugin_registry_observed",
            AdapterEventKind::ConnectorRegistryObserved => "connector_registry_observed",
            AdapterEventKind::AppRegistryObserved => "app_registry_observed",
            AdapterEventKind::ToolAllowlistChecked => "tool_allowlist_checked",
            AdapterEventKind::ToolCallObserved => "tool_call_observed",
            AdapterEventKind::ToolCallDenied => "tool_call_denied",
            AdapterEventKind::NativeExtraToolObserved => "native_extra_tool_observed",
            AdapterEventKind::SupervisorDispatchAttempted => "supervisor_dispatch_attempted",
            AdapterEventKind::SupervisorDispatchSuppressed => "supervisor_dispatch_suppressed",
            AdapterEventKind::AgentLeaseExpired => "agent_lease_expired",
            AdapterEventKind::AgentHeartbeatObserved => "agent_heartbeat_observed",
        }
    }
}

trait AdapterEventStatusExt {
    fn is_positive(&self) -> bool;
    fn is_missing(&self) -> bool;
}

impl AdapterEventStatusExt for AdapterEventStatus {
    fn is_positive(&self) -> bool {
        matches!(self, Self::Observed | Self::Accepted)
    }

    fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AdapterCorrelation, AdapterEventId, AdapterKind, AgentUnitId};
    use chrono::Utc;
    use serde_json::json;

    fn binding(required: &[&str]) -> CapabilityBinding {
        CapabilityBinding {
            id: "toolset-wiki".to_string(),
            transport: crate::CapabilityTransport::Mcp,
            tool_names: vec!["wiki.search".to_string()],
            config: Value::Null,
            policy: Value::Null,
            proof_required: required.iter().map(|item| (*item).to_string()).collect(),
        }
    }

    fn event(kind: AdapterEventKind, status: AdapterEventStatus) -> AdapterEvent {
        AdapterEvent {
            id: AdapterEventId(format!("event-{kind:?}")),
            at: Utc::now(),
            unit_id: AgentUnitId("unit-1".to_string()),
            adapter: AdapterKind::LocalTest,
            kind,
            status,
            correlation: AdapterCorrelation::default(),
            evidence: Value::Null,
            redaction: crate::AdapterRedaction::default(),
        }
    }

    fn complete_transport_event() -> AdapterEvent {
        let mut event = event(
            AdapterEventKind::TransportIdentityObserved,
            AdapterEventStatus::Observed,
        );
        event.correlation.thread_id = Some("thread-1".to_string());
        event.correlation.session_id = Some("session-1".to_string());
        event.correlation.turn_id = Some("turn-1".to_string());
        event.evidence = json!({
            "generated_ids": {
                "unit_id": "agent-unit-1",
                "certificate_id": "certificate-1"
            }
        });
        event
    }

    #[test]
    fn required_categories_parse_aliases_deterministically() {
        let summary = summarize_required_proof(&[binding(&[
            "wake_steering",
            "tool_allowlist_conformance",
            "codex_skill_registry",
            "plugin_registry_observed",
            "transport_identity",
            "unknown_future_proof",
        ])]);

        assert_eq!(
            summary.required,
            vec![
                ProofCategory::TransportIdentity,
                ProofCategory::Steering,
                ProofCategory::SkillRegistry,
                ProofCategory::PluginRegistry,
                ProofCategory::ToolConformance,
            ]
        );
        assert_eq!(summary.unknown_required, vec!["unknown_future_proof"]);
        assert_eq!(summary.warnings[0].code, "unknown_required_proof");
    }

    #[test]
    fn missing_required_proof_blocks_lifecycle_hint() {
        let summary = summarize_proof_status(
            &[binding(&["transport_identity", "context_injection"])],
            &[complete_transport_event()],
        );

        assert_eq!(summary.gate_status, ProofGateStatus::Blocked);
        assert_eq!(summary.lifecycle_hint, HarnessLifecycleState::Blocked);
        assert_eq!(summary.missing, vec![ProofCategory::ContextInjection]);
    }

    #[test]
    fn executed_context_injection_satisfies_context_proof() {
        let summary = summarize_proof_status(
            &[binding(&["context_injection"])],
            &[
                event(
                    AdapterEventKind::ContextInjectionRequested,
                    AdapterEventStatus::Observed,
                ),
                event(
                    AdapterEventKind::ContextInjectionExecuted,
                    AdapterEventStatus::Accepted,
                ),
            ],
        );

        assert_eq!(summary.gate_status, ProofGateStatus::Satisfied);
        assert_eq!(summary.lifecycle_hint, HarnessLifecycleState::Ready);
        assert_eq!(summary.missing, Vec::<ProofCategory>::new());
    }

    #[test]
    fn hooks_require_registry_and_decision() {
        let partial = summarize_proof_status(
            &[binding(&["hooks"])],
            &[event(
                AdapterEventKind::HookRegistryObserved,
                AdapterEventStatus::Observed,
            )],
        );
        assert_eq!(partial.gate_status, ProofGateStatus::Blocked);
        assert_eq!(partial.missing, vec![ProofCategory::Hooks]);

        let complete = summarize_proof_status(
            &[binding(&["hooks"])],
            &[
                event(
                    AdapterEventKind::HookRegistryObserved,
                    AdapterEventStatus::Observed,
                ),
                event(
                    AdapterEventKind::HookDecisionObserved,
                    AdapterEventStatus::Observed,
                ),
            ],
        );
        assert_eq!(complete.gate_status, ProofGateStatus::Satisfied);
    }

    #[test]
    fn registry_events_satisfy_attachment_proof_categories() {
        let summary = summarize_proof_status(
            &[binding(&[
                "skill_registry",
                "plugin_registry",
                "connector_registry",
                "app_registry",
            ])],
            &[
                event(
                    AdapterEventKind::SkillRegistryObserved,
                    AdapterEventStatus::Observed,
                ),
                event(
                    AdapterEventKind::PluginRegistryObserved,
                    AdapterEventStatus::Observed,
                ),
                event(
                    AdapterEventKind::ConnectorRegistryObserved,
                    AdapterEventStatus::Observed,
                ),
                event(
                    AdapterEventKind::AppRegistryObserved,
                    AdapterEventStatus::Observed,
                ),
            ],
        );

        assert_eq!(summary.gate_status, ProofGateStatus::Satisfied);
        assert_eq!(
            summary.observed,
            vec![
                ProofCategory::SkillRegistry,
                ProofCategory::PluginRegistry,
                ProofCategory::ConnectorRegistry,
                ProofCategory::AppRegistry,
            ]
        );
        assert_eq!(summary.missing, Vec::<ProofCategory>::new());
    }

    #[test]
    fn native_extra_tool_is_warning_not_tool_conformance() {
        let mut native_extra = event(
            AdapterEventKind::NativeExtraToolObserved,
            AdapterEventStatus::Observed,
        );
        native_extra.evidence = json!({ "tool_name": "native.shell" });

        let summary = summarize_proof_status(&[binding(&["tool_conformance"])], &[native_extra]);

        assert_eq!(summary.gate_status, ProofGateStatus::Blocked);
        assert_eq!(summary.missing, vec![ProofCategory::ToolConformance]);
        assert_eq!(summary.warnings[0].code, "native_extra_tool_observed");
        assert_eq!(
            summary.warnings[0].tool_name.as_deref(),
            Some("native.shell")
        );
    }

    #[test]
    fn warnings_degrade_when_required_proof_is_satisfied() {
        let mut native_extra = event(
            AdapterEventKind::NativeExtraToolObserved,
            AdapterEventStatus::Observed,
        );
        native_extra.evidence = json!({ "tool_name": "native.shell" });

        let summary = summarize_proof_status(
            &[binding(&["tool_conformance"])],
            &[
                event(
                    AdapterEventKind::ToolAllowlistChecked,
                    AdapterEventStatus::Accepted,
                ),
                native_extra,
            ],
        );

        assert_eq!(summary.gate_status, ProofGateStatus::Degraded);
        assert_eq!(summary.lifecycle_hint, HarnessLifecycleState::Waiting);
        assert_eq!(summary.missing, Vec::<ProofCategory>::new());
    }

    #[test]
    fn incomplete_transport_identity_is_partial_and_warned() {
        let evidence = summarize_adapter_evidence(&[event(
            AdapterEventKind::TransportIdentityObserved,
            AdapterEventStatus::Observed,
        )]);

        assert_eq!(evidence.observed, Vec::<ProofCategory>::new());
        assert_eq!(evidence.partial, vec![ProofCategory::TransportIdentity]);
        assert_eq!(evidence.warnings[0].code, "incomplete_transport_identity");
    }

    #[test]
    fn suppressed_dispatch_counts_for_steering_and_liveness() {
        let summary = summarize_proof_status(
            &[binding(&["steering", "dispatch_lease_liveness"])],
            &[event(
                AdapterEventKind::SupervisorDispatchSuppressed,
                AdapterEventStatus::Suppressed,
            )],
        );

        assert_eq!(summary.gate_status, ProofGateStatus::Satisfied);
        assert_eq!(
            summary.observed,
            vec![
                ProofCategory::Steering,
                ProofCategory::DispatchLeaseLiveness
            ]
        );
    }
}
