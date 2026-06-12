use onecontext_context_engine::agent_execution::codex_worker_thread_config;
use onecontext_context_engine::artifacts::ArtifactHandle;
use onecontext_context_engine::conditions::ConditionContext;
use onecontext_context_engine::fanout::FanoutContext;
use onecontext_context_engine::harness_executor::{
    agent_mail_address_for_unit, build_harness_turn_request,
    build_harness_turn_request_for_runtime_turn, evaluate_harness_turn_completion,
    HarnessTurnReceipts,
};
use onecontext_context_engine::orchestration::build_executable_orchestration_plan;
use onecontext_context_engine::orchestrator::load_wiki_company_orchestrator_config;
use onecontext_context_engine::pack::{load_wiki_company_pack, validate_wiki_company_pack};
use onecontext_context_engine::runtime_executor::{
    build_runtime_execution_scaffold, RuntimeExecutionScaffoldRequest,
};
use onecontext_context_engine::scheduler::{build_scheduler_run_plan, SchedulerRunRequest};
use onecontext_context_engine::source_packets::MaterializedSourcePacket;
use onecontext_context_engine::ContextEnginePaths;
use std::fs;
use std::path::PathBuf;

#[test]
fn builds_codex_app_server_harness_turn_request_for_for_you_curator() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let report = validate_wiki_company_pack(&paths, &pack);
    assert!(
        report.is_valid(),
        "pack validation issues: {:#?}",
        report.issues
    );

    let request = build_harness_turn_request(
        &paths,
        &pack,
        "release-demo-20260605",
        "memory.wiki.for_you_curator",
    )
    .expect("build for-you curator harness turn request");

    assert_eq!(request.kind, "onecontext.agent_harness.turn_request.v1");
    assert_eq!(request.harness.id, "codex-app-server");
    assert_eq!(request.harness.runner, "onecontext-codex-adapter");
    assert_eq!(request.harness.transport, "codex-app-server");
    assert_eq!(
        request.harness.command,
        "onecontext-codex-adapter spawn-child"
    );
    assert_eq!(request.agent.id, "for-you-curator");
    assert_eq!(request.agent.provider, "codex");
    assert_eq!(request.agent.model, "gpt-5.5");
    assert_eq!(request.agent.reasoning_effort, "xhigh");
    assert_eq!(request.agent.memory_mode, "persistent");
    assert_eq!(request.agent.memory_attach, "last_for_job");
    assert!(request.agent.persistent_session);
    assert_eq!(request.unit_id, "wiki-for-you-curator");
    assert_eq!(request.job.id, "memory.wiki.for_you_curator");
    assert_eq!(
        request.job.outputs,
        vec![
            "article_section_updates",
            "decided_entries",
            "concern_entries"
        ]
    );
    assert!(request
        .tool_policy
        .default_tools
        .contains(&"wiki.talk.append".to_string()));
    assert!(request
        .tool_policy
        .default_tools
        .contains(&"agent.mail.write".to_string()));

    let prompt_paths = request
        .prompt_bundle
        .iter()
        .map(|prompt| prompt.path.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        prompt_paths,
        vec![
            "prompts/codex-app-server.instructions.md",
            "prompts/agent-profile.md",
            "prompts/wiki-curator.md",
            "prompts/wiki-talk-style-guide.md",
            "prompts/for-you-talk-conventions.md",
            "prompts/operator-touched-convention.md",
            "prompts/for-you-curator.md"
        ]
    );
    assert!(request
        .prompt_text
        .contains("harness-born 1Context agent through the Codex app-server"));
    assert!(request.prompt_text.contains("agentic Wikipedia"));
    assert!(request.prompt_text.contains("## Agent Mail Context"));
    assert!(request
        .prompt_text
        .contains("Write a non-empty final-message.md"));
    assert!(request
        .prompt_text
        .contains("the 1Context harness will append it to wiki talk and Agent Mail"));
    assert!(request
        .prompt_text
        .contains("Do not call wiki.talk.append for your final report"));
    assert!(!request
        .prompt_text
        .contains("Post your final report through wiki.talk.append"));

    assert_eq!(request.mail_context.thread_id, "mail://wiki-company");
    assert_eq!(request.talk_report.delivery_mode, "mail");
    assert_eq!(request.talk_report.thread_id, "job.run_id");
    assert_eq!(
        request.talk_report.from,
        agent_mail_address_for_unit(&request.unit_id)
    );
    assert_eq!(request.talk_report.to, vec!["mailbox://page/for-you"]);
    assert!(request.required_receipts.require_birth_certificate);
    assert!(request.required_receipts.require_turn_start);
    assert!(request.required_receipts.require_context_injection);
    assert!(request.required_receipts.require_adapter_events);
    assert!(request.required_receipts.require_final_message);
    assert!(request.required_receipts.require_talk_append);
    assert!(request.required_receipts.require_mail_delivery);
    assert!(request.required_receipts.require_turn_complete);
    assert!(request
        .required_receipts
        .final_message_path
        .starts_with("context-engine/live/runs/release-demo-20260605/turns/"));
    assert!(request
        .required_receipts
        .final_message_path
        .ends_with("/attempt-0001/final-message.md"));
}

