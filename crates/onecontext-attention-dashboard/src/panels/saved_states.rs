use eframe::egui;

use crate::{
    fixture::DashboardFixture,
    media::{ImageAssetCache, LoadedImage},
    schema::{
        AttentionFilterOutput, AttentionRegion, DashboardSignal, DecisionExplanation,
        RawBufferItem, SavedAttentionState,
    },
};

pub(super) fn render_minute_visual_output(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    fixture: &DashboardFixture,
    image_assets: &mut ImageAssetCache,
    selected_saved_state_id: Option<&str>,
) {
    let output = &fixture.filter_output;
    ui.horizontal_wrapped(|ui| {
        ui.strong("End-of-minute attention output");
        ui.label(format!("{} saved receipts", output.saved_states.len()));
        ui.label(format!(
            "{} candidate screenshots",
            output.raw_buffer_audit.len()
        ));
        ui.label(format!(
            "range: {} - {}",
            format_time_ms(output.time_range_ms[0]),
            format_time_ms(output.time_range_ms[1])
        ));
    });
    if !output.summary.activity_summary.is_empty() {
        ui.label(&output.summary.activity_summary);
    }

    if output.saved_states.is_empty() {
        ui.label("No saved visual receipts are available yet.");
        return;
    }

    for state in &output.saved_states {
        render_saved_visual_receipt(
            ui,
            ctx,
            fixture,
            image_assets,
            state,
            selected_saved_state_id,
        );
    }

    egui::CollapsingHeader::new(format!(
        "Candidate filter contact sheet ({})",
        output.raw_buffer_audit.len()
    ))
    .default_open(false)
    .show(ui, |ui| {
        render_candidate_contact_sheet(ui, ctx, fixture, image_assets, &output.raw_buffer_audit);
    });
}

fn render_saved_visual_receipt(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    fixture: &DashboardFixture,
    image_assets: &mut ImageAssetCache,
    state: &SavedAttentionState,
    selected_saved_state_id: Option<&str>,
) {
    let selected = selected_saved_state_id == Some(state.id.as_str());
    let title = if state.title.is_empty() {
        state.id.as_str()
    } else {
        state.title.as_str()
    };
    ui.group(|ui| {
        ui.horizontal_wrapped(|ui| {
            ui.strong(format!(
                "{}  {}",
                decision_label(&state.decision),
                format_time_ms(state.time_ms)
            ));
            if selected {
                ui.colored_label(egui::Color32::from_rgb(80, 160, 255), "selected");
            }
            ui.label(title);
        });

        if let Some(path) = state
            .base_screenshot_ref
            .as_deref()
            .or(state.thumbnail_ref.as_deref())
        {
            let path = fixture.resolve_visual_asset(path);
            match image_assets.image_for_path(ctx, &path) {
                Some(image) => {
                    draw_attention_image(ui, image, &state.overlay_regions, 360.0, true);
                }
                None => {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 64, 64),
                        image_assets
                            .last_error()
                            .unwrap_or("could not load saved screenshot"),
                    );
                    ui.small(path.display().to_string());
                }
            }
        } else {
            ui.colored_label(
                egui::Color32::from_rgb(220, 126, 38),
                "saved state has no screenshot receipt",
            );
        }

        ui.horizontal_wrapped(|ui| {
            if let Some(app) = &state.app_name {
                score_label(ui, "app", app);
            }
            if let Some(window) = &state.window_title {
                score_label(ui, "window", window);
            }
            if let Some(explanation) = &state.explanation {
                score_pill(ui, "attention", explanation.attention_score);
                score_pill(ui, "memory", explanation.memory_value_score);
                score_pill(ui, "confidence", explanation.confidence);
            }
        });
        if let Some(explanation) = &state.explanation {
            if !explanation.primary_reason.is_empty() {
                ui.small(&explanation.primary_reason);
            }
        }
        if !state.overlay_regions.is_empty() {
            ui.small(format!("{} overlay regions", state.overlay_regions.len()));
        }
    });
}

