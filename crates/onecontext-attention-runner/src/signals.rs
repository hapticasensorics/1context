use anyhow::Result;
use serde_json::Value;

use crate::{
    fixture::AttentionFixture,
    model::{AttentionRegion, AttentionSignal, CandidateState, CaptureEvent, ProvenanceRef, Rect},
};

const ALGORITHM_ID: &str = "heuristic-event-signals.v1";

pub fn score_candidates(
    _fixture: &AttentionFixture,
    candidates: Vec<CandidateState>,
) -> Result<Vec<CandidateState>> {
    Ok(candidates
        .into_iter()
        .map(|mut candidate| {
            let mut attention_score: f32 = 0.0;
            let mut memory_value_score: f32 = 0.0;

            for event in &candidate.nearby_events {
                let Some(signal) = signal_for_event(event) else {
                    continue;
                };
                let strength = clamp_score(signal.strength);
                attention_score = attention_score.max(strength);
                if signal.hard_keep {
                    memory_value_score = memory_value_score.max(strength.max(0.68));
                } else {
                    memory_value_score =
                        memory_value_score.max(strength * signal.memory_value_multiplier);
                }
                candidate.signals.push(AttentionSignal {
                    algorithm: ALGORITHM_ID.to_string(),
                    kind: signal.kind,
                    strength,
                    hard_keep: signal.hard_keep.then_some(true),
                    region: signal.region,
                    explanation: signal.explanation,
                    provenance_refs: vec![ProvenanceRef {
                        id: Some(event.id.clone()),
                        kind: Some(event.event_type.clone()),
                        path: Some(format!("{}:{}", event.source_ref, event.source_line)),
                        t_ms: Some(event.t_ms),
                    }],
                });
            }

            candidate.signals.sort_by(|left, right| {
                right
                    .hard_keep
                    .unwrap_or(false)
                    .cmp(&left.hard_keep.unwrap_or(false))
                    .then_with(|| right.strength.total_cmp(&left.strength))
            });
            candidate.attention_score = attention_score;
            candidate.memory_value_score = memory_value_score;
            candidate
        })
        .collect())
}

#[derive(Debug, Clone)]
struct ExtractedSignal {
    kind: String,
    strength: f32,
    hard_keep: bool,
    memory_value_multiplier: f32,
    region: Option<AttentionRegion>,
    explanation: String,
}

fn signal_for_event(event: &CaptureEvent) -> Option<ExtractedSignal> {
    match event.event_type.as_str() {
        "capture.ux.keyboard_activity.v1" => Some(keyboard_signal(event)),
        "capture.ux.shortcut.v1" => Some(shortcut_signal(event)),
        "capture.ux.modifiers.v1" => Some(modifier_signal(event)),
        "capture.ux.pointer.v1" => Some(pointer_signal(event)),
        "capture.ux.scroll_burst.v1" => Some(scroll_signal(event)),
        "capture.ax_focused_context" => Some(ax_focused_context_signal(event)),
        event_type if event_type.starts_with("capture.ax_semantic.") => {
            Some(ax_semantic_signal(event))
        }
        "capture.active_window_frame_metadata" => Some(active_window_frame_signal(event)),
        "attention.derived.visual_frame_change.v1" => Some(visual_frame_change_signal(event)),
        event_type if event_type.contains("focus_transition") => {
            Some(focus_transition_signal(event))
        }
        _ => None,
    }
}

