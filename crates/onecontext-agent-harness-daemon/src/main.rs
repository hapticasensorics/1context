use onecontext_agent_harness_core::{
    describe_agent_harness_contract, summarize_proof_status, utc_now_rfc3339, AdapterEvent,
    AgentAvailability, AgentCallRequest, AgentHarnessInventoryEntry, AgentHarnessPaths,
    AgentHarnessStore, AgentHarnessUnit, AgentUnitId, HarnessError, HarnessLifecycleState,
    ReceiptKind, AGENT_HARNESS_SCHEMA_VERSION,
};
mod protocol;

use protocol::{
    observe_proof_request_payload, parse_complete_turn, parse_observe_proof,
    parse_record_adapter_event, parse_start_turn, parse_transport_plan, protocol_receipt,
    transport_plan_request_payload, ProtocolError, TransportPlanRequest,
};
use serde_json::{json, Map, Value};
use std::fs;
use std::path::PathBuf;

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result = run(args);
    match result {
        Ok(payload) => print_json(&payload),
        Err(error) => {
            print_json(&json!({
                "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
                "status": "error",
                "surface": "agent_harness",
                "error": {
                    "code": error.code,
                    "message": error.message,
                    "details": error.details,
                },
                "repair_hints": error.repair_hints,
            }));
            std::process::exit(error.exit_code);
        }
    }
}

fn run(mut args: Vec<String>) -> Result<Value, CliError> {
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(help_payload());
    }

    let root = take_required_flag_value(&mut args, "--root")?
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("runtime-test/1Context"));
    let request = take_request(&mut args)?;
    let command = args.first().cloned().unwrap_or_else(|| "help".to_string());
    if !args.is_empty() {
        args.remove(0);
    }
    if !args.is_empty() {
        return Err(CliError::invalid_arguments(
            format!("Unexpected argument for {command}: {}", args[0]),
            vec!["Pass request data with --request-json or --request-file.".to_string()],
        ));
    }
    let paths = AgentHarnessPaths::new(root);

    match normalize_command(&command).as_str() {
        "help" => Ok(help_payload()),
        "describe" => Ok(describe_agent_harness_contract()),
        "ensure" => {
            paths.ensure_dirs().map_err(|error| CliError {
                exit_code: 1,
                code: "agent_harness_ensure_failed".to_string(),
                message: error.to_string(),
                details: json!({}),
                repair_hints: vec![
                    "Check that --root points to a writable 1Context directory.".to_string()
                ],
            })?;
            Ok(json!({
                "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
                "status": "ok",
                "operation": "agent.harness.ensure",
                "at": utc_now_rfc3339(),
                "paths": paths.status_payload(),
            }))
        }
        "status" => Ok(paths.status_payload()),
        "call" => call_route(&paths, "call", request),
        "birth" => call_route(&paths, "birth", request),
        "start-turn" => start_turn_route(&paths, request),
        "complete-turn" => complete_turn_route(&paths, request),
        "observe-proof" => observe_proof_route(&paths, request),
        "record-adapter-event" => record_adapter_event_route(&paths, request),
        "transport-plan" => transport_plan_route(&paths, request),
        "agents" => agents_route(&paths, request),
        "agent-status" => agent_status_route(&paths, request),
        "retire" => retire_route(&paths, request),
        other => Err(CliError {
            exit_code: 2,
            code: "agent_harness_unknown_command".to_string(),
            message: format!("Unknown agent harness command: {other}"),
            details: json!({ "command": other }),
            repair_hints: vec!["Run onecontext-agent-harness --help.".to_string()],
        }),
    }
}

fn help_payload() -> Value {
    json!({
        "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
        "status": "ok",
        "surface": "agent_harness_help",
        "commands": [
            "ensure",
            "status",
            "describe",
            "call",
            "birth",
            "start-turn",
            "complete-turn",
            "observe-proof",
            "record-adapter-event",
            "transport-plan",
            "agents",
            "agent-status",
            "retire"
        ],
    })
}

fn call_route(paths: &AgentHarnessPaths, command: &str, request: Value) -> Result<Value, CliError> {
    let request = validate_request(command, request, RequestShape::Call)?;
    let call_request =
        serde_json::from_value::<AgentCallRequest>(request.clone()).map_err(|error| {
            CliError::invalid_request(
                command,
                error.to_string(),
                vec!["Match the public AgentCallRequest schema.".to_string()],
            )
        })?;
    let store = AgentHarnessStore::from_paths(paths.clone());
    let unit = store
        .call(call_request)
        .map_err(|error| CliError::from_harness_error(command, error))?;
    Ok(unit_payload(command, unit))
}

