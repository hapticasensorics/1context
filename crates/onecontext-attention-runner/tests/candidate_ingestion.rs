use std::path::PathBuf;

use onecontext_attention_runner::{candidates::build_candidates, fixture::AttentionFixture};

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../docs/assets/attention-capture-mockup/attention-debug-20260524-215739/attention-dashboard-session.json",
    )
}

#[test]
fn uses_snapshot_index_as_event_base_time() {
    let fixture = AttentionFixture::load(&fixture_path()).expect("load attention fixture");

    // First raw event is at 04:57:40.035Z. This should be relative to the
    // snapshot-index base at 04:57:39Z, not pinned to the first event.
    assert_eq!(fixture.events.first().expect("first event").t_ms, 1_035);
}

#[test]
fn emits_dashboard_aligned_candidate_frames() {
    let fixture = AttentionFixture::load(&fixture_path()).expect("load attention fixture");
    let candidates = build_candidates(&fixture).expect("build candidates");

    assert_eq!(candidates.len(), 120);

    let first = candidates.first().expect("first candidate");
    assert_eq!(first.id, "candidate-2fps:frame-001");
    assert_eq!(first.frame_id, "2fps:frame-001");
    assert_eq!(first.t_ms, 0);
    assert_eq!(first.image_ref, "frames-2fps/frame-001.jpg");

    let last = candidates.last().expect("last candidate");
    assert_eq!(last.id, "candidate-2fps:frame-120");
    assert_eq!(last.frame_id, "2fps:frame-120");
    assert_eq!(last.t_ms, 59_500);
    assert_eq!(last.image_ref, "frames-2fps/frame-120.jpg");
}

#[test]
fn preserves_event_metadata_and_action_outcome_join() {
    let fixture = AttentionFixture::load(&fixture_path()).expect("load attention fixture");

    let codex_event = fixture
        .events
        .iter()
        .find(|event| event.app_name() == Some("Codex") && event.privacy_class().is_some())
        .expect("codex metadata event");
    assert_eq!(codex_event.bundle_id(), Some("com.openai.codex"));
    assert_eq!(codex_event.window_title(), Some("Codex"));
    assert_eq!(codex_event.durability(), Some("lossless"));
    assert_eq!(codex_event.privacy_class(), Some("accessibility_semantic"));
    assert_eq!(codex_event.source_clock(), Some("accessibility_api"));
    assert!(codex_event.source_line > 0);

    let candidates = build_candidates(&fixture).expect("build candidates");
    let post_shortcut_candidate = candidates
        .iter()
        .find(|candidate| candidate.t_ms == 41_000)
        .expect("41s candidate");
    assert!(post_shortcut_candidate
        .nearby_events
        .iter()
        .any(|event| event.event_type == "capture.ux.shortcut.v1"));
}
