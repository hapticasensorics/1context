use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
    path::PathBuf,
    process::{Command, Output, Stdio},
    sync::mpsc::{self, Receiver, TryRecvError},
    thread,
    time::{Duration, Instant},
};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use eframe::egui::{
    self, Color32, ColorImage, FontId, Label, Pos2, Rect, RichText, Stroke, StrokeKind,
    TextureHandle, TextureOptions, Vec2,
};
use onecontext_capture_core::{
    list_bundles, BundleDirectoryClass, BundleEntry, RetentionPolicy, SweepActionKind,
};
use serde::{Deserialize, Deserializer};
use serde_json::Value;
use xcap::{
    image::{imageops, RgbaImage},
    Monitor, Window,
};

const DEFAULT_SNAPSHOT_REFRESH_SECONDS: f32 = 5.0;
const DEFAULT_PREVIEW_REFRESH_SECONDS: f32 = 3.0;
const DEFAULT_PREVIEW_MAX_DIMENSION: u32 = 960;
const DEFAULT_OVERLAY_LIMIT: usize = 12;
const EVENT_REFRESH_SECONDS: f32 = 1.0;
const EVENT_MAX_FILES: usize = 3;
const EVENT_MAX_BYTES_PER_FILE: u64 = 256 * 1024;
const EVENT_MAX_LINES: usize = 800;
const EVENT_PANEL_LIMIT: usize = 16;
const TIMELINE_PANEL_LIMIT: usize = 32;
const CLI_SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(6);
const CLI_STATUS_TIMEOUT: Duration = Duration::from_secs(3);
const CAPTURE_JOB_TIMEOUT: Duration = Duration::from_secs(15);
const LIVE_EVENT_JOB_TIMEOUT: Duration = Duration::from_secs(8);
const BUNDLE_REFRESH_SECONDS: f32 = 5.0;
const BUNDLE_JOB_TIMEOUT: Duration = Duration::from_secs(8);
const RECENT_READY_BUNDLE_LIMIT: usize = 6;
const DASHBOARD_APP_NAMES: &[&str] =
    &["onecontext-capture-dashboard", "1Context Capture Dashboard"];

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1280.0, 820.0])
            .with_min_inner_size([920.0, 620.0])
            .with_title("1Context Capture Dashboard"),
        ..Default::default()
    };

    eframe::run_native(
        "1Context Capture Dashboard",
        options,
        Box::new(|cc| Ok(Box::new(CaptureDashboardApp::new(cc)))),
    )
}

struct CaptureDashboardApp {
    cli_path: PathBuf,
    snapshot: Option<CaptureSnapshot>,
    preview: Option<TextureHandle>,
    preview_size: [usize; 2],
    preview_source: String,
    preview_space: PreviewCoordinateSpace,
    preview_target: PreviewTarget,
    last_snapshot_refresh: Option<Instant>,
    last_preview_refresh: Option<Instant>,
    snapshot_interval: f32,
    preview_interval: f32,
    preview_max_dimension: u32,
    auto_refresh: bool,
    show_overlays: bool,
    show_overlay_labels: bool,
    overlay_limit: usize,
    live_events: LiveEventState,
    bundle_inventory: BundleInventoryState,
    last_event_refresh: Option<Instant>,
    last_bundle_refresh: Option<Instant>,
    pending_capture_refresh: Option<PendingCaptureRefresh>,
    pending_live_event_refresh: Option<PendingLiveEventRefresh>,
    pending_bundle_refresh: Option<PendingBundleRefresh>,
    last_error: Option<String>,
    last_timing: Option<RefreshTiming>,
}

type DashboardResult<T> = std::result::Result<T, String>;

struct PendingCaptureRefresh {
    kind: CaptureRefreshKind,
    started_at: Instant,
    receiver: Receiver<CaptureRefreshOutput>,
}

struct PendingLiveEventRefresh {
    started_at: Instant,
    receiver: Receiver<DashboardResult<LiveEventState>>,
}

struct PendingBundleRefresh {
    started_at: Instant,
    receiver: Receiver<DashboardResult<BundleInventoryState>>,
}

#[derive(Debug, Clone, Copy)]
enum CaptureRefreshKind {
    SnapshotAndPreview,
    PreviewOnly,
}

impl CaptureRefreshKind {
    fn label(self) -> &'static str {
        match self {
            Self::SnapshotAndPreview => "snapshot refresh",
            Self::PreviewOnly => "preview refresh",
        }
    }
}

enum CaptureRefreshOutput {
    SnapshotAndPreview(DashboardResult<SnapshotPreviewRefresh>),
    PreviewOnly(DashboardResult<PreviewOnlyRefresh>),
}

struct SnapshotPreviewRefresh {
    snapshot: CaptureSnapshot,
    preview: DashboardResult<PreviewFrame>,
    snapshot_ms: u128,
    total_ms: u128,
}

struct PreviewOnlyRefresh {
    preview: PreviewFrame,
    total_ms: u128,
}

impl CaptureDashboardApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let mut app = Self {
            cli_path: resolve_cli_path(),
            snapshot: None,
            preview: None,
            preview_size: [1, 1],
            preview_source: "not captured yet".to_string(),
            preview_space: PreviewCoordinateSpace::Display,
            preview_target: PreviewTarget::Auto,
            last_snapshot_refresh: None,
            last_preview_refresh: None,
            snapshot_interval: DEFAULT_SNAPSHOT_REFRESH_SECONDS,
            preview_interval: DEFAULT_PREVIEW_REFRESH_SECONDS,
            preview_max_dimension: DEFAULT_PREVIEW_MAX_DIMENSION,
            auto_refresh: true,
            show_overlays: true,
            show_overlay_labels: false,
            overlay_limit: DEFAULT_OVERLAY_LIMIT,
            live_events: LiveEventState::default(),
            bundle_inventory: BundleInventoryState::default(),
            last_event_refresh: None,
            last_bundle_refresh: None,
            pending_capture_refresh: None,
            pending_live_event_refresh: None,
            pending_bundle_refresh: None,
            last_error: None,
            last_timing: None,
        };
        app.refresh_snapshot_and_preview(&cc.egui_ctx);
        app.refresh_live_events(&cc.egui_ctx);
        app.refresh_bundle_inventory(&cc.egui_ctx);
        app
    }

    fn refresh_snapshot_and_preview(&mut self, ctx: &egui::Context) {
        if self.pending_capture_refresh.is_some() {
            return;
        }

        self.preview_source = "refreshing capture snapshot...".to_string();
        let cli_path = self.cli_path.clone();
        let preview_target = self.preview_target;
        let preview_max_dimension = self.preview_max_dimension;
        let (sender, receiver) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        let spawn_result = thread::Builder::new()
            .name("capture-dashboard-snapshot-refresh".to_string())
            .spawn(move || {
                let result = load_snapshot_timed(&cli_path)
                    .map(|(snapshot, snapshot_ms)| {
                        let preview_started = Instant::now();
                        let preview =
                            capture_preview(&snapshot, preview_target, preview_max_dimension)
                                .map(|mut frame| {
                                    frame.timing.preview_total_ms =
                                        Some(preview_started.elapsed().as_millis());
                                    frame
                                })
                                .map_err(|error| error.to_string());
                        SnapshotPreviewRefresh {
                            snapshot,
                            preview,
                            snapshot_ms,
                            total_ms: preview_started.elapsed().as_millis() + snapshot_ms,
                        }
                    })
                    .map_err(|error| error.to_string());
                let _ = sender.send(CaptureRefreshOutput::SnapshotAndPreview(result));
                repaint_ctx.request_repaint();
            });

        match spawn_result {
            Ok(_) => {
                self.pending_capture_refresh = Some(PendingCaptureRefresh {
                    kind: CaptureRefreshKind::SnapshotAndPreview,
                    started_at: Instant::now(),
                    receiver,
                });
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(error) => {
                self.last_error = Some(format!("start capture refresh failed: {error}"));
            }
        }
    }

    fn refresh_live_events(&mut self, ctx: &egui::Context) {
        if self.pending_live_event_refresh.is_some() {
            return;
        }

        let cli_path = self.cli_path.clone();
        let existing_events_directory = self.live_events.events_directory.clone();
        let (sender, receiver) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        let spawn_result = thread::Builder::new()
            .name("capture-dashboard-live-events-refresh".to_string())
            .spawn(move || {
                let started = Instant::now();
                let result = refresh_live_events(&cli_path, existing_events_directory.as_deref())
                    .map(|mut snapshot| {
                        snapshot.refresh_ms = Some(started.elapsed().as_millis());
                        snapshot
                    })
                    .map_err(|error| error.to_string());
                let _ = sender.send(result);
                repaint_ctx.request_repaint();
            });

        match spawn_result {
            Ok(_) => {
                self.pending_live_event_refresh = Some(PendingLiveEventRefresh {
                    started_at: Instant::now(),
                    receiver,
                });
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(error) => {
                self.live_events.error = Some(format!("start event refresh failed: {error}"));
            }
        }
    }

    fn refresh_bundle_inventory(&mut self, ctx: &egui::Context) {
        if self.pending_bundle_refresh.is_some() {
            return;
        }

        let cli_path = self.cli_path.clone();
        let root_hint = self.capture_root_hint();
        let events_hint = self.capture_events_hint();
        let (sender, receiver) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        let spawn_result = thread::Builder::new()
            .name("capture-dashboard-bundle-refresh".to_string())
            .spawn(move || {
                let started = Instant::now();
                let result =
                    load_bundle_inventory(&cli_path, root_hint.as_deref(), events_hint.as_deref())
                        .map(|mut inventory| {
                            inventory.refresh_ms = Some(started.elapsed().as_millis());
                            inventory
                        })
                        .map_err(|error| error.to_string());
                let _ = sender.send(result);
                repaint_ctx.request_repaint();
            });

        match spawn_result {
            Ok(_) => {
                self.pending_bundle_refresh = Some(PendingBundleRefresh {
                    started_at: Instant::now(),
                    receiver,
                });
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(error) => {
                self.bundle_inventory.error = Some(format!("start bundle refresh failed: {error}"));
            }
        }
    }

    fn refresh_preview(&mut self, ctx: &egui::Context) {
        if self.preview_target.requires_fresh_snapshot_for_preview() {
            self.refresh_snapshot_and_preview(ctx);
            return;
        }
        self.refresh_preview_from_current_snapshot(ctx);
    }

    fn refresh_preview_from_current_snapshot(&mut self, ctx: &egui::Context) {
        if self.pending_capture_refresh.is_some() {
            return;
        }

        let Some(snapshot) = self.snapshot.clone() else {
            self.last_error = Some("no capture snapshot available for preview".to_string());
            self.last_preview_refresh = Some(Instant::now());
            return;
        };

        self.preview_source = "refreshing preview...".to_string();
        let preview_target = self.preview_target;
        let preview_max_dimension = self.preview_max_dimension;
        let (sender, receiver) = mpsc::channel();
        let repaint_ctx = ctx.clone();
        let spawn_result = thread::Builder::new()
            .name("capture-dashboard-preview-refresh".to_string())
            .spawn(move || {
                let started = Instant::now();
                let result = capture_preview(&snapshot, preview_target, preview_max_dimension)
                    .map(|mut frame| {
                        let total_ms = started.elapsed().as_millis();
                        frame.timing.preview_total_ms = Some(total_ms);
                        PreviewOnlyRefresh {
                            preview: frame,
                            total_ms,
                        }
                    })
                    .map_err(|error| error.to_string());
                let _ = sender.send(CaptureRefreshOutput::PreviewOnly(result));
                repaint_ctx.request_repaint();
            });

        match spawn_result {
            Ok(_) => {
                self.pending_capture_refresh = Some(PendingCaptureRefresh {
                    kind: CaptureRefreshKind::PreviewOnly,
                    started_at: Instant::now(),
                    receiver,
                });
                ctx.request_repaint_after(Duration::from_millis(100));
            }
            Err(error) => {
                self.last_error = Some(format!("start preview refresh failed: {error}"));
            }
        }
    }

    fn apply_preview(&mut self, ctx: &egui::Context, preview: PreviewFrame) -> u128 {
        self.preview_size = preview.size;
        self.preview_source = preview.source;
        self.preview_space = preview.space;
        let started = Instant::now();
        if let Some(texture) = self.preview.as_mut() {
            texture.set(preview.image, TextureOptions::LINEAR);
        } else {
            self.preview =
                Some(ctx.load_texture("live-preview", preview.image, TextureOptions::LINEAR));
        }
        started.elapsed().as_millis()
    }

    fn poll_refresh_jobs(&mut self, ctx: &egui::Context) {
        self.poll_capture_refresh(ctx);
        self.poll_live_event_refresh();
        self.poll_bundle_refresh();
    }

    fn poll_capture_refresh(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending_capture_refresh.take() else {
            return;
        };

        match pending.receiver.try_recv() {
            Ok(output) => {
                self.handle_capture_refresh_output(ctx, output);
            }
            Err(TryRecvError::Empty) => {
                if pending.started_at.elapsed() >= CAPTURE_JOB_TIMEOUT {
                    let kind = pending.kind.label();
                    self.last_error = Some(format!(
                        "{kind} is taking longer than {:.0}s; auto refresh paused so the dashboard stays responsive",
                        CAPTURE_JOB_TIMEOUT.as_secs_f32()
                    ));
                    self.auto_refresh = false;
                    let now = Instant::now();
                    self.last_snapshot_refresh = Some(now);
                    self.last_preview_refresh = Some(now);
                } else {
                    self.pending_capture_refresh = Some(pending);
                    ctx.request_repaint_after(Duration::from_millis(250));
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.last_error =
                    Some("capture refresh worker exited before reporting".to_string());
            }
        }
    }

    fn handle_capture_refresh_output(&mut self, ctx: &egui::Context, output: CaptureRefreshOutput) {
        match output {
            CaptureRefreshOutput::SnapshotAndPreview(result) => {
                self.handle_snapshot_preview_refresh(ctx, result);
            }
            CaptureRefreshOutput::PreviewOnly(result) => {
                self.handle_preview_refresh(ctx, result);
            }
        }
    }

    fn handle_snapshot_preview_refresh(
        &mut self,
        ctx: &egui::Context,
        result: DashboardResult<SnapshotPreviewRefresh>,
    ) {
        let now = Instant::now();
        self.last_snapshot_refresh = Some(now);
        self.last_preview_refresh = Some(now);
        match result {
            Ok(refresh) => {
                if let Some(status) = refresh.snapshot.capture_status.clone() {
                    self.live_events.apply_capture_status(status);
                }
                self.snapshot = Some(refresh.snapshot);
                match refresh.preview {
                    Ok(preview) => {
                        let mut timing = preview.timing.clone();
                        let texture_ms = self.apply_preview(ctx, preview);
                        timing.snapshot_ms = Some(refresh.snapshot_ms);
                        timing.texture_upload_ms = Some(texture_ms);
                        timing.total_ms = Some(refresh.total_ms + texture_ms);
                        self.last_timing = Some(timing);
                        self.last_error = None;
                    }
                    Err(error) => {
                        if is_timeout_error(&error) {
                            self.auto_refresh = false;
                        }
                        self.last_timing = Some(RefreshTiming {
                            snapshot_ms: Some(refresh.snapshot_ms),
                            total_ms: Some(refresh.total_ms),
                            ..RefreshTiming::default()
                        });
                        self.last_error = Some(error);
                    }
                }
            }
            Err(error) => {
                if should_pause_auto_refresh_after_capture_error(&error) {
                    self.auto_refresh = false;
                }
                let now = Instant::now();
                self.last_snapshot_refresh = Some(now);
                self.last_preview_refresh = Some(now);
                self.last_error = Some(error);
            }
        }
    }

    fn handle_preview_refresh(
        &mut self,
        ctx: &egui::Context,
        result: DashboardResult<PreviewOnlyRefresh>,
    ) {
        self.last_preview_refresh = Some(Instant::now());
        match result {
            Ok(refresh) => {
                let mut timing = refresh.preview.timing.clone();
                let texture_ms = self.apply_preview(ctx, refresh.preview);
                timing.texture_upload_ms = Some(texture_ms);
                timing.total_ms = Some(refresh.total_ms + texture_ms);
                self.last_timing = Some(timing);
                self.last_error = None;
            }
            Err(error) => {
                if is_timeout_error(&error) {
                    self.auto_refresh = false;
                }
                self.last_error = Some(error);
            }
        }
    }

    fn poll_live_event_refresh(&mut self) {
        let Some(pending) = self.pending_live_event_refresh.take() else {
            return;
        };

        match pending.receiver.try_recv() {
            Ok(result) => {
                self.last_event_refresh = Some(Instant::now());
                match result {
                    Ok(snapshot) => {
                        self.live_events = snapshot;
                    }
                    Err(error) => {
                        if is_timeout_error(&error) {
                            self.auto_refresh = false;
                        }
                        self.live_events.error = Some(error);
                        self.live_events.refresh_ms =
                            Some(pending.started_at.elapsed().as_millis());
                    }
                }
            }
            Err(TryRecvError::Empty) => {
                if pending.started_at.elapsed() >= LIVE_EVENT_JOB_TIMEOUT {
                    self.last_event_refresh = Some(Instant::now());
                    self.live_events.error = Some(format!(
                        "event refresh timed out after {:.0}s",
                        LIVE_EVENT_JOB_TIMEOUT.as_secs_f32()
                    ));
                    self.live_events.refresh_ms = Some(pending.started_at.elapsed().as_millis());
                    self.auto_refresh = false;
                } else {
                    self.pending_live_event_refresh = Some(pending);
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.last_event_refresh = Some(Instant::now());
                self.live_events.error =
                    Some("event refresh worker exited before reporting".to_string());
            }
        }
    }

    fn poll_bundle_refresh(&mut self) {
        let Some(pending) = self.pending_bundle_refresh.take() else {
            return;
        };

        match pending.receiver.try_recv() {
            Ok(result) => {
                self.last_bundle_refresh = Some(Instant::now());
                match result {
                    Ok(inventory) => {
                        self.bundle_inventory = inventory;
                    }
                    Err(error) => {
                        self.bundle_inventory.error = Some(error);
                        self.bundle_inventory.refresh_ms =
                            Some(pending.started_at.elapsed().as_millis());
                    }
                }
            }
            Err(TryRecvError::Empty) => {
                if pending.started_at.elapsed() >= BUNDLE_JOB_TIMEOUT {
                    self.last_bundle_refresh = Some(Instant::now());
                    self.bundle_inventory.error = Some(format!(
                        "bundle refresh timed out after {:.0}s",
                        BUNDLE_JOB_TIMEOUT.as_secs_f32()
                    ));
                    self.bundle_inventory.refresh_ms =
                        Some(pending.started_at.elapsed().as_millis());
                } else {
                    self.pending_bundle_refresh = Some(pending);
                }
            }
            Err(TryRecvError::Disconnected) => {
                self.last_bundle_refresh = Some(Instant::now());
                self.bundle_inventory.error =
                    Some("bundle refresh worker exited before reporting".to_string());
            }
        }
    }

    fn capture_root_hint(&self) -> Option<PathBuf> {
        self.live_events
            .root_directory
            .clone()
            .or_else(|| {
                self.live_events
                    .capture_status
                    .as_ref()
                    .and_then(|status| status.root_directory.clone())
            })
            .or_else(|| {
                self.snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.capture_status.as_ref())
                    .and_then(|status| status.root_directory.clone())
            })
    }

    fn capture_events_hint(&self) -> Option<PathBuf> {
        self.live_events
            .events_directory
            .clone()
            .or_else(|| {
                self.live_events
                    .capture_status
                    .as_ref()
                    .and_then(|status| status.events_directory.clone())
            })
            .or_else(|| {
                self.snapshot
                    .as_ref()
                    .and_then(|snapshot| snapshot.capture_status.as_ref())
                    .and_then(|status| status.events_directory.clone())
            })
    }

    fn pending_status(&self) -> Option<String> {
        let mut statuses = Vec::new();
        if let Some(pending) = &self.pending_capture_refresh {
            statuses.push(format!(
                "{} {:.1}s",
                pending.kind.label(),
                pending.started_at.elapsed().as_secs_f32()
            ));
        }
        if let Some(pending) = &self.pending_live_event_refresh {
            statuses.push(format!(
                "events {:.1}s",
                pending.started_at.elapsed().as_secs_f32()
            ));
        }
        if let Some(pending) = &self.pending_bundle_refresh {
            statuses.push(format!(
                "bundles {:.1}s",
                pending.started_at.elapsed().as_secs_f32()
            ));
        }
        if statuses.is_empty() {
            None
        } else {
            Some(statuses.join(" | "))
        }
    }

    fn maybe_refresh(&mut self, ctx: &egui::Context) {
        self.poll_refresh_jobs(ctx);
        if !self.auto_refresh {
            return;
        }
        let events_due = self
            .last_event_refresh
            .map(|last| last.elapsed() >= Duration::from_secs_f32(EVENT_REFRESH_SECONDS))
            .unwrap_or(true);
        if events_due {
            self.refresh_live_events(ctx);
        }
        let bundles_due = self
            .last_bundle_refresh
            .map(|last| last.elapsed() >= Duration::from_secs_f32(BUNDLE_REFRESH_SECONDS))
            .unwrap_or(true);
        if bundles_due {
            self.refresh_bundle_inventory(ctx);
        }
        let snapshot_due = self
            .last_snapshot_refresh
            .map(|last| last.elapsed() >= Duration::from_secs_f32(self.snapshot_interval))
            .unwrap_or(true);
        if snapshot_due {
            self.refresh_snapshot_and_preview(ctx);
            ctx.request_repaint_after(self.next_refresh_delay());
            return;
        }
        let preview_due = self
            .last_preview_refresh
            .map(|last| last.elapsed() >= Duration::from_secs_f32(self.preview_interval))
            .unwrap_or(true);
        if preview_due {
            self.refresh_preview(ctx);
            ctx.request_repaint_after(self.next_refresh_delay());
            return;
        }
        ctx.request_repaint_after(self.next_refresh_delay());
    }

    fn next_refresh_delay(&self) -> Duration {
        let snapshot_remaining =
            remaining_seconds(self.last_snapshot_refresh, self.snapshot_interval);
        let preview_remaining = remaining_seconds(self.last_preview_refresh, self.preview_interval);
        let event_remaining = remaining_seconds(self.last_event_refresh, EVENT_REFRESH_SECONDS);
        let bundle_remaining = remaining_seconds(self.last_bundle_refresh, BUNDLE_REFRESH_SECONDS);
        Duration::from_secs_f32(
            snapshot_remaining
                .min(preview_remaining)
                .min(event_remaining)
                .min(bundle_remaining)
                .max(0.25),
        )
    }
}

fn remaining_seconds(last_refresh: Option<Instant>, interval: f32) -> f32 {
    last_refresh
        .map(|last| interval - last.elapsed().as_secs_f32())
        .unwrap_or(0.0)
}

impl eframe::App for CaptureDashboardApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.maybe_refresh(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("1Context Capture Dashboard");
                ui.separator();
                let previous_target = self.preview_target;
                egui::ComboBox::from_label("Target")
                    .selected_text(self.preview_target.label())
                    .show_ui(ui, |ui| {
                        ui.selectable_value(
                            &mut self.preview_target,
                            PreviewTarget::Auto,
                            PreviewTarget::Auto.label(),
                        );
                        ui.selectable_value(
                            &mut self.preview_target,
                            PreviewTarget::MainDisplay,
                            PreviewTarget::MainDisplay.label(),
                        );
                        ui.selectable_value(
                            &mut self.preview_target,
                            PreviewTarget::FocusedWindow,
                            PreviewTarget::FocusedWindow.label(),
                        );
                        ui.selectable_value(
                            &mut self.preview_target,
                            PreviewTarget::TopWindow,
                            PreviewTarget::TopWindow.label(),
                        );
                    });
                if self.preview_target != previous_target {
                    self.refresh_preview(ui.ctx());
                }
                ui.checkbox(&mut self.show_overlays, "Overlays");
                ui.checkbox(&mut self.show_overlay_labels, "Labels");
                ui.add(
                    egui::DragValue::new(&mut self.overlay_limit)
                        .range(0..=40)
                        .prefix("Rects "),
                );
                ui.add(
                    egui::Slider::new(&mut self.snapshot_interval, 2.0..=30.0)
                        .suffix("s")
                        .text("Metadata"),
                );
                ui.add(
                    egui::Slider::new(&mut self.preview_interval, 1.0..=15.0)
                        .suffix("s")
                        .text("Preview"),
                );
                ui.add(
                    egui::Slider::new(&mut self.preview_max_dimension, 480..=1920)
                        .suffix("px")
                        .text("Pixels"),
                );
                ui.checkbox(&mut self.auto_refresh, "Live");
                if ui.button("Capture Now").clicked() {
                    self.refresh_snapshot_and_preview(ui.ctx());
                }
                if ui.button("Preview Now").clicked() {
                    self.refresh_preview(ui.ctx());
                }
                if ui.button("Events Now").clicked() {
                    self.refresh_live_events(ui.ctx());
                }
                if ui.button("Bundles Now").clicked() {
                    self.refresh_bundle_inventory(ui.ctx());
                }
            });
        });

        egui::Panel::right("metadata")
            .resizable(true)
            .default_size(360.0)
            .min_size(300.0)
            .max_size(480.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        render_metadata_panel(
                            ui,
                            self.snapshot.as_ref(),
                            &self.preview_source,
                            self.preview_target,
                            self.preview_space,
                            self.pending_status().as_deref(),
                            self.last_error.as_deref(),
                            self.last_timing.as_ref(),
                            &self.live_events,
                            &mut self.bundle_inventory,
                        );
                    });
            });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            render_preview(
                ui,
                self.snapshot.as_ref(),
                self.preview.as_ref(),
                self.preview_size,
                self.preview_space,
                self.show_overlays,
                self.show_overlay_labels,
                self.overlay_limit,
            );
        });
    }
}

