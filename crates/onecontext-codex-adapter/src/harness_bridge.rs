use std::path::PathBuf;

use onecontext_agent_harness_core::{
    AgentCallRequest, AgentHarnessInventory, AgentHarnessStore, AgentHarnessUnit, AgentUnitId,
    CapabilityBinding, HarnessError, HarnessLifecycleState, ProofCategory,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

use crate::proof_recorder::{HarnessProofRecordPlan, ProofRecordTarget};
use crate::{CodexAdapterError, CodexAdapterResult};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernedChildAgentRequest {
    pub parent_unit_id: AgentUnitId,
    pub unit_id: Option<AgentUnitId>,
    pub spawn_request_id: String,
    pub role: String,
    pub model: String,
    #[serde(default)]
    pub identity: Value,
    #[serde(default)]
    pub instructions: BTreeMap<String, String>,
    #[serde(default)]
    pub runtime: Value,
    #[serde(default)]
    pub capabilities: Vec<CapabilityBinding>,
    pub visibility: onecontext_agent_harness_core::AgentVisibility,
    #[serde(default)]
    pub metadata: Value,
}

impl GovernedChildAgentRequest {
    pub fn into_call_request(self) -> AgentCallRequest {
        AgentCallRequest {
            unit_id: self.unit_id,
            parent_unit_id: Some(self.parent_unit_id),
            spawn_request_id: Some(self.spawn_request_id),
            role: self.role,
            model: self.model,
            identity: self.identity,
            instructions: self.instructions,
            runtime: self.runtime,
            capabilities: self.capabilities,
            visibility: self.visibility,
            metadata: self.metadata,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GovernedChildSpawnPolicy {
    #[serde(default)]
    pub allowed_parent_unit_ids: Option<BTreeSet<AgentUnitId>>,
    #[serde(default)]
    pub allowed_models: Option<BTreeSet<String>>,
    #[serde(default)]
    pub max_active_children_per_parent: Option<usize>,
    #[serde(default)]
    pub required_metadata_keys: BTreeSet<String>,
    #[serde(default)]
    pub max_metadata_bytes: Option<usize>,
    #[serde(default)]
    pub max_capability_bindings: Option<usize>,
    #[serde(default)]
    pub max_capability_tool_names: Option<usize>,
    #[serde(default)]
    pub allowed_capability_ids: Option<BTreeSet<String>>,
}

impl GovernedChildSpawnPolicy {
    pub fn unrestricted() -> Self {
        Self::default()
    }

    pub fn validate_spawn(
        &self,
        request: &GovernedChildAgentRequest,
        inventory: Option<&AgentHarnessInventory>,
    ) -> CodexAdapterResult<()> {
        if let Some(allowed_parent_unit_ids) = &self.allowed_parent_unit_ids {
            if !allowed_parent_unit_ids.contains(&request.parent_unit_id) {
                return Err(CodexAdapterError::PolicyDenied(format!(
                    "parent unit {} is not allowed to spawn child agents",
                    request.parent_unit_id.0
                )));
            }
        }

        if let Some(allowed_models) = &self.allowed_models {
            if !allowed_models.contains(&request.model) {
                return Err(CodexAdapterError::PolicyDenied(format!(
                    "model {} is not allowed for governed child spawn",
                    request.model
                )));
            }
        }

        if let (Some(max_children), Some(inventory)) =
            (self.max_active_children_per_parent, inventory)
        {
            let active_child_count = inventory
                .active
                .iter()
                .filter(|entry| entry.parent_unit_id.as_ref() == Some(&request.parent_unit_id))
                .count();
            if active_child_count >= max_children {
                return Err(CodexAdapterError::PolicyDenied(format!(
                    "parent unit {} already has {active_child_count} active child agents; policy limit is {max_children}",
                    request.parent_unit_id.0
                )));
            }
        }

        for required_key in &self.required_metadata_keys {
            if request.metadata.get(required_key).is_none() {
                return Err(CodexAdapterError::PolicyDenied(format!(
                    "child spawn metadata missing required key {required_key}"
                )));
            }
        }

        if let Some(max_metadata_bytes) = self.max_metadata_bytes {
            let metadata_bytes = serde_json::to_vec(&request.metadata).map_err(|error| {
                CodexAdapterError::InvalidState(format!(
                    "failed to measure child spawn metadata: {error}"
                ))
            })?;
            if metadata_bytes.len() > max_metadata_bytes {
                return Err(CodexAdapterError::PolicyDenied(format!(
                    "child spawn metadata is {} bytes; policy limit is {max_metadata_bytes}",
                    metadata_bytes.len()
                )));
            }
        }

        if let Some(max_capability_bindings) = self.max_capability_bindings {
            if request.capabilities.len() > max_capability_bindings {
                return Err(CodexAdapterError::PolicyDenied(format!(
                    "child spawn requested {} capability bindings; policy limit is {max_capability_bindings}",
                    request.capabilities.len()
                )));
            }
        }

        if let Some(max_capability_tool_names) = self.max_capability_tool_names {
            let tool_name_count = request
                .capabilities
                .iter()
                .map(|capability| capability.tool_names.len())
                .sum::<usize>();
            if tool_name_count > max_capability_tool_names {
                return Err(CodexAdapterError::PolicyDenied(format!(
                    "child spawn requested {tool_name_count} capability tool names; policy limit is {max_capability_tool_names}"
                )));
            }
        }

        if let Some(allowed_capability_ids) = &self.allowed_capability_ids {
            for capability in &request.capabilities {
                if !allowed_capability_ids.contains(&capability.id) {
                    return Err(CodexAdapterError::PolicyDenied(format!(
                        "capability {} is not allowed for governed child spawn",
                        capability.id
                    )));
                }
            }
        }

        Ok(())
    }

    fn needs_inventory(&self) -> bool {
        self.max_active_children_per_parent.is_some()
    }
}

pub trait HarnessProofSink {
    fn record_proof_plan(
        &self,
        plan: &HarnessProofRecordPlan,
    ) -> CodexAdapterResult<Option<AgentHarnessUnit>>;
}

pub trait HarnessAgentSpawner {
    fn spawn_child(
        &self,
        request: GovernedChildAgentRequest,
    ) -> CodexAdapterResult<AgentHarnessUnit>;
}

#[derive(Clone, Debug)]
pub struct InProcessHarnessBridge {
    store: AgentHarnessStore,
    child_spawn_policy: GovernedChildSpawnPolicy,
}

impl InProcessHarnessBridge {
    pub fn new(onecontext_root: impl Into<PathBuf>) -> Self {
        Self {
            store: AgentHarnessStore::new(onecontext_root),
            child_spawn_policy: GovernedChildSpawnPolicy::unrestricted(),
        }
    }

    pub fn from_store(store: AgentHarnessStore) -> Self {
        Self {
            store,
            child_spawn_policy: GovernedChildSpawnPolicy::unrestricted(),
        }
    }

    pub fn with_child_spawn_policy(mut self, child_spawn_policy: GovernedChildSpawnPolicy) -> Self {
        self.child_spawn_policy = child_spawn_policy;
        self
    }

    pub fn store(&self) -> &AgentHarnessStore {
        &self.store
    }

    pub fn child_spawn_policy(&self) -> &GovernedChildSpawnPolicy {
        &self.child_spawn_policy
    }
}

impl HarnessProofSink for InProcessHarnessBridge {
    fn record_proof_plan(
        &self,
        plan: &HarnessProofRecordPlan,
    ) -> CodexAdapterResult<Option<AgentHarnessUnit>> {
        if plan.target != ProofRecordTarget::AgentHarness {
            return Ok(None);
        }
        let Some(request) = plan.harness_request.clone() else {
            return Err(CodexAdapterError::PolicyDenied(format!(
                "harness proof plan {} was not redacted enough to persist: {:?}",
                plan.kind, plan.policy.reason
            )));
        };
        self.store
            .record_adapter_event(request)
            .map(Some)
            .map_err(adapter_error_from_harness)
    }
}

impl HarnessAgentSpawner for InProcessHarnessBridge {
    fn spawn_child(
        &self,
        request: GovernedChildAgentRequest,
    ) -> CodexAdapterResult<AgentHarnessUnit> {
        let inventory = if self.child_spawn_policy.needs_inventory() {
            Some(self.store.inventory().map_err(adapter_error_from_harness)?)
        } else {
            None
        };
        self.child_spawn_policy
            .validate_spawn(&request, inventory.as_ref())?;
        self.store
            .call(request.into_call_request())
            .map_err(adapter_error_from_harness)
    }
}

fn adapter_error_from_harness(error: HarnessError) -> CodexAdapterError {
    CodexAdapterError::InvalidState(format!("agent harness operation failed: {error}"))
}

pub fn proof_status_has_category(unit: &AgentHarnessUnit, category: ProofCategory) -> bool {
    unit.proof_status().observed.contains(&category)
}

pub fn unit_is_ready_or_born(unit: &AgentHarnessUnit) -> bool {
    matches!(
        unit.lifecycle_state,
        HarnessLifecycleState::Born | HarnessLifecycleState::Ready
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use onecontext_agent_harness_core::{
        AgentVisibility, CapabilityTransport, ProofCategory, ProofGateStatus,
    };
    use serde_json::json;

    use crate::proof_recorder::wake_proof_plan;
    use crate::wake_dispatcher::{CodexWakeAttempt, CodexWakeAttemptResult, CodexWakeStrategy};

    fn parent_call_request(unit_id: &str, proof_required: Vec<String>) -> AgentCallRequest {
        AgentCallRequest {
            unit_id: Some(AgentUnitId(unit_id.to_string())),
            parent_unit_id: None,
            spawn_request_id: None,
            role: "parent agent".to_string(),
            model: "gpt-5-codex".to_string(),
            identity: json!({ "display_name": "parent" }),
            instructions: BTreeMap::new(),
            runtime: json!({ "adapter": "codex_app_server" }),
            capabilities: vec![CapabilityBinding {
                id: "codex-wake".to_string(),
                transport: CapabilityTransport::CodexAppServerDynamicTool,
                tool_names: vec!["turn.steer".to_string()],
                config: json!({}),
                policy: json!({}),
                proof_required,
            }],
            visibility: AgentVisibility::Private,
            metadata: json!({ "title": "Parent" }),
        }
    }

    fn child_spawn_request(
        parent_unit_id: AgentUnitId,
        unit_id: &str,
    ) -> GovernedChildAgentRequest {
        GovernedChildAgentRequest {
            parent_unit_id,
            unit_id: Some(AgentUnitId(unit_id.to_string())),
            spawn_request_id: format!("spawn-{unit_id}"),
            role: "child reviewer".to_string(),
            model: "gpt-5-codex".to_string(),
            identity: json!({ "display_name": "child" }),
            instructions: BTreeMap::new(),
            runtime: json!({ "adapter": "codex_app_server", "room_id": "child-room" }),
            capabilities: Vec::new(),
            visibility: AgentVisibility::Private,
            metadata: json!({ "title": "Child", "purpose": "review" }),
        }
    }

    #[test]
    fn bridge_records_adapter_proof_plan_into_harness_store() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store
            .call(parent_call_request(
                "agent-parent",
                vec!["steering".to_string()],
            ))
            .unwrap();
        let bridge = InProcessHarnessBridge::from_store(store);
        let attempt = CodexWakeAttempt {
            attempt_id: "wake-1".to_string(),
            notification_id: "notification-1".to_string(),
            delivery_id: "delivery-1".to_string(),
            message_id: "message-1".to_string(),
            agent_id: "agent-parent".to_string(),
            thread_id: Some("thread-1".to_string()),
            active_turn_id: Some("turn-1".to_string()),
            strategy: CodexWakeStrategy::SteerActiveTurn,
            result: CodexWakeAttemptResult::Accepted,
            error_code: None,
            created_at: Utc::now(),
        };

        let updated = bridge
            .record_proof_plan(&wake_proof_plan(unit.unit_id.0, &attempt))
            .unwrap()
            .expect("updated unit");

        assert!(proof_status_has_category(&updated, ProofCategory::Steering));
        assert_eq!(
            updated.proof_status().gate_status,
            ProofGateStatus::Satisfied
        );
    }

    #[test]
    fn bridge_spawns_child_as_harness_unit_with_parent_lineage() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let parent = store
            .call(parent_call_request("agent-parent", Vec::new()))
            .unwrap();
        let bridge = InProcessHarnessBridge::from_store(store);

        let child = bridge
            .spawn_child(GovernedChildAgentRequest {
                spawn_request_id: "spawn-child-1".to_string(),
                metadata: json!({ "title": "Child" }),
                ..child_spawn_request(parent.unit_id.clone(), "agent-child")
            })
            .unwrap();

        assert!(unit_is_ready_or_born(&child));
        assert_eq!(
            child.certificate.lineage.parent_unit_id,
            Some(parent.unit_id)
        );
        assert_eq!(child.certificate.lineage.root_unit_id.0, "agent-parent");
        assert_eq!(
            child.certificate.lineage.spawn_request_id.as_deref(),
            Some("spawn-child-1")
        );
    }

    #[test]
    fn governed_child_spawn_policy_default_allows_existing_spawn_shape() {
        let parent_unit_id = AgentUnitId("agent-parent".to_string());
        let request = child_spawn_request(parent_unit_id, "agent-child");

        GovernedChildSpawnPolicy::unrestricted()
            .validate_spawn(&request, None)
            .unwrap();
    }

    #[test]
    fn governed_child_spawn_policy_rejects_disallowed_parent_and_model() {
        let parent_unit_id = AgentUnitId("agent-parent".to_string());
        let mut request = child_spawn_request(parent_unit_id, "agent-child");
        let policy = GovernedChildSpawnPolicy {
            allowed_parent_unit_ids: Some(BTreeSet::from([AgentUnitId(
                "agent-other-parent".to_string(),
            )])),
            ..GovernedChildSpawnPolicy::unrestricted()
        };

        let parent_error = policy.validate_spawn(&request, None).unwrap_err();
        assert!(matches!(
            parent_error,
            CodexAdapterError::PolicyDenied(message)
                if message.contains("not allowed to spawn")
        ));

        request.parent_unit_id = AgentUnitId("agent-other-parent".to_string());
        request.model = "unapproved-model".to_string();
        let policy = GovernedChildSpawnPolicy {
            allowed_parent_unit_ids: Some(BTreeSet::from([request.parent_unit_id.clone()])),
            allowed_models: Some(BTreeSet::from(["gpt-5-codex".to_string()])),
            ..GovernedChildSpawnPolicy::unrestricted()
        };

        let model_error = policy.validate_spawn(&request, None).unwrap_err();
        assert!(matches!(
            model_error,
            CodexAdapterError::PolicyDenied(message)
                if message.contains("model unapproved-model")
        ));
    }

    #[test]
    fn governed_child_spawn_policy_rejects_metadata_and_capability_budget_overflow() {
        let parent_unit_id = AgentUnitId("agent-parent".to_string());
        let mut request = child_spawn_request(parent_unit_id, "agent-child");
        request.metadata = json!({ "title": "Child", "details": "too much" });
        request.capabilities = vec![CapabilityBinding {
            id: "codex-wake".to_string(),
            transport: CapabilityTransport::CodexAppServerDynamicTool,
            tool_names: vec!["turn.steer".to_string(), "thread.inject_items".to_string()],
            config: json!({}),
            policy: json!({}),
            proof_required: Vec::new(),
        }];

        let policy = GovernedChildSpawnPolicy {
            required_metadata_keys: BTreeSet::from(["purpose".to_string()]),
            ..GovernedChildSpawnPolicy::unrestricted()
        };
        let metadata_key_error = policy.validate_spawn(&request, None).unwrap_err();
        assert!(matches!(
            metadata_key_error,
            CodexAdapterError::PolicyDenied(message)
                if message.contains("missing required key purpose")
        ));

        let policy = GovernedChildSpawnPolicy {
            max_metadata_bytes: Some(4),
            ..GovernedChildSpawnPolicy::unrestricted()
        };
        let metadata_size_error = policy.validate_spawn(&request, None).unwrap_err();
        assert!(matches!(
            metadata_size_error,
            CodexAdapterError::PolicyDenied(message)
                if message.contains("metadata is")
        ));

        let policy = GovernedChildSpawnPolicy {
            max_capability_bindings: Some(0),
            ..GovernedChildSpawnPolicy::unrestricted()
        };
        let capability_count_error = policy.validate_spawn(&request, None).unwrap_err();
        assert!(matches!(
            capability_count_error,
            CodexAdapterError::PolicyDenied(message)
                if message.contains("1 capability bindings")
        ));

        let policy = GovernedChildSpawnPolicy {
            max_capability_tool_names: Some(1),
            allowed_capability_ids: Some(BTreeSet::from(["codex-inject".to_string()])),
            ..GovernedChildSpawnPolicy::unrestricted()
        };
        let tool_budget_error = policy.validate_spawn(&request, None).unwrap_err();
        assert!(matches!(
            tool_budget_error,
            CodexAdapterError::PolicyDenied(message)
                if message.contains("2 capability tool names")
        ));

        let policy = GovernedChildSpawnPolicy {
            allowed_capability_ids: Some(BTreeSet::from(["codex-inject".to_string()])),
            ..GovernedChildSpawnPolicy::unrestricted()
        };
        let capability_id_error = policy.validate_spawn(&request, None).unwrap_err();
        assert!(matches!(
            capability_id_error,
            CodexAdapterError::PolicyDenied(message)
                if message.contains("capability codex-wake")
        ));
    }

    #[test]
    fn bridge_rejects_child_spawn_when_parent_active_child_limit_is_reached() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let parent = store
            .call(parent_call_request("agent-parent", Vec::new()))
            .unwrap();
        store
            .call(
                child_spawn_request(parent.unit_id.clone(), "agent-existing-child")
                    .into_call_request(),
            )
            .unwrap();
        let bridge = InProcessHarnessBridge::from_store(store).with_child_spawn_policy(
            GovernedChildSpawnPolicy {
                max_active_children_per_parent: Some(1),
                ..GovernedChildSpawnPolicy::unrestricted()
            },
        );

        let error = bridge
            .spawn_child(child_spawn_request(parent.unit_id, "agent-second-child"))
            .unwrap_err();

        assert!(matches!(
            error,
            CodexAdapterError::PolicyDenied(message)
                if message.contains("already has 1 active child agents")
        ));
    }
}