fn agents_route(paths: &AgentHarnessPaths, request: Value) -> Result<Value, CliError> {
    validate_request("agents", request, RequestShape::AnyObject)?;
    let store = AgentHarnessStore::from_paths(paths.clone());
    let inventory = store
        .inventory()
        .map_err(|error| CliError::from_harness_error("agents", error))?;

    let mut active = Vec::new();
    let mut waiting = Vec::new();
    let mut blocked = Vec::new();
    let mut done = Vec::new();
    for entry in inventory.active {
        let payload = inventory_entry_payload(&entry);
        match entry.lifecycle_state {
            HarnessLifecycleState::Waiting => waiting.push(payload),
            HarnessLifecycleState::Blocked => blocked.push(payload),
            HarnessLifecycleState::Done => done.push(payload),
            _ => active.push(payload),
        }
    }
    let retired = inventory
        .retired
        .iter()
        .map(inventory_entry_payload)
        .collect::<Vec<_>>();
    let active_count = active.len();
    let waiting_count = waiting.len();
    let blocked_count = blocked.len();
    let done_count = done.len();
    let retired_count = retired.len();

    Ok(json!({
        "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
        "status": "ok",
        "surface": "agent_harness_protocol",
        "operation": "agent.harness.agents",
        "at": utc_now_rfc3339(),
        "agents": {
            "active": active,
            "waiting": waiting,
            "blocked": blocked,
            "done": done,
            "retired": retired,
            "counts": {
                "active": active_count,
                "waiting": waiting_count,
                "blocked": blocked_count,
                "done": done_count,
                "retired": retired_count
            }
        }
    }))
}

fn agent_status_route(paths: &AgentHarnessPaths, request: Value) -> Result<Value, CliError> {
    let request = validate_request("agent-status", request, RequestShape::Unit)?;
    let unit_id = unit_id_from_request("agent-status", &request)?;
    let store = AgentHarnessStore::from_paths(paths.clone());
    let status = store
        .agent_status(&unit_id)
        .map_err(|error| CliError::from_harness_error("agent-status", error))?;
    Ok(agent_status_payload(status.unit))
}

fn retire_route(paths: &AgentHarnessPaths, request: Value) -> Result<Value, CliError> {
    let request = validate_request("retire", request, RequestShape::Unit)?;
    let unit_id = unit_id_from_request("retire", &request)?;
    let reason = request
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("retired by agent harness daemon request");
    let store = AgentHarnessStore::from_paths(paths.clone());
    let unit = store
        .retire(&unit_id, reason)
        .map_err(|error| CliError::from_harness_error("retire", error))?;
    Ok(unit_payload("retire", unit))
}

fn start_turn_route(paths: &AgentHarnessPaths, request: Value) -> Result<Value, CliError> {
    let request =
        parse_start_turn(&request).map_err(|error| protocol_error("start-turn", error))?;
    let store = AgentHarnessStore::from_paths(paths.clone());
    let unit = store
        .start_turn(request.into_core())
        .map_err(|error| CliError::from_harness_error("start-turn", error))?;
    Ok(unit_payload("start-turn", unit))
}

fn complete_turn_route(paths: &AgentHarnessPaths, request: Value) -> Result<Value, CliError> {
    let request =
        parse_complete_turn(&request).map_err(|error| protocol_error("complete-turn", error))?;
    let store = AgentHarnessStore::from_paths(paths.clone());
    let unit = store
        .complete_turn(request.into_core())
        .map_err(|error| CliError::from_harness_error("complete-turn", error))?;
    Ok(unit_payload("complete-turn", unit))
}

fn observe_proof_route(paths: &AgentHarnessPaths, request: Value) -> Result<Value, CliError> {
    let request =
        parse_observe_proof(&request).map_err(|error| protocol_error("observe-proof", error))?;
    let unit = load_active_unit(paths, "observe-proof", &request.unit_id)?;
    let at = utc_now_rfc3339();
    let request_payload = observe_proof_request_payload(&request);
    let receipt = protocol_receipt(
        "observe-proof",
        &unit.unit_id.0,
        &at,
        "proof_observed",
        format!("validated proof observation for {}", unit.unit_id.0),
        json!({
            "request": request_payload,
            "proof_status_before": proof_status_payload(&unit),
            "core_method_expected": "AgentHarnessStore::observe_proof(ObserveProofRequest) -> Result<AgentHarnessUnit, HarnessError>",
        }),
    );

    Ok(frontier_scaffold_payload(
        "observe-proof",
        at,
        unit.unit_id.0,
        request_payload,
        receipt,
        "AgentHarnessStore::observe_proof(ObserveProofRequest) -> Result<AgentHarnessUnit, HarnessError>",
    ))
}

fn record_adapter_event_route(
    paths: &AgentHarnessPaths,
    request: Value,
) -> Result<Value, CliError> {
    let request = parse_record_adapter_event(&request)
        .map_err(|error| protocol_error("record-adapter-event", error))?;
    let store = AgentHarnessStore::from_paths(paths.clone());
    let unit = store
        .record_adapter_event(request.into_core())
        .map_err(|error| CliError::from_harness_error("record-adapter-event", error))?;
    Ok(unit_payload("record-adapter-event", unit))
}

