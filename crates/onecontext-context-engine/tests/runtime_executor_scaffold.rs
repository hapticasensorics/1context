use onecontext_context_engine::artifacts::ArtifactHandle;
use onecontext_context_engine::conditions::{ConditionContext, ConditionStatus};
use onecontext_context_engine::fanout::FanoutContext;
use onecontext_context_engine::harness_executor::agent_mail_address_for_unit;
use onecontext_context_engine::orchestration::build_executable_orchestration_plan;
use onecontext_context_engine::orchestrator::load_wiki_company_orchestrator_config;
use onecontext_context_engine::pack::load_wiki_company_pack;
use onecontext_context_engine::runtime_executor::{
    build_runtime_execution_scaffold, RuntimeExecutionScaffoldRequest,
};
use onecontext_context_engine::scheduler::{
    build_scheduler_run_plan, SchedulerRunPlan, SchedulerRunRequest,
};
use onecontext_context_engine::source_packets::MaterializedSourcePacket;
use onecontext_context_engine::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use std::path::PathBuf;

#[test]
fn ready_scribe_jobs_become_harness_suitable_turn_descriptors() {
    let run_id = "runtime/turns";
    let source_packet = packet("packet-a", "2026-06-07", "09");
    let schedule = shipped_schedule(
        run_id,
        ["import_perception", "plan_scribe_packets"],
        ["source_packet"],
    );

    let runtime = build_runtime_execution_scaffold(RuntimeExecutionScaffoldRequest {
        run_id: run_id.to_string(),
        schedule,
        fanout_context: FanoutContext {
            run_id: run_id.to_string(),
            selected_packets: vec![source_packet.clone()],
            refreshed_days: Vec::new(),
            available_artifacts: vec![artifact("source_packet")],
        },
        condition_context: ConditionContext::default(),
    });

    let safe_run = safe_run_id(run_id);
    assert_eq!(runtime.run_id, safe_run);
    assert_eq!(runtime.ready_phase_ids, vec!["wake_scribes"]);
    assert_eq!(runtime.runnable_units.len(), 1);
    assert_eq!(runtime.runnable_turns.len(), 1);
    assert!(runtime.conditional_turns.is_empty());

    let turn = &runtime.runnable_turns[0];
    assert_eq!(turn.status, "runnable");
    assert_eq!(turn.job_id, "memory.hourly.scribe");
    assert_eq!(turn.phase_id, "wake_scribes");
    assert_eq!(turn.packet_id.as_deref(), Some("packet-a"));
    assert_eq!(turn.date.as_deref(), Some("2026-06-07"));
    assert_eq!(turn.hour.as_deref(), Some("09"));
    assert_eq!(turn.bindings["source_packet_path"], "/tmp/packet-a.md");
    assert_eq!(turn.source_packet_hash.as_deref(), Some("sha-packet-a"));
    assert_eq!(turn.route.thread_id, "mail://wiki-company");
    assert_eq!(turn.route.to, vec!["role://memory.wiki.historian"]);
    assert_eq!(
        turn.route.from_mailbox,
        agent_mail_address_for_unit(&turn.harness_unit_id)
    );
    assert_eq!(turn.receipt_expectations.talk_delivery_mode, "mail");
    assert!(turn.receipt_expectations.require_mail_delivery);
    assert!(turn
        .final_message_path
        .starts_with(&format!("context-engine/live/runs/{safe_run}/turns/")));
    assert!(turn
        .final_message_path
        .ends_with("/attempt-0001/final-message.md"));

    let aggregate_block = runtime
        .blocked_jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.aggregate_scribe")
        .expect("aggregate scribe should wait for upstream artifacts");
    assert_eq!(aggregate_block.scheduler_status, "waiting_for_artifacts");
    assert_eq!(
        aggregate_block.missing_required_artifacts,
        vec!["scribe_artifacts"]
    );
    assert_eq!(
        aggregate_block.reason,
        "waiting for artifacts: scribe_artifacts"
    );
}