fn render_metadata_panel(
    ui: &mut egui::Ui,
    snapshot: Option<&CaptureSnapshot>,
    preview_source: &str,
    preview_target: PreviewTarget,
    preview_space: PreviewCoordinateSpace,
    pending_status: Option<&str>,
    error: Option<&str>,
    timing: Option<&RefreshTiming>,
    live_events: &LiveEventState,
    bundle_inventory: &mut BundleInventoryState,
) {
    ui.label(RichText::new("Preview").strong());
    ui.label(format!("Target: {}", preview_target.label()));
    ui.label(format!("Space: {}", preview_space.label()));
    ui.add(Label::new(preview_source).wrap());
    if let Some(pending_status) = pending_status {
        ui.small(format!("Refresh: {pending_status}"));
    }
    if let Some(timing) = timing {
        render_timing(ui, timing);
    }
    if let Some(error) = error {
        ui.add_space(8.0);
        ui.add(Label::new(RichText::new(error).color(Color32::RED)).wrap());
    }

    ui.separator();
    render_live_event_panels(ui, live_events);

    ui.separator();
    render_bundle_inventory_panel(ui, bundle_inventory);

    ui.separator();
    let Some(snapshot) = snapshot else {
        if let Some(status) = live_events.capture_status.as_ref() {
            render_permission_metadata(ui, None, Some(status));
            ui.separator();
        }
        ui.label("No capture snapshot yet.");
        return;
    };

    render_permission_metadata(
        ui,
        Some(snapshot),
        snapshot
            .capture_status
            .as_ref()
            .or(live_events.capture_status.as_ref()),
    );

    ui.separator();
    ui.label(RichText::new("Snapshot").strong());
    ui.monospace(snapshot.generated_at.as_deref().unwrap_or("unknown"));
    if let Some(active) = &snapshot.active_application {
        let mut active_line = format!(
            "Active: {}",
            active.app_name.as_deref().unwrap_or("unknown")
        );
        if let Some(pid) = active.process_id {
            active_line.push_str(&format!(" pid={pid}"));
        }
        if let Some(bundle) = &active.bundle_id {
            active_line.push_str(&format!(" bundle={bundle}"));
        }
        ui.add(Label::new(active_line).wrap());
    }
    ui.label(format!("Displays: {}", snapshot.displays.len()));
    if let Some(display) = snapshot.main_display() {
        let id = display.display_id.as_deref().unwrap_or("unknown");
        let scale = display.scale_factor.unwrap_or(1.0);
        ui.label(format!("Main display: {id} @ {scale:.1}x"));
    }
    ui.label(format!("Windows: {}", snapshot.windows.len()));
    ui.label(format!(
        "Visible: {}",
        snapshot.windows.iter().filter(|w| w.is_on_screen).count()
    ));
    ui.label(format!(
        "Capture eligible: {}",
        snapshot
            .windows
            .iter()
            .filter(|w| w.capture_eligible)
            .count()
    ));

    ui.separator();
    ui.label(RichText::new("Focused").strong());
    let focused: Vec<_> = snapshot.windows.iter().filter(|w| w.is_focused).collect();
    if focused.is_empty() {
        ui.label("none");
    } else {
        for window in focused.into_iter().take(4) {
            render_window_line(ui, window);
        }
    }

    ui.separator();
    ui.label(RichText::new("Top Capture-Eligible Windows").strong());
    let mut eligible: Vec<_> = snapshot
        .windows
        .iter()
        .filter(|w| should_list_window(w))
        .collect();
    eligible.sort_by_key(|w| (w.z_rank.unwrap_or(i64::MAX), w.window_id.unwrap_or(0)));
    for window in eligible.into_iter().take(14) {
        render_window_line(ui, window);
    }
}

fn render_timing(ui: &mut egui::Ui, timing: &RefreshTiming) {
    let mut pieces = Vec::new();
    if let Some(value) = timing.snapshot_ms {
        pieces.push(format!("snapshot {value}ms"));
    }
    if let Some(value) = timing.preview_total_ms {
        pieces.push(format!("preview {value}ms"));
    }
    if let Some(value) = timing.xcap_list_ms {
        pieces.push(format!("list {value}ms"));
    }
    if let Some(value) = timing.xcap_capture_ms {
        pieces.push(format!("capture {value}ms"));
    }
    if let Some(value) = timing.resize_ms {
        pieces.push(format!("resize {value}ms"));
    }
    if let Some(value) = timing.color_image_ms {
        pieces.push(format!("image {value}ms"));
    }
    if let Some(value) = timing.texture_upload_ms {
        pieces.push(format!("texture {value}ms"));
    }
    if let Some(value) = timing.total_ms {
        pieces.push(format!("total {value}ms"));
    }
    if let (Some(input), Some(output)) = (timing.input_pixels, timing.output_pixels) {
        pieces.push(format!(
            "{}x{} -> {}x{}",
            input[0], input[1], output[0], output[1]
        ));
    }
    if !pieces.is_empty() {
        ui.small(pieces.join(" | "));
    }
}

fn render_permission_metadata(
    ui: &mut egui::Ui,
    snapshot: Option<&CaptureSnapshot>,
    status: Option<&CaptureStatus>,
) {
    ui.label(RichText::new("Permission-Derived Metadata").strong());
    let has_anything = status.is_some()
        || snapshot
            .and_then(|snapshot| snapshot.focused_context.as_ref())
            .is_some();
    if !has_anything {
        ui.small("No permission-derived metadata in the latest status/snapshot.");
        return;
    }

    if let Some(status) = status {
        render_capture_status_signals(ui, status);
        if let Some(snapshot) = snapshot {
            if let Some(window) = recent_motion_target_window(snapshot, status) {
                ui.small(format!(
                    "Recent UX target: {}",
                    compact_window_label(window)
                ));
            }
        }
    }

    if let Some(context) = snapshot.and_then(|snapshot| snapshot.focused_context.as_ref()) {
        render_focused_context(ui, context);
    }

    if let Some(snapshot) = snapshot {
        let focused_with_metadata: Vec<_> = snapshot
            .windows
            .iter()
            .filter(|window| window.is_focused && window.focus_metadata.is_some())
            .collect();
        if !focused_with_metadata.is_empty() {
            for window in focused_with_metadata.into_iter().take(3) {
                if let Some(focus) = &window.focus_metadata {
                    ui.small(format!(
                        "Window focus: {} -> {}",
                        compact_window_label(window),
                        focus.compact_label()
                    ));
                }
            }
        }
    }
}

fn render_capture_status_signals(ui: &mut egui::Ui, status: &CaptureStatus) {
    let mut status_bits = Vec::new();
    if let Some(surface) = &status.surface {
        status_bits.push(format!("surface={surface}"));
    }
    if let Some(unit) = status.window_unit_of_truth {
        status_bits.push(format!("window truth={}", yes_no(unit)));
    }
    if !status.available_methods.is_empty() {
        status_bits.push(format!("methods={}", status.available_methods.len()));
    }
    if !status_bits.is_empty() {
        ui.small(status_bits.join(" | "));
    }

    render_path_summary(ui, "Root", status.root_directory.as_ref());
    render_path_summary(ui, "Events", status.events_directory.as_ref());
    render_path_summary(ui, "Windows", status.windows_directory.as_ref());
    render_path_summary(ui, "Media", status.media_directory.as_ref());

    if let Some(metadata) = &status.permission_derived_metadata {
        render_permission_derived_metadata(ui, metadata);
    }

    if let Some(fusion) = &status.metadata_sample_fusion {
        let enabled = fusion
            .ux_motion_hints_enabled
            .map(yes_no)
            .unwrap_or("unknown");
        let pixels = fusion.pixels_untouched.map(yes_no).unwrap_or("unknown");
        ui.small(format!(
            "Fusion: ux_motion_hints={enabled} source={} pixels_untouched={pixels}",
            fusion.source.as_deref().unwrap_or("unknown")
        ));
    }

    if let Some(tap) = &status.ux_event_tap {
        render_ux_event_tap_status(ui, tap);
    }
    if let Some(hints) = capture_status_motion_hints(status) {
        render_motion_hints(ui, "Motion", hints);
    }

    if let Some(subject) = status.permission_subject.as_ref().or_else(|| {
        status
            .ux_event_tap
            .as_ref()
            .and_then(|tap| tap.permission_subject.as_ref())
    }) {
        ui.add(Label::new(format!("Subject: {}", subject.compact_label())).wrap());
    }

    if !status.capability_signals.is_empty() {
        let line = status
            .capability_signals
            .iter()
            .map(CapabilitySignal::compact_label)
            .collect::<Vec<_>>()
            .join(" | ");
        ui.add(Label::new(format!("Capabilities: {line}")).wrap());
    }
}

fn render_path_summary(ui: &mut egui::Ui, label: &str, path: Option<&PathBuf>) {
    if let Some(path) = path {
        ui.add(Label::new(format!("{label}: {}", path.display())).wrap());
    }
}

fn render_permission_derived_metadata(ui: &mut egui::Ui, metadata: &PermissionDerivedMetadata) {
    let mut header = Vec::new();
    if let Some(version) = metadata.schema_version {
        header.push(format!("schema=v{version}"));
    }
    if let Some(generated_at) = &metadata.generated_at {
        header.push(format!("at={}", compact_timestamp(generated_at)));
    }
    if !header.is_empty() {
        ui.small(format!("Permission metadata: {}", header.join(" | ")));
    }

    if let Some(privacy) = &metadata.privacy {
        ui.small(format!(
            "Privacy: raw_keys={} raw_text={} coords={} aggregates_only={}",
            yes_no(privacy.raw_keystrokes_included.unwrap_or(false)),
            yes_no(privacy.raw_text_included.unwrap_or(false)),
            yes_no(privacy.coordinates_included.unwrap_or(false)),
            yes_no(privacy.aggregates_and_counts_only.unwrap_or(true))
        ));
    }

    if !metadata.process_identities.is_empty() {
        let identities = metadata
            .process_identities
            .iter()
            .map(PermissionProcessIdentity::compact_label)
            .collect::<Vec<_>>()
            .join(" | ");
        ui.add(Label::new(format!("Identities: {identities}")).wrap());
    }

    if let Some(paths) = &metadata.capture_paths {
        let path_line = [
            paths
                .root_directory
                .as_ref()
                .map(|path| format!("root={}", compact_path(path))),
            paths
                .events_directory
                .as_ref()
                .map(|path| format!("events={}", compact_path(path))),
            paths
                .windows_directory
                .as_ref()
                .map(|path| format!("windows={}", compact_path(path))),
            paths
                .media_directory
                .as_ref()
                .map(|path| format!("media={}", compact_path(path))),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
        if !path_line.is_empty() {
            ui.add(Label::new(format!("Permission paths: {}", path_line.join(" | "))).wrap());
        }
    }

    if !metadata.signals.is_empty() {
        let line = metadata
            .signals
            .iter()
            .map(|(name, signal)| signal.compact_label(name))
            .collect::<Vec<_>>()
            .join(" | ");
        ui.add(Label::new(format!("Permission signals: {line}")).wrap());
    }
}

fn render_ux_event_tap_status(ui: &mut egui::Ui, tap: &CaptureUXEventTapStatus) {
    let active = tap.tap_active.map(yes_no).unwrap_or("unknown");
    let owner = tap
        .tap_owner_pid
        .map(|pid| format!("pid={pid}"))
        .unwrap_or_else(|| "pid=?".to_string());
    let exe = tap
        .tap_owner_executable
        .as_deref()
        .and_then(|path| Path::new(path).file_name().and_then(|name| name.to_str()))
        .or(tap.tap_owner_executable.as_deref())
        .unwrap_or("unknown");
    ui.add(
        Label::new(format!(
            "Input Monitoring: tap={active} lifecycle={} owner={owner} {exe} observed={} callbacks={} queue={} dropped={}",
            tap.lifecycle_state.as_deref().unwrap_or("unknown"),
            display_count(tap.observed_event_count),
            display_count(tap.callback_count),
            display_count(tap.queue_depth),
            display_count(tap.dropped_count)
        ))
        .wrap(),
    );
    let mut details = Vec::new();
    if let Some(event_tap) = &tap.event_tap {
        details.push(format!("event_tap={event_tap}"));
    }
    if let Some(options) = &tap.tap_options {
        details.push(format!("options={options}"));
    }
    if let Some(startup_wired) = tap.startup_wired {
        details.push(format!("startup_wired={}", yes_no(startup_wired)));
    }
    if let Some(bundle) = &tap.tap_owner_bundle {
        details.push(format!("bundle={bundle}"));
    }
    if !tap.event_mask.is_empty() {
        details.push(format!("mask={}", tap.event_mask.len()));
    }
    if let Some(last_event) = &tap.last_event_at {
        details.push(format!("last={}", compact_timestamp(last_event)));
    }
    if let Some(disabled) = tap.disabled_count {
        details.push(format!("disabled={disabled}"));
    }
    if let Some(attempts) = tap.reenable_attempt_count {
        details.push(format!("reenable={attempts}"));
    }
    if let Some(successes) = tap.reenable_success_count {
        details.push(format!("reenable_ok={successes}"));
    }
    if let Some(failures) = tap.reenable_failure_count {
        details.push(format!("reenable_fail={failures}"));
    }
    if let Some(coalesced) = tap.coalesced_count {
        details.push(format!("coalesced={coalesced}"));
    }
    if let Some(last_us) = tap.callback_last_microseconds {
        details.push(format!("callback_last={last_us:.1}us"));
    }
    if let Some(avg_us) = tap.callback_average_microseconds {
        details.push(format!("callback_avg={avg_us:.1}us"));
    }
    if let Some(max_us) = tap.callback_max_microseconds {
        details.push(format!("callback_max={max_us:.1}us"));
    }
    if let Some(error) = &tap.last_error {
        details.push(format!("error={error}"));
    } else if let Some(error) = &tap.startup_error {
        details.push(format!("startup_error={error}"));
    }
    if !details.is_empty() {
        ui.small(details.join(" | "));
    }
    if let Some(note) = &tap.note {
        ui.add(Label::new(format!("Tap note: {note}")).wrap());
    }
    if let Some(note) = &tap.tcc_identity_note {
        ui.add(Label::new(format!("TCC: {note}")).wrap());
    }
}

fn render_motion_hints(ui: &mut egui::Ui, label: &str, hints: &CaptureMotionHints) {
    let mut bits = Vec::new();
    bits.push(format!(
        "scroll={}",
        hints.scroll_event_recently.map(yes_no).unwrap_or("unknown")
    ));
    bits.push(format!(
        "keys={}",
        hints
            .keyboard_activity_recently
            .map(yes_no)
            .unwrap_or("unknown")
    ));
    bits.push(format!(
        "focus={}",
        hints.focused_recently.map(yes_no).unwrap_or("unknown")
    ));
    if let Some(pid) = hints.recent_target_process_id {
        bits.push(format!("target_pid={pid}"));
    }
    if let Some(dy) = hints.estimated_scroll_dy {
        bits.push(format!("dy={dy:.2}"));
    }
    if let Some(generated_at) = &hints.generated_at {
        bits.push(format!("at={}", compact_timestamp(generated_at)));
    }
    ui.small(format!("{label}: {}", bits.join(" | ")));
}

fn render_focused_context(ui: &mut egui::Ui, context: &CaptureFocusedContext) {
    let trusted = context.is_process_trusted.map(yes_no).unwrap_or("unknown");
    let mut line = format!(
        "Accessibility: trusted={trusted} status={}",
        context.status.as_deref().unwrap_or("unknown")
    );
    if let Some(pid) = context.focused_application_process_id {
        line.push_str(&format!(" focused_pid={pid}"));
    }
    if let Some(id) = context.matched_window_id {
        line.push_str(&format!(" matched_window=#{id}"));
    }
    if let Some(active) = &context.active_application {
        if let Some(app) = &active.app_name {
            line.push_str(&format!(" app={app}"));
        }
    }
    ui.add(Label::new(line).wrap());
    if let Some(element) = &context.focused_element {
        ui.small(format!("AX element: {}", element.compact_label()));
    }
    if let Some(window) = &context.focused_window {
        ui.small(format!("AX window: {}", window.compact_label()));
    }
    if !context.issues.is_empty() {
        let issue_line = context
            .issues
            .iter()
            .take(3)
            .map(CaptureFocusedContextIssue::compact_label)
            .collect::<Vec<_>>()
            .join(" | ");
        ui.add(Label::new(format!("AX issues: {issue_line}")).wrap());
    }
}

fn render_live_event_panels(ui: &mut egui::Ui, live_events: &LiveEventState) {
    ui.label(RichText::new("Capture Events").strong());
    if let Some(dir) = &live_events.events_directory {
        ui.add(Label::new(dir.display().to_string()).wrap());
    } else {
        ui.small("Events directory: resolving from capture.status");
    }
    let mut status = vec![
        format!("poll {:.1}s", EVENT_REFRESH_SECONDS),
        format!("{} lines", live_events.scanned_lines),
        format!("{} files", live_events.source_files.len()),
    ];
    if let Some(ms) = live_events.refresh_ms {
        status.push(format!("{ms}ms"));
    }
    ui.small(status.join(" | "));
    if !live_events.source_files.is_empty() {
        ui.small(live_event_tail_coverage_label(
            &live_events.source_files,
            live_events.scanned_lines,
        ));
    }
    if let Some(fusion) = &live_events.metadata_sample_fusion {
        ui.small(fusion.summary_label());
    }
    if let Some(error) = &live_events.error {
        ui.add(Label::new(RichText::new(error).color(Color32::RED)).wrap());
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Aligned Timeline").strong());
    ui.small(
        "Merged by parsed event_time_start/event_time_end with legacy recordedAt/capturedAt fallbacks.",
    );
    if live_events.timeline.is_empty() {
        ui.small("No timeline events in recent JSONL yet.");
    } else {
        for event in live_events.timeline.iter().take(TIMELINE_PANEL_LIMIT) {
            render_timeline_event_line(ui, event);
        }
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Recent UX Anchors").strong());
    if live_events.recent_ux.is_empty() {
        ui.small("No UX anchor events in recent JSONL yet.");
    } else {
        for event in live_events.recent_ux.iter().take(EVENT_PANEL_LIMIT) {
            render_ux_anchor_line(ui, event);
        }
    }

    ui.add_space(8.0);
    ui.label(RichText::new("Recent Active-Window Frames").strong());
    if live_events.recent_frames.is_empty() {
        ui.small("No active-window frame metadata in recent JSONL yet.");
    } else {
        for event in live_events.recent_frames.iter().take(EVENT_PANEL_LIMIT) {
            render_frame_metadata_line(ui, event);
        }
    }
}

fn render_bundle_inventory_panel(ui: &mut egui::Ui, inventory: &mut BundleInventoryState) {
    egui::CollapsingHeader::new("Capture Bundles")
        .default_open(true)
        .show(ui, |ui| {
            if let Some(root) = &inventory.capture_root {
                ui.add(Label::new(root.display().to_string()).wrap());
            } else {
                ui.small("Capture root: resolving from status or Application Support");
            }

            let mut status = vec![
                format!("live {}", inventory.live_count),
                format!("processing {}", inventory.processing_count),
                format!("failed {}", inventory.failed_count),
                format!("total {}", human_bytes(inventory.total_bytes)),
                format!("{} files", inventory.total_files),
            ];
            if inventory.pinned_count > 0 {
                status.push(format!("pinned {}", inventory.pinned_count));
            }
            if let Some(ms) = inventory.refresh_ms {
                status.push(format!("{ms}ms"));
            }
            ui.small(status.join(" | "));

            if let Some(action_status) = &inventory.action_status {
                ui.add(Label::new(action_status).wrap());
            }
            if let Some(error) = &inventory.error {
                ui.add(Label::new(RichText::new(error).color(Color32::RED)).wrap());
            }

            ui.horizontal_wrapped(|ui| {
                if ui.button("Validate").clicked() {
                    inventory.action_status = Some(
                        inventory
                            .recent_ready
                            .first()
                            .map(|bundle| {
                                format!(
                                    "Validate placeholder: latest ready bundle is {}",
                                    bundle.path.display()
                                )
                            })
                            .unwrap_or_else(|| {
                                "Validate placeholder: no ready bundle is currently listed"
                                    .to_string()
                            }),
                    );
                }
                if ui.button("Export").clicked() {
                    inventory.action_status = Some(
                        "Export placeholder: export wiring is intentionally not invoked from this read-only panel yet"
                            .to_string(),
                    );
                }
                if ui.button("Sweep").clicked() {
                    inventory.action_status = Some(sweep_placeholder_status(inventory));
                }
            });

            if inventory.recent_ready.is_empty() {
                ui.small("No ready bundles listed yet.");
            } else {
                ui.label(RichText::new("Recent Ready").strong());
                for bundle in &inventory.recent_ready {
                    render_ready_bundle_line(ui, bundle);
                }
            }
        });
}

fn render_ready_bundle_line(ui: &mut egui::Ui, bundle: &BundleEntry) {
    let class = bundle_directory_label(&bundle.directory_class);
    let time = bundle
        .ready_at
        .or(bundle.created_at)
        .map(|time| compact_timestamp(&time.to_rfc3339()))
        .unwrap_or_else(|| "unknown time".to_string());
    let pinned = if bundle.pinned { " pinned" } else { "" };
    ui.add(
        Label::new(format!(
            "{time} {class}{pinned}: {} ({}, {} files)",
            bundle.capture_id,
            human_bytes(bundle.byte_count),
            bundle.file_count
        ))
        .wrap(),
    );
}

fn bundle_directory_label(class: &BundleDirectoryClass) -> &'static str {
    match class {
        BundleDirectoryClass::Processing => "processing",
        BundleDirectoryClass::Live => "live",
        BundleDirectoryClass::Failed => "failed",
        BundleDirectoryClass::Pinned => "pinned",
    }
}

fn sweep_placeholder_status(inventory: &BundleInventoryState) -> String {
    let delete_count = inventory
        .sweep_delete_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| "?".to_string());
    let move_count = inventory
        .sweep_move_count
        .map(|count| count.to_string())
        .unwrap_or_else(|| "?".to_string());
    let bytes = inventory
        .sweep_reclaimable_bytes
        .map(human_bytes)
        .unwrap_or_else(|| "unknown".to_string());
    format!("Sweep dry run: {delete_count} deletes, {move_count} moves, {bytes} reclaimable")
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[unit])
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn render_timeline_event_line(ui: &mut egui::Ui, event: &TimelineEvent) {
    ui.separator();
    let mut header = format!(
        "{}  [{}] {}",
        compact_timestamp(&event.recorded_at),
        event.source_kind,
        event.headline
    );
    if let Some(window_id) = event.window_id {
        header.push_str(&format!("  #{window_id}"));
    }
    ui.small(header);

    let detail = timeline_event_detail_line(event);
    if !detail.is_empty() {
        ui.add(Label::new(detail).wrap());
    }
}

fn live_event_tail_coverage_label(source_files: &[String], scanned_lines: usize) -> String {
    let files = source_files
        .iter()
        .take(4)
        .map(|path| compact_path(path))
        .collect::<Vec<_>>()
        .join(", ");
    let suffix = if source_files.len() > 4 {
        format!(", +{} more", source_files.len() - 4)
    } else {
        String::new()
    };
    format!(
        "tail: {} files | {} lines | {}{}",
        source_files.len(),
        scanned_lines,
        files,
        suffix
    )
}

fn timeline_event_detail_line(event: &TimelineEvent) -> String {
    timeline_event_detail_parts(event).join(" | ")
}

fn timeline_event_detail_parts(event: &TimelineEvent) -> Vec<String> {
    let mut detail = Vec::new();
    detail.push(event.event_type.clone());
    detail.push(format!("time={}", event.time_source));
    if let Some(start) = &event.event_time_start {
        detail.push(format!("start={}", compact_timestamp(start)));
    }
    if let Some(end) = &event.event_time_end {
        detail.push(format!("end={}", compact_timestamp(end)));
    }
    if let Some(ingested_at) = &event.ingested_at {
        detail.push(format!("ingested={}", compact_timestamp(ingested_at)));
    }
    if let Some(durability) = &event.durability {
        detail.push(format!("durability={durability}"));
    }
    if let Some(lane_id) = &event.lane_id {
        detail.push(format!("lane={}", abbreviate(lane_id, 32)));
    }
    if let Some(stream_id) = &event.stream_id {
        detail.push(format!("stream={}", abbreviate(stream_id, 32)));
    }
    if let Some(capture_bundle_id) = &event.capture_bundle_id {
        detail.push(format!("bundle={}", abbreviate(capture_bundle_id, 32)));
    }
    if let Some(source_record_id) = &event.source_record_id {
        detail.push(format!(
            "source_record={}",
            abbreviate(source_record_id, 32)
        ));
    }
    if let Some(source_hash) = &event.source_hash {
        detail.push(format!("source_hash={}", abbreviate(source_hash, 16)));
    }
    if let Some(privacy_class) = &event.privacy_class {
        detail.push(format!("privacy={}", abbreviate(privacy_class, 24)));
    }
    if let Some(shape) = &event.shape {
        detail.push(format!("shape={}", abbreviate(shape, 24)));
    }
    if let Some(target) = &event.target {
        detail.push(target.clone());
    }
    if let Some(summary) = &event.detail {
        detail.push(summary.clone());
    }
    detail.push(format!("file={}", event.source_file));
    detail
}

fn render_ux_anchor_line(ui: &mut egui::Ui, event: &UXAnchorEvent) {
    ui.separator();
    ui.small(format!(
        "{}  {}  {}",
        compact_timestamp(&event.recorded_at),
        event.event_type,
        event.kind
    ));
    let mut parts = Vec::new();
    if let Some(started_at) = &event.started_at {
        parts.push(format!("start={}", compact_timestamp(started_at)));
    }
    if let Some(ended_at) = &event.ended_at {
        parts.push(format!("end={}", compact_timestamp(ended_at)));
    }
    if let Some(scroll) = &event.scroll {
        parts.push(format!(
            "scroll dx={} dy={} events={}",
            format_double(scroll.total_dx),
            format_double(scroll.total_dy),
            scroll.event_count.unwrap_or(0)
        ));
    }
    if let Some(keyboard) = &event.keyboard {
        parts.push(format!(
            "keys events={} down={} repeat={}",
            keyboard.event_count.unwrap_or(0),
            keyboard.key_down_count.unwrap_or(0),
            keyboard.auto_repeat_count.unwrap_or(0)
        ));
    }
    if let Some(pointer) = &event.pointer {
        parts.push(format!(
            "pointer {} {} events={}",
            pointer.action.as_deref().unwrap_or("unknown"),
            pointer.button.as_deref().unwrap_or("unknown"),
            pointer.event_count.unwrap_or(0)
        ));
    }
    if let Some(shortcut) = &event.shortcut {
        parts.push(format!(
            "shortcut events={} down={} repeat={} categories={}",
            shortcut.event_count.unwrap_or(0),
            shortcut.key_down_count.unwrap_or(0),
            shortcut.auto_repeat_count.unwrap_or(0),
            shortcut.categories.join(",")
        ));
    }
    if let Some(focus) = &event.focus_transition {
        parts.push(format!(
            "focus {} -> {} trigger={} confidence={}",
            focus.previous_process_id.as_deref().unwrap_or("?"),
            focus.current_process_id.as_deref().unwrap_or("?"),
            focus.trigger.as_deref().unwrap_or("unknown"),
            focus.confidence.as_deref().unwrap_or("unknown")
        ));
    }
    if let Some(pid) = &event.recent_target_process_id {
        parts.push(format!("target_pid={pid}"));
    }
    if parts.is_empty() {
        parts.push(format!(
            "source={}",
            event.source.as_deref().unwrap_or("unknown")
        ));
    }
    ui.add(Label::new(parts.join(" | ")).wrap());
}

fn render_frame_metadata_line(ui: &mut egui::Ui, event: &FrameMetadataEvent) {
    ui.separator();
    ui.small(format!(
        "{}  {}  seq={}",
        compact_timestamp(&event.recorded_at),
        event.event_type,
        event
            .sequence
            .map(|value| value.to_string())
            .unwrap_or_else(|| "?".to_string())
    ));
    ui.add(
        Label::new(format!(
            "target={} | status={} | dirty area={} rects={}",
            event.target_label(),
            event.frame_status.as_deref().unwrap_or("unknown"),
            format_percent(event.dirty_area_ratio),
            event.dirty_rect_count.unwrap_or(0)
        ))
        .wrap(),
    );
    let mut parts = Vec::new();
    if let Some(ratio) = event.changed_tile_ratio {
        parts.push(format!("tiles={}", format_percent(Some(ratio))));
    }
    if let Some(dy) = event.estimated_dy {
        parts.push(format!("dy={}", format_double(dy)));
    }
    if let Some(mode) = event.motion_label() {
        parts.push(format!("motion={mode}"));
    }
    if let Some(decision) = &event.adaptive_decision {
        parts.push(format!(
            "adaptive={}/{} target_fps={} analysis_fps={} update={} reason={}",
            decision.classifier_mode.as_deref().unwrap_or("unknown"),
            decision.controller_mode.as_deref().unwrap_or("unknown"),
            decision
                .target_fps
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
            decision
                .target_analysis_fps
                .map(|value| value.to_string())
                .unwrap_or_else(|| "?".to_string()),
            decision
                .should_update_stream_configuration
                .map(yes_no)
                .unwrap_or("?"),
            decision.update_reason.as_deref().unwrap_or("unknown")
        ));
    }
    if let Some(feed) = event.feeds_motion_classifier {
        parts.push(format!("classifier_feed={}", yes_no(feed)));
    }
    if let Some(fused) = event.ux_motion_hints_fused {
        parts.push(format!("ux_fused={}", yes_no(fused)));
    }
    if event.ux_motion_hints_present {
        parts.push("ux_hints=present".to_string());
    }
    if let Some(untouched) = event.pixels_untouched {
        parts.push(format!(
            "pixels={}",
            if untouched { "untouched" } else { "sampled" }
        ));
    }
    if !parts.is_empty() {
        ui.add(Label::new(parts.join(" | ")).wrap());
    }
}

fn compact_timestamp(value: &str) -> String {
    value.strip_suffix('Z').unwrap_or(value).replace('T', " ")
}

fn parse_event_timestamp(value: &str) -> Option<EventTimestamp> {
    let value = value.trim();
    let (date, time_with_zone) = value
        .split_once('T')
        .or_else(|| value.split_once('t'))
        .or_else(|| value.split_once(' '))?;
    let (year, month, day) = parse_rfc3339_date(date)?;
    let (time, offset_seconds) = split_rfc3339_time_zone(time_with_zone)?;
    let (hour, minute, second, nanos) = parse_rfc3339_time(time)?;

    let days = days_from_civil(year, month, day);
    Some(EventTimestamp {
        epoch_seconds: days * 86_400
            + i64::from(hour) * 3_600
            + i64::from(minute) * 60
            + i64::from(second)
            - i64::from(offset_seconds),
        nanos,
    })
}

fn parse_rfc3339_date(value: &str) -> Option<(i32, u32, u32)> {
    let mut parts = value.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) {
        return None;
    }
    let max_day = days_in_month(year, month);
    if day == 0 || day > max_day {
        return None;
    }
    Some((year, month, day))
}

