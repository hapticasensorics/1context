use std::{path::Path, process::Command};

use eframe::egui;

use crate::{fixture::DashboardFixture, media::FrameCacheState};

pub fn video_panel(
    ui: &mut egui::Ui,
    ctx: &egui::Context,
    fixture: &DashboardFixture,
    media: &mut FrameCacheState,
    current_time_ms: &mut u64,
    is_playing: &mut bool,
) {
    *current_time_ms = (*current_time_ms).min(fixture.duration_ms());
    let video_path = fixture.resolve_fixture_asset(&fixture.session.media.video_ref);
    let external_video = matches!(
        fixture.session.media.playback_mode.as_str(),
        "external_player" | "native_decoder"
    ) && looks_like_video_file(&video_path);

    ui.horizontal_wrapped(|ui| {
        if external_video {
            if ui.button("Open Video").clicked() {
                *is_playing = false;
                let _ = open_media_path(&video_path);
            }
        } else {
            if ui
                .button(if *is_playing { "Pause" } else { "Play" })
                .clicked()
            {
                *is_playing = !*is_playing;
            }
        }
        if ui.button("Frame -").clicked() {
            if let Some(time_ms) = media.previous_frame_time_ms(*current_time_ms) {
                *current_time_ms = time_ms;
                *is_playing = false;
            }
        }
        if ui.button("Frame +").clicked() {
            if let Some(time_ms) = media.next_frame_time_ms(*current_time_ms) {
                *current_time_ms = time_ms.min(fixture.duration_ms());
                *is_playing = false;
            }
        }
        if ui.button("Candidate -").clicked() {
            if let Some(time_ms) = fixture.previous_candidate_time_ms(*current_time_ms) {
                *current_time_ms = time_ms;
                *is_playing = false;
            }
        }
        if ui.button("Candidate +").clicked() {
            if let Some(time_ms) = fixture.next_candidate_time_ms(*current_time_ms) {
                *current_time_ms = time_ms;
                *is_playing = false;
            }
        }
        if ui.button("End").clicked() {
            *current_time_ms = fixture.duration_ms();
            *is_playing = false;
        }
        ui.add(
            egui::Slider::new(current_time_ms, 0..=fixture.duration_ms())
                .text("time")
                .custom_formatter(|value, _| format!("{:.2}s", value / 1000.0)),
        );
    });

    if external_video {
        ui.horizontal_wrapped(|ui| {
            ui.small("review video");
            ui.small(video_path.display().to_string());
            ui.small("candidate stepping uses the 2fps evidence frame cache below");
        });
    }

    let frame_count = media.frame_count();
    if let Some(frame) = media.frame_for_time(ctx, *current_time_ms) {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!(
                "frame {} at {:.2}s",
                frame.frame_index,
                frame.time_ms as f32 / 1000.0
            ));
            if let Some(count) = frame_count {
                ui.small(format!("of {count}"));
            }
            ui.small(frame.path.display().to_string());
        });

        let available = ui.available_size_before_wrap();
        let image_size = egui::vec2(frame.size[0] as f32, frame.size[1] as f32);
        let scale = (available.x / image_size.x)
            .min((available.y - 12.0).max(1.0) / image_size.y)
            .min(1.0);
        let scaled_size = image_size * scale.max(0.05);
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.image((frame.texture.id(), scaled_size));
            });
    } else if let Some(error) = media.last_error() {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.colored_label(egui::Color32::from_rgb(220, 64, 64), error);
            if let Some(path) = media.frame_path_for_time(*current_time_ms) {
                ui.small(format!("expected frame: {}", path.display()));
            }
            ui.small(format!(
                "time: {:.2}s / duration: {:.2}s",
                *current_time_ms as f32 / 1000.0,
                fixture.duration_ms() as f32 / 1000.0
            ));
        });
    } else {
        ui.vertical_centered(|ui| {
            ui.add_space(24.0);
            ui.label("No frame loaded.");
            if fixture.session.media.frame_cache.is_none() {
                ui.small("The session does not define a frame cache.");
            }
        });
    }
}

fn looks_like_video_file(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(|extension| extension.to_ascii_lowercase())
            .as_deref(),
        Some("mp4" | "mov" | "m4v" | "webm")
    )
}

fn open_media_path(path: &Path) -> std::io::Result<()> {
    Command::new("open").arg(path).spawn().map(|_| ())
}
