use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use onecontext_agent_harness_core::{
    AdapterCorrelation, AdapterEventKind, AdapterEventRequest, AdapterEventStatus, AdapterKind,
    AgentUnitId,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::app_server_client::AppServerMethod;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveAppServerDogfoodPhase {
    SpawnAppServer,
    GenerateSchema,
    Initialize,
    HarnessParentBirth,
    ThreadStart,
    HarnessChildBirth,
    TurnStart,
    ThreadInjectItems,
    TurnSteer,
    ThreadLoadedList,
    RecordHarnessProof,
    SummarizeEvidence,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveAppServerDogfoodPlan {
    pub kind: String,
    pub phases: Vec<LiveAppServerDogfoodPhase>,
    pub codex_command: Vec<String>,
    pub schema_command: Vec<String>,
    pub listen_url: String,
    pub required_methods: Vec<String>,
    pub proof_categories: Vec<String>,
    pub artifacts: LiveAppServerDogfoodArtifacts,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveAppServerDogfoodArtifacts {
    pub evidence_dir: PathBuf,
    pub schema_dir: PathBuf,
    pub runtime_root: PathBuf,
    pub transcript_jsonl: PathBuf,
    pub proof_summary_json: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveAppServerDogfoodRequest {
    pub evidence_dir: PathBuf,
    pub runtime_root: PathBuf,
    #[serde(default = "default_codex_bin")]
    pub codex_bin: String,
    #[serde(default = "default_listen_url")]
    pub listen_url: String,
}

impl LiveAppServerDogfoodRequest {
    pub fn plan(&self) -> LiveAppServerDogfoodPlan {
        let schema_dir = self.evidence_dir.join("generated-schemas");
        LiveAppServerDogfoodPlan {
            kind: "onecontext.codex_adapter.live_app_server_dogfood_plan".to_string(),
            phases: live_app_server_dogfood_phases(),
            codex_command: vec![
                self.codex_bin.clone(),
                "app-server".to_string(),
                "--listen".to_string(),
                self.listen_url.clone(),
            ],
            schema_command: vec![
                self.codex_bin.clone(),
                "app-server".to_string(),
                "generate-json-schema".to_string(),
                "--experimental".to_string(),
                "--out".to_string(),
                schema_dir.display().to_string(),
            ],
            listen_url: self.listen_url.clone(),
            required_methods: live_required_methods()
                .iter()
                .map(|method| method.wire_name().to_string())
                .collect(),
            proof_categories: vec![
                "transport_identity".to_string(),
                "steering".to_string(),
                "context_injection".to_string(),
                "tool_conformance".to_string(),
                "hooks".to_string(),
                "dispatch_liveness".to_string(),
            ],
            artifacts: LiveAppServerDogfoodArtifacts {
                evidence_dir: self.evidence_dir.clone(),
                schema_dir,
                runtime_root: self.runtime_root.clone(),
                transcript_jsonl: self.evidence_dir.join("app-server-transcript.jsonl"),
                proof_summary_json: self.evidence_dir.join("proof-summary.json"),
            },
        }
    }
}

impl Default for LiveAppServerDogfoodRequest {
    fn default() -> Self {
        Self {
            evidence_dir: PathBuf::from("test-results/codex-adapter-live-server-dogfood"),
            runtime_root: PathBuf::from(
                "test-results/codex-adapter-live-server-dogfood/runtime/1Context",
            ),
            codex_bin: default_codex_bin(),
            listen_url: default_listen_url(),
        }
    }
}

fn default_codex_bin() -> String {
    "codex".to_string()
}

fn default_listen_url() -> String {
    "stdio://".to_string()
}

pub fn live_app_server_dogfood_phases() -> Vec<LiveAppServerDogfoodPhase> {
    vec![
        LiveAppServerDogfoodPhase::SpawnAppServer,
        LiveAppServerDogfoodPhase::GenerateSchema,
        LiveAppServerDogfoodPhase::Initialize,
        LiveAppServerDogfoodPhase::HarnessParentBirth,
        LiveAppServerDogfoodPhase::ThreadStart,
        LiveAppServerDogfoodPhase::HarnessChildBirth,
        LiveAppServerDogfoodPhase::TurnStart,
        LiveAppServerDogfoodPhase::ThreadInjectItems,
        LiveAppServerDogfoodPhase::TurnSteer,
        LiveAppServerDogfoodPhase::ThreadLoadedList,
        LiveAppServerDogfoodPhase::RecordHarnessProof,
        LiveAppServerDogfoodPhase::SummarizeEvidence,
    ]
}

pub fn live_required_methods() -> &'static [AppServerMethod] {
    &[
        AppServerMethod::Initialize,
        AppServerMethod::ThreadStart,
        AppServerMethod::TurnStart,
        AppServerMethod::ThreadInjectItems,
        AppServerMethod::TurnSteer,
        AppServerMethod::ThreadLoadedList,
    ]
}

pub fn json_rpc_request(id: impl Into<Value>, method: AppServerMethod, params: Value) -> Value {
    json!({
        "id": id.into(),
        "method": method.wire_name(),
        "params": params,
    })
}

pub fn initialize_request(id: impl Into<Value>, experimental_api: bool) -> Value {
    json_rpc_request(
        id,
        AppServerMethod::Initialize,
        json!({
            "clientInfo": {
                "name": "onecontext-codex-adapter-live-dogfood",
                "title": "1Context Codex Adapter Live Dogfood",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "capabilities": {
                "experimentalApi": experimental_api,
            },
        }),
    )
}

pub fn thread_start_request(
    id: impl Into<Value>,
    cwd: impl Into<PathBuf>,
    model: impl Into<String>,
) -> Value {
    json_rpc_request(
        id,
        AppServerMethod::ThreadStart,
        json!({
            "cwd": cwd.into(),
            "model": model.into(),
            "approvalPolicy": "never",
            "sandbox": "workspace-write",
            "ephemeral": true,
            "baseInstructions": "You are a live 1Context Codex adapter dogfood worker.",
            "developerInstructions": "Keep output short. Do not mutate files unless explicitly asked by the live dogfood turn.",
            "config": {
                "onecontext_adapter_dogfood": true,
            },
            "persistExtendedHistory": true,
        }),
    )
}

pub fn turn_start_request(
    id: impl Into<Value>,
    thread_id: impl Into<String>,
    text: impl Into<String>,
) -> Value {
    json_rpc_request(
        id,
        AppServerMethod::TurnStart,
        json!({
            "threadId": thread_id.into(),
            "input": [{
                "type": "text",
                "text": text.into(),
            }],
        }),
    )
}

pub fn thread_inject_items_request(
    id: impl Into<Value>,
    thread_id: impl Into<String>,
    items: Vec<Value>,
) -> Value {
    json_rpc_request(
        id,
        AppServerMethod::ThreadInjectItems,
        json!({
            "threadId": thread_id.into(),
            "items": items,
        }),
    )
}

pub fn turn_steer_request(
    id: impl Into<Value>,
    thread_id: impl Into<String>,
    expected_turn_id: impl Into<String>,
    text: impl Into<String>,
) -> Value {
    json_rpc_request(
        id,
        AppServerMethod::TurnSteer,
        json!({
            "threadId": thread_id.into(),
            "expectedTurnId": expected_turn_id.into(),
            "input": [{
                "type": "text",
                "text": text.into(),
            }],
        }),
    )
}

pub fn thread_loaded_list_request(id: impl Into<Value>, limit: u32) -> Value {
    json_rpc_request(
        id,
        AppServerMethod::ThreadLoadedList,
        json!({
            "limit": limit,
        }),
    )
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveAppServerSchemaAvailability {
    pub available_methods: BTreeSet<AppServerMethod>,
    pub missing_required_methods: BTreeSet<AppServerMethod>,
    pub source_count: u64,
    pub warnings: Vec<String>,
}

impl LiveAppServerSchemaAvailability {
    fn from_available_methods(
        available_methods: BTreeSet<AppServerMethod>,
        source_count: u64,
        warnings: Vec<String>,
    ) -> Self {
        let missing_required_methods = live_required_methods()
            .iter()
            .copied()
            .filter(|method| !available_methods.contains(method))
            .collect();

        Self {
            available_methods,
            missing_required_methods,
            source_count,
            warnings,
        }
    }

    pub fn supports_live_required_methods(&self) -> bool {
        self.missing_required_methods.is_empty()
    }
}

pub fn live_schema_availability_from_file_names<I, S>(
    file_names: I,
) -> LiveAppServerSchemaAvailability
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut available_methods = BTreeSet::new();
    let mut source_count = 0;

    for file_name in file_names {
        source_count += 1;
        let Some(method) = method_from_schema_file_name(file_name.as_ref()) else {
            continue;
        };
        available_methods.insert(method);
    }

    LiveAppServerSchemaAvailability::from_available_methods(available_methods, source_count, vec![])
}

pub fn live_schema_availability_from_bundle_json(
    bundle: &Value,
) -> LiveAppServerSchemaAvailability {
    let mut available_methods = BTreeSet::new();
    let mut source_count = 0;
    collect_schema_methods_from_value(bundle, &mut available_methods, &mut source_count);

    let warnings = if source_count == 0 {
        vec!["schema bundle did not mention any known app-server methods".to_string()]
    } else {
        vec![]
    };

    LiveAppServerSchemaAvailability::from_available_methods(
        available_methods,
        source_count,
        warnings,
    )
}

fn method_from_schema_file_name(file_name: &str) -> Option<AppServerMethod> {
    let file_name = Path::new(file_name).file_name()?.to_str()?;
    let stem = file_name
        .strip_suffix(".schema.json")
        .or_else(|| file_name.strip_suffix(".json"))
        .unwrap_or(file_name);
    method_from_loose_name(stem)
}

fn collect_schema_methods_from_value(
    value: &Value,
    available_methods: &mut BTreeSet<AppServerMethod>,
    source_count: &mut u64,
) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                if let Some(method) = method_from_loose_name(key) {
                    if available_methods.insert(method) {
                        *source_count += 1;
                    }
                }
                collect_schema_methods_from_value(value, available_methods, source_count);
            }
        }
        Value::Array(values) => {
            for value in values {
                collect_schema_methods_from_value(value, available_methods, source_count);
            }
        }
        Value::String(value) => {
            if let Some(method) = method_from_loose_name(value) {
                if available_methods.insert(method) {
                    *source_count += 1;
                }
            }
        }
        _ => {}
    }
}

