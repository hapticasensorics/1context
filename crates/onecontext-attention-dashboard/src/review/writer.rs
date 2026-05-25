use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
};

use anyhow::{Context, Result};

use super::labels::ReviewLabelEvent;

#[derive(Debug, Default)]
pub struct LabelLoadResult {
    pub labels: Vec<ReviewLabelEvent>,
    pub skipped_errors: Vec<String>,
}

pub fn load_labels(path: &Path) -> Result<LabelLoadResult> {
    if !path.exists() {
        return Ok(LabelLoadResult::default());
    }

    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut result = LabelLoadResult::default();

    for (index, line) in reader.lines().enumerate() {
        let line_number = index + 1;
        let line = line.with_context(|| format!("read {}:{line_number}", path.display()))?;
        if line.trim().is_empty() {
            continue;
        }

        match serde_json::from_str::<ReviewLabelEvent>(&line) {
            Ok(label) => result.labels.push(label),
            Err(error) => result
                .skipped_errors
                .push(format!("{}:{line_number}: {error}", path.display())),
        }
    }

    Ok(result)
}

pub fn append_label(path: &Path, label: &ReviewLabelEvent) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("open {}", path.display()))?;

    serde_json::to_writer(&mut file, label)
        .with_context(|| format!("serialize label {}", label.label_id))?;
    file.write_all(b"\n")
        .with_context(|| format!("write newline {}", path.display()))?;
    file.flush()
        .with_context(|| format!("flush {}", path.display()))?;

    Ok(())
}
