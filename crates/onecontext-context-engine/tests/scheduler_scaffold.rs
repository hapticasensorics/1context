use onecontext_context_engine::orchestration::build_executable_orchestration_plan;
use onecontext_context_engine::orchestrator::{
    load_wiki_company_orchestrator_config, validate_wiki_company_orchestrator,
};
use onecontext_context_engine::pack::{load_wiki_company_pack, validate_wiki_company_pack};
use onecontext_context_engine::scheduler::{
    build_scheduler_run_plan, ready_jobs, SchedulerRunRequest,
};
use onecontext_context_engine::{safe_run_id, ContextEnginePaths};
use std::path::PathBuf;

#[test]
fn shipped_orchestrator_builds_scheduler_plan_from_toml() {
    let executable = shipped_executable_plan();

    let schedule = build_scheduler_run_plan(&executable, SchedulerRunRequest::new("scheduler/run"));

    assert_eq!(
        schedule.kind,
        "onecontext.context_engine.scheduler_run_plan.v1"
    );
    assert_eq!(schedule.run_id, safe_run_id("scheduler/run"));
    assert_eq!(schedule.phase_count, 9);
    assert_eq!(schedule.job_count, executable.job_count);
    assert!(schedule.issues.is_empty(), "{:#?}", schedule.issues);
    assert_eq!(
        schedule
            .phases
            .iter()
            .map(|phase| phase.id.as_str())
            .collect::<Vec<_>>(),
        vec![
            "import_perception",
            "plan_scribe_packets",
            "wake_scribes",
            "historian_questions",
            "hourly_answers",
            "for_you_editor",
            "specialists",
            "curators",
            "publish"
        ]
    );
    assert_eq!(schedule.ready_phase_ids, vec!["import_perception"]);
    assert_eq!(schedule.ready_job_count, 0);
    assert!(schedule.receipt_policy.require_adapter_events);
    assert!(schedule.receipt_policy.require_mail_delivery);
    assert_eq!(schedule.receipt_policy.talk_delivery_mode, "mail");
    assert_eq!(schedule.packet_policy.target_source_tokens, 160_000);
}

#[test]
fn completed_receipts_and_artifacts_unlock_scribe_jobs() {
    let executable = shipped_executable_plan();
    let mut request = SchedulerRunRequest::new("scheduler-scribes");
    request.max_concurrent_agents = 5;
    request.completed_phase_ids = vec![
        "import_perception".to_string(),
        "plan_scribe_packets".to_string(),
    ];
    request.available_artifacts = vec!["source_packet".to_string()];

    let schedule = build_scheduler_run_plan(&executable, request);

    assert_eq!(schedule.ready_phase_ids, vec!["wake_scribes"]);
    let wake_scribes = schedule
        .phases
        .iter()
        .find(|phase| phase.id == "wake_scribes")
        .expect("wake_scribes phase");
    assert_eq!(wake_scribes.status, "ready");
    let scribe = wake_scribes
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.scribe")
        .expect("hourly scribe job");
    assert_eq!(scribe.status, "ready");
    assert_eq!(scribe.fanout, "selected_packets");
    assert_eq!(scribe.bindings["date"], "packet.date");
    assert_eq!(scribe.bindings["talk_folder"], "wiki.page('for-you').talk");
    assert_eq!(scribe.route.to, vec!["role://memory.wiki.historian"]);

    let aggregate = wake_scribes
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.aggregate_scribe")
        .expect("aggregate scribe job");
    assert_eq!(aggregate.status, "waiting_for_artifacts");
    assert_eq!(
        aggregate.missing_required_artifacts,
        vec!["scribe_artifacts"]
    );
    assert_eq!(ready_jobs(&schedule).len(), 1);
}

#[test]
fn scheduler_can_gate_on_scoped_artifact_availability_keys() {
    let mut executable = shipped_executable_plan();
    let wake_scribe_job = executable
        .phases
        .iter_mut()
        .find(|phase| phase.id == "wake_scribes")
        .expect("wake_scribes phase")
        .jobs
        .iter_mut()
        .find(|job| job.job_id == "memory.hourly.scribe")
        .expect("hourly scribe job");
    wake_scribe_job.required_artifacts = vec!["source_packet:key:recent-2026-06-08".to_string()];

    let mut broad_kind_only = SchedulerRunRequest::new("scheduler-scoped-artifacts");
    broad_kind_only.completed_phase_ids = vec![
        "import_perception".to_string(),
        "plan_scribe_packets".to_string(),
    ];
    broad_kind_only.available_artifacts = vec!["source_packet".to_string()];
    let blocked = build_scheduler_run_plan(&executable, broad_kind_only);
    let blocked_scribe = blocked
        .phases
        .iter()
        .find(|phase| phase.id == "wake_scribes")
        .expect("wake_scribes phase")
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.scribe")
        .expect("hourly scribe job");
    assert_eq!(blocked_scribe.status, "waiting_for_artifacts");
    assert_eq!(
        blocked_scribe.missing_required_artifacts,
        vec!["source_packet:key:recent-2026-06-08"]
    );

    let mut scoped_key = SchedulerRunRequest::new("scheduler-scoped-artifacts");
    scoped_key.completed_phase_ids = vec![
        "import_perception".to_string(),
        "plan_scribe_packets".to_string(),
    ];
    scoped_key.available_artifacts = vec![
        "source_packet".to_string(),
        "source_packet:key:recent-2026-06-08".to_string(),
    ];
    let ready = build_scheduler_run_plan(&executable, scoped_key);
    let ready_scribe = ready
        .phases
        .iter()
        .find(|phase| phase.id == "wake_scribes")
        .expect("wake_scribes phase")
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.scribe")
        .expect("hourly scribe job");
    assert_eq!(ready_scribe.status, "ready");
}