fn render_candidate_contact_sheet(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    fixture: &DashboardFixture,
    image_assets: &mut ImageAssetCache,
    raw_items: &[RawBufferItem],
) {
    if raw_items.is_empty() {
        ui.label("No candidate screenshot audit is available.");
        return;
    }

    let columns = (ui.available_width() / 136.0).floor().max(1.0) as usize;
    egui::Grid::new("attention-candidate-contact-sheet")
        .num_columns(columns)
        .spacing([8.0, 8.0])
        .show(ui, |ui| {
            for (index, item) in raw_items.iter().enumerate() {
                render_candidate_tile(ui, ctx, fixture, image_assets, item);
                if (index + 1) % columns == 0 {
                    ui.end_row();
                }
            }
        });
}

fn render_candidate_tile(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    fixture: &DashboardFixture,
    image_assets: &mut ImageAssetCache,
    item: &RawBufferItem,
) {
    let regions = signal_regions(&item.top_signals);
    let border = decision_color(&item.decision);
    ui.vertical(|ui| {
        ui.set_width(128.0);
        let path = fixture.resolve_visual_asset(&item.thumbnail_ref);
        if let Some(image) = image_assets.image_for_path(ctx, &path) {
            let response = draw_attention_image(ui, image, &regions, 92.0, true);
            ui.painter().rect_stroke(
                response.rect.expand(2.0),
                2.0,
                egui::Stroke::new(2.0, border),
                egui::StrokeKind::Outside,
            );
        } else {
            ui.allocate_ui(egui::vec2(128.0, 72.0), |ui| {
                ui.centered_and_justified(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(220, 64, 64), "missing image");
                });
            });
        }
        ui.small(format!("{}  {}", format_time_ms(item.t_ms), item.decision));
    });
}

