use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::schema::{AttentionDashboardSession, AttentionFilterOutput};

#[derive(Debug, Clone)]
pub struct DashboardFixture {
    pub session_path: PathBuf,
    pub session_dir: PathBuf,
    pub fixture_root: PathBuf,
    pub session: AttentionDashboardSession,
    pub filter_output: AttentionFilterOutput,
}

impl DashboardFixture {
    pub fn load(session_path: &Path) -> Result<Self> {
        let session_path = normalize_path(session_path);
        let session_dir = session_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .to_path_buf();

        let session_text = fs::read_to_string(&session_path)
            .with_context(|| format!("read session {}", session_path.display()))?;
        let session: AttentionDashboardSession = serde_json::from_str(&session_text)
            .with_context(|| format!("parse session {}", session_path.display()))?;
        let fixture_root = resolve_root(&session.fixture.root, &session_dir);

        let filter_output_path = session_dir.join(&session.filter_output.path);
        let filter_output_text = fs::read_to_string(&filter_output_path)
            .with_context(|| format!("read filter output {}", filter_output_path.display()))?;
        let filter_output = serde_json::from_str(&filter_output_text)
            .with_context(|| format!("parse filter output {}", filter_output_path.display()))?;

        Ok(Self {
            session_path,
            session_dir,
            fixture_root,
            session,
            filter_output,
        })
    }

    pub fn duration_ms(&self) -> u64 {
        self.session.fixture.duration_ms
    }

    pub fn resolve(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.session_dir.join(path)
        }
    }

    pub fn resolve_fixture_asset(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.fixture_root.join(path)
        }
    }

    pub fn review_labels_path(&self) -> PathBuf {
        self.resolve(&self.session.review.labels_ref)
    }

    pub fn candidate_times_ms(&self) -> Vec<u64> {
        let mut times: Vec<u64> = self
            .filter_output
            .raw_buffer_audit
            .iter()
            .map(|item| item.t_ms.min(self.duration_ms()))
            .collect();

        if times.is_empty() {
            if let Some(frame_set) = self.preferred_candidate_frame_set() {
                let fps = frame_set.fps.max(0.1) as f64;
                times.extend((1..=frame_set.count).map(|index| {
                    (((index.saturating_sub(1)) as f64 / fps) * 1000.0)
                        .round()
                        .max(0.0) as u64
                }));
            }
        }

        times.sort_unstable();
        times.dedup();
        times
            .into_iter()
            .filter(|time_ms| *time_ms <= self.duration_ms())
            .collect()
    }

    pub fn previous_candidate_time_ms(&self, current_time_ms: u64) -> Option<u64> {
        self.candidate_times_ms()
            .into_iter()
            .rev()
            .find(|time_ms| *time_ms < current_time_ms)
    }

    pub fn next_candidate_time_ms(&self, current_time_ms: u64) -> Option<u64> {
        self.candidate_times_ms()
            .into_iter()
            .find(|time_ms| *time_ms > current_time_ms)
    }

    pub fn metadata_lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        lines.push(format!("title: {}", self.session.title));
        lines.push(format!("run: {}", self.session.fixture.run_id));
        lines.push(format!(
            "duration: {:.2}s",
            self.duration_ms() as f32 / 1000.0
        ));
        lines.push(format!(
            "video: {}x{} / {:.2}s{}",
            self.session.media.video_width,
            self.session.media.video_height,
            self.session.media.video_duration_ms as f32 / 1000.0,
            self.session
                .media
                .video_fps
                .map(|fps| format!(" / {fps:.2} fps"))
                .unwrap_or_default()
        ));
        if let Some(cache) = &self.session.media.frame_cache {
            lines.push(format!(
                "frame cache: {}x{} / {:.2} fps / {}",
                cache.frame_width, cache.frame_height, cache.fps, cache.root
            ));
        }
        lines.push(format!(
            "filter output: {} saved / {} raw candidates",
            self.filter_output.saved_states.len(),
            self.filter_output.raw_buffer_audit.len()
        ));
        lines.push(format!(
            "candidate times: {}",
            self.candidate_times_ms().len()
        ));
        lines
    }

    pub fn asset_status_lines(&self) -> Vec<(bool, String)> {
        let mut lines = Vec::new();
        lines.push(self.asset_line("session", &self.session_path));
        lines.push(self.asset_line(
            "video",
            self.resolve_fixture_asset(&self.session.media.video_ref),
        ));
        if let Some(cache) = &self.session.media.frame_cache {
            lines.push(self.asset_line("frame cache", self.resolve_fixture_asset(&cache.root)));
            lines
                .push(self.asset_line("frame index", self.resolve_fixture_asset(&cache.index_ref)));
        }
        for event_ref in &self.session.inputs.event_refs {
            if event_ref.required {
                lines.push(
                    self.asset_line(&event_ref.id, self.resolve_fixture_asset(&event_ref.path)),
                );
            }
        }
        lines
    }

    fn asset_line(&self, label: impl AsRef<str>, path: impl AsRef<Path>) -> (bool, String) {
        let path = path.as_ref();
        let exists = path.exists();
        (
            exists,
            format!(
                "{}: {}{}",
                label.as_ref(),
                path.display(),
                if exists { "" } else { " (missing)" }
            ),
        )
    }

    fn preferred_candidate_frame_set(&self) -> Option<&crate::schema::CandidateFrameSet> {
        let frame_cache_root = self
            .session
            .media
            .frame_cache
            .as_ref()
            .map(|cache| cache.root.as_str());
        self.session
            .media
            .candidate_frame_sets
            .iter()
            .find(|set| Some(set.root.as_str()) == frame_cache_root)
            .or_else(|| self.session.media.candidate_frame_sets.first())
    }
}

fn normalize_path(path: &Path) -> PathBuf {
    if path.exists() {
        fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
    } else if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    }
}

fn resolve_root(root: &str, session_dir: &Path) -> PathBuf {
    let path = Path::new(root);
    if path.is_absolute() {
        return path.to_path_buf();
    }

    let cwd_candidate = PathBuf::from(root);
    if cwd_candidate.exists() {
        return fs::canonicalize(&cwd_candidate).unwrap_or(cwd_candidate);
    }

    session_dir.join(path)
}
