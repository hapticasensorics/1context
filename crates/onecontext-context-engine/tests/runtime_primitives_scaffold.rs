use onecontext_context_engine::artifacts::ArtifactStore;
use onecontext_context_engine::conditions::{ConditionContext, ConditionStatus};
use onecontext_context_engine::fanout::FanoutContext;
use onecontext_context_engine::orchestration::{
    build_executable_orchestration_plan, ExecutableOrchestrationPlan,
};
use onecontext_context_engine::orchestrator::load_wiki_company_orchestrator_config;
use onecontext_context_engine::pack::load_wiki_company_pack;
use onecontext_context_engine::packet_planner::{
    build_packet_plan, PacketPlannerPolicy, SourceEvent,
};
use onecontext_context_engine::run_state::{load_or_new_state, save_state};
use onecontext_context_engine::runtime_executor::{
    build_runtime_execution_scaffold, RuntimeExecutionScaffoldRequest,
};
use onecontext_context_engine::scheduler::{build_scheduler_run_plan, SchedulerRunRequest};
use onecontext_context_engine::source_packets::{
    materialize_source_packets, MaterializedSourcePacket, SourcePacketMaterializationRequest,
};
use onecontext_context_engine::wiki_publish::{
    draft_from_status, write_pages_and_publish_scaffold, WikiPublishRequest,
};
use onecontext_context_engine::ContextEnginePaths;
use serde_json::json;
use std::path::PathBuf;

#[test]
fn runtime_primitives_fit_together_for_tiny_scribe_slice() {
    let temp = tempfile::tempdir().expect("tempdir");
    let temp_paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    temp_paths.ensure_release_dirs().expect("release dirs");

    let events = vec![SourceEvent {
        ts: "2026-06-07T09:15:00Z".to_string(),
        session_id: "session-a".to_string(),
        kind: "assistant_summary".to_string(),
        text: "User asked to replace the old Python wiki company with Rust primitives.".to_string(),
        source: Some("fixture".to_string()),
        cwd: Some("/tmp/project".to_string()),
        project_key: Some("1context".to_string()),
        char_count: None,
        ..SourceEvent::default()
    }];
    let packet_plan = build_packet_plan(
        &events,
        PacketPlannerPolicy {
            window_days: 3,
            max_packets_per_run: 1,
            ..PacketPlannerPolicy::default()
        },
        &Default::default(),
    );
    let packet_result = materialize_source_packets(
        &temp_paths,
        &packet_plan,
        &events,
        SourcePacketMaterializationRequest {
            run_id: "tiny-runtime".to_string(),
            source_window_days: 3,
            current_pages: Vec::new(),
        },
    )
    .expect("materialize source packets");
    assert_eq!(packet_result.packet_count, 1);
    assert!(PathBuf::from(&packet_result.packets[0].path).is_file());

    let artifact_store = ArtifactStore::from_paths(&temp_paths);
    let source_artifact = artifact_store
        .write_json(
            "tiny-runtime",
            "source_packet",
            &packet_result.packets[0].packet_id,
            json!({ "path": packet_result.packets[0].path }),
            Default::default(),
        )
        .expect("write source packet artifact");

    let repo_paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&repo_paths).expect("load pack");
    let orchestrator =
        load_wiki_company_orchestrator_config(&repo_paths).expect("load orchestrator");
    let executable =
        build_executable_orchestration_plan(&pack, &orchestrator).expect("executable plan");
    let schedule = build_scheduler_run_plan(
        &executable,
        SchedulerRunRequest {
            run_id: "tiny-runtime".to_string(),
            max_concurrent_agents: 2,
            completed_phase_ids: vec![
                "import_perception".to_string(),
                "plan_scribe_packets".to_string(),
            ],
            available_artifacts: vec!["source_packet".to_string()],
            available_mail: Vec::new(),
        },
    );

    let runtime = build_runtime_execution_scaffold(RuntimeExecutionScaffoldRequest {
        run_id: "tiny-runtime".to_string(),
        schedule,
        fanout_context: FanoutContext {
            run_id: "tiny-runtime".to_string(),
            selected_packets: packet_result.packets.clone(),
            refreshed_days: Vec::new(),
            available_artifacts: vec![source_artifact.clone()],
        },
        condition_context: ConditionContext::default(),
    });
    assert_eq!(runtime.ready_phase_ids, vec!["wake_scribes"]);
    assert_eq!(runtime.runnable_units.len(), 1);
    assert_eq!(runtime.runnable_units[0].job_id, "memory.hourly.scribe");
    assert_eq!(runtime.runnable_turns.len(), 1);
    assert_eq!(runtime.runnable_turns[0].job_id, "memory.hourly.scribe");
    assert_eq!(runtime.runnable_turns[0].bindings["date"], "2026-06-07");
    assert_eq!(runtime.runnable_turns[0].bindings["hour"], "09");
    assert_eq!(
        runtime.runnable_turns[0].unit_scope["packet.date"],
        "2026-06-07"
    );
    assert!(runtime.runnable_turns[0]
        .source_packet_path
        .as_ref()
        .is_some_and(|path| PathBuf::from(path).is_file()));

    let mut state = load_or_new_state(&temp_paths, "tiny-runtime").expect("new state");
    state.add_artifact(source_artifact);
    state.mark_phase_completed("plan_scribe_packets");
    let state_path = save_state(&temp_paths, &state).expect("save state");
    assert!(state_path.is_file());

    let publish = write_pages_and_publish_scaffold(
        &temp_paths,
        WikiPublishRequest {
            run_id: "tiny-runtime".to_string(),
            drafts: vec![draft_from_status(
                "for-you",
                "For You",
                "A tiny scaffold publish proof.",
            )],
        },
    )
    .expect("publish scaffold");
    assert_eq!(publish.status, "complete");
    assert_eq!(publish.page_receipts.len(), 1);
    assert!(PathBuf::from(publish.publish_proof_path).is_file());
}