fn split_rfc3339_time_zone(value: &str) -> Option<(&str, i32)> {
    let value = value.trim();
    if let Some(time) = value.strip_suffix('Z').or_else(|| value.strip_suffix('z')) {
        return Some((time, 0));
    }

    let zone_index = value
        .char_indices()
        .rev()
        .find_map(|(index, ch)| (index > 0 && (ch == '+' || ch == '-')).then_some(index));
    if let Some(index) = zone_index {
        let (time, zone) = value.split_at(index);
        return Some((time, parse_rfc3339_offset(zone)?));
    }

    Some((value, 0))
}

fn parse_rfc3339_offset(value: &str) -> Option<i32> {
    let (sign, offset) = value.split_at(1);
    let sign = match sign {
        "+" => 1,
        "-" => -1,
        _ => return None,
    };
    let (hours, minutes) = if let Some((hours, minutes)) = offset.split_once(':') {
        (hours, minutes)
    } else if offset.len() == 4 {
        offset.split_at(2)
    } else if offset.len() == 2 {
        (offset, "0")
    } else {
        return None;
    };
    let hours = hours.parse::<i32>().ok()?;
    let minutes = minutes.parse::<i32>().ok()?;
    if !(0..=23).contains(&hours) || !(0..=59).contains(&minutes) {
        return None;
    }
    Some(sign * (hours * 3_600 + minutes * 60))
}

fn parse_rfc3339_time(value: &str) -> Option<(u32, u32, u32, u32)> {
    let mut parts = value.split(':');
    let hour = parts.next()?.parse::<u32>().ok()?;
    let minute = parts.next()?.parse::<u32>().ok()?;
    let seconds = parts.next()?;
    if parts.next().is_some() || hour > 23 || minute > 59 {
        return None;
    }
    let (second, nanos) = if let Some((second, fraction)) = seconds.split_once('.') {
        (
            second.parse::<u32>().ok()?,
            parse_fractional_nanos(fraction)?,
        )
    } else {
        (seconds.parse::<u32>().ok()?, 0)
    };
    if second > 59 {
        return None;
    }
    Some((hour, minute, second, nanos))
}

fn parse_fractional_nanos(value: &str) -> Option<u32> {
    let digits = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.is_empty() {
        return None;
    }
    let mut nanos = digits.chars().take(9).collect::<String>();
    while nanos.len() < 9 {
        nanos.push('0');
    }
    nanos.parse::<u32>().ok()
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = year - if month <= 2 { 1 } else { 0 };
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let year_of_era = year - era * 400;
    let month = month as i32;
    let day = day as i32;
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    i64::from(era) * 146_097 + i64::from(day_of_era) - 719_468
}

fn format_percent(value: Option<f64>) -> String {
    value
        .map(|value| format!("{:.2}%", value * 100.0))
        .unwrap_or_else(|| "n/a".to_string())
}

fn format_double(value: f64) -> String {
    format!("{value:.2}")
}

fn yes_no(value: bool) -> &'static str {
    if value {
        "yes"
    } else {
        "no"
    }
}

fn is_timeout_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("timed out") || error.contains("did not return a response")
}

fn should_pause_auto_refresh_after_capture_error(error: &str) -> bool {
    is_timeout_error(error)
}

fn display_count(value: Option<u64>) -> String {
    value
        .map(|value| value.to_string())
        .unwrap_or_else(|| "?".to_string())
}

fn compact_window_label(window: &CaptureWindowState) -> String {
    let id = window
        .window_id
        .map(|id| format!("#{id} "))
        .unwrap_or_default();
    let app = window.app_name.as_deref().unwrap_or("unknown");
    let title = window
        .title
        .as_deref()
        .filter(|title| !title.is_empty())
        .unwrap_or("(untitled)");
    format!("{id}{app} - {title}")
}

fn render_window_line(ui: &mut egui::Ui, window: &CaptureWindowState) {
    let title = window
        .title
        .as_deref()
        .filter(|value| !value.is_empty())
        .unwrap_or("(untitled)");
    egui::Frame::group(ui.style())
        .inner_margin(egui::Margin::symmetric(8, 6))
        .show(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "#{} z={} {}",
                    window.window_id.unwrap_or(0),
                    window.z_rank.unwrap_or(-1),
                    window.app_name.as_deref().unwrap_or("unknown")
                ))
                .strong(),
            );
            ui.add(Label::new(title).wrap());
            if let Some(frame) = &window.frame_points {
                ui.monospace(format!(
                    "{}x{} +{},{}",
                    frame.width.round() as i64,
                    frame.height.round() as i64,
                    frame.x.round() as i64,
                    frame.y.round() as i64
                ));
            }
            if let Some(display_id) = &window.display_id {
                ui.small(format!("display: {display_id}"));
            }
            if let Some(frame) = &window.frame_pixels {
                ui.small(format!(
                    "pixels: {}x{} +{},{}",
                    frame.width.round() as i64,
                    frame.height.round() as i64,
                    frame.x.round() as i64,
                    frame.y.round() as i64
                ));
            }
            if let Some(source) = window.source.as_deref().filter(|source| !source.is_empty()) {
                ui.small(format!("source: {source}"));
            }
            if let Some(focus) = &window.focus_metadata {
                ui.small(format!("focus: {}", focus.compact_label()));
            }
        });
}

fn render_preview(
    ui: &mut egui::Ui,
    snapshot: Option<&CaptureSnapshot>,
    texture: Option<&TextureHandle>,
    image_size: [usize; 2],
    preview_space: PreviewCoordinateSpace,
    show_overlays: bool,
    show_overlay_labels: bool,
    overlay_limit: usize,
) {
    let Some(texture) = texture else {
        ui.centered_and_justified(|ui| {
            ui.label("No live preview yet.");
        });
        return;
    };

    let available = ui.available_size();
    let image_vec = Vec2::new(image_size[0] as f32, image_size[1] as f32);
    let scale = (available.x / image_vec.x)
        .min(available.y / image_vec.y)
        .max(0.05);
    let draw_size = image_vec * scale;
    let (available_rect, _) = ui.allocate_exact_size(available, egui::Sense::hover());
    let rect = Rect::from_center_size(available_rect.center(), draw_size);
    ui.put(rect, egui::Image::new(texture).fit_to_exact_size(draw_size));

    if show_overlays && preview_space == PreviewCoordinateSpace::Display {
        if let Some(snapshot) = snapshot {
            draw_window_overlays(ui, snapshot, rect, show_overlay_labels, overlay_limit);
        }
    }
}

fn draw_window_overlays(
    ui: &egui::Ui,
    snapshot: &CaptureSnapshot,
    image_rect: Rect,
    show_labels: bool,
    overlay_limit: usize,
) {
    let Some(display) = snapshot.main_display() else {
        return;
    };
    let Some(display_frame) = &display.frame_points else {
        return;
    };
    if display_frame.width <= 0.0 || display_frame.height <= 0.0 {
        return;
    }

    let sx = image_rect.width() / display_frame.width as f32;
    let sy = image_rect.height() / display_frame.height as f32;
    let painter = ui.painter();

    let mut windows = overlay_candidates(snapshot, display_frame, overlay_limit);
    windows.sort_by_key(|window| {
        (
            if window.is_focused { 1 } else { 0 },
            -window.z_rank.unwrap_or(i64::MAX),
            window.window_id.unwrap_or(0),
        )
    });

    for (index, window) in windows.into_iter().enumerate() {
        let Some(rect) = overlay_rect(window, display_frame, image_rect, sx, sy) else {
            continue;
        };
        let color = if window.is_focused {
            Color32::from_rgb(255, 207, 64)
        } else {
            Color32::from_rgb(54, 205, 114)
        };
        painter.rect_stroke(
            rect,
            0.0,
            Stroke::new(if window.is_focused { 3.0 } else { 1.5 }, color),
            StrokeKind::Inside,
        );
        if show_labels
            && (window.is_focused || index < 4)
            && rect.width() >= 120.0
            && rect.height() >= 42.0
        {
            let label = format!(
                "{} #{}",
                window.app_name.as_deref().unwrap_or("window"),
                window.window_id.unwrap_or(0)
            );
            painter.text(
                rect.min + Vec2::new(5.0, 5.0),
                egui::Align2::LEFT_TOP,
                label,
                FontId::monospace(11.0),
                color,
            );
        }
    }
}

fn load_snapshot(cli_path: &PathBuf) -> Result<CaptureSnapshot> {
    let mut command = Command::new(cli_path);
    command.args(["capture", "snapshot"]);
    let output = command_output_with_timeout(command, CLI_SNAPSHOT_TIMEOUT, "capture snapshot")
        .with_context(|| format!("run {}", cli_path.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "capture snapshot failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    serde_json::from_slice::<CaptureSnapshot>(&output.stdout).context("parse capture snapshot")
}

fn load_snapshot_timed(cli_path: &PathBuf) -> Result<(CaptureSnapshot, u128)> {
    let started = Instant::now();
    let snapshot = load_snapshot(cli_path)?;
    Ok((snapshot, started.elapsed().as_millis()))
}

fn load_capture_status(cli_path: &PathBuf) -> Result<CaptureStatus> {
    let mut command = Command::new(cli_path);
    command.args(["capture", "status"]);
    let output = command_output_with_timeout(command, CLI_STATUS_TIMEOUT, "capture status")
        .with_context(|| format!("run {}", cli_path.display()))?;
    if !output.status.success() {
        return Err(anyhow!(
            "capture status failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let value: Value = serde_json::from_slice(&output.stdout).context("parse capture status")?;
    Ok(capture_status_from_value(
        value.get("result").unwrap_or(&value),
    ))
}

fn command_output_with_timeout(
    mut command: Command,
    timeout: Duration,
    label: &str,
) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn().with_context(|| format!("spawn {label}"))?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow!("capture {label} stdout"))?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| anyhow!("capture {label} stderr"))?;
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait().context("poll child process")? {
            let stdout = stdout_reader
                .join()
                .map_err(|_| anyhow!("{label} stdout reader panicked"))?
                .with_context(|| format!("read {label} stdout"))?;
            let stderr = stderr_reader
                .join()
                .map_err(|_| anyhow!("{label} stderr reader panicked"))?
                .with_context(|| format!("read {label} stderr"))?;
            return Ok(Output {
                status,
                stdout,
                stderr,
            });
        }
        if started.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_reader.join();
            let _ = stderr_reader.join();
            return Err(anyhow!(
                "{label} timed out after {:.1}s",
                timeout.as_secs_f32()
            ));
        }
        thread::sleep(Duration::from_millis(25));
    }
}

fn refresh_live_events(
    cli_path: &PathBuf,
    existing_events_directory: Option<&Path>,
) -> Result<LiveEventState> {
    let mut state = LiveEventState::default();
    if state.events_directory.is_none() {
        state.events_directory = existing_events_directory.map(PathBuf::from);
    }
    if state.events_directory.is_none() {
        state.events_directory = resolve_known_events_directory();
    }
    if state.events_directory.is_none() {
        let status = load_capture_status(cli_path)?;
        state.apply_capture_status(status);
    }

    let Some(events_directory) = state.events_directory.clone() else {
        return Err(anyhow!(
            "could not resolve capture events directory from capture.status or known runtime paths"
        ));
    };
    if !events_directory.exists() {
        state.error = Some(format!(
            "events directory does not exist yet: {}",
            events_directory.display()
        ));
        return Ok(state);
    }

    let inferred_windows_directory = events_directory.parent().map(|root| root.join("windows"));
    let windows_directory = state
        .capture_status
        .as_ref()
        .and_then(|status| status.windows_directory.as_deref())
        .or(inferred_windows_directory.as_deref());
    let recent = read_recent_capture_events(&events_directory, windows_directory)?;
    state.timeline = recent.timeline;
    state.recent_ux = recent.ux;
    state.recent_frames = recent.frames;
    state.source_files = recent.source_files;
    state.scanned_lines = recent.scanned_lines;
    Ok(state)
}

fn load_bundle_inventory(
    cli_path: &PathBuf,
    root_hint: Option<&Path>,
    events_hint: Option<&Path>,
) -> Result<BundleInventoryState> {
    let capture_root = resolve_capture_bundle_root(cli_path, root_hint, events_hint)?;
    let inventory = list_bundles(&capture_root)
        .with_context(|| format!("list capture bundles under {}", capture_root.display()))?;
    let mut state = BundleInventoryState {
        capture_root: Some(inventory.capture_root.clone()),
        total_bytes: inventory.total_bytes,
        total_files: inventory.total_files,
        ..BundleInventoryState::default()
    };
    for entry in &inventory.entries {
        match entry.directory_class {
            BundleDirectoryClass::Processing => state.processing_count += 1,
            BundleDirectoryClass::Live => state.live_count += 1,
            BundleDirectoryClass::Failed => state.failed_count += 1,
            BundleDirectoryClass::Pinned => state.pinned_count += 1,
        }
    }

    state.recent_ready = inventory
        .entries
        .iter()
        .filter(|entry| entry.ready)
        .cloned()
        .collect();
    state
        .recent_ready
        .sort_by_key(|entry| entry.ready_at.or(entry.created_at));
    state.recent_ready.reverse();
    state.recent_ready.truncate(RECENT_READY_BUNDLE_LIMIT);

    let mut policy = RetentionPolicy::default();
    policy.dry_run = true;
    let plan = onecontext_capture_core::plan_retention_sweep(&capture_root, &policy, Utc::now())
        .with_context(|| format!("plan dry-run sweep under {}", capture_root.display()))?;
    state.sweep_delete_count = Some(
        plan.actions
            .iter()
            .filter(|action| action.kind == SweepActionKind::Delete)
            .count(),
    );
    state.sweep_move_count = Some(
        plan.actions
            .iter()
            .filter(|action| action.kind == SweepActionKind::MoveToFailed)
            .count(),
    );
    state.sweep_reclaimable_bytes = Some(
        plan.actions
            .iter()
            .filter(|action| action.kind == SweepActionKind::Delete)
            .map(|action| action.byte_count)
            .sum(),
    );
    Ok(state)
}

