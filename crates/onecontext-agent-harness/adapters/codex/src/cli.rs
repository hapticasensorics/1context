use std::env;
use std::fmt;
use std::io::{self, Write};
use std::path::PathBuf;

use onecontext_agent_harness_core::AgentHarnessUnit;
use onecontext_codex_adapter::{
    CodexAdapterError, GovernedChildAgentRequest, HarnessAgentSpawner, HarnessProofRecordPlan,
    HarnessProofSink, InProcessHarnessBridge, LiveAppServerDogfoodPhase, LiveAppServerDogfoodPlan,
    LiveAppServerDogfoodRequest, ProofRecordTarget, CODEX_ADAPTER_SCHEMA_VERSION,
};
use serde::Serialize;

const LIVE_SERVER_PLAN_CONTRACT_VERSION: u32 = 1;
const LIVE_SERVER_MODEL_TURN_SKIP_FLAG: &str = "--skip-model-turns";
const LIVE_SERVER_MODEL_TURN_LEGACY_ALLOW_FLAG: &str = "--allow-model-turns";

#[derive(Clone, Debug, PartialEq, Eq)]
enum CliCommand {
    Describe,
    LiveServerPlan {
        evidence_dir: PathBuf,
        runtime_root: PathBuf,
        codex_bin: String,
        listen_url: String,
        model_turns_enabled: bool,
    },
    SpawnChild {
        root: PathBuf,
        request_json: String,
    },
    RecordProof {
        root: PathBuf,
        request_json: String,
    },
}

#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Json {
        context: &'static str,
        source: serde_json::Error,
    },
    Adapter(CodexAdapterError),
    Io(io::Error),
}

impl fmt::Display for CliError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage(message) => write!(f, "{message}\n\n{}", usage()),
            Self::Json { context, source } => write!(f, "failed to parse {context}: {source}"),
            Self::Adapter(error) => write!(f, "{error}"),
            Self::Io(error) => write!(f, "failed to write CLI output: {error}"),
        }
    }
}

impl std::error::Error for CliError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json { source, .. } => Some(source),
            Self::Adapter(source) => Some(source),
            Self::Io(source) => Some(source),
            Self::Usage(_) => None,
        }
    }
}