fn transport_plan_route(paths: &AgentHarnessPaths, request: Value) -> Result<Value, CliError> {
    let request =
        parse_transport_plan(&request).map_err(|error| protocol_error("transport-plan", error))?;
    let unit = match &request.unit_id {
        Some(unit_id) => Some(load_active_unit(paths, "transport-plan", unit_id)?),
        None => None,
    };
    paths.ensure_dirs().map_err(|error| CliError {
        exit_code: 1,
        code: "agent_harness_ensure_failed".to_string(),
        message: error.to_string(),
        details: json!({ "command": "transport-plan" }),
        repair_hints: vec![
            "Check that --root points to a writable 1Context directory.".to_string(),
        ],
    })?;

    let at = utc_now_rfc3339();
    let request_payload = transport_plan_request_payload(&request);
    let unit_id = unit
        .as_ref()
        .map(|unit| unit.unit_id.0.clone())
        .unwrap_or_else(|| "transport-plan".to_string());
    let transport_plan = build_transport_plan(unit.as_ref(), &request);
    let receipt = protocol_receipt(
        "transport-plan",
        &unit_id,
        &at,
        "transport_plan",
        "planned harness-owned capability transport bindings",
        json!({
            "request": request_payload,
            "transport_plan": transport_plan,
            "core_method_expected_for_persistence": "AgentHarnessStore::plan_transport(TransportPlanRequest) -> Result<TransportPlan, HarnessError>",
        }),
    );

    Ok(json!({
        "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
        "status": "ok",
        "surface": "agent_harness_protocol",
        "operation": "agent.harness.transport-plan",
        "at": at,
        "unit_id": unit.as_ref().map(|unit| unit.unit_id.0.clone()),
        "request": request_payload,
        "transport_plan": transport_plan,
        "receipt": receipt,
        "compatibility": {
            "status": "daemon_planned",
            "core_method_expected_for_persistence": "AgentHarnessStore::plan_transport(TransportPlanRequest) -> Result<TransportPlan, HarnessError>"
        }
    }))
}

fn unit_payload(command: &str, unit: AgentHarnessUnit) -> Value {
    let unit_id = unit.unit_id.0.clone();
    let receipt = unit.receipts.last().cloned();
    json!({
        "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
        "status": "ok",
        "surface": "agent_harness_protocol",
        "operation": operation_name(command),
        "at": utc_now_rfc3339(),
        "unit_id": unit_id,
        "unit": unit,
        "receipt": receipt,
    })
}

fn frontier_scaffold_payload(
    command: &str,
    at: String,
    unit_id: String,
    request: Value,
    receipt: Value,
    expected_core_method: &str,
) -> Value {
    json!({
        "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
        "status": "scaffold",
        "surface": "agent_harness_protocol",
        "operation": operation_name(command),
        "at": at,
        "unit_id": unit_id,
        "request": request,
        "receipt": receipt,
        "feature_gate": {
            "status": "blocked",
            "reason": "onecontext-agent-harness-core does not expose the durable store mutation API yet",
            "owner_lane": "rust-core-store-and-invariants",
            "expected_core_method": expected_core_method,
            "daemon_behavior": "parsed typed request, verified active unit, and emitted stable compatibility receipt"
        }
    })
}

fn agent_status_payload(unit: AgentHarnessUnit) -> Value {
    let unit_id = unit.unit_id.0.clone();
    let adapter_events = persisted_adapter_events(&unit);
    let proof_status = summarize_proof_status(&unit.certificate.capabilities, &adapter_events);
    json!({
        "schema_version": AGENT_HARNESS_SCHEMA_VERSION,
        "status": "ok",
        "surface": "agent_harness_protocol",
        "operation": "agent.harness.agent-status",
        "at": utc_now_rfc3339(),
        "unit_id": unit_id,
        "certificate": &unit.certificate,
        "lifecycle": {
            "state": &unit.lifecycle_state,
            "availability": &unit.metadata.availability,
            "session_id": &unit.session_id,
            "active_turn_id": &unit.metadata.active_turn_id,
            "turns_started": unit.metadata.turns_started,
            "turns_completed": unit.metadata.turns_completed,
        },
        "lineage": {
            "parent_unit_id": unit.certificate.lineage.parent_unit_id.as_ref().map(|unit_id| unit_id.0.clone()),
            "root_unit_id": &unit.certificate.lineage.root_unit_id.0,
            "spawn_request_id": &unit.certificate.lineage.spawn_request_id,
        },
        "turns": turn_status_payload(&unit),
        "usage": usage_metadata_payload(&unit),
        "capabilities": &unit.certificate.capabilities,
        "adapter_evidence": {
            "events": adapter_events,
            "persisted_event_count": adapter_events.len(),
            "source": "unit.receipts[].evidence.adapter_event | unit.receipts[].evidence.event | unit.receipts[].evidence"
        },
        "proof_status": proof_status,
        "receipts": &unit.receipts,
    })
}

fn proof_status_payload(unit: &AgentHarnessUnit) -> Value {
    let adapter_events = persisted_adapter_events(unit);
    json!(summarize_proof_status(
        &unit.certificate.capabilities,
        &adapter_events
    ))
}

