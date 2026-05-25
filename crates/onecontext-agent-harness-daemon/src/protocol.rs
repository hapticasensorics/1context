use onecontext_agent_harness_core::{
    AdapterCorrelation, AdapterEventKind, AdapterEventRequest, AdapterEventStatus, AdapterKind,
    AdapterRedaction, AgentTurnCompleteRequest, AgentTurnCompletionState, AgentTurnId,
    AgentTurnStartRequest, AgentTurnUsage, AgentUnitId, CapabilityTransport,
    AGENT_HARNESS_SCHEMA_VERSION,
};
use serde_json::{json, Map, Value};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartTurnRequest {
    pub unit_id: AgentUnitId,
    pub turn_id: Option<AgentTurnId>,
    pub parent_turn_id: Option<AgentTurnId>,
    pub reason: Option<String>,
    pub expected_transport: Option<CapabilityTransport>,
    pub context: Value,
    pub metadata: Value,
}

impl StartTurnRequest {
    pub fn into_core(self) -> AgentTurnStartRequest {
        AgentTurnStartRequest {
            unit_id: self.unit_id,
            turn_id: self.turn_id,
            metadata: json!({
                "parent_turn_id": self.parent_turn_id,
                "reason": self.reason,
                "expected_transport": self.expected_transport,
                "context": self.context,
                "metadata": self.metadata,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompleteTurnRequest {
    pub unit_id: AgentUnitId,
    pub turn_id: AgentTurnId,
    pub outcome: TurnOutcome,
    pub usage: UsageDelta,
    pub duration_ms: Option<u64>,
    pub error: Value,
    pub metadata: Value,
}

impl CompleteTurnRequest {
    pub fn into_core(self) -> AgentTurnCompleteRequest {
        AgentTurnCompleteRequest {
            unit_id: self.unit_id,
            turn_id: self.turn_id,
            usage: AgentTurnUsage {
                input_tokens: self.usage.input_tokens,
                output_tokens: self.usage.output_tokens,
                total_tokens: Some(self.usage.total_tokens),
                duration_ms: self.duration_ms.unwrap_or(0),
            },
            next_state: self.outcome.into(),
            metadata: json!({
                "error": self.error,
                "metadata": self.metadata,
            }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordAdapterEventRequest {
    pub unit_id: AgentUnitId,
    pub adapter: AdapterKind,
    pub kind: AdapterEventKind,
    pub status: AdapterEventStatus,
    pub event_id: Option<String>,
    pub at: Option<String>,
    pub correlation: AdapterCorrelation,
    pub evidence: Value,
    pub redaction: AdapterRedaction,
}

impl RecordAdapterEventRequest {
    pub fn into_core(self) -> AdapterEventRequest {
        AdapterEventRequest {
            unit_id: self.unit_id,
            adapter: self.adapter,
            kind: self.kind,
            status: self.status,
            correlation: self.correlation,
            evidence: self.evidence,
            redaction: self.redaction,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ObserveProofRequest {
    pub unit_id: AgentUnitId,
    pub proof_key: String,
    pub status: Option<String>,
    pub evidence: Value,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransportPlanRequest {
    pub unit_id: Option<AgentUnitId>,
    pub capability_id: Option<String>,
    pub requested_transports: Vec<CapabilityTransport>,
    pub intent: Option<String>,
    pub metadata: Value,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageDelta {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    Waiting,
    Done,
}

impl TurnOutcome {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Waiting => "waiting",
            Self::Done => "done",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "completed" | "ready" | "success" | "ok" => Some(Self::Completed),
            "waiting" => Some(Self::Waiting),
            "done" => Some(Self::Done),
            _ => None,
        }
    }
}

impl From<TurnOutcome> for AgentTurnCompletionState {
    fn from(value: TurnOutcome) -> Self {
        match value {
            TurnOutcome::Completed => Self::Ready,
            TurnOutcome::Waiting => Self::Waiting,
            TurnOutcome::Done => Self::Done,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError {
    pub message: String,
    pub repair_hints: Vec<String>,
}

impl ProtocolError {
    fn new(message: impl Into<String>, repair_hints: Vec<String>) -> Self {
        Self {
            message: message.into(),
            repair_hints,
        }
    }

    fn missing_string(field: &str) -> Self {
        Self::new(
            format!("Request JSON is missing required string field: {field}"),
            vec![format!(
                "Add a non-empty {field} string to the request JSON."
            )],
        )
    }
}

pub fn parse_start_turn(request: &Value) -> Result<StartTurnRequest, ProtocolError> {
    let object = object(request)?;
    Ok(StartTurnRequest {
        unit_id: AgentUnitId(required_string(object, "unit_id")?),
        turn_id: optional_string(object, "turn_id")?.map(AgentTurnId),
        parent_turn_id: optional_string(object, "parent_turn_id")?.map(AgentTurnId),
        reason: optional_string(object, "reason")?,
        expected_transport: optional_transport(object, "expected_transport")?,
        context: optional_value(object, "context"),
        metadata: optional_value(object, "metadata"),
    })
}

pub fn parse_complete_turn(request: &Value) -> Result<CompleteTurnRequest, ProtocolError> {
    let object = object(request)?;
    let outcome = optional_string(object, "outcome")?
        .or(optional_string(object, "status")?)
        .map(|value| {
            TurnOutcome::parse(&value).ok_or_else(|| {
                ProtocolError::new(
                    format!("Unsupported complete-turn outcome: {value}"),
                    vec!["Use completed, waiting, or done.".to_string()],
                )
            })
        })
        .transpose()?
        .unwrap_or(TurnOutcome::Completed);

    let input_tokens = optional_u64(object, "input_tokens")?
        .or_else(|| nested_u64(object, "usage", "input_tokens"))
        .unwrap_or(0);
    let output_tokens = optional_u64(object, "output_tokens")?
        .or_else(|| nested_u64(object, "usage", "output_tokens"))
        .unwrap_or(0);
    let total_tokens = optional_u64(object, "total_tokens")?
        .or_else(|| nested_u64(object, "usage", "total_tokens"))
        .unwrap_or(input_tokens + output_tokens);

    Ok(CompleteTurnRequest {
        unit_id: AgentUnitId(required_string(object, "unit_id")?),
        turn_id: AgentTurnId(required_string(object, "turn_id")?),
        outcome,
        usage: UsageDelta {
            input_tokens,
            output_tokens,
            total_tokens,
        },
        duration_ms: optional_u64(object, "duration_ms")?,
        error: optional_value(object, "error"),
        metadata: optional_value(object, "metadata"),
    })
}

pub fn parse_record_adapter_event(
    request: &Value,
) -> Result<RecordAdapterEventRequest, ProtocolError> {
    let object = object(request)?;
    Ok(RecordAdapterEventRequest {
        unit_id: AgentUnitId(required_string(object, "unit_id")?),
        adapter: adapter_kind(object, "adapter")?,
        kind: adapter_event_kind(object, "kind")?,
        status: adapter_event_status(object, "status")?,
        event_id: optional_string(object, "event_id")?.or(optional_string(object, "id")?),
        at: optional_string(object, "at")?,
        correlation: optional_correlation(object, "correlation")?,
        evidence: optional_value(object, "evidence"),
        redaction: optional_redaction(object, "redaction")?,
    })
}

pub fn parse_observe_proof(request: &Value) -> Result<ObserveProofRequest, ProtocolError> {
    let object = object(request)?;
    let proof_key = optional_string(object, "proof_key")?
        .or(optional_string(object, "proof")?)
        .or(optional_string(object, "kind")?)
        .ok_or_else(|| {
            ProtocolError::new(
                "observe-proof requires proof_key, proof, or kind.",
                vec![
                    "Pass proof_key for a named proof requirement, or kind for a proof family."
                        .to_string(),
                ],
            )
        })?;
    Ok(ObserveProofRequest {
        unit_id: AgentUnitId(required_string(object, "unit_id")?),
        proof_key,
        status: optional_string(object, "status")?,
        evidence: optional_value(object, "evidence"),
        metadata: optional_value(object, "metadata"),
    })
}

pub fn parse_transport_plan(request: &Value) -> Result<TransportPlanRequest, ProtocolError> {
    let object = object(request)?;
    Ok(TransportPlanRequest {
        unit_id: optional_string(object, "unit_id")?.map(AgentUnitId),
        capability_id: optional_string(object, "capability_id")?,
        requested_transports: optional_transport_array(object, "requested_transports")?,
        intent: optional_string(object, "intent")?,
        metadata: optional_value(object, "metadata"),
    })
}

pub fn observe_proof_request_payload(request: &ObserveProofRequest) -> Value {
    json!({
        "unit_id": request.unit_id.0.clone(),
        "proof_key": request.proof_key.clone(),
        "status": request.status.clone(),
        "evidence": &request.evidence,
        "metadata": &request.metadata,
    })
}

pub fn transport_plan_request_payload(request: &TransportPlanRequest) -> Value {
    json!({
        "unit_id": request.unit_id.as_ref().map(|unit_id| unit_id.0.clone()),
        "capability_id": request.capability_id.clone(),
        "requested_transports": &request.requested_transports,
        "intent": request.intent.clone(),
        "metadata": &request.metadata,
    })
}

pub fn protocol_receipt(
    command: &str,
    unit_id: &str,
    at: &str,
    receipt_kind: &str,
    summary: impl Into<String>,
    evidence: Value,
) -> Value {
    json!({
        "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
        "id": format!("daemon-{receipt_kind}-{unit_id}-{command}", command = command.replace('-', "_")),
        "at": at,
        "unit_id": unit_id,
        "kind": receipt_kind,
        "summary": summary.into(),
        "evidence": evidence,
    })
}

fn object(request: &Value) -> Result<&Map<String, Value>, ProtocolError> {
    request.as_object().ok_or_else(|| {
        ProtocolError::new(
            "Request JSON must be an object.",
            vec!["Use an object such as {\"unit_id\":\"agent-...\"}.".to_string()],
        )
    })
}

fn required_string(object: &Map<String, Value>, field: &str) -> Result<String, ProtocolError> {
    optional_string(object, field)?.ok_or_else(|| ProtocolError::missing_string(field))
}

fn optional_string(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<String>, ProtocolError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    match value
        .as_str()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => Ok(Some(value.to_string())),
        None if value.is_null() => Ok(None),
        None => Err(ProtocolError::missing_string(field)),
    }
}

fn optional_u64(object: &Map<String, Value>, field: &str) -> Result<Option<u64>, ProtocolError> {
    let Some(value) = object.get(field) else {
        return Ok(None);
    };
    value.as_u64().map(Some).ok_or_else(|| {
        ProtocolError::new(
            format!("Request field {field} must be an unsigned integer."),
            vec![format!(
                "Pass {field} as a JSON number greater than or equal to 0."
            )],
        )
    })
}

fn nested_u64(object: &Map<String, Value>, parent: &str, field: &str) -> Option<u64> {
    object
        .get(parent)
        .and_then(Value::as_object)
        .and_then(|usage| usage.get(field))
        .and_then(Value::as_u64)
}

fn optional_value(object: &Map<String, Value>, field: &str) -> Value {
    object.get(field).cloned().unwrap_or_else(|| json!({}))
}

fn optional_correlation(
    object: &Map<String, Value>,
    field: &str,
) -> Result<AdapterCorrelation, ProtocolError> {
    let Some(value) = object.get(field) else {
        return Ok(AdapterCorrelation::default());
    };
    serde_json::from_value::<AdapterCorrelation>(value.clone()).map_err(|error| {
        ProtocolError::new(
            format!("Request field {field} has invalid shape: {error}"),
            vec![format!("Check the object shape for {field}.")],
        )
    })
}

fn optional_redaction(
    object: &Map<String, Value>,
    field: &str,
) -> Result<AdapterRedaction, ProtocolError> {
    let Some(value) = object.get(field) else {
        return Ok(AdapterRedaction::default());
    };
    serde_json::from_value::<AdapterRedaction>(value.clone()).map_err(|error| {
        ProtocolError::new(
            format!("Request field {field} has invalid shape: {error}"),
            vec![format!("Check the object shape for {field}.")],
        )
    })
}

fn optional_transport(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Option<CapabilityTransport>, ProtocolError> {
    let Some(raw) = optional_string(object, field)? else {
        return Ok(None);
    };
    parse_transport(raw, field).map(Some)
}

fn adapter_kind(object: &Map<String, Value>, field: &str) -> Result<AdapterKind, ProtocolError> {
    let raw = required_string(object, field)?;
    serde_json::from_value::<AdapterKind>(Value::String(raw.clone()))
        .map_err(|error| unsupported_enum_error(field, &raw, error))
}

fn adapter_event_kind(
    object: &Map<String, Value>,
    field: &str,
) -> Result<AdapterEventKind, ProtocolError> {
    let raw = required_string(object, field)?;
    serde_json::from_value::<AdapterEventKind>(Value::String(raw.clone()))
        .map_err(|error| unsupported_enum_error(field, &raw, error))
}

fn adapter_event_status(
    object: &Map<String, Value>,
    field: &str,
) -> Result<AdapterEventStatus, ProtocolError> {
    let raw = required_string(object, field)?;
    serde_json::from_value::<AdapterEventStatus>(Value::String(raw.clone()))
        .map_err(|error| unsupported_enum_error(field, &raw, error))
}

fn parse_transport(raw: String, field: &str) -> Result<CapabilityTransport, ProtocolError> {
    serde_json::from_value::<CapabilityTransport>(Value::String(raw.clone()))
        .map_err(|error| unsupported_enum_error(field, &raw, error))
}

fn unsupported_enum_error(field: &str, raw: &str, error: serde_json::Error) -> ProtocolError {
    ProtocolError::new(
        format!("Request field {field} has unsupported value {raw}: {error}"),
        vec![format!("Use a supported snake_case value for {field}.")],
    )
}

fn optional_transport_array(
    object: &Map<String, Value>,
    field: &str,
) -> Result<Vec<CapabilityTransport>, ProtocolError> {
    let Some(value) = object.get(field) else {
        return Ok(Vec::new());
    };
    let Some(values) = value.as_array() else {
        return Err(ProtocolError::new(
            format!("Request field {field} must be an array."),
            vec![format!("Pass {field} as an array of snake_case strings.")],
        ));
    };
    values
        .iter()
        .enumerate()
        .map(|(index, value)| {
            let Some(raw) = value
                .as_str()
                .map(str::trim)
                .filter(|value| !value.is_empty())
            else {
                return Err(ProtocolError::new(
                    format!("Request field {field}[{index}] must be a non-empty string."),
                    vec![format!("Pass {field} as an array of snake_case strings.")],
                ));
            };
            parse_transport(raw.to_string(), &format!("{field}[{index}]"))
        })
        .collect()
}