#[test]
fn harness_default_leaves_native_multi_agent_off_for_unflagged_agents() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let report = validate_wiki_company_pack(&paths, &pack);
    assert!(
        report.is_valid(),
        "pack validation issues: {:#?}",
        report.issues
    );

    let request = build_harness_turn_request(
        &paths,
        &pack,
        "release-demo-20260605",
        "memory.hourly.scribe",
    )
    .expect("build hourly scribe harness turn request");

    assert_eq!(request.agent.id, "hourly-scribe");
    assert!(!request.agent.persistent_session);
    assert_eq!(
        request.unit_id,
        "memory.hourly.scribe-tmp-release-demo-20260605"
    );
    let run_id = request
        .unit_id
        .rsplit_once("-tmp-")
        .map(|(_, run_id)| run_id)
        .expect("ephemeral unit id should end with -tmp-<run-id>");
    assert_eq!(run_id, "release-demo-20260605");
}

#[test]
fn codex_app_server_worker_config_excludes_native_agent_tools_without_disabling_model_tools() {
    let config = codex_worker_thread_config();

    assert_eq!(config["onecontext_worker_turn"].as_bool(), Some(true));
    assert_eq!(config["features.multi_agent"].as_bool(), Some(false));
    assert!(multi_agent_v2_disabled(&config));
    assert_eq!(config.get("tools"), None);
    assert_eq!(config.get("enabled_tools"), None);
    assert_eq!(config.get("disabled_tools"), None);
    assert_eq!(
        config.get("features.multi_agent_v2.encrypted_messages"),
        None
    );
    assert_eq!(config.get("features.code_mode"), None);
    assert_eq!(config.get("features.code_mode_only"), None);

    let serialized = config.to_string();
    for native_tool in [
        "spawn_agent",
        "send_message",
        "followup_task",
        "wait_agent",
        "list_agents",
    ] {
        assert!(
            !serialized.contains(native_tool),
            "worker config should not name native agent tool {native_tool}"
        );
    }
}

fn multi_agent_v2_disabled(config: &serde_json::Value) -> bool {
    let values = [
        config
            .get("features.multi_agent_v2")
            .and_then(serde_json::Value::as_bool),
        config
            .get("features.multi_agent_v2.enabled")
            .and_then(serde_json::Value::as_bool),
    ];
    values.iter().flatten().any(|value| !*value) && !values.iter().flatten().any(|value| *value)
}

#[test]
fn harness_request_rejects_unknown_jobs() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let error = build_harness_turn_request(&paths, &pack, "run", "missing.job")
        .expect_err("unknown job should fail before launching a harness turn");
    assert!(error.contains("unknown job"));
}

