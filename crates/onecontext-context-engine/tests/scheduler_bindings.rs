use onecontext_context_engine::scheduler_bindings::{
    resolve_binding, BindingResolutionContext, BindingResolutionStatus, BindingValue,
};
use onecontext_context_engine::ContextEnginePaths;
use std::path::PathBuf;

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
