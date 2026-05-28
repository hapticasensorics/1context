use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
};

use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};

#[derive(Default)]
pub struct ImageAssetCache {
    max_cached_images: usize,
    images: HashMap<PathBuf, LoadedImage>,
    lru: VecDeque<PathBuf>,
    last_error: Option<String>,
}

pub struct LoadedImage {
    pub texture: TextureHandle,
    pub size: [usize; 2],
    pub path: PathBuf,
}

impl ImageAssetCache {
    pub fn new() -> Self {
        Self {
            max_cached_images: 96,
            ..Default::default()
        }
    }

    pub fn image_for_path(&mut self, ctx: &egui::Context, path: &Path) -> Option<&LoadedImage> {
        let path = normalize_path(path);
        match self.ensure_image(ctx, &path) {
            Ok(()) => {
                self.last_error = None;
            }
            Err(error) => {
                self.last_error = Some(error);
            }
        }
        self.images.get(&path)
    }

    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    fn ensure_image(&mut self, ctx: &egui::Context, path: &Path) -> Result<(), String> {
        if self.images.contains_key(path) {
            self.touch(path);
            return Ok(());
        }

        let image = load_image(ctx, path)?;
        self.images.insert(path.to_path_buf(), image);
        self.touch(path);
        self.evict_old_images();
        Ok(())
    }

    fn touch(&mut self, path: &Path) {
        self.lru.retain(|candidate| candidate != path);
        self.lru.push_back(path.to_path_buf());
    }

    fn evict_old_images(&mut self) {
        while self.images.len() > self.max_cached_images {
            let Some(path) = self.lru.pop_front() else {
                break;
            };
            self.images.remove(&path);
        }
    }
}

fn load_image(ctx: &egui::Context, path: &Path) -> Result<LoadedImage, String> {
    let image = image::open(path)
        .map_err(|error| format!("load image {} failed: {error}", path.display()))?
        .to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    let color_image = ColorImage::from_rgba_unmultiplied(size, image.as_raw());
    let texture = ctx.load_texture(
        format!("attention-output-image:{}", path.display()),
        color_image,
        TextureOptions::LINEAR,
    );

    Ok(LoadedImage {
        texture,
        size,
        path: path.to_path_buf(),
    })
}

fn normalize_path(path: &Path) -> PathBuf {
    if path.exists() {
        std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    }
}
