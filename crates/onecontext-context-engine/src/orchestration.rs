//! Executable orchestration IR.
//!
//! These types are the inspectable execution contract the scheduler and the
//! runtime executor consume. The forthcoming FSM DSL runner hydrates them.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutableOrchestrationPlan {
    pub schema_version: u32,
    pub kind: String,
    pub phase_count: usize,
    pub job_count: usize,
    pub phases: Vec<ExecutablePhase>,
    pub receipt_policy: ExecutableReceiptPolicy,
    pub packet_policy: ExecutablePacketPolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutablePhase {
    pub id: String,
    pub label: String,
    pub owner: String,
    pub depends_on: Vec<String>,
    pub strategy: String,
    pub completion: String,
    pub reads_raw_history: bool,
    pub durable_receipt: String,
    pub jobs: Vec<ExecutablePhaseJob>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutablePhaseJob {
    pub phase_id: String,
    pub job_id: String,
    pub agent_id: String,
    pub fanout: String,
    pub when: Option<String>,
    pub max_concurrent: Option<u32>,
    pub required_artifacts: Vec<String>,
    pub required_mail: Vec<String>,
    pub bindings: BTreeMap<String, String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub route: ResolvedRoute,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutableReceiptPolicy {
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ExecutablePacketPolicy {
    pub recent_first_days: u32,
    pub backfill_days: u32,
    pub backfill_order: String,
    pub incremental_unit: Option<String>,
    pub model_context_tokens: u32,
    pub source_context_fraction: f64,
    pub target_source_tokens: u32,
    pub split_when_estimated_tokens_exceed: u32,
    pub raw_history_roles: Vec<String>,
    pub downstream_roles_read_scribe_artifacts: bool,
    pub cache_enabled: bool,
    pub cache_key: Option<String>,
    pub skip_unchanged_scribe_packets: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRoute {
    pub from_role: String,
    pub thread_id: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
}