fn resolve_capture_bundle_root(
    cli_path: &PathBuf,
    root_hint: Option<&Path>,
    events_hint: Option<&Path>,
) -> Result<PathBuf> {
    if let Some(root) = root_hint {
        return Ok(root.to_path_buf());
    }
    if let Some(root) = events_hint.and_then(Path::parent) {
        return Ok(root.to_path_buf());
    }
    if let Some(root) = resolve_known_capture_root() {
        return Ok(root);
    }
    let status = load_capture_status(cli_path)?;
    if let Some(root) = status.root_directory {
        return Ok(root);
    }
    if let Some(root) = status.events_directory.as_deref().and_then(Path::parent) {
        return Ok(root.to_path_buf());
    }
    Err(anyhow!(
        "could not resolve capture root from capture.status or Application Support"
    ))
}

fn read_recent_capture_events(
    events_directory: &Path,
    windows_directory: Option<&Path>,
) -> Result<RecentCaptureEvents> {
    let mut files = recent_capture_log_files(events_directory, ".events.jsonl")?;
    if let Some(windows_directory) = windows_directory.filter(|path| path.exists()) {
        files.extend(recent_capture_log_files(
            windows_directory,
            ".windows.jsonl",
        )?);
    }
    files.sort_by(|lhs, rhs| {
        let lhs_modified = fs::metadata(lhs).and_then(|meta| meta.modified()).ok();
        let rhs_modified = fs::metadata(rhs).and_then(|meta| meta.modified()).ok();
        rhs_modified
            .cmp(&lhs_modified)
            .then_with(|| rhs.file_name().cmp(&lhs.file_name()))
    });
    files.truncate(EVENT_MAX_FILES * 2);

    let mut recent = RecentCaptureEvents::default();
    for file in files {
        recent.source_files.push(file.display().to_string());
        let lines = tail_non_empty_lines(&file, EVENT_MAX_BYTES_PER_FILE)?;
        for line in lines.into_iter().rev() {
            if recent.scanned_lines >= EVENT_MAX_LINES {
                recent.sort_and_cap_timeline();
                return Ok(recent);
            }
            recent.scanned_lines += 1;
            append_recent_event_line(&line, &file, &mut recent);
        }
    }
    recent.sort_and_cap_timeline();
    Ok(recent)
}

fn append_recent_event_line(line: &str, source_file: &Path, recent: &mut RecentCaptureEvents) {
    let Ok(envelope) = serde_json::from_str::<CaptureEventEnvelopeValue>(line) else {
        return;
    };

    let mut recognized = false;
    if let Some(frame) = parse_frame_metadata_event(&envelope) {
        recognized = true;
        if let Some(event) = timeline_event_from_frame(&frame, &envelope, source_file) {
            recent.timeline.push(event);
        }
        if recent.frames.len() < EVENT_PANEL_LIMIT {
            recent.frames.push(frame);
        }
    }

    let anchors = parse_ux_anchor_events(&envelope);
    if !anchors.is_empty() {
        recognized = true;
    }
    for anchor in anchors {
        recent
            .timeline
            .push(timeline_event_from_ux_anchor(&anchor, source_file));
        if recent.ux.len() < EVENT_PANEL_LIMIT {
            recent.ux.push(anchor);
        }
    }

    if !recognized {
        if let Some(event) = generic_timeline_event(&envelope, source_file) {
            recent.timeline.push(event);
        }
    }
}

fn recent_capture_log_files(directory: &Path, suffix: &str) -> Result<Vec<PathBuf>> {
    let mut files: Vec<PathBuf> = fs::read_dir(directory)
        .with_context(|| format!("read {}", directory.display()))?
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .map(|name| name.ends_with(suffix))
                .unwrap_or(false)
        })
        .collect();
    files.sort_by(|lhs, rhs| {
        let lhs_modified = fs::metadata(lhs).and_then(|meta| meta.modified()).ok();
        let rhs_modified = fs::metadata(rhs).and_then(|meta| meta.modified()).ok();
        rhs_modified
            .cmp(&lhs_modified)
            .then_with(|| rhs.file_name().cmp(&lhs.file_name()))
    });
    Ok(files)
}

fn tail_non_empty_lines(path: &Path, max_bytes: u64) -> Result<Vec<String>> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let len = file
        .metadata()
        .with_context(|| format!("stat {}", path.display()))?
        .len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))
        .with_context(|| format!("seek {}", path.display()))?;
    let mut bytes = Vec::with_capacity((len - start).min(max_bytes) as usize);
    file.read_to_end(&mut bytes)
        .with_context(|| format!("read {}", path.display()))?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if start > 0 {
        if let Some(index) = text.find('\n') {
            text = text[index + 1..].to_string();
        }
    }
    Ok(text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect())
}

fn capture_status_from_value(value: &Value) -> CaptureStatus {
    let mut status =
        serde_json::from_value::<CaptureStatus>(value.clone()).unwrap_or_else(|_| CaptureStatus {
            root_directory: string_field(value, &["root_directory", "rootDirectory"])
                .map(PathBuf::from),
            events_directory: string_field(value, &["events_directory", "eventsDirectory"])
                .map(PathBuf::from),
            metadata_sample_fusion: value_field(
                value,
                &["metadata_sample_fusion", "metadataSampleFusion"],
            )
            .map(metadata_sample_fusion_from_value),
            ..CaptureStatus::default()
        });
    if status.root_directory.is_none() {
        status.root_directory =
            string_field(value, &["root_directory", "rootDirectory"]).map(PathBuf::from);
    }
    if status.events_directory.is_none() {
        status.events_directory =
            string_field(value, &["events_directory", "eventsDirectory"]).map(PathBuf::from);
    }
    if status.metadata_sample_fusion.is_none() {
        status.metadata_sample_fusion =
            value_field(value, &["metadata_sample_fusion", "metadataSampleFusion"])
                .map(metadata_sample_fusion_from_value);
    }
    status.capability_signals = collect_capability_signals(value);
    status
}

fn deserialize_capture_status_option<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<CaptureStatus>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.as_ref().map(capture_status_from_value))
}

fn metadata_sample_fusion_from_value(value: &Value) -> MetadataSampleFusion {
    MetadataSampleFusion {
        ux_motion_hints_enabled: bool_field(
            value,
            &["ux_motion_hints_enabled", "uxMotionHintsEnabled"],
        ),
        source: string_field(value, &["source"]),
        pixels_untouched: bool_field(value, &["pixels_untouched", "pixelsUntouched"]),
    }
}

fn resolve_known_events_directory() -> Option<PathBuf> {
    if let Ok(path) = env::var("ONECONTEXT_CAPTURE_EVENTS_DIR") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }
    if let Ok(path) = env::var("ONECONTEXT_CAPTURE_ROOT") {
        let path = PathBuf::from(path).join("events");
        if path.exists() {
            return Some(path);
        }
    }

    let home = env::var("HOME").ok().map(PathBuf::from)?;
    let app_support = home.join("Library/Application Support");
    let mut candidates = vec![
        app_support.join("1Context Dev/capture/events"),
        app_support.join("1Context/capture/events"),
    ];
    if let Ok(entries) = fs::read_dir(&app_support) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with("1Context") || name.starts_with("com.haptica.1context") {
                candidates.push(path.join("capture/events"));
            }
        }
    }
    candidates
        .into_iter()
        .filter(|path| path.exists())
        .max_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
}

fn resolve_known_capture_root() -> Option<PathBuf> {
    if let Ok(path) = env::var("ONECONTEXT_CAPTURE_ROOT") {
        return Some(PathBuf::from(path));
    }
    if let Ok(path) = env::var("ONECONTEXT_CAPTURE_EVENTS_DIR") {
        if let Some(root) = Path::new(&path).parent() {
            return Some(root.to_path_buf());
        }
    }

    let home = env::var("HOME").ok().map(PathBuf::from)?;
    let app_support = home.join("Library/Application Support");
    let mut candidates = vec![
        app_support.join("1Context Dev/capture"),
        app_support.join("1Context/capture"),
    ];
    if let Ok(entries) = fs::read_dir(&app_support) {
        for entry in entries.flatten() {
            let path = entry.path();
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            if name.starts_with("1Context") || name.starts_with("com.haptica.1context") {
                candidates.push(path.join("capture"));
            }
        }
    }
    candidates
        .iter()
        .filter(|path| path.exists())
        .max_by_key(|path| fs::metadata(path).and_then(|meta| meta.modified()).ok())
        .cloned()
        .or_else(|| candidates.into_iter().next())
}

fn parse_frame_metadata_event(envelope: &CaptureEventEnvelopeValue) -> Option<FrameMetadataEvent> {
    let event_type = envelope.event_type.as_str();
    let payload = &envelope.payload;
    let looks_like_frame = event_type == "capture.active_window_frame_metadata"
        || value_field(payload, &["dirtyRectSummary", "dirty_rect_summary"]).is_some()
        || value_field(payload, &["motionFeatures", "motion_features"]).is_some()
        || string_field(payload, &["frameStatus", "frame_status"]).is_some();
    if !looks_like_frame {
        return None;
    }

    let recorded_at =
        select_event_time(envelope, payload, &["capturedAt", "captured_at"])?.display_at;
    let target = value_field(payload, &["target"]);
    let dirty = value_field(payload, &["dirtyRectSummary", "dirty_rect_summary"]);
    let motion = value_field(payload, &["motionFeatures", "motion_features"]);
    let policy = value_field(
        payload,
        &[
            "capturePolicyDecision",
            "capture_policy_decision",
            "policyDecision",
            "policy_decision",
        ],
    );
    let adaptive = value_field(payload, &["adaptiveDecision", "adaptive_decision"]);
    Some(FrameMetadataEvent {
        event_type: envelope.event_type.clone(),
        recorded_at,
        sequence: integer_field(payload, &["sequence"]),
        frame_status: string_field(payload, &["frameStatus", "frame_status"]),
        dirty_area_ratio: dirty
            .and_then(|dirty| number_field(dirty, &["dirtyAreaRatio", "dirty_area_ratio"]))
            .or_else(|| {
                motion.and_then(|motion| {
                    number_field(motion, &["dirtyAreaRatio", "dirty_area_ratio"])
                })
            }),
        dirty_rect_count: dirty
            .and_then(|dirty| integer_field(dirty, &["dirtyRectCount", "dirty_rect_count"]))
            .or_else(|| {
                motion.and_then(|motion| {
                    integer_field(motion, &["dirtyRectCount", "dirty_rect_count"])
                })
            }),
        changed_tile_ratio: dirty
            .and_then(|dirty| number_field(dirty, &["changedTileRatio", "changed_tile_ratio"]))
            .or_else(|| {
                motion.and_then(|motion| {
                    number_field(motion, &["changedTileRatio", "changed_tile_ratio"])
                })
            }),
        estimated_dy: dirty
            .and_then(|dirty| number_field(dirty, &["estimatedDY", "estimated_dy"]))
            .or_else(|| {
                motion.and_then(|motion| number_field(motion, &["estimatedDY", "estimated_dy"]))
            }),
        motion_classification: string_field(
            payload,
            &[
                "motionClassification",
                "motion_classification",
                "classification",
            ],
        )
        .or_else(|| {
            motion.and_then(|motion| {
                string_field(
                    motion,
                    &[
                        "motionClassification",
                        "motion_classification",
                        "classification",
                    ],
                )
            })
        }),
        motion_mode: string_field(
            payload,
            &["motionMode", "motion_mode", "captureMode", "capture_mode"],
        )
        .or_else(|| {
            adaptive
                .and_then(|adaptive| string_field(adaptive, &["controllerMode", "controller_mode"]))
        })
        .or_else(|| policy.and_then(|policy| string_field(policy, &["mode"]))),
        feeds_motion_classifier: bool_field(
            payload,
            &["feedsMotionClassifier", "feeds_motion_classifier"],
        ),
        ux_motion_hints_fused: bool_field(
            payload,
            &["uxMotionHintsFused", "ux_motion_hints_fused"],
        ),
        ux_motion_hints_present: value_field(payload, &["uxMotionHints", "ux_motion_hints"])
            .is_some(),
        pixels_untouched: bool_field(payload, &["pixelsUntouched", "pixels_untouched"]).or_else(
            || {
                value_field(payload, &["metadataSampleFusion", "metadata_sample_fusion"])
                    .and_then(|fusion| bool_field(fusion, &["pixelsUntouched", "pixels_untouched"]))
            },
        ),
        adaptive_decision: adaptive.map(adaptive_decision_from_value),
        target_window_id: target
            .and_then(|target| integer_field(target, &["windowID", "window_id"])),
        target_app_name: target.and_then(|target| string_field(target, &["appName", "app_name"])),
        target_title: target.and_then(|target| string_field(target, &["title"])),
    })
}

fn parse_ux_anchor_events(envelope: &CaptureEventEnvelopeValue) -> Vec<UXAnchorEvent> {
    let mut anchors = Vec::new();
    let event_type = envelope.event_type.as_str();
    let payload = &envelope.payload;

    if let Some(array) = value_field(payload, &["anchors"]).and_then(Value::as_array) {
        for value in array {
            if let Some(anchor) = parse_ux_anchor_payload(envelope, event_type, value) {
                anchors.push(anchor);
            }
        }
        return anchors;
    }

    if let Some(anchor) = parse_ux_anchor_payload(envelope, event_type, payload) {
        anchors.push(anchor);
    }
    anchors
}

fn parse_ux_anchor_payload(
    envelope: &CaptureEventEnvelopeValue,
    event_type: &str,
    payload: &Value,
) -> Option<UXAnchorEvent> {
    let kind = string_field(payload, &["kind"])?;
    let has_ux_event_type = event_type.contains(".ux") || event_type.contains("ux_");
    if !has_ux_event_type
        && value_field(payload, &["scroll"]).is_none()
        && value_field(payload, &["pointer"]).is_none()
        && value_field(payload, &["keyboardActivity", "keyboard_activity"]).is_none()
    {
        return None;
    }
    let started_at = envelope
        .event_time_start
        .clone()
        .or_else(|| string_field(payload, &["startedAt", "started_at"]));
    let ended_at = envelope
        .event_time_end
        .clone()
        .or_else(|| string_field(payload, &["endedAt", "ended_at"]));
    let timeline_common = timeline_common_fields(
        envelope,
        payload,
        Path::new(""),
        &["endedAt", "ended_at", "startedAt", "started_at"],
    )?;
    let recorded_at = timeline_common.recorded_at.clone();
    let scroll = value_field(payload, &["scroll"]).map(|scroll| UXScrollSummary {
        event_count: integer_field(scroll, &["eventCount", "event_count"]),
        total_dx: number_field(scroll, &["totalDX", "total_dx"]).unwrap_or(0.0),
        total_dy: number_field(scroll, &["totalDY", "total_dy"]).unwrap_or(0.0),
    });
    let keyboard =
        value_field(payload, &["keyboardActivity", "keyboard_activity"]).map(|keyboard| {
            UXKeyboardSummary {
                event_count: integer_field(keyboard, &["eventCount", "event_count"]),
                key_down_count: integer_field(keyboard, &["keyDownCount", "key_down_count"]),
                auto_repeat_count: integer_field(
                    keyboard,
                    &["autoRepeatCount", "auto_repeat_count"],
                ),
            }
        });
    let pointer = value_field(payload, &["pointer"]).map(|pointer| UXPointerEventSummary {
        action: string_field(pointer, &["action"]),
        button: string_field(pointer, &["button"]),
        event_count: integer_field(pointer, &["eventCount", "event_count"]),
    });
    let shortcut = value_field(payload, &["shortcut"]).map(shortcut_summary_from_value);
    let focus_transition = value_field(payload, &["focusTransition", "focus_transition"])
        .map(focus_transition_from_value);
    Some(UXAnchorEvent {
        event_type: event_type.to_string(),
        recorded_at,
        timeline_common,
        started_at,
        ended_at,
        kind,
        source: string_field(payload, &["source"]),
        recent_target_process_id: string_field(
            payload,
            &["recentTargetProcessID", "recent_target_process_id"],
        ),
        scroll,
        keyboard,
        pointer,
        shortcut,
        focus_transition,
    })
}

fn shortcut_summary_from_value(value: &Value) -> UXShortcutSummary {
    let categories = value_field(value, &["actionCategories", "action_categories"])
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| string_field(item, &["category"]))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    UXShortcutSummary {
        event_count: integer_field(value, &["eventCount", "event_count"]),
        key_down_count: integer_field(value, &["keyDownCount", "key_down_count"]),
        auto_repeat_count: integer_field(value, &["autoRepeatCount", "auto_repeat_count"]),
        categories,
    }
}

fn focus_transition_from_value(value: &Value) -> UXFocusTransitionSummary {
    UXFocusTransitionSummary {
        previous_process_id: string_field(value, &["previousProcessID", "previous_process_id"]),
        current_process_id: string_field(value, &["currentProcessID", "current_process_id"]),
        trigger: string_field(value, &["trigger"]),
        confidence: string_field(value, &["confidence"]),
    }
}

fn adaptive_decision_from_value(value: &Value) -> FrameAdaptiveDecision {
    FrameAdaptiveDecision {
        classifier_mode: string_field(value, &["classifierMode", "classifier_mode"]),
        controller_mode: string_field(value, &["controllerMode", "controller_mode"]),
        target_fps: integer_field(value, &["targetFPS", "target_fps"]),
        target_analysis_fps: integer_field(value, &["targetAnalysisFPS", "target_analysis_fps"]),
        should_update_stream_configuration: bool_field(
            value,
            &[
                "shouldUpdateStreamConfiguration",
                "should_update_stream_configuration",
            ],
        ),
        update_reason: string_field(value, &["updateReason", "update_reason"]),
    }
}

fn timeline_event_from_frame(
    frame: &FrameMetadataEvent,
    envelope: &CaptureEventEnvelopeValue,
    source_file: &Path,
) -> Option<TimelineEvent> {
    let mut detail = Vec::new();
    detail.push(format!(
        "status={}",
        frame.frame_status.as_deref().unwrap_or("unknown")
    ));
    detail.push(format!("dirty={}", format_percent(frame.dirty_area_ratio)));
    detail.push(format!("rects={}", frame.dirty_rect_count.unwrap_or(0)));
    if let Some(dy) = frame.estimated_dy {
        detail.push(format!("dy={}", format_double(dy)));
    }
    if let Some(motion) = frame.motion_label() {
        detail.push(format!("motion={motion}"));
    }

    let common = timeline_common_fields(
        envelope,
        &envelope.payload,
        source_file,
        &["capturedAt", "captured_at"],
    )?;
    Some(timeline_event_with_common(
        common,
        "SCK".to_string(),
        frame.event_type.clone(),
        frame
            .motion_label()
            .or_else(|| frame.frame_status.clone())
            .unwrap_or_else(|| "frame metadata".to_string()),
        Some(detail.join(" | ")),
        frame.target_window_id,
        Some(frame.target_label()),
    ))
}

fn timeline_event_from_ux_anchor(anchor: &UXAnchorEvent, source_file: &Path) -> TimelineEvent {
    let mut detail = Vec::new();
    if let Some(scroll) = &anchor.scroll {
        detail.push(format!(
            "scroll dx={} dy={} events={}",
            format_double(scroll.total_dx),
            format_double(scroll.total_dy),
            scroll.event_count.unwrap_or(0)
        ));
    }
    if let Some(keyboard) = &anchor.keyboard {
        detail.push(format!(
            "keys events={} down={} repeat={}",
            keyboard.event_count.unwrap_or(0),
            keyboard.key_down_count.unwrap_or(0),
            keyboard.auto_repeat_count.unwrap_or(0)
        ));
    }
    if let Some(pointer) = &anchor.pointer {
        detail.push(format!(
            "pointer {} {} events={}",
            pointer.action.as_deref().unwrap_or("unknown"),
            pointer.button.as_deref().unwrap_or("unknown"),
            pointer.event_count.unwrap_or(0)
        ));
    }

    let mut common = anchor.timeline_common.clone();
    common.source_file = source_file_label(source_file);
    timeline_event_with_common(
        common,
        "UX".to_string(),
        anchor.event_type.clone(),
        anchor.kind.clone(),
        (!detail.is_empty()).then(|| detail.join(" | ")),
        None,
        anchor.source.clone(),
    )
}

fn generic_timeline_event(
    envelope: &CaptureEventEnvelopeValue,
    source_file: &Path,
) -> Option<TimelineEvent> {
    let payload = &envelope.payload;
    let common = timeline_common_fields(
        envelope,
        payload,
        source_file,
        &[
            "capturedAt",
            "captured_at",
            "generatedAt",
            "generated_at",
            "endedAt",
            "ended_at",
            "startedAt",
            "started_at",
        ],
    )?;
    let target = value_field(payload, &["target"]);
    let window_id = integer_field(payload, &["windowID", "window_id"])
        .or_else(|| integer_field(payload, &["matchedWindowID", "matched_window_id"]))
        .or_else(|| target.and_then(|target| integer_field(target, &["windowID", "window_id"])));
    let target_label = target
        .and_then(|target| {
            let app = string_field(target, &["appName", "app_name"]);
            let title = string_field(target, &["title"]);
            match (app, title) {
                (Some(app), Some(title)) if !title.is_empty() => Some(format!("{app} - {title}")),
                (Some(app), _) => Some(app),
                (_, Some(title)) if !title.is_empty() => Some(title),
                _ => None,
            }
        })
        .or_else(|| {
            value_field(payload, &["activeApplication", "active_application"])
                .and_then(|active| string_field(active, &["appName", "app_name"]))
        });
    let headline = timeline_headline(&envelope.event_type, payload);
    let mut detail = Vec::new();
    if let Some(windows) = value_field(payload, &["windows"]).and_then(Value::as_array) {
        detail.push(format!("windows={}", windows.len()));
    }
    if let Some(displays) = value_field(payload, &["displays"]).and_then(Value::as_array) {
        detail.push(format!("displays={}", displays.len()));
    }
    if let Some(status) = string_field(payload, &["status"]) {
        detail.push(format!("status={}", abbreviate(&status, 48)));
    }
    if let Some(active) = value_field(payload, &["activeApplication", "active_application"])
        .and_then(|active| string_field(active, &["appName", "app_name"]))
    {
        detail.push(format!("active_app={}", abbreviate(&active, 48)));
    }
    if let Some(matched) = integer_field(payload, &["matchedWindowID", "matched_window_id"]) {
        detail.push(format!("matched_window=#{matched}"));
    }
    for key in ["frameStatus", "frame_status", "source"] {
        if let Some(value) = string_field(payload, &[key]) {
            detail.push(format!("{key}={}", abbreviate(&value, 48)));
        }
    }
    Some(timeline_event_with_common(
        common,
        timeline_source_kind(&envelope.event_type).to_string(),
        envelope.event_type.clone(),
        headline,
        (!detail.is_empty()).then(|| detail.join(" | ")),
        window_id,
        target_label,
    ))
}

fn timeline_headline(event_type: &str, payload: &Value) -> String {
    match event_type {
        "capture.ax_focused_context" => "AX focused context".to_string(),
        "capture.window_snapshot" => "window graph snapshot".to_string(),
        _ => string_field(payload, &["surface", "kind", "operation", "status"])
            .unwrap_or_else(|| event_type.to_string()),
    }
}

fn timeline_source_kind(event_type: &str) -> &'static str {
    if event_type.contains("ux") {
        "UX"
    } else if event_type.contains("focused") || event_type.contains("ax") {
        "AX"
    } else if event_type.contains("window") {
        "Window"
    } else {
        "Event"
    }
}

fn source_file_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.display().to_string())
}