#[test]
fn runtime_turn_bridge_injects_materialized_source_packet() {
    let temp = tempfile::tempdir().expect("tempdir");
    let packet_path = temp.path().join("packet-a.md");
    let packet_body = "# Bounded Scribe Source Packet\n\nONLY_TINY_LIVE_PACKET_BODY\n";
    fs::write(&packet_path, packet_body).expect("packet body");

    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let orchestrator =
        load_wiki_company_orchestrator_config(&paths).expect("load shipped orchestrator");
    let executable =
        build_executable_orchestration_plan(&pack, &orchestrator).expect("build executable plan");
    let schedule = build_scheduler_run_plan(
        &executable,
        SchedulerRunRequest {
            run_id: "runtime-bridge".to_string(),
            max_concurrent_agents: 1,
            completed_phase_ids: vec![
                "import_perception".to_string(),
                "plan_scribe_packets".to_string(),
            ],
            available_artifacts: vec!["source_packet".to_string()],
            available_mail: Vec::new(),
        },
    );
    let runtime = build_runtime_execution_scaffold(RuntimeExecutionScaffoldRequest {
        run_id: "runtime-bridge".to_string(),
        schedule,
        fanout_context: FanoutContext {
            run_id: "runtime-bridge".to_string(),
            selected_packets: vec![materialized_packet(&packet_path)],
            refreshed_days: Vec::new(),
            available_artifacts: vec![artifact("source_packet")],
        },
        condition_context: ConditionContext::default(),
    });
    let turn = runtime
        .runnable_turns
        .iter()
        .find(|turn| turn.job_id == "memory.hourly.scribe")
        .expect("scribe runtime turn");

    let request = build_harness_turn_request_for_runtime_turn(&paths, &pack, &orchestrator, turn)
        .expect("runtime harness request");

    assert_eq!(request.operation_id, turn.operation_id);
    assert_eq!(request.unit_id, turn.harness_unit_id);
    assert_eq!(request.source_packet.packet_id, "packet-a");
    assert_eq!(
        request.source_packet.path.as_deref(),
        Some(packet_path.display().to_string().as_str())
    );
    assert_eq!(
        request.source_packet.content_sha256.as_deref(),
        Some("sha-packet-a")
    );
    assert!(request.source_packet.bytes.unwrap_or_default() > 0);
    assert!(request
        .prompt_bundle
        .iter()
        .any(|part| part.source == "runtime-source-packet"
            && part.text.contains("ONLY_TINY_LIVE_PACKET_BODY")));
    assert!(request.prompt_text.contains("ONLY_TINY_LIVE_PACKET_BODY"));
    assert_eq!(request.talk_report.from, turn.route.from_mailbox);
    assert_eq!(request.talk_report.to, turn.route.to);
    assert_eq!(
        request.required_receipts.final_message_path,
        turn.final_message_path
    );
}

#[test]
fn harness_completion_requires_all_required_receipts() {
    let request = for_you_curator_request();
    let completion = evaluate_harness_turn_completion(&request, &complete_receipts());

    assert!(completion.complete);
    assert_eq!(completion.status, "complete");
    assert!(completion.issues.is_empty());
}

#[test]
fn harness_completion_rejects_missing_final_message() {
    let request = for_you_curator_request();
    let mut receipts = complete_receipts();
    receipts.final_message = Some("   ".to_string());
    let completion = evaluate_harness_turn_completion(&request, &receipts);

    assert!(!completion.complete);
    assert!(completion
        .issues
        .iter()
        .any(|issue| issue.contains("missing non-empty final message")));
}

#[test]
fn harness_completion_rejects_missing_talk_or_mail_receipt() {
    let request = for_you_curator_request();
    let mut receipts = complete_receipts();
    receipts.talk_receipt = None;
    receipts.mail_receipt = None;
    let completion = evaluate_harness_turn_completion(&request, &receipts);

    assert!(!completion.complete);
    assert!(completion
        .issues
        .iter()
        .any(|issue| issue.contains("wiki.talk.append")));
    assert!(completion
        .issues
        .iter()
        .any(|issue| issue.contains("Agent Mail delivery")));
}

#[test]
fn harness_completion_rejects_audit_jsonl_as_agent_mail_receipt() {
    let request = for_you_curator_request();
    let mut receipts = complete_receipts();
    receipts.mail_receipt = Some("context-engine/live/mail/threads/wiki-company.jsonl".to_string());
    let completion = evaluate_harness_turn_completion(&request, &receipts);

    assert!(!completion.complete);
    assert!(completion
        .issues
        .iter()
        .any(|issue| issue == "missing Agent Mail delivery receipt"));
}

#[test]
fn harness_completion_rejects_mail_thread_uri_as_agent_mail_receipt() {
    let request = for_you_curator_request();
    let mut receipts = complete_receipts();
    receipts.mail_receipt = Some("mail://wiki-company".to_string());
    let completion = evaluate_harness_turn_completion(&request, &receipts);

    assert!(!completion.complete);
    assert!(completion
        .issues
        .iter()
        .any(|issue| issue == "missing Agent Mail delivery receipt"));
}

