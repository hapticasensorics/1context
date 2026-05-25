mod agent_packet;
mod decision;
mod metrics;
mod raw_audit;
mod saved_states;
mod video;

pub use decision::decision_panel;
pub use video::video_panel;

use eframe::egui;

use crate::timeline::{TimelineEvent, TimelineLane, TimelineState, TimelineViewState};

pub fn timeline_panel(
    ui: &mut egui::Ui,
    timeline: &TimelineState,
    view: &mut TimelineViewState,
    current_time_ms: &mut u64,
    selected_candidate_id: &mut Option<String>,
    selected_saved_state_id: &mut Option<String>,
) {
    ui.heading("Timeline");
    ui.horizontal_wrapped(|ui| {
        ui.label(format!("{} lanes", timeline.lanes.len()));
        ui.label(format!("{} events", timeline.events.len()));
        ui.label(format!("current: {:.2}s", *current_time_ms as f32 / 1000.0));
        ui.label(format!(
            "duration: {:.2}s",
            timeline.duration_ms as f32 / 1000.0
        ));
    });
    if !timeline.warnings.is_empty() {
        ui.collapsing(
            format!("{} timeline warnings", timeline.warnings.len()),
            |ui| {
                for warning in &timeline.warnings {
                    ui.small(warning);
                }
            },
        );
    }

    let duration_ms = timeline.duration_ms.max(1);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for lane in timeline.visible_lanes() {
                draw_lane_row(
                    ui,
                    lane,
                    timeline,
                    view,
                    duration_ms,
                    current_time_ms,
                    selected_candidate_id,
                    selected_saved_state_id,
                );
            }
        });
}