fn timeline_common_fields(
    envelope: &CaptureEventEnvelopeValue,
    payload: &Value,
    source_file: &Path,
    fallback_payload_time_keys: &[&str],
) -> Option<TimelineCommonFields> {
    let timing = select_event_time(envelope, payload, fallback_payload_time_keys)?;
    Some(TimelineCommonFields {
        recorded_at: timing.display_at.clone(),
        sort_time: timing.sort_time,
        sort_key_raw: timing.display_at,
        time_source: timing.source,
        event_time_start: envelope.event_time_start.clone(),
        event_time_end: envelope.event_time_end.clone(),
        ingested_at: envelope.ingested_at.clone(),
        durability: canonical_or_payload_field(&envelope.durability, payload, &["durability"]),
        lane_id: canonical_or_payload_field(
            &envelope.lane_id,
            payload,
            &["lane_id", "laneID", "laneId"],
        ),
        stream_id: canonical_or_payload_field(
            &envelope.stream_id,
            payload,
            &["stream_id", "streamID", "streamId"],
        ),
        source_record_id: canonical_or_payload_field(
            &envelope.source_record_id,
            payload,
            &["source_record_id", "sourceRecordID", "sourceRecordId"],
        ),
        source_hash: canonical_or_payload_field(
            &envelope.source_hash,
            payload,
            &["source_hash", "sourceHash"],
        ),
        capture_bundle_id: canonical_or_payload_field(
            &envelope.capture_bundle_id,
            payload,
            &["capture_bundle_id", "captureBundleID", "captureBundleId"],
        ),
        privacy_class: canonical_or_payload_field(
            &envelope.privacy_class,
            payload,
            &["privacy_class", "privacyClass"],
        ),
        shape: canonical_or_payload_field(&envelope.shape, payload, &["shape"]),
        source_file: source_file_label(source_file),
    })
}

fn timeline_event_with_common(
    common: TimelineCommonFields,
    source_kind: String,
    event_type: String,
    headline: String,
    detail: Option<String>,
    window_id: Option<u64>,
    target: Option<String>,
) -> TimelineEvent {
    TimelineEvent {
        recorded_at: common.recorded_at,
        sort_time: common.sort_time,
        sort_key_raw: common.sort_key_raw,
        time_source: common.time_source,
        event_time_start: common.event_time_start,
        event_time_end: common.event_time_end,
        ingested_at: common.ingested_at,
        source_kind,
        event_type,
        headline,
        detail,
        window_id,
        target,
        durability: common.durability,
        lane_id: common.lane_id,
        stream_id: common.stream_id,
        source_record_id: common.source_record_id,
        source_hash: common.source_hash,
        capture_bundle_id: common.capture_bundle_id,
        privacy_class: common.privacy_class,
        shape: common.shape,
        source_file: common.source_file,
    }
}

fn canonical_or_payload_field(
    envelope_value: &Option<String>,
    payload: &Value,
    payload_keys: &[&str],
) -> Option<String> {
    envelope_value
        .clone()
        .or_else(|| string_field(payload, payload_keys))
}

fn select_event_time(
    envelope: &CaptureEventEnvelopeValue,
    payload: &Value,
    fallback_payload_keys: &[&str],
) -> Option<EventTimeSelection> {
    for (source, value) in [
        ("event_time_start", envelope.event_time_start.as_deref()),
        ("event_time_end", envelope.event_time_end.as_deref()),
        ("recordedAt", envelope.recorded_at.as_deref()),
    ] {
        if let Some(value) = non_empty_string(value) {
            return Some(event_time_selection(source, value));
        }
    }

    for key in fallback_payload_keys {
        if let Some(value) = string_field(payload, &[*key]) {
            if !value.trim().is_empty() {
                return Some(event_time_selection(*key, value));
            }
        }
    }

    non_empty_string(envelope.ingested_at.as_deref())
        .map(|value| event_time_selection("ingested_at", value))
}

fn event_time_selection(source: impl Into<String>, value: String) -> EventTimeSelection {
    EventTimeSelection {
        display_at: value.clone(),
        sort_time: parse_event_timestamp(&value),
        source: source.into(),
    }
}

fn non_empty_string(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

fn value_field<'a>(value: &'a Value, keys: &[&str]) -> Option<&'a Value> {
    keys.iter().find_map(|key| value.get(*key))
}

fn string_field(value: &Value, keys: &[&str]) -> Option<String> {
    let value = value_field(value, keys)?;
    if let Some(value) = value.as_str() {
        return Some(value.to_string());
    }
    if value.is_number() || value.is_boolean() {
        return Some(value.to_string());
    }
    None
}

fn number_field(value: &Value, keys: &[&str]) -> Option<f64> {
    let value = value_field(value, keys)?;
    value
        .as_f64()
        .or_else(|| value.as_i64().map(|value| value as f64))
        .or_else(|| value.as_u64().map(|value| value as f64))
        .or_else(|| value.as_str().and_then(|value| value.parse::<f64>().ok()))
}

fn bool_field(value: &Value, keys: &[&str]) -> Option<bool> {
    let value = value_field(value, keys)?;
    if let Some(value) = value.as_bool() {
        return Some(value);
    }
    let value = value.as_str()?.trim();
    if value.eq_ignore_ascii_case("true") || value == "1" || value.eq_ignore_ascii_case("yes") {
        return Some(true);
    }
    if value.eq_ignore_ascii_case("false") || value == "0" || value.eq_ignore_ascii_case("no") {
        return Some(false);
    }
    None
}

fn integer_field(value: &Value, keys: &[&str]) -> Option<u64> {
    let value = value_field(value, keys)?;
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|value| u64::try_from(value).ok()))
        .or_else(|| value.as_str().and_then(|value| value.parse::<u64>().ok()))
}

fn compact_path(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string())
}

fn abbreviate(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut shortened = value.chars().take(max_chars).collect::<String>();
    shortened.push_str("...");
    shortened
}

fn capture_status_motion_hints(status: &CaptureStatus) -> Option<&CaptureMotionHints> {
    status.motion_hints.as_ref().or_else(|| {
        status
            .ux_event_tap
            .as_ref()
            .and_then(|tap| tap.motion_hints.as_ref())
    })
}

fn recent_motion_target_window<'a>(
    snapshot: &'a CaptureSnapshot,
    status: &CaptureStatus,
) -> Option<&'a CaptureWindowState> {
    let pid = capture_status_motion_hints(status)?.recent_target_process_id?;
    snapshot
        .windows
        .iter()
        .filter(|window| window.app_pid == Some(pid) && should_list_window(window))
        .min_by_key(|window| {
            (
                if window.is_focused { 0 } else { 1 },
                window.z_rank.unwrap_or(i64::MAX),
                window.window_id.unwrap_or(0),
            )
        })
}

fn collect_capability_signals(value: &Value) -> Vec<CapabilitySignal> {
    let mut signals = Vec::new();
    let mut seen = BTreeSet::new();
    let direct_keys = [
        "screen_recording",
        "screenRecording",
        "system_audio",
        "systemAudio",
        "screen_audio",
        "screenAudio",
        "accessibility",
        "input_monitoring",
        "inputMonitoring",
        "browser_extension",
        "browserExtension",
        "microphone",
        "automation",
        "full_disk_access",
        "fullDiskAccess",
    ];

    for key in direct_keys {
        if let Some(signal) = value
            .get(key)
            .map(|field| capability_signal_from_value(normalize_capability_name(key), field))
        {
            if seen.insert(signal.name.clone()) {
                signals.push(signal);
            }
        }
    }

    for container_key in [
        "capabilities",
        "capability_signals",
        "capabilitySignals",
        "permissions",
        "remembering_permissions",
        "rememberingPermissions",
    ] {
        let Some(object) = value.get(container_key).and_then(Value::as_object) else {
            continue;
        };
        for (key, field) in object {
            let signal = capability_signal_from_value(normalize_capability_name(key), field);
            if seen.insert(signal.name.clone()) {
                signals.push(signal);
            }
        }
    }

    if let Some(object) = value_field(
        value,
        &["permission_derived_metadata", "permissionDerivedMetadata"],
    )
    .and_then(|metadata| value_field(metadata, &["signals"]))
    .and_then(Value::as_object)
    {
        for (key, field) in object {
            let signal = capability_signal_from_value(normalize_capability_name(key), field);
            if seen.insert(signal.name.clone()) {
                signals.push(signal);
            }
        }
    }

    signals
}

fn capability_signal_from_value(name: String, value: &Value) -> CapabilitySignal {
    if let Some(ready) = value.as_bool() {
        return CapabilitySignal {
            name,
            ready: Some(ready),
            ..CapabilitySignal::default()
        };
    }
    if let Some(status) = value.as_str() {
        return CapabilitySignal {
            name,
            status: Some(status.to_string()),
            ..CapabilitySignal::default()
        };
    }
    CapabilitySignal {
        name,
        ready: bool_field(
            value,
            &[
                "ready",
                "granted",
                "available",
                "enabled",
                "trusted",
                "active",
                "is_trusted",
                "isTrusted",
            ],
        ),
        status: string_field(value, &["status", "state", "result"]),
        detail: string_field(
            value,
            &[
                "detail",
                "message",
                "reason",
                "error",
                "last_error",
                "lastError",
            ],
        ),
        source: string_field(value, &["source", "surface"]),
    }
}

fn normalize_capability_name(name: &str) -> String {
    let mut normalized = String::new();
    for (index, ch) in name.chars().enumerate() {
        if ch.is_ascii_uppercase() && index > 0 {
            normalized.push('_');
        }
        normalized.push(ch.to_ascii_lowercase());
    }
    normalized.replace("__", "_")
}

fn capture_preview(
    snapshot: &CaptureSnapshot,
    target: PreviewTarget,
    max_dimension: u32,
) -> Result<PreviewFrame> {
    match preview_capture_plan(snapshot, target) {
        PreviewCapturePlan::Display(reason) => fallback_display(reason, max_dimension),
        PreviewCapturePlan::Window { window_id, reason } => {
            capture_window(window_id, max_dimension)
                .map(|mut frame| {
                    frame.source = format!("{reason}: #{window_id}");
                    frame
                })
                .with_context(|| format!("{reason} #{window_id} unavailable"))
        }
        PreviewCapturePlan::Refuse(reason) => Err(anyhow!(reason)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewCapturePlan {
    Display(&'static str),
    Window {
        window_id: u32,
        reason: &'static str,
    },
    Refuse(&'static str),
}

fn preview_capture_plan(snapshot: &CaptureSnapshot, target: PreviewTarget) -> PreviewCapturePlan {
    match target {
        PreviewTarget::MainDisplay => PreviewCapturePlan::Display("explicit main display target"),
        PreviewTarget::FocusedWindow => {
            if let Some(window_id) = focused_window_id(snapshot) {
                PreviewCapturePlan::Window {
                    window_id,
                    reason: "focused non-dashboard window",
                }
            } else if focused_window_is_dashboard(snapshot) {
                PreviewCapturePlan::Refuse(
                    "dashboard is focused; focus another app or choose Top Window/Main Display",
                )
            } else {
                PreviewCapturePlan::Refuse("no focused non-dashboard capture candidate")
            }
        }
        PreviewTarget::TopWindow => {
            if let Some(window_id) = top_capture_window_id(snapshot) {
                PreviewCapturePlan::Window {
                    window_id,
                    reason: "top capture-eligible non-dashboard window",
                }
            } else if focused_window_is_dashboard(snapshot) {
                PreviewCapturePlan::Refuse(
                    "dashboard is focused and no other capture-eligible window is available",
                )
            } else {
                PreviewCapturePlan::Refuse("no top capture-eligible non-dashboard window")
            }
        }
        PreviewTarget::Auto => {
            if let Some(window_id) =
                selected_window_id(snapshot).or_else(|| top_capture_window_id(snapshot))
            {
                PreviewCapturePlan::Window {
                    window_id,
                    reason: "auto non-dashboard window",
                }
            } else if focused_window_is_dashboard(snapshot) {
                PreviewCapturePlan::Refuse(
                    "dashboard is focused and auto found no non-dashboard capture target",
                )
            } else {
                PreviewCapturePlan::Refuse("auto found no non-dashboard capture target")
            }
        }
    }
}

fn fallback_display(reason: &str, max_dimension: u32) -> Result<PreviewFrame> {
    let mut frame = capture_primary_monitor(max_dimension)?;
    frame.source = format!("main display fallback; {reason}");
    Ok(frame)
}

fn selected_window_id(snapshot: &CaptureSnapshot) -> Option<u32> {
    focused_window_id(snapshot)
}

fn focused_window_id(snapshot: &CaptureSnapshot) -> Option<u32> {
    focused_window_candidate(snapshot).and_then(|window| window.window_id)
}

fn focused_window_candidate(snapshot: &CaptureSnapshot) -> Option<&CaptureWindowState> {
    snapshot
        .windows
        .iter()
        .filter(|window| window.is_focused && should_list_window(window))
        .min_by_key(|window| {
            (
                window.z_rank.unwrap_or(i64::MAX),
                window.window_id.unwrap_or(0),
            )
        })
}

fn focused_window_is_dashboard(snapshot: &CaptureSnapshot) -> bool {
    snapshot
        .windows
        .iter()
        .any(|window| window.is_focused && is_dashboard_window(window))
}

fn capture_window(window_id: u32, max_dimension: u32) -> Result<PreviewFrame> {
    let list_started = Instant::now();
    let windows = Window::all().context("list windows through xcap")?;
    let xcap_list_ms = list_started.elapsed().as_millis();
    let window = windows
        .into_iter()
        .find(|window| window.id().ok() == Some(window_id))
        .ok_or_else(|| anyhow!("xcap did not list window"))?;
    let capture_started = Instant::now();
    let image = window
        .capture_image()
        .context("capture focused window through xcap")?;
    let xcap_capture_ms = capture_started.elapsed().as_millis();
    let mut frame = frame_from_rgba(
        image,
        format!("focused window #{window_id}"),
        PreviewCoordinateSpace::Window,
        max_dimension,
    );
    frame.timing.xcap_list_ms = Some(xcap_list_ms);
    frame.timing.xcap_capture_ms = Some(xcap_capture_ms);
    Ok(frame)
}

fn capture_primary_monitor(max_dimension: u32) -> Result<PreviewFrame> {
    let list_started = Instant::now();
    let monitors = Monitor::all().context("list monitors through xcap")?;
    let xcap_list_ms = list_started.elapsed().as_millis();
    let monitor = monitors
        .iter()
        .find(|monitor| monitor.is_primary().unwrap_or(false))
        .or_else(|| monitors.first())
        .ok_or_else(|| anyhow!("no monitors available"))?;
    let name = monitor
        .friendly_name()
        .or_else(|_| monitor.name())
        .unwrap_or_else(|_| "display".to_string());
    let capture_started = Instant::now();
    let image = monitor
        .capture_image()
        .context("capture main display through xcap")?;
    let xcap_capture_ms = capture_started.elapsed().as_millis();
    let mut frame = frame_from_rgba(
        image,
        format!("main display: {name}"),
        PreviewCoordinateSpace::Display,
        max_dimension,
    );
    frame.timing.xcap_list_ms = Some(xcap_list_ms);
    frame.timing.xcap_capture_ms = Some(xcap_capture_ms);
    Ok(frame)
}

fn frame_from_rgba(
    image: RgbaImage,
    source: String,
    space: PreviewCoordinateSpace,
    max_dimension: u32,
) -> PreviewFrame {
    let resize_started = Instant::now();
    let original_size = [image.width() as usize, image.height() as usize];
    let image = downscale_preview_image(image, max_dimension);
    let resize_ms = resize_started.elapsed().as_millis();
    let size = [image.width() as usize, image.height() as usize];
    let color_started = Instant::now();
    let pixels = image.into_raw();
    let image = ColorImage::from_rgba_unmultiplied(size, &pixels);
    let color_image_ms = color_started.elapsed().as_millis();
    PreviewFrame {
        image,
        size,
        source,
        space,
        timing: RefreshTiming {
            resize_ms: Some(resize_ms),
            color_image_ms: Some(color_image_ms),
            input_pixels: Some(original_size),
            output_pixels: Some(size),
            ..RefreshTiming::default()
        },
    }
}

fn downscale_preview_image(image: RgbaImage, max_dimension: u32) -> RgbaImage {
    let width = image.width();
    let height = image.height();
    let largest = width.max(height);
    if max_dimension == 0 || largest <= max_dimension {
        return image;
    }
    let scale = max_dimension as f32 / largest as f32;
    let target_width = ((width as f32 * scale).round() as u32).max(1);
    let target_height = ((height as f32 * scale).round() as u32).max(1);
    imageops::resize(
        &image,
        target_width,
        target_height,
        imageops::FilterType::Triangle,
    )
}

fn resolve_cli_path() -> PathBuf {
    if let Ok(path) = env::var("ONECONTEXT_CLI") {
        return PathBuf::from(path);
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join("1context-cli");
            if sibling.exists() {
                return sibling;
            }
        }
    }
    let installed = PathBuf::from("/Applications/1Context Dev.app/Contents/MacOS/1context-cli");
    if installed.exists() {
        return installed;
    }
    PathBuf::from("macos/.build/debug/1context")
}

fn is_dashboard_app_name(name: &str) -> bool {
    DASHBOARD_APP_NAMES
        .iter()
        .any(|dashboard_name| name.eq_ignore_ascii_case(dashboard_name))
}

fn is_dashboard_window(window: &CaptureWindowState) -> bool {
    window
        .app_name
        .as_deref()
        .map(is_dashboard_app_name)
        .unwrap_or(false)
        || window
            .title
            .as_deref()
            .map(|title| title.contains("1Context Capture Dashboard"))
            .unwrap_or(false)
}

fn should_draw_overlay(window: &CaptureWindowState) -> bool {
    if !should_list_window(window) {
        return false;
    }
    let Some(frame) = &window.frame_points else {
        return false;
    };
    frame.width >= 220.0 && frame.height >= 160.0
}

fn overlay_candidates<'a>(
    snapshot: &'a CaptureSnapshot,
    display_frame: &CaptureRect,
    limit: usize,
) -> Vec<&'a CaptureWindowState> {
    if limit == 0 {
        return Vec::new();
    }
    let mut windows: Vec<_> = snapshot
        .windows
        .iter()
        .filter(|window| {
            should_draw_overlay(window)
                && window
                    .frame_points
                    .as_ref()
                    .map(|frame| intersects(frame, display_frame))
                    .unwrap_or(false)
        })
        .collect();
    windows.sort_by_key(|window| {
        (
            window.z_rank.unwrap_or(i64::MAX),
            window.window_id.unwrap_or(0),
        )
    });
    windows.truncate(limit);
    windows
}

fn overlay_rect(
    window: &CaptureWindowState,
    display_frame: &CaptureRect,
    image_rect: Rect,
    sx: f32,
    sy: f32,
) -> Option<Rect> {
    let frame = clipped_to(frame_rect(window)?, display_frame)?;
    let left = image_rect.left() + ((frame.x - display_frame.x) as f32 * sx);
    let top = image_rect.top() + ((frame.y - display_frame.y) as f32 * sy);
    let width = frame.width as f32 * sx;
    let height = frame.height as f32 * sy;
    if width <= 2.0 || height <= 2.0 {
        return None;
    }
    Some(Rect::from_min_size(
        Pos2::new(left, top),
        Vec2::new(width, height),
    ))
}

fn frame_rect(window: &CaptureWindowState) -> Option<&CaptureRect> {
    window.frame_points.as_ref()
}

fn should_list_window(window: &CaptureWindowState) -> bool {
    if !window.capture_eligible || !window.is_on_screen || is_dashboard_window(window) {
        return false;
    }
    if window.is_minimized || is_system_window(window) {
        return false;
    }
    if window.layer.unwrap_or(0) != 0 {
        return false;
    }
    if window.alpha.map(|alpha| alpha <= 0.0).unwrap_or(false) {
        return false;
    }
    if window
        .visible_fraction_estimate
        .map(|visible| visible <= 0.0)
        .unwrap_or(false)
    {
        return false;
    }
    let Some(frame) = &window.frame_points else {
        return false;
    };
    frame.width >= 120.0 && frame.height >= 80.0
}

fn is_system_window(window: &CaptureWindowState) -> bool {
    matches!(
        window.app_name.as_deref(),
        Some(
            "Window Server" | "Dock" | "Control Center" | "Notification Center" | "SystemUIServer"
        )
    ) || matches!(
        window.bundle_id.as_deref(),
        Some(
            "com.apple.WindowServer"
                | "com.apple.dock"
                | "com.apple.controlcenter"
                | "com.apple.notificationcenterui"
                | "com.apple.systemuiserver"
        )
    )
}

fn intersects(a: &CaptureRect, b: &CaptureRect) -> bool {
    a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y
}

fn clipped_to(rect: &CaptureRect, bounds: &CaptureRect) -> Option<CaptureRect> {
    let left = rect.x.max(bounds.x);
    let top = rect.y.max(bounds.y);
    let right = (rect.x + rect.width).min(bounds.x + bounds.width);
    let bottom = (rect.y + rect.height).min(bounds.y + bounds.height);
    if right <= left || bottom <= top {
        return None;
    }
    Some(CaptureRect {
        x: left,
        y: top,
        width: right - left,
        height: bottom - top,
    })
}

fn top_capture_window_id(snapshot: &CaptureSnapshot) -> Option<u32> {
    let mut windows: Vec<_> = snapshot
        .windows
        .iter()
        .filter(|window| should_list_window(window))
        .collect();
    windows.sort_by_key(|window| {
        (
            window.z_rank.unwrap_or(i64::MAX),
            window.window_id.unwrap_or(0),
        )
    });
    windows.first().and_then(|window| window.window_id)
}

#[derive(Debug, Clone, Default)]
struct LiveEventState {
    root_directory: Option<PathBuf>,
    events_directory: Option<PathBuf>,
    metadata_sample_fusion: Option<MetadataSampleFusion>,
    capture_status: Option<CaptureStatus>,
    timeline: Vec<TimelineEvent>,
    recent_ux: Vec<UXAnchorEvent>,
    recent_frames: Vec<FrameMetadataEvent>,
    source_files: Vec<String>,
    scanned_lines: usize,
    refresh_ms: Option<u128>,
    error: Option<String>,
}

impl LiveEventState {
    fn apply_capture_status(&mut self, status: CaptureStatus) {
        self.capture_status = Some(status.clone());
        if status.root_directory.is_some() {
            self.root_directory = status.root_directory;
        }
        if status.events_directory.is_some() {
            self.events_directory = status.events_directory;
        }
        if status.metadata_sample_fusion.is_some() {
            self.metadata_sample_fusion = status.metadata_sample_fusion;
        }
    }
}

#[derive(Debug, Clone, Default)]
struct BundleInventoryState {
    capture_root: Option<PathBuf>,
    live_count: usize,
    processing_count: usize,
    failed_count: usize,
    pinned_count: usize,
    total_bytes: u64,
    total_files: u64,
    recent_ready: Vec<BundleEntry>,
    sweep_delete_count: Option<usize>,
    sweep_move_count: Option<usize>,
    sweep_reclaimable_bytes: Option<u64>,
    refresh_ms: Option<u128>,
    action_status: Option<String>,
    error: Option<String>,
}

#[derive(Debug, Clone, Default)]
struct RecentCaptureEvents {
    timeline: Vec<TimelineEvent>,
    ux: Vec<UXAnchorEvent>,
    frames: Vec<FrameMetadataEvent>,
    source_files: Vec<String>,
    scanned_lines: usize,
}

impl RecentCaptureEvents {
    fn sort_and_cap_timeline(&mut self) {
        self.timeline.sort_by(|lhs, rhs| {
            rhs.sort_time
                .cmp(&lhs.sort_time)
                .then_with(|| rhs.sort_key_raw.cmp(&lhs.sort_key_raw))
                .then_with(|| rhs.event_type.cmp(&lhs.event_type))
                .then_with(|| rhs.source_file.cmp(&lhs.source_file))
        });
        self.timeline.truncate(TIMELINE_PANEL_LIMIT);
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
struct EventTimestamp {
    epoch_seconds: i64,
    nanos: u32,
}

#[derive(Debug, Clone)]
struct EventTimeSelection {
    display_at: String,
    sort_time: Option<EventTimestamp>,
    source: String,
}

#[derive(Debug, Clone)]
struct TimelineCommonFields {
    recorded_at: String,
    sort_time: Option<EventTimestamp>,
    sort_key_raw: String,
    time_source: String,
    event_time_start: Option<String>,
    event_time_end: Option<String>,
    ingested_at: Option<String>,
    durability: Option<String>,
    lane_id: Option<String>,
    stream_id: Option<String>,
    source_record_id: Option<String>,
    source_hash: Option<String>,
    capture_bundle_id: Option<String>,
    privacy_class: Option<String>,
    shape: Option<String>,
    source_file: String,
}

#[derive(Debug, Clone)]
struct TimelineEvent {
    recorded_at: String,
    sort_time: Option<EventTimestamp>,
    sort_key_raw: String,
    time_source: String,
    event_time_start: Option<String>,
    event_time_end: Option<String>,
    ingested_at: Option<String>,
    source_kind: String,
    event_type: String,
    headline: String,
    detail: Option<String>,
    window_id: Option<u64>,
    target: Option<String>,
    durability: Option<String>,
    lane_id: Option<String>,
    stream_id: Option<String>,
    source_record_id: Option<String>,
    source_hash: Option<String>,
    capture_bundle_id: Option<String>,
    privacy_class: Option<String>,
    shape: Option<String>,
    source_file: String,
}

#[derive(Debug, Clone)]
struct UXAnchorEvent {
    event_type: String,
    recorded_at: String,
    timeline_common: TimelineCommonFields,
    started_at: Option<String>,
    ended_at: Option<String>,
    kind: String,
    source: Option<String>,
    recent_target_process_id: Option<String>,
    scroll: Option<UXScrollSummary>,
    keyboard: Option<UXKeyboardSummary>,
    pointer: Option<UXPointerEventSummary>,
    shortcut: Option<UXShortcutSummary>,
    focus_transition: Option<UXFocusTransitionSummary>,
}

#[derive(Debug, Clone)]
struct UXScrollSummary {
    event_count: Option<u64>,
    total_dx: f64,
    total_dy: f64,
}

#[derive(Debug, Clone)]
struct UXKeyboardSummary {
    event_count: Option<u64>,
    key_down_count: Option<u64>,
    auto_repeat_count: Option<u64>,
}

#[derive(Debug, Clone)]
struct UXPointerEventSummary {
    action: Option<String>,
    button: Option<String>,
    event_count: Option<u64>,
}

#[derive(Debug, Clone)]
struct UXShortcutSummary {
    event_count: Option<u64>,
    key_down_count: Option<u64>,
    auto_repeat_count: Option<u64>,
    categories: Vec<String>,
}

#[derive(Debug, Clone)]
struct UXFocusTransitionSummary {
    previous_process_id: Option<String>,
    current_process_id: Option<String>,
    trigger: Option<String>,
    confidence: Option<String>,
}

#[derive(Debug, Clone)]
struct FrameMetadataEvent {
    event_type: String,
    recorded_at: String,
    sequence: Option<u64>,
    frame_status: Option<String>,
    dirty_area_ratio: Option<f64>,
    dirty_rect_count: Option<u64>,
    changed_tile_ratio: Option<f64>,
    estimated_dy: Option<f64>,
    motion_classification: Option<String>,
    motion_mode: Option<String>,
    feeds_motion_classifier: Option<bool>,
    ux_motion_hints_fused: Option<bool>,
    ux_motion_hints_present: bool,
    pixels_untouched: Option<bool>,
    adaptive_decision: Option<FrameAdaptiveDecision>,
    target_window_id: Option<u64>,
    target_app_name: Option<String>,
    target_title: Option<String>,
}

impl FrameMetadataEvent {
    fn target_label(&self) -> String {
        let id = self
            .target_window_id
            .map(|id| format!("#{id} "))
            .unwrap_or_default();
        let app = self.target_app_name.as_deref().unwrap_or("unknown");
        let title = self
            .target_title
            .as_deref()
            .filter(|title| !title.is_empty())
            .unwrap_or("(untitled)");
        format!("{id}{app} - {title}")
    }

    fn motion_label(&self) -> Option<String> {
        match (&self.motion_classification, &self.motion_mode) {
            (Some(classification), Some(mode)) if classification != mode => {
                Some(format!("{classification}/{mode}"))
            }
            (Some(classification), _) => Some(classification.clone()),
            (_, Some(mode)) => Some(mode.clone()),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
struct FrameAdaptiveDecision {
    classifier_mode: Option<String>,
    controller_mode: Option<String>,
    target_fps: Option<u64>,
    target_analysis_fps: Option<u64>,
    should_update_stream_configuration: Option<bool>,
    update_reason: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CaptureStatus {
    #[serde(default)]
    surface: Option<String>,
    #[serde(default, rename = "root_directory", alias = "rootDirectory")]
    root_directory: Option<PathBuf>,
    #[serde(default, rename = "events_directory", alias = "eventsDirectory")]
    events_directory: Option<PathBuf>,
    #[serde(default, rename = "windows_directory", alias = "windowsDirectory")]
    windows_directory: Option<PathBuf>,
    #[serde(default, rename = "media_directory", alias = "mediaDirectory")]
    media_directory: Option<PathBuf>,
    #[serde(default, rename = "window_unit_of_truth", alias = "windowUnitOfTruth")]
    window_unit_of_truth: Option<bool>,
    #[serde(default, rename = "available_methods", alias = "availableMethods")]
    available_methods: Vec<String>,
    #[serde(default, rename = "motion_hints", alias = "motionHints")]
    motion_hints: Option<CaptureMotionHints>,
    #[serde(
        default,
        rename = "metadata_sample_fusion",
        alias = "metadataSampleFusion"
    )]
    metadata_sample_fusion: Option<MetadataSampleFusion>,
    #[serde(default, rename = "ux_event_tap", alias = "uxEventTap")]
    ux_event_tap: Option<CaptureUXEventTapStatus>,
    #[serde(default, rename = "permission_subject", alias = "permissionSubject")]
    permission_subject: Option<PermissionSubject>,
    #[serde(
        default,
        rename = "permission_derived_metadata",
        alias = "permissionDerivedMetadata"
    )]
    permission_derived_metadata: Option<PermissionDerivedMetadata>,
    #[serde(skip)]
    capability_signals: Vec<CapabilitySignal>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionDerivedMetadata {
    #[serde(default, rename = "schema_version", alias = "schemaVersion")]
    schema_version: Option<u64>,
    #[serde(default, rename = "generated_at", alias = "generatedAt")]
    generated_at: Option<String>,
    #[serde(default)]
    privacy: Option<PermissionMetadataPrivacy>,
    #[serde(default, rename = "process_identities", alias = "processIdentities")]
    process_identities: Vec<PermissionProcessIdentity>,
    #[serde(default, rename = "capture_paths", alias = "capturePaths")]
    capture_paths: Option<PermissionCapturePaths>,
    #[serde(default)]
    signals: BTreeMap<String, PermissionSignalMetadata>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionMetadataPrivacy {
    #[serde(
        default,
        rename = "raw_keystrokes_included",
        alias = "rawKeystrokesIncluded"
    )]
    raw_keystrokes_included: Option<bool>,
    #[serde(default, rename = "raw_text_included", alias = "rawTextIncluded")]
    raw_text_included: Option<bool>,
    #[serde(
        default,
        rename = "coordinates_included",
        alias = "coordinatesIncluded"
    )]
    coordinates_included: Option<bool>,
    #[serde(
        default,
        rename = "aggregates_and_counts_only",
        alias = "aggregatesAndCountsOnly"
    )]
    aggregates_and_counts_only: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionProcessIdentity {
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    pid: Option<i64>,
    #[serde(default, rename = "executable_path", alias = "executablePath")]
    executable_path: Option<String>,
    #[serde(default, rename = "bundle_identifier", alias = "bundleIdentifier")]
    bundle_identifier: Option<String>,
    #[serde(default, rename = "app_version", alias = "appVersion")]
    app_version: Option<String>,
    #[serde(
        default,
        rename = "designated_requirement_sha256",
        alias = "designatedRequirementSHA256"
    )]
    designated_requirement_sha256: Option<String>,
}

