//! Shared Rust implementation for 1Context capture bundles.
//!
//! This crate owns the file-level handoff contract up to READY bundle
//! production. It does not run macOS sensors, score attention, or write durable
//! Timescale memory rows.

pub mod bundle;
pub mod error;
pub mod event;
pub mod fixtures;
pub mod lanes;
pub mod paths;
pub mod retention;
pub mod spool;
pub mod spool_index;

pub use bundle::{
    export_ready_bundle, validate_ready_bundle, AtomicBundleWriter, BundleState,
    CaptureBundleManifest, CaptureTarget, ExportRequest, ExportResponse, KnownGapRecord,
    LaneSource, RetentionClass, SourceInventory, SourceStatus, ValidationFinding, ValidationReport,
    ValidationSeverity,
};
pub use error::{CaptureCoreError, CaptureCoreResult};
pub use event::{CaptureEventEnvelope, RawSpoolProvenance, RawSpoolRecord};
pub use fixtures::{seed_demo_capture_spool, DemoCaptureFixture, DemoCaptureLane};
pub use lanes::{mandatory_lane_ids, required_bundle_files, CONTRACT_VERSION};
pub use paths::{BundleRelativePath, CaptureRootPaths};
pub use retention::{
    append_sweep_audit, apply_retention_sweep, list_bundles, plan_retention_sweep, sweep_bundles,
    BundleDirectoryClass, BundleEntry, BundleInventory, RetentionPolicy, SweepAction,
    SweepActionKind, SweepPlan, SweepReport,
};
pub use spool::{
    read_spool_window, read_spool_window_report, read_spool_window_strict,
    read_spool_window_tolerant, MalformedSpoolLine, SpoolFileStats, SpoolQuery, SpoolReadMode,
    SpoolReadReport,
};
pub use spool_index::{
    query_windows_jsonl_time_index, rebuild_windows_jsonl_time_index,
    windows_jsonl_time_index_path, write_windows_jsonl_time_index, SourceFileFingerprint,
    UnindexedWindowsJsonlLine, UnindexedWindowsJsonlLineReason, WindowsJsonlIndexFallback,
    WindowsJsonlIndexFallbackReason, WindowsJsonlIndexLookup, WindowsJsonlIndexedRange,
    WindowsJsonlRangeLookup, WindowsJsonlTimeIndex, WindowsJsonlTimeIndexEntry,
};