#[test]
fn historian_question_artifacts_and_mail_unlock_answerer_phase() {
    let executable = shipped_executable_plan();
    let mut missing_mail = SchedulerRunRequest::new("scheduler-answerer-missing-mail");
    missing_mail.completed_phase_ids = vec![
        "import_perception".to_string(),
        "plan_scribe_packets".to_string(),
        "wake_scribes".to_string(),
        "historian_questions".to_string(),
    ];
    missing_mail.available_artifacts = vec!["hourly_answer_questions".to_string()];

    let blocked = build_scheduler_run_plan(&executable, missing_mail);
    assert_eq!(blocked.ready_phase_ids, vec!["hourly_answers"]);
    let blocked_answerer = blocked
        .phases
        .iter()
        .find(|phase| phase.id == "hourly_answers")
        .expect("hourly answers phase")
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.answerer")
        .expect("answerer job");
    assert_eq!(blocked_answerer.status, "waiting_for_mail");
    assert_eq!(
        blocked_answerer.missing_required_mail,
        vec!["job:memory.wiki.historian"]
    );

    let mut with_mail = SchedulerRunRequest::new("scheduler-answerer-with-mail");
    with_mail.completed_phase_ids = vec![
        "import_perception".to_string(),
        "plan_scribe_packets".to_string(),
        "wake_scribes".to_string(),
        "historian_questions".to_string(),
    ];
    with_mail.available_artifacts = vec!["hourly_answer_questions".to_string()];
    with_mail.available_mail = vec!["job:memory.wiki.historian".to_string()];

    let ready = build_scheduler_run_plan(&executable, with_mail);
    let answerer = ready
        .phases
        .iter()
        .find(|phase| phase.id == "hourly_answers")
        .expect("hourly answers phase")
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.answerer")
        .expect("answerer job");
    assert_eq!(answerer.status, "conditional");
    assert!(answerer.missing_required_mail.is_empty());
}

#[test]
fn publish_phase_only_becomes_ready_after_curator_receipts() {
    let executable = shipped_executable_plan();
    let mut request = SchedulerRunRequest::new("scheduler-publish");
    request.completed_phase_ids = vec![
        "import_perception".to_string(),
        "plan_scribe_packets".to_string(),
        "wake_scribes".to_string(),
        "historian_questions".to_string(),
        "hourly_answers".to_string(),
        "for_you_editor".to_string(),
        "specialists".to_string(),
        "curators".to_string(),
    ];

    let schedule = build_scheduler_run_plan(&executable, request);

    assert_eq!(schedule.ready_phase_ids, vec!["publish"]);
    let publish = schedule
        .phases
        .iter()
        .find(|phase| phase.id == "publish")
        .expect("publish phase");
    assert_eq!(publish.owner, "onecontext-wiki-core");
    assert_eq!(publish.completion, "page_write_and_publish_receipts_ready");
    assert!(publish.jobs.is_empty());
}

#[test]
fn conditional_jobs_wait_for_condition_evaluation_even_when_artifacts_exist() {
    let executable = shipped_executable_plan();
    let mut request = SchedulerRunRequest::new("scheduler-specialists");
    request.completed_phase_ids = vec![
        "import_perception".to_string(),
        "plan_scribe_packets".to_string(),
        "wake_scribes".to_string(),
        "historian_questions".to_string(),
        "hourly_answers".to_string(),
        "for_you_editor".to_string(),
    ];
    request.available_artifacts = vec!["daily_editorial_artifacts".to_string()];

    let schedule = build_scheduler_run_plan(&executable, request);

    assert_eq!(schedule.ready_phase_ids, vec!["specialists"]);
    let specialists = schedule
        .phases
        .iter()
        .find(|phase| phase.id == "specialists")
        .expect("specialists phase");
    assert!(specialists
        .jobs
        .iter()
        .all(|job| job.status == "conditional"));
    assert_eq!(ready_jobs(&schedule).len(), 0);
}

#[test]
fn orchestrator_validation_rejects_invalid_route_addresses() {
    let mut config = shipped_orchestrator_config();
    config
        .routing
        .routes
        .push(onecontext_context_engine::orchestrator::RouteConfig {
            from_role: "memory.wiki.invalid".to_string(),
            to: vec!["mailbox://wiki-company".to_string()],
            cc: vec!["role://memory.wiki.historian".to_string()],
        });

    let report = validate_wiki_company_orchestrator(&config);

    assert!(report
        .issues
        .iter()
        .any(|issue| issue.contains("invalid recipient address")
            && issue.contains("mailbox://wiki-company")));
}

fn shipped_executable_plan() -> onecontext_context_engine::orchestration::ExecutableOrchestrationPlan
{
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let pack_report = validate_wiki_company_pack(&paths, &pack);
    assert!(
        pack_report.is_valid(),
        "pack validation issues: {:#?}",
        pack_report.issues
    );
    let orchestrator =
        load_wiki_company_orchestrator_config(&paths).expect("load shipped orchestrator");
    let orchestrator_report = validate_wiki_company_orchestrator(&orchestrator);
    assert!(
        orchestrator_report.is_valid(),
        "orchestrator validation issues: {:#?}",
        orchestrator_report.issues
    );
    build_executable_orchestration_plan(&pack, &orchestrator).expect("build executable plan")
}

fn shipped_orchestrator_config(
) -> onecontext_context_engine::orchestrator::WikiCompanyOrchestratorConfig {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    load_wiki_company_orchestrator_config(&paths).expect("load shipped orchestrator")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives under repo/crates/onecontext-context-engine")
        .to_path_buf()
}