impl PermissionProcessIdentity {
    fn compact_label(&self) -> String {
        let role = self.role.as_deref().unwrap_or("identity");
        let mut parts = Vec::new();
        if let Some(pid) = self.pid {
            parts.push(format!("pid={pid}"));
        }
        if let Some(bundle) = &self.bundle_identifier {
            parts.push(format!("bundle={bundle}"));
        }
        if let Some(path) = &self.executable_path {
            parts.push(format!("exe={}", compact_path(path)));
        }
        if let Some(version) = &self.app_version {
            parts.push(format!("version={version}"));
        }
        if let Some(hash) = &self.designated_requirement_sha256 {
            parts.push(format!("dr={}", abbreviate(hash, 12)));
        }
        if parts.is_empty() {
            role.to_string()
        } else {
            format!("{role}({})", parts.join(" "))
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionCapturePaths {
    #[serde(default, rename = "root_directory", alias = "rootDirectory")]
    root_directory: Option<String>,
    #[serde(default, rename = "events_directory", alias = "eventsDirectory")]
    events_directory: Option<String>,
    #[serde(default, rename = "windows_directory", alias = "windowsDirectory")]
    windows_directory: Option<String>,
    #[serde(default, rename = "media_directory", alias = "mediaDirectory")]
    media_directory: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionSignalMetadata {
    #[serde(default)]
    ready: Option<bool>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default, rename = "owner_role", alias = "ownerRole")]
    owner_role: Option<String>,
    #[serde(
        default,
        rename = "permission_subject_role",
        alias = "permissionSubjectRole"
    )]
    permission_subject_role: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default, rename = "focused_context", alias = "focusedContext")]
    focused_context: Option<PermissionFocusedContext>,
    #[serde(default, rename = "event_tap", alias = "eventTap")]
    event_tap: Option<PermissionEventTap>,
    #[serde(default)]
    proof: Option<PermissionProof>,
}

impl PermissionSignalMetadata {
    fn compact_label(&self, name: &str) -> String {
        let mut value = self
            .status
            .clone()
            .or_else(|| self.ready.map(yes_no).map(ToOwned::to_owned))
            .unwrap_or_else(|| "present".to_string());
        if let Some(owner) = &self.owner_role {
            value.push_str(&format!(" owner={owner}"));
        }
        if let Some(source) = &self.source {
            value.push_str(&format!(" source={}", abbreviate(source, 40)));
        }
        if let Some(context) = &self.focused_context {
            value.push_str(&format!(
                " ax_trusted={}",
                context.trusted.map(yes_no).unwrap_or("unknown")
            ));
            if let Some(available) = context.available {
                value.push_str(&format!(" ax_available={}", yes_no(available)));
            }
            if let Some(status) = &context.status {
                value.push_str(&format!(" ax_status={status}"));
            }
            if let Some(source) = &context.source {
                value.push_str(&format!(" ax_source={}", abbreviate(source, 28)));
            }
        }
        if let Some(tap) = &self.event_tap {
            value.push_str(&format!(
                " tap={} events={}",
                tap.active.map(yes_no).unwrap_or("unknown"),
                display_count(tap.observed_event_count.map(|value| value as u64))
            ));
            if let Some(lifecycle) = &tap.lifecycle_state {
                value.push_str(&format!(" lifecycle={lifecycle}"));
            }
            if let Some(event_tap) = &tap.event_tap {
                value.push_str(&format!(" event_tap={event_tap}"));
            }
            if let Some(options) = &tap.tap_options {
                value.push_str(&format!(" options={options}"));
            }
            if !tap.event_mask.is_empty() {
                value.push_str(&format!(" mask={}", tap.event_mask.len()));
            }
            if let Some(queue) = tap.queue_depth {
                value.push_str(&format!(" queue={queue}"));
            }
            if let Some(dropped) = tap.dropped_count {
                value.push_str(&format!(" dropped={dropped}"));
            }
            if let Some(coalesced) = tap.coalesced_count {
                value.push_str(&format!(" coalesced={coalesced}"));
            }
            if let Some(last) = &tap.last_event_at {
                value.push_str(&format!(" last={}", compact_timestamp(last)));
            }
        }
        if let Some(proof) = &self.proof {
            value.push_str(&format!(
                " proof={} match={}",
                yes_no(proof.recorded.unwrap_or(false)),
                yes_no(proof.matches_current_subject.unwrap_or(false))
            ));
            if let Some(key) = &proof.proof_key {
                value.push_str(&format!(" key={}", abbreviate(key, 28)));
            }
            if let Some(method) = &proof.method {
                value.push_str(&format!(" method={method}"));
            }
            if let Some(proved_at) = &proof.proved_at {
                value.push_str(&format!(" proved={}", compact_timestamp(proved_at)));
            }
        }
        if let Some(subject) = &self.permission_subject_role {
            value.push_str(&format!(" subject={subject}"));
        }
        if let Some(note) = &self.note {
            value.push_str(&format!(" note={}", abbreviate(note, 36)));
        }
        format!("{name}={value}")
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionFocusedContext {
    #[serde(default)]
    available: Option<bool>,
    #[serde(default)]
    trusted: Option<bool>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    source: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionEventTap {
    #[serde(default)]
    active: Option<bool>,
    #[serde(default, rename = "lifecycle_state", alias = "lifecycleState")]
    lifecycle_state: Option<String>,
    #[serde(default, rename = "event_tap", alias = "eventTap")]
    event_tap: Option<String>,
    #[serde(default, rename = "tap_options", alias = "tapOptions")]
    tap_options: Option<String>,
    #[serde(default, rename = "event_mask", alias = "eventMask")]
    event_mask: Vec<String>,
    #[serde(default, rename = "observed_event_count", alias = "observedEventCount")]
    observed_event_count: Option<i64>,
    #[serde(default, rename = "queue_depth", alias = "queueDepth")]
    queue_depth: Option<i64>,
    #[serde(default, rename = "dropped_count", alias = "droppedCount")]
    dropped_count: Option<i64>,
    #[serde(default, rename = "coalesced_count", alias = "coalescedCount")]
    coalesced_count: Option<i64>,
    #[serde(default, rename = "last_event_at", alias = "lastEventAt")]
    last_event_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionProof {
    #[serde(default, rename = "proof_key", alias = "proofKey")]
    proof_key: Option<String>,
    #[serde(default)]
    recorded: Option<bool>,
    #[serde(
        default,
        rename = "matches_current_subject",
        alias = "matchesCurrentSubject"
    )]
    matches_current_subject: Option<bool>,
    #[serde(default)]
    method: Option<String>,
    #[serde(default, rename = "proved_at", alias = "provedAt")]
    proved_at: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct MetadataSampleFusion {
    #[serde(
        default,
        rename = "ux_motion_hints_enabled",
        alias = "uxMotionHintsEnabled"
    )]
    ux_motion_hints_enabled: Option<bool>,
    #[serde(default)]
    source: Option<String>,
    #[serde(default, rename = "pixels_untouched", alias = "pixelsUntouched")]
    pixels_untouched: Option<bool>,
}

impl MetadataSampleFusion {
    fn summary_label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(enabled) = self.ux_motion_hints_enabled {
            parts.push(format!(
                "ux_hints={}",
                if enabled { "enabled" } else { "off" }
            ));
        }
        if let Some(source) = &self.source {
            parts.push(format!("source={source}"));
        }
        if let Some(untouched) = self.pixels_untouched {
            parts.push(format!(
                "pixels={}",
                if untouched { "untouched" } else { "sampled" }
            ));
        }
        if parts.is_empty() {
            "metadata fusion: n/a".to_string()
        } else {
            format!("metadata fusion: {}", parts.join(" | "))
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CaptureMotionHints {
    #[serde(default, rename = "generated_at", alias = "generatedAt")]
    generated_at: Option<String>,
    #[serde(
        default,
        rename = "scroll_event_recently",
        alias = "scrollEventRecently"
    )]
    scroll_event_recently: Option<bool>,
    #[serde(
        default,
        rename = "keyboard_activity_recently",
        alias = "keyboardActivityRecently"
    )]
    keyboard_activity_recently: Option<bool>,
    #[serde(default, rename = "estimated_scroll_dy", alias = "estimatedScrollDY")]
    estimated_scroll_dy: Option<f64>,
    #[serde(default, rename = "focused_recently", alias = "focusedRecently")]
    focused_recently: Option<bool>,
    #[serde(
        default,
        rename = "recent_target_process_id",
        alias = "recentTargetProcessID",
        alias = "recent_target_pid",
        alias = "recentTargetPID"
    )]
    recent_target_process_id: Option<i64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CaptureUXEventTapStatus {
    #[serde(default, rename = "startup_wired", alias = "startupWired")]
    startup_wired: Option<bool>,
    #[serde(default, rename = "lifecycle_state", alias = "lifecycleState")]
    lifecycle_state: Option<String>,
    #[serde(default, rename = "tap_active", alias = "tapActive")]
    tap_active: Option<bool>,
    #[serde(default, rename = "tap_owner_pid", alias = "tapOwnerPID")]
    tap_owner_pid: Option<i64>,
    #[serde(default, rename = "tap_owner_executable", alias = "tapOwnerExecutable")]
    tap_owner_executable: Option<String>,
    #[serde(default, rename = "tap_owner_bundle", alias = "tapOwnerBundle")]
    tap_owner_bundle: Option<String>,
    #[serde(default, rename = "event_tap", alias = "eventTap")]
    event_tap: Option<String>,
    #[serde(default, rename = "tap_options", alias = "tapOptions")]
    tap_options: Option<String>,
    #[serde(default, rename = "event_mask", alias = "eventMask")]
    event_mask: Vec<String>,
    #[serde(default, rename = "last_event_at", alias = "lastEventAt")]
    last_event_at: Option<String>,
    #[serde(default, rename = "disabled_count", alias = "disabledCount")]
    disabled_count: Option<u64>,
    #[serde(
        default,
        rename = "reenable_attempt_count",
        alias = "reenableAttemptCount"
    )]
    reenable_attempt_count: Option<u64>,
    #[serde(
        default,
        rename = "reenable_success_count",
        alias = "reenableSuccessCount"
    )]
    reenable_success_count: Option<u64>,
    #[serde(
        default,
        rename = "reenable_failure_count",
        alias = "reenableFailureCount"
    )]
    reenable_failure_count: Option<u64>,
    #[serde(default, rename = "dropped_count", alias = "droppedCount")]
    dropped_count: Option<u64>,
    #[serde(default, rename = "coalesced_count", alias = "coalescedCount")]
    coalesced_count: Option<u64>,
    #[serde(default, rename = "queue_depth", alias = "queueDepth")]
    queue_depth: Option<u64>,
    #[serde(default, rename = "observed_event_count", alias = "observedEventCount")]
    observed_event_count: Option<u64>,
    #[serde(default, rename = "callback_count", alias = "callbackCount")]
    callback_count: Option<u64>,
    #[serde(
        default,
        rename = "callback_last_us",
        alias = "callbackLastMicroseconds"
    )]
    callback_last_microseconds: Option<f64>,
    #[serde(
        default,
        rename = "callback_average_us",
        alias = "callbackAverageMicroseconds"
    )]
    callback_average_microseconds: Option<f64>,
    #[serde(default, rename = "callback_max_us", alias = "callbackMaxMicroseconds")]
    callback_max_microseconds: Option<f64>,
    #[serde(default, rename = "last_error", alias = "lastError")]
    last_error: Option<String>,
    #[serde(default, rename = "startup_error", alias = "startupError")]
    startup_error: Option<String>,
    #[serde(default)]
    note: Option<String>,
    #[serde(default, rename = "motion_hints", alias = "motionHints")]
    motion_hints: Option<CaptureMotionHints>,
    #[serde(default, rename = "permission_subject", alias = "permissionSubject")]
    permission_subject: Option<PermissionSubject>,
    #[serde(default, rename = "tcc_identity_note", alias = "tccIdentityNote")]
    tcc_identity_note: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct PermissionSubject {
    #[serde(default, rename = "bundle_identifier", alias = "bundleIdentifier")]
    bundle_identifier: Option<String>,
    #[serde(
        default,
        rename = "designated_requirement_sha256",
        alias = "designatedRequirementSHA256"
    )]
    designated_requirement_sha256: Option<String>,
    #[serde(default, rename = "executable_path", alias = "executablePath")]
    executable_path: Option<String>,
    #[serde(default, rename = "app_version", alias = "appVersion")]
    app_version: Option<String>,
}

impl PermissionSubject {
    fn compact_label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(bundle) = &self.bundle_identifier {
            parts.push(format!("bundle={bundle}"));
        }
        if let Some(path) = &self.executable_path {
            parts.push(format!("exe={}", compact_path(path)));
        }
        if let Some(version) = &self.app_version {
            parts.push(format!("version={version}"));
        }
        if let Some(hash) = &self.designated_requirement_sha256 {
            parts.push(format!("dr_sha256={}", abbreviate(hash, 12)));
        }
        if parts.is_empty() {
            "unknown".to_string()
        } else {
            parts.join(" ")
        }
    }
}

#[derive(Debug, Clone, Default)]
struct CapabilitySignal {
    name: String,
    ready: Option<bool>,
    status: Option<String>,
    detail: Option<String>,
    source: Option<String>,
}

impl CapabilitySignal {
    fn compact_label(&self) -> String {
        let mut value = self
            .status
            .clone()
            .or_else(|| self.ready.map(yes_no).map(ToOwned::to_owned))
            .unwrap_or_else(|| "present".to_string());
        if let Some(source) = &self.source {
            value.push_str(&format!("@{source}"));
        }
        if let Some(detail) = &self.detail {
            value.push_str(&format!(" ({})", abbreviate(detail, 48)));
        }
        format!("{}={value}", self.name)
    }
}

#[derive(Debug, Deserialize)]
struct CaptureEventEnvelopeValue {
    #[serde(default, rename = "eventType", alias = "event_type")]
    event_type: String,
    #[serde(default, rename = "event_time_start", alias = "eventTimeStart")]
    event_time_start: Option<String>,
    #[serde(default, rename = "event_time_end", alias = "eventTimeEnd")]
    event_time_end: Option<String>,
    #[serde(default, rename = "ingested_at", alias = "ingestedAt")]
    ingested_at: Option<String>,
    #[serde(default, rename = "recordedAt", alias = "recorded_at")]
    recorded_at: Option<String>,
    #[serde(default, rename = "lane_id", alias = "laneID", alias = "laneId")]
    lane_id: Option<String>,
    #[serde(default, rename = "stream_id", alias = "streamID", alias = "streamId")]
    stream_id: Option<String>,
    #[serde(
        default,
        rename = "source_record_id",
        alias = "sourceRecordID",
        alias = "sourceRecordId"
    )]
    source_record_id: Option<String>,
    #[serde(default, rename = "source_hash", alias = "sourceHash")]
    source_hash: Option<String>,
    #[serde(
        default,
        rename = "capture_bundle_id",
        alias = "captureBundleID",
        alias = "captureBundleId"
    )]
    capture_bundle_id: Option<String>,
    #[serde(default, rename = "privacy_class", alias = "privacyClass")]
    privacy_class: Option<String>,
    #[serde(default)]
    shape: Option<String>,
    #[serde(default)]
    durability: Option<String>,
    #[serde(default)]
    payload: Value,
}

struct PreviewFrame {
    image: ColorImage,
    size: [usize; 2],
    source: String,
    space: PreviewCoordinateSpace,
    timing: RefreshTiming,
}

#[derive(Debug, Clone, Default)]
struct RefreshTiming {
    snapshot_ms: Option<u128>,
    preview_total_ms: Option<u128>,
    xcap_list_ms: Option<u128>,
    xcap_capture_ms: Option<u128>,
    resize_ms: Option<u128>,
    color_image_ms: Option<u128>,
    texture_upload_ms: Option<u128>,
    total_ms: Option<u128>,
    input_pixels: Option<[usize; 2]>,
    output_pixels: Option<[usize; 2]>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewCoordinateSpace {
    Display,
    Window,
}

impl PreviewCoordinateSpace {
    fn label(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Window => "window",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PreviewTarget {
    Auto,
    MainDisplay,
    FocusedWindow,
    TopWindow,
}

impl PreviewTarget {
    fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::MainDisplay => "Main Display",
            Self::FocusedWindow => "Focused Window",
            Self::TopWindow => "Top Window",
        }
    }