fn usage_metadata_payload(unit: &AgentHarnessUnit) -> Value {
    json!({
        "input_tokens": unit.metadata.input_tokens,
        "output_tokens": unit.metadata.output_tokens,
        "total_tokens": unit.metadata.total_tokens,
        "total_duration_ms": unit.metadata.total_duration_ms,
        "usage_receipts": receipts_matching(
            unit,
            &[ReceiptKind::UsageUpdated, ReceiptKind::TurnCompleted]
        ),
    })
}

fn turn_status_payload(unit: &AgentHarnessUnit) -> Value {
    json!({
        "active_turn_id": &unit.metadata.active_turn_id,
        "turns_started": unit.metadata.turns_started,
        "turns_completed": unit.metadata.turns_completed,
        "turn_receipts": receipts_matching(
            unit,
            &[ReceiptKind::TurnStarted, ReceiptKind::TurnCompleted]
        ),
    })
}

fn receipts_matching(unit: &AgentHarnessUnit, kinds: &[ReceiptKind]) -> Vec<Value> {
    unit.receipts
        .iter()
        .filter(|receipt| kinds.iter().any(|kind| kind == &receipt.kind))
        .map(|receipt| {
            json!({
                "id": &receipt.id,
                "at": receipt.at,
                "kind": &receipt.kind,
                "turn_id": receipt.evidence.get("turn_id").cloned().unwrap_or(Value::Null),
                "usage": receipt.evidence.get("usage").cloned().unwrap_or(Value::Null),
                "evidence": &receipt.evidence,
            })
        })
        .collect()
}

fn persisted_adapter_events(unit: &AgentHarnessUnit) -> Vec<AdapterEvent> {
    let mut events = unit.adapter_events.clone();
    for receipt in &unit.receipts {
        for candidate in adapter_event_candidates(&receipt.evidence) {
            if let Ok(event) = serde_json::from_value::<AdapterEvent>(candidate) {
                if !events.iter().any(|existing| existing.id == event.id) {
                    events.push(event);
                }
                break;
            }
        }
    }
    events
}

fn adapter_event_candidates(evidence: &Value) -> Vec<Value> {
    ["adapter_event", "event"]
        .iter()
        .filter_map(|key| evidence.get(*key).cloned())
        .chain(std::iter::once(evidence.clone()))
        .collect()
}

fn build_transport_plan(unit: Option<&AgentHarnessUnit>, request: &TransportPlanRequest) -> Value {
    let requested = json!(&request.requested_transports);
    let Some(unit) = unit else {
        return json!({
            "status": "request_only",
            "harness_owned": harness_owned_transport_surfaces(),
            "external_bindings": external_transport_bindings(),
            "requested_transports": requested,
            "capabilities": [],
        });
    };

    let capabilities = unit
        .certificate
        .capabilities
        .iter()
        .filter(|binding| {
            request
                .capability_id
                .as_ref()
                .map(|capability_id| capability_id == &binding.id)
                .unwrap_or(true)
        })
        .map(|binding| {
            let requested_match = request.requested_transports.is_empty()
                || request
                    .requested_transports
                    .iter()
                    .any(|transport| transport == &binding.transport);
            json!({
                "capability_id": &binding.id,
                "declared_transport": &binding.transport,
                "selected_transport": if requested_match { json!(&binding.transport) } else { Value::Null },
                "tool_names": &binding.tool_names,
                "proof_required": &binding.proof_required,
                "status": if requested_match { "planned" } else { "transport_not_declared_for_capability" },
            })
        })
        .collect::<Vec<_>>();

    json!({
        "status": "planned",
        "unit_id": unit.unit_id.0,
        "harness_owned": harness_owned_transport_surfaces(),
        "external_bindings": external_transport_bindings(),
        "requested_transports": requested,
        "capabilities": capabilities,
    })
}

fn harness_owned_transport_surfaces() -> Vec<&'static str> {
    vec![
        "capability declarations",
        "adapter evidence receipts",
        "proof status",
        "turn lifecycle receipts",
        "usage receipts",
    ]
}

fn external_transport_bindings() -> Vec<&'static str> {
    vec![
        "mcp",
        "codex_app_server_dynamic_tool",
        "codex_skill",
        "codex_plugin",
        "codex_connector",
        "codex_app",
        "host_hook",
        "local_test",
    ]
}

fn inventory_entry_payload(entry: &AgentHarnessInventoryEntry) -> Value {
    json!({
        "unit_id": &entry.unit_id.0,
        "parent_unit_id": entry.parent_unit_id.as_ref().map(|unit_id| unit_id.0.clone()),
        "role": &entry.role,
        "model": &entry.model,
        "lifecycle_state": &entry.lifecycle_state,
        "availability": &entry.availability,
        "called_at": entry.called_at,
        "last_active_at": entry.last_active_at,
        "retired_at": entry.retired_at,
    })
}

fn load_unit(
    paths: &AgentHarnessPaths,
    command: &str,
    unit_id: &AgentUnitId,
) -> Result<AgentHarnessUnit, CliError> {
    let store = AgentHarnessStore::from_paths(paths.clone());
    let snapshot = store
        .load()
        .map_err(|error| CliError::from_harness_error(command, error))?;
    snapshot
        .units
        .get(unit_id)
        .cloned()
        .ok_or_else(|| CliError::unit_not_found(command, unit_id))
}