impl From<CodexAdapterError> for CliError {
    fn from(error: CodexAdapterError) -> Self {
        Self::Adapter(error)
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

pub fn run_from_env() -> Result<(), CliError> {
    let command = parse_cli(env::args().skip(1))?;
    execute(command, &mut io::stdout())
}

fn parse_cli(args: impl IntoIterator<Item = String>) -> Result<CliCommand, CliError> {
    let mut args = args.into_iter();
    let Some(command) = args.next() else {
        return Err(CliError::Usage("missing command".to_string()));
    };

    match command.as_str() {
        "describe" => {
            let extra: Vec<String> = args.collect();
            if !extra.is_empty() {
                return Err(CliError::Usage(format!(
                    "describe does not accept arguments: {}",
                    extra.join(" ")
                )));
            }
            Ok(CliCommand::Describe)
        }
        "spawn-child" => parse_rooted_json_command("spawn-child", args)
            .map(|(root, request_json)| CliCommand::SpawnChild { root, request_json }),
        "record-proof" => parse_rooted_json_command("record-proof", args)
            .map(|(root, request_json)| CliCommand::RecordProof { root, request_json }),
        "live-server-plan" => parse_live_server_plan_command(args),
        "--help" | "-h" | "help" => Err(CliError::Usage("".to_string())),
        other => Err(CliError::Usage(format!("unknown command: {other}"))),
    }
}

fn parse_live_server_plan_command(
    args: impl Iterator<Item = String>,
) -> Result<CliCommand, CliError> {
    let mut evidence_dir = None;
    let mut runtime_root = None;
    let mut codex_bin = None;
    let mut listen_url = None;
    let mut model_turns_enabled = true;
    let mut saw_model_turn_skip = false;
    let mut saw_legacy_allow = false;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--evidence-dir" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "live-server-plan missing value for --evidence-dir".to_string(),
                    ));
                };
                if evidence_dir.replace(PathBuf::from(value)).is_some() {
                    return Err(CliError::Usage(
                        "live-server-plan received --evidence-dir twice".to_string(),
                    ));
                }
            }
            "--runtime-root" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "live-server-plan missing value for --runtime-root".to_string(),
                    ));
                };
                if runtime_root.replace(PathBuf::from(value)).is_some() {
                    return Err(CliError::Usage(
                        "live-server-plan received --runtime-root twice".to_string(),
                    ));
                }
            }
            "--codex-bin" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "live-server-plan missing value for --codex-bin".to_string(),
                    ));
                };
                if codex_bin.replace(value).is_some() {
                    return Err(CliError::Usage(
                        "live-server-plan received --codex-bin twice".to_string(),
                    ));
                }
            }
            "--listen-url" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(
                        "live-server-plan missing value for --listen-url".to_string(),
                    ));
                };
                if listen_url.replace(value).is_some() {
                    return Err(CliError::Usage(
                        "live-server-plan received --listen-url twice".to_string(),
                    ));
                }
            }
            LIVE_SERVER_MODEL_TURN_SKIP_FLAG => {
                if saw_model_turn_skip {
                    return Err(CliError::Usage(format!(
                        "live-server-plan received {LIVE_SERVER_MODEL_TURN_SKIP_FLAG} twice"
                    )));
                }
                if saw_legacy_allow {
                    return Err(CliError::Usage(format!(
                        "live-server-plan cannot combine {LIVE_SERVER_MODEL_TURN_SKIP_FLAG} with {LIVE_SERVER_MODEL_TURN_LEGACY_ALLOW_FLAG}"
                    )));
                }
                saw_model_turn_skip = true;
                model_turns_enabled = false;
            }
            LIVE_SERVER_MODEL_TURN_LEGACY_ALLOW_FLAG => {
                if saw_legacy_allow {
                    return Err(CliError::Usage(format!(
                        "live-server-plan received {LIVE_SERVER_MODEL_TURN_LEGACY_ALLOW_FLAG} twice"
                    )));
                }
                if saw_model_turn_skip {
                    return Err(CliError::Usage(format!(
                        "live-server-plan cannot combine {LIVE_SERVER_MODEL_TURN_LEGACY_ALLOW_FLAG} with {LIVE_SERVER_MODEL_TURN_SKIP_FLAG}"
                    )));
                }
                saw_legacy_allow = true;
            }
            other => {
                return Err(CliError::Usage(format!(
                    "live-server-plan received unknown argument: {other}"
                )));
            }
        }
    }

    let evidence_dir = evidence_dir.ok_or_else(|| {
        CliError::Usage("live-server-plan requires --evidence-dir <path>".to_string())
    })?;
    let runtime_root = runtime_root.ok_or_else(|| {
        CliError::Usage("live-server-plan requires --runtime-root <path>".to_string())
    })?;

    Ok(CliCommand::LiveServerPlan {
        evidence_dir,
        runtime_root,
        codex_bin: codex_bin.unwrap_or_else(|| "codex".to_string()),
        listen_url: listen_url.unwrap_or_else(|| "stdio://".to_string()),
        model_turns_enabled,
    })
}

fn parse_rooted_json_command(
    command: &str,
    args: impl Iterator<Item = String>,
) -> Result<(PathBuf, String), CliError> {
    let mut root = None;
    let mut request_json = None;
    let mut args = args.peekable();

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(format!(
                        "{command} missing value for --root"
                    )));
                };
                if root.replace(PathBuf::from(value)).is_some() {
                    return Err(CliError::Usage(format!("{command} received --root twice")));
                }
            }
            "--request-json" => {
                let Some(value) = args.next() else {
                    return Err(CliError::Usage(format!(
                        "{command} missing value for --request-json"
                    )));
                };
                if request_json.replace(value).is_some() {
                    return Err(CliError::Usage(format!(
                        "{command} received --request-json twice"
                    )));
                }
            }
            other => {
                return Err(CliError::Usage(format!(
                    "{command} received unknown argument: {other}"
                )));
            }
        }
    }

    let root = root.ok_or_else(|| CliError::Usage(format!("{command} requires --root <path>")))?;
    let request_json = request_json
        .ok_or_else(|| CliError::Usage(format!("{command} requires --request-json <json>")))?;

    Ok((root, request_json))
}

