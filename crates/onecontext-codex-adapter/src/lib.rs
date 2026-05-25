//! Codex-specific adapter seams for 1Context.
//!
//! The crate is intentionally a scaffold first: typed records, validation, and
//! pure planning helpers before any live app-server transport is wired in.

pub mod agent_binding;
pub mod app_server_client;
pub mod event_mirror;
pub mod harness_bridge;
pub mod hook_manager;
pub mod injection_bridge;
pub mod live_app_server;
pub mod policy_bridge;
pub mod proof_recorder;
pub mod schema_registry;
pub mod wake_dispatcher;

use std::fmt;

pub use agent_binding::{
    validate_binding, AgentLeaseProjection, AgentLeaseStateProjection, BindingValidationIssue,
    BindingValidationReport, CodexAdapterBinding, CodexAdapterPolicyProjection, CodexLoadedState,
    CodexToolsetProjection, CodexTransportProjection, EffectiveLeaseState,
    ALLOWED_VISIBLE_TOOLSETS, CODEX_APP_SERVER_TRANSPORT_KIND, ONECONTEXT_MCP_SERVER_NAME,
};
pub use app_server_client::{
    required_app_server_methods, AppServerCapability, AppServerCapabilityCheck,
    AppServerCapabilityReport, AppServerConnectionConfig, AppServerMethod,
};
pub use event_mirror::{
    mirror_runtime_event, CodexRuntimeEvent, EventMirrorPlan, EventMirrorTarget,
};
pub use harness_bridge::{
    proof_status_has_category, unit_is_ready_or_born, GovernedChildAgentRequest,
    GovernedChildSpawnPolicy, HarnessAgentSpawner, HarnessProofSink, InProcessHarnessBridge,
};
pub use hook_manager::{
    required_mail_hook_events, CodexHookEventName, CodexHookHandlerType, CodexHookIntent,
    CodexHookIntentAction, CodexHookRecord, CodexHookRegistryObservation,
    CodexHookRegistryTrustState, CodexHookSourceKind, CodexHookTrustStatus, CodexStopGuardBudget,
    CodexStopGuardContext, CodexStopGuardDecision, CodexStopGuardPlan,
};
pub use injection_bridge::{
    AuthorizedInjectionTarget, BodylessContentDeliveryRequest, BodylessMailOpenResult,
    BodylessOpenedMessageSummary, CodexInjectionJob, InjectionJobStatus,
    InjectionReceiptAndProofPlan, InjectionReceiptPlan, InjectionReceiptResult,
};
pub use live_app_server::{
    initialize_request, json_rpc_request, live_app_server_dogfood_phases,
    live_app_server_proof_event_requests, live_required_method_observations, live_required_methods,
    live_schema_availability_from_bundle_json, live_schema_availability_from_file_names,
    summarize_json_rpc_transcript, summarize_json_rpc_transcript_jsonl,
    thread_inject_items_request, thread_loaded_list_request, thread_start_request,
    turn_start_request, turn_steer_request, JsonRpcTranscriptSummary,
    LiveAppServerDogfoodArtifacts, LiveAppServerDogfoodPhase, LiveAppServerDogfoodPlan,
    LiveAppServerDogfoodRequest, LiveAppServerMethodObservation, LiveAppServerMethodStatus,
    LiveAppServerSchemaAvailability,
};
pub use policy_bridge::{
    check_evidence_redaction, infer_toolset_for_tool, AdapterPolicyDecision, AdapterPolicyReason,
    EvidenceRedactionCheck,
};
pub use proof_recorder::{
    hook_proof_plan, injection_proof_plan, toolset_visibility_proof_plan,
    toolset_visibility_proof_plan_for_required, wake_proof_plan, HarnessProofRecordPlan,
    ProofRecordFamily, ProofRecordPlan, ProofRecordTarget, ToolsetVisibilityObservation,
};
pub use schema_registry::{SchemaGenerationCommandPlan, SchemaRegistryStatus, SchemaSource};
pub use wake_dispatcher::{
    plan_wake, CodexWakeAttempt, CodexWakeAttemptResult, CodexWakePlan, CodexWakePlanInput,
    CodexWakePlanReason, CodexWakeStrategy, CodexWakeSuppression,
};

pub const CODEX_ADAPTER_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodexAdapterError {
    MissingField(&'static str),
    InvalidState(String),
    PolicyDenied(String),
}

impl fmt::Display for CodexAdapterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingField(field) => write!(f, "missing required field: {field}"),
            Self::InvalidState(message) => write!(f, "invalid adapter state: {message}"),
            Self::PolicyDenied(message) => write!(f, "policy denied adapter action: {message}"),
        }
    }
}

impl std::error::Error for CodexAdapterError {}

pub type CodexAdapterResult<T> = Result<T, CodexAdapterError>;
