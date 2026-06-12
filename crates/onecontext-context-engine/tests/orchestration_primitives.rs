use onecontext_context_engine::orchestration::{
    build_executable_orchestration_plan, planner_policy_from_config,
};
use onecontext_context_engine::orchestrator::{
    load_wiki_company_orchestrator_config, route_for_role, validate_wiki_company_orchestrator,
};
use onecontext_context_engine::pack::{load_wiki_company_pack, validate_wiki_company_pack};
use onecontext_context_engine::ContextEnginePaths;
use std::path::PathBuf;

#[test]
fn shipped_orchestrator_declares_executable_phase_jobs_and_receipts() {
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

    let plan =
        build_executable_orchestration_plan(&pack, &orchestrator).expect("build executable plan");
    assert_eq!(plan.phase_count, 9);
    assert!(plan.job_count >= 10);
    assert!(plan.receipt_policy.require_adapter_events);
    assert!(plan.receipt_policy.require_mail_delivery);
    assert_eq!(plan.receipt_policy.talk_delivery_mode, "mail");
    assert_eq!(
        plan.packet_policy.raw_history_roles,
        vec!["memory.hourly.scribe", "memory.hourly.block_scribe"]
    );
    assert!(plan.packet_policy.cache_enabled);

    let wake_scribes = plan
        .phases
        .iter()
        .find(|phase| phase.id == "wake_scribes")
        .expect("wake_scribes phase");
    assert_eq!(wake_scribes.depends_on, vec!["plan_scribe_packets"]);
    assert_eq!(wake_scribes.strategy, "fanout_selected_packets");
    let scribe = wake_scribes
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.scribe")
        .expect("scribe phase job");
    assert_eq!(scribe.fanout, "selected_packets");
    assert_eq!(scribe.bindings["date"], "packet.date");
    assert_eq!(scribe.bindings["source_packet_path"], "packet.path");
    assert_eq!(scribe.route.to, vec!["role://memory.wiki.historian"]);

    let historian = plan
        .phases
        .iter()
        .find(|phase| phase.id == "historian_questions")
        .expect("historian question phase")
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.wiki.historian")
        .expect("historian question job");
    assert_eq!(historian.fanout, "refreshed_days");
    assert_eq!(
        historian.bindings["scribe_artifacts"],
        "artifacts.scribe_for_day(day.date)"
    );
    assert_eq!(historian.route.to, vec!["role://memory.hourly.answerer"]);

    let answerer = plan
        .phases
        .iter()
        .find(|phase| phase.id == "hourly_answers")
        .expect("hourly answers phase")
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.hourly.answerer")
        .expect("hourly answerer job");
    assert_eq!(answerer.fanout, "historian_questions");
    assert_eq!(answerer.required_mail, vec!["job:memory.wiki.historian"]);
    assert_eq!(answerer.route.to, vec!["role://memory.wiki.for_you_editor"]);

    let editor = plan
        .phases
        .iter()
        .find(|phase| phase.id == "for_you_editor")
        .expect("editor phase")
        .jobs
        .iter()
        .find(|job| job.job_id == "memory.wiki.for_you_editor")
        .expect("for-you editor job");
    assert_eq!(editor.fanout, "refreshed_days");
    assert_eq!(
        editor.bindings["scribe_artifacts"],
        "artifacts.scribe_for_day(day.date)"
    );
    assert!(editor
        .required_artifacts
        .contains(&"hourly_answer_replies".to_string()));
}

#[test]
fn route_lookup_uses_orchestrator_routing_toml() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let orchestrator =
        load_wiki_company_orchestrator_config(&paths).expect("load shipped orchestrator");

    let editor_route = route_for_role(&orchestrator.routing, "memory.wiki.for_you_editor");
    assert_eq!(editor_route.thread_id, "mail://wiki-company");
    assert_eq!(editor_route.to, vec!["role://memory.wiki.for_you_curator"]);
    assert_eq!(
        editor_route.cc,
        vec!["role://memory.wiki.librarian", "mailbox://page/for-you"]
    );

    let historian_route = route_for_role(&orchestrator.routing, "memory.wiki.historian");
    assert_eq!(historian_route.to, vec!["role://memory.hourly.answerer"]);

    let answerer_route = route_for_role(&orchestrator.routing, "memory.hourly.answerer");
    assert_eq!(answerer_route.to, vec!["role://memory.wiki.for_you_editor"]);

    let biographer_route = route_for_role(&orchestrator.routing, "memory.wiki.biographer");
    assert_eq!(
        biographer_route.to,
        vec!["role://memory.wiki.for_you_curator"]
    );

    let fallback = route_for_role(&orchestrator.routing, "memory.wiki.unknown");
    assert_eq!(fallback.to, vec!["list://wiki-company"]);
    assert_eq!(fallback.cc, vec!["list://wiki-company"]);
}

#[test]
fn packet_planner_policy_is_derived_from_orchestrator_toml() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let orchestrator =
        load_wiki_company_orchestrator_config(&paths).expect("load shipped orchestrator");

    let policy = planner_policy_from_config(&orchestrator, 3);
    assert_eq!(policy.window_days, 3);
    assert_eq!(policy.usable_context_tokens, 258_000);
    assert_eq!(policy.context_fraction, 0.62);
    assert_eq!(policy.target_packet_tokens_override, Some(160_000));
    assert_eq!(policy.target_packet_tokens(), 160_000);
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives under repo/crates/onecontext-context-engine")
        .to_path_buf()
}