fn execute(command: CliCommand, writer: &mut impl Write) -> Result<(), CliError> {
    match command {
        CliCommand::Describe => write_json(writer, &describe_output()),
        CliCommand::LiveServerPlan {
            evidence_dir,
            runtime_root,
            codex_bin,
            listen_url,
            model_turns_enabled,
        } => {
            let plan = LiveAppServerDogfoodRequest {
                evidence_dir,
                runtime_root,
                codex_bin,
                listen_url,
                ..LiveAppServerDogfoodRequest::default()
            }
            .plan();
            write_json(
                writer,
                &LiveServerPlanCliOutput::new(plan, model_turns_enabled),
            )
        }
        CliCommand::SpawnChild { root, request_json } => {
            let request: GovernedChildAgentRequest =
                parse_request_json("GovernedChildAgentRequest", &request_json)?;
            let bridge = InProcessHarnessBridge::new(root);
            let unit = bridge.spawn_child(request)?;
            write_json(writer, &unit)
        }
        CliCommand::RecordProof { root, request_json } => {
            let plan: HarnessProofRecordPlan =
                parse_request_json("HarnessProofRecordPlan", &request_json)?;
            let target = plan.target.clone();
            let bridge = InProcessHarnessBridge::new(root);
            match bridge.record_proof_plan(&plan)? {
                Some(unit) => write_json(writer, &unit),
                None => write_json(writer, &RecordProofNullResult::for_target(target)),
            }
        }
    }
}

fn parse_request_json<T: serde::de::DeserializeOwned>(
    context: &'static str,
    request_json: &str,
) -> Result<T, CliError> {
    serde_json::from_str(request_json).map_err(|source| CliError::Json { context, source })
}

fn write_json<T: Serialize>(writer: &mut impl Write, value: &T) -> Result<(), CliError> {
    serde_json::to_writer_pretty(&mut *writer, value).map_err(|source| CliError::Json {
        context: "CLI output",
        source,
    })?;
    writer.write_all(b"\n")?;
    Ok(())
}

#[derive(Serialize)]
struct DescribeOutput<'a> {
    kind: &'a str,
    adapter: &'a str,
    schema_version: u32,
    bridge: BridgeDescription<'a>,
    commands: Vec<CommandDescription<'a>>,
}

#[derive(Serialize)]
struct BridgeDescription<'a> {
    kind: &'a str,
    mode: &'a str,
    store_root_flag: &'a str,
}

#[derive(Serialize)]
struct CommandDescription<'a> {
    name: &'a str,
    summary: &'a str,
    required_flags: Vec<&'a str>,
    optional_flags: Vec<&'a str>,
    output: &'a str,
}

#[derive(Serialize)]
struct LiveServerPlanCliOutput {
    #[serde(flatten)]
    plan: LiveAppServerDogfoodPlan,
    cli_contract_version: u32,
    execution_policy: LiveServerExecutionPolicy,
}

impl LiveServerPlanCliOutput {
    fn new(plan: LiveAppServerDogfoodPlan, model_turns_enabled: bool) -> Self {
        Self {
            plan,
            cli_contract_version: LIVE_SERVER_PLAN_CONTRACT_VERSION,
            execution_policy: LiveServerExecutionPolicy::new(model_turns_enabled),
        }
    }
}

#[derive(Serialize)]
struct LiveServerExecutionPolicy {
    allow_model_consuming_turns: bool,
    model_turn_skip_flag: &'static str,
    default_allows_model_consuming_turns: bool,
    model_consuming_methods: Vec<&'static str>,
    model_consuming_phases: Vec<LiveAppServerDogfoodPhase>,
    minimum_non_model_methods: Vec<&'static str>,
    runner_requirement: &'static str,
}

impl LiveServerExecutionPolicy {
    fn new(allow_model_consuming_turns: bool) -> Self {
        Self {
            allow_model_consuming_turns,
            model_turn_skip_flag: LIVE_SERVER_MODEL_TURN_SKIP_FLAG,
            default_allows_model_consuming_turns: true,
            model_consuming_methods: vec!["turn/start", "turn/steer"],
            model_consuming_phases: vec![
                LiveAppServerDogfoodPhase::TurnStart,
                LiveAppServerDogfoodPhase::TurnSteer,
            ],
            minimum_non_model_methods: vec![
                "initialize",
                "thread/start",
                "thread/loaded/list",
            ],
            runner_requirement:
                "runners execute turn/start and turn/steer by default; use --skip-model-turns only for non-model debugging",
        }
    }
}

