use onecontext_context_engine::pack::{load_wiki_company_pack, validate_wiki_company_pack};
use onecontext_context_engine::wiki_company::build_wiki_company_dry_run;
use onecontext_context_engine::{ContextEnginePaths, WikiCompanyRunMode, WikiCompanyRunRequest};
use std::path::PathBuf;
use std::sync::Mutex;

static ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn dry_run_lists_agents_routes_packet_policy_and_publish_intent() {
    let _lock = ENV_LOCK.lock().unwrap();
    std::env::set_var("ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK", "1");

    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let report = validate_wiki_company_pack(&paths, &pack);
    assert!(
        report.is_valid(),
        "pack validation issues: {:#?}",
        report.issues
    );

    let mut request = WikiCompanyRunRequest::new("dry-run-test");
    request.trigger = "menu.update_wiki".to_string();
    request.mode = WikiCompanyRunMode::DryRun;
    request.source_window_days = 3;

    let dry_run =
        build_wiki_company_dry_run(&paths, &pack, &request).expect("build wiki-company dry run");

    assert!(["ok", "error", "unavailable"].contains(&dry_run.source_metadata.status.as_str()));
    assert_eq!(dry_run.source_metadata.provider, "onecontext-memoryd");
    assert!(dry_run.source_metadata.empty_fallback_allowed);
    assert!(["ok", "error", "not_ready", "unavailable"]
        .contains(&dry_run.source_metadata.storage_gate_status.as_str()));
    assert!(dry_run.source_metadata.note.contains("storage readiness"));
    assert_eq!(dry_run.packet_plan.usable_context_tokens, 258_400);
    assert_eq!(dry_run.packet_plan.context_fraction, 0.62);
    assert_eq!(dry_run.packet_plan.target_packet_tokens, 160_208);
    assert_eq!(dry_run.harness_previews.len(), 16);
    assert!(dry_run.route_count >= 16);
    assert_eq!(dry_run.publish_intent.publisher, "onecontext-wiki-core");
    assert_eq!(
        dry_run.publish_intent.after,
        "all required harness receipts are complete"
    );

    let scribe = dry_run
        .harness_previews
        .iter()
        .find(|preview| preview.job_id == "memory.hourly.scribe")
        .expect("scribe preview");
    assert_eq!(scribe.agent_id, "hourly-scribe");
    assert_eq!(scribe.harness_id, "codex-app-server");
    assert_eq!(scribe.transport, "codex-app-server");
    assert_eq!(scribe.to, vec!["role://memory.wiki.for_you_editor"]);
    assert!(scribe.requires_final_message);
    assert!(scribe.requires_talk_append);
    assert!(scribe.requires_mail_delivery);

    let curator = dry_run
        .harness_previews
        .iter()
        .find(|preview| preview.job_id == "memory.wiki.for_you_curator")
        .expect("for-you curator preview");
    assert_eq!(curator.agent_id, "for-you-curator");
    assert_eq!(curator.reasoning_effort, "xhigh");
    assert_eq!(curator.to, vec!["mailbox://page/for-you"]);
    assert!(curator.prompt_part_count >= 7);

    std::env::remove_var("ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK");
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|path| path.parent())
        .expect("crate lives under repo/crates/onecontext-context-engine")
        .to_path_buf()
}