fn load_active_unit(
    paths: &AgentHarnessPaths,
    command: &str,
    unit_id: &AgentUnitId,
) -> Result<AgentHarnessUnit, CliError> {
    let unit = load_unit(paths, command, unit_id)?;
    if unit.metadata.availability == AgentAvailability::Retired {
        return Err(CliError::retired_unit(command, &unit.unit_id));
    }
    Ok(unit)
}

#[derive(Clone, Copy)]
enum RequestShape {
    AnyObject,
    Call,
    Unit,
}

fn validate_request(command: &str, request: Value, shape: RequestShape) -> Result<Value, CliError> {
    let object = match request {
        Value::Object(object) => object,
        _ => {
            return Err(CliError::invalid_request(
                command,
                "Request JSON must be an object.",
                vec!["Use an object such as {\"unit_id\":\"agent-...\"}.".to_string()],
            ))
        }
    };

    match shape {
        RequestShape::AnyObject => {}
        RequestShape::Call => {
            require_string(command, &object, "role")?;
            require_string(command, &object, "model")?;
            require_string(command, &object, "visibility")?;
        }
        RequestShape::Unit => {
            require_string(command, &object, "unit_id")?;
        }
    }

    Ok(Value::Object(object))
}

fn unit_id_from_request(command: &str, request: &Value) -> Result<AgentUnitId, CliError> {
    request
        .get("unit_id")
        .and_then(Value::as_str)
        .map(|unit_id| AgentUnitId(unit_id.to_string()))
        .ok_or_else(|| {
            CliError::invalid_request(
                command,
                "Request JSON is missing required string field: unit_id",
                vec!["Add a non-empty unit_id string to the request JSON.".to_string()],
            )
        })
}

fn require_string(command: &str, object: &Map<String, Value>, key: &str) -> Result<(), CliError> {
    if has_string(object, key) {
        return Ok(());
    }
    Err(CliError::invalid_request(
        command,
        format!("Request JSON is missing required string field: {key}"),
        vec![format!("Add a non-empty {key} string to the request JSON.")],
    ))
}

