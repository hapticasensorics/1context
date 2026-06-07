use onecontext_context_engine::{
    append_wiki_company_mail_receipt, build_wiki_company_plan, read_wiki_company_mail_receipts,
    ContextEnginePaths, WikiCompanyRunRequest,
};
use std::fs;

#[test]
fn wiki_company_plan_is_recorded_in_mail_without_file_runs() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("1Context");
    let paths = ContextEnginePaths::new(&root);
    paths.ensure_release_dirs().unwrap();
    fs::write(
        paths.pack_agents.join("for-you-curator.toml"),
        "id = 'agent'\n",
    )
    .unwrap();
    fs::write(
        paths.pack_jobs.join("memory.wiki.for_you_curator.toml"),
        "id = 'job'\n",
    )
    .unwrap();
    fs::write(
        paths.pack_prompts.join("for-you-curator.md"),
        "# For You Curator\n",
    )
    .unwrap();
    fs::write(
        paths.pack_harnesses.join("codex-app-server.toml"),
        "id = 'harness'\n",
    )
    .unwrap();
    fs::write(
        paths.pack_lived_experiences.join("for-you-curator.md"),
        "# For You Curator Experience\n",
    )
    .unwrap();
    fs::write(
        &paths.orchestrator_file,
        "id = 'wiki-company-orchestrator-v1'\n",
    )
    .unwrap();
    fs::write(
        &paths.orchestrator_phases_file,
        "[[phases]]\nid = 'wake_scribes'\n",
    )
    .unwrap();
    fs::write(
        &paths.packet_policy_file,
        "source_context_fraction = 0.62\n",
    )
    .unwrap();
    fs::write(
        &paths.routing_file,
        "default_thread = 'mail://wiki-company'\n",
    )
    .unwrap();
    fs::write(&paths.receipts_file, "[required]\nfinal_message = true\n").unwrap();

    let mut request = WikiCompanyRunRequest::new("release smoke/run");
    request.max_concurrent_agents = 5;

    let plan = build_wiki_company_plan(&paths, request);
    assert_eq!(plan.runner, "context_engine.wiki_company");
    assert_eq!(plan.company_pack.id, "wiki-company-v1");
    assert!(plan
        .company_pack
        .root
        .ends_with("context-engine/packs/wiki-company-v1"));
    assert_eq!(plan.company_pack.agent_count, 1);
    assert_eq!(plan.company_pack.job_count, 1);
    assert_eq!(plan.company_pack.prompt_count, 1);
    assert_eq!(plan.company_pack.harness_count, 1);
    assert_eq!(plan.company_pack.lived_experience_count, 1);
    assert_eq!(plan.company_orchestrator.id, "wiki-company-orchestrator-v1");
    assert_eq!(plan.company_orchestrator.pack_id, "wiki-company-v1");
    assert_eq!(plan.company_orchestrator.active_harness, "codex-app-server");
    assert_eq!(plan.company_orchestrator.config_file_count, 5);
    assert!(plan
        .company_orchestrator
        .root
        .ends_with("context-engine/orchestrators/wiki-company-orchestrator-v1"));
    assert!(plan
        .company_pack
        .linking_file
        .ends_with("context-engine/packs/wiki-company-v1/linking.toml"));
    assert!(plan
        .company_pack
        .native_memory_file
        .ends_with("context-engine/packs/wiki-company-v1/native-memory.toml"));
    assert_eq!(
        plan.company_pack.prompt_policy,
        "preserve original donor prompts as much as possible"
    );
    assert_eq!(
        plan.release_boundary.memory_core_release_status,
        "not_on_release_path"
    );
    assert_eq!(
        plan.release_boundary.execution_history,
        "mail_first_until_postgres_timescale_run_history"
    );
    assert!(plan
        .mail_thread
        .ends_with("context-engine/mail/threads/wiki-company.jsonl"));
    assert!(plan.phases.iter().any(|phase| phase.id == "wake_scribes"));
    assert!(
        plan.phases
            .iter()
            .find(|phase| phase.id == "wake_scribes")
            .unwrap()
            .reads_raw_history
    );

    let path = append_wiki_company_mail_receipt(&plan).unwrap();
    assert!(path.ends_with("context-engine/mail/threads/wiki-company.jsonl"));
    assert!(!root.join("context-engine/runs").exists());

    let receipts = read_wiki_company_mail_receipts(path).unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].thread_id, "mail://wiki-company");
    assert_eq!(receipts[0].operation_id, "release-smoke-run");
    assert_eq!(receipts[0].plan.run_id, "release-smoke-run");
    assert_eq!(receipts[0].plan.company_pack.id, "wiki-company-v1");
    assert_eq!(
        receipts[0].plan.company_orchestrator.id,
        "wiki-company-orchestrator-v1"
    );
    assert_eq!(receipts[0].plan.request.max_concurrent_agents, 5);
}