#[derive(Serialize)]
struct RecordProofNullResult {
    kind: &'static str,
    recorded: bool,
    target: ProofRecordTarget,
    unit: Option<AgentHarnessUnit>,
}

impl RecordProofNullResult {
    fn for_target(target: ProofRecordTarget) -> Self {
        Self {
            kind: "onecontext.codex_adapter.record_proof_result",
            recorded: false,
            target,
            unit: None,
        }
    }
}

fn describe_output() -> DescribeOutput<'static> {
    DescribeOutput {
        kind: "onecontext.codex_adapter.cli",
        adapter: "onecontext-codex-adapter",
        schema_version: CODEX_ADAPTER_SCHEMA_VERSION,
        bridge: BridgeDescription {
            kind: "in_process_harness_bridge",
            mode: "local_store",
            store_root_flag: "--root",
        },
        commands: vec![
            CommandDescription {
                name: "describe",
                summary: "print this deterministic adapter CLI description",
                required_flags: Vec::new(),
                optional_flags: Vec::new(),
                output: "onecontext.codex_adapter.cli",
            },
            CommandDescription {
                name: "live-server-plan",
                summary: "print the live Codex app-server dogfood phase and artifact plan",
                required_flags: vec!["--evidence-dir", "--runtime-root"],
                optional_flags: vec![
                    "--codex-bin",
                    "--listen-url",
                    LIVE_SERVER_MODEL_TURN_SKIP_FLAG,
                ],
                output: "onecontext.codex_adapter.live_app_server_dogfood_plan",
            },
            CommandDescription {
                name: "spawn-child",
                summary: "parse a GovernedChildAgentRequest and create a harness child unit",
                required_flags: vec!["--root", "--request-json"],
                optional_flags: Vec::new(),
                output: "onecontext.agent_harness.unit",
            },
            CommandDescription {
                name: "record-proof",
                summary: "parse a HarnessProofRecordPlan and record an adapter proof event",
                required_flags: vec!["--root", "--request-json"],
                optional_flags: Vec::new(),
                output:
                    "onecontext.agent_harness.unit or onecontext.codex_adapter.record_proof_result",
            },
        ],
    }
}

fn usage() -> &'static str {
    "usage:
  onecontext-codex-adapter describe
  onecontext-codex-adapter live-server-plan --evidence-dir <path> --runtime-root <path> [--codex-bin <path>] [--listen-url <url>] [--skip-model-turns]
  onecontext-codex-adapter spawn-child --root <path> --request-json <json>
  onecontext-codex-adapter record-proof --root <path> --request-json <json>"
}

#[cfg(test)]
mod tests {
    use super::*;
    use onecontext_agent_harness_core::{
        AdapterCorrelation, AdapterEventKind, AdapterEventRequest, AdapterEventStatus, AdapterKind,
        AdapterRedaction, AgentCallRequest, AgentHarnessStore, AgentUnitId, AgentVisibility,
    };
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    fn parent_call_request(unit_id: &str) -> AgentCallRequest {
        AgentCallRequest {
            unit_id: Some(AgentUnitId(unit_id.to_string())),
            parent_unit_id: None,
            spawn_request_id: None,
            role: "parent agent".to_string(),
            model: "gpt-5-codex".to_string(),
            identity: json!({ "display_name": "parent" }),
            instructions: BTreeMap::new(),
            runtime: json!({ "adapter": "codex_app_server" }),
            capabilities: Vec::new(),
            visibility: AgentVisibility::Private,
            metadata: json!({ "title": "Parent" }),
        }
    }