fn has_string(object: &Map<String, Value>, key: &str) -> bool {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

fn operation_name(command: &str) -> String {
    format!("agent.harness.{command}")
}

fn protocol_error(command: &str, error: ProtocolError) -> CliError {
    CliError::invalid_request(command, error.message, error.repair_hints)
}

fn take_required_flag_value(
    args: &mut Vec<String>,
    flag: &str,
) -> Result<Option<String>, CliError> {
    let Some(position) = args.iter().position(|arg| arg == flag) else {
        return Ok(None);
    };
    args.remove(position);
    if position >= args.len() {
        return Err(CliError::invalid_arguments(
            format!("{flag} requires a value"),
            vec![format!("Pass {flag} followed by a path.")],
        ));
    }
    Ok(Some(args.remove(position)))
}

fn take_request(args: &mut Vec<String>) -> Result<Value, CliError> {
    let mut request = None;
    for flag in ["--request-json", "--json", "--request"] {
        if let Some(raw) = take_required_flag_value(args, flag)? {
            set_request(&mut request, parse_request_json(&raw, flag)?)?;
        }
    }
    for flag in ["--request-file", "--request-path"] {
        if let Some(path) = take_required_flag_value(args, flag)? {
            let raw = fs::read_to_string(&path).map_err(|error| CliError {
                exit_code: 2,
                code: "agent_harness_request_file_read_failed".to_string(),
                message: error.to_string(),
                details: json!({ "path": path }),
                repair_hints: vec![
                    "Check that the request file exists and is readable.".to_string()
                ],
            })?;
            set_request(&mut request, parse_request_json(&raw, flag)?)?;
        }
    }

    if request.is_none() && args.len() > 1 {
        if let Some(raw) = args.last() {
            if looks_like_json(raw) {
                let raw = args.pop().unwrap();
                set_request(&mut request, parse_request_json(&raw, "positional-json")?)?;
            }
        }
    }

    Ok(request.unwrap_or_else(|| json!({})))
}

fn set_request(target: &mut Option<Value>, value: Value) -> Result<(), CliError> {
    if target.is_some() {
        return Err(CliError::invalid_arguments(
            "Multiple request JSON sources were provided.".to_string(),
            vec![
                "Use exactly one of --request-json, --request-file, or trailing inline JSON."
                    .to_string(),
            ],
        ));
    }
    *target = Some(value);
    Ok(())
}

fn parse_request_json(raw: &str, source: &str) -> Result<Value, CliError> {
    serde_json::from_str(raw).map_err(|error| CliError {
        exit_code: 2,
        code: "agent_harness_invalid_request_json".to_string(),
        message: error.to_string(),
        details: json!({ "source": source }),
        repair_hints: vec!["Pass valid UTF-8 JSON request data.".to_string()],
    })
}

fn looks_like_json(raw: &str) -> bool {
    let trimmed = raw.trim_start();
    trimmed.starts_with('{') || trimmed.starts_with('[')
}

fn normalize_command(command: &str) -> String {
    command.replace('_', "-").to_ascii_lowercase()
}

fn print_json(payload: &Value) {
    println!(
        "{}",
        serde_json::to_string_pretty(payload).unwrap_or_else(|_| {
            "{\"schema_version\":1,\"status\":\"error\",\"surface\":\"agent_harness\"}".to_string()
        })
    );
}

#[derive(Debug)]
struct CliError {
    exit_code: i32,
    code: String,
    message: String,
    details: Value,
    repair_hints: Vec<String>,
}

impl CliError {
    fn invalid_arguments(message: String, repair_hints: Vec<String>) -> Self {
        Self {
            exit_code: 2,
            code: "agent_harness_invalid_arguments".to_string(),
            message,
            details: json!({}),
            repair_hints,
        }
    }

    fn invalid_request(
        command: &str,
        message: impl Into<String>,
        repair_hints: Vec<String>,
    ) -> Self {
        Self {
            exit_code: 2,
            code: "agent_harness_invalid_request".to_string(),
            message: message.into(),
            details: json!({ "command": command }),
            repair_hints,
        }
    }

    fn unit_not_found(command: &str, unit_id: &AgentUnitId) -> Self {
        Self {
            exit_code: 2,
            code: "agent_harness_unit_not_found".to_string(),
            message: format!("Agent harness unit not found: {}", unit_id.0),
            details: json!({ "command": command, "unit_id": unit_id.0 }),
            repair_hints: vec![
                "Call agent.harness.agents to inspect known active and retired units.".to_string(),
            ],
        }
    }

    fn retired_unit(command: &str, unit_id: &AgentUnitId) -> Self {
        Self {
            exit_code: 2,
            code: "agent_harness_unit_retired".to_string(),
            message: format!("Agent harness unit is retired: {}", unit_id.0),
            details: json!({ "command": command, "unit_id": unit_id.0 }),
            repair_hints: vec![
                "Create a new agent unit instead of mutating a retired unit.".to_string(),
            ],
        }
    }

    fn from_harness_error(command: &str, error: HarnessError) -> Self {
        match error {
            HarnessError::UnitNotFound(unit_id) => Self::unit_not_found(command, &unit_id),
            HarnessError::UnitRetired(unit_id) => Self::retired_unit(command, &unit_id),
            HarnessError::UnitAlreadyExists(unit_id) => Self {
                exit_code: 2,
                code: "agent_harness_unit_already_exists".to_string(),
                message: format!("Agent harness unit already exists: {}", unit_id.0),
                details: json!({ "command": command, "unit_id": unit_id.0 }),
                repair_hints: vec![
                    "Reuse the same call request for idempotency, or choose a different unit_id."
                        .to_string(),
                ],
            },
            HarnessError::TurnAlreadyActive {
                unit_id,
                active_turn_id,
            } => Self {
                exit_code: 2,
                code: "agent_harness_turn_already_active".to_string(),
                message: format!(
                    "Agent harness unit {} already has active turn {}",
                    unit_id.0, active_turn_id.0
                ),
                details: json!({
                    "command": command,
                    "unit_id": unit_id.0,
                    "active_turn_id": active_turn_id.0
                }),
                repair_hints: vec![
                    "Complete the active turn before starting another turn.".to_string(),
                ],
            },
            HarnessError::NoActiveTurn { unit_id } => Self {
                exit_code: 2,
                code: "agent_harness_no_active_turn".to_string(),
                message: format!("Agent harness unit {} has no active turn", unit_id.0),
                details: json!({ "command": command, "unit_id": unit_id.0 }),
                repair_hints: vec![
                    "Start a turn before calling complete-turn.".to_string(),
                ],
            },
            HarnessError::TurnMismatch {
                unit_id,
                active_turn_id,
                requested_turn_id,
            } => Self {
                exit_code: 2,
                code: "agent_harness_turn_mismatch".to_string(),
                message: format!(
                    "Agent harness unit {} active turn {} does not match requested turn {}",
                    unit_id.0, active_turn_id.0, requested_turn_id.0
                ),
                details: json!({
                    "command": command,
                    "unit_id": unit_id.0,
                    "active_turn_id": active_turn_id.0,
                    "requested_turn_id": requested_turn_id.0
                }),
                repair_hints: vec![
                    "Complete the active turn id reported by agent-status.".to_string(),
                ],
            },
            HarnessError::InvalidInput(message) => Self::invalid_request(
                command,
                message,
                vec!["Check the request against the agent harness schema.".to_string()],
            ),
            HarnessError::Json(error) => Self::invalid_request(
                command,
                error.to_string(),
                vec!["Check the request or store JSON syntax.".to_string()],
            ),
            HarnessError::StoreLocked(path) => Self {
                exit_code: 3,
                code: "agent_harness_store_locked".to_string(),
                message: format!("Agent harness store is locked by {}", path.display()),
                details: json!({ "command": command, "path": path }),
                repair_hints: vec![
                    "Retry after the in-flight harness operation completes.".to_string(),
                ],
            },
            HarnessError::CorruptStore { path, message } => Self {
                exit_code: 3,
                code: "agent_harness_corrupt_store".to_string(),
                message,
                details: json!({ "command": command, "path": path }),
                repair_hints: vec![
                    "Inspect agent-harness.json and receipt artifacts before retrying.".to_string(),
                ],
            },
            HarnessError::NotImplemented(operation) => Self {
                exit_code: 3,
                code: "agent_harness_core_not_implemented".to_string(),
                message: format!("Core store operation is not implemented: {operation}"),
                details: json!({ "command": command, "operation": operation }),
                repair_hints: vec![
                    "Route this command through the daemon scaffold feature gate until the core API lands."
                        .to_string(),
                ],
            },
            HarnessError::Io(error) => Self {
                exit_code: 1,
                code: "agent_harness_io_failed".to_string(),
                message: error.to_string(),
                details: json!({ "command": command }),
                repair_hints: vec![
                    "Check that the runtime root is writable and the harness store is accessible."
                        .to_string(),
                ],
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn parses_inline_request_json_for_call() {
        let root = temp_root("inline-call");
        let payload = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "call".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-inline-1","role":"researcher","model":"gpt-5","visibility":"private"}"#
                .to_string(),
        ])
        .expect("call receipt");

        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["operation"], "agent.harness.call");
        assert_eq!(payload["unit_id"], "agent-inline-1");
        assert_eq!(payload["receipt"]["kind"], "agent_called");
        assert!(root
            .join("context-engine/agents/harness/birth-certificates")
            .exists());
    }

    #[test]
    fn parses_request_json_from_file() {
        let root = temp_root("file-request");
        create_unit(&root, "agent-1");
        let request_file = root.join("request.json");
        fs::create_dir_all(&root).expect("temp root");
        fs::write(&request_file, r#"{"unit_id":"agent-1"}"#).expect("request file");

        let payload = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "agent-status".to_string(),
            "--request-file".to_string(),
            request_file.display().to_string(),
        ])
        .expect("agent status scaffold");

        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["unit_id"], "agent-1");
        assert_eq!(payload["lifecycle"]["state"], "born");
    }

    #[test]
    fn rejects_bad_request_json_with_structured_error() {
        let error = run(vec![
            "call".to_string(),
            "--request-json".to_string(),
            "{not-json".to_string(),
        ])
        .expect_err("bad json should fail");

        assert_eq!(error.exit_code, 2);
        assert_eq!(error.code, "agent_harness_invalid_request_json");
        assert_eq!(error.details["source"], "--request-json");
    }

    #[test]
    fn rejects_missing_unit_id_for_unit_commands() {
        let error = run(vec![
            "start-turn".to_string(),
            "--request-json".to_string(),
            r#"{"turn_id":"turn-1"}"#.to_string(),
        ])
        .expect_err("unit command requires unit id");

        assert_eq!(error.code, "agent_harness_invalid_request");
        assert_eq!(error.details["command"], "start-turn");
        assert!(error.message.contains("unit_id"));
    }

    #[test]
    fn agents_reports_lifecycle_buckets_from_core_inventory() {
        let root = temp_root("agents");
        create_unit(&root, "agent-active-1");
        let payload = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "agents".to_string(),
        ])
        .expect("agents");

        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["agents"]["counts"]["active"], 1);
        assert_eq!(payload["agents"]["counts"]["waiting"], 0);
        assert_eq!(payload["agents"]["counts"]["blocked"], 0);
        assert_eq!(payload["agents"]["counts"]["done"], 0);
        assert_eq!(payload["agents"]["counts"]["retired"], 0);
    }

    #[test]
    fn validates_adapter_event_request_shape() {
        let root = temp_root("adapter-event");
        create_unit(&root, "agent-1");
        let payload = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "record_adapter_event".to_string(),
            r#"{"unit_id":"agent-1","adapter":"local_test","kind":"context_injection_executed","status":"observed"}"#
                .to_string(),
        ])
        .expect("adapter event");

        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["operation"], "agent.harness.record-adapter-event");
        assert_eq!(payload["receipt"]["kind"], "proof_observed");
    }

    #[test]
    fn start_and_complete_turn_update_lifecycle_and_usage() {
        let root = temp_root("turn-lifecycle");
        create_unit(&root, "agent-turn-1");

        let started = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "start-turn".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-turn-1","turn_id":"turn-1","metadata":{"source":"test"}}"#
                .to_string(),
        ])
        .expect("start turn");

        assert_eq!(started["status"], "ok");
        assert_eq!(started["receipt"]["kind"], "turn_started");
        assert_eq!(started["unit"]["metadata"]["active_turn_id"], "turn-1");

        let completed = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "complete-turn".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-turn-1","turn_id":"turn-1","usage":{"input_tokens":7,"output_tokens":11},"duration_ms":30,"outcome":"done"}"#
                .to_string(),
        ])
        .expect("complete turn");

        assert_eq!(completed["status"], "ok");
        assert_eq!(completed["receipt"]["kind"], "turn_completed");
        assert_eq!(completed["unit"]["metadata"]["turns_started"], 1);
        assert_eq!(completed["unit"]["metadata"]["turns_completed"], 1);
        assert_eq!(completed["unit"]["metadata"]["total_tokens"], 18);

        let status = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "agent-status".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-turn-1"}"#.to_string(),
        ])
        .expect("agent status");

        assert_eq!(status["usage"]["total_tokens"], 18);
        assert_eq!(status["turns"]["turns_completed"], 1);
        assert_eq!(
            status["turns"]["turn_receipts"].as_array().unwrap().len(),
            2
        );
    }

    #[test]
    fn record_adapter_event_feeds_agent_status_proof_summary() {
        let root = temp_root("proof-status");
        create_unit_with_context_proof(&root, "agent-proof-1");

        let recorded = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "record-adapter-event".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-proof-1","adapter":"local_test","kind":"context_injection_executed","status":"accepted","correlation":{"turn_id":"turn-proof"},"evidence":{"summary":"context injected"}}"#
                .to_string(),
        ])
        .expect("record adapter event");

        assert_eq!(recorded["status"], "ok");
        assert_eq!(recorded["receipt"]["kind"], "proof_observed");

        let status = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "agent-status".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-proof-1"}"#.to_string(),
        ])
        .expect("agent status");

        assert_eq!(status["adapter_evidence"]["persisted_event_count"], 1);
        assert_eq!(status["proof_status"]["gate_status"], "satisfied");
        assert_eq!(
            status["proof_status"]["missing"].as_array().unwrap().len(),
            0
        );
    }

    #[test]
    fn rejects_bad_adapter_event_enum_with_structured_error() {
        let root = temp_root("bad-adapter-event");
        create_unit(&root, "agent-1");
        let error = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "record-adapter-event".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-1","adapter":"local_test","kind":"not_real","status":"observed"}"#
                .to_string(),
        ])
        .expect_err("bad adapter kind should fail");

        assert_eq!(error.code, "agent_harness_invalid_request");
        assert_eq!(error.details["command"], "record-adapter-event");
        assert!(error.message.contains("kind"));
    }

    #[test]
    fn transport_plan_projects_declared_capabilities() {
        let root = temp_root("transport-plan");
        create_unit_with_context_proof(&root, "agent-plan-1");

        let payload = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "transport-plan".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-plan-1","requested_transports":["local_test","codex_plugin"]}"#
                .to_string(),
        ])
        .expect("transport plan");

        assert_eq!(payload["status"], "ok");
        assert_eq!(payload["transport_plan"]["status"], "planned");
        assert_eq!(
            payload["transport_plan"]["capabilities"][0]["selected_transport"],
            "local_test"
        );
        let external_bindings = payload["transport_plan"]["external_bindings"]
            .as_array()
            .unwrap();
        assert!(external_bindings.iter().any(|item| item == "codex_plugin"));
    }

    #[test]
    fn reports_missing_unit_with_stable_error_code() {
        let root = temp_root("missing-unit");
        let error = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "agent-status".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"missing-agent"}"#.to_string(),
        ])
        .expect_err("missing unit should fail");

        assert_eq!(error.code, "agent_harness_unit_not_found");
        assert_eq!(error.details["unit_id"], "missing-agent");
    }

    #[test]
    fn rejects_lifecycle_scaffold_command_for_retired_unit() {
        let root = temp_root("retired-unit");
        create_unit(&root, "agent-retired-1");
        run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "retire".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-retired-1","reason":"test"}"#.to_string(),
        ])
        .expect("retire unit");

        let error = run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "start-turn".to_string(),
            "--request-json".to_string(),
            r#"{"unit_id":"agent-retired-1"}"#.to_string(),
        ])
        .expect_err("retired unit should fail");

        assert_eq!(error.code, "agent_harness_unit_retired");
        assert_eq!(error.details["unit_id"], "agent-retired-1");
    }

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("onecontext-agent-harness-daemon-{label}-{nanos}"));
        remove_if_exists(&path);
        path
    }

    fn remove_if_exists(path: &Path) {
        if path.exists() {
            fs::remove_dir_all(path).expect("clean temp root");
        }
    }

    fn create_unit(root: &Path, unit_id: &str) {
        run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "call".to_string(),
            "--request-json".to_string(),
            format!(
                r#"{{"unit_id":"{unit_id}","role":"researcher","model":"gpt-5","visibility":"private"}}"#
            ),
        ])
        .expect("create unit");
    }

    fn create_unit_with_context_proof(root: &Path, unit_id: &str) {
        run(vec![
            "--root".to_string(),
            root.display().to_string(),
            "call".to_string(),
            "--request-json".to_string(),
            format!(
                r#"{{
                    "unit_id":"{unit_id}",
                    "role":"researcher",
                    "model":"gpt-5",
                    "visibility":"private",
                    "capabilities":[{{
                        "id":"context-writer",
                        "transport":"local_test",
                        "tool_names":["context.write"],
                        "proof_required":["context_injection"]
                    }}]
                }}"#
            ),
        ])
        .expect("create unit with proof requirement");
    }
}
