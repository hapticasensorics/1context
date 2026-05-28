use std::path::PathBuf;

use onecontext_attention_runner::{
    fixture::AttentionFixture,
    fusion::fuse_attention,
    model::{
        CandidateFrameSet, CandidateState, CaptureEvent, DashboardFixtureConfig,
        DashboardInputsConfig, DashboardMediaConfig, DashboardSession, EventRef, FilterOutputRef,
    },
    signals::score_candidates,
};
use serde_json::{json, Value};

fn fixture() -> AttentionFixture {
    AttentionFixture {
        session_path: PathBuf::from("synthetic-session.json"),
        root: PathBuf::from("."),
        session: DashboardSession {
            session_id: "synthetic".to_string(),
            title: "synthetic".to_string(),
            created_at: "2026-05-25T00:00:00Z".to_string(),
            fixture: DashboardFixtureConfig {
                run_id: "run".to_string(),
                root: ".".to_string(),
                duration_ms: 10_000,
            },
            media: DashboardMediaConfig {
                video_ref: "video.mp4".to_string(),
                video_width: 100,
                video_height: 100,
                candidate_frame_sets: vec![CandidateFrameSet {
                    id: "2fps".to_string(),
                    root: "frames".to_string(),
                    fps: 2.0,
                    count: 1,
                    naming: "frame-{index:03}.jpg".to_string(),
                    frame_times_ms: None,
                }],
            },
            inputs: DashboardInputsConfig {
                candidate_index_ref: None,
                snapshots_root: None,
                event_refs: Vec::<EventRef>::new(),
            },
            filter_output: FilterOutputRef {
                path: "out.json".to_string(),
            },
        },
        events: Vec::new(),
    }
}

fn candidate(events: Vec<CaptureEvent>) -> CandidateState {
    candidate_at("candidate-001", "frame-001", 1_000, events)
}

fn candidate_at(id: &str, frame_id: &str, t_ms: u64, events: Vec<CaptureEvent>) -> CandidateState {
    CandidateState {
        id: id.to_string(),
        frame_id: frame_id.to_string(),
        t_ms,
        image_ref: "frame.jpg".to_string(),
        nearby_events: events,
        signals: Vec::new(),
        attention_score: 0.0,
        memory_value_score: 0.0,
    }
}

fn event(id: &str, event_type: &str, t_ms: u64, payload: Value) -> CaptureEvent {
    CaptureEvent {
        id: id.to_string(),
        event_type: event_type.to_string(),
        t_ms,
        duration_ms: None,
        payload,
        source_ref: "events.jsonl".to_string(),
        source_line: 7,
    }
}

fn run_one(events: Vec<CaptureEvent>) -> Value {
    let fixture = fixture();
    let scored = score_candidates(&fixture, vec![candidate(events)]).expect("score candidates");
    let output = fuse_attention(&fixture, scored).expect("fuse output");
    serde_json::to_value(output).expect("serialize output")
}

#[test]
fn ax_and_ux_transition_signals_are_canonical_transition() {
    let output = run_one(vec![
        event(
            "ax",
            "capture.ax_semantic.focused_window_changed.v1",
            980,
            json!({
                "payload": {
                    "kind": "focused_window_changed",
                    "activeApplication": {
                        "appName": "Google Chrome",
                        "bundleID": "com.google.Chrome"
                    },
                    "focusedWindow": { "title": "Composite Screenshot System" }
                }
            }),
        ),
        event(
            "ux",
            "capture.ux.focus_transition.v1",
            1_100,
            json!({
                "payload": {
                    "kind": "focus_transition",
                    "focus_transition": {
                        "confidence": "high",
                        "previous_process_id": 28730,
                        "current_process_id": 19780,
                        "trigger": "pointer"
                    }
                }
            }),
        ),
    ]);

    let saved = output
        .get("saved_states")
        .and_then(Value::as_array)
        .expect("saved states");
    assert_eq!(
        saved
            .first()
            .and_then(|state| state.get("decision"))
            .and_then(Value::as_str),
        Some("save_transition")
    );

    let classes = saved
        .first()
        .and_then(|state| state.pointer("/explanation/score_components/canonical_signal_classes"))
        .and_then(Value::as_array)
        .expect("canonical classes");
    assert!(classes.iter().any(|class| class == "transition"));
}

#[test]
fn ax_focus_truth_wins_over_nearer_sck_visual_receipt() {
    let output = run_one(vec![
        event(
            "sck",
            "capture.active_window_frame_metadata",
            1_000,
            json!({
                "payload": {
                    "target": {
                        "appName": "Code",
                        "bundleID": "com.microsoft.VSCode",
                        "title": "screencap-attention-algorithm-notes.md"
                    },
                    "motionFeatures": {
                        "dirtyAreaRatio": 0,
                        "changedTileRatio": 0,
                        "dirtyRectCount": 0,
                        "estimatedDY": 0,
                        "meanPixelDiff": 0
                    }
                }
            }),
        ),
        event(
            "ax",
            "capture.ax_semantic.focused_window_changed.v1",
            1_250,
            json!({
                "payload": {
                    "kind": "focused_window_changed",
                    "activeApplication": {
                        "appName": "Google Chrome",
                        "bundleID": "com.google.Chrome"
                    },
                    "focusedWindow": { "title": "Composite Screenshot System" }
                }
            }),
        ),
    ]);

    let saved = output
        .get("saved_states")
        .and_then(Value::as_array)
        .and_then(|states| states.first())
        .expect("saved state");
    assert_eq!(
        saved.get("app_name").and_then(Value::as_str),
        Some("Google Chrome")
    );
    assert_eq!(
        saved
            .pointer("/explanation/score_components/source_resolution/winning_source")
            .and_then(Value::as_str),
        Some("ax_window_changed")
    );

    let conflicts = output
        .get("source_conflicts")
        .and_then(Value::as_array)
        .expect("source conflicts");
    assert!(conflicts.iter().any(|conflict| {
        conflict.get("source_b").and_then(Value::as_str) == Some("sck_active_window")
            && conflict.get("severity").and_then(Value::as_str) == Some("info")
    }));
}

#[test]
fn shared_keyboard_episode_drops_unlabeled_duplicate_candidate() {
    let fixture = fixture();
    let keyboard_event = event(
        "keyboard-burst",
        "capture.ux.keyboard_activity.v1",
        1_000,
        json!({
            "payload": {
                "keyboard_activity": {
                    "duration_ms": 5_000,
                    "event_count": 60,
                    "modified_key_event_count": 0,
                    "auto_repeat_count": 0
                },
                "recent_target_process_id": 42
            }
        }),
    );
    let candidates = vec![
        candidate_at(
            "candidate-a",
            "frame-001",
            1_500,
            vec![keyboard_event.clone()],
        ),
        candidate_at("candidate-b", "frame-002", 4_400, vec![keyboard_event]),
    ];

    let scored = score_candidates(&fixture, candidates).expect("score candidates");
    let output = fuse_attention(&fixture, scored).expect("fuse output");
    let value = serde_json::to_value(output).expect("serialize output");
    let saved = value
        .get("saved_states")
        .and_then(Value::as_array)
        .expect("saved states");
    assert_eq!(
        saved.len(),
        1,
        "shared causal keyboard burst should not create duplicate receipts"
    );

    let raw_items = value
        .get("raw_buffer_audit")
        .and_then(Value::as_array)
        .expect("raw buffer");
    assert!(raw_items.iter().any(|item| {
        item.get("decision")
            .and_then(Value::as_str)
            .is_some_and(|decision| decision.contains("dropped_redundant_shared_event"))
    }));
}