#[test]
fn runtime_scaffold_expands_hour_aggregate_when_scribe_artifacts_are_ready() {
    let executable = shipped_executable_plan();
    let schedule = build_scheduler_run_plan(
        &executable,
        SchedulerRunRequest {
            run_id: "aggregate-runtime".to_string(),
            max_concurrent_agents: 5,
            completed_phase_ids: vec![
                "import_perception".to_string(),
                "plan_scribe_packets".to_string(),
            ],
            available_artifacts: vec!["source_packet".to_string(), "scribe_artifacts".to_string()],
            available_mail: Vec::new(),
        },
    );

    let runtime = build_runtime_execution_scaffold(RuntimeExecutionScaffoldRequest {
        run_id: "aggregate-runtime".to_string(),
        schedule,
        fanout_context: FanoutContext {
            run_id: "aggregate-runtime".to_string(),
            selected_packets: vec![
                packet("packet-a", "2026-06-07", "09"),
                packet("packet-b", "2026-06-07", "09"),
            ],
            refreshed_days: Vec::new(),
            available_artifacts: Vec::new(),
        },
        condition_context: ConditionContext::default()
            .with_fact("packet.hour_has_multiple_scribe_artifacts", true),
    });

    let aggregate = runtime
        .runnable_turns
        .iter()
        .find(|turn| turn.job_id == "memory.hourly.aggregate_scribe")
        .expect("aggregate turn");
    assert_eq!(aggregate.date.as_deref(), Some("2026-06-07"));
    assert_eq!(aggregate.hour.as_deref(), Some("09"));
    assert_eq!(aggregate.bindings["date"], "2026-06-07");
    assert_eq!(aggregate.bindings["hour"], "09");
    assert_eq!(aggregate.bindings["hour_id"], "2026-06-07-09");
    assert_eq!(aggregate.unit_scope["hour.id"], "2026-06-07-09");
    assert_eq!(
        aggregate
            .condition
            .as_ref()
            .expect("aggregate condition")
            .status,
        ConditionStatus::True
    );
}