#[test]
fn conditional_ready_jobs_are_evaluated_before_turn_selection() {
    let run_id = "runtime-specialists";
    let schedule = shipped_schedule(
        run_id,
        [
            "import_perception",
            "plan_scribe_packets",
            "wake_scribes",
            "historian_questions",
            "hourly_answers",
            "for_you_editor",
        ],
        ["daily_editorial_artifacts"],
    );

    let runtime = build_runtime_execution_scaffold(RuntimeExecutionScaffoldRequest {
        run_id: run_id.to_string(),
        schedule,
        fanout_context: FanoutContext {
            run_id: run_id.to_string(),
            selected_packets: Vec::new(),
            refreshed_days: Vec::new(),
            available_artifacts: vec![artifact("daily_editorial_artifacts")],
        },
        condition_context: ConditionContext::with_known_artifacts([artifact(
            "daily_editorial_artifacts",
        )])
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

    let blocked = runtime
        .blocked_jobs
        .iter()
        .find(|job| job.job_id == "memory.wiki.contradiction_flagger")
        .expect("false condition should block the contradiction flagger");
    assert_eq!(
        blocked
            .condition
            .as_ref()
            .map(|condition| &condition.status),
        Some(&ConditionStatus::False)
    );
    assert_eq!(
        blocked.reason,
        "condition false: recent_page_or_concept_changes"
    );
}

#[test]
fn answerer_waiting_for_historian_mail_is_not_a_runnable_turn() {
    let run_id = "runtime-answerer-gate";
    let schedule = shipped_schedule(
        run_id,
        [
            "import_perception",
            "plan_scribe_packets",
            "wake_scribes",
            "historian_questions",
        ],
        ["hourly_answer_questions"],
    );

    let runtime = build_runtime_execution_scaffold(RuntimeExecutionScaffoldRequest {
        run_id: run_id.to_string(),
        schedule,
        fanout_context: FanoutContext {
            run_id: run_id.to_string(),
            selected_packets: Vec::new(),
            refreshed_days: Vec::new(),
            available_artifacts: vec![artifact("hourly_answer_questions")],
        },
        condition_context: ConditionContext::default(),
    });

    assert!(!runtime
        .runnable_turns
        .iter()
        .any(|turn| turn.job_id == "memory.hourly.answerer"));
    let blocked = runtime
        .blocked_jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.answerer")
        .expect("answerer should be blocked by missing historian mail");
    assert_eq!(blocked.scheduler_status, "waiting_for_mail");
    assert_eq!(
        blocked.missing_required_mail,
        vec!["job:memory.wiki.historian"]
    );
}

#[test]
fn unknown_conditions_keep_expanded_units_in_conditional_turns() {
    let run_id = "runtime-unknown-condition";
    let mut schedule = shipped_schedule(
        run_id,
        [
            "import_perception",
            "plan_scribe_packets",
            "wake_scribes",
            "historian_questions",
            "hourly_answers",
            "for_you_editor",
        ],
        ["daily_editorial_artifacts"],
    );
    let unknown_job = schedule
        .phases
        .iter_mut()
        .flat_map(|phase| phase.jobs.iter_mut())
        .find(|job| job.job_id == "memory.wiki.contradiction_flagger")
        .expect("contradiction flagger job");
    unknown_job.when = Some("runtime.unmodeled_gate".to_string());

    let runtime = build_runtime_execution_scaffold(RuntimeExecutionScaffoldRequest {
        run_id: run_id.to_string(),
        schedule,
        fanout_context: FanoutContext {
            run_id: run_id.to_string(),
            selected_packets: Vec::new(),
            refreshed_days: Vec::new(),
            available_artifacts: vec![artifact("daily_editorial_artifacts")],
        },
        condition_context: ConditionContext::with_known_artifacts([artifact(
            "daily_editorial_artifacts",
        )]),
    });

    let conditional = runtime
        .conditional_turns
        .iter()
        .find(|turn| turn.job_id == "memory.wiki.contradiction_flagger")
        .expect("unknown condition keeps a descriptor for later execution");
    assert_eq!(conditional.status, "conditional");
    assert_eq!(
        conditional
            .condition
            .as_ref()
            .map(|condition| &condition.status),
        Some(&ConditionStatus::Unknown)
    );

    let blocked = runtime
        .blocked_jobs
        .iter()
        .find(|job| job.job_id == "memory.wiki.contradiction_flagger")
        .expect("unknown condition should be reported as a gate");
    assert_eq!(
        blocked.reason,
        "condition unknown: runtime.unmodeled_gate (runtime condition fact was not supplied)"
    );
}

fn shipped_schedule<'a>(
    run_id: &str,
    completed_phase_ids: impl IntoIterator<Item = &'a str>,
    available_artifacts: impl IntoIterator<Item = &'a str>,
) -> SchedulerRunPlan {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let orchestrator =
        load_wiki_company_orchestrator_config(&paths).expect("load shipped orchestrator");
    let executable =
        build_executable_orchestration_plan(&pack, &orchestrator).expect("build executable plan");
    build_scheduler_run_plan(
        &executable,
        SchedulerRunRequest {
            run_id: run_id.to_string(),
            max_concurrent_agents: 5,
            completed_phase_ids: completed_phase_ids
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
            available_artifacts: available_artifacts
                .into_iter()
                .map(ToOwned::to_owned)
                .collect(),
            available_mail: Vec::new(),
        },
    )
}

fn packet(packet_id: &str, date: &str, hour: &str) -> MaterializedSourcePacket {
    MaterializedSourcePacket {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.materialized_source_packet.v1".to_string(),
        run_id: "runtime-turns".to_string(),
        packet_id: packet_id.to_string(),
        packet_kind: "recent_hour".to_string(),
        date: date.to_string(),
        hour: hour.to_string(),
        shard_index: 0,
        shard_count: 1,
        event_count: 2,
        char_count: 24,
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
        estimated_tokens: 42,
        target_packet_tokens: 160_000,
        source_window_days: 3,
        page_snapshot_count: 0,
        page_snapshots: Vec::new(),
        path: format!("/tmp/{packet_id}.md"),
        metadata_path: format!("/tmp/{packet_id}.json"),
        cache_path: format!("/tmp/cache/{packet_id}.json"),
        source_packet_hash: format!("sha-{packet_id}"),
        content_sha256: format!("sha-{packet_id}"),
        created_at: "2026-06-07T00:00:00Z".to_string(),
    }
}

fn artifact(kind: &str) -> ArtifactHandle {
    ArtifactHandle {
        artifact_id: format!("{kind}-artifact"),
        kind: kind.to_string(),
        run_id: "runtime-turns".to_string(),
        key: kind.to_string(),
        path: format!("/tmp/{kind}.json"),
        content_sha256: format!("sha-{kind}"),
        created_at: "2026-06-07T00:00:00Z".to_string(),
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives under repo/crates/onecontext-context-engine")
        .to_path_buf()
}
