//! Turn lifecycle request types.
//!
//! Invariant protected here: turn completion can only resolve an active turn to
//! one of the non-running post-turn lifecycle states owned by the core store.

use crate::{AgentTurnId, AgentUnitId, HarnessLifecycleState};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTurnStartRequest {
    pub unit_id: AgentUnitId,
    pub turn_id: Option<AgentTurnId>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTurnCompleteRequest {
    pub unit_id: AgentUnitId,
    pub turn_id: AgentTurnId,
    #[serde(default)]
    pub usage: AgentTurnUsage,
    #[serde(default)]
    pub next_state: AgentTurnCompletionState,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AgentTurnUsage {
    #[serde(default)]
    pub input_tokens: u64,
    #[serde(default)]
    pub output_tokens: u64,
    #[serde(default)]
    pub total_tokens: Option<u64>,
    #[serde(default)]
    pub duration_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AgentTurnCompletionState {
    #[default]
    Ready,
    Waiting,
    Done,
}

impl AgentTurnCompletionState {
    pub const fn lifecycle_state(&self) -> HarnessLifecycleState {
        match self {
            Self::Ready => HarnessLifecycleState::Ready,
            Self::Waiting => HarnessLifecycleState::Waiting,
            Self::Done => HarnessLifecycleState::Done,
        }
    }
}
