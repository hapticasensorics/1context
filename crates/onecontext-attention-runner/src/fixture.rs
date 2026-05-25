use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};

use crate::{
    events::{event_epoch_ms, load_capture_events, parse_rfc3339_ms},
    model::{CaptureEvent, DashboardSession},
};

#[derive(Debug, Clone)]
pub struct AttentionFixture {
    pub session_path: PathBuf,
    pub root: PathBuf,
    pub session: DashboardSession,
    pub events: Vec<CaptureEvent>,
}

impl AttentionFixture {
    pub fn load(session_path: &Path) -> Result<Self> {
        let session_text = fs::read_to_string(session_path)
            .with_context(|| format!("read session {}", session_path.display()))?;
        let session: DashboardSession = serde_json::from_str(&session_text)
            .with_context(|| format!("parse session {}", session_path.display()))?;
        let root = resolve_session_root(session_path, &session.fixture.root);
        let mut events = Vec::new();
        let event_base_epoch_ms = snapshot_index_base_epoch_ms(&root, &session)?
            .or_else(|| capture_events_base_epoch_ms(&root, &session).unwrap_or(None));

        for event_ref in &session.inputs.event_refs {
            if event_ref.kind == "capture_events" {
                events.extend(load_capture_events(&root, event_ref, event_base_epoch_ms)?);
            }
        }

        Ok(Self {
            session_path: session_path.to_path_buf(),
            root,
            session,
            events,
        })
    }

    pub fn resolve(&self, path: &str) -> PathBuf {
        normalize_path(self.root.join(path))
    }

    pub fn default_output_path(&self) -> PathBuf {
        self.resolve(&self.session.filter_output.path)
    }
}

fn snapshot_index_base_epoch_ms(root: &Path, session: &DashboardSession) -> Result<Option<i64>> {
    let Some(index_ref) = &session.inputs.candidate_index_ref else {
        return Ok(None);
    };
    let path = root.join(index_ref);
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    Ok(text.lines().find_map(snapshot_index_line_epoch_ms))
}

fn snapshot_index_line_epoch_ms(line: &str) -> Option<i64> {
    let mut fields = line.split('\t');
    fields.next()?;
    let timestamp = fields.next()?;
    parse_rfc3339_ms(timestamp)
}

fn capture_events_base_epoch_ms(root: &Path, session: &DashboardSession) -> Result<Option<i64>> {
    let mut base_epoch_ms: Option<i64> = None;

    for event_ref in &session.inputs.event_refs {
        if event_ref.kind != "capture_events" {
            continue;
        }
        let path = root.join(&event_ref.path);
        let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
        let min_epoch_ms = text
            .lines()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str(line).ok())
            .filter_map(|value| event_epoch_ms(&value))
            .min();
        base_epoch_ms = match (base_epoch_ms, min_epoch_ms) {
            (Some(current), Some(candidate)) => Some(current.min(candidate)),
            (None, Some(candidate)) => Some(candidate),
            (current, None) => current,
        };
    }

    Ok(base_epoch_ms)
}

fn normalize_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

fn resolve_session_root(session_path: &Path, root: &str) -> PathBuf {
    let root_path = PathBuf::from(root);
    if !root_path.is_absolute() {
        if let Some(parent) = session_path.parent() {
            let parent = normalize_path(parent.to_path_buf());
            if parent.ends_with(&root_path) {
                return parent;
            }
        }
    }

    let direct = normalize_path(PathBuf::from(root));
    if direct.exists() {
        return direct;
    }

    normalize_path(
        session_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(root),
    )
}