fn keyboard_signal(event: &CaptureEvent) -> ExtractedSignal {
    let keyboard = value_at(&event.payload, &["payload", "keyboard_activity"]);
    let duration_ms = u64_child(keyboard, "duration_ms")
        .or(event.duration_ms)
        .unwrap_or(0);
    let event_count = u64_child(keyboard, "event_count").unwrap_or(0);
    let modified_count = u64_child(keyboard, "modified_key_event_count").unwrap_or(0);
    let auto_repeat_count = u64_child(keyboard, "auto_repeat_count").unwrap_or(0);
    let target_pid = u64_at(&event.payload, &["payload", "recent_target_process_id"]);
    let modified_ratio = ratio(modified_count as f32, event_count.max(1) as f32);
    let repeat_ratio = ratio(auto_repeat_count as f32, event_count.max(1) as f32);
    let possible_composition = event_count >= 6 && duration_ms >= 400 && modified_ratio <= 0.25;
    let mostly_short_noise = event_count <= 2 && duration_ms <= 200;
    let strength = if mostly_short_noise {
        0.28 + ratio(duration_ms as f32, 500.0) * 0.08
    } else {
        0.34 + ratio(duration_ms as f32, 4_500.0) * 0.24
            + ratio(event_count as f32, 60.0) * 0.24
            + if possible_composition { 0.08 } else { 0.0 }
            + if modified_count > 0 && !possible_composition {
                0.04
            } else {
                0.0
            }
            - repeat_ratio * 0.12
    };
    let kind = if possible_composition {
        "keyboard_typing_burst_composition"
    } else if modified_count > 0 {
        "keyboard_modified_activity"
    } else if mostly_short_noise {
        "keyboard_single_key_activity"
    } else {
        "keyboard_typing_burst"
    };
    let composition_note = if possible_composition {
        "mostly-unmodified burst suggests possible composition"
    } else {
        "keyboard activity marks local user intent"
    };

    ExtractedSignal {
        kind: kind.to_string(),
        strength,
        hard_keep: false,
        memory_value_multiplier: 0.74,
        region: None,
        explanation: format!(
            "reason={kind} duration_ms={duration_ms} event_count={event_count} modified_events={modified_count} auto_repeat_events={auto_repeat_count} target_pid={}; {composition_note}.",
            optional_u64(target_pid)
        ),
    }
}

