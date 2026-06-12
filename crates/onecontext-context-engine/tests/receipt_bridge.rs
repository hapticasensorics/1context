use onecontext_context_engine::receipt_bridge::{
    agent_mail_receipt_for_job, append_scheduler_receipt, apply_appended_receipt_to_state,
    audit_receipt_path, receipt_for_job, AgentMailReceiptState, ReceiptAuthority,
    SchedulerReceiptState,
};
use onecontext_context_engine::run_state::WikiCompanyRunState;
use onecontext_context_engine::ContextEnginePaths;
use serde_json::json;

#[test]
fn audit_mirror_receipts_do_not_advance_scheduler_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");
    let mut state = WikiCompanyRunState::new("receipt-run");
    let receipt = receipt_for_job(
        "receipt-run",
        "unit-a",
        "wake_scribes",
        "memory.hourly.scribe",
        ReceiptAuthority::AuditMirror,
        SchedulerReceiptState::AgentMail(AgentMailReceiptState::delivered(
            "mail://wiki-company",
            "mail-receipt-a",
            "hosted-agent-mail",
        )),
        json!({ "note": "mirror only" }),
    );

    let append = append_scheduler_receipt(&paths, &receipt).expect("append receipt");
    assert!(!append.authoritative);
    assert!(!append.completes_job);
    assert!(audit_receipt_path(&paths, "receipt-run").is_file());

    apply_appended_receipt_to_state(&mut state, &receipt, &append);
    assert!(state.job_receipts.is_empty());
}

#[test]
fn authoritative_agent_mail_receipts_advance_job_state() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");
    let mut state = WikiCompanyRunState::new("receipt-run");
    let mut receipt = agent_mail_receipt_for_job(
        "receipt-run",
        "unit-a",
        "wake_scribes",
        "memory.hourly.scribe",
        ReceiptAuthority::AgentMail,
        AgentMailReceiptState::delivered(
            "mail://wiki-company",
            "mail-receipt-a",
            "hosted-agent-mail",
        ),
        json!({ "note": "real mail delivery" }),
    );
    receipt.path = Some("mail://wiki-company/mail-receipt-a".to_string());
    receipt.external_id = Some("mail-receipt-a".to_string());

    let append = append_scheduler_receipt(&paths, &receipt).expect("append receipt");
    assert!(append.authoritative);
    assert!(append.completes_job);

    apply_appended_receipt_to_state(&mut state, &receipt, &append);
    assert_eq!(state.job_receipts.len(), 1);
    assert_eq!(state.job_receipts[0].channel, "agent_mail");
    assert_eq!(state.job_receipts[0].authority, "agent_mail");
    assert!(state.job_receipts[0].authoritative);
    assert_eq!(
        state.job_receipts[0].receipt_paths,
        vec!["mail://wiki-company/mail-receipt-a"]
    );
    assert_eq!(state.job_receipts[0].audit_paths, vec![append.audit_path]);
    assert_eq!(
        state.job_receipts[0].external_id.as_deref(),
        Some("mail-receipt-a")
    );
}

#[test]
fn external_pending_agent_mail_receipts_remain_pending() {
    let receipt = agent_mail_receipt_for_job(
        "receipt-run",
        "unit-a",
        "wake_scribes",
        "memory.hourly.scribe",
        ReceiptAuthority::ExternalPending,
        AgentMailReceiptState::pending_external(
            "mail://wiki-company",
            "list://wiki-company",
            "hosted agent_mail.write unavailable",
        ),
        json!({}),
    );

    assert!(!receipt.has_authoritative_source());
    assert!(!receipt.completes_job());
}