    fn requires_fresh_snapshot_for_preview(self) -> bool {
        matches!(self, Self::Auto | Self::FocusedWindow | Self::TopWindow)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureSnapshot {
    generated_at: Option<String>,
    active_application: Option<CaptureActiveApplication>,
    #[serde(
        default,
        alias = "capture_status",
        deserialize_with = "deserialize_capture_status_option"
    )]
    capture_status: Option<CaptureStatus>,
    #[serde(default, alias = "focused_context")]
    focused_context: Option<CaptureFocusedContext>,
    #[serde(default)]
    displays: Vec<CaptureDisplayState>,
    #[serde(default)]
    windows: Vec<CaptureWindowState>,
}

impl CaptureSnapshot {
    fn main_display(&self) -> Option<&CaptureDisplayState> {
        self.displays
            .iter()
            .find(|display| display.is_main)
            .or_else(|| self.displays.first())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureActiveApplication {
    #[serde(rename = "processID")]
    process_id: Option<i64>,
    #[serde(rename = "bundleID")]
    bundle_id: Option<String>,
    app_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureDisplayState {
    #[serde(rename = "displayID")]
    display_id: Option<String>,
    frame_points: Option<CaptureRect>,
    scale_factor: Option<f64>,
    #[serde(default)]
    is_main: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureWindowState {
    #[serde(rename = "windowID")]
    window_id: Option<u32>,
    #[serde(rename = "appPID")]
    app_pid: Option<i64>,
    app_name: Option<String>,
    #[serde(rename = "bundleID")]
    bundle_id: Option<String>,
    title: Option<String>,
    frame_points: Option<CaptureRect>,
    frame_pixels: Option<CaptureRect>,
    #[serde(rename = "displayID")]
    display_id: Option<String>,
    layer: Option<i64>,
    alpha: Option<f64>,
    z_rank: Option<i64>,
    #[serde(default)]
    is_minimized: bool,
    #[serde(default)]
    is_focused: bool,
    #[serde(default)]
    is_on_screen: bool,
    #[serde(default)]
    capture_eligible: bool,
    visible_fraction_estimate: Option<f64>,
    source: Option<String>,
    focus_metadata: Option<CaptureWindowFocusMetadata>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureWindowFocusMetadata {
    source: Option<String>,
    status: Option<String>,
    confidence: Option<String>,
    #[serde(rename = "matchedWindowID")]
    matched_window_id: Option<u32>,
    #[serde(default)]
    match_signals: Vec<String>,
}

impl CaptureWindowFocusMetadata {
    fn compact_label(&self) -> String {
        let mut parts = Vec::new();
        parts.push(self.source.as_deref().unwrap_or("unknown").to_string());
        parts.push(self.status.as_deref().unwrap_or("unknown").to_string());
        parts.push(format!(
            "confidence={}",
            self.confidence.as_deref().unwrap_or("unknown")
        ));
        if let Some(id) = self.matched_window_id {
            parts.push(format!("matched=#{id}"));
        }
        if !self.match_signals.is_empty() {
            parts.push(format!("signals={}", self.match_signals.join(",")));
        }
        parts.join(" ")
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureFocusedContext {
    status: Option<String>,
    is_process_trusted: Option<bool>,
    active_application: Option<CaptureActiveApplication>,
    #[serde(rename = "focusedApplicationProcessID")]
    focused_application_process_id: Option<i64>,
    focused_window: Option<CaptureAXNodeContext>,
    focused_element: Option<CaptureAXNodeContext>,
    #[serde(rename = "matchedWindowID")]
    matched_window_id: Option<u32>,
    #[serde(default)]
    issues: Vec<CaptureFocusedContextIssue>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureAXNodeContext {
    role: Option<String>,
    subrole: Option<String>,
    title: Option<String>,
    identifier: Option<String>,
    element_description: Option<String>,
    value_shape: Option<CaptureAXValueShape>,
    selection: Option<CaptureAXSelectionContext>,
    #[serde(default)]
    is_sensitive: bool,
    #[serde(default)]
    redaction_reasons: Vec<String>,
}

impl CaptureAXNodeContext {
    fn compact_label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(role) = &self.role {
            parts.push(format!("role={role}"));
        }
        if let Some(subrole) = &self.subrole {
            parts.push(format!("subrole={subrole}"));
        }
        if let Some(title) = &self.title {
            parts.push(format!("title={}", abbreviate(title, 40)));
        }
        if let Some(identifier) = &self.identifier {
            parts.push(format!("id={}", abbreviate(identifier, 32)));
        }
        if let Some(description) = &self.element_description {
            parts.push(format!("desc={}", abbreviate(description, 40)));
        }
        if let Some(value_shape) = &self.value_shape {
            parts.push(format!("value={}", value_shape.compact_label()));
        }
        if let Some(selection) = &self.selection {
            parts.push(format!("selection={}", selection.compact_label()));
        }
        if self.is_sensitive {
            parts.push("sensitive=yes".to_string());
        }
        if !self.redaction_reasons.is_empty() {
            parts.push(format!("redacted={}", self.redaction_reasons.join(",")));
        }
        if parts.is_empty() {
            "unknown".to_string()
        } else {
            parts.join(" ")
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureAXValueShape {
    kind: Option<String>,
    character_count: Option<u64>,
    source_attribute: Option<String>,
    #[serde(default)]
    redacted: bool,
}

impl CaptureAXValueShape {
    fn compact_label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(kind) = &self.kind {
            parts.push(kind.clone());
        }
        if let Some(count) = self.character_count {
            parts.push(format!("{count} chars"));
        }
        if let Some(attribute) = &self.source_attribute {
            parts.push(format!("attr={attribute}"));
        }
        if self.redacted {
            parts.push("redacted".to_string());
        }
        if parts.is_empty() {
            "unknown".to_string()
        } else {
            parts.join(" ")
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureAXSelectionContext {
    selected_text_character_count: Option<u64>,
    #[serde(default)]
    selected_text_truncated: bool,
    #[serde(default)]
    selected_text_redacted: bool,
}

impl CaptureAXSelectionContext {
    fn compact_label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(count) = self.selected_text_character_count {
            parts.push(format!("{count} chars"));
        }
        if self.selected_text_truncated {
            parts.push("truncated".to_string());
        }
        if self.selected_text_redacted {
            parts.push("redacted".to_string());
        }
        if parts.is_empty() {
            "present".to_string()
        } else {
            parts.join(" ")
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaptureFocusedContextIssue {
    code: Option<String>,
    status: Option<String>,
    element: Option<String>,
    attribute: Option<String>,
    message: Option<String>,
}

impl CaptureFocusedContextIssue {
    fn compact_label(&self) -> String {
        let mut parts = Vec::new();
        if let Some(code) = &self.code {
            parts.push(code.clone());
        }
        if let Some(status) = &self.status {
            parts.push(format!("status={status}"));
        }
        if let Some(element) = &self.element {
            parts.push(format!("element={element}"));
        }
        if let Some(attribute) = &self.attribute {
            parts.push(format!("attr={attribute}"));
        }
        if let Some(message) = &self.message {
            parts.push(abbreviate(message, 56));
        }
        if parts.is_empty() {
            "unknown".to_string()
        } else {
            parts.join(" ")
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct CaptureRect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_app() -> CaptureDashboardApp {
        CaptureDashboardApp {
            cli_path: PathBuf::from("/tmp/1context-cli"),
            snapshot: None,
            preview: None,
            preview_size: [1, 1],
            preview_source: "stable".to_string(),
            preview_space: PreviewCoordinateSpace::Display,
            preview_target: PreviewTarget::Auto,
            last_snapshot_refresh: None,
            last_preview_refresh: None,
            snapshot_interval: DEFAULT_SNAPSHOT_REFRESH_SECONDS,
            preview_interval: DEFAULT_PREVIEW_REFRESH_SECONDS,
            preview_max_dimension: DEFAULT_PREVIEW_MAX_DIMENSION,
            auto_refresh: true,
            show_overlays: true,
            show_overlay_labels: false,
            overlay_limit: DEFAULT_OVERLAY_LIMIT,
            live_events: LiveEventState::default(),
            bundle_inventory: BundleInventoryState::default(),
            last_event_refresh: None,
            last_bundle_refresh: None,
            pending_capture_refresh: None,
            pending_live_event_refresh: None,
            pending_bundle_refresh: None,
            last_error: None,
            last_timing: None,
        }
    }

    fn snapshot(windows: Vec<CaptureWindowState>) -> CaptureSnapshot {
        CaptureSnapshot {
            generated_at: None,
            active_application: None,
            capture_status: None,
            focused_context: None,
            displays: vec![CaptureDisplayState {
                display_id: Some("main".to_string()),
                frame_points: Some(rect(0.0, 0.0, 1470.0, 956.0)),
                scale_factor: Some(2.0),
                is_main: true,
            }],
            windows,
        }
    }

    fn window(
        id: u32,
        app_name: &str,
        title: &str,
        z_rank: i64,
        focused: bool,
    ) -> CaptureWindowState {
        CaptureWindowState {
            window_id: Some(id),
            app_pid: Some(id as i64),
            app_name: Some(app_name.to_string()),
            bundle_id: None,
            title: Some(title.to_string()),
            frame_points: Some(rect(10.0, 20.0, 500.0, 300.0)),
            frame_pixels: None,
            display_id: None,
            layer: Some(0),
            alpha: Some(1.0),
            z_rank: Some(z_rank),
            is_minimized: false,
            is_focused: focused,
            is_on_screen: true,
            capture_eligible: true,
            visible_fraction_estimate: Some(1.0),
            source: Some("test".to_string()),
            focus_metadata: None,
        }
    }

    fn rect(x: f64, y: f64, width: f64, height: f64) -> CaptureRect {
        CaptureRect {
            x,
            y,
            width,
            height,
        }
    }

    fn one_pixel_preview(source: &str) -> PreviewFrame {
        PreviewFrame {
            image: ColorImage::from_rgba_unmultiplied([1, 1], &[0, 0, 0, 255]),
            size: [1, 1],
            source: source.to_string(),
            space: PreviewCoordinateSpace::Window,
            timing: RefreshTiming::default(),
        }
    }

    #[test]
    fn selected_window_skips_dashboard_itself() {
        let snapshot = snapshot(vec![
            window(
                1,
                "onecontext-capture-dashboard",
                "1Context Capture Dashboard",
                0,
                true,
            ),
            window(2, "Terminal", "build", 1, false),
        ]);
        assert_eq!(selected_window_id(&snapshot), None);
    }

    #[test]
    fn selected_window_skips_dashboard_title_even_when_app_name_is_bundle_display_name() {
        let snapshot = snapshot(vec![window(
            1,
            "1Context Dev",
            "1Context Capture Dashboard",
            0,
            true,
        )]);
        assert_eq!(selected_window_id(&snapshot), None);
    }

    #[test]
    fn auto_target_uses_top_non_dashboard_window_when_dashboard_is_focused() {
        let snapshot = snapshot(vec![
            window(1, "1Context Dev", "1Context Capture Dashboard", 0, true),
            window(2, "Terminal", "build", 2, false),
            window(3, "Google Chrome", "docs", 1, false),
        ]);

        assert_eq!(
            preview_capture_plan(&snapshot, PreviewTarget::Auto),
            PreviewCapturePlan::Window {
                window_id: 3,
                reason: "auto non-dashboard window"
            }
        );
    }

    #[test]
    fn auto_target_preserves_focused_window_behind_top_candidate() {
        let snapshot = snapshot(vec![
            window(1, "Google Chrome", "docs", 1, false),
            window(2, "Codex", "Codex", 2, true),
        ]);

        assert_eq!(selected_window_id(&snapshot), Some(2));
        assert_eq!(
            preview_capture_plan(&snapshot, PreviewTarget::Auto),
            PreviewCapturePlan::Window {
                window_id: 2,
                reason: "auto non-dashboard window"
            }
        );
    }

    #[test]
    fn explicit_targets_disambiguate_focus_z_order_conflicts() {
        let snapshot = snapshot(vec![
            window(1, "Google Chrome", "docs", 1, false),
            window(2, "Codex", "Codex", 2, true),
        ]);

        assert_eq!(
            preview_capture_plan(&snapshot, PreviewTarget::Auto),
            PreviewCapturePlan::Window {
                window_id: 2,
                reason: "auto non-dashboard window"
            }
        );
        assert_eq!(
            preview_capture_plan(&snapshot, PreviewTarget::FocusedWindow),
            PreviewCapturePlan::Window {
                window_id: 2,
                reason: "focused non-dashboard window"
            }
        );
        assert_eq!(
            preview_capture_plan(&snapshot, PreviewTarget::TopWindow),
            PreviewCapturePlan::Window {
                window_id: 1,
                reason: "top capture-eligible non-dashboard window"
            }
        );
    }

    #[test]
    fn selected_window_accepts_focused_frontmost_candidate() {
        let snapshot = snapshot(vec![
            window(1, "Codex", "Codex", 1, true),
            window(2, "Google Chrome", "docs", 2, false),
        ]);

        assert_eq!(selected_window_id(&snapshot), Some(1));
    }

    #[test]
    fn dynamic_preview_targets_refresh_snapshot_metadata_before_preview() {
        assert!(PreviewTarget::Auto.requires_fresh_snapshot_for_preview());
        assert!(PreviewTarget::FocusedWindow.requires_fresh_snapshot_for_preview());
        assert!(PreviewTarget::TopWindow.requires_fresh_snapshot_for_preview());
        assert!(!PreviewTarget::MainDisplay.requires_fresh_snapshot_for_preview());
    }

    #[test]
    fn focused_target_refuses_display_fallback_when_only_dashboard_is_focused() {
        let snapshot = snapshot(vec![window(
            1,
            "1Context Dev",
            "1Context Capture Dashboard",
            0,
            true,
        )]);

        assert!(matches!(
            preview_capture_plan(&snapshot, PreviewTarget::FocusedWindow),
            PreviewCapturePlan::Refuse(_)
        ));
    }

    #[test]
    fn auto_target_refuses_main_display_when_no_window_candidate_exists() {
        let snapshot = snapshot(vec![]);

        assert!(matches!(
            preview_capture_plan(&snapshot, PreviewTarget::Auto),
            PreviewCapturePlan::Refuse(_)
        ));
        assert!(matches!(
            preview_capture_plan(&snapshot, PreviewTarget::MainDisplay),
            PreviewCapturePlan::Display("explicit main display target")
        ));
    }

    #[test]
    fn list_window_filters_system_and_non_content_surfaces() {
        let mut normal = window(1, "Terminal", "build", 1, false);
        assert!(should_list_window(&normal));

        normal.app_name = Some("Window Server".to_string());
        assert!(!should_list_window(&normal));

        let mut dock = window(2, "Dock", "Dock", 2, false);
        dock.bundle_id = Some("com.apple.dock".to_string());
        assert!(!should_list_window(&dock));

        let mut layer = window(3, "Terminal", "popup", 3, false);
        layer.layer = Some(24);
        assert!(!should_list_window(&layer));

        let mut minimized = window(4, "Terminal", "minimized", 4, false);
        minimized.is_minimized = true;
        assert!(!should_list_window(&minimized));

        let mut hidden = window(5, "Terminal", "hidden", 5, false);
        hidden.visible_fraction_estimate = Some(0.0);
        assert!(!should_list_window(&hidden));

        let mut tiny = window(6, "Terminal", "tiny", 6, false);
        tiny.frame_points = Some(rect(10.0, 10.0, 40.0, 40.0));
        assert!(!should_list_window(&tiny));
    }

    #[test]
    fn overlay_candidates_pick_frontmost_visible_windows() {
        let display = rect(0.0, 0.0, 1000.0, 1000.0);
        let snapshot = snapshot(vec![
            window(1, "Terminal", "front", 1, false),
            window(2, "Chrome", "middle", 2, false),
            window(3, "Finder", "back", 3, false),
        ]);
        let ids: Vec<_> = overlay_candidates(&snapshot, &display, 2)
            .into_iter()
            .filter_map(|window| window.window_id)
            .collect();

        assert_eq!(ids, vec![1, 2]);
    }

    #[test]
    fn downscale_preview_image_preserves_aspect_ratio_under_cap() {
        let image = RgbaImage::new(2000, 1000);
        let resized = downscale_preview_image(image, 500);

        assert_eq!(resized.width(), 500);
        assert_eq!(resized.height(), 250);
    }

    #[test]
    fn parses_capture_snapshot_shape() {
        let raw = r#"{
          "generatedAt": "2026-05-24T20:19:25.810Z",
          "capture_status": {
            "root_directory": "/tmp/1context/capture",
            "events_directory": "/tmp/1context/capture/events"
          },
          "activeApplication": { "appName": "Terminal" },
          "displays": [
            {
              "displayID": "1",
              "framePoints": { "x": 0, "y": 0, "width": 1470, "height": 956 },
              "isMain": true,
              "scaleFactor": 2
            }
          ],
          "windows": [
            {
              "windowID": 42,
              "appName": "Terminal",
              "bundleID": "com.apple.Terminal",
              "title": "logs",
              "framePoints": { "x": 10, "y": 20, "width": 500, "height": 300 },
              "layer": 0,
              "alpha": 1,
              "zRank": 1,
              "isMinimized": false,
              "isFocused": true,
              "isOnScreen": true,
              "captureEligible": true,
              "visibleFractionEstimate": 1,
              "source": "sc+cg"
            }
          ]
        }"#;
        let snapshot: CaptureSnapshot = serde_json::from_str(raw).unwrap();
        assert_eq!(
            snapshot.generated_at.as_deref(),
            Some("2026-05-24T20:19:25.810Z")
        );
        assert_eq!(snapshot.windows[0].window_id, Some(42));
        assert_eq!(
            snapshot.windows[0].bundle_id.as_deref(),
            Some("com.apple.Terminal")
        );
        assert_eq!(snapshot.displays[0].display_id.as_deref(), Some("1"));
        assert_eq!(snapshot.displays[0].scale_factor, Some(2.0));
        assert_eq!(
            snapshot
                .capture_status
                .as_ref()
                .and_then(|status| status.events_directory.as_ref())
                .map(|path| path.display().to_string())
                .as_deref(),
            Some("/tmp/1context/capture/events")
        );
        assert_eq!(selected_window_id(&snapshot), Some(42));
    }

    #[test]
    fn parses_ux_anchor_event_envelope_shape() {
        let raw = r#"{
          "schemaVersion": 1,
          "eventType": "capture.ux_anchor",
          "durability": "lossless",
          "recordedAt": "2026-05-24T20:20:01.250Z",
          "payload": {
            "schema_version": 1,
            "kind": "scroll_burst",
            "source": "cg_event_tap",
            "started_at": "2026-05-24T20:20:01.000Z",
            "ended_at": "2026-05-24T20:20:01.250Z",
            "scroll": {
              "event_count": 3,
              "total_dx": 0,
              "total_dy": -42.5,
              "max_abs_dy": 20,
              "momentum_event_count": 1,
              "duration_ms": 250
            }
          }
        }"#;
        let envelope: CaptureEventEnvelopeValue = serde_json::from_str(raw).unwrap();
        let anchors = parse_ux_anchor_events(&envelope);

        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].event_type, "capture.ux_anchor");
        assert_eq!(anchors[0].kind, "scroll_burst");
        assert_eq!(anchors[0].recorded_at, "2026-05-24T20:20:01.250Z");
        let scroll = anchors[0].scroll.as_ref().unwrap();
        assert_eq!(scroll.event_count, Some(3));
        assert_eq!(scroll.total_dy, -42.5);
    }

    #[test]
    fn parses_active_window_frame_metadata_envelope_shape() {
        let raw = r#"{
          "schemaVersion": 1,
          "eventType": "capture.active_window_frame_metadata",
          "durability": "best_effort",
          "recordedAt": "2026-05-24T20:21:01.125Z",
          "payload": {
            "schemaVersion": 1,
            "streamID": "sample",
            "sequence": 7,
            "capturedAt": "2026-05-24T20:21:01.125Z",
            "target": {
              "windowID": 42,
              "appPID": 100,
              "appName": "Terminal",
              "bundleID": "com.apple.Terminal",
              "title": "logs",
              "framePoints": { "x": 10, "y": 20, "width": 500, "height": 300 },
              "zRank": 1,
              "isFocused": true,
              "captureEligible": true,
              "source": "sc+cg"
            },
            "frameStatus": "complete",
            "frameStatusRawValue": 0,
            "attachmentsPresent": true,
            "dirtyRectSummary": {
              "dirtyRectCount": 2,
              "dirtyAreaRatio": 0.125,
              "changedTileRatio": 0.25,
              "cappedRects": [],
              "cappedRectLimit": 8,
              "malformedRectCount": 0,
              "estimatedDY": 0
            },
            "motionFeatures": {
              "dirtyAreaRatio": 0.125,
              "dirtyRectCount": 2,
              "meanPixelDiff": 0,
              "changedTileRatio": 0.25,
              "estimatedDY": 0,
              "scrollEventRecently": false,
              "keyboardEventRecently": false,
              "ocrNewLineRate": 0,
              "focused": true,
              "classification": "watch"
            },
            "capturePolicyDecision": {
              "mode": "watch",
              "targetCaptureFPS": 3
            },
            "adaptiveDecision": {
              "classifierMode": "watch",
              "controllerMode": "activeText",
              "targetFPS": 10,
              "targetAnalysisFPS": 10,
              "shouldUpdateStreamConfiguration": true,
              "updateReason": "fps_increase"
            },
            "uxMotionHints": {
              "scrollEventRecently": false,
              "keyboardEventRecently": true
            },
            "uxMotionHintsFused": true,
            "feedsMotionClassifier": true,
            "metadataSampleFusion": {
              "pixelsUntouched": true
            },
            "parseWarnings": []
          }
        }"#;
        let envelope: CaptureEventEnvelopeValue = serde_json::from_str(raw).unwrap();
        let frame = parse_frame_metadata_event(&envelope).unwrap();

        assert_eq!(frame.event_type, "capture.active_window_frame_metadata");
        assert_eq!(frame.sequence, Some(7));
        assert_eq!(frame.frame_status.as_deref(), Some("complete"));
        assert_eq!(frame.dirty_rect_count, Some(2));
        assert_eq!(frame.dirty_area_ratio, Some(0.125));
        assert_eq!(frame.changed_tile_ratio, Some(0.25));
        assert_eq!(frame.estimated_dy, Some(0.0));
        assert_eq!(frame.motion_label().as_deref(), Some("watch/activeText"));
        assert_eq!(frame.feeds_motion_classifier, Some(true));
        assert_eq!(frame.ux_motion_hints_fused, Some(true));
        assert!(frame.ux_motion_hints_present);
        assert_eq!(frame.pixels_untouched, Some(true));
        let adaptive = frame.adaptive_decision.as_ref().unwrap();
        assert_eq!(adaptive.classifier_mode.as_deref(), Some("watch"));
        assert_eq!(adaptive.controller_mode.as_deref(), Some("activeText"));
        assert_eq!(adaptive.target_fps, Some(10));
        assert_eq!(adaptive.target_analysis_fps, Some(10));
        assert_eq!(adaptive.should_update_stream_configuration, Some(true));
        assert_eq!(adaptive.update_reason.as_deref(), Some("fps_increase"));
        assert_eq!(frame.target_window_id, Some(42));
        assert_eq!(frame.target_app_name.as_deref(), Some("Terminal"));
        assert_eq!(frame.target_label(), "#42 Terminal - logs");
    }

    #[test]
    fn timeline_sorts_by_parsed_timestamp_not_raw_string() {
        let dir =
            env::temp_dir().join(format!("1ctx-dashboard-parsed-sort-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("2026-05-24.events.jsonl"),
            r#"{"eventType":"capture.generic","recordedAt":"2026-05-24T23:30:00+05:00","payload":{"status":"older-by-utc"}}
{"eventType":"capture.generic","recordedAt":"2026-05-24T19:00:00Z","payload":{"status":"newer-by-utc"}}
"#,
        )
        .unwrap();

        let recent = read_recent_capture_events(&dir, None).unwrap();
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(
            recent
                .timeline
                .iter()
                .map(|event| event.recorded_at.as_str())
                .collect::<Vec<_>>(),
            vec!["2026-05-24T19:00:00Z", "2026-05-24T23:30:00+05:00"]
        );
        assert!(recent
            .timeline
            .iter()
            .all(|event| event.sort_time.is_some()));
    }

    #[test]
    fn canonical_envelope_fields_win_and_render_durability_source_identity() {
        let raw = r#"{
          "eventType": "capture.generic",
          "event_time_start": "2026-05-24T20:00:00.100Z",
          "event_time_end": "2026-05-24T20:00:00.900Z",
          "ingested_at": "2026-05-24T21:00:00Z",
          "recordedAt": "2026-05-24T22:00:00Z",
          "durability": "lossless",
          "lane_id": "canonical-lane",
          "stream_id": "canonical-stream",
          "source_record_id": "canonical-record",
          "source_hash": "abcdef1234567890",
          "capture_bundle_id": "bundle-42",
          "privacy_class": "metadata_only",
          "shape": "window_state",
          "payload": {
            "capturedAt": "2026-05-24T23:00:00Z",
            "lane_id": "payload-lane",
            "streamID": "payload-stream",
            "durability": "best_effort",
            "status": "available"
          }
        }"#;
        let envelope: CaptureEventEnvelopeValue = serde_json::from_str(raw).unwrap();
        let event =
            generic_timeline_event(&envelope, Path::new("/tmp/canonical.events.jsonl")).unwrap();
        let rendered = timeline_event_detail_line(&event);

        assert_eq!(event.recorded_at, "2026-05-24T20:00:00.100Z");
        assert_eq!(event.time_source, "event_time_start");
        assert_eq!(event.lane_id.as_deref(), Some("canonical-lane"));
        assert_eq!(event.stream_id.as_deref(), Some("canonical-stream"));
        assert_eq!(event.durability.as_deref(), Some("lossless"));
        assert!(rendered.contains("durability=lossless"));
        assert!(rendered.contains("lane=canonical-lane"));
        assert!(rendered.contains("stream=canonical-stream"));
        assert!(rendered.contains("bundle=bundle-42"));
        assert!(rendered.contains("source_record=canonical-record"));
        assert!(rendered.contains("source_hash=abcdef1234567890"));
        assert!(rendered.contains("privacy=metadata_only"));
        assert!(rendered.contains("shape=window_state"));
        assert!(rendered.contains("file=canonical.events.jsonl"));
    }

    #[test]
    fn parses_shortcut_and_focus_transition_ux_anchors() {
        let raw = r#"{
          "eventType": "capture.ux.shortcut.v1",
          "recordedAt": "2026-05-24T20:20:02.000Z",
          "payload": {
            "kind": "shortcut",
            "started_at": "2026-05-24T20:20:01.900Z",
            "ended_at": "2026-05-24T20:20:02.000Z",
            "recent_target_process_id": 4242,
            "shortcut": {
              "event_count": 2,
              "key_down_count": 2,
              "auto_repeat_count": 1,
              "action_categories": [
                { "category": "editing", "event_count": 1 },
                { "category": "navigation", "event_count": 1 }
              ]
            },
            "focus_transition": {
              "previous_process_id": 111,
              "current_process_id": 4242,
              "trigger": "keyboard",
              "confidence": "medium"
            }
          }
        }"#;
        let envelope: CaptureEventEnvelopeValue = serde_json::from_str(raw).unwrap();
        let anchors = parse_ux_anchor_events(&envelope);

        assert_eq!(anchors.len(), 1);
        assert_eq!(anchors[0].recent_target_process_id.as_deref(), Some("4242"));
        let shortcut = anchors[0].shortcut.as_ref().unwrap();
        assert_eq!(shortcut.event_count, Some(2));
        assert_eq!(shortcut.key_down_count, Some(2));
        assert_eq!(shortcut.auto_repeat_count, Some(1));
        assert_eq!(shortcut.categories, vec!["editing", "navigation"]);
        let focus = anchors[0].focus_transition.as_ref().unwrap();
        assert_eq!(focus.previous_process_id.as_deref(), Some("111"));
        assert_eq!(focus.current_process_id.as_deref(), Some("4242"));
        assert_eq!(focus.trigger.as_deref(), Some("keyboard"));
        assert_eq!(focus.confidence.as_deref(), Some("medium"));
    }

    #[test]
    fn parses_capture_status_metadata_sample_fusion() {
        let raw = r#"{
          "root_directory": "/tmp/1context/capture",
          "events_directory": "/tmp/1context/capture/events",
          "metadata_sample_fusion": {
            "ux_motion_hints_enabled": true,
            "source": "persistent_ux_event_tap",
            "pixels_untouched": true
          }
        }"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        let status = capture_status_from_value(&value);
        let fusion = status.metadata_sample_fusion.unwrap();

        assert_eq!(fusion.ux_motion_hints_enabled, Some(true));
        assert_eq!(fusion.source.as_deref(), Some("persistent_ux_event_tap"));
        assert_eq!(fusion.pixels_untouched, Some(true));
        assert_eq!(
            fusion.summary_label(),
            "metadata fusion: ux_hints=enabled | source=persistent_ux_event_tap | pixels=untouched"
        );
    }

    #[test]
    fn parses_capture_status_permission_metadata() {
        let raw = r#"{
          "surface": "capture_status",
          "root_directory": "/tmp/1context/capture",
          "events_directory": "/tmp/1context/capture/events",
          "windows_directory": "/tmp/1context/capture/windows",
          "media_directory": "/tmp/1context/capture/media",
          "window_unit_of_truth": true,
          "available_methods": ["capture.status", "capture.snapshot"],
          "motion_hints": {
            "generated_at": "2026-05-24T20:30:00Z",
            "scroll_event_recently": true,
            "keyboard_activity_recently": false,
            "estimated_scroll_dy": -8.5,
            "focused_recently": true,
            "recent_target_process_id": 42
          },
          "ux_event_tap": {
            "startup_wired": true,
            "lifecycle_state": "running",
            "tap_active": true,
            "tap_owner_pid": 77,
            "tap_owner_executable": "/Applications/1Context Dev.app/Contents/MacOS/1contextd",
            "tap_owner_bundle": "com.haptica.1context.dev",
            "event_tap": "cgSessionEventTap",
            "observed_event_count": 12,
            "callback_count": 14,
            "queue_depth": 1,
            "dropped_count": 0,
            "callback_max_us": 3.25,
            "permission_subject": {
              "bundle_identifier": "com.haptica.1context.dev",
              "designated_requirement_sha256": "1234567890abcdef",
              "executable_path": "/Applications/1Context Dev.app/Contents/MacOS/1contextd",
              "app_version": "0.1.0"
            }
          },
          "capabilities": {
            "screenRecording": { "ready": true, "source": "preflight" },
            "systemAudio": "not_required"
          }
        }"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        let status = capture_status_from_value(&value);

        assert_eq!(status.surface.as_deref(), Some("capture_status"));
        assert_eq!(
            status
                .windows_directory
                .as_ref()
                .map(|path| path.display().to_string()),
            Some("/tmp/1context/capture/windows".to_string())
        );
        assert_eq!(status.window_unit_of_truth, Some(true));
        assert_eq!(status.available_methods.len(), 2);
        let hints = capture_status_motion_hints(&status).unwrap();
        assert_eq!(hints.recent_target_process_id, Some(42));
        assert_eq!(hints.estimated_scroll_dy, Some(-8.5));
        let tap = status.ux_event_tap.as_ref().unwrap();
        assert_eq!(tap.tap_active, Some(true));
        assert_eq!(tap.tap_owner_pid, Some(77));
        assert_eq!(tap.observed_event_count, Some(12));
        assert_eq!(tap.callback_count, Some(14));
        assert_eq!(
            tap.permission_subject.as_ref().unwrap().compact_label(),
            "bundle=com.haptica.1context.dev exe=1contextd version=0.1.0 dr_sha256=1234567890ab..."
        );
        assert_eq!(status.capability_signals.len(), 2);
        assert_eq!(
            status.capability_signals[0].compact_label(),
            "screen_recording=yes@preflight"
        );
        assert_eq!(
            status.capability_signals[1].compact_label(),
            "system_audio=not_required"
        );
    }

    #[test]
    fn motion_hint_target_prefers_matching_focused_window_pid() {
        let mut focused = window(7, "Terminal", "logs", 2, true);
        focused.app_pid = Some(42);
        let mut frontmost = window(8, "Terminal", "other", 1, false);
        frontmost.app_pid = Some(42);
        let snapshot = snapshot(vec![frontmost, focused]);
        let status = CaptureStatus {
            motion_hints: Some(CaptureMotionHints {
                recent_target_process_id: Some(42),
                ..CaptureMotionHints::default()
            }),
            ..CaptureStatus::default()
        };

        assert_eq!(
            recent_motion_target_window(&snapshot, &status).and_then(|window| window.window_id),
            Some(7)
        );
    }

    #[test]
    fn parses_focused_context_and_window_focus_metadata() {
        let raw = r#"{
          "generatedAt": "2026-05-24T20:31:00Z",
          "activeApplication": {
            "processID": 100,
            "bundleID": "com.apple.Terminal",
            "appName": "Terminal"
          },
          "focusedContext": {
            "status": "available",
            "isProcessTrusted": true,
            "focusedApplicationProcessID": 100,
            "matchedWindowID": 42,
            "activeApplication": {
              "processID": 100,
              "bundleID": "com.apple.Terminal",
              "appName": "Terminal"
            },
            "focusedElement": {
              "role": "AXTextArea",
              "title": "Editor",
              "valueShape": {
                "kind": "string",
                "characterCount": 120,
                "sourceAttribute": "AXValue",
                "redacted": true
              },
              "selection": {
                "selectedTextCharacterCount": 4,
                "selectedTextRedacted": true
              },
              "isSensitive": true,
              "redactionReasons": ["selected_text"]
            },
            "issues": [
              { "code": "partial_value", "status": "cannot_complete", "message": "AX value redacted" }
            ]
          },
          "windows": [
            {
              "windowID": 42,
              "appPID": 100,
              "appName": "Terminal",
              "bundleID": "com.apple.Terminal",
              "title": "logs",
              "framePoints": { "x": 10, "y": 20, "width": 500, "height": 300 },
              "layer": 0,
              "alpha": 1,
              "zRank": 1,
              "isFocused": true,
              "isOnScreen": true,
              "captureEligible": true,
              "visibleFractionEstimate": 1,
              "source": "coregraphics+screencapturekit",
              "focusMetadata": {
                "source": "ax_focused_context",
                "status": "matched",
                "confidence": "high",
                "matchedWindowID": 42,
                "matchSignals": ["pid", "title", "bounds"]
              }
            }
          ]
        }"#;
        let snapshot: CaptureSnapshot = serde_json::from_str(raw).unwrap();
        let active = snapshot.active_application.as_ref().unwrap();
        assert_eq!(active.process_id, Some(100));
        assert_eq!(active.bundle_id.as_deref(), Some("com.apple.Terminal"));
        let context = snapshot.focused_context.as_ref().unwrap();
        assert_eq!(context.status.as_deref(), Some("available"));
        assert_eq!(context.is_process_trusted, Some(true));
        assert_eq!(context.focused_application_process_id, Some(100));
        assert_eq!(context.matched_window_id, Some(42));
        assert_eq!(
            context.focused_element.as_ref().unwrap().compact_label(),
            "role=AXTextArea title=Editor value=string 120 chars attr=AXValue redacted selection=4 chars redacted sensitive=yes redacted=selected_text"
        );
        assert_eq!(
            context.issues[0].compact_label(),
            "partial_value status=cannot_complete AX value redacted"
        );
        assert_eq!(
            snapshot.windows[0]
                .focus_metadata
                .as_ref()
                .unwrap()
                .compact_label(),
            "ax_focused_context matched confidence=high matched=#42 signals=pid,title,bounds"
        );
    }

    #[test]
    fn reads_recent_jsonl_tail_without_unbounded_scan() {
        let dir = env::temp_dir().join(format!("1ctx-dashboard-events-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("2026-05-24.events.jsonl");
        fs::write(
            &path,
            r#"{"eventType":"capture.ux_anchor","recordedAt":"2026-05-24T20:20:01Z","payload":{"kind":"keyboard_activity","keyboard_activity":{"event_count":4,"key_down_count":2,"auto_repeat_count":1}}}
{"eventType":"capture.active_window_frame_metadata","recordedAt":"2026-05-24T20:21:01Z","payload":{"sequence":1,"frameStatus":"idle","target":{"windowID":8,"appName":"Terminal","title":"logs"},"dirtyRectSummary":{"dirtyRectCount":0,"dirtyAreaRatio":0}}}
"#,
        )
        .unwrap();

        let recent = read_recent_capture_events(&dir, None).unwrap();
        let _ = fs::remove_dir_all(&dir);

        assert_eq!(recent.ux.len(), 1);
        assert_eq!(recent.frames.len(), 1);
        assert_eq!(recent.frames[0].frame_status.as_deref(), Some("idle"));
        assert_eq!(recent.scanned_lines, 2);
    }

    #[test]
    fn tail_coverage_exposes_source_files_for_log_gap_debugging() {
        let root = env::temp_dir().join(format!(
            "1ctx-dashboard-tail-coverage-{}",
            std::process::id()
        ));
        let events_dir = root.join("events");
        let windows_dir = root.join("windows");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&events_dir).unwrap();
        fs::create_dir_all(&windows_dir).unwrap();
        fs::write(
            events_dir.join("2026-05-24.events.jsonl"),
            r#"{"eventType":"capture.ux_anchor","recordedAt":"2026-05-24T20:20:01Z","payload":{"kind":"keyboard_activity","keyboard_activity":{"event_count":4}}}
"#,
        )
        .unwrap();
        fs::write(
            windows_dir.join("2026-05-24.windows.jsonl"),
            r#"{"eventType":"capture.window_snapshot","recordedAt":"2026-05-24T20:22:01Z","payload":{"windows":[],"displays":[]}}
"#,
        )
        .unwrap();

        let recent = read_recent_capture_events(&events_dir, Some(&windows_dir)).unwrap();
        let label = live_event_tail_coverage_label(&recent.source_files, recent.scanned_lines);
        let _ = fs::remove_dir_all(&root);

        assert_eq!(recent.scanned_lines, 2);
        assert_eq!(recent.source_files.len(), 2);
        assert!(label.contains("2 files"));
        assert!(label.contains("2 lines"));
        assert!(label.contains("2026-05-24.events.jsonl"));
        assert!(label.contains("2026-05-24.windows.jsonl"));
        assert!(recent.timeline.iter().any(
            |event| timeline_event_detail_line(event).contains("file=2026-05-24.windows.jsonl")
        ));
    }

    #[test]
    fn startup_socket_absence_does_not_pause_auto_refresh() {
        assert!(!should_pause_auto_refresh_after_capture_error(
            "capture snapshot failed: 1Context needs attention: Could not connect to /Users/paulhan/Library/Application Support/1Context Dev/run/1context.sock"
        ));
        assert!(!should_pause_auto_refresh_after_capture_error(
            "capture status failed: 1Context needs attention: Could not connect to /tmp/1context.sock"
        ));
        assert!(!should_pause_auto_refresh_after_capture_error(
            "connect: no such file or directory"
        ));
        assert!(!should_pause_auto_refresh_after_capture_error(
            "Connection refused"
        ));
        assert!(should_pause_auto_refresh_after_capture_error(
            "capture snapshot timed out after 6.0s"
        ));
        assert!(should_pause_auto_refresh_after_capture_error(
            "1Context did not return a response"
        ));
    }

    #[test]
    fn snapshot_socket_absence_keeps_auto_refresh_enabled() {
        let ctx = egui::Context::default();
        let mut app = test_app();

        app.handle_snapshot_preview_refresh(
            &ctx,
            Err("capture snapshot failed: 1Context needs attention: Could not connect to /Users/paulhan/Library/Application Support/1Context Dev/run/1context.sock".to_string()),
        );

        assert!(app.auto_refresh);
        assert!(app.snapshot.is_none());
        assert!(app.last_snapshot_refresh.is_some());
        assert!(app.last_preview_refresh.is_some());
        assert!(app
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("Could not connect"));
    }

    #[test]
    fn snapshot_socket_absence_recovers_on_next_success() {
        let ctx = egui::Context::default();
        let mut app = test_app();

        app.handle_snapshot_preview_refresh(
            &ctx,
            Err("capture snapshot failed: 1Context needs attention: Could not connect to /tmp/1context.sock".to_string()),
        );
        app.handle_snapshot_preview_refresh(
            &ctx,
            Ok(SnapshotPreviewRefresh {
                snapshot: snapshot(vec![window(42, "Terminal", "logs", 1, true)]),
                preview: Ok(one_pixel_preview("test preview")),
                snapshot_ms: 2,
                total_ms: 3,
            }),
        );

        assert!(app.auto_refresh);
        assert!(app.last_error.is_none());
        assert!(app.snapshot.is_some());
        assert!(app.preview.is_some());
        assert_eq!(app.preview_source, "test preview");
    }

    #[test]
    fn capture_refresh_is_single_flight() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (_sender, receiver) = mpsc::channel();
        app.pending_capture_refresh = Some(PendingCaptureRefresh {
            kind: CaptureRefreshKind::SnapshotAndPreview,
            started_at: Instant::now(),
            receiver,
        });
        app.preview_source = "stable".to_string();

        app.refresh_snapshot_and_preview(&ctx);
        app.refresh_snapshot_and_preview(&ctx);

        assert!(app.pending_capture_refresh.is_some());
        assert_eq!(app.preview_source, "stable");
    }

    #[test]
    fn pending_capture_timeout_releases_job_and_pauses_live() {
        let ctx = egui::Context::default();
        let mut app = test_app();
        let (_sender, receiver) = mpsc::channel();
        app.pending_capture_refresh = Some(PendingCaptureRefresh {
            kind: CaptureRefreshKind::SnapshotAndPreview,
            started_at: Instant::now() - CAPTURE_JOB_TIMEOUT,
            receiver,
        });

        app.poll_capture_refresh(&ctx);

        assert!(app.pending_capture_refresh.is_none());
        assert!(!app.auto_refresh);
        assert!(app.last_snapshot_refresh.is_some());
        assert!(app.last_preview_refresh.is_some());
        assert!(app
            .last_error
            .as_deref()
            .unwrap_or_default()
            .contains("taking longer"));
    }

    #[test]
    fn live_event_socket_absence_records_error_without_pausing() {
        let mut app = test_app();
        let (sender, receiver) = mpsc::channel();
        sender
            .send(Err("capture status failed: 1Context needs attention: Could not connect to /tmp/1context.sock".to_string()))
            .unwrap();
        app.pending_live_event_refresh = Some(PendingLiveEventRefresh {
            started_at: Instant::now(),
            receiver,
        });

        app.poll_live_event_refresh();

        assert!(app.auto_refresh);
        assert!(app.pending_live_event_refresh.is_none());
        assert!(app.last_event_refresh.is_some());
        assert!(app
            .live_events
            .error
            .as_deref()
            .unwrap_or_default()
            .contains("Could not connect"));
    }

    #[test]
    fn bundle_inventory_counts_ready_processing_and_failed_dirs() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("capture");
        let live = root.join("bundles/live/ready-a");
        let processing = root.join("bundles/processing/work-b.partial");
        let failed = root.join("bundles/failed/failed-c");
        fs::create_dir_all(&live).unwrap();
        fs::create_dir_all(&processing).unwrap();
        fs::create_dir_all(&failed).unwrap();
        fs::write(live.join("READY"), "").unwrap();
        fs::write(live.join("payload.bin"), vec![1_u8; 2048]).unwrap();
        fs::write(processing.join("partial.bin"), vec![2_u8; 128]).unwrap();
        fs::write(failed.join("error.txt"), "failed").unwrap();

        let inventory = load_bundle_inventory(
            &PathBuf::from("/tmp/missing-1context-cli"),
            Some(&root),
            None,
        )
        .unwrap();

        assert_eq!(inventory.capture_root.as_deref(), Some(root.as_path()));
        assert_eq!(inventory.live_count, 1);
        assert_eq!(inventory.processing_count, 1);
        assert_eq!(inventory.failed_count, 1);
        assert!(inventory.total_bytes >= 2048);
        assert_eq!(inventory.recent_ready.len(), 1);
        assert_eq!(inventory.recent_ready[0].capture_id, "ready-a");
        assert!(inventory.sweep_delete_count.is_some());
        assert!(inventory.sweep_move_count.is_some());
    }

    #[test]
    fn merges_recent_capture_logs_into_time_aligned_timeline() {
        let root = env::temp_dir().join(format!("1ctx-dashboard-timeline-{}", std::process::id()));
        let events_dir = root.join("events");
        let windows_dir = root.join("windows");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&events_dir).unwrap();
        fs::create_dir_all(&windows_dir).unwrap();

        fs::write(
            events_dir.join("2026-05-24.events.jsonl"),
            r#"{"eventType":"capture.ax_focused_context","recordedAt":"2026-05-24T20:19:01Z","payload":{"status":"available","activeApplication":{"appName":"Terminal"},"matchedWindowID":42}}
{"eventType":"capture.ux.scroll_burst.v1","recordedAt":"2026-05-24T20:20:01Z","payload":{"kind":"scroll_burst","source":"cg_event_tap","started_at":"2026-05-24T20:20:00.900Z","ended_at":"2026-05-24T20:20:01Z","scroll":{"event_count":3,"total_dx":0,"total_dy":-30}}}
{"eventType":"capture.active_window_frame_metadata","recordedAt":"2026-05-24T20:21:01Z","payload":{"sequence":1,"frameStatus":"complete","target":{"windowID":42,"appName":"Terminal","title":"logs"},"dirtyRectSummary":{"dirtyRectCount":2,"dirtyAreaRatio":0.125,"estimatedDY":-12}}}
"#,
        )
        .unwrap();
        fs::write(
            windows_dir.join("2026-05-24.windows.jsonl"),
            r#"{"eventType":"capture.window_snapshot","recordedAt":"2026-05-24T20:22:01Z","payload":{"generatedAt":"2026-05-24T20:22:01Z","activeApplication":{"appName":"Terminal"},"displays":[{"displayID":"1"}],"windows":[{"windowID":42,"appName":"Terminal","title":"logs"}]}}
"#,
        )
        .unwrap();

        let recent = read_recent_capture_events(&events_dir, Some(&windows_dir)).unwrap();
        let _ = fs::remove_dir_all(&root);

        assert_eq!(
            recent
                .timeline
                .iter()
                .map(|event| event.event_type.as_str())
                .collect::<Vec<_>>(),
            vec![
                "capture.window_snapshot",
                "capture.active_window_frame_metadata",
                "capture.ux.scroll_burst.v1",
                "capture.ax_focused_context"
            ]
        );
        assert_eq!(recent.timeline[0].source_kind, "Window");
        assert_eq!(recent.timeline[1].source_kind, "SCK");
        assert_eq!(recent.timeline[2].source_kind, "UX");
        assert_eq!(recent.timeline[3].source_kind, "AX");
        assert_eq!(recent.timeline[1].window_id, Some(42));
        assert!(recent.timeline[0]
            .detail
            .as_deref()
            .unwrap_or_default()
            .contains("windows=1"));
    }
}