#[test]
fn runtime_scaffold_evaluates_conditional_specialists_in_ready_phase() {
    let temp = tempfile::tempdir().expect("tempdir");
    let temp_paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    temp_paths.ensure_release_dirs().expect("release dirs");
    let daily_artifact = ArtifactStore::from_paths(&temp_paths)
        .write_json(
            "specialist-runtime",
            "daily_editorial_artifacts",
            "2026-06-07",
            json!({ "status": "ready" }),
            Default::default(),
        )
        .expect("write daily editorial artifact");
    let executable = shipped_executable_plan();
    let schedule = build_scheduler_run_plan(
        &executable,
        SchedulerRunRequest {
            run_id: "specialist-runtime".to_string(),
            max_concurrent_agents: 5,
            completed_phase_ids: vec![
                "import_perception".to_string(),
                "plan_scribe_packets".to_string(),
                "wake_scribes".to_string(),
                "historian_questions".to_string(),
                "hourly_answers".to_string(),
                "for_you_editor".to_string(),
            ],
            available_artifacts: vec!["daily_editorial_artifacts".to_string()],
            available_mail: Vec::new(),
        },
    );

    let runtime = build_runtime_execution_scaffold(RuntimeExecutionScaffoldRequest {
        run_id: "specialist-runtime".to_string(),
        schedule,
        fanout_context: FanoutContext {
            run_id: "specialist-runtime".to_string(),
            selected_packets: Vec::new(),
            refreshed_days: vec!["2026-06-07".to_string()],
            available_artifacts: vec![daily_artifact.clone()],
        },
        condition_context: ConditionContext::with_known_artifacts([daily_artifact])
            .with_fact("recent_page_or_concept_changes", false),
    });

    let runnable_jobs = runtime
        .runnable_turns
        .iter()
        .map(|turn| turn.job_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        runnable_jobs,
        vec!["memory.wiki.biographer", "memory.wiki.librarian"]
    );
    assert!(runtime
        .runnable_turns
        .iter()
        .all(|turn| turn.bindings["era"] == "2026-06-07"));
    assert!(runtime.blocked_jobs.iter().any(|job| {
        job.job_id == "memory.wiki.contradiction_flagger"
            && job.reason == "condition false: recent_page_or_concept_changes"
    }));
}

fn shipped_executable_plan() -> ExecutableOrchestrationPlan {
    let repo_paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&repo_paths).expect("load pack");
    let orchestrator =
        load_wiki_company_orchestrator_config(&repo_paths).expect("load orchestrator");
    build_executable_orchestration_plan(&pack, &orchestrator).expect("executable plan")
}

fn packet(packet_id: &str, date: &str, hour: &str) -> MaterializedSourcePacket {
    MaterializedSourcePacket {
        schema_version: 1,
        kind: "onecontext.context_engine.materialized_source_packet.v1".to_string(),
        run_id: "runtime-test".to_string(),
        packet_id: packet_id.to_string(),
        packet_kind: "hour".to_string(),
        date: date.to_string(),
        hour: hour.to_string(),
        shard_index: 0,
        shard_count: 1,
        event_count: 1,
        char_count: 32,
        session_ids: vec![format!("session-{packet_id}")],
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
        estimated_tokens: 100,
        target_packet_tokens: 160_000,
        source_window_days: 3,
        page_snapshot_count: 0,
        page_snapshots: Vec::new(),
        path: format!("context-engine/live/runs/runtime-test/source-packets/{packet_id}.md"),
        metadata_path: format!(
            "context-engine/live/runs/runtime-test/source-packets/{packet_id}.json"
        ),
        cache_path: format!(
            "context-engine/live/state/wiki-memory-cache/scribe-packets/{packet_id}.json"
        ),
        source_packet_hash: format!("hash-{packet_id}"),
        content_sha256: format!("hash-{packet_id}"),
        created_at: "2026-06-07T09:00:00Z".to_string(),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives under repo/crates/onecontext-context-engine")
        .to_path_buf()
}