#[test]
fn harness_completion_rejects_codex_exit_only() {
    let request = for_you_curator_request();
    let receipts = HarnessTurnReceipts {
        codex_exit_status: Some(0),
        ..HarnessTurnReceipts::default()
    };
    let completion = evaluate_harness_turn_completion(&request, &receipts);

    assert!(!completion.complete);
    assert!(completion
        .issues
        .iter()
        .any(|issue| issue == "codex exit status alone is not a completion receipt"));
}

#[test]
fn harness_completion_rejects_failed_codex_worker_turn() {
    let request = for_you_curator_request();
    let mut receipts = complete_receipts();
    receipts.codex_exit_status = Some(1);
    let completion = evaluate_harness_turn_completion(&request, &receipts);

    assert!(!completion.complete);
    assert!(completion
        .issues
        .iter()
        .any(|issue| issue == "codex adapter did not report a successful worker turn"));
}

fn materialized_packet(path: &std::path::Path) -> MaterializedSourcePacket {
    MaterializedSourcePacket {
        schema_version: onecontext_context_engine::CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.materialized_source_packet.v1".to_string(),
        run_id: "runtime-bridge".to_string(),
        packet_id: "packet-a".to_string(),
        packet_kind: "hour".to_string(),
        date: "2026-06-07".to_string(),
        hour: "09".to_string(),
        shard_index: 1,
        shard_count: 1,
        event_count: 1,
        char_count: 42,
        session_ids: vec!["session-a".to_string()],
        session_count: 1,
        object_ids: Vec::new(),
        source_ids: Vec::new(),
        source_types: Vec::new(),
        source_keys: Vec::new(),
        source_record_ids: Vec::new(),
        source_record_keys: Vec::new(),
        source_record_hashes: Vec::new(),
        series_ids: Vec::new(),
        series_kinds: Vec::new(),
        series_keys: Vec::new(),
        series_display_names: Vec::new(),
        sources: vec!["fixture".to_string()],
        roles: Vec::new(),
        privacy_classes: Vec::new(),
        body_types: Vec::new(),
        project_keys: vec!["1context".to_string()],
        cwd_values: vec!["/tmp/project".to_string()],
        estimated_tokens: 16,
        target_packet_tokens: 160_000,
        source_window_days: 1,
        page_snapshot_count: 0,
        page_snapshots: Vec::new(),
        path: path.display().to_string(),
        metadata_path: "/tmp/packet-a.json".to_string(),
        cache_path: "/tmp/cache/sha-packet-a.json".to_string(),
        source_packet_hash: "sha-packet-a".to_string(),
        content_sha256: "sha-packet-a".to_string(),
        created_at: "2026-06-07T00:00:00Z".to_string(),
    }
}

fn artifact(kind: &str) -> ArtifactHandle {
    ArtifactHandle {
        artifact_id: format!("{kind}-1"),
        kind: kind.to_string(),
        run_id: "runtime-bridge".to_string(),
        key: "packet-a".to_string(),
        path: format!("/tmp/{kind}.json"),
        content_sha256: "sha".to_string(),
        created_at: "2026-06-07T00:00:00Z".to_string(),
    }
}

fn for_you_curator_request() -> onecontext_context_engine::harness_executor::HarnessTurnRequest {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    build_harness_turn_request(
        &paths,
        &pack,
        "release-demo-20260605",
        "memory.wiki.for_you_curator",
    )
    .expect("build for-you curator harness turn request")
}

fn complete_receipts() -> HarnessTurnReceipts {
    HarnessTurnReceipts {
        codex_exit_status: Some(0),
        harness_birth_certificate: true,
        harness_turn_start: true,
        context_injection_receipt: true,
        adapter_events: true,
        final_message: Some(
            "status: complete\n\nevidence: wrote a talk report and proposed wiki diff".to_string(),
        ),
        talk_receipt: Some("wiki.talk.append:delivery=mail:receipt-123".to_string()),
        mail_receipt: Some("agent-mail://mailmsg-final-001/delivery-final-001".to_string()),
        harness_turn_complete: true,
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives under repo/crates/onecontext-context-engine")
        .to_path_buf()
}
