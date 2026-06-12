use onecontext_context_engine::artifacts::ArtifactHandle;
use onecontext_context_engine::conditions::{
    evaluate_condition, ConditionContext, ConditionStatus,
};
use onecontext_context_engine::orchestration::build_executable_orchestration_plan;
use onecontext_context_engine::orchestrator::load_wiki_company_orchestrator_config;
use onecontext_context_engine::pack::load_wiki_company_pack;
use onecontext_context_engine::ContextEnginePaths;
use std::collections::BTreeSet;
use std::path::PathBuf;

#[test]
fn shipped_wiki_company_when_expressions_have_runtime_evaluators() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let orchestrator =
        load_wiki_company_orchestrator_config(&paths).expect("load shipped orchestrator");
    let plan =
        build_executable_orchestration_plan(&pack, &orchestrator).expect("build executable plan");
    let expressions = plan
        .phases
        .iter()
        .flat_map(|phase| phase.jobs.iter())
        .filter_map(|job| job.when.as_deref())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        expressions,
        BTreeSet::from([
            "daily_editorial_artifacts.ready",
            "for_you_proposals.pending",
            "hourly_answer_questions.pending",
            "packet.hour_has_multiple_scribe_artifacts",
            "recent_page_or_concept_changes",
            "your_context_proposals.pending",
        ])
    );

    let context = ConditionContext::with_known_artifacts([artifact("daily_editorial_artifacts")])
        .with_fact("for_you_proposals.pending", true)
        .with_fact("hourly_answer_questions.pending", true)
        .with_fact("packet.hour_has_multiple_scribe_artifacts", true)
        .with_fact("recent_page_or_concept_changes", true)
        .with_fact("your_context_proposals.pending", false);

    for expression in expressions {
        let evaluation = evaluate_condition(Some(expression), &context);
        assert_ne!(
            evaluation.status,
            ConditionStatus::Unknown,
            "condition {expression} should be supported with supplied runtime facts: {evaluation:#?}"
        );
    }
}

fn artifact(kind: &str) -> ArtifactHandle {
    ArtifactHandle {
        artifact_id: format!("{kind}-1"),
        kind: kind.to_string(),
        run_id: "run".to_string(),
        key: "key".to_string(),
        path: format!("/tmp/{kind}.json"),
        content_sha256: "sha256".to_string(),
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
