use std::{fs, path::Path};

use anyhow::{Context, Result};

use crate::model::AttentionFilterOutput;

pub fn write_filter_output(path: &Path, output: &AttentionFilterOutput) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }
    let json = serde_json::to_string_pretty(output).context("serialize attention filter output")?;
    fs::write(path, format!("{json}\n")).with_context(|| format!("write {}", path.display()))?;
    Ok(())
}
