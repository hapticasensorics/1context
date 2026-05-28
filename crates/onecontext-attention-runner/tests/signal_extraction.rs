use std::path::PathBuf;

use onecontext_attention_runner::{
    fixture::AttentionFixture,
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
    CandidateState {
        id: "candidate".to_string(),
        frame_id: "frame".to_string(),
        t_ms: 1_000,
        image_ref: "frame.jpg".to_string(),
        nearby_events: events,
        signals: Vec::new(),
        attention_score: 0.0,
        memory_value_score: 0.0,
    }
}

fn event(event_type: &str, payload: Value) -> CaptureEvent {
    CaptureEvent {
        id: event_type.to_string(),
        event_type: event_type.to_string(),
        t_ms: 1_000,
        duration_ms: None,
        payload,
        source_ref: "events.jsonl".to_string(),
        source_line: 7,
    }
}

fn score_one(events: Vec<CaptureEvent>) -> CandidateState {
    score_candidates(&fixture(), vec![candidate(events)])
        .expect("score candidates")
        .pop()
        .expect("candidate")
}

#[test]
fn keyboard_burst_and_editing_shortcut_explain_hard_keep() {
    let scored = score_one(vec![
        event(
            "capture.ux.keyboard_activity.v1",
            json!({
                "payload": {
                    "keyboard_activity": {
                        "duration_ms": 3771,
                        "event_count": 54,
                        "modified_key_event_count": 0,
                        "auto_repeat_count": 0
                    },
                    "recent_target_process_id": 3257
                }
            }),
        ),
        event(
            "capture.ux.shortcut.v1",
            json!({
                "payload": {
                    "shortcut": {
                        "event_count": 1,
                        "action_categories": [{ "category": "editing", "event_count": 1 }],
                        "modifier_combinations": [{ "modifiers": ["command"], "event_count": 1 }]
                    },
                    "recent_target_process_id": 3257
                }
            }),
        ),
    ]);

    assert!(scored.attention_score >= 0.9);
    assert!(scored.memory_value_score >= 0.9);
    assert!(scored
        .signals
        .iter()
        .any(|signal| signal.kind == "keyboard_typing_burst_composition"
            && signal.explanation.contains("target_pid=3257")));
    assert!(scored.signals.iter().any(|signal| {
        signal.kind == "shortcut_command_editing" && signal.hard_keep == Some(true)
    }));
}

#[test]
fn pointer_click_scores_above_micro_drag_noise() {
    let scored = score_one(vec![
        event(
            "capture.ux.pointer.v1",
            json!({
                "payload": {
                    "pointer": {
                        "action": "drag",
                        "button": "left",
                        "click_count": 1,
                        "distance_points": 0.11,
                        "duration_ms": 67,
                        "event_count": 3
                    }
                }
            }),
        ),
        event(
            "capture.ux.pointer.v1",
            json!({
                "payload": {
                    "pointer": {
                        "action": "click",
                        "button": "left",
                        "click_count": 1,
                        "distance_points": 0,
                        "duration_ms": 66,
                        "event_count": 2
                    }
                }
            }),
        ),
    ]);

    let click = scored
        .signals
        .iter()
        .find(|signal| signal.kind == "pointer_click")
        .expect("click signal");
    let noise = scored
        .signals
        .iter()
        .find(|signal| signal.kind == "pointer_micro_drag_noise")
        .expect("noise signal");

    assert!(click.strength > 0.5);
    assert!(noise.strength < 0.2);
    assert_eq!(scored.attention_score, click.strength);
}

