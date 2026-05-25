use crate::bundle::schema::{BundleState, CaptureBundleManifest};
use crate::bundle::writer::compute_tree_totals;
use crate::error::{CaptureCoreError, CaptureCoreResult};
use crate::paths::CaptureRootPaths;
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BundleDirectoryClass {
    Processing,
    Live,
    Failed,
    Pinned,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleEntry {
    pub capture_id: String,
    pub directory_class: BundleDirectoryClass,
    pub path: PathBuf,
    #[serde(default)]
    pub state: Option<BundleState>,
    #[serde(default)]
    pub ready: bool,
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub byte_count: u64,
    #[serde(default)]
    pub file_count: u64,
    #[serde(default)]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub ready_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BundleInventory {
    pub capture_root: PathBuf,
    pub entries: Vec<BundleEntry>,
    pub total_bytes: u64,
    pub total_files: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RetentionPolicy {
    pub processing_stale_after_seconds: i64,
    pub live_ttl_seconds: i64,
    pub failed_ttl_seconds: i64,
    pub keep_last_ready: usize,
    pub dry_run: bool,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            processing_stale_after_seconds: 15 * 60,
            live_ttl_seconds: 60 * 60,
            failed_ttl_seconds: 72 * 60 * 60,
            keep_last_ready: 20,
            dry_run: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SweepActionKind {
    Delete,
    MoveToFailed,
    Preserve,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SweepAction {
    pub kind: SweepActionKind,
    pub path: PathBuf,
    #[serde(default)]
    pub destination: Option<PathBuf>,
    pub reason: String,
    pub byte_count: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SweepPlan {
    pub generated_at: DateTime<Utc>,
    pub dry_run: bool,
    pub actions: Vec<SweepAction>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SweepReport {
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub dry_run: bool,
    pub deleted_paths: Vec<PathBuf>,
    pub moved_paths: Vec<PathBuf>,
    pub preserved_paths: Vec<PathBuf>,
    pub deleted_bytes: u64,
    pub errors: Vec<String>,
}

pub fn list_bundles(capture_root: impl AsRef<Path>) -> CaptureCoreResult<BundleInventory> {
    let paths = CaptureRootPaths::new(capture_root.as_ref());
    let mut entries = Vec::new();
    collect_entries(
        &paths.processing_dir,
        BundleDirectoryClass::Processing,
        &mut entries,
    )?;
    collect_entries(&paths.live_dir, BundleDirectoryClass::Live, &mut entries)?;
    collect_entries(
        &paths.failed_dir,
        BundleDirectoryClass::Failed,
        &mut entries,
    )?;
    collect_entries(
        &paths.pinned_dir,
        BundleDirectoryClass::Pinned,
        &mut entries,
    )?;
    let total_bytes = entries.iter().map(|entry| entry.byte_count).sum();
    let total_files = entries.iter().map(|entry| entry.file_count).sum();
    Ok(BundleInventory {
        capture_root: paths.capture_root,
        entries,
        total_bytes,
        total_files,
    })
}

pub fn plan_retention_sweep(
    capture_root: impl AsRef<Path>,
    policy: &RetentionPolicy,
    now: DateTime<Utc>,
) -> CaptureCoreResult<SweepPlan> {
    let inventory = list_bundles(capture_root.as_ref())?;
    let mut actions = Vec::new();
    let mut live_ready = inventory
        .entries
        .iter()
        .filter(|entry| {
            entry.directory_class == BundleDirectoryClass::Live && entry.ready && !entry.pinned
        })
        .collect::<Vec<_>>();
    live_ready.sort_by_key(|entry| entry.ready_at.or(entry.created_at));
    live_ready.reverse();

    for entry in &inventory.entries {
        if entry.pinned || entry.directory_class == BundleDirectoryClass::Pinned {
            actions.push(preserve(entry, "pinned bundle"));
            continue;
        }
        match entry.directory_class {
            BundleDirectoryClass::Processing => {
                if is_older_than(entry.created_at, now, policy.processing_stale_after_seconds) {
                    let destination = entry.path.file_name().map(|name| {
                        CaptureRootPaths::new(&inventory.capture_root)
                            .failed_dir
                            .join(name)
                    });
                    actions.push(SweepAction {
                        kind: if entry.file_count == 0 {
                            SweepActionKind::Delete
                        } else {
                            SweepActionKind::MoveToFailed
                        },
                        path: entry.path.clone(),
                        destination,
                        reason: "stale processing bundle".to_string(),
                        byte_count: entry.byte_count,
                    });
                } else {
                    actions.push(preserve(entry, "processing bundle still fresh"));
                }
            }
            BundleDirectoryClass::Live => {
                let keep_rank = live_ready
                    .iter()
                    .position(|candidate| candidate.path == entry.path);
                let keep_by_rank = keep_rank.is_some_and(|rank| rank < policy.keep_last_ready);
                let expired = entry.expires_at.is_some_and(|expires_at| expires_at <= now)
                    || is_older_than(
                        entry.ready_at.or(entry.created_at),
                        now,
                        policy.live_ttl_seconds,
                    );
                if expired && !keep_by_rank {
                    actions.push(delete(entry, "expired live bundle"));
                } else {
                    actions.push(preserve(entry, "live bundle retained"));
                }
            }
            BundleDirectoryClass::Failed => {
                if is_older_than(entry.created_at, now, policy.failed_ttl_seconds) {
                    actions.push(delete(entry, "expired failed bundle"));
                } else {
                    actions.push(preserve(entry, "failed bundle retained for audit"));
                }
            }
            BundleDirectoryClass::Pinned => actions.push(preserve(entry, "pinned bundle")),
        }
    }

    Ok(SweepPlan {
        generated_at: now,
        dry_run: policy.dry_run,
        actions,
    })
}

pub fn apply_retention_sweep(plan: &SweepPlan) -> SweepReport {
    let started_at = Utc::now();
    let mut report = SweepReport {
        started_at,
        completed_at: started_at,
        dry_run: plan.dry_run,
        deleted_paths: Vec::new(),
        moved_paths: Vec::new(),
        preserved_paths: Vec::new(),
        deleted_bytes: 0,
        errors: Vec::new(),
    };
    for action in &plan.actions {
        match action.kind {
            SweepActionKind::Preserve => report.preserved_paths.push(action.path.clone()),
            SweepActionKind::Delete => {
                if !plan.dry_run {
                    if let Err(error) = fs::remove_dir_all(&action.path) {
                        report
                            .errors
                            .push(format!("{}: {error}", action.path.display()));
                        continue;
                    }
                }
                report.deleted_bytes = report.deleted_bytes.saturating_add(action.byte_count);
                report.deleted_paths.push(action.path.clone());
            }
            SweepActionKind::MoveToFailed => {
                if let Some(destination) = &action.destination {
                    if !plan.dry_run {
                        if let Some(parent) = destination.parent() {
                            if let Err(error) = fs::create_dir_all(parent) {
                                report.errors.push(format!("{}: {error}", parent.display()));
                                continue;
                            }
                        }
                        if let Err(error) = fs::rename(&action.path, destination) {
                            report
                                .errors
                                .push(format!("{}: {error}", action.path.display()));
                            continue;
                        }
                    }
                    report.moved_paths.push(destination.clone());
                }
            }
        }
    }
    report.completed_at = Utc::now();
    report
}

pub fn sweep_bundles(
    capture_root: impl AsRef<Path>,
    policy: &RetentionPolicy,
    now: DateTime<Utc>,
) -> CaptureCoreResult<SweepReport> {
    let plan = plan_retention_sweep(capture_root.as_ref(), policy, now)?;
    let report = apply_retention_sweep(&plan);
    append_sweep_audit(capture_root, &report)?;
    Ok(report)
}

pub fn append_sweep_audit(
    capture_root: impl AsRef<Path>,
    report: &SweepReport,
) -> CaptureCoreResult<()> {
    let paths = CaptureRootPaths::new(capture_root.as_ref());
    fs::create_dir_all(&paths.retention_dir)
        .map_err(|error| CaptureCoreError::io(Some(paths.retention_dir.clone()), error))?;
    let mut line =
        serde_json::to_vec(report).map_err(|error| CaptureCoreError::json(None, error))?;
    line.push(b'\n');
    use std::io::Write;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&paths.sweeps_log)
        .map_err(|error| CaptureCoreError::io(Some(paths.sweeps_log.clone()), error))?;
    file.write_all(&line)
        .map_err(|error| CaptureCoreError::io(Some(paths.sweeps_log.clone()), error))
}

fn collect_entries(
    directory: &Path,
    class: BundleDirectoryClass,
    entries: &mut Vec<BundleEntry>,
) -> CaptureCoreResult<()> {
    let Ok(read_dir) = fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in read_dir {
        let entry =
            entry.map_err(|error| CaptureCoreError::io(Some(directory.to_path_buf()), error))?;
        let path = entry.path();
        if !entry
            .metadata()
            .map_err(|error| CaptureCoreError::io(Some(path.clone()), error))?
            .is_dir()
        {
            continue;
        }
        let manifest = read_manifest(&path.join("manifest.json")).ok();
        let (byte_count, file_count) = compute_tree_totals(&path)?;
        let capture_id = manifest
            .as_ref()
            .map(|manifest| manifest.capture_id.clone())
            .or_else(|| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_string)
            })
            .unwrap_or_else(|| "unknown".to_string());
        entries.push(BundleEntry {
            capture_id,
            directory_class: class.clone(),
            ready: path.join("READY").exists(),
            pinned: manifest.as_ref().is_some_and(|manifest| manifest.pinned)
                || class == BundleDirectoryClass::Pinned,
            state: manifest.as_ref().map(|manifest| manifest.state.clone()),
            byte_count,
            file_count,
            created_at: manifest.as_ref().map(|manifest| manifest.created_at),
            ready_at: manifest.as_ref().and_then(|manifest| manifest.ready_at),
            expires_at: manifest.as_ref().and_then(|manifest| manifest.expires_at),
            path,
        });
    }
    Ok(())
}

fn read_manifest(path: &Path) -> CaptureCoreResult<CaptureBundleManifest> {
    let text = fs::read_to_string(path)
        .map_err(|error| CaptureCoreError::io(Some(path.to_path_buf()), error))?;
    serde_json::from_str(&text)
        .map_err(|error| CaptureCoreError::json(Some(path.to_path_buf()), error))
}

fn is_older_than(time: Option<DateTime<Utc>>, now: DateTime<Utc>, seconds: i64) -> bool {
    time.is_some_and(|time| now.signed_duration_since(time) > Duration::seconds(seconds))
}

fn delete(entry: &BundleEntry, reason: &str) -> SweepAction {
    SweepAction {
        kind: SweepActionKind::Delete,
        path: entry.path.clone(),
        destination: None,
        reason: reason.to_string(),
        byte_count: entry.byte_count,
    }
}

fn preserve(entry: &BundleEntry, reason: &str) -> SweepAction {
    SweepAction {
        kind: SweepActionKind::Preserve,
        path: entry.path.clone(),
        destination: None,
        reason: reason.to_string(),
        byte_count: entry.byte_count,
    }
}
