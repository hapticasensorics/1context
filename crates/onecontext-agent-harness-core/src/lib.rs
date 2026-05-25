//! Portable 1Context Agent Harness contract.
//!
//! This crate is the Rust scaffold for the agent-unit authority. It deliberately
//! starts with schemas, path layout, status, and adapter evidence types before
//! implementing mutation semantics.

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub mod proof;
pub mod turn;

pub use proof::{
    summarize_adapter_evidence, summarize_proof_status, summarize_required_proof,
    AdapterEvidenceSummary, CapabilityProofSummary, ProofCategory, ProofGateStatus,
    ProofStatusSummary, ProofWarning, RequiredProofSummary,
};
pub use turn::{
    AgentTurnCompleteRequest, AgentTurnCompletionState, AgentTurnStartRequest, AgentTurnUsage,
};

pub const AGENT_HARNESS_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentUnitId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CertificateId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentCallId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentSessionId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AgentTurnId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AdapterEventId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentVisibility {
    Private,
    Team,
    Public,
    Hidden,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentAvailability {
    Active,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessLifecycleState {
    New,
    Born,
    Ready,
    Running,
    Waiting,
    Blocked,
    Done,
    Error,
    Retired,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityTransport {
    Mcp,
    CodexAppServerDynamicTool,
    CodexSkill,
    CodexPlugin,
    CodexConnector,
    CodexApp,
    HostHook,
    LocalTest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptKind {
    AgentCalled,
    AgentRetired,
    Birth,
    CapabilityBound,
    SessionStarted,
    TurnStarted,
    TurnCompleted,
    UsageUpdated,
    ProofObserved,
    StateChanged,
    Error,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterKind {
    CodexAppServer,
    CodexCli,
    CodexSkill,
    CodexPlugin,
    CodexConnector,
    CodexApp,
    Mcp,
    HostHook,
    LocalTest,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterEventKind {
    TransportIdentityObserved,
    RuntimeWakeupAttempted,
    RuntimeWakeupAccepted,
    RuntimeWakeupFailed,
    ContextInjectionRequested,
    ContextInjectionExecuted,
    HookRegistryObserved,
    HookDecisionObserved,
    SkillRegistryObserved,
    PluginRegistryObserved,
    ConnectorRegistryObserved,
    AppRegistryObserved,
    ToolAllowlistChecked,
    ToolCallObserved,
    ToolCallDenied,
    NativeExtraToolObserved,
    SupervisorDispatchAttempted,
    SupervisorDispatchSuppressed,
    AgentLeaseExpired,
    AgentHeartbeatObserved,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterEventStatus {
    Observed,
    Accepted,
    Failed,
    Blocked,
    Suppressed,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityBinding {
    pub id: String,
    pub transport: CapabilityTransport,
    #[serde(default)]
    pub tool_names: Vec<String>,
    #[serde(default)]
    pub config: Value,
    #[serde(default)]
    pub policy: Value,
    #[serde(default)]
    pub proof_required: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InheritancePolicy {
    pub ambient_config: String,
    pub codex_home: String,
    pub project_docs: String,
    pub tools: String,
}

impl Default for InheritancePolicy {
    fn default() -> Self {
        Self {
            ambient_config: "deny_unless_declared".to_string(),
            codex_home: "isolated".to_string(),
            project_docs: "disabled_unless_declared".to_string(),
            tools: "declared_capabilities_only".to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCallRequest {
    pub unit_id: Option<AgentUnitId>,
    #[serde(default)]
    pub parent_unit_id: Option<AgentUnitId>,
    #[serde(default)]
    pub spawn_request_id: Option<String>,
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
    pub visibility: AgentVisibility,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessBirthCertificate {
    pub kind: String,
    pub version: u32,
    pub certificate_id: CertificateId,
    pub call_id: AgentCallId,
    pub issued_at: DateTime<Utc>,
    pub unit_id: AgentUnitId,
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
    #[serde(default)]
    pub birth_inputs: Value,
    #[serde(default)]
    pub generated_ids: BTreeMap<String, String>,
    #[serde(default)]
    pub inheritance_policy: InheritancePolicy,
    #[serde(default)]
    pub lineage: AgentLineage,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLineage {
    pub parent_unit_id: Option<AgentUnitId>,
    pub root_unit_id: AgentUnitId,
    pub spawn_request_id: Option<String>,
}

impl Default for AgentLineage {
    fn default() -> Self {
        Self {
            parent_unit_id: None,
            root_unit_id: AgentUnitId(String::new()),
            spawn_request_id: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHarnessMetadata {
    pub visibility: AgentVisibility,
    pub availability: AgentAvailability,
    pub title: Option<String>,
    pub owner: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub called_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub retired_at: Option<DateTime<Utc>>,
    pub retirement_reason: Option<String>,
    #[serde(default)]
    pub parent_unit_id: Option<AgentUnitId>,
    #[serde(default)]
    pub spawn_request_id: Option<String>,
    pub turns_started: u64,
    pub turns_completed: u64,
    pub active_turn_id: Option<AgentTurnId>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub total_duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessReceipt {
    pub id: String,
    pub at: DateTime<Utc>,
    pub unit_id: AgentUnitId,
    pub kind: ReceiptKind,
    pub summary: String,
    #[serde(default)]
    pub evidence: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHarnessUnit {
    pub kind: String,
    pub unit_id: AgentUnitId,
    pub certificate: HarnessBirthCertificate,
    pub namespace_path: PathBuf,
    pub lifecycle_state: HarnessLifecycleState,
    pub session_id: AgentSessionId,
    pub metadata: AgentHarnessMetadata,
    #[serde(default)]
    pub receipts: Vec<HarnessReceipt>,
    #[serde(default)]
    pub adapter_events: Vec<AdapterEvent>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHarnessSnapshot {
    pub kind: String,
    pub version: u32,
    pub generated_id_sequence: u64,
    #[serde(default)]
    pub units: BTreeMap<AgentUnitId, AgentHarnessUnit>,
    #[serde(default)]
    pub receipts: Vec<HarnessReceipt>,
    #[serde(default)]
    pub adapter_events: Vec<AdapterEvent>,
}

impl Default for AgentHarnessSnapshot {
    fn default() -> Self {
        Self {
            kind: "onecontext.agent_harness.store".to_string(),
            version: AGENT_HARNESS_SCHEMA_VERSION,
            generated_id_sequence: 0,
            units: BTreeMap::new(),
            receipts: Vec::new(),
            adapter_events: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHarnessInventory {
    #[serde(default)]
    pub active: Vec<AgentHarnessInventoryEntry>,
    #[serde(default)]
    pub retired: Vec<AgentHarnessInventoryEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHarnessInventoryEntry {
    pub unit_id: AgentUnitId,
    pub parent_unit_id: Option<AgentUnitId>,
    pub role: String,
    pub model: String,
    pub lifecycle_state: HarnessLifecycleState,
    pub availability: AgentAvailability,
    pub called_at: DateTime<Utc>,
    pub last_active_at: DateTime<Utc>,
    pub retired_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AdapterCorrelation {
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub expected_turn_id: Option<String>,
    pub notification_id: Option<String>,
    pub transport_attempt_id: Option<String>,
    pub delivery_id: Option<String>,
    pub message_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub hook_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterRedaction {
    pub raw_prompts_redacted: bool,
    pub mail_bodies_redacted: bool,
    pub tool_outputs_redacted: bool,
    pub secrets_redacted: bool,
}

impl Default for AdapterRedaction {
    fn default() -> Self {
        Self {
            raw_prompts_redacted: true,
            mail_bodies_redacted: true,
            tool_outputs_redacted: true,
            secrets_redacted: true,
        }
    }
}

impl AdapterRedaction {
    pub fn is_complete(&self) -> bool {
        self.raw_prompts_redacted
            && self.mail_bodies_redacted
            && self.tool_outputs_redacted
            && self.secrets_redacted
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterEvent {
    pub id: AdapterEventId,
    pub at: DateTime<Utc>,
    pub unit_id: AgentUnitId,
    pub adapter: AdapterKind,
    pub kind: AdapterEventKind,
    pub status: AdapterEventStatus,
    pub correlation: AdapterCorrelation,
    #[serde(default)]
    pub evidence: Value,
    #[serde(default)]
    pub redaction: AdapterRedaction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterEventRequest {
    pub unit_id: AgentUnitId,
    pub adapter: AdapterKind,
    pub kind: AdapterEventKind,
    pub status: AdapterEventStatus,
    #[serde(default)]
    pub correlation: AdapterCorrelation,
    #[serde(default)]
    pub evidence: Value,
    #[serde(default)]
    pub redaction: AdapterRedaction,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentHarnessUnitStatus {
    pub unit: AgentHarnessUnit,
    pub proof_status: ProofStatusSummary,
}

impl AgentHarnessUnit {
    pub fn proof_status(&self) -> ProofStatusSummary {
        summarize_proof_status(&self.certificate.capabilities, &self.adapter_events)
    }

    pub fn status(&self) -> AgentHarnessUnitStatus {
        AgentHarnessUnitStatus {
            unit: self.clone(),
            proof_status: self.proof_status(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHarnessStore {
    paths: AgentHarnessPaths,
}

impl AgentHarnessStore {
    pub fn new(onecontext_root: impl Into<PathBuf>) -> Self {
        Self {
            paths: AgentHarnessPaths::new(onecontext_root),
        }
    }

    pub fn from_paths(paths: AgentHarnessPaths) -> Self {
        Self { paths }
    }

    pub fn paths(&self) -> &AgentHarnessPaths {
        &self.paths
    }

    pub fn load(&self) -> Result<AgentHarnessSnapshot, HarnessError> {
        self.paths.ensure_dirs()?;
        self.load_snapshot_unlocked()
    }

    pub fn call(&self, request: AgentCallRequest) -> Result<AgentHarnessUnit, HarnessError> {
        validate_call_request(&request)?;
        self.paths.ensure_dirs()?;
        let _lock = StoreLock::acquire(&self.paths)?;
        let mut snapshot = self.load_snapshot_unlocked()?;

        if let Some(unit_id) = &request.unit_id {
            validate_id("unit_id", &unit_id.0)?;
            if let Some(existing) = snapshot.units.get(unit_id) {
                let requested_inputs = serde_json::to_value(&request)?;
                if existing.certificate.birth_inputs == requested_inputs {
                    return Ok(existing.clone());
                }
                return Err(HarnessError::UnitAlreadyExists(unit_id.clone()));
            }
        }
        let root_unit_id = root_unit_id_for_call(&snapshot, &request)?;

        let now = Utc::now();
        let requested_inputs = serde_json::to_value(&request)?;
        let parent_unit_id = request.parent_unit_id.clone();
        let spawn_request_id = request.spawn_request_id.clone();
        let generated = allocate_call_ids(&mut snapshot, request.unit_id.clone());
        let root_unit_id = root_unit_id.unwrap_or_else(|| generated.unit_id.clone());
        let generated_ids = generated.generated_ids();
        let metadata = metadata_from_request(&request, now);
        let certificate = HarnessBirthCertificate {
            kind: "onecontext.agent_harness.birth_certificate".to_string(),
            version: AGENT_HARNESS_SCHEMA_VERSION,
            certificate_id: generated.certificate_id.clone(),
            call_id: generated.call_id.clone(),
            issued_at: now,
            unit_id: generated.unit_id.clone(),
            role: request.role,
            model: request.model,
            identity: request.identity,
            instructions: request.instructions,
            runtime: request.runtime,
            capabilities: request.capabilities,
            birth_inputs: requested_inputs,
            generated_ids,
            inheritance_policy: InheritancePolicy::default(),
            lineage: AgentLineage {
                parent_unit_id,
                root_unit_id,
                spawn_request_id,
            },
        };
        let namespace_path = self.paths.units_dir.join(&generated.unit_id.0);
        let unit = AgentHarnessUnit {
            kind: "onecontext.agent_harness.unit".to_string(),
            unit_id: generated.unit_id.clone(),
            certificate: certificate.clone(),
            namespace_path,
            lifecycle_state: HarnessLifecycleState::Born,
            session_id: generated.session_id.clone(),
            metadata,
            receipts: Vec::new(),
            adapter_events: Vec::new(),
        };
        let receipt = HarnessReceipt {
            id: generated.receipt_id,
            at: now,
            unit_id: generated.unit_id.clone(),
            kind: ReceiptKind::AgentCalled,
            summary: format!("agent unit {} born", generated.unit_id.0),
            evidence: json!({
                "certificate": certificate,
                "session_id": generated.session_id,
                "namespace_path": unit.namespace_path,
                "lifecycle_state": unit.lifecycle_state,
                "metadata": unit.metadata,
            }),
        };

        let mut unit_with_receipt = unit;
        unit_with_receipt.receipts.push(receipt.clone());
        snapshot.receipts.push(receipt.clone());
        snapshot
            .units
            .insert(generated.unit_id.clone(), unit_with_receipt.clone());
        snapshot.validate()?;

        fs::create_dir_all(&unit_with_receipt.namespace_path)?;
        write_json_atomic(
            &self.paths.birth_certificates_dir.join(format!(
                "{}.json",
                unit_with_receipt.certificate.certificate_id.0
            )),
            &unit_with_receipt.certificate,
        )?;
        write_receipt_artifact(&self.paths, &receipt)?;
        write_json_atomic(&self.paths.store_path, &snapshot)?;

        Ok(unit_with_receipt)
    }

    pub fn start_turn(
        &self,
        request: AgentTurnStartRequest,
    ) -> Result<AgentHarnessUnit, HarnessError> {
        validate_turn_start_request(&request)?;
        self.paths.ensure_dirs()?;
        let _lock = StoreLock::acquire(&self.paths)?;
        let mut snapshot = self.load_snapshot_unlocked()?;
        let now = Utc::now();
        let turn_id = match request.turn_id {
            Some(turn_id) => turn_id,
            None => allocate_turn_id(&mut snapshot),
        };
        let receipt_id = allocate_receipt_id(&mut snapshot, "turn-started");
        let updated = {
            let unit = snapshot
                .units
                .get_mut(&request.unit_id)
                .ok_or_else(|| HarnessError::UnitNotFound(request.unit_id.clone()))?;
            ensure_unit_accepts_turn_mutation(unit)?;
            if let Some(active_turn_id) = &unit.metadata.active_turn_id {
                return Err(HarnessError::TurnAlreadyActive {
                    unit_id: request.unit_id.clone(),
                    active_turn_id: active_turn_id.clone(),
                });
            }

            unit.lifecycle_state = HarnessLifecycleState::Running;
            unit.metadata.active_turn_id = Some(turn_id.clone());
            unit.metadata.turns_started = unit
                .metadata
                .turns_started
                .checked_add(1)
                .ok_or_else(|| overflow_error("turns_started"))?;
            unit.metadata.last_active_at = now;
            unit.clone()
        };

        let receipt = HarnessReceipt {
            id: receipt_id,
            at: now,
            unit_id: request.unit_id.clone(),
            kind: ReceiptKind::TurnStarted,
            summary: format!(
                "agent unit {} started turn {}",
                request.unit_id.0, turn_id.0
            ),
            evidence: json!({
                "turn_id": turn_id,
                "started_at": now,
                "lifecycle_state": HarnessLifecycleState::Running,
                "active_turn_id": updated.metadata.active_turn_id,
                "turns_started": updated.metadata.turns_started,
                "metadata": request.metadata,
            }),
        };

        let mut updated = updated;
        updated.receipts.push(receipt.clone());
        snapshot
            .units
            .insert(request.unit_id.clone(), updated.clone());
        snapshot.receipts.push(receipt.clone());
        snapshot.validate()?;

        write_receipt_artifact(&self.paths, &receipt)?;
        write_json_atomic(&self.paths.store_path, &snapshot)?;

        Ok(updated)
    }

    pub fn complete_turn(
        &self,
        request: AgentTurnCompleteRequest,
    ) -> Result<AgentHarnessUnit, HarnessError> {
        validate_turn_complete_request(&request)?;
        self.paths.ensure_dirs()?;
        let _lock = StoreLock::acquire(&self.paths)?;
        let mut snapshot = self.load_snapshot_unlocked()?;
        let now = Utc::now();
        let receipt_id = allocate_receipt_id(&mut snapshot, "turn-completed");
        let total_tokens = accounted_total_tokens(&request.usage)?;
        let next_lifecycle_state = request.next_state.lifecycle_state();
        let updated =
            {
                let unit = snapshot
                    .units
                    .get_mut(&request.unit_id)
                    .ok_or_else(|| HarnessError::UnitNotFound(request.unit_id.clone()))?;
                ensure_unit_accepts_turn_mutation(unit)?;
                let active_turn_id = unit.metadata.active_turn_id.clone().ok_or_else(|| {
                    HarnessError::NoActiveTurn {
                        unit_id: request.unit_id.clone(),
                    }
                })?;
                if active_turn_id != request.turn_id {
                    return Err(HarnessError::TurnMismatch {
                        unit_id: request.unit_id.clone(),
                        active_turn_id,
                        requested_turn_id: request.turn_id.clone(),
                    });
                }

                unit.lifecycle_state = next_lifecycle_state.clone();
                unit.metadata.active_turn_id = None;
                unit.metadata.turns_completed = unit
                    .metadata
                    .turns_completed
                    .checked_add(1)
                    .ok_or_else(|| overflow_error("turns_completed"))?;
                add_usage_to_metadata(&mut unit.metadata, &request.usage, total_tokens)?;
                unit.metadata.last_active_at = now;
                unit.clone()
            };

        let receipt = HarnessReceipt {
            id: receipt_id,
            at: now,
            unit_id: request.unit_id.clone(),
            kind: ReceiptKind::TurnCompleted,
            summary: format!(
                "agent unit {} completed turn {}",
                request.unit_id.0, request.turn_id.0
            ),
            evidence: json!({
                "turn_id": request.turn_id,
                "completed_at": now,
                "usage": request.usage,
                "accounted_total_tokens": total_tokens,
                "next_state": request.next_state,
                "lifecycle_state": next_lifecycle_state,
                "active_turn_id": Value::Null,
                "turns_completed": updated.metadata.turns_completed,
                "metadata": request.metadata,
            }),
        };

        let mut updated = updated;
        updated.receipts.push(receipt.clone());
        snapshot
            .units
            .insert(request.unit_id.clone(), updated.clone());
        snapshot.receipts.push(receipt.clone());
        snapshot.validate()?;

        write_receipt_artifact(&self.paths, &receipt)?;
        write_json_atomic(&self.paths.store_path, &snapshot)?;

        Ok(updated)
    }

    pub fn retire(
        &self,
        unit_id: &AgentUnitId,
        reason: impl Into<String>,
    ) -> Result<AgentHarnessUnit, HarnessError> {
        validate_id("unit_id", &unit_id.0)?;
        self.paths.ensure_dirs()?;
        let _lock = StoreLock::acquire(&self.paths)?;
        let mut snapshot = self.load_snapshot_unlocked()?;
        let now = Utc::now();
        let reason = reason.into();
        let receipt_id = allocate_receipt_id(&mut snapshot, "retired");
        let updated = {
            let unit = snapshot
                .units
                .get_mut(unit_id)
                .ok_or_else(|| HarnessError::UnitNotFound(unit_id.clone()))?;

            if unit.metadata.availability == AgentAvailability::Retired {
                return Ok(unit.clone());
            }

            unit.lifecycle_state = HarnessLifecycleState::Retired;
            unit.metadata.availability = AgentAvailability::Retired;
            unit.metadata.active_turn_id = None;
            unit.metadata.retired_at = Some(now);
            unit.metadata.retirement_reason = Some(reason.clone());
            unit.metadata.last_active_at = now;
            unit.clone()
        };

        let receipt = HarnessReceipt {
            id: receipt_id,
            at: now,
            unit_id: unit_id.clone(),
            kind: ReceiptKind::AgentRetired,
            summary: format!("agent unit {} retired", unit_id.0),
            evidence: json!({
                "retired_at": now,
                "retirement_reason": reason,
                "lifecycle_state": HarnessLifecycleState::Retired,
                "availability": AgentAvailability::Retired,
                "active_turn_id": Value::Null,
            }),
        };

        let mut updated = updated;
        updated.receipts.push(receipt.clone());
        snapshot.units.insert(unit_id.clone(), updated.clone());
        snapshot.receipts.push(receipt.clone());
        snapshot.validate()?;

        write_receipt_artifact(&self.paths, &receipt)?;
        write_json_atomic(&self.paths.store_path, &snapshot)?;

        Ok(updated)
    }

    pub fn record_adapter_event(
        &self,
        request: AdapterEventRequest,
    ) -> Result<AgentHarnessUnit, HarnessError> {
        validate_adapter_event_request(&request)?;
        self.paths.ensure_dirs()?;
        let _lock = StoreLock::acquire(&self.paths)?;
        let mut snapshot = self.load_snapshot_unlocked()?;
        let event_id = allocate_adapter_event_id(&mut snapshot);
        let receipt_id = allocate_receipt_id(&mut snapshot, "proof-observed");
        let event = redact_adapter_event_for_persistence(AdapterEvent {
            id: event_id,
            at: Utc::now(),
            unit_id: request.unit_id,
            adapter: request.adapter,
            kind: request.kind,
            status: request.status,
            correlation: request.correlation,
            evidence: request.evidence,
            redaction: request.redaction,
        });
        validate_adapter_event(&event)?;

        let updated = {
            let unit = snapshot
                .units
                .get_mut(&event.unit_id)
                .ok_or_else(|| HarnessError::UnitNotFound(event.unit_id.clone()))?;
            unit.adapter_events.push(event.clone());
            unit.metadata.last_active_at = event.at;
            unit.clone()
        };

        let receipt = HarnessReceipt {
            id: receipt_id,
            at: event.at,
            unit_id: event.unit_id.clone(),
            kind: ReceiptKind::ProofObserved,
            summary: format!("adapter event {} observed", event.id.0),
            evidence: json!({
                "adapter_event": event,
            }),
        };

        let mut updated = updated;
        updated.receipts.push(receipt.clone());
        snapshot
            .units
            .insert(event.unit_id.clone(), updated.clone());
        snapshot.adapter_events.push(event.clone());
        snapshot.receipts.push(receipt.clone());
        snapshot.validate()?;

        write_adapter_event_artifact(&self.paths, &event)?;
        write_receipt_artifact(&self.paths, &receipt)?;
        write_json_atomic(&self.paths.store_path, &snapshot)?;

        Ok(updated)
    }

    pub fn inventory(&self) -> Result<AgentHarnessInventory, HarnessError> {
        Ok(self.load()?.inventory())
    }

    pub fn proof_status(&self, unit_id: &AgentUnitId) -> Result<ProofStatusSummary, HarnessError> {
        let snapshot = self.load()?;
        snapshot.proof_status(unit_id)
    }

    pub fn agent_status(
        &self,
        unit_id: &AgentUnitId,
    ) -> Result<AgentHarnessUnitStatus, HarnessError> {
        let snapshot = self.load()?;
        snapshot.agent_status(unit_id)
    }

    fn load_snapshot_unlocked(&self) -> Result<AgentHarnessSnapshot, HarnessError> {
        if !self.paths.store_path.exists() {
            return Ok(AgentHarnessSnapshot::default());
        }

        match read_snapshot(&self.paths.store_path) {
            Ok(snapshot) => Ok(snapshot),
            Err(store_error) => {
                let receipts = read_receipt_artifacts(&self.paths)?;
                if receipts.is_empty() {
                    return Err(store_error);
                }
                replay_snapshot_from_receipts(&self.paths, &receipts)
            }
        }
    }
}

pub fn replay_snapshot_from_receipts(
    paths: &AgentHarnessPaths,
    receipts: &[HarnessReceipt],
) -> Result<AgentHarnessSnapshot, HarnessError> {
    let mut snapshot = AgentHarnessSnapshot {
        generated_id_sequence: highest_generated_sequence(receipts),
        units: replay_units_from_receipts(paths, receipts)?,
        receipts: receipts.to_vec(),
        ..AgentHarnessSnapshot::default()
    };
    replay_adapter_events(paths, receipts, &mut snapshot)?;
    normalize_snapshot_legacy_fields(&mut snapshot);
    snapshot.validate()?;
    Ok(snapshot)
}

impl AgentHarnessSnapshot {
    pub fn inventory(&self) -> AgentHarnessInventory {
        let mut active = Vec::new();
        let mut retired = Vec::new();

        for unit in self.units.values() {
            let entry = AgentHarnessInventoryEntry {
                unit_id: unit.unit_id.clone(),
                parent_unit_id: unit.metadata.parent_unit_id.clone(),
                role: unit.certificate.role.clone(),
                model: unit.certificate.model.clone(),
                lifecycle_state: unit.lifecycle_state.clone(),
                availability: unit.metadata.availability.clone(),
                called_at: unit.metadata.called_at,
                last_active_at: unit.metadata.last_active_at,
                retired_at: unit.metadata.retired_at,
            };

            match unit.metadata.availability {
                AgentAvailability::Active => active.push(entry),
                AgentAvailability::Retired => retired.push(entry),
            }
        }

        AgentHarnessInventory { active, retired }
    }

    pub fn proof_status(&self, unit_id: &AgentUnitId) -> Result<ProofStatusSummary, HarnessError> {
        let unit = self
            .units
            .get(unit_id)
            .ok_or_else(|| HarnessError::UnitNotFound(unit_id.clone()))?;
        Ok(unit.proof_status())
    }

    pub fn agent_status(
        &self,
        unit_id: &AgentUnitId,
    ) -> Result<AgentHarnessUnitStatus, HarnessError> {
        let unit = self
            .units
            .get(unit_id)
            .ok_or_else(|| HarnessError::UnitNotFound(unit_id.clone()))?;
        Ok(unit.status())
    }

    pub fn validate(&self) -> Result<(), HarnessError> {
        if self.version != AGENT_HARNESS_SCHEMA_VERSION {
            return Err(HarnessError::InvalidInput(format!(
                "unsupported agent harness store version: {}",
                self.version
            )));
        }

        let mut generated_ids = BTreeSet::new();
        let mut adapter_event_ids = BTreeSet::new();
        let mut adapter_events_by_id = BTreeMap::new();
        let mut receipt_ids = BTreeSet::new();
        for receipt in &self.receipts {
            validate_id("receipt_id", &receipt.id)?;
            if !receipt_ids.insert(receipt.id.clone()) {
                return Err(HarnessError::InvalidInput(format!(
                    "duplicate receipt id: {}",
                    receipt.id
                )));
            }
        }

        for (unit_id, unit) in &self.units {
            validate_id("unit_id", &unit_id.0)?;
            if unit_id != &unit.unit_id || unit_id != &unit.certificate.unit_id {
                return Err(HarnessError::InvalidInput(format!(
                    "unit id mismatch for {}",
                    unit_id.0
                )));
            }
            if unit.certificate.lineage.root_unit_id.0.is_empty() {
                return Err(HarnessError::InvalidInput(format!(
                    "unit {} must have a root lineage id",
                    unit_id.0
                )));
            }
            if let Some(parent_unit_id) = &unit.certificate.lineage.parent_unit_id {
                validate_id("parent_unit_id", &parent_unit_id.0)?;
                if parent_unit_id == unit_id {
                    return Err(HarnessError::InvalidInput(format!(
                        "unit {} must not be its own parent",
                        unit_id.0
                    )));
                }
                if !self.units.contains_key(parent_unit_id) {
                    return Err(HarnessError::InvalidInput(format!(
                        "unit {} references missing parent {}",
                        unit_id.0, parent_unit_id.0
                    )));
                }
            }
            validate_id("root_unit_id", &unit.certificate.lineage.root_unit_id.0)?;
            validate_id("certificate_id", &unit.certificate.certificate_id.0)?;
            validate_id("call_id", &unit.certificate.call_id.0)?;
            validate_id("session_id", &unit.session_id.0)?;
            for generated_id in unit.certificate.generated_ids.values() {
                validate_id("generated_id", generated_id)?;
                if !generated_ids.insert(generated_id.clone()) {
                    return Err(HarnessError::InvalidInput(format!(
                        "duplicate host-generated id: {generated_id}"
                    )));
                }
            }
            if unit.metadata.availability == AgentAvailability::Retired
                && unit.lifecycle_state != HarnessLifecycleState::Retired
            {
                return Err(HarnessError::InvalidInput(format!(
                    "retired unit {} must have retired lifecycle state",
                    unit_id.0
                )));
            }
            if let Some(active_turn_id) = &unit.metadata.active_turn_id {
                validate_id("turn_id", &active_turn_id.0)?;
                if unit.lifecycle_state == HarnessLifecycleState::Retired {
                    return Err(HarnessError::InvalidInput(format!(
                        "retired unit {} must not have an active turn",
                        unit_id.0
                    )));
                }
            }
            if unit.metadata.turns_completed > unit.metadata.turns_started {
                return Err(HarnessError::InvalidInput(format!(
                    "unit {} has more completed turns than started turns",
                    unit_id.0
                )));
            }
            for event in &unit.adapter_events {
                if &event.unit_id != unit_id {
                    return Err(HarnessError::InvalidInput(format!(
                        "adapter event {} stored under mismatched unit {}",
                        event.id.0, unit_id.0
                    )));
                }
            }
        }

        for event in &self.adapter_events {
            validate_adapter_event(event)?;
            if !self.units.contains_key(&event.unit_id) {
                return Err(HarnessError::InvalidInput(format!(
                    "adapter event {} references missing unit {}",
                    event.id.0, event.unit_id.0
                )));
            }
            if !adapter_event_ids.insert(event.id.0.clone()) {
                return Err(HarnessError::InvalidInput(format!(
                    "duplicate adapter event id: {}",
                    event.id.0
                )));
            }
            adapter_events_by_id.insert(event.id.clone(), event.clone());
        }

        for (unit_id, unit) in &self.units {
            let mut unit_adapter_event_ids = BTreeSet::new();
            for event in &unit.adapter_events {
                validate_adapter_event(event)?;
                if !unit_adapter_event_ids.insert(event.id.0.clone()) {
                    return Err(HarnessError::InvalidInput(format!(
                        "duplicate adapter event id on unit {}: {}",
                        unit_id.0, event.id.0
                    )));
                }
                if adapter_events_by_id.get(&event.id) != Some(event) {
                    return Err(HarnessError::InvalidInput(format!(
                        "unit {} adapter event {} missing from snapshot index",
                        unit_id.0, event.id.0
                    )));
                }
            }
        }

        Ok(())
    }
}

fn normalize_snapshot_legacy_fields(snapshot: &mut AgentHarnessSnapshot) {
    let lineage_updates: Vec<_> = snapshot
        .units
        .iter()
        .map(|(unit_id, unit)| {
            let mut lineage = unit.certificate.lineage.clone();
            if lineage.parent_unit_id.is_none() {
                lineage.parent_unit_id = unit.metadata.parent_unit_id.clone();
            }
            if lineage.spawn_request_id.is_none() {
                lineage.spawn_request_id = unit.metadata.spawn_request_id.clone();
            }
            if lineage.root_unit_id.0.is_empty() {
                lineage.root_unit_id =
                    infer_root_unit_id(snapshot, unit_id, lineage.parent_unit_id.as_ref());
            }
            (unit_id.clone(), lineage)
        })
        .collect();

    for (unit_id, lineage) in lineage_updates {
        if let Some(unit) = snapshot.units.get_mut(&unit_id) {
            if unit.metadata.parent_unit_id.is_none() {
                unit.metadata.parent_unit_id = lineage.parent_unit_id.clone();
            }
            if unit.metadata.spawn_request_id.is_none() {
                unit.metadata.spawn_request_id = lineage.spawn_request_id.clone();
            }
            unit.certificate.lineage = lineage;
        }
    }
}

fn infer_root_unit_id(
    snapshot: &AgentHarnessSnapshot,
    unit_id: &AgentUnitId,
    parent_unit_id: Option<&AgentUnitId>,
) -> AgentUnitId {
    let Some(parent_unit_id) = parent_unit_id else {
        return unit_id.clone();
    };
    let mut current = parent_unit_id.clone();
    let mut visited = BTreeSet::new();
    loop {
        if !visited.insert(current.clone()) {
            return parent_unit_id.clone();
        }
        let Some(parent) = snapshot.units.get(&current) else {
            return current;
        };
        if !parent.certificate.lineage.root_unit_id.0.is_empty() {
            return parent.certificate.lineage.root_unit_id.clone();
        }
        if let Some(next_parent) = parent
            .certificate
            .lineage
            .parent_unit_id
            .as_ref()
            .or(parent.metadata.parent_unit_id.as_ref())
        {
            current = next_parent.clone();
        } else {
            return parent.unit_id.clone();
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentHarnessPaths {
    pub onecontext_root: PathBuf,
    pub harness_root: PathBuf,
    pub store_path: PathBuf,
    pub birth_certificates_dir: PathBuf,
    pub units_dir: PathBuf,
}

impl AgentHarnessPaths {
    pub fn new(onecontext_root: impl Into<PathBuf>) -> Self {
        let onecontext_root = onecontext_root.into();
        let harness_root = onecontext_root.join("context-engine/agents/harness");
        Self {
            store_path: harness_root.join("agent-harness.json"),
            birth_certificates_dir: harness_root.join("birth-certificates"),
            units_dir: harness_root.join("units"),
            harness_root,
            onecontext_root,
        }
    }

    pub fn ensure_dirs(&self) -> Result<(), HarnessError> {
        fs::create_dir_all(&self.birth_certificates_dir)?;
        fs::create_dir_all(&self.units_dir)?;
        Ok(())
    }

    pub fn status_payload(&self) -> Value {
        json!({
            "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
            "surface": "agent_harness_status",
            "status": "ok",
            "provider": "onecontext-agent-harness",
            "onecontext_root": self.onecontext_root,
            "harness_root": self.harness_root,
            "store_path": self.store_path,
            "birth_certificates_dir": self.birth_certificates_dir,
            "units_dir": self.units_dir,
            "store_exists": self.store_path.exists(),
            "harness_root_exists": self.harness_root.exists(),
        })
    }
}

#[derive(Debug)]
pub enum HarnessError {
    Io(std::io::Error),
    Json(serde_json::Error),
    CorruptStore {
        path: PathBuf,
        message: String,
    },
    StoreLocked(PathBuf),
    UnitAlreadyExists(AgentUnitId),
    UnitNotFound(AgentUnitId),
    UnitRetired(AgentUnitId),
    TurnAlreadyActive {
        unit_id: AgentUnitId,
        active_turn_id: AgentTurnId,
    },
    NoActiveTurn {
        unit_id: AgentUnitId,
    },
    TurnMismatch {
        unit_id: AgentUnitId,
        active_turn_id: AgentTurnId,
        requested_turn_id: AgentTurnId,
    },
    NotImplemented(&'static str),
    InvalidInput(String),
}

impl fmt::Display for HarnessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Json(error) => write!(formatter, "{error}"),
            Self::CorruptStore { path, message } => {
                write!(
                    formatter,
                    "corrupt agent harness store {}: {message}",
                    path.display()
                )
            }
            Self::StoreLocked(path) => {
                write!(
                    formatter,
                    "agent harness store is locked by {}",
                    path.display()
                )
            }
            Self::UnitAlreadyExists(unit_id) => {
                write!(formatter, "agent unit already exists: {}", unit_id.0)
            }
            Self::UnitNotFound(unit_id) => write!(formatter, "agent unit not found: {}", unit_id.0),
            Self::UnitRetired(unit_id) => {
                write!(formatter, "agent unit is retired: {}", unit_id.0)
            }
            Self::TurnAlreadyActive {
                unit_id,
                active_turn_id,
            } => write!(
                formatter,
                "agent unit {} already has active turn {}",
                unit_id.0, active_turn_id.0
            ),
            Self::NoActiveTurn { unit_id } => {
                write!(formatter, "agent unit {} has no active turn", unit_id.0)
            }
            Self::TurnMismatch {
                unit_id,
                active_turn_id,
                requested_turn_id,
            } => write!(
                formatter,
                "agent unit {} active turn {} does not match requested turn {}",
                unit_id.0, active_turn_id.0, requested_turn_id.0
            ),
            Self::NotImplemented(operation) => {
                write!(
                    formatter,
                    "agent harness scaffold has not implemented {operation}"
                )
            }
            Self::InvalidInput(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for HarnessError {}

impl From<std::io::Error> for HarnessError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for HarnessError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Clone, Debug)]
struct GeneratedCallIds {
    unit_id: AgentUnitId,
    unit_id_was_generated: bool,
    certificate_id: CertificateId,
    call_id: AgentCallId,
    session_id: AgentSessionId,
    receipt_id: String,
}

impl GeneratedCallIds {
    fn generated_ids(&self) -> BTreeMap<String, String> {
        let mut ids = BTreeMap::from([
            ("call_id".to_string(), self.call_id.0.clone()),
            ("certificate_id".to_string(), self.certificate_id.0.clone()),
            ("session_id".to_string(), self.session_id.0.clone()),
            ("receipt_id".to_string(), self.receipt_id.clone()),
        ]);
        if self.unit_id_was_generated {
            ids.insert("unit_id".to_string(), self.unit_id.0.clone());
        }
        ids
    }
}

struct StoreLock {
    path: PathBuf,
}

impl StoreLock {
    fn acquire(paths: &AgentHarnessPaths) -> Result<Self, HarnessError> {
        let path = paths.harness_root.join("agent-harness.lock");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                writeln!(
                    file,
                    "{}",
                    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
                )?;
                file.sync_all()?;
                Ok(Self { path })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Err(HarnessError::StoreLocked(path))
            }
            Err(error) => Err(HarnessError::Io(error)),
        }
    }
}

impl Drop for StoreLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}

fn read_snapshot(path: &Path) -> Result<AgentHarnessSnapshot, HarnessError> {
    let bytes = fs::read(path)?;
    let mut snapshot = serde_json::from_slice::<AgentHarnessSnapshot>(&bytes).map_err(|error| {
        HarnessError::CorruptStore {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    })?;
    normalize_snapshot_legacy_fields(&mut snapshot);
    snapshot.validate()?;
    Ok(snapshot)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<(), HarnessError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = path.with_extension(format!(
        "tmp-{}",
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let bytes = serde_json::to_vec_pretty(value)?;
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)?;
        file.write_all(&bytes)?;
        file.write_all(b"\n")?;
        file.sync_all()?;
    }
    fs::rename(&temp_path, path)?;
    if let Some(parent) = path.parent() {
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

fn write_receipt_artifact(
    paths: &AgentHarnessPaths,
    receipt: &HarnessReceipt,
) -> Result<(), HarnessError> {
    let receipt_path = receipt_artifact_path(paths, receipt);
    write_json_atomic(&receipt_path, receipt)
}

fn write_adapter_event_artifact(
    paths: &AgentHarnessPaths,
    event: &AdapterEvent,
) -> Result<(), HarnessError> {
    let event_path = adapter_event_artifact_path(paths, event);
    write_json_atomic(&event_path, event)
}

fn receipt_artifact_path(paths: &AgentHarnessPaths, receipt: &HarnessReceipt) -> PathBuf {
    paths
        .units_dir
        .join(&receipt.unit_id.0)
        .join("receipts")
        .join(format!("{}.json", receipt.id))
}

fn adapter_event_artifact_path(paths: &AgentHarnessPaths, event: &AdapterEvent) -> PathBuf {
    paths
        .units_dir
        .join(&event.unit_id.0)
        .join("adapter-events")
        .join(format!("{}.json", event.id.0))
}

fn read_receipt_artifacts(paths: &AgentHarnessPaths) -> Result<Vec<HarnessReceipt>, HarnessError> {
    let mut receipt_paths = Vec::new();
    if !paths.units_dir.exists() {
        return Ok(Vec::new());
    }

    for unit_entry in fs::read_dir(&paths.units_dir)? {
        let unit_entry = unit_entry?;
        if !unit_entry.file_type()?.is_dir() {
            continue;
        }
        let receipts_dir = unit_entry.path().join("receipts");
        if !receipts_dir.exists() {
            continue;
        }
        for receipt_entry in fs::read_dir(receipts_dir)? {
            let receipt_entry = receipt_entry?;
            if receipt_entry.file_type()?.is_file()
                && receipt_entry
                    .path()
                    .extension()
                    .and_then(|ext| ext.to_str())
                    == Some("json")
            {
                receipt_paths.push(receipt_entry.path());
            }
        }
    }

    receipt_paths.sort();
    let mut receipts = Vec::new();
    for path in receipt_paths {
        let bytes = fs::read(&path)?;
        let receipt = serde_json::from_slice::<HarnessReceipt>(&bytes).map_err(|error| {
            HarnessError::CorruptStore {
                path: path.clone(),
                message: error.to_string(),
            }
        })?;
        receipts.push(receipt);
    }
    receipts.sort_by(|left, right| left.at.cmp(&right.at).then_with(|| left.id.cmp(&right.id)));
    Ok(receipts)
}

fn read_adapter_event_artifacts(
    paths: &AgentHarnessPaths,
) -> Result<Vec<AdapterEvent>, HarnessError> {
    let mut event_paths = Vec::new();
    if !paths.units_dir.exists() {
        return Ok(Vec::new());
    }

    for unit_entry in fs::read_dir(&paths.units_dir)? {
        let unit_entry = unit_entry?;
        if !unit_entry.file_type()?.is_dir() {
            continue;
        }
        let events_dir = unit_entry.path().join("adapter-events");
        if !events_dir.exists() {
            continue;
        }
        for event_entry in fs::read_dir(events_dir)? {
            let event_entry = event_entry?;
            if event_entry.file_type()?.is_file()
                && event_entry.path().extension().and_then(|ext| ext.to_str()) == Some("json")
            {
                event_paths.push(event_entry.path());
            }
        }
    }

    event_paths.sort();
    let mut events = Vec::new();
    for path in event_paths {
        let bytes = fs::read(&path)?;
        let event = serde_json::from_slice::<AdapterEvent>(&bytes).map_err(|error| {
            HarnessError::CorruptStore {
                path: path.clone(),
                message: error.to_string(),
            }
        })?;
        events.push(redact_adapter_event_for_persistence(event));
    }
    events.sort_by(|left, right| left.at.cmp(&right.at).then_with(|| left.id.cmp(&right.id)));
    Ok(events)
}

fn replay_units_from_receipts(
    paths: &AgentHarnessPaths,
    receipts: &[HarnessReceipt],
) -> Result<BTreeMap<AgentUnitId, AgentHarnessUnit>, HarnessError> {
    let mut units = BTreeMap::new();

    for receipt in receipts {
        match receipt.kind {
            ReceiptKind::AgentCalled | ReceiptKind::Birth => {
                if units.contains_key(&receipt.unit_id) {
                    continue;
                }
                let certificate = value_field::<HarnessBirthCertificate>(
                    &receipt.evidence,
                    "certificate",
                    &receipt.id,
                )?;
                let session_id =
                    value_field::<AgentSessionId>(&receipt.evidence, "session_id", &receipt.id)?;
                let metadata = value_field::<AgentHarnessMetadata>(
                    &receipt.evidence,
                    "metadata",
                    &receipt.id,
                )?;
                let lifecycle_state = value_field::<HarnessLifecycleState>(
                    &receipt.evidence,
                    "lifecycle_state",
                    &receipt.id,
                )?;
                let namespace_path = receipt
                    .evidence
                    .get("namespace_path")
                    .cloned()
                    .map(serde_json::from_value::<PathBuf>)
                    .transpose()?
                    .unwrap_or_else(|| paths.units_dir.join(&receipt.unit_id.0));
                let mut unit = AgentHarnessUnit {
                    kind: "onecontext.agent_harness.unit".to_string(),
                    unit_id: receipt.unit_id.clone(),
                    certificate,
                    namespace_path,
                    lifecycle_state,
                    session_id,
                    metadata,
                    receipts: Vec::new(),
                    adapter_events: Vec::new(),
                };
                unit.receipts.push(receipt.clone());
                units.insert(receipt.unit_id.clone(), unit);
            }
            ReceiptKind::AgentRetired => {
                let unit = units
                    .get_mut(&receipt.unit_id)
                    .ok_or_else(|| HarnessError::UnitNotFound(receipt.unit_id.clone()))?;
                unit.lifecycle_state = HarnessLifecycleState::Retired;
                unit.metadata.availability = AgentAvailability::Retired;
                unit.metadata.active_turn_id = None;
                unit.metadata.retired_at = receipt
                    .evidence
                    .get("retired_at")
                    .cloned()
                    .map(serde_json::from_value::<DateTime<Utc>>)
                    .transpose()?
                    .or(Some(receipt.at));
                unit.metadata.retirement_reason = receipt
                    .evidence
                    .get("retirement_reason")
                    .and_then(Value::as_str)
                    .map(ToString::to_string);
                unit.metadata.last_active_at = receipt.at;
                unit.receipts.push(receipt.clone());
            }
            ReceiptKind::TurnStarted => {
                let unit = units
                    .get_mut(&receipt.unit_id)
                    .ok_or_else(|| HarnessError::UnitNotFound(receipt.unit_id.clone()))?;
                replay_turn_started(unit, receipt)?;
                unit.receipts.push(receipt.clone());
            }
            ReceiptKind::TurnCompleted => {
                let unit = units
                    .get_mut(&receipt.unit_id)
                    .ok_or_else(|| HarnessError::UnitNotFound(receipt.unit_id.clone()))?;
                replay_turn_completed(unit, receipt)?;
                unit.receipts.push(receipt.clone());
            }
            _ => {
                if let Some(unit) = units.get_mut(&receipt.unit_id) {
                    unit.receipts.push(receipt.clone());
                }
            }
        }
    }

    Ok(units)
}

fn replay_adapter_events(
    paths: &AgentHarnessPaths,
    receipts: &[HarnessReceipt],
    snapshot: &mut AgentHarnessSnapshot,
) -> Result<(), HarnessError> {
    let mut events_by_id = BTreeMap::new();

    for receipt in receipts {
        if receipt.kind != ReceiptKind::ProofObserved {
            continue;
        }
        if let Some(event_value) = receipt.evidence.get("adapter_event").cloned() {
            let event = redact_adapter_event_for_persistence(serde_json::from_value(event_value)?);
            events_by_id.insert(event.id.clone(), event);
        }
    }

    for event in read_adapter_event_artifacts(paths)? {
        events_by_id.entry(event.id.clone()).or_insert(event);
    }

    for event in events_by_id.into_values() {
        if let Some(unit) = snapshot.units.get_mut(&event.unit_id) {
            if !unit
                .adapter_events
                .iter()
                .any(|existing| existing.id == event.id)
            {
                unit.adapter_events.push(event.clone());
            }
            if unit.metadata.last_active_at < event.at {
                unit.metadata.last_active_at = event.at;
            }
            snapshot.adapter_events.push(event);
        }
    }

    snapshot
        .adapter_events
        .sort_by(|left, right| left.at.cmp(&right.at).then_with(|| left.id.cmp(&right.id)));
    for unit in snapshot.units.values_mut() {
        unit.adapter_events
            .sort_by(|left, right| left.at.cmp(&right.at).then_with(|| left.id.cmp(&right.id)));
    }

    Ok(())
}

fn replay_turn_started(
    unit: &mut AgentHarnessUnit,
    receipt: &HarnessReceipt,
) -> Result<(), HarnessError> {
    ensure_unit_accepts_turn_mutation(unit)?;
    if let Some(active_turn_id) = &unit.metadata.active_turn_id {
        return Err(HarnessError::TurnAlreadyActive {
            unit_id: unit.unit_id.clone(),
            active_turn_id: active_turn_id.clone(),
        });
    }
    let turn_id = value_field::<AgentTurnId>(&receipt.evidence, "turn_id", &receipt.id)?;

    unit.lifecycle_state = HarnessLifecycleState::Running;
    unit.metadata.active_turn_id = Some(turn_id);
    unit.metadata.turns_started = unit
        .metadata
        .turns_started
        .checked_add(1)
        .ok_or_else(|| overflow_error("turns_started"))?;
    unit.metadata.last_active_at = receipt.at;
    Ok(())
}

fn replay_turn_completed(
    unit: &mut AgentHarnessUnit,
    receipt: &HarnessReceipt,
) -> Result<(), HarnessError> {
    ensure_unit_accepts_turn_mutation(unit)?;
    let turn_id = value_field::<AgentTurnId>(&receipt.evidence, "turn_id", &receipt.id)?;
    let active_turn_id =
        unit.metadata
            .active_turn_id
            .clone()
            .ok_or_else(|| HarnessError::NoActiveTurn {
                unit_id: unit.unit_id.clone(),
            })?;
    if active_turn_id != turn_id {
        return Err(HarnessError::TurnMismatch {
            unit_id: unit.unit_id.clone(),
            active_turn_id,
            requested_turn_id: turn_id,
        });
    }
    let usage = value_field::<AgentTurnUsage>(&receipt.evidence, "usage", &receipt.id)?;
    let total_tokens = receipt
        .evidence
        .get("accounted_total_tokens")
        .cloned()
        .map(serde_json::from_value::<u64>)
        .transpose()?
        .unwrap_or(accounted_total_tokens(&usage)?);
    let lifecycle_state = receipt
        .evidence
        .get("lifecycle_state")
        .cloned()
        .map(serde_json::from_value::<HarnessLifecycleState>)
        .transpose()?
        .unwrap_or_else(|| AgentTurnCompletionState::Ready.lifecycle_state());
    if !matches!(
        lifecycle_state,
        HarnessLifecycleState::Ready | HarnessLifecycleState::Waiting | HarnessLifecycleState::Done
    ) {
        return Err(HarnessError::InvalidInput(format!(
            "receipt {} has invalid completed turn lifecycle state",
            receipt.id
        )));
    }

    unit.lifecycle_state = lifecycle_state;
    unit.metadata.active_turn_id = None;
    unit.metadata.turns_completed = unit
        .metadata
        .turns_completed
        .checked_add(1)
        .ok_or_else(|| overflow_error("turns_completed"))?;
    add_usage_to_metadata(&mut unit.metadata, &usage, total_tokens)?;
    unit.metadata.last_active_at = receipt.at;
    Ok(())
}

fn value_field<T: for<'de> Deserialize<'de>>(
    value: &Value,
    field: &str,
    receipt_id: &str,
) -> Result<T, HarnessError> {
    let field_value = value.get(field).cloned().ok_or_else(|| {
        HarnessError::InvalidInput(format!("receipt {receipt_id} missing replay field {field}"))
    })?;
    Ok(serde_json::from_value(field_value)?)
}

fn validate_call_request(request: &AgentCallRequest) -> Result<(), HarnessError> {
    if request.role.trim().is_empty() {
        return Err(HarnessError::InvalidInput(
            "agent call role must not be empty".to_string(),
        ));
    }
    if request.model.trim().is_empty() {
        return Err(HarnessError::InvalidInput(
            "agent call model must not be empty".to_string(),
        ));
    }
    if let Some(unit_id) = &request.unit_id {
        validate_id("unit_id", &unit_id.0)?;
    }
    if let Some(parent_unit_id) = &request.parent_unit_id {
        validate_id("parent_unit_id", &parent_unit_id.0)?;
        if request.unit_id.as_ref() == Some(parent_unit_id) {
            return Err(HarnessError::InvalidInput(
                "agent call parent_unit_id must not equal unit_id".to_string(),
            ));
        }
    }
    if let Some(spawn_request_id) = &request.spawn_request_id {
        validate_id("spawn_request_id", spawn_request_id)?;
    }
    for capability in &request.capabilities {
        validate_id("capability_id", &capability.id)?;
    }
    Ok(())
}

fn root_unit_id_for_call(
    snapshot: &AgentHarnessSnapshot,
    request: &AgentCallRequest,
) -> Result<Option<AgentUnitId>, HarnessError> {
    let Some(parent_unit_id) = &request.parent_unit_id else {
        return Ok(None);
    };
    let parent = snapshot
        .units
        .get(parent_unit_id)
        .ok_or_else(|| HarnessError::UnitNotFound(parent_unit_id.clone()))?;
    if parent.metadata.availability == AgentAvailability::Retired
        || parent.lifecycle_state == HarnessLifecycleState::Retired
    {
        return Err(HarnessError::UnitRetired(parent_unit_id.clone()));
    }
    let root = if parent.certificate.lineage.root_unit_id.0.is_empty() {
        parent.unit_id.clone()
    } else {
        parent.certificate.lineage.root_unit_id.clone()
    };
    Ok(Some(root))
}

fn validate_adapter_event_request(request: &AdapterEventRequest) -> Result<(), HarnessError> {
    validate_id("unit_id", &request.unit_id.0)?;
    validate_adapter_correlation(&request.correlation)?;
    validate_redacted_value(&redact_value_for_persistence(request.evidence.clone()))?;
    Ok(())
}

fn validate_turn_start_request(request: &AgentTurnStartRequest) -> Result<(), HarnessError> {
    validate_id("unit_id", &request.unit_id.0)?;
    if let Some(turn_id) = &request.turn_id {
        validate_id("turn_id", &turn_id.0)?;
    }
    Ok(())
}

fn validate_turn_complete_request(request: &AgentTurnCompleteRequest) -> Result<(), HarnessError> {
    validate_id("unit_id", &request.unit_id.0)?;
    validate_id("turn_id", &request.turn_id.0)?;
    let _ = accounted_total_tokens(&request.usage)?;
    Ok(())
}

fn ensure_unit_accepts_turn_mutation(unit: &AgentHarnessUnit) -> Result<(), HarnessError> {
    if unit.metadata.availability == AgentAvailability::Retired
        || unit.lifecycle_state == HarnessLifecycleState::Retired
    {
        return Err(HarnessError::UnitRetired(unit.unit_id.clone()));
    }
    Ok(())
}

fn accounted_total_tokens(usage: &AgentTurnUsage) -> Result<u64, HarnessError> {
    match usage.total_tokens {
        Some(total_tokens) => Ok(total_tokens),
        None => usage
            .input_tokens
            .checked_add(usage.output_tokens)
            .ok_or_else(|| overflow_error("total_tokens")),
    }
}

fn add_usage_to_metadata(
    metadata: &mut AgentHarnessMetadata,
    usage: &AgentTurnUsage,
    total_tokens: u64,
) -> Result<(), HarnessError> {
    metadata.input_tokens = metadata
        .input_tokens
        .checked_add(usage.input_tokens)
        .ok_or_else(|| overflow_error("input_tokens"))?;
    metadata.output_tokens = metadata
        .output_tokens
        .checked_add(usage.output_tokens)
        .ok_or_else(|| overflow_error("output_tokens"))?;
    metadata.total_tokens = metadata
        .total_tokens
        .checked_add(total_tokens)
        .ok_or_else(|| overflow_error("total_tokens"))?;
    metadata.total_duration_ms = metadata
        .total_duration_ms
        .checked_add(usage.duration_ms)
        .ok_or_else(|| overflow_error("total_duration_ms"))?;
    Ok(())
}

fn overflow_error(field: &str) -> HarnessError {
    HarnessError::InvalidInput(format!("{field} accounting overflow"))
}

fn validate_adapter_event(event: &AdapterEvent) -> Result<(), HarnessError> {
    validate_id("adapter_event_id", &event.id.0)?;
    validate_id("unit_id", &event.unit_id.0)?;
    validate_adapter_correlation(&event.correlation)?;
    if !event.redaction.is_complete() {
        return Err(HarnessError::InvalidInput(format!(
            "adapter event {} must be fully redacted before persistence",
            event.id.0
        )));
    }
    validate_redacted_value(&event.evidence)?;
    Ok(())
}

fn validate_adapter_correlation(correlation: &AdapterCorrelation) -> Result<(), HarnessError> {
    for (label, id) in [
        ("session_id", correlation.session_id.as_deref()),
        ("turn_id", correlation.turn_id.as_deref()),
        ("expected_turn_id", correlation.expected_turn_id.as_deref()),
        ("tool_call_id", correlation.tool_call_id.as_deref()),
    ] {
        if let Some(id) = id {
            validate_id(label, id)?;
        }
    }
    Ok(())
}

fn redact_adapter_event_for_persistence(mut event: AdapterEvent) -> AdapterEvent {
    event.evidence = redact_value_for_persistence(event.evidence);
    event.redaction = AdapterRedaction::default();
    event
}

fn redact_value_for_persistence(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(
            values
                .into_iter()
                .map(redact_value_for_persistence)
                .collect(),
        ),
        Value::Object(object) => Value::Object(
            object
                .into_iter()
                .map(|(key, value)| {
                    let value = if is_sensitive_evidence_key(&key) {
                        Value::String("[redacted]".to_string())
                    } else {
                        redact_value_for_persistence(value)
                    };
                    (key, value)
                })
                .collect(),
        ),
        other => other,
    }
}

fn validate_redacted_value(value: &Value) -> Result<(), HarnessError> {
    validate_redacted_value_at_key(None, value)
}

fn validate_redacted_value_at_key(key: Option<&str>, value: &Value) -> Result<(), HarnessError> {
    if key.is_some_and(is_sensitive_evidence_key) && !is_redacted_marker(value) {
        return Err(HarnessError::InvalidInput(
            "adapter evidence contains unredacted sensitive body data".to_string(),
        ));
    }
    match value {
        Value::Array(values) => {
            for value in values {
                validate_redacted_value_at_key(None, value)?;
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                validate_redacted_value_at_key(Some(key), value)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn is_redacted_marker(value: &Value) -> bool {
    matches!(value, Value::Null)
        || value
            .as_str()
            .is_some_and(|value| value == "[redacted]" || value == "<redacted>")
}

fn is_sensitive_evidence_key(key: &str) -> bool {
    let normalized = key.replace(['-', ' '], "_").to_ascii_lowercase();
    normalized == "message"
        || normalized == "messages"
        || normalized == "body"
        || normalized.ends_with("_body")
        || normalized == "content"
        || normalized.ends_with("_content")
        || normalized == "text"
        || normalized.ends_with("_text")
        || normalized == "raw"
        || normalized.starts_with("raw_")
        || normalized.contains("prompt")
        || normalized.contains("secret")
        || normalized.contains("password")
        || normalized == "token"
        || normalized.ends_with("_token")
        || normalized == "output"
        || normalized.ends_with("_output")
}

fn validate_id(label: &str, id: &str) -> Result<(), HarnessError> {
    let valid = !id.is_empty()
        && id.len() <= 128
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    if valid {
        Ok(())
    } else {
        Err(HarnessError::InvalidInput(format!(
            "{label} must be a non-empty path-safe id"
        )))
    }
}

fn metadata_from_request(request: &AgentCallRequest, now: DateTime<Utc>) -> AgentHarnessMetadata {
    AgentHarnessMetadata {
        visibility: request.visibility.clone(),
        availability: AgentAvailability::Active,
        title: request
            .metadata
            .get("title")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        owner: request
            .metadata
            .get("owner")
            .and_then(Value::as_str)
            .map(ToString::to_string),
        tags: request
            .metadata
            .get("tags")
            .and_then(Value::as_array)
            .map(|tags| {
                tags.iter()
                    .filter_map(Value::as_str)
                    .map(ToString::to_string)
                    .collect()
            })
            .unwrap_or_default(),
        called_at: now,
        last_active_at: now,
        retired_at: None,
        retirement_reason: None,
        parent_unit_id: request.parent_unit_id.clone(),
        spawn_request_id: request.spawn_request_id.clone(),
        turns_started: 0,
        turns_completed: 0,
        active_turn_id: None,
        input_tokens: 0,
        output_tokens: 0,
        total_tokens: 0,
        total_duration_ms: 0,
    }
}

fn allocate_call_ids(
    snapshot: &mut AgentHarnessSnapshot,
    requested_unit_id: Option<AgentUnitId>,
) -> GeneratedCallIds {
    let existing_generated_ids = snapshot
        .units
        .values()
        .flat_map(|unit| unit.certificate.generated_ids.values().cloned())
        .collect::<BTreeSet<_>>();
    let mut sequence = snapshot.generated_id_sequence;

    loop {
        sequence += 1;
        let unit_id_was_generated = requested_unit_id.is_none();
        let unit_id = requested_unit_id
            .clone()
            .unwrap_or_else(|| AgentUnitId(format!("agent-unit-{sequence:012}")));
        let ids = GeneratedCallIds {
            unit_id: unit_id.clone(),
            unit_id_was_generated,
            certificate_id: CertificateId(format!("agent-cert-{sequence:012}")),
            call_id: AgentCallId(format!("agent-call-{sequence:012}")),
            session_id: AgentSessionId(format!("agent-session-{sequence:012}")),
            receipt_id: format!("agent-receipt-{sequence:012}-called"),
        };
        let generated_values = ids.generated_ids();
        let generated_collision = generated_values
            .values()
            .any(|value| existing_generated_ids.contains(value));
        if !generated_collision && !snapshot.units.contains_key(&unit_id) {
            snapshot.generated_id_sequence = sequence;
            return ids;
        }
    }
}

fn allocate_receipt_id(snapshot: &mut AgentHarnessSnapshot, suffix: &str) -> String {
    let existing_receipts = snapshot
        .receipts
        .iter()
        .map(|receipt| receipt.id.clone())
        .collect::<BTreeSet<_>>();
    loop {
        snapshot.generated_id_sequence += 1;
        let receipt_id = format!(
            "agent-receipt-{:012}-{suffix}",
            snapshot.generated_id_sequence
        );
        if !existing_receipts.contains(&receipt_id) {
            return receipt_id;
        }
    }
}

fn allocate_adapter_event_id(snapshot: &mut AgentHarnessSnapshot) -> AdapterEventId {
    let existing_event_ids = snapshot
        .adapter_events
        .iter()
        .map(|event| event.id.clone())
        .collect::<BTreeSet<_>>();
    loop {
        snapshot.generated_id_sequence += 1;
        let event_id = AdapterEventId(format!(
            "agent-adapter-event-{:012}",
            snapshot.generated_id_sequence
        ));
        if !existing_event_ids.contains(&event_id) {
            return event_id;
        }
    }
}

fn allocate_turn_id(snapshot: &mut AgentHarnessSnapshot) -> AgentTurnId {
    let existing_turn_ids = snapshot
        .receipts
        .iter()
        .filter_map(receipt_turn_id)
        .collect::<BTreeSet<_>>();
    loop {
        snapshot.generated_id_sequence += 1;
        let turn_id = AgentTurnId(format!("agent-turn-{:012}", snapshot.generated_id_sequence));
        if !existing_turn_ids.contains(&turn_id) {
            return turn_id;
        }
    }
}

fn receipt_turn_id(receipt: &HarnessReceipt) -> Option<AgentTurnId> {
    receipt
        .evidence
        .get("turn_id")
        .cloned()
        .and_then(|value| serde_json::from_value::<AgentTurnId>(value).ok())
}

fn highest_generated_sequence(receipts: &[HarnessReceipt]) -> u64 {
    receipts
        .iter()
        .filter_map(|receipt| {
            receipt
                .id
                .strip_prefix("agent-receipt-")
                .and_then(|rest| rest.get(..12))
                .and_then(|digits| digits.parse::<u64>().ok())
        })
        .max()
        .unwrap_or_default()
}

pub fn utc_now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true)
}

pub fn describe_agent_harness_contract() -> Value {
    json!({
        "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
        "surface": "agent_harness_contract",
        "status": "scaffold",
        "owned_by_harness": [
            "birth_certificate",
            "host_generated_ids",
            "durable_unit_namespace",
            "lifecycle_state",
            "turn_ledger",
            "capability_declarations",
            "required_proof_status",
            "visibility_accounting_metadata",
            "active_agent_inventory",
            "retirement_authority",
            "usage_accounting",
            "append_only_receipts",
            "adapter_transport_evidence"
        ],
        "external_product_layers": [
            "toolset-mail",
            "toolset-wiki",
            "memory-db",
            "mcp-server-implementations",
            "codex-skills",
            "codex-plugins",
            "codex-connectors",
            "codex-apps",
            "codex-app-server-transport"
        ],
        "adapter_event_families": [
            "transport_identity",
            "wake_steering",
            "context_injection",
            "hooks",
            "skill_registry",
            "plugin_registry",
            "connector_registry",
            "app_registry",
            "tool_allowlist_conformance",
            "dispatch_lease_liveness"
        ]
    })
}

pub fn onecontext_root_from_path(path: impl AsRef<Path>) -> PathBuf {
    path.as_ref().to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call_request(unit_id: Option<&str>) -> AgentCallRequest {
        AgentCallRequest {
            unit_id: unit_id.map(|id| AgentUnitId(id.to_string())),
            parent_unit_id: None,
            spawn_request_id: None,
            role: "researcher".to_string(),
            model: "gpt-test".to_string(),
            identity: json!({"name": "lane-one"}),
            instructions: BTreeMap::from([(
                "system".to_string(),
                "Keep the harness boundary small.".to_string(),
            )]),
            runtime: json!({"adapter": "local-test"}),
            capabilities: vec![CapabilityBinding {
                id: "toolset-local".to_string(),
                transport: CapabilityTransport::LocalTest,
                tool_names: vec!["echo".to_string()],
                config: json!({"source": "test"}),
                policy: json!({}),
                proof_required: vec!["transport_identity".to_string()],
            }],
            visibility: AgentVisibility::Private,
            metadata: json!({
                "title": "Lane 1",
                "owner": "tests",
                "tags": ["core", "store"]
            }),
        }
    }

    fn complete_transport_event_request(unit: &AgentHarnessUnit) -> AdapterEventRequest {
        AdapterEventRequest {
            unit_id: unit.unit_id.clone(),
            adapter: AdapterKind::LocalTest,
            kind: AdapterEventKind::TransportIdentityObserved,
            status: AdapterEventStatus::Observed,
            correlation: AdapterCorrelation {
                thread_id: Some("thread-1".to_string()),
                session_id: Some(unit.session_id.0.clone()),
                turn_id: Some("turn-1".to_string()),
                ..AdapterCorrelation::default()
            },
            evidence: json!({
                "generated_ids": unit.certificate.generated_ids.clone(),
                "transport": "local-test"
            }),
            redaction: AdapterRedaction::default(),
        }
    }

    fn remove_snapshot_certificate_lineage(snapshot: &mut Value, unit_id: &str) {
        snapshot["units"][unit_id]["certificate"]
            .as_object_mut()
            .expect("certificate object")
            .remove("lineage");
    }

    fn remove_receipt_certificate_lineage(path: &Path) {
        let mut receipt =
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).expect("receipt json");
        receipt["evidence"]["certificate"]
            .as_object_mut()
            .expect("receipt certificate object")
            .remove("lineage");
        fs::write(path, serde_json::to_vec_pretty(&receipt).unwrap()).unwrap();
    }

    #[test]
    fn paths_match_public_runtime_contract() {
        let paths = AgentHarnessPaths::new("/tmp/1Context");

        assert!(paths
            .harness_root
            .ends_with("context-engine/agents/harness"));
        assert!(paths.store_path.ends_with("agent-harness.json"));
        assert!(paths.birth_certificates_dir.ends_with("birth-certificates"));
        assert!(paths.units_dir.ends_with("units"));
    }

    #[test]
    fn adapter_events_redact_sensitive_surfaces_by_default() {
        assert_eq!(
            AdapterRedaction::default(),
            AdapterRedaction {
                raw_prompts_redacted: true,
                mail_bodies_redacted: true,
                tool_outputs_redacted: true,
                secrets_redacted: true,
            }
        );
    }

    #[test]
    fn describe_contract_names_adapter_evidence() {
        let payload = describe_agent_harness_contract();
        let families = payload["adapter_event_families"].as_array().unwrap();

        assert!(families.iter().any(|item| item == "context_injection"));
        assert!(families
            .iter()
            .any(|item| item == "tool_allowlist_conformance"));
        assert!(families
            .iter()
            .any(|item| item == "dispatch_lease_liveness"));
        assert!(families.iter().any(|item| item == "skill_registry"));
        assert!(families.iter().any(|item| item == "plugin_registry"));
        assert!(families.iter().any(|item| item == "connector_registry"));
        assert!(families.iter().any(|item| item == "app_registry"));
    }

    #[test]
    fn call_creates_birth_certificate_receipt_and_active_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());

        let unit = store.call(call_request(None)).unwrap();

        assert_eq!(unit.lifecycle_state, HarnessLifecycleState::Born);
        assert_eq!(unit.metadata.availability, AgentAvailability::Active);
        assert_eq!(unit.receipts.len(), 1);
        assert_eq!(unit.receipts[0].kind, ReceiptKind::AgentCalled);
        assert!(unit.namespace_path.exists());
        assert!(store
            .paths()
            .birth_certificates_dir
            .join(format!("{}.json", unit.certificate.certificate_id.0))
            .exists());
        assert!(receipt_artifact_path(store.paths(), &unit.receipts[0]).exists());

        let inventory = store.inventory().unwrap();
        assert_eq!(inventory.active.len(), 1);
        assert_eq!(inventory.retired.len(), 0);
        assert_eq!(inventory.active[0].unit_id, unit.unit_id);
    }

    #[test]
    fn record_adapter_event_persists_unit_snapshot_artifact_and_proof_status() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store.call(call_request(Some("agent-proof"))).unwrap();

        let updated = store
            .record_adapter_event(complete_transport_event_request(&unit))
            .unwrap();

        assert_eq!(updated.adapter_events.len(), 1);
        assert_eq!(
            updated.receipts.last().unwrap().kind,
            ReceiptKind::ProofObserved
        );
        assert!(adapter_event_artifact_path(store.paths(), &updated.adapter_events[0]).exists());

        let snapshot = store.load().unwrap();
        assert_eq!(snapshot.adapter_events.len(), 1);
        assert_eq!(
            snapshot.units[&unit.unit_id].adapter_events,
            updated.adapter_events
        );

        let status = store.agent_status(&unit.unit_id).unwrap();
        assert_eq!(status.unit.adapter_events.len(), 1);
        assert_eq!(status.proof_status.gate_status, ProofGateStatus::Satisfied);
        assert_eq!(
            status.proof_status.lifecycle_hint,
            HarnessLifecycleState::Ready
        );
    }

    #[test]
    fn persisted_adapter_events_redact_body_like_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store.call(call_request(Some("agent-redaction"))).unwrap();
        let mut request = complete_transport_event_request(&unit);
        request.evidence["mail_body"] = json!("secret mail body");
        request.evidence["messages"] = json!(["raw prompt text"]);
        request.evidence["nested"] = json!({
            "wiki_content": "secret wiki body",
            "memory_text": "secret memory body",
            "tool_output": "secret tool output",
            "api_token": "secret token"
        });

        let updated = store.record_adapter_event(request).unwrap();
        let event = &updated.adapter_events[0];
        let event_path = adapter_event_artifact_path(store.paths(), event);
        let event_artifact = fs::read_to_string(event_path).unwrap();
        let receipt_artifact = fs::read_to_string(receipt_artifact_path(
            store.paths(),
            updated.receipts.last().unwrap(),
        ))
        .unwrap();

        assert_eq!(event.evidence["mail_body"], "[redacted]");
        assert_eq!(event.evidence["messages"], "[redacted]");
        assert_eq!(event.evidence["nested"]["wiki_content"], "[redacted]");
        assert!(event.evidence["generated_ids"].is_object());
        for artifact in [event_artifact, receipt_artifact] {
            assert!(!artifact.contains("secret mail body"));
            assert!(!artifact.contains("secret wiki body"));
            assert!(!artifact.contains("secret memory body"));
            assert!(!artifact.contains("secret tool output"));
            assert!(!artifact.contains("secret token"));
        }
    }

    #[test]
    fn corrupt_store_replays_adapter_events_from_durable_evidence() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store
            .call(call_request(Some("agent-replay-proof")))
            .unwrap();
        store
            .record_adapter_event(complete_transport_event_request(&unit))
            .unwrap();
        fs::write(&store.paths().store_path, b"{not valid json").unwrap();

        let recovered = store.load().unwrap();
        let recovered_unit = &recovered.units[&unit.unit_id];

        assert_eq!(recovered.adapter_events.len(), 1);
        assert_eq!(recovered_unit.adapter_events.len(), 1);
        assert_eq!(
            recovered.proof_status(&unit.unit_id).unwrap().gate_status,
            ProofGateStatus::Satisfied
        );
    }

    #[test]
    fn legacy_snapshot_without_lineage_loads_with_inferred_roots() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let parent = store.call(call_request(Some("agent-parent"))).unwrap();
        let mut child_request = call_request(Some("agent-child"));
        child_request.parent_unit_id = Some(parent.unit_id.clone());
        child_request.spawn_request_id = Some("spawn-request-1".to_string());
        let child = store.call(child_request).unwrap();

        let mut snapshot = serde_json::to_value(store.load().unwrap()).unwrap();
        remove_snapshot_certificate_lineage(&mut snapshot, "agent-parent");
        remove_snapshot_certificate_lineage(&mut snapshot, "agent-child");
        fs::write(
            &store.paths().store_path,
            serde_json::to_vec_pretty(&snapshot).unwrap(),
        )
        .unwrap();

        let loaded = store.load().unwrap();
        let loaded_parent = &loaded.units[&parent.unit_id];
        let loaded_child = &loaded.units[&child.unit_id];

        assert_eq!(
            loaded_parent.certificate.lineage.root_unit_id,
            parent.unit_id
        );
        assert_eq!(
            loaded_child.certificate.lineage.parent_unit_id,
            Some(parent.unit_id.clone())
        );
        assert_eq!(
            loaded_child.certificate.lineage.root_unit_id,
            parent.unit_id
        );
        assert_eq!(
            loaded_child.certificate.lineage.spawn_request_id.as_deref(),
            Some("spawn-request-1")
        );
    }

    #[test]
    fn corrupt_store_replays_legacy_receipts_without_lineage() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let parent = store.call(call_request(Some("agent-parent"))).unwrap();
        let mut child_request = call_request(Some("agent-child"));
        child_request.parent_unit_id = Some(parent.unit_id.clone());
        child_request.spawn_request_id = Some("spawn-request-1".to_string());
        let child = store.call(child_request).unwrap();
        remove_receipt_certificate_lineage(&receipt_artifact_path(
            store.paths(),
            &parent.receipts[0],
        ));
        remove_receipt_certificate_lineage(&receipt_artifact_path(
            store.paths(),
            &child.receipts[0],
        ));
        fs::write(&store.paths().store_path, b"{not valid json").unwrap();

        let recovered = store.load().unwrap();
        let recovered_parent = &recovered.units[&parent.unit_id];
        let recovered_child = &recovered.units[&child.unit_id];

        assert_eq!(
            recovered_parent.certificate.lineage.root_unit_id,
            parent.unit_id
        );
        assert_eq!(
            recovered_child.certificate.lineage.parent_unit_id,
            Some(parent.unit_id.clone())
        );
        assert_eq!(
            recovered_child.certificate.lineage.root_unit_id,
            parent.unit_id
        );
    }

    #[test]
    fn generated_ids_are_unique_inside_one_store() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());

        let first = store.call(call_request(None)).unwrap();
        let second = store.call(call_request(None)).unwrap();

        let mut ids = BTreeSet::new();
        for value in first
            .certificate
            .generated_ids
            .values()
            .chain(second.certificate.generated_ids.values())
        {
            assert!(ids.insert(value.clone()), "duplicate generated id: {value}");
        }
        assert_ne!(first.unit_id, second.unit_id);
    }

    #[test]
    fn child_call_records_parent_lineage_and_inventory_parent() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let parent = store.call(call_request(Some("agent-parent"))).unwrap();
        let mut child_request = call_request(Some("agent-child"));
        child_request.parent_unit_id = Some(parent.unit_id.clone());
        child_request.spawn_request_id = Some("spawn-request-1".to_string());

        let child = store.call(child_request).unwrap();

        assert_eq!(
            child.certificate.lineage.parent_unit_id,
            Some(parent.unit_id.clone())
        );
        assert_eq!(child.certificate.lineage.root_unit_id, parent.unit_id);
        assert_eq!(
            child.certificate.lineage.spawn_request_id.as_deref(),
            Some("spawn-request-1")
        );
        assert_eq!(
            child.metadata.parent_unit_id.as_ref(),
            child.certificate.lineage.parent_unit_id.as_ref()
        );

        let inventory = store.inventory().unwrap();
        let child_entry = inventory
            .active
            .iter()
            .find(|entry| entry.unit_id.0 == "agent-child")
            .expect("child inventory entry");
        assert_eq!(
            child_entry.parent_unit_id.as_ref(),
            child.certificate.lineage.parent_unit_id.as_ref()
        );
    }

    #[test]
    fn retired_parent_cannot_spawn_child_unit() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let parent = store.call(call_request(Some("agent-parent"))).unwrap();
        store.retire(&parent.unit_id, "done").unwrap();
        let mut child_request = call_request(Some("agent-child"));
        child_request.parent_unit_id = Some(parent.unit_id.clone());

        let error = store.call(child_request).unwrap_err();

        assert!(matches!(error, HarnessError::UnitRetired(unit_id) if unit_id.0 == "agent-parent"));
    }

    #[test]
    fn explicit_unit_id_call_is_idempotent_for_same_inputs() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let request = call_request(Some("agent-fixed"));

        let first = store.call(request.clone()).unwrap();
        let second = store.call(request).unwrap();

        assert_eq!(first, second);
        let snapshot = store.load().unwrap();
        assert_eq!(snapshot.units.len(), 1);
        assert_eq!(snapshot.receipts.len(), 1);
    }

    #[test]
    fn explicit_unit_id_rejects_different_inputs() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        store.call(call_request(Some("agent-fixed"))).unwrap();

        let mut changed = call_request(Some("agent-fixed"));
        changed.model = "different-model".to_string();
        let error = store.call(changed).unwrap_err();

        assert!(
            matches!(error, HarnessError::UnitAlreadyExists(unit_id) if unit_id.0 == "agent-fixed")
        );
    }

    #[test]
    fn receipts_replay_active_and_retired_inventory() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let active = store.call(call_request(Some("agent-active"))).unwrap();
        let retired = store.call(call_request(Some("agent-retired"))).unwrap();
        store.retire(&retired.unit_id, "finished").unwrap();

        let receipts = read_receipt_artifacts(store.paths()).unwrap();
        let snapshot = replay_snapshot_from_receipts(store.paths(), &receipts).unwrap();
        let inventory = snapshot.inventory();

        assert_eq!(inventory.active.len(), 1);
        assert_eq!(inventory.retired.len(), 1);
        assert_eq!(inventory.active[0].unit_id, active.unit_id);
        assert_eq!(inventory.retired[0].unit_id, retired.unit_id);
        assert_eq!(
            inventory.retired[0].lifecycle_state,
            HarnessLifecycleState::Retired
        );
    }

    #[test]
    fn corrupt_store_recovers_from_receipt_artifacts() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store.call(call_request(None)).unwrap();
        fs::write(&store.paths().store_path, b"{not valid json").unwrap();

        let recovered = store.load().unwrap();
        let inventory = recovered.inventory();

        assert_eq!(inventory.active.len(), 1);
        assert_eq!(inventory.active[0].unit_id, unit.unit_id);
    }

    #[test]
    fn retirement_updates_snapshot_and_receipt_replay() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store.call(call_request(None)).unwrap();

        let retired = store.retire(&unit.unit_id, "done").unwrap();
        let inventory = store.inventory().unwrap();

        assert_eq!(retired.lifecycle_state, HarnessLifecycleState::Retired);
        assert_eq!(retired.metadata.retirement_reason.as_deref(), Some("done"));
        assert_eq!(retired.receipts.len(), 2);
        assert_eq!(inventory.active.len(), 0);
        assert_eq!(inventory.retired.len(), 1);
    }

    #[test]
    fn turn_lifecycle_accounts_usage_and_next_state() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store.call(call_request(Some("agent-turns"))).unwrap();
        let turn_id = AgentTurnId("turn-001".to_string());

        let running = store
            .start_turn(AgentTurnStartRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: Some(turn_id.clone()),
                metadata: json!({"reason": "test"}),
            })
            .unwrap();

        assert_eq!(running.lifecycle_state, HarnessLifecycleState::Running);
        assert_eq!(running.metadata.active_turn_id, Some(turn_id.clone()));
        assert_eq!(running.metadata.turns_started, 1);
        assert_eq!(
            running.receipts.last().unwrap().kind,
            ReceiptKind::TurnStarted
        );

        let waiting = store
            .complete_turn(AgentTurnCompleteRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: turn_id.clone(),
                usage: AgentTurnUsage {
                    input_tokens: 11,
                    output_tokens: 7,
                    total_tokens: None,
                    duration_ms: 1234,
                },
                next_state: AgentTurnCompletionState::Waiting,
                metadata: json!({"stop": "needs-user"}),
            })
            .unwrap();

        assert_eq!(waiting.lifecycle_state, HarnessLifecycleState::Waiting);
        assert_eq!(waiting.metadata.active_turn_id, None);
        assert_eq!(waiting.metadata.turns_started, 1);
        assert_eq!(waiting.metadata.turns_completed, 1);
        assert_eq!(waiting.metadata.input_tokens, 11);
        assert_eq!(waiting.metadata.output_tokens, 7);
        assert_eq!(waiting.metadata.total_tokens, 18);
        assert_eq!(waiting.metadata.total_duration_ms, 1234);
        assert_eq!(
            waiting.receipts.last().unwrap().kind,
            ReceiptKind::TurnCompleted
        );
    }

    #[test]
    fn duplicate_and_invalid_turns_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store.call(call_request(Some("agent-turn-errors"))).unwrap();
        let turn_id = AgentTurnId("turn-active".to_string());
        store
            .start_turn(AgentTurnStartRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: Some(turn_id.clone()),
                metadata: json!({}),
            })
            .unwrap();

        let duplicate = store
            .start_turn(AgentTurnStartRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: Some(turn_id.clone()),
                metadata: json!({}),
            })
            .unwrap_err();
        assert!(matches!(
            duplicate,
            HarnessError::TurnAlreadyActive {
                unit_id,
                active_turn_id
            } if unit_id.0 == "agent-turn-errors" && active_turn_id == turn_id
        ));

        let mismatch = store
            .complete_turn(AgentTurnCompleteRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: AgentTurnId("turn-other".to_string()),
                usage: AgentTurnUsage::default(),
                next_state: AgentTurnCompletionState::Ready,
                metadata: json!({}),
            })
            .unwrap_err();
        assert!(matches!(
            mismatch,
            HarnessError::TurnMismatch {
                requested_turn_id,
                ..
            } if requested_turn_id.0 == "turn-other"
        ));

        store
            .complete_turn(AgentTurnCompleteRequest {
                unit_id: unit.unit_id.clone(),
                turn_id,
                usage: AgentTurnUsage::default(),
                next_state: AgentTurnCompletionState::Ready,
                metadata: json!({}),
            })
            .unwrap();
        let no_active = store
            .complete_turn(AgentTurnCompleteRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: AgentTurnId("turn-active".to_string()),
                usage: AgentTurnUsage::default(),
                next_state: AgentTurnCompletionState::Ready,
                metadata: json!({}),
            })
            .unwrap_err();
        assert!(matches!(no_active, HarnessError::NoActiveTurn { .. }));
    }

    #[test]
    fn retired_units_reject_turn_mutations() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store
            .call(call_request(Some("agent-retired-turns")))
            .unwrap();
        store.retire(&unit.unit_id, "done").unwrap();

        let start_error = store
            .start_turn(AgentTurnStartRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: Some(AgentTurnId("turn-retired".to_string())),
                metadata: json!({}),
            })
            .unwrap_err();
        assert!(
            matches!(start_error, HarnessError::UnitRetired(unit_id) if unit_id == unit.unit_id)
        );

        let complete_error = store
            .complete_turn(AgentTurnCompleteRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: AgentTurnId("turn-retired".to_string()),
                usage: AgentTurnUsage::default(),
                next_state: AgentTurnCompletionState::Done,
                metadata: json!({}),
            })
            .unwrap_err();
        assert!(
            matches!(complete_error, HarnessError::UnitRetired(unit_id) if unit_id == unit.unit_id)
        );
    }

    #[test]
    fn receipt_replay_restores_turn_lifecycle_and_usage() {
        let temp = tempfile::tempdir().unwrap();
        let store = AgentHarnessStore::new(temp.path());
        let unit = store
            .call(call_request(Some("agent-replay-turns")))
            .unwrap();
        let first_turn = AgentTurnId("turn-one".to_string());
        let second_turn = AgentTurnId("turn-two".to_string());

        store
            .start_turn(AgentTurnStartRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: Some(first_turn.clone()),
                metadata: json!({}),
            })
            .unwrap();
        store
            .complete_turn(AgentTurnCompleteRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: first_turn,
                usage: AgentTurnUsage {
                    input_tokens: 3,
                    output_tokens: 5,
                    total_tokens: Some(10),
                    duration_ms: 100,
                },
                next_state: AgentTurnCompletionState::Ready,
                metadata: json!({}),
            })
            .unwrap();
        store
            .start_turn(AgentTurnStartRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: Some(second_turn.clone()),
                metadata: json!({}),
            })
            .unwrap();
        store
            .complete_turn(AgentTurnCompleteRequest {
                unit_id: unit.unit_id.clone(),
                turn_id: second_turn,
                usage: AgentTurnUsage {
                    input_tokens: 7,
                    output_tokens: 2,
                    total_tokens: None,
                    duration_ms: 200,
                },
                next_state: AgentTurnCompletionState::Done,
                metadata: json!({}),
            })
            .unwrap();

        let receipts = read_receipt_artifacts(store.paths()).unwrap();
        let replayed = replay_snapshot_from_receipts(store.paths(), &receipts).unwrap();
        let replayed_unit = replayed.units.get(&unit.unit_id).unwrap();

        assert_eq!(replayed_unit.lifecycle_state, HarnessLifecycleState::Done);
        assert_eq!(replayed_unit.metadata.active_turn_id, None);
        assert_eq!(replayed_unit.metadata.turns_started, 2);
        assert_eq!(replayed_unit.metadata.turns_completed, 2);
        assert_eq!(replayed_unit.metadata.input_tokens, 10);
        assert_eq!(replayed_unit.metadata.output_tokens, 7);
        assert_eq!(replayed_unit.metadata.total_tokens, 19);
        assert_eq!(replayed_unit.metadata.total_duration_ms, 300);
        assert_eq!(replayed_unit.receipts.len(), 5);
    }
}