#[test]
fn scroll_distinguishes_fast_skim_from_pause_friendly_coverage() {
    let fast = score_one(vec![event(
        "capture.ux.scroll_burst.v1",
        json!({
            "payload": {
                "scroll": {
                    "duration_ms": 319,
                    "event_count": 22,
                    "momentum_event_count": 12,
                    "total_dx": -13,
                    "total_dy": -771,
                    "max_abs_dy": 55
                }
            }
        }),
    )]);
    let pause = score_one(vec![event(
        "capture.ux.scroll_burst.v1",
        json!({
            "payload": {
                "scroll": {
                    "duration_ms": 1507,
                    "event_count": 90,
                    "momentum_event_count": 59,
                    "total_dx": 110,
                    "total_dy": -982,
                    "max_abs_dy": 42
                }
            }
        }),
    )]);

    assert_eq!(fast.signals[0].kind, "scroll_fast_skim");
    assert_eq!(pause.signals[0].kind, "scroll_pause_friendly_coverage");
    assert!(pause.memory_value_score > fast.memory_value_score);
}

#[test]
fn zero_distance_scroll_is_noise() {
    let scored = score_one(vec![event(
        "capture.ux.scroll_burst.v1",
        json!({
            "payload": {
                "scroll": {
                    "duration_ms": 0,
                    "event_count": 1,
                    "momentum_event_count": 0,
                    "total_dx": 0,
                    "total_dy": 0,
                    "max_abs_dy": 0
                }
            }
        }),
    )]);

    assert_eq!(scored.signals[0].kind, "scroll_noise");
    assert!(scored.attention_score < 0.1);
    assert!(scored.memory_value_score < 0.01);
}

#[test]
fn useful_ax_selection_and_value_changes_become_hard_keep() {
    let selection = score_one(vec![event(
        "capture.ax_semantic.selected_text_changed.v1",
        json!({
            "payload": {
                "kind": "selected_text_changed",
                "activeApplication": { "appName": "Code" },
                "focusedElement": {
                    "role": "AXTextArea",
                    "frame": { "x": 10, "y": 20, "width": 300, "height": 40 },
                    "selection": {
                        "selectedTextCharacterCount": 12,
                        "range": { "location": 2, "length": 12 }
                    }
                }
            }
        }),
    )]);
    let copied_value = score_one(vec![event(
        "capture.ax_semantic.value_changed.v1",
        json!({
            "payload": {
                "kind": "value_changed",
                "activeApplication": { "appName": "Chrome" },
                "focusedElement": {
                    "role": "AXButton",
                    "elementDescription": "Response copied",
                    "frame": { "x": 1, "y": 2, "width": 32, "height": 32 },
                    "valueShape": { "characterCount": 0 }
                }
            }
        }),
    )]);

    assert_eq!(selection.signals[0].kind, "ax_selection_changed_useful");
    assert_eq!(selection.signals[0].hard_keep, Some(true));
    assert!(selection.signals[0].region.is_some());
    assert_eq!(copied_value.signals[0].kind, "ax_value_changed_useful");
    assert_eq!(copied_value.signals[0].hard_keep, Some(true));
}

#[test]
fn static_active_window_metadata_is_low_information() {
    let scored = score_one(vec![event(
        "capture.active_window_frame_metadata",
        json!({
            "payload": {
                "motionFeatures": {
                    "dirtyAreaRatio": 0,
                    "changedTileRatio": 0,
                    "dirtyRectCount": 0,
                    "estimatedDY": 0,
                    "meanPixelDiff": 0
                },
                "adaptiveDecision": {
                    "updateReason": "unchanged",
                    "shouldStoreKeyframe": true
                }
            }
        }),
    )]);

    assert_eq!(scored.signals[0].kind, "visual_static_low_information");
    assert!(scored.attention_score < 0.1);
    assert!(scored.signals[0]
        .explanation
        .contains("low-information visual penalty"));
}

#[test]
fn derived_top_band_visual_change_marks_window_transition() {
    let scored = score_one(vec![event(
        "attention.derived.visual_frame_change.v1",
        json!({
            "payload": {
                "from_frame": 66,
                "to_frame": 67,
                "full_diff_score": 0.24,
                "top_band_diff_score": 0.31,
                "reason": "top/window band changed between adjacent review frames"
            }
        }),
    )]);

    assert_eq!(scored.signals[0].kind, "visual_window_transition");
    assert_eq!(scored.signals[0].hard_keep, Some(true));
    assert!(scored.attention_score > 0.7);
}
