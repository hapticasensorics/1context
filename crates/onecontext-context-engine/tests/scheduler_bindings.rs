use onecontext_context_engine::orchestration::build_executable_orchestration_plan;
use onecontext_context_engine::orchestrator::load_wiki_company_orchestrator_config;
use onecontext_context_engine::pack::load_wiki_company_pack;
use onecontext_context_engine::scheduler_bindings::{
    resolve_binding, BindingResolutionContext, BindingResolutionStatus, BindingValue,
    DayBindingContext, HourBindingContext, PacketBindingContext,
};
use onecontext_context_engine::ContextEnginePaths;
use std::path::PathBuf;

#[test]
fn shipped_phase_bindings_are_resolved_or_marked_runtime_required() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let orchestrator =
        load_wiki_company_orchestrator_config(&paths).expect("load shipped orchestrator");
    let plan =
        build_executable_orchestration_plan(&pack, &orchestrator).expect("build executable plan");
    let mut context = BindingResolutionContext::from_paths(&paths, "binding-run");
    context.latest_refreshed_day = Some("2026-06-07".to_string());
    context.packet = Some(PacketBindingContext {
        id: "packet-2026-06-07-09".to_string(),
        date: "2026-06-07".to_string(),
        hour: "09".to_string(),
        path: "context-engine/packets/packet-2026-06-07-09.json".to_string(),
        content_sha256: "abc123".to_string(),
        hour_has_multiple_scribe_artifacts: Some(true),
    });
    context.hour = Some(HourBindingContext {
        id: "2026-06-07-09".to_string(),
        date: "2026-06-07".to_string(),
        hour: "09".to_string(),
    });
    context.day = Some(DayBindingContext {
        date: "2026-06-07".to_string(),
    });

    let mut resolved = 0;
    let mut runtime_required = 0;
    for job in plan.phases.iter().flat_map(|phase| phase.jobs.iter()) {
        for expression in job.bindings.values() {
            let resolution = resolve_binding(expression, &context);
            match resolution.status {
                BindingResolutionStatus::Resolved => resolved += 1,
                BindingResolutionStatus::RuntimeRequired => runtime_required += 1,
                BindingResolutionStatus::Unsupported => {
                    panic!("unsupported binding expression {expression}: {resolution:#?}");
                }
            }
        }
    }

    assert!(resolved > 20);
    assert!(runtime_required >= 1);
}

#[test]
fn resolver_handles_static_wiki_handles_and_runtime_packet_values() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let context = BindingResolutionContext::from_paths(&paths, "binding-run");

    let talk = resolve_binding("wiki.page('for-you').talk", &context);
    assert_eq!(talk.status, BindingResolutionStatus::Resolved);
    match talk.value {
        Some(BindingValue::Scalar(path)) => {
            assert!(path.ends_with("user-wiki/source/families/for-you/for-you/talk/for-you.talk"));
        }
        other => panic!("expected scalar talk path, got {other:#?}"),
    }

    let adjacent = resolve_binding(
        "wiki.page('your-context').talk,wiki.page('topics').talk",
        &context,
    );
    assert_eq!(adjacent.status, BindingResolutionStatus::Resolved);
    match adjacent.value {
        Some(BindingValue::List(paths)) => assert_eq!(paths.len(), 2),
        other => panic!("expected list of talk paths, got {other:#?}"),
    }

    let packet_date = resolve_binding("packet.date", &context);
    assert_eq!(packet_date.status, BindingResolutionStatus::RuntimeRequired);
    let artifact = resolve_binding("artifact.scribe(packet.id)", &context);
    assert_eq!(artifact.status, BindingResolutionStatus::RuntimeRequired);
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives under repo/crates/onecontext-context-engine")
        .to_path_buf()
}