fn draw_attention_image(
    ui: &mut egui::Ui,
    image: &LoadedImage,
    regions: &[AttentionRegion],
    max_height: f32,
    allow_upscale: bool,
) -> egui::Response {
    let image_size = egui::vec2(image.size[0] as f32, image.size[1] as f32);
    let available_width = ui.available_width().max(96.0);
    let mut scale = (available_width / image_size.x).min(max_height.max(1.0) / image_size.y);
    if !allow_upscale {
        scale = scale.min(1.0);
    }
    let scaled_size = image_size * scale.max(0.02);
    let (rect, response) = ui.allocate_exact_size(scaled_size, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.image(
        image.texture.id(),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        egui::Color32::WHITE,
    );

    for region in regions {
        draw_region_overlay(&painter, rect, image_size, region);
    }

    response.on_hover_text(image.path.display().to_string())
}

fn draw_region_overlay(
    painter: &egui::Painter,
    image_rect: egui::Rect,
    source_size: egui::Vec2,
    region: &AttentionRegion,
) {
    let Some(region_rect) = region
        .bbox
        .as_ref()
        .and_then(|bbox| region_rect_on_image(bbox, image_rect, source_size))
    else {
        return;
    };
    let color = tint_color(&region.tint);
    let stroke = egui::Color32::from_rgba_premultiplied(color.r(), color.g(), color.b(), 230);
    painter.rect_stroke(
        region_rect,
        2.0,
        egui::Stroke::new(2.0, stroke),
        egui::StrokeKind::Inside,
    );

    let label = if region.label.is_empty() {
        "attention"
    } else {
        region.label.as_str()
    };
    let label_pos = region_rect.left_top() + egui::vec2(6.0, 6.0);
    let label_rect =
        egui::Rect::from_min_size(label_pos, egui::vec2(132.0, 22.0)).intersect(image_rect);
    painter.rect_filled(
        label_rect,
        3.0,
        egui::Color32::from_rgba_premultiplied(10, 10, 10, 190),
    );
    painter.text(
        label_rect.left_center() + egui::vec2(6.0, 0.0),
        egui::Align2::LEFT_CENTER,
        fitted_overlay_label(label),
        egui::FontId::proportional(11.0),
        egui::Color32::WHITE,
    );
}

fn region_rect_on_image(
    bbox: &crate::schema::Rect,
    image_rect: egui::Rect,
    source_size: egui::Vec2,
) -> Option<egui::Rect> {
    if bbox.width <= 0.0 || bbox.height <= 0.0 {
        return None;
    }

    let coordinate_space = bbox.coordinate_space.to_ascii_lowercase();
    let normalized = coordinate_space.contains("normalized")
        || coordinate_space.contains("relative")
        || (bbox.x.abs() <= 1.25
            && bbox.y.abs() <= 1.25
            && bbox.width.abs() <= 1.25
            && bbox.height.abs() <= 1.25);
    let percent = coordinate_space.contains("percent");

    let (x, y, width, height) = if normalized {
        (
            bbox.x * source_size.x,
            bbox.y * source_size.y,
            bbox.width * source_size.x,
            bbox.height * source_size.y,
        )
    } else if percent {
        (
            bbox.x / 100.0 * source_size.x,
            bbox.y / 100.0 * source_size.y,
            bbox.width / 100.0 * source_size.x,
            bbox.height / 100.0 * source_size.y,
        )
    } else {
        (bbox.x, bbox.y, bbox.width, bbox.height)
    };

    let scale_x = image_rect.width() / source_size.x.max(1.0);
    let scale_y = image_rect.height() / source_size.y.max(1.0);
    let rect = egui::Rect::from_min_size(
        egui::pos2(
            image_rect.left() + x * scale_x,
            image_rect.top() + y * scale_y,
        ),
        egui::vec2(width * scale_x, height * scale_y),
    )
    .intersect(image_rect);

    (rect.width() >= 2.0 && rect.height() >= 2.0).then_some(rect)
}

fn signal_regions(signals: &[DashboardSignal]) -> Vec<AttentionRegion> {
    signals
        .iter()
        .filter_map(|signal| signal.region.clone())
        .take(3)
        .collect()
}

fn tint_color(tint: &str) -> egui::Color32 {
    if let Some(color) = parse_hex_color(tint) {
        return color;
    }
    match tint.to_ascii_lowercase().as_str() {
        "green" | "coverage" | "semantic" => egui::Color32::from_rgb(34, 197, 94),
        "blue" | "transition" => egui::Color32::from_rgb(59, 130, 246),
        "purple" | "outcome" => egui::Color32::from_rgb(168, 85, 247),
        "red" | "error" => egui::Color32::from_rgb(239, 68, 68),
        "gray" | "grey" | "rejected" => egui::Color32::from_rgb(148, 163, 184),
        "yellow" | "selection" => egui::Color32::from_rgb(234, 179, 8),
        "motion" => egui::Color32::from_rgb(249, 115, 22),
        _ => egui::Color32::from_rgb(245, 158, 11),
    }
}

fn parse_hex_color(value: &str) -> Option<egui::Color32> {
    let hex = value.trim().strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    let red = u8::from_str_radix(&hex[0..2], 16).ok()?;
    let green = u8::from_str_radix(&hex[2..4], 16).ok()?;
    let blue = u8::from_str_radix(&hex[4..6], 16).ok()?;
    Some(egui::Color32::from_rgb(red, green, blue))
}

fn decision_color(decision: &str) -> egui::Color32 {
    if decision == "saved" || decision.starts_with("save") {
        egui::Color32::from_rgb(34, 197, 94)
    } else if decision.contains("debt") || decision.contains("missed") {
        egui::Color32::from_rgb(239, 68, 68)
    } else {
        egui::Color32::from_rgb(148, 163, 184)
    }
}

fn fitted_overlay_label(label: &str) -> String {
    let max_chars = 22;
    if label.chars().count() <= max_chars {
        return label.to_string();
    }
    format!(
        "{}...",
        label.chars().take(max_chars - 3).collect::<String>()
    )
}

fn score_label(ui: &mut egui::Ui, label: &str, value: &str) {
    if !value.is_empty() {
        ui.small(format!("{label}: {value}"));
    }
}

pub(super) fn render_saved_states(
    ui: &mut egui::Ui,
    output: &AttentionFilterOutput,
    selected_saved_state_id: Option<&str>,
) {
    if output.saved_states.is_empty() {
        ui.label("No saved states yet.");
        return;
    }

    for state in &output.saved_states {
        let selected = selected_saved_state_id == Some(state.id.as_str());
        let title = state_title(state);
        egui::CollapsingHeader::new(format!(
            "{}  {:.2}s  {}",
            decision_label(&state.decision),
            state.time_ms as f32 / 1000.0,
            title
        ))
        .default_open(selected)
        .show(ui, |ui| {
            render_saved_state_detail(ui, state);
        });
    }
}

pub(super) fn render_saved_state_detail(ui: &mut egui::Ui, state: &SavedAttentionState) {
    ui.push_id(("saved-state-detail", &state.id), |ui| {
        render_saved_state_detail_inner(ui, state);
    });
}

fn render_saved_state_detail_inner(ui: &mut egui::Ui, state: &SavedAttentionState) {
    key_value(ui, "id", &state.id);
    if let Some(candidate_id) = &state.candidate_id {
        key_value(ui, "candidate", candidate_id);
    }
    key_value(ui, "decision", &state.decision);
    key_value(
        ui,
        "time",
        &format_duration(state.time_ms, state.duration_ms),
    );

    if let Some(app_name) = &state.app_name {
        key_value(ui, "app", app_name);
    }
    if let Some(window_title) = &state.window_title {
        key_value(ui, "window", window_title);
    }
    if let Some(url) = &state.url {
        key_value(ui, "url", url);
    }
    if let Some(active_file) = &state.active_file {
        key_value(ui, "file", active_file);
    }
    if let Some(command) = &state.terminal_command {
        key_value(ui, "command", command);
    }
    if let Some(path) = &state.thumbnail_ref {
        key_value(ui, "thumbnail", path);
    }
    if let Some(path) = &state.base_screenshot_ref {
        key_value(ui, "screenshot", path);
    }

    if let Some(excerpt) = &state.semantic_excerpt {
        ui.separator();
        ui.strong("Semantic Excerpt");
        ui.label(excerpt);
    }
    if let Some(summary) = &state.redaction_summary {
        ui.separator();
        ui.strong("Redaction");
        ui.label(summary);
    }

    if let Some(explanation) = &state.explanation {
        ui.separator();
        render_explanation(ui, explanation);
    }

    if !state.overlay_regions.is_empty() {
        ui.separator();
        render_overlay_regions(ui, &state.overlay_regions);
    }

    if let Some(proof_bundle) = &state.proof_bundle {
        ui.separator();
        ui.strong("Proof Bundle");
        if let Some(tier) = &proof_bundle.proof_tier {
            key_value(ui, "tier", tier);
        }
        if let Some(explanation) = &proof_bundle.explanation {
            ui.label(explanation);
        }
        for evidence in &proof_bundle.evidence_refs {
            let label = evidence
                .label
                .as_deref()
                .or(evidence.id.as_deref())
                .unwrap_or("evidence");
            let path = evidence.path.as_deref().unwrap_or("");
            key_value(ui, label, path);
        }
        render_string_list(ui, "raw events", &proof_bundle.raw_event_refs);
        render_string_list(ui, "screenshots", &proof_bundle.screenshot_refs);
        render_string_list(ui, "semantic refs", &proof_bundle.semantic_refs);
    }

    render_string_list(ui, "composites", &state.related_composite_ids);
    render_string_list(ui, "object lineage", &state.related_object_lineage_ids);

    if !state.extra.is_empty() {
        ui.separator();
        ui.strong("Extra Fields");
        render_json_map(ui, &state.extra);
    }
}

pub(super) fn render_explanation(ui: &mut egui::Ui, explanation: &DecisionExplanation) {
    if !explanation.primary_reason.is_empty() {
        ui.strong(&explanation.primary_reason);
    }
    if !explanation.reasons.is_empty() {
        ui.horizontal_wrapped(|ui| {
            for reason in &explanation.reasons {
                ui.label(format!("[{reason}]"));
            }
        });
    }

    ui.horizontal_wrapped(|ui| {
        score_pill(ui, "attention", explanation.attention_score);
        score_pill(ui, "memory", explanation.memory_value_score);
        score_pill(ui, "confidence", explanation.confidence);
    });

    if !explanation.score_components.is_empty() {
        ui.collapsing("Score Components", |ui| {
            render_json_map(ui, &explanation.score_components);
        });
    }

    if !explanation.algorithm_votes.is_empty() {
        ui.collapsing("Algorithm Votes", |ui| {
            for vote in &explanation.algorithm_votes {
                ui.horizontal_wrapped(|ui| {
                    ui.strong(if vote.algorithm.is_empty() {
                        "algorithm"
                    } else {
                        &vote.algorithm
                    });
                    ui.label(if vote.vote.is_empty() {
                        "vote"
                    } else {
                        &vote.vote
                    });
                    score_pill(ui, "strength", vote.strength);
                });
                if !vote.reason.is_empty() {
                    ui.small(&vote.reason);
                }
            }
        });
    }

    if !explanation.source_conflicts.is_empty() {
        ui.collapsing("Source Conflicts", |ui| {
            for conflict in &explanation.source_conflicts {
                ui.monospace(json_preview(conflict));
            }
        });
    }

    if !explanation.extra.is_empty() {
        ui.collapsing("Extra Explanation Fields", |ui| {
            render_json_map(ui, &explanation.extra);
        });
    }
}

pub(super) fn render_overlay_regions(ui: &mut egui::Ui, regions: &[AttentionRegion]) {
    ui.strong(format!("Overlay Regions ({})", regions.len()));
    for (index, region) in regions.iter().enumerate() {
        let label = if region.label.is_empty() {
            format!("region {}", index + 1)
        } else {
            region.label.clone()
        };
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(label);
                if !region.tint.is_empty() {
                    ui.label(format!("tint: {}", region.tint));
                }
                score_pill(ui, "score", region.score);
                if region.sensitive.unwrap_or(false) {
                    ui.colored_label(egui::Color32::from_rgb(210, 64, 64), "sensitive");
                }
            });
            if let Some(bbox) = &region.bbox {
                key_value(
                    ui,
                    "bbox",
                    &format!(
                        "{:.1}, {:.1}, {:.1} x {:.1} {}",
                        bbox.x, bbox.y, bbox.width, bbox.height, bbox.coordinate_space
                    ),
                );
            }
            render_string_list(ui, "reasons", &region.reasons);
            render_string_list(ui, "evidence", &region.evidence);
            if !region.extra.is_empty() {
                render_json_map(ui, &region.extra);
            }
        });
    }
}

