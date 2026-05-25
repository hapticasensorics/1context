mod app;
mod fixture;
mod media;
mod panels;
mod review;
mod schema;
mod timeline;

use std::path::PathBuf;

use anyhow::{bail, Result};

use crate::app::AttentionDashboardApp;

const DEFAULT_SESSION_PATH: &str = "docs/assets/attention-capture-mockup/attention-debug-20260524-215739/attention-dashboard-session.json";

fn main() -> eframe::Result<()> {
    let session_path = match parse_args() {
        Ok(Some(path)) => path,
        Ok(None) => {
            print_usage();
            return Ok(());
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1440.0, 920.0])
            .with_min_inner_size([1080.0, 720.0])
            .with_title("1Context Attention Dashboard"),
        ..Default::default()
    };

    eframe::run_native(
        "1Context Attention Dashboard",
        options,
        Box::new(move |cc| {
            Ok(Box::new(AttentionDashboardApp::new(
                cc,
                session_path.clone(),
            )))
        }),
    )
}

fn parse_args() -> Result<Option<PathBuf>> {
    let mut args = std::env::args_os().skip(1);
    let mut session_path = None;

    while let Some(arg) = args.next() {
        if arg == "--session" {
            let Some(value) = args.next() else {
                bail!("--session requires a path");
            };
            session_path = Some(PathBuf::from(value));
        } else if arg == "--help" || arg == "-h" {
            return Ok(None);
        } else {
            bail!("unknown argument: {}", arg.to_string_lossy());
        }
    }

    Ok(Some(session_path.unwrap_or_else(default_session_path)))
}

fn print_usage() {
    println!(
        "usage: onecontext-attention-dashboard [--session <attention-dashboard-session.json>]\n\
         default session: {DEFAULT_SESSION_PATH}"
    );
}

fn default_session_path() -> PathBuf {
    let cwd_path = PathBuf::from(DEFAULT_SESSION_PATH);
    if cwd_path.exists() {
        return cwd_path;
    }

    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(DEFAULT_SESSION_PATH)
}
