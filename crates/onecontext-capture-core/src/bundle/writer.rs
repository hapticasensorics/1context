use crate::bundle::schema::CaptureBundleManifest;
use crate::error::{CaptureCoreError, CaptureCoreResult};
use crate::paths::{BundleRelativePath, CaptureRootPaths};
use serde::Serialize;
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct AtomicBundleWriter {
    paths: CaptureRootPaths,
    capture_id: String,
    partial_path: PathBuf,
    live_path: PathBuf,
}

impl AtomicBundleWriter {
    pub fn create(
        paths: CaptureRootPaths,
        capture_id: impl Into<String>,
    ) -> CaptureCoreResult<Self> {
        paths.ensure_directories()?;
        let capture_id = capture_id.into();
        let partial_path = paths.processing_dir.join(format!("{capture_id}.partial"));
        let live_path = paths.live_dir.join(&capture_id);
        if partial_path.exists() || live_path.exists() {
            return Err(CaptureCoreError::InvalidState(format!(
                "bundle already exists for capture_id {capture_id}"
            )));
        }
        fs::create_dir_all(&partial_path)
            .map_err(|error| CaptureCoreError::io(Some(partial_path.clone()), error))?;
        Ok(Self {
            paths,
            capture_id,
            partial_path,
            live_path,
        })
    }

    pub fn capture_id(&self) -> &str {
        &self.capture_id
    }

    pub fn partial_path(&self) -> &Path {
        &self.partial_path
    }

    pub fn write_json<T: Serialize>(&self, relative: &str, value: &T) -> CaptureCoreResult<()> {
        let data = serde_json::to_vec_pretty(value)
            .map_err(|error| CaptureCoreError::json(None, error))?;
        self.write_bytes(relative, &data)
    }

    pub fn write_jsonl_values<T: Serialize>(
        &self,
        relative: &str,
        values: &[T],
    ) -> CaptureCoreResult<()> {
        let mut data = Vec::new();
        for value in values {
            serde_json::to_writer(&mut data, value)
                .map_err(|error| CaptureCoreError::json(None, error))?;
            data.push(b'\n');
        }
        self.write_bytes(relative, &data)
    }

    pub fn write_raw_jsonl_lines(&self, relative: &str, lines: &[String]) -> CaptureCoreResult<()> {
        let mut data = Vec::new();
        for line in lines {
            data.extend_from_slice(line.as_bytes());
            data.push(b'\n');
        }
        self.write_bytes(relative, &data)
    }

    pub fn write_bytes(&self, relative: &str, data: &[u8]) -> CaptureCoreResult<()> {
        let relative = BundleRelativePath::parse(relative)?;
        let path = relative.join_under(&self.partial_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| CaptureCoreError::io(Some(parent.to_path_buf()), error))?;
        }
        let mut file =
            File::create(&path).map_err(|error| CaptureCoreError::io(Some(path.clone()), error))?;
        file.write_all(data)
            .map_err(|error| CaptureCoreError::io(Some(path), error))
    }

    pub fn copy_file_from(&self, relative: &str, source: &Path) -> CaptureCoreResult<u64> {
        let relative = BundleRelativePath::parse(relative)?;
        let destination = relative.join_under(&self.partial_path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| CaptureCoreError::io(Some(parent.to_path_buf()), error))?;
        }
        fs::copy(source, &destination)
            .map_err(|error| CaptureCoreError::io(Some(destination), error))
    }

    pub fn write_manifest(&self, manifest: &CaptureBundleManifest) -> CaptureCoreResult<()> {
        self.write_json("manifest.json", manifest)
    }

    pub fn write_ready_sentinel(&self) -> CaptureCoreResult<()> {
        self.write_bytes("READY", b"READY\n")
    }

    pub fn remove_ready_sentinel(&self) -> CaptureCoreResult<()> {
        let path = self.partial_path.join("READY");
        if path.exists() {
            fs::remove_file(&path).map_err(|error| CaptureCoreError::io(Some(path), error))?;
        }
        Ok(())
    }

    pub fn compute_partial_totals(&self) -> CaptureCoreResult<(u64, u64)> {
        compute_tree_totals(&self.partial_path)
    }

    pub fn promote(self) -> CaptureCoreResult<PathBuf> {
        fs::rename(&self.partial_path, &self.live_path)
            .map_err(|error| CaptureCoreError::io(Some(self.live_path.clone()), error))?;
        Ok(self.live_path)
    }

    pub fn fail(self) -> CaptureCoreResult<PathBuf> {
        let failed_path = self.paths.failed_dir.join(&self.capture_id);
        if failed_path.exists() {
            fs::remove_dir_all(&failed_path)
                .map_err(|error| CaptureCoreError::io(Some(failed_path.clone()), error))?;
        }
        fs::rename(&self.partial_path, &failed_path)
            .map_err(|error| CaptureCoreError::io(Some(failed_path.clone()), error))?;
        Ok(failed_path)
    }

    pub fn move_to_failed(self) -> CaptureCoreResult<PathBuf> {
        let failed_path = self.paths.failed_dir.join(&self.capture_id);
        if failed_path.exists() {
            return Err(CaptureCoreError::InvalidState(format!(
                "failed bundle already exists for capture_id {}",
                self.capture_id
            )));
        }
        fs::create_dir_all(&self.paths.failed_dir)
            .map_err(|error| CaptureCoreError::io(Some(self.paths.failed_dir.clone()), error))?;
        fs::rename(&self.partial_path, &failed_path)
            .map_err(|error| CaptureCoreError::io(Some(failed_path.clone()), error))?;
        Ok(failed_path)
    }

    pub fn paths(&self) -> &CaptureRootPaths {
        &self.paths
    }
}

pub fn compute_tree_totals(root: &Path) -> CaptureCoreResult<(u64, u64)> {
    let mut bytes = 0_u64;
    let mut files = 0_u64;
    for file in walk_files(root)? {
        let metadata =
            fs::metadata(&file).map_err(|error| CaptureCoreError::io(Some(file.clone()), error))?;
        bytes = bytes.saturating_add(metadata.len());
        files += 1;
    }
    Ok((bytes, files))
}

pub fn walk_files(root: &Path) -> CaptureCoreResult<Vec<PathBuf>> {
    let mut files = Vec::new();
    if !root.exists() {
        return Ok(files);
    }
    let mut stack = vec![root.to_path_buf()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory)
            .map_err(|error| CaptureCoreError::io(Some(directory.clone()), error))?
        {
            let entry =
                entry.map_err(|error| CaptureCoreError::io(Some(directory.clone()), error))?;
            let path = entry.path();
            let metadata = entry
                .metadata()
                .map_err(|error| CaptureCoreError::io(Some(path.clone()), error))?;
            if metadata.is_dir() {
                stack.push(path);
            } else if metadata.is_file() {
                files.push(path);
            }
        }
    }
    Ok(files)
}