    #[test]
    fn parses_spawn_child_command_flags() {
        let command = parse_cli([
            "spawn-child".to_string(),
            "--request-json".to_string(),
            "{}".to_string(),
            "--root".to_string(),
            "/tmp/onecontext".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            CliCommand::SpawnChild {
                root: PathBuf::from("/tmp/onecontext"),
                request_json: "{}".to_string(),
            }
        );
    }

    #[test]
    fn rooted_command_requires_root() {
        let error = parse_cli([
            "record-proof".to_string(),
            "--request-json".to_string(),
            "{}".to_string(),
        ])
        .unwrap_err();

        assert!(matches!(error, CliError::Usage(message) if message.contains("--root")));
    }

    #[test]
    fn parses_live_server_plan_command_flags() {
        let command = parse_cli([
            "live-server-plan".to_string(),
            "--evidence-dir".to_string(),
            "/tmp/evidence".to_string(),
            "--runtime-root".to_string(),
            "/tmp/evidence/runtime/1Context".to_string(),
            "--codex-bin".to_string(),
            "/usr/local/bin/codex".to_string(),
            "--listen-url".to_string(),
            "tcp://127.0.0.1:0".to_string(),
            "--skip-model-turns".to_string(),
        ])
        .unwrap();

        assert_eq!(
            command,
            CliCommand::LiveServerPlan {
                evidence_dir: PathBuf::from("/tmp/evidence"),
                runtime_root: PathBuf::from("/tmp/evidence/runtime/1Context"),
                codex_bin: "/usr/local/bin/codex".to_string(),
                listen_url: "tcp://127.0.0.1:0".to_string(),
                model_turns_enabled: false,
            }
        );
    }

    #[test]
    fn live_server_plan_rejects_duplicate_optional_flags() {
        let error = parse_cli([
            "live-server-plan".to_string(),
            "--evidence-dir".to_string(),
            "/tmp/evidence".to_string(),
            "--runtime-root".to_string(),
            "/tmp/runtime".to_string(),
            "--listen-url".to_string(),
            "stdio://".to_string(),
            "--listen-url".to_string(),
            "tcp://127.0.0.1:0".to_string(),
        ])
        .unwrap_err();

        assert!(
            matches!(error, CliError::Usage(message) if message.contains("--listen-url twice"))
        );
    }

    #[test]
    fn describe_output_is_structured_json() {
        let mut output = Vec::new();
        execute(CliCommand::Describe, &mut output).unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();

        assert_eq!(value["kind"], "onecontext.codex_adapter.cli");
        assert_eq!(value["commands"][0]["name"], "describe");
        assert_eq!(value["commands"][1]["name"], "live-server-plan");
        assert_eq!(value["commands"][2]["name"], "spawn-child");
        assert_eq!(value["commands"][3]["name"], "record-proof");
    }

    #[test]
    fn live_server_plan_prints_phase_order() {
        let mut output = Vec::new();
        execute(
            CliCommand::LiveServerPlan {
                evidence_dir: PathBuf::from("/tmp/evidence"),
                runtime_root: PathBuf::from("/tmp/evidence/runtime/1Context"),
                codex_bin: "codex".to_string(),
                listen_url: "stdio://".to_string(),
                model_turns_enabled: true,
            },
            &mut output,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();

        assert_eq!(
            value["kind"],
            "onecontext.codex_adapter.live_app_server_dogfood_plan"
        );
        assert_eq!(value["phases"][0], "spawn_app_server");
        assert_eq!(value["phases"][2], "initialize");
        assert!(value["required_methods"]
            .as_array()
            .unwrap()
            .iter()
            .any(|method| method == "thread/start"));
        assert_eq!(
            value["cli_contract_version"],
            LIVE_SERVER_PLAN_CONTRACT_VERSION
        );
        assert_eq!(
            value["execution_policy"]["model_turn_skip_flag"],
            LIVE_SERVER_MODEL_TURN_SKIP_FLAG
        );
        assert_eq!(
            value["execution_policy"]["allow_model_consuming_turns"],
            true
        );
        assert_eq!(
            value["execution_policy"]["default_allows_model_consuming_turns"],
            true
        );
        assert_eq!(
            value["execution_policy"]["model_consuming_methods"],
            json!(["turn/start", "turn/steer"])
        );
    }

    #[test]
    fn live_server_plan_can_explicitly_skip_model_turns() {
        let mut output = Vec::new();
        execute(
            CliCommand::LiveServerPlan {
                evidence_dir: PathBuf::from("/tmp/evidence"),
                runtime_root: PathBuf::from("/tmp/evidence/runtime/1Context"),
                codex_bin: "codex".to_string(),
                listen_url: "stdio://".to_string(),
                model_turns_enabled: false,
            },
            &mut output,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();

        assert_eq!(
            value["execution_policy"]["allow_model_consuming_turns"],
            false
        );
    }

    #[test]
    fn spawn_child_prints_harness_unit_json() {
        let temp = tempfile::tempdir().unwrap();
        AgentHarnessStore::new(temp.path())
            .call(parent_call_request("agent-parent"))
            .unwrap();
        let request_json = json!({
            "parent_unit_id": "agent-parent",
            "unit_id": "agent-child",
            "spawn_request_id": "spawn-child-1",
            "role": "child reviewer",
            "model": "gpt-5-codex",
            "identity": { "display_name": "child" },
            "instructions": {},
            "runtime": { "adapter": "codex_app_server" },
            "capabilities": [],
            "visibility": "private",
            "metadata": { "title": "Child" }
        })
        .to_string();

        let mut output = Vec::new();
        execute(
            CliCommand::SpawnChild {
                root: temp.path().to_path_buf(),
                request_json,
            },
            &mut output,
        )
        .unwrap();
        let unit: AgentHarnessUnit = serde_json::from_slice(&output).unwrap();

        assert_eq!(unit.unit_id.0, "agent-child");
        assert_eq!(
            unit.certificate.lineage.parent_unit_id,
            Some(AgentUnitId("agent-parent".to_string()))
        );
        assert_eq!(
            unit.certificate.lineage.spawn_request_id.as_deref(),
            Some("spawn-child-1")
        );
    }

    #[test]
    fn record_proof_prints_null_result_for_non_harness_target() {
        let request_json = json!({
            "target": "adapter_diagnostics",
            "family": "wake",
            "kind": "diagnostic.only",
            "summary": "diagnostic proof not bound for harness persistence",
            "redacted": true,
            "policy": {
                "allowed": true,
                "reason": "allowed",
                "message": "ok"
            },
            "harness_request": null,
            "created_at": "2026-01-01T00:00:00Z"
        })
        .to_string();
        let mut output = Vec::new();

        execute(
            CliCommand::RecordProof {
                root: PathBuf::from("/tmp/unused-onecontext-root"),
                request_json,
            },
            &mut output,
        )
        .unwrap();
        let value: Value = serde_json::from_slice(&output).unwrap();

        assert_eq!(
            value["kind"],
            "onecontext.codex_adapter.record_proof_result"
        );
        assert_eq!(value["recorded"], false);
        assert_eq!(value["target"], "adapter_diagnostics");
        assert!(value["unit"].is_null());
    }

    #[test]
    fn record_proof_prints_updated_harness_unit_json() {
        let temp = tempfile::tempdir().unwrap();
        AgentHarnessStore::new(temp.path())
            .call(parent_call_request("agent-parent"))
            .unwrap();
        let request_json = json!({
            "target": "agent_harness",
            "family": "wake",
            "kind": "codex.wake_attempt",
            "summary": "wake accepted",
            "redacted": true,
            "policy": {
                "allowed": true,
                "reason": "allowed",
                "message": "ok"
            },
            "harness_request": AdapterEventRequest {
                unit_id: AgentUnitId("agent-parent".to_string()),
                adapter: AdapterKind::CodexAppServer,
                kind: AdapterEventKind::RuntimeWakeupAccepted,
                status: AdapterEventStatus::Accepted,
                correlation: AdapterCorrelation {
                    notification_id: Some("notification-1".to_string()),
                    ..AdapterCorrelation::default()
                },
                evidence: json!({ "attempt_id": "wake-1" }),
                redaction: AdapterRedaction::default(),
            },
            "created_at": "2026-01-01T00:00:00Z"
        })
        .to_string();
        let mut output = Vec::new();

        execute(
            CliCommand::RecordProof {
                root: temp.path().to_path_buf(),
                request_json,
            },
            &mut output,
        )
        .unwrap();
        let unit: AgentHarnessUnit = serde_json::from_slice(&output).unwrap();

        assert_eq!(unit.unit_id.0, "agent-parent");
        assert_eq!(unit.adapter_events.len(), 1);
        assert_eq!(
            unit.adapter_events[0].kind,
            AdapterEventKind::RuntimeWakeupAccepted
        );
    }
}
