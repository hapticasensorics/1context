use onecontext_context_engine::orchestrator::{
    load_wiki_company_orchestrator_config, validate_wiki_company_orchestrator,
};
use onecontext_context_engine::pack::{load_wiki_company_pack, validate_wiki_company_pack};
use onecontext_context_engine::ContextEnginePaths;
use std::path::PathBuf;

#[test]
fn shipped_wiki_company_pack_and_orchestrator_validate() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));

    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let pack_report = validate_wiki_company_pack(&paths, &pack);
    assert!(
        pack_report.is_valid(),
        "pack validation issues: {:#?}",
        pack_report.issues
    );
    assert_eq!(pack_report.pack_id, "wiki-company-v1");
    assert_eq!(pack_report.provider_count, 1);
    assert_eq!(pack_report.harness_count, 1);
    assert_eq!(pack_report.agent_count, 15);
    assert_eq!(pack_report.job_count, 16);
    assert!(pack_report.prompt_reference_count > 40);

    let orchestrator = load_wiki_company_orchestrator_config(&paths)
        .expect("load shipped wiki-company orchestrator");
    let orchestrator_report = validate_wiki_company_orchestrator(&orchestrator);
    assert!(
        orchestrator_report.is_valid(),
        "orchestrator validation issues: {:#?}",
        orchestrator_report.issues
    );
    assert_eq!(
        orchestrator_report.orchestrator_id,
        "wiki-company-orchestrator-v1"
    );
    assert_eq!(orchestrator_report.phase_count, 7);
    assert_eq!(orchestrator_report.route_count, 9);
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives under repo/crates/onecontext-context-engine")
        .to_path_buf()
}
