use std::{path::PathBuf, time::Instant};

use eframe::egui;

use crate::{
    fixture::DashboardFixture,
    media::{FrameCacheState, ImageAssetCache},
    panels,
    review::ReviewState,
    timeline::{TimelineState, TimelineViewState},
};

pub struct AttentionDashboardApp {
    session_path: PathBuf,
    fixture: Option<DashboardFixture>,
    media: FrameCacheState,
    image_assets: ImageAssetCache,
    timeline: TimelineState,
    timeline_view: TimelineViewState,
    review: ReviewState,
    selected_candidate_id: Option<String>,
    selected_saved_state_id: Option<String>,
    current_time_ms: u64,
    is_playing: bool,
    last_play_tick: Option<Instant>,
    last_error: Option<String>,
}

impl AttentionDashboardApp {
    pub fn new(_cc: &eframe::CreationContext<'_>, session_path: PathBuf) -> Self {
        let mut app = Self {
            session_path,
            fixture: None,
            media: FrameCacheState::default(),
            image_assets: ImageAssetCache::new(),
            timeline: TimelineState::default(),
            timeline_view: TimelineViewState::default(),
            review: ReviewState::default(),
            selected_candidate_id: None,
            selected_saved_state_id: None,
            current_time_ms: 0,
            is_playing: false,
            last_play_tick: None,
            last_error: None,
        };
        app.load_session();
        app
    }

    fn load_session(&mut self) {
        match DashboardFixture::load(&self.session_path) {
            Ok(fixture) => {
                self.current_time_ms = 0;
                self.is_playing = false;
                self.last_play_tick = None;
                self.selected_candidate_id = None;
                self.selected_saved_state_id = None;
                self.timeline = TimelineState::from_fixture(&fixture);
                self.review = ReviewState::new(fixture.review_labels_path());
                self.media = FrameCacheState::new(&fixture);
                self.image_assets = ImageAssetCache::new();
                self.fixture = Some(fixture);
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(error.to_string());
            }
        }
    }

    fn tick_playback(&mut self, ctx: &egui::Context) {
        self.clamp_current_time();

        if self.is_playing {
            let now = Instant::now();
            let delta_ms = self
                .last_play_tick
                .map(|last| now.saturating_duration_since(last).as_millis() as u64)
                .unwrap_or(0);
            self.last_play_tick = Some(now);

            self.current_time_ms = self.current_time_ms.saturating_add(delta_ms).min(
                self.fixture
                    .as_ref()
                    .map_or(0, |fixture| fixture.duration_ms()),
            );

            if self
                .fixture
                .as_ref()
                .is_some_and(|fixture| self.current_time_ms >= fixture.duration_ms())
            {
                self.is_playing = false;
                self.last_play_tick = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        } else {
            self.last_play_tick = None;
        }
    }

    fn clamp_current_time(&mut self) {
        if let Some(fixture) = &self.fixture {
            self.current_time_ms = self.current_time_ms.min(fixture.duration_ms());
        }
    }

    fn render_top(&mut self, root_ui: &mut egui::Ui) {
        egui::Panel::top("attention_dashboard_top").show_inside(root_ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("1Context Attention Dashboard");
                ui.separator();
                ui.label(self.session_path.display().to_string());
                if ui.button("Reload").clicked() {
                    self.load_session();
                }
                if ui.button("Stop").clicked() {
                    self.is_playing = false;
                    self.current_time_ms = 0;
                    self.last_play_tick = None;
                }
                if let Some(error) = &self.last_error {
                    ui.colored_label(egui::Color32::from_rgb(220, 64, 64), error);
                }
            });
            if let Some(fixture) = &self.fixture {
                ui.horizontal_wrapped(|ui| {
                    for line in fixture.metadata_lines() {
                        ui.small(line);
                    }
                });
                let missing_assets: Vec<_> = fixture
                    .asset_status_lines()
                    .into_iter()
                    .filter(|(exists, _)| !exists)
                    .map(|(_, line)| line)
                    .collect();
                if !missing_assets.is_empty() {
                    ui.collapsing("Missing assets", |ui| {
                        for line in missing_assets {
                            ui.colored_label(egui::Color32::from_rgb(220, 64, 64), line);
                        }
                    });
                }
            }
        });
    }

    fn render_timeline(&mut self, root_ui: &mut egui::Ui) {
        egui::Panel::bottom("attention_dashboard_timeline")
            .resizable(true)
            .default_size(220.0)
            .show_inside(root_ui, |ui| {
                panels::timeline_panel(
                    ui,
                    &self.timeline,
                    &mut self.timeline_view,
                    &mut self.current_time_ms,
                    &mut self.selected_candidate_id,
                    &mut self.selected_saved_state_id,
                );
            });
    }

    fn render_inspector(&mut self, root_ui: &mut egui::Ui) {
        egui::Panel::right("attention_dashboard_inspector")
            .resizable(true)
            .default_size(430.0)
            .show_inside(root_ui, |ui| {
                if let Some(fixture) = &self.fixture {
                    let ctx = ui.ctx().clone();
                    panels::decision_panel(
                        ui,
                        &ctx,
                        fixture,
                        &mut self.image_assets,
                        self.current_time_ms,
                        self.selected_candidate_id.as_deref(),
                        self.selected_saved_state_id.as_deref(),
                        &mut self.review,
                    );
                } else {
                    ui.label("No fixture loaded.");
                }
            });
    }

    fn render_video(&mut self, root_ui: &mut egui::Ui) {
        egui::CentralPanel::default().show_inside(root_ui, |ui| {
            if let Some(fixture) = &self.fixture {
                let ctx = ui.ctx().clone();
                panels::video_panel(
                    ui,
                    &ctx,
                    fixture,
                    &mut self.media,
                    &mut self.current_time_ms,
                    &mut self.is_playing,
                );
            } else {
                ui.centered_and_justified(|ui| {
                    ui.label("Load a dashboard session to begin.");
                });
            }
        });
    }
}

impl eframe::App for AttentionDashboardApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tick_playback(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.render_top(ui);
        self.render_timeline(ui);
        self.render_inspector(ui);
        self.render_video(ui);
    }
}