fn shortcut_signal(event: &CaptureEvent) -> ExtractedSignal {
    let shortcut = value_at(&event.payload, &["payload", "shortcut"]);
    let categories = shortcut
        .and_then(|value| value.get("action_categories"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.get("category").and_then(Value::as_str))
                .map(|value| value.to_ascii_lowercase())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let combinations = shortcut
        .and_then(|value| value.get("modifier_combinations"))
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| {
                    let modifiers = value
                        .get("modifiers")
                        .and_then(Value::as_array)?
                        .iter()
                        .filter_map(Value::as_str)
                        .collect::<Vec<_>>();
                    Some(modifiers.join("+"))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let event_count = u64_child(shortcut, "event_count").unwrap_or(1);
    let target_pid = u64_at(&event.payload, &["payload", "recent_target_process_id"]);
    let hard_keep = categories.iter().any(|category| {
        matches!(
            category.as_str(),
            "commit" | "copy" | "cut" | "edit" | "editing" | "paste" | "save" | "submit"
        )
    });
    let primary_category = categories
        .first()
        .cloned()
        .unwrap_or_else(|| "uncategorized".to_string());
    let strength = if hard_keep {
        0.93
    } else {
        0.82 + ratio(event_count as f32, 4.0) * 0.06
    };

    ExtractedSignal {
        kind: format!("shortcut_command_{primary_category}"),
        strength,
        hard_keep,
        memory_value_multiplier: 0.82,
        region: None,
        explanation: format!(
            "reason=shortcut_command category={} modifiers={} event_count={event_count} target_pid={}; command shortcut is an intentional operation{}.",
            list_or_none(&categories),
            list_or_none(&combinations),
            optional_u64(target_pid),
            if hard_keep {
                " and matches a likely commit/copy/editing outcome, so it is hard_keep"
            } else {
                ""
            }
        ),
    }
}

fn modifier_signal(event: &CaptureEvent) -> ExtractedSignal {
    let active = string_array_at(
        &event.payload,
        &["payload", "modifiers", "active_modifiers"],
    );
    let changed = string_array_at(
        &event.payload,
        &["payload", "modifiers", "changed_modifiers"],
    );
    let has_command = active
        .iter()
        .chain(changed.iter())
        .any(|modifier| modifier.eq_ignore_ascii_case("command"));
    let target_pid = u64_at(&event.payload, &["payload", "recent_target_process_id"]);
    let kind = if has_command {
        "modifier_command_intent"
    } else {
        "modifier_state_change"
    };

    ExtractedSignal {
        kind: kind.to_string(),
        strength: if has_command { 0.44 } else { 0.28 },
        hard_keep: false,
        memory_value_multiplier: 0.45,
        region: None,
        explanation: format!(
            "reason={kind} active_modifiers={} changed_modifiers={} target_pid={}; modifier-only events hint at command setup but are not enough to keep alone.",
            list_or_none(&active),
            list_or_none(&changed),
            optional_u64(target_pid)
        ),
    }
}

fn pointer_signal(event: &CaptureEvent) -> ExtractedSignal {
    let pointer = value_at(&event.payload, &["payload", "pointer"]);
    let action = str_child(pointer, "action")
        .unwrap_or("unknown")
        .to_ascii_lowercase();
    let button = str_child(pointer, "button").unwrap_or("unknown");
    let click_count = u64_child(pointer, "click_count").unwrap_or(0);
    let event_count = u64_child(pointer, "event_count").unwrap_or(0);
    let duration_ms = u64_child(pointer, "duration_ms")
        .or(event.duration_ms)
        .unwrap_or(0);
    let distance_points = f32_child(pointer, "distance_points").unwrap_or(0.0).abs();
    let target_pid = u64_at(&event.payload, &["payload", "recent_target_process_id"]);

    let (kind, strength, explanation_tail) = match action.as_str() {
        "click" => (
            if click_count >= 2 {
                "pointer_multi_click"
            } else {
                "pointer_click"
            },
            0.54 + ratio(click_count as f32, 3.0) * 0.12,
            "click is a discrete selection/activation signal",
        ),
        "drag" if distance_points >= 3.0 || duration_ms >= 150 => (
            "pointer_drag",
            0.50 + ratio(distance_points, 300.0) * 0.16 + ratio(duration_ms as f32, 1_500.0) * 0.08,
            "drag covers intentional spatial manipulation",
        ),
        "drag" => (
            "pointer_micro_drag_noise",
            0.11,
            "drag distance is tiny, so this is treated as pointer noise",
        ),
        "move" | "moved" | "hover" if distance_points >= 12.0 => (
            "pointer_hover_movement",
            0.22 + ratio(distance_points, 250.0) * 0.12,
            "movement without click may indicate hover or inspection",
        ),
        "move" | "moved" | "hover" => (
            "pointer_movement_noise",
            0.06,
            "low-distance movement is low-information pointer noise",
        ),
        "down" | "up" => (
            "pointer_press_edge",
            0.14,
            "button edge without completed click is weak on its own",
        ),
        _ => (
            "pointer_action_unknown",
            0.18,
            "unclassified pointer action is weak until paired with other evidence",
        ),
    };

    ExtractedSignal {
        kind: kind.to_string(),
        strength,
        hard_keep: false,
        memory_value_multiplier: if kind.contains("noise") { 0.18 } else { 0.55 },
        region: None,
        explanation: format!(
            "reason={kind} action={action} button={button} click_count={click_count} distance_points={distance_points:.1} duration_ms={duration_ms} event_count={event_count} target_pid={}; {explanation_tail}.",
            optional_u64(target_pid)
        ),
    }
}

fn scroll_signal(event: &CaptureEvent) -> ExtractedSignal {
    let scroll = value_at(&event.payload, &["payload", "scroll"]);
    let duration_ms = u64_child(scroll, "duration_ms")
        .or(event.duration_ms)
        .unwrap_or(0);
    let event_count = u64_child(scroll, "event_count").unwrap_or(0);
    let total_dy = f32_child(scroll, "total_dy").unwrap_or(0.0);
    let total_dx = f32_child(scroll, "total_dx").unwrap_or(0.0);
    let max_abs_dy = f32_child(scroll, "max_abs_dy").unwrap_or(0.0);
    let momentum_count = u64_child(scroll, "momentum_event_count").unwrap_or(0);
    let abs_dy = total_dy.abs();
    let abs_dx = total_dx.abs();
    let velocity = if duration_ms == 0 {
        abs_dy
    } else {
        abs_dy / duration_ms as f32
    };
    let tiny_scroll =
        abs_dy < 4.0 && abs_dx < 4.0 && max_abs_dy < 4.0 && event_count <= 2 && duration_ms <= 100;
    let fast_skim =
        (velocity >= 1.8 && duration_ms <= 700) || (abs_dy >= 700.0 && duration_ms <= 500);
    let pause_friendly = duration_ms >= 800 && velocity <= 1.2;
    let kind = if tiny_scroll {
        "scroll_noise"
    } else if fast_skim {
        "scroll_fast_skim"
    } else if pause_friendly {
        "scroll_pause_friendly_coverage"
    } else {
        "scroll_coverage"
    };
    let strength = if tiny_scroll {
        0.08
    } else {
        0.34 + ratio(abs_dy, 1_200.0) * 0.22
            + ratio(event_count as f32, 70.0) * 0.13
            + ratio(duration_ms as f32, 1_800.0) * 0.08
            + if momentum_count > 0 { 0.04 } else { 0.0 }
            + if pause_friendly { 0.10 } else { 0.0 }
            - if fast_skim { 0.12 } else { 0.0 }
    };
    let target_pid = u64_at(&event.payload, &["payload", "recent_target_process_id"]);

    ExtractedSignal {
        kind: kind.to_string(),
        strength,
        hard_keep: false,
        memory_value_multiplier: if tiny_scroll {
            0.05
        } else if fast_skim {
            0.35
        } else if pause_friendly {
            0.62
        } else {
            0.50
        },
        region: None,
        explanation: format!(
            "reason={kind} total_dy={total_dy:.1} total_dx={total_dx:.1} velocity_points_per_ms={velocity:.2} max_abs_dy={max_abs_dy:.1} momentum_events={momentum_count} duration_ms={duration_ms} event_count={event_count} target_pid={}; {}.",
            optional_u64(target_pid),
            if tiny_scroll {
                "tiny zero-distance scroll is treated as input noise"
            } else if fast_skim {
                "fast skim gets coverage credit but lower memory value"
            } else if pause_friendly {
                "slower scroll leaves a more reviewable visual receipt"
            } else {
                "scroll exposed nearby visual content"
            }
        ),
    }
}

fn ax_focused_context_signal(event: &CaptureEvent) -> ExtractedSignal {
    let status = str_at(&event.payload, &["payload", "status"]).unwrap_or("unknown");
    let app_name = str_at(&event.payload, &["payload", "activeApplication", "appName"])
        .unwrap_or("unknown_app");
    let window_title = str_at(&event.payload, &["payload", "focusedWindow", "title"]).unwrap_or("");
    let role = str_at(&event.payload, &["payload", "focusedElement", "role"])
        .or_else(|| str_at(&event.payload, &["payload", "focusedWindow", "role"]))
        .unwrap_or("unknown_role");
    let has_focused_element = value_at(&event.payload, &["payload", "focusedElement"]).is_some();
    let visible_region_count = u64_at(
        &event.payload,
        &["payload", "visibleContext", "capturedRegionCount"],
    )
    .unwrap_or(0);
    let value_chars = value_character_count(&event.payload);
    let text_context = matches!(role, "AXTextArea" | "AXTextField") || value_chars > 0;
    let kind = if text_context {
        "ax_focus_text_context"
    } else if has_focused_element {
        "ax_focus_element_context"
    } else {
        "ax_focus_window_context"
    };
    let strength = if text_context {
        0.50
    } else if has_focused_element {
        0.43
    } else {
        0.30
    } + if visible_region_count > 0 { 0.05 } else { 0.0 };

    ExtractedSignal {
        kind: kind.to_string(),
        strength,
        hard_keep: false,
        memory_value_multiplier: 0.58,
        region: region_at(
            &event.payload,
            &["payload", "focusedElement", "frame"],
            kind,
            "Focused accessibility element near this candidate.",
        ),
        explanation: format!(
            "reason={kind} status={status} app={app_name} role={role} value_chars={value_chars} visible_regions={visible_region_count} window_title=\"{}\"; focused accessibility context explains what the nearby visual frame likely contains.",
            compact(window_title, 80)
        ),
    }
}

fn ax_semantic_signal(event: &CaptureEvent) -> ExtractedSignal {
    let semantic_kind = str_at(&event.payload, &["payload", "kind"])
        .unwrap_or_else(|| event.event_type.rsplit('.').nth(1).unwrap_or("semantic"));
    let app_name = str_at(&event.payload, &["payload", "activeApplication", "appName"])
        .unwrap_or("unknown_app");
    let role = str_at(&event.payload, &["payload", "focusedElement", "role"])
        .or_else(|| str_at(&event.payload, &["payload", "focusedWindow", "role"]))
        .unwrap_or("unknown_role");
    let selection_chars = selection_character_count(&event.payload);
    let value_chars = value_character_count(&event.payload);
    let description = str_at(
        &event.payload,
        &["payload", "focusedElement", "elementDescription"],
    )
    .or_else(|| str_at(&event.payload, &["payload", "focusedElement", "title"]))
    .unwrap_or("");
    let outcome_phrase = contains_outcome_phrase(description);

    let (kind, strength, hard_keep, multiplier, tail) = if semantic_kind == "selected_text_changed"
    {
        let useful = selection_chars > 0;
        (
            if useful {
                "ax_selection_changed_useful"
            } else {
                "ax_selection_insertion_point"
            },
            if useful { 0.91 } else { 0.46 },
            useful,
            0.78,
            if useful {
                "selection changed with selected text, often preceding copy or review"
            } else {
                "selection event carried only an insertion point"
            },
        )
    } else if semantic_kind == "value_changed" {
        let useful = value_chars > 0 || outcome_phrase;
        (
            if useful {
                "ax_value_changed_useful"
            } else {
                "ax_value_changed_empty"
            },
            if useful { 0.82 } else { 0.52 },
            useful,
            0.74,
            if useful {
                "value change has text or outcome wording worth preserving"
            } else {
                "value changed but payload has little retained semantic content"
            },
        )
    } else if semantic_kind == "focused_element_changed" {
        (
            "ax_focused_element_changed",
            0.56,
            false,
            0.58,
            "focused element transition marks a new local interaction target",
        )
    } else if semantic_kind == "focused_window_changed" {
        (
            "ax_focused_window_changed",
            0.78,
            true,
            0.82,
            "focused window transition marks a workspace switch and is preserved as a transition receipt",
        )
    } else {
        (
            "ax_semantic_event",
            0.42,
            false,
            0.50,
            "semantic accessibility event is recognized but weakly typed",
        )
    };

    ExtractedSignal {
        kind: kind.to_string(),
        strength,
        hard_keep,
        memory_value_multiplier: multiplier,
        region: region_at(
            &event.payload,
            &["payload", "focusedElement", "frame"],
            kind,
            "Semantic accessibility event location.",
        ),
        explanation: format!(
            "reason={kind} semantic_kind={semantic_kind} app={app_name} role={role} selection_chars={selection_chars} value_chars={value_chars} description=\"{}\"; {tail}{}.",
            compact(description, 80),
            if hard_keep { " and is hard_keep" } else { "" }
        ),
    }
}

fn focus_transition_signal(_event: &CaptureEvent) -> ExtractedSignal {
    ExtractedSignal {
        kind: "ux_focus_transition".to_string(),
        strength: 0.40,
        hard_keep: false,
        memory_value_multiplier: 0.45,
        region: None,
        explanation: "reason=ux_focus_transition; raw UX focus transition marks a likely context switch near this candidate.".to_string(),
    }
}

fn active_window_frame_signal(event: &CaptureEvent) -> ExtractedSignal {
    let dirty_area_ratio = f32_at(
        &event.payload,
        &["payload", "motionFeatures", "dirtyAreaRatio"],
    )
    .or_else(|| {
        f32_at(
            &event.payload,
            &["payload", "dirtyRectSummary", "dirtyAreaRatio"],
        )
    })
    .unwrap_or(0.0);
    let changed_tile_ratio = f32_at(
        &event.payload,
        &["payload", "motionFeatures", "changedTileRatio"],
    )
    .or_else(|| {
        f32_at(
            &event.payload,
            &["payload", "dirtyRectSummary", "changedTileRatio"],
        )
    })
    .unwrap_or(0.0);
    let dirty_rect_count = u64_at(
        &event.payload,
        &["payload", "motionFeatures", "dirtyRectCount"],
    )
    .or_else(|| {
        u64_at(
            &event.payload,
            &["payload", "dirtyRectSummary", "dirtyRectCount"],
        )
    })
    .unwrap_or(0);
    let estimated_dy = f32_at(
        &event.payload,
        &["payload", "motionFeatures", "estimatedDY"],
    )
    .or_else(|| {
        f32_at(
            &event.payload,
            &["payload", "dirtyRectSummary", "estimatedDY"],
        )
    })
    .unwrap_or(0.0);
    let mean_pixel_diff = f32_at(
        &event.payload,
        &["payload", "motionFeatures", "meanPixelDiff"],
    )
    .unwrap_or(0.0);
    let update_reason = str_at(
        &event.payload,
        &["payload", "adaptiveDecision", "updateReason"],
    )
    .unwrap_or("");
    let should_store_keyframe = bool_at(
        &event.payload,
        &["payload", "adaptiveDecision", "shouldStoreKeyframe"],
    )
    .unwrap_or(false);
    let keyboard_recent = bool_at(
        &event.payload,
        &["payload", "motionFeatures", "keyboardEventRecently"],
    )
    .or_else(|| {
        bool_at(
            &event.payload,
            &["payload", "adaptiveDecision", "keyboardEventRecently"],
        )
    })
    .unwrap_or(false);
    let scroll_recent = bool_at(
        &event.payload,
        &["payload", "motionFeatures", "scrollEventRecently"],
    )
    .or_else(|| {
        bool_at(
            &event.payload,
            &["payload", "adaptiveDecision", "scrollEventRecently"],
        )
    })
    .unwrap_or(false);
    let no_visual_change = dirty_area_ratio <= 0.0
        && changed_tile_ratio <= 0.0
        && dirty_rect_count == 0
        && estimated_dy.abs() <= f32::EPSILON
        && mean_pixel_diff <= 0.0;

    if no_visual_change {
        return ExtractedSignal {
            kind: "visual_static_low_information".to_string(),
            strength: 0.05,
            hard_keep: false,
            memory_value_multiplier: 0.10,
            region: None,
            explanation: format!(
                "reason=visual_static_low_information dirty_area_ratio={dirty_area_ratio:.3} changed_tile_ratio={changed_tile_ratio:.3} dirty_rect_count={dirty_rect_count} estimated_dy={estimated_dy:.1} update_reason={update_reason}; zero dirty/changed ratios are treated as a low-information visual penalty."
            ),
        };
    }

    let motion_ratio = dirty_area_ratio
        .max(changed_tile_ratio)
        .max(mean_pixel_diff);
    let kind = if dirty_rect_count > 0 {
        "visual_dirty_region_metadata"
    } else {
        "visual_motion_metadata"
    };
    let strength = 0.22
        + ratio(motion_ratio, 1.0) * 0.32
        + ratio(dirty_rect_count as f32, 12.0) * 0.12
        + ratio(estimated_dy.abs(), 900.0) * 0.08
        + if keyboard_recent || scroll_recent {
            0.04
        } else {
            0.0
        }
        + if should_store_keyframe { 0.06 } else { 0.0 };

    ExtractedSignal {
        kind: kind.to_string(),
        strength,
        hard_keep: false,
        memory_value_multiplier: 0.40,
        region: dirty_rect_region(&event.payload),
        explanation: format!(
            "reason={kind} dirty_area_ratio={dirty_area_ratio:.3} changed_tile_ratio={changed_tile_ratio:.3} dirty_rect_count={dirty_rect_count} estimated_dy={estimated_dy:.1} mean_pixel_diff={mean_pixel_diff:.3} update_reason={update_reason} keyboard_recent={keyboard_recent} scroll_recent={scroll_recent} should_store_keyframe={should_store_keyframe}; visual metadata adds motion evidence around the candidate."
        ),
    }
}

fn visual_frame_change_signal(event: &CaptureEvent) -> ExtractedSignal {
    let full_diff_score = f32_at(&event.payload, &["payload", "full_diff_score"]).unwrap_or(0.0);
    let top_band_diff_score =
        f32_at(&event.payload, &["payload", "top_band_diff_score"]).unwrap_or(0.0);
    let from_frame = u64_at(&event.payload, &["payload", "from_frame"]).unwrap_or(0);
    let to_frame = u64_at(&event.payload, &["payload", "to_frame"]).unwrap_or(0);
    let reason = str_at(&event.payload, &["payload", "reason"]).unwrap_or("visual frame changed");
    let window_band_change = top_band_diff_score >= 0.06;
    let kind = if window_band_change {
        "visual_window_transition"
    } else {
        "visual_content_change"
    };
    let strength = (0.28
        + ratio(full_diff_score, 0.40) * 0.28
        + ratio(top_band_diff_score, 0.25) * 0.30
        + if window_band_change { 0.10 } else { 0.0 })
    .min(0.92);

    ExtractedSignal {
        kind: kind.to_string(),
        strength,
        hard_keep: window_band_change,
        memory_value_multiplier: if window_band_change { 0.72 } else { 0.46 },
        region: None,
        explanation: format!(
            "reason={kind} from_frame={from_frame} to_frame={to_frame} full_diff_score={full_diff_score:.3} top_band_diff_score={top_band_diff_score:.3}; {reason}."
        ),
    }
}

fn value_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut cursor = value;
    for key in path {
        cursor = cursor.get(*key)?;
    }
    Some(cursor)
}

fn str_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    value_at(value, path).and_then(Value::as_str)
}

