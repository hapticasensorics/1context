use onecontext_context_engine::harness_executor::{
    build_harness_turn_request, evaluate_harness_turn_completion, HarnessTurnReceipts,
};
use onecontext_context_engine::pack::{load_wiki_company_pack, validate_wiki_company_pack};
use onecontext_context_engine::ContextEnginePaths;
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
    assert!(request.agent.native_multi_agent_v2);
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

    assert_eq!(request.mail_context.thread_id, "mail://wiki-company");
    assert_eq!(request.talk_report.delivery_mode, "mail");
    assert_eq!(request.talk_report.thread_id, "job.run_id");
    assert_eq!(
        request.talk_report.from,
        format!("agent://{}", request.unit_id)
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
        .ends_with("/final-message.md"));
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
    assert!(!request.agent.native_multi_agent_v2);
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
fn harness_request_rejects_unknown_jobs() {
    let paths = ContextEnginePaths::new(repo_root().join("runtime/1Context"));
    let pack = load_wiki_company_pack(&paths).expect("load shipped wiki-company pack");
    let error = build_harness_turn_request(&paths, &pack, "run", "missing.job")
        .expect_err("unknown job should fail before launching a harness turn");
    assert!(error.contains("unknown job"));
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
        mail_receipt: Some("agent-mail:delivery=receipt-456".to_string()),
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
