pub mod bundle;
pub mod candidates;
pub mod events;
pub mod fixture;
pub mod fusion;
pub mod model;
pub mod output;
pub mod signals;

use std::path::Path;

use anyhow::Result;

use crate::{
    bundle::{prepare_bundle_run, write_dashboard_session},
    candidates::build_candidates,
    fixture::AttentionFixture,
    fusion::fuse_attention,
    output::write_filter_output,
    signals::score_candidates,
};

pub fn run_attention_filter(session_path: &Path, output_path: Option<&Path>) -> Result<RunSummary> {
    let fixture = AttentionFixture::load(session_path)?;
    let candidates = build_candidates(&fixture)?;
    let scored = score_candidates(&fixture, candidates)?;
    let output = fuse_attention(&fixture, scored)?;
    let path = match output_path {
        Some(path) => path.to_path_buf(),
        None => fixture.default_output_path(),
    };

    write_filter_output(&path, &output)?;

    Ok(RunSummary {
        output_path: path,
        dashboard_session_path: None,
        candidates: output.raw_buffer_audit.len(),
        saved: output.saved_states.len(),
    })
}

pub fn run_attention_filter_on_bundle(
    bundle_path: &Path,
    output_path: Option<&Path>,
) -> Result<RunSummary> {
    let bundle_run = prepare_bundle_run(bundle_path, output_path)?;
    let candidates = build_candidates(&bundle_run.fixture)?;
    let scored = score_candidates(&bundle_run.fixture, candidates)?;
    let output = fuse_attention(&bundle_run.fixture, scored)?;

    write_filter_output(&bundle_run.output_path, &output)?;
    write_dashboard_session(&bundle_run)?;

    Ok(RunSummary {
        output_path: bundle_run.output_path,
        dashboard_session_path: Some(bundle_run.dashboard_session_path),
        candidates: output.raw_buffer_audit.len(),
        saved: output.saved_states.len(),
    })
}

#[derive(Debug, Clone)]
pub struct RunSummary {
    pub output_path: std::path::PathBuf,
    pub dashboard_session_path: Option<std::path::PathBuf>,
    pub candidates: usize,
    pub saved: usize,
}