fn str_child<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a str> {
    value
        .and_then(|value| value.get(key))
        .and_then(Value::as_str)
}

fn u64_at(value: &Value, path: &[&str]) -> Option<u64> {
    value_at(value, path).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
            .or_else(|| value.as_f64().map(|number| number.max(0.0) as u64))
    })
}

fn u64_child(value: Option<&Value>, key: &str) -> Option<u64> {
    value.and_then(|value| value.get(key)).and_then(|value| {
        value
            .as_u64()
            .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
            .or_else(|| value.as_f64().map(|number| number.max(0.0) as u64))
    })
}

fn f32_at(value: &Value, path: &[&str]) -> Option<f32> {
    value_at(value, path).and_then(number_as_f32)
}

fn f32_child(value: Option<&Value>, key: &str) -> Option<f32> {
    value
        .and_then(|value| value.get(key))
        .and_then(number_as_f32)
}

fn number_as_f32(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|number| number as f32)
        .or_else(|| value.as_i64().map(|number| number as f32))
        .or_else(|| value.as_u64().map(|number| number as f32))
}

fn bool_at(value: &Value, path: &[&str]) -> Option<bool> {
    value_at(value, path).and_then(Value::as_bool)
}

fn string_array_at(value: &Value, path: &[&str]) -> Vec<String> {
    value_at(value, path)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn selection_character_count(value: &Value) -> u64 {
    u64_at(
        value,
        &["payload", "selection", "selectedTextCharacterCount"],
    )
    .or_else(|| {
        u64_at(
            value,
            &[
                "payload",
                "focusedElement",
                "selection",
                "selectedTextCharacterCount",
            ],
        )
    })
    .or_else(|| u64_at(value, &["payload", "selection", "range", "length"]))
    .or_else(|| {
        u64_at(
            value,
            &["payload", "focusedElement", "selection", "range", "length"],
        )
    })
    .or_else(|| {
        str_at(value, &["payload", "selection", "selectedText"])
            .or_else(|| {
                str_at(
                    value,
                    &["payload", "focusedElement", "selection", "selectedText"],
                )
            })
            .map(|text| text.chars().count() as u64)
    })
    .unwrap_or(0)
}

fn value_character_count(value: &Value) -> u64 {
    u64_at(value, &["payload", "valueShape", "characterCount"])
        .or_else(|| {
            u64_at(
                value,
                &["payload", "focusedElement", "valueShape", "characterCount"],
            )
        })
        .unwrap_or(0)
}

fn contains_outcome_phrase(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    ["copied", "saved", "submitted", "sent", "committed"]
        .iter()
        .any(|needle| lower.contains(needle))
}

fn region_at(
    value: &Value,
    path: &[&str],
    label: &str,
    explanation: &str,
) -> Option<AttentionRegion> {
    let frame = value_at(value, path)?;
    let rect = rect_from_value(frame)?;
    Some(AttentionRegion {
        bbox: rect,
        score: 0.72,
        tint: "semantic".to_string(),
        label: label.to_string(),
        explanation: explanation.to_string(),
    })
}

fn dirty_rect_region(value: &Value) -> Option<AttentionRegion> {
    let rect = value_at(value, &["payload", "dirtyRectSummary", "cappedRects"])?
        .as_array()?
        .first()
        .and_then(rect_from_value)?;
    Some(AttentionRegion {
        bbox: rect,
        score: 0.55,
        tint: "motion".to_string(),
        label: "visual_dirty_region_metadata".to_string(),
        explanation: "First dirty rect reported by active window frame metadata.".to_string(),
    })
}

fn rect_from_value(value: &Value) -> Option<Rect> {
    Some(Rect {
        x: f32_at(value, &["x"])?,
        y: f32_at(value, &["y"])?,
        width: f32_at(value, &["width"])?,
        height: f32_at(value, &["height"])?,
    })
}

fn ratio(value: f32, max: f32) -> f32 {
    if max <= 0.0 {
        0.0
    } else {
        (value / max).clamp(0.0, 1.0)
    }
}

fn clamp_score(value: f32) -> f32 {
    value.clamp(0.0, 1.0)
}

fn optional_u64(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn list_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join("|")
    }
}

fn compact(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let shortened = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{shortened}...")
    } else {
        shortened
    }
}