fn method_from_loose_name(value: &str) -> Option<AppServerMethod> {
    let normalized = normalize_method_name(value);
    required_app_server_methods_for_parsing()
        .iter()
        .copied()
        .find(|method| {
            let method_name = normalize_method_name(method.wire_name());
            normalized == method_name
                || normalized.ends_with(&method_name)
                || normalized.contains(&method_name)
        })
}

fn required_app_server_methods_for_parsing() -> &'static [AppServerMethod] {
    &[
        AppServerMethod::ThreadInjectItems,
        AppServerMethod::ThreadLoadedList,
        AppServerMethod::ThreadStart,
        AppServerMethod::ThreadResume,
        AppServerMethod::TurnInterrupt,
        AppServerMethod::TurnStart,
        AppServerMethod::TurnSteer,
        AppServerMethod::Initialize,
    ]
}

fn normalize_method_name(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JsonRpcTranscriptSummary {
    pub request_methods: BTreeMap<String, u64>,
    pub response_methods: BTreeMap<String, u64>,
    pub error_methods: BTreeMap<String, u64>,
    pub response_count: u64,
    pub error_count: u64,
    pub notification_methods: BTreeMap<String, u64>,
    pub request_ids: BTreeMap<String, String>,
    pub thread_id: Option<String>,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub generated_ids: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

impl JsonRpcTranscriptSummary {
    pub fn observe(&mut self, message: &Value) {
        if let Some(method) = message.get("method").and_then(Value::as_str) {
            if message.get("id").is_some() {
                *self.request_methods.entry(method.to_string()).or_default() += 1;
                if let Some(id) = json_rpc_id_key(message.get("id")) {
                    self.request_ids.insert(id, method.to_string());
                }
                self.observe_request_params(method, message.get("params"));
            } else {
                *self
                    .notification_methods
                    .entry(method.to_string())
                    .or_default() += 1;
            }
        }
        let response_method = message
            .get("id")
            .and_then(|id| json_rpc_id_key(Some(id)))
            .and_then(|id| self.request_ids.get(&id).cloned());
        if let Some(result) = message.get("result") {
            self.response_count += 1;
            if let Some(method) = &response_method {
                *self.response_methods.entry(method.clone()).or_default() += 1;
                self.observe_response_result(method, result);
            } else {
                self.warnings
                    .push("json-rpc response could not be matched to a request id".to_string());
            }
        }
        if message.get("error").is_some() {
            self.error_count += 1;
            if let Some(method) = &response_method {
                *self.error_methods.entry(method.clone()).or_default() += 1;
                self.warnings
                    .push(format!("json-rpc error observed for method {method}"));
            } else {
                self.warnings
                    .push("json-rpc error could not be matched to a request id".to_string());
            }
        }
    }

    fn observe_request_params(&mut self, method: &str, params: Option<&Value>) {
        if method == AppServerMethod::ThreadInjectItems.wire_name() {
            if let Some(thread_id) = params.and_then(|params| value_string(params, &["threadId"])) {
                self.remember_id("thread_id", thread_id);
            }
        }
        if method == AppServerMethod::TurnStart.wire_name()
            || method == AppServerMethod::TurnSteer.wire_name()
        {
            if let Some(thread_id) = params.and_then(|params| value_string(params, &["threadId"])) {
                self.remember_id("thread_id", thread_id);
            }
        }
        if method == AppServerMethod::TurnSteer.wire_name() {
            if let Some(turn_id) =
                params.and_then(|params| value_string(params, &["expectedTurnId"]))
            {
                self.remember_id("turn_id", turn_id);
            }
        }
    }

    fn observe_response_result(&mut self, method: &str, result: &Value) {
        if method == AppServerMethod::Initialize.wire_name() {
            if let Some(session_id) = value_string(result, &["sessionId", "session_id"]) {
                self.remember_id("session_id", session_id);
            }
        }
        if method == AppServerMethod::ThreadStart.wire_name() {
            if let Some(thread_id) =
                value_string(result, &["threadId", "thread_id"]).or_else(|| {
                    result
                        .get("thread")
                        .and_then(|thread| value_string(thread, &["id", "threadId", "thread_id"]))
                })
            {
                self.remember_id("thread_id", thread_id);
            }
        }
        if method == AppServerMethod::TurnStart.wire_name() {
            if let Some(turn_id) = value_string(result, &["turnId", "turn_id"]).or_else(|| {
                result
                    .get("turn")
                    .and_then(|turn| value_string(turn, &["id", "turnId", "turn_id"]))
            }) {
                self.remember_id("turn_id", turn_id);
            }
        }
    }

    fn remember_id(&mut self, key: &'static str, value: String) {
        if value.trim().is_empty() {
            return;
        }
        match key {
            "thread_id" => self.thread_id = Some(value.clone()),
            "session_id" => self.session_id = Some(value.clone()),
            "turn_id" => self.turn_id = Some(value.clone()),
            _ => {}
        }
        self.generated_ids.insert(key.to_string(), value);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LiveAppServerMethodStatus {
    Missing,
    Requested,
    Responded,
    Failed,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveAppServerMethodObservation {
    pub method: String,
    pub requested: bool,
    pub responded: bool,
    pub errored: bool,
    pub request_count: u64,
    pub response_count: u64,
    pub error_count: u64,
    pub status: LiveAppServerMethodStatus,
}

pub fn summarize_json_rpc_transcript<'a>(
    messages: impl IntoIterator<Item = &'a Value>,
) -> JsonRpcTranscriptSummary {
    let mut summary = JsonRpcTranscriptSummary::default();
    for message in messages {
        summary.observe(message);
    }
    add_missing_required_method_warnings(&mut summary);
    summary
}

pub fn summarize_json_rpc_transcript_jsonl(transcript_jsonl: &str) -> JsonRpcTranscriptSummary {
    let mut summary = JsonRpcTranscriptSummary::default();
    for (index, line) in transcript_jsonl.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        match serde_json::from_str::<Value>(line) {
            Ok(message) => summary.observe(&message),
            Err(_) => summary.warnings.push(format!(
                "transcript line {} was not valid JSON and was skipped",
                index + 1
            )),
        }
    }
    add_missing_required_method_warnings(&mut summary);
    summary
}

pub fn live_required_method_observations(
    summary: &JsonRpcTranscriptSummary,
) -> Vec<LiveAppServerMethodObservation> {
    live_required_methods()
        .iter()
        .map(|method| {
            let method = method.wire_name();
            let request_count = summary.request_methods.get(method).copied().unwrap_or(0);
            let response_count = summary.response_methods.get(method).copied().unwrap_or(0);
            let error_count = summary.error_methods.get(method).copied().unwrap_or(0);
            let requested = request_count > 0;
            let responded = response_count > 0;
            let errored = error_count > 0;
            let status = if responded {
                LiveAppServerMethodStatus::Responded
            } else if errored {
                LiveAppServerMethodStatus::Failed
            } else if requested {
                LiveAppServerMethodStatus::Requested
            } else {
                LiveAppServerMethodStatus::Missing
            };

            LiveAppServerMethodObservation {
                method: method.to_string(),
                requested,
                responded,
                errored,
                request_count,
                response_count,
                error_count,
                status,
            }
        })
        .collect()
}

pub fn live_app_server_proof_event_requests(
    unit_id: impl Into<String>,
    summary: &JsonRpcTranscriptSummary,
) -> Vec<AdapterEventRequest> {
    let unit_id = AgentUnitId(unit_id.into());
    let correlation = AdapterCorrelation {
        thread_id: summary.thread_id.clone(),
        session_id: summary.session_id.clone(),
        turn_id: summary.turn_id.clone(),
        expected_turn_id: summary.turn_id.clone(),
        ..AdapterCorrelation::default()
    };

    vec![
        proof_request(
            unit_id.clone(),
            AdapterEventKind::TransportIdentityObserved,
            transport_identity_status(summary),
            correlation.clone(),
            json!({
                "observed_methods": observed_required_methods(summary),
                "missing_methods": missing_required_methods(summary),
                "generated_ids": summary.generated_ids,
                "warning_count": summary.warnings.len(),
            }),
        ),
        proof_request(
            unit_id.clone(),
            AdapterEventKind::SupervisorDispatchAttempted,
            proof_status_for_methods(
                summary,
                &[
                    AppServerMethod::ThreadStart,
                    AppServerMethod::TurnStart,
                    AppServerMethod::ThreadLoadedList,
                ],
            ),
            correlation.clone(),
            method_evidence(
                summary,
                &[
                    AppServerMethod::ThreadStart,
                    AppServerMethod::TurnStart,
                    AppServerMethod::ThreadLoadedList,
                ],
            ),
        ),
        proof_request(
            unit_id.clone(),
            AdapterEventKind::ContextInjectionExecuted,
            proof_status_for_methods(summary, &[AppServerMethod::ThreadInjectItems]),
            correlation.clone(),
            method_evidence(summary, &[AppServerMethod::ThreadInjectItems]),
        ),
        proof_request(
            unit_id.clone(),
            AdapterEventKind::RuntimeWakeupAccepted,
            proof_status_for_methods(summary, &[AppServerMethod::TurnSteer]),
            correlation.clone(),
            method_evidence(summary, &[AppServerMethod::TurnSteer]),
        ),
        proof_request(
            unit_id,
            AdapterEventKind::ToolAllowlistChecked,
            proof_status_for_methods(summary, &[AppServerMethod::ThreadLoadedList]),
            correlation,
            method_evidence(summary, &[AppServerMethod::ThreadLoadedList]),
        ),
    ]
}

fn add_missing_required_method_warnings(summary: &mut JsonRpcTranscriptSummary) {
    for observation in live_required_method_observations(summary) {
        match observation.status {
            LiveAppServerMethodStatus::Missing => summary.warnings.push(format!(
                "required live method {} was not requested",
                observation.method
            )),
            LiveAppServerMethodStatus::Requested => summary.warnings.push(format!(
                "required live method {} was requested but no response was observed",
                observation.method
            )),
            LiveAppServerMethodStatus::Failed | LiveAppServerMethodStatus::Responded => {}
        }
    }
}

fn observed_required_methods(summary: &JsonRpcTranscriptSummary) -> Vec<String> {
    live_required_method_observations(summary)
        .into_iter()
        .filter(|observation| observation.responded)
        .map(|observation| observation.method)
        .collect()
}

fn missing_required_methods(summary: &JsonRpcTranscriptSummary) -> Vec<String> {
    live_required_method_observations(summary)
        .into_iter()
        .filter(|observation| !observation.responded)
        .map(|observation| observation.method)
        .collect()
}

fn transport_identity_status(summary: &JsonRpcTranscriptSummary) -> AdapterEventStatus {
    let base_status = proof_status_for_methods(
        summary,
        &[AppServerMethod::Initialize, AppServerMethod::ThreadStart],
    );
    if base_status != AdapterEventStatus::Accepted {
        return base_status;
    }
    if summary.thread_id.is_some() {
        AdapterEventStatus::Accepted
    } else {
        AdapterEventStatus::Missing
    }
}

fn proof_status_for_methods(
    summary: &JsonRpcTranscriptSummary,
    methods: &[AppServerMethod],
) -> AdapterEventStatus {
    let any_error = methods
        .iter()
        .any(|method| summary.error_methods.contains_key(method.wire_name()));
    let all_responded = methods
        .iter()
        .all(|method| summary.response_methods.contains_key(method.wire_name()));
    let any_requested = methods
        .iter()
        .any(|method| summary.request_methods.contains_key(method.wire_name()));

    if all_responded {
        AdapterEventStatus::Accepted
    } else if any_error {
        AdapterEventStatus::Failed
    } else if any_requested {
        AdapterEventStatus::Missing
    } else {
        AdapterEventStatus::Missing
    }
}

fn method_evidence(summary: &JsonRpcTranscriptSummary, methods: &[AppServerMethod]) -> Value {
    let observations = live_required_method_observations(summary)
        .into_iter()
        .filter(|observation| {
            methods
                .iter()
                .any(|method| method.wire_name() == observation.method)
        })
        .collect::<Vec<_>>();

    json!({
        "methods": observations,
        "warning_count": summary.warnings.len(),
    })
}

fn proof_request(
    unit_id: AgentUnitId,
    kind: AdapterEventKind,
    status: AdapterEventStatus,
    correlation: AdapterCorrelation,
    evidence: Value,
) -> AdapterEventRequest {
    AdapterEventRequest {
        unit_id,
        adapter: AdapterKind::CodexAppServer,
        kind,
        status,
        correlation,
        evidence,
        redaction: Default::default(),
    }
}

fn json_rpc_id_key(id: Option<&Value>) -> Option<String> {
    match id? {
        Value::Null => None,
        Value::String(value) => Some(value.clone()),
        value => Some(value.to_string()),
    }
}

fn value_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_str))
        .map(ToOwned::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::check_evidence_redaction;

    #[test]
    fn live_plan_starts_server_before_json_rpc_work() {
        let plan = LiveAppServerDogfoodRequest::default().plan();

        assert_eq!(
            plan.phases[..3],
            [
                LiveAppServerDogfoodPhase::SpawnAppServer,
                LiveAppServerDogfoodPhase::GenerateSchema,
                LiveAppServerDogfoodPhase::Initialize,
            ]
        );
        assert_eq!(
            plan.required_methods,
            vec![
                "initialize",
                "thread/start",
                "turn/start",
                "thread/inject_items",
                "turn/steer",
                "thread/loaded/list",
            ]
        );
    }

    #[test]
    fn request_builders_emit_current_app_server_wire_names() {
        assert_eq!(initialize_request(1, true)["method"], "initialize");
        assert_eq!(
            thread_start_request(2, "/tmp/work", "gpt-5.4-mini")["method"],
            "thread/start"
        );
        assert_eq!(
            turn_start_request(3, "thread-1", "hello")["method"],
            "turn/start"
        );
        assert_eq!(
            thread_inject_items_request(4, "thread-1", vec![json!({"type": "message"})])["method"],
            "thread/inject_items"
        );
        assert_eq!(
            turn_steer_request(5, "thread-1", "turn-1", "wake")["params"]["expectedTurnId"],
            "turn-1"
        );
    }

    #[test]
    fn transcript_summary_counts_requests_responses_errors_and_notifications() {
        let mut summary = JsonRpcTranscriptSummary::default();
        summary.observe(&initialize_request(1, true));
        summary.observe(&json!({"id": 1, "result": {"ok": true}}));
        summary.observe(&json!({"id": 2, "error": {"code": -32000, "message": "nope"}}));
        summary.observe(&json!({"method": "thread/started", "params": {"threadId": "t"}}));

        assert_eq!(summary.request_methods["initialize"], 1);
        assert_eq!(summary.response_count, 1);
        assert_eq!(summary.error_count, 1);
        assert_eq!(summary.notification_methods["thread/started"], 1);
    }

    #[test]
    fn schema_availability_detects_required_methods_from_files_and_bundle() {
        let file_availability = live_schema_availability_from_file_names([
            "initialize.schema.json",
            "thread-start.schema.json",
            "turn-start.schema.json",
            "thread-inject-items.schema.json",
            "turn-steer.schema.json",
            "thread-loaded-list.schema.json",
        ]);

        assert!(file_availability.supports_live_required_methods());
        assert!(file_availability
            .available_methods
            .contains(&AppServerMethod::ThreadInjectItems));

        let bundle_availability = live_schema_availability_from_bundle_json(&json!({
            "methods": [
                {"name": "initialize"},
                {"name": "thread/start"},
                {"name": "turn/start"},
                {"name": "thread/inject_items"},
                {"name": "turn/steer"},
                {"name": "thread/loaded/list"}
            ]
        }));

        assert!(bundle_availability.supports_live_required_methods());
        assert!(bundle_availability
            .available_methods
            .contains(&AppServerMethod::ThreadLoadedList));
    }

    #[test]
    fn transcript_detects_required_live_method_observations() {
        let messages = vec![
            initialize_request("init", true),
            json!({"id": "init", "result": {"sessionId": "session-1"}}),
            thread_start_request("thread-start", "/tmp/work", "gpt-5.4-mini"),
            json!({"id": "thread-start", "result": {"threadId": "thread-1"}}),
            turn_start_request("turn-start", "thread-1", "hello"),
            json!({"id": "turn-start", "result": {"turnId": "turn-1"}}),
            thread_inject_items_request("inject", "thread-1", vec![json!({"type": "note"})]),
            json!({"id": "inject", "result": {"accepted": true}}),
            turn_steer_request("steer", "thread-1", "turn-1", "continue"),
            json!({"id": "steer", "result": {"accepted": true}}),
            thread_loaded_list_request("loaded", 10),
            json!({"id": "loaded", "result": {"threads": [{"threadId": "thread-1"}]}}),
        ];

        let summary = summarize_json_rpc_transcript(&messages);
        let observations = live_required_method_observations(&summary);

        assert!(observations
            .iter()
            .all(|observation| observation.status == LiveAppServerMethodStatus::Responded));
        assert_eq!(summary.session_id.as_deref(), Some("session-1"));
        assert_eq!(summary.thread_id.as_deref(), Some("thread-1"));
        assert_eq!(summary.turn_id.as_deref(), Some("turn-1"));
        assert!(summary.warnings.is_empty());
    }

    #[test]
    fn transcript_detects_actual_thread_start_response_shape() {
        let messages = vec![
            initialize_request("init", true),
            json!({"id": "init", "result": {"userAgent": "Codex Desktop/test", "codexHome": "/tmp/codex"}}),
            thread_start_request("thread-start", "/tmp/work", "gpt-5.4-mini"),
            json!({"id": "thread-start", "result": {"thread": {"id": "thread-actual", "status": {"type": "idle"}}}}),
            thread_loaded_list_request("loaded", 10),
            json!({"id": "loaded", "result": {"data": ["thread-actual"], "nextCursor": null}}),
        ];

        let summary = summarize_json_rpc_transcript(&messages);
        let proof_requests = live_app_server_proof_event_requests("unit-1", &summary);
        let transport = proof_requests
            .iter()
            .find(|request| request.kind == AdapterEventKind::TransportIdentityObserved)
            .expect("transport proof request");

        assert_eq!(summary.thread_id.as_deref(), Some("thread-actual"));
        assert_eq!(transport.status, AdapterEventStatus::Accepted);
    }

    #[test]
    fn transcript_errors_become_warnings_and_missing_or_failed_proof() {
        let messages = vec![
            initialize_request("init", true),
            json!({"id": "init", "result": {"sessionId": "session-1"}}),
            thread_start_request("thread-start", "/tmp/work", "gpt-5.4-mini"),
            json!({"id": "thread-start", "result": {"threadId": "thread-1"}}),
            turn_start_request("turn-start", "thread-1", "hello"),
            json!({"id": "turn-start", "result": {"turnId": "turn-1"}}),
            thread_inject_items_request("inject", "thread-1", vec![json!({"type": "note"})]),
            json!({"id": "inject", "result": {"accepted": true}}),
            turn_steer_request("steer", "thread-1", "turn-1", "continue"),
            json!({"id": "steer", "error": {"code": -32000, "message": "denied"}}),
        ];

        let summary = summarize_json_rpc_transcript(&messages);
        let observations = live_required_method_observations(&summary);
        let steer = observations
            .iter()
            .find(|observation| observation.method == "turn/steer")
            .expect("steer observation");
        let loaded = observations
            .iter()
            .find(|observation| observation.method == "thread/loaded/list")
            .expect("loaded list observation");

        assert_eq!(steer.status, LiveAppServerMethodStatus::Failed);
        assert_eq!(loaded.status, LiveAppServerMethodStatus::Missing);
        assert!(summary
            .warnings
            .contains(&"json-rpc error observed for method turn/steer".to_string()));

        let proof_requests = live_app_server_proof_event_requests("unit-1", &summary);
        let steering = proof_requests
            .iter()
            .find(|request| request.kind == AdapterEventKind::RuntimeWakeupAccepted)
            .expect("steering proof request");
        let loaded_list = proof_requests
            .iter()
            .find(|request| request.kind == AdapterEventKind::ToolAllowlistChecked)
            .expect("loaded-list proof request");

        assert_eq!(steering.status, AdapterEventStatus::Failed);
        assert_eq!(loaded_list.status, AdapterEventStatus::Missing);
    }

    #[test]
    fn proof_event_requests_are_redacted() {
        let messages = vec![
            initialize_request("init", true),
            json!({"id": "init", "result": {"sessionId": "session-1"}}),
            thread_start_request("thread-start", "/tmp/work", "gpt-5.4-mini"),
            json!({"id": "thread-start", "result": {"threadId": "thread-1"}}),
            turn_start_request("turn-start", "thread-1", "raw prompt that must not persist"),
            json!({"id": "turn-start", "result": {"turnId": "turn-1"}}),
            thread_inject_items_request(
                "inject",
                "thread-1",
                vec![json!({"content": "secret body"})],
            ),
            json!({"id": "inject", "result": {"accepted": true}}),
            turn_steer_request("steer", "thread-1", "turn-1", "raw steering text"),
            json!({"id": "steer", "result": {"accepted": true}}),
            thread_loaded_list_request("loaded", 10),
            json!({"id": "loaded", "result": {"threads": [{"threadId": "thread-1"}]}}),
        ];
        let summary = summarize_json_rpc_transcript(&messages);

        for request in live_app_server_proof_event_requests("unit-1", &summary) {
            assert!(request.redaction.is_complete());
            assert!(
                check_evidence_redaction(&request.evidence).allowed,
                "proof evidence should be redacted: {:?}",
                request.evidence
            );
            let serialized = serde_json::to_string(&request.evidence).unwrap();
            assert!(!serialized.contains("raw prompt"));
            assert!(!serialized.contains("secret body"));
            assert!(!serialized.contains("raw steering text"));
        }
    }
}
