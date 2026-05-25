use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
};

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

use crate::fixture::DashboardFixture;

#[derive(Default)]
pub struct FrameCacheState {
    root: Option<PathBuf>,
    fps: f32,
    format: String,
    naming: String,
    frame_count: Option<usize>,
    max_cached_frames: usize,
    frames: HashMap<usize, LoadedFrame>,
    lru: VecDeque<usize>,
    current_frame_index: Option<usize>,
    last_error: Option<String>,
}

pub struct LoadedFrame {
    pub frame_index: usize,
    pub time_ms: u64,
    pub texture: TextureHandle,
    pub size: [usize; 2],
    pub path: PathBuf,
}

impl FrameCacheState {
    pub fn new(fixture: &DashboardFixture) -> Self {
        let Some(cache) = fixture.session.media.frame_cache.as_ref() else {
            return Self {
                last_error: Some("session has no frame_cache config".to_string()),
                ..Default::default()
            };
        };

        let frame_set = fixture
            .session
            .media
            .candidate_frame_sets
            .iter()
            .find(|set| set.root == cache.root);
        let frame_count = frame_set.map(|set| set.count);
        let naming = frame_set
            .map(|set| set.naming.clone())
            .unwrap_or_else(|| format!("frame-{{index:03}}.{}", cache.format));

        Self {
            root: Some(fixture.resolve_fixture_asset(&cache.root)),
            fps: cache.fps,
            format: cache.format.clone(),
            naming,
            frame_count,
            max_cached_frames: 12,
            frames: HashMap::new(),
            lru: VecDeque::new(),
            current_frame_index: None,
            last_error: None,
        }
    }

    pub fn frame_for_time(&mut self, ctx: &egui::Context, time_ms: u64) -> Option<&LoadedFrame> {
        let frame_index = self.frame_index_for_time(time_ms);
        match self.ensure_frame(ctx, frame_index) {
            Ok(()) => {
                self.last_error = None;
                self.current_frame_index = Some(frame_index);
            }
            Err(error) => {
                self.last_error = Some(error);
            }
        }
        self.frames.get(&frame_index)
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    pub fn frame_count(&self) -> Option<usize> {
        self.frame_count
    }

    pub fn frame_path_for_time(&self, time_ms: u64) -> Option<PathBuf> {
        Some(self.frame_path(self.frame_index_for_time(time_ms)))
    }

    pub fn previous_frame_time_ms(&self, current_time_ms: u64) -> Option<u64> {
        let frame_index = self.frame_index_for_time(current_time_ms);
        (frame_index > 1).then(|| self.time_for_frame_index(frame_index - 1))
    }

    pub fn next_frame_time_ms(&self, current_time_ms: u64) -> Option<u64> {
        let frame_index = self.frame_index_for_time(current_time_ms);
        let next = frame_index + 1;
        if self.frame_count.is_some_and(|count| next > count) {
            None
        } else {
            Some(self.time_for_frame_index(next))
        }
    }

    pub fn frame_index_for_time(&self, time_ms: u64) -> usize {
        let fps = self.fps.max(0.1);
        let zero_based = ((time_ms as f32 / 1000.0) * fps).round().max(0.0) as usize;
        let frame_index = zero_based + 1;
        self.frame_count
            .map(|count| frame_index.clamp(1, count.max(1)))
            .unwrap_or(frame_index.max(1))
    }

    pub fn time_for_frame_index(&self, frame_index: usize) -> u64 {
        let fps = self.fps.max(0.1) as f64;
        let frame_index = self
            .frame_count
            .map(|count| frame_index.clamp(1, count.max(1)))
            .unwrap_or(frame_index.max(1));
        (((frame_index.saturating_sub(1)) as f64 / fps) * 1000.0).round() as u64
    }

    fn ensure_frame(&mut self, ctx: &egui::Context, frame_index: usize) -> Result<(), String> {
        if self.frames.contains_key(&frame_index) {
            self.touch(frame_index);
            return Ok(());
        }

        let frame = self.load_frame(ctx, frame_index)?;
        self.frames.insert(frame_index, frame);
        self.touch(frame_index);
        self.evict_old_frames();
        Ok(())
    }

    fn load_frame(&self, ctx: &egui::Context, frame_index: usize) -> Result<LoadedFrame, String> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| "missing frame cache root".to_string())?;
        if !root.exists() {
            return Err(format!("frame cache root missing: {}", root.display()));
        }

        let path = self.frame_path(frame_index);
        let image = image::open(&path)
            .map_err(|error| format!("load frame {} failed: {error}", path.display()))?
            .to_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        let color_image = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
        let texture = ctx.load_texture(
            format!("frame-cache-{frame_index}"),
            color_image,
            TextureOptions::LINEAR,
        );

        Ok(LoadedFrame {
            frame_index,
            time_ms: self.time_for_frame_index(frame_index),
            texture,
            size,
            path,
        })
    }

    fn frame_path(&self, frame_index: usize) -> PathBuf {
        let extension = match self.format.trim_start_matches('.') {
            "png" => "png",
            "jpeg" | "jpg" => "jpg",
            _ => "jpg",
        };
        let file_name = self
            .naming
            .replace("{index:06}", &format!("{frame_index:06}"))
            .replace("{index:03}", &format!("{frame_index:03}"))
            .replace("{index}", &frame_index.to_string());
        let file_name = if PathBuf::from(&file_name).extension().is_some() {
            file_name
        } else {
            format!("{file_name}.{extension}")
        };
        self.root.clone().unwrap_or_default().join(file_name)
    }

    fn touch(&mut self, frame_index: usize) {
        self.lru.retain(|index| *index != frame_index);
        self.lru.push_back(frame_index);
    }

    fn evict_old_frames(&mut self) {
        while self.frames.len() > self.max_cached_frames {
            let Some(frame_index) = self.lru.pop_front() else {
                break;
            };
            if Some(frame_index) == self.current_frame_index {
                self.lru.push_back(frame_index);
                continue;
            }
            self.frames.remove(&frame_index);
        }
    }
}