pub(super) fn score_pill(ui: &mut egui::Ui, label: &str, score: f32) {
    let clamped = score.clamp(0.0, 1.0);
    let color = if clamped >= 0.75 {
        egui::Color32::from_rgb(54, 145, 86)
    } else if clamped >= 0.4 {
        egui::Color32::from_rgb(180, 126, 38)
    } else {
        egui::Color32::from_rgb(120, 120, 120)
    };
    ui.colored_label(color, format!("{label}: {:.2}", score));
}

pub(super) fn key_value(ui: &mut egui::Ui, key: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    ui.horizontal_wrapped(|ui| {
        ui.small(format!("{key}:"));
        ui.label(value);
    });
}

pub(super) fn render_string_list(ui: &mut egui::Ui, label: &str, values: &[String]) {
    if values.is_empty() {
        return;
    }
    ui.collapsing(format!("{label} ({})", values.len()), |ui| {
        for value in values {
            ui.label(value);
        }
    });
}

pub(super) fn render_json_map(ui: &mut egui::Ui, map: &serde_json::Map<String, serde_json::Value>) {
    for (key, value) in map {
        key_value(ui, key, &json_preview(value));
    }
}

pub(super) fn json_preview(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Number(value) => value.to_string(),
        serde_json::Value::Bool(value) => value.to_string(),
        serde_json::Value::Null => "null".to_string(),
        _ => serde_json::to_string(value).unwrap_or_else(|_| "<json>".to_string()),
    }
}

pub(super) fn format_time_ms(t_ms: u64) -> String {
    format!("{:.2}s", t_ms as f32 / 1000.0)
}

fn format_duration(time_ms: u64, duration_ms: Option<u64>) -> String {
    if let Some(duration_ms) = duration_ms {
        format!(
            "{} + {}",
            format_time_ms(time_ms),
            format_time_ms(duration_ms)
        )
    } else {
        format_time_ms(time_ms)
    }
}

fn state_title(state: &SavedAttentionState) -> String {
    if !state.title.is_empty() {
        state.title.clone()
    } else if !state.id.is_empty() {
        state.id.clone()
    } else {
        "untitled state".to_string()
    }
}

fn decision_label(decision: &str) -> &str {
    match decision {
        "save_high_attention" => "attention",
        "save_high_memory" => "memory",
        "save_coverage" => "coverage",
        "save_transition" => "transition",
        "save_outcome" => "outcome",
        "save_error" => "error",
        "save_sensitive_redacted" => "redacted",
        _ if decision.is_empty() => "state",
        _ => decision,
    }
}