fn draw_lane_row(
    ui: &mut egui::Ui,
    lane: &TimelineLane,
    timeline: &TimelineState,
    view: &TimelineViewState,
    duration_ms: u64,
    current_time_ms: &mut u64,
    selected_candidate_id: &mut Option<String>,
    selected_saved_state_id: &mut Option<String>,
) {
    let row_height = view.lane_height.max(24.0);
    let desired_size = egui::vec2(ui.available_width().max(520.0), row_height);
    let (row_rect, row_response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    let painter = ui.painter_at(row_rect);
    let label_width = view.label_width.min(row_rect.width() * 0.38).max(104.0);
    let gap = 8.0;
    let label_rect = egui::Rect::from_min_max(
        row_rect.min,
        egui::pos2(row_rect.min.x + label_width, row_rect.max.y),
    );
    let track_rect = egui::Rect::from_min_max(
        egui::pos2(label_rect.max.x + gap, row_rect.min.y + 4.0),
        egui::pos2(row_rect.max.x, row_rect.max.y - 4.0),
    );

    painter.rect_filled(row_rect, 0.0, egui::Color32::from_gray(18));
    painter.rect_filled(label_rect, 0.0, egui::Color32::from_gray(24));
    painter.rect_filled(track_rect, 0.0, egui::Color32::from_gray(30));
    painter.rect_stroke(
        track_rect,
        0.0,
        egui::Stroke::new(1.0, lane.soft_color()),
        egui::StrokeKind::Inside,
    );
    painter.text(
        label_rect.left_center() + egui::vec2(8.0, 0.0),
        egui::Align2::LEFT_CENTER,
        format!("{} ({})", lane.title, timeline.lane_event_count(&lane.id)),
        egui::FontId::proportional(12.0),
        lane.color,
    );
    painter.text(
        label_rect.right_center() - egui::vec2(8.0, 0.0),
        egui::Align2::RIGHT_CENTER,
        &lane.kind,
        egui::FontId::proportional(10.0),
        egui::Color32::from_gray(150),
    );

    draw_time_ticks(&painter, track_rect, duration_ms);

    if row_response.clicked() {
        if let Some(pointer_pos) = row_response.interact_pointer_pos() {
            if track_rect.contains(pointer_pos) {
                *current_time_ms = time_from_x(track_rect, pointer_pos.x, duration_ms);
            }
        }
    }

    for event in timeline
        .events
        .iter()
        .filter(|event| event.lane_id == lane.id)
    {
        draw_event_marker(
            ui,
            &painter,
            lane,
            event,
            track_rect,
            duration_ms,
            view.min_event_width.max(4.0),
            current_time_ms,
            selected_candidate_id,
            selected_saved_state_id,
        );
    }

    draw_playhead(&painter, track_rect, duration_ms, *current_time_ms);
    ui.add_space(4.0);
}

fn draw_event_marker(
    ui: &mut egui::Ui,
    painter: &egui::Painter,
    lane: &TimelineLane,
    event: &TimelineEvent,
    track_rect: egui::Rect,
    duration_ms: u64,
    min_width: f32,
    current_time_ms: &mut u64,
    selected_candidate_id: &mut Option<String>,
    selected_saved_state_id: &mut Option<String>,
) {
    let x = x_from_time(track_rect, event.t_ms, duration_ms);
    let end_x = x_from_time(track_rect, event.end_ms(), duration_ms);
    let width = (end_x - x).abs().max(min_width);
    let marker_rect = if event.duration_ms.unwrap_or(0) > 0 {
        egui::Rect::from_min_size(
            egui::pos2(x, track_rect.center().y - 5.0),
            egui::vec2(width, 10.0),
        )
    } else {
        egui::Rect::from_center_size(
            egui::pos2(x, track_rect.center().y),
            egui::vec2(min_width, track_rect.height() - 4.0),
        )
    }
    .intersect(track_rect.expand(2.0));

    let selected = event_is_selected(
        event,
        selected_candidate_id.as_deref(),
        selected_saved_state_id.as_deref(),
    );
    let fill = lane.event_color(selected);
    painter.rect_filled(marker_rect, 1.5, fill);
    if selected {
        painter.rect_stroke(
            marker_rect.expand(2.0),
            1.5,
            egui::Stroke::new(1.5, lane.color),
            egui::StrokeKind::Outside,
        );
    }
    draw_event_label(painter, lane, event, marker_rect, track_rect);

    let response = ui.interact(
        marker_rect.expand(3.0),
        ui.id()
            .with("timeline-event")
            .with(&event.lane_id)
            .with(&event.id),
        egui::Sense::click(),
    );
    if response.clicked() {
        *current_time_ms = event.t_ms.min(duration_ms);
        *selected_candidate_id = event.candidate_id.clone();
        *selected_saved_state_id = event.saved_state_id.clone();
    }
    response.on_hover_text(event.tooltip_text());
}

fn draw_event_label(
    painter: &egui::Painter,
    lane: &TimelineLane,
    event: &TimelineEvent,
    marker_rect: egui::Rect,
    track_rect: egui::Rect,
) {
    if !event_label_is_useful(event) {
        return;
    }

    let right_room = track_rect.right() - marker_rect.right() - 8.0;
    let left_room = marker_rect.left() - track_rect.left() - 8.0;
    let (pos, align, room) = if right_room >= 72.0 || right_room >= left_room {
        (
            egui::pos2(marker_rect.right() + 5.0, track_rect.center().y),
            egui::Align2::LEFT_CENTER,
            right_room,
        )
    } else {
        (
            egui::pos2(marker_rect.left() - 5.0, track_rect.center().y),
            egui::Align2::RIGHT_CENTER,
            left_room,
        )
    };
    let Some(label) = fitted_label(&event.title, room) else {
        return;
    };
    painter.text(
        pos,
        align,
        label,
        egui::FontId::proportional(10.0),
        lane.color,
    );
}

fn event_label_is_useful(event: &TimelineEvent) -> bool {
    matches!(
        event.lane_id.as_str(),
        "window-changes" | "focus-transitions" | "focused-elements"
    )
}

fn fitted_label(label: &str, available_width: f32) -> Option<String> {
    let max_chars = (available_width / 5.7).floor() as usize;
    if max_chars < 8 {
        return None;
    }
    if label.chars().count() <= max_chars {
        return Some(label.to_string());
    }
    Some(format!(
        "{}...",
        label
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect::<String>()
    ))
}

fn draw_time_ticks(painter: &egui::Painter, track_rect: egui::Rect, duration_ms: u64) {
    let tick_count = 6;
    for tick in 0..=tick_count {
        let ratio = tick as f32 / tick_count as f32;
        let x = egui::lerp(track_rect.left()..=track_rect.right(), ratio);
        let color = if tick == 0 || tick == tick_count {
            egui::Color32::from_gray(80)
        } else {
            egui::Color32::from_gray(48)
        };
        painter.line_segment(
            [
                egui::pos2(x, track_rect.top()),
                egui::pos2(x, track_rect.bottom()),
            ],
            egui::Stroke::new(1.0, color),
        );
        if tick != 0 && tick != tick_count {
            let t_ms = (duration_ms as f32 * ratio).round() as u64;
            painter.text(
                egui::pos2(x + 3.0, track_rect.top() + 1.0),
                egui::Align2::LEFT_TOP,
                format!("{:.0}s", t_ms as f32 / 1000.0),
                egui::FontId::proportional(9.0),
                egui::Color32::from_gray(120),
            );
        }
    }
}

fn draw_playhead(
    painter: &egui::Painter,
    track_rect: egui::Rect,
    duration_ms: u64,
    current_time_ms: u64,
) {
    let x = x_from_time(track_rect, current_time_ms, duration_ms);
    painter.line_segment(
        [
            egui::pos2(x, track_rect.top() - 3.0),
            egui::pos2(x, track_rect.bottom() + 3.0),
        ],
        egui::Stroke::new(2.0, egui::Color32::WHITE),
    );
}

fn x_from_time(track_rect: egui::Rect, t_ms: u64, duration_ms: u64) -> f32 {
    let ratio = (t_ms.min(duration_ms) as f32 / duration_ms.max(1) as f32).clamp(0.0, 1.0);
    egui::lerp(track_rect.left()..=track_rect.right(), ratio)
}

fn time_from_x(track_rect: egui::Rect, x: f32, duration_ms: u64) -> u64 {
    let ratio = ((x - track_rect.left()) / track_rect.width().max(1.0)).clamp(0.0, 1.0);
    (duration_ms as f32 * ratio).round() as u64
}

fn event_is_selected(
    event: &TimelineEvent,
    selected_candidate_id: Option<&str>,
    selected_saved_state_id: Option<&str>,
) -> bool {
    event
        .candidate_id
        .as_deref()
        .is_some_and(|candidate_id| Some(candidate_id) == selected_candidate_id)
        || event
            .saved_state_id
            .as_deref()
            .is_some_and(|saved_state_id| Some(saved_state_id) == selected_saved_state_id)
}
