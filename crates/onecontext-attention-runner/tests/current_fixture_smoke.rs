use std::{env, fs, path::PathBuf, process::Command};

use serde_json::Value;

const SESSION_PATH: &str = "docs/assets/attention-capture-mockup/attention-debug-20260524-215739/attention-dashboard-session.json";

#[test]
fn current_fixture_runner_output_matches_dashboard_contract() {
    let output_path = temp_output_path("attention-runner-current-fixture.json");
    let _ = fs::remove_file(&output_path);

    let status = Command::new(env!("CARGO_BIN_EXE_onecontext-attention-runner"))
        .arg("--session")
        .arg(repo_path(SESSION_PATH))
        .arg("--out")
        .arg(&output_path)
        .status()
        .expect("run onecontext-attention-runner");

    assert!(status.success(), "runner exited with {status}");

    let output_text = fs::read_to_string(&output_path).expect("read runner output json");
    let output: Value = serde_json::from_str(&output_text).expect("parse runner output as json");

    assert_eq!(
        output.get("version").and_then(Value::as_str),
        Some("attention-ledger.v3")
    );

    let raw_buffer_audit = output
        .get("raw_buffer_audit")
        .and_then(Value::as_array)
        .expect("raw_buffer_audit array");
    assert!(
        (110..=130).contains(&raw_buffer_audit.len()),
        "expected roughly 120 raw buffer candidates for the 2fps fixture, got {}",
        raw_buffer_audit.len()
    );

    let saved_states = output
        .get("saved_states")
        .and_then(Value::as_array)
        .expect("saved_states array");
    assert!(!saved_states.is_empty(), "saved_states should not be empty");

    let algorithms = output
        .get("algorithms")
        .and_then(Value::as_array)
        .expect("algorithms array");
    assert!(
        algorithms.iter().any(has_algorithm_summary),
        "expected at least one algorithm summary with counts"
    );

    for saved_state in saved_states {
        let id = saved_state
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or("<missing id>");
        let raw_event_refs = saved_state
            .pointer("/proof_bundle/raw_event_refs")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{id} missing proof_bundle.raw_event_refs"));
        assert!(
            !raw_event_refs.is_empty(),
            "{id} should carry proof_bundle.raw_event_refs"
        );
        assert!(
            raw_event_refs.iter().all(|item| item.as_str().is_some()),
            "{id} raw event refs should be string refs"
        );

        let provenance_refs = saved_state
            .get("provenance_refs")
            .and_then(Value::as_array)
            .unwrap_or_else(|| panic!("{id} missing provenance_refs"));
        assert!(
            !provenance_refs.is_empty(),
            "{id} should carry provenance refs"
        );
        assert!(
            provenance_refs
                .iter()
                .any(|item| item.get("path").and_then(Value::as_str).is_some()),
            "{id} should include at least one raw path provenance ref"
        );
    }

    fs::remove_file(output_path).expect("remove temp runner output");
}

fn has_algorithm_summary(value: &Value) -> bool {
    value.get("id").and_then(Value::as_str).is_some()
        && value.get("summary").and_then(Value::as_str).is_some()
        && value
            .get("candidates_considered")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
        && value
            .get("saved_count")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            > 0
}

fn temp_output_path(file_name: &str) -> std::path::PathBuf {
    env::temp_dir().join(format!("{}-{file_name}", std::process::id()))
}

fn repo_path(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}
