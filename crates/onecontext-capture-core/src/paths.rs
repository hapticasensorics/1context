use crate::error::{CaptureCoreError, CaptureCoreResult};
use std::fs;
use std::path::{Component, Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureRootPaths {
    pub capture_root: PathBuf,
    pub events_dir: PathBuf,
    pub windows_dir: PathBuf,
    pub displays_dir: PathBuf,
    pub media_dir: PathBuf,
    pub bundles_dir: PathBuf,
    pub processing_dir: PathBuf,
    pub live_dir: PathBuf,
    pub failed_dir: PathBuf,
    pub pinned_dir: PathBuf,
    pub retention_dir: PathBuf,
    pub sweeps_log: PathBuf,
}

impl CaptureRootPaths {
    pub fn new(capture_root: impl Into<PathBuf>) -> Self {
        let capture_root = capture_root.into();
        let bundles_dir = capture_root.join("bundles");
        let retention_dir = capture_root.join("retention");
        Self {
            events_dir: capture_root.join("events"),
            windows_dir: capture_root.join("windows"),
            displays_dir: capture_root.join("displays"),
            media_dir: capture_root.join("media"),
            processing_dir: bundles_dir.join("processing"),
            live_dir: bundles_dir.join("live"),
            failed_dir: bundles_dir.join("failed"),
            pinned_dir: bundles_dir.join("pinned"),
            sweeps_log: retention_dir.join("sweeps.jsonl"),
            retention_dir,
            bundles_dir,
            capture_root,
        }
    }

    pub fn ensure_directories(&self) -> CaptureCoreResult<()> {
        for directory in [
            &self.capture_root,
            &self.events_dir,
            &self.windows_dir,
            &self.displays_dir,
            &self.media_dir,
            &self.bundles_dir,
            &self.processing_dir,
            &self.live_dir,
            &self.failed_dir,
            &self.pinned_dir,
            &self.retention_dir,
        ] {
            fs::create_dir_all(directory)
                .map_err(|error| CaptureCoreError::io(Some(directory.clone()), error))?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BundleRelativePath(String);

impl BundleRelativePath {
    pub fn parse(value: impl AsRef<str>) -> CaptureCoreResult<Self> {
        let value = value.as_ref();
        let path = Path::new(value);
        if value.trim().is_empty() {
            return Err(CaptureCoreError::InvalidPath {
                path: value.to_string(),
                message: "bundle path cannot be empty".to_string(),
            });
        }
        if path.is_absolute() {
            return Err(CaptureCoreError::InvalidPath {
                path: value.to_string(),
                message: "bundle path must be relative".to_string(),
            });
        }
        for component in path.components() {
            match component {
                Component::Normal(_) | Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(CaptureCoreError::InvalidPath {
                        path: value.to_string(),
                        message: "bundle path cannot escape bundle root".to_string(),
                    });
                }
            }
        }
        Ok(Self(value.replace('\\', "/")))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn join_under(&self, root: impl AsRef<Path>) -> PathBuf {
        root.as_ref().join(&self.0)
    }
}
