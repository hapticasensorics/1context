use std::path::PathBuf;

use onecontext_memory_db::{
    default_context_for_source, default_home_dir, ingest_claude_incremental,
    ingest_codex_incremental, ingest_imessage_incremental, probe_local_sources,
    sample_claude_objects, sample_codex_objects, sample_imessage_objects, AdapterSampleOptions,
    IncrementalIngestOptions, LocalIngestCursors, SessionIngestProfile,
};
use serde_json::json;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.is_empty() {
        print_usage();
        std::process::exit(2);
    }

    let command = args.remove(0);
    let home = take_option_value(&mut args, "--home")
        .map(PathBuf::from)
        .unwrap_or_else(default_home_dir);
    let limit = take_option_value(&mut args, "--limit")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(20);
    let max_events = take_option_value(&mut args, "--max-events")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1_000);
    let max_lines = take_option_value(&mut args, "--max-lines")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(50_000);
    let database_url = take_option_value(&mut args, "--database-url");
    let include_sensitive_text = take_flag(&mut args, "--include-sensitive-text");
    let session_profile = take_option_value(&mut args, "--profile")
        .as_deref()
        .map(parse_session_profile)
        .transpose()?
        .unwrap_or_default();

    match command.as_str() {
        "probe" => {
            let reports = probe_local_sources(&home);
            println!("{}", serde_json::to_string_pretty(&reports)?);
        }
        "sample" => {
            let source = take_option_value(&mut args, "--source").unwrap_or_else(|| {
                eprintln!("missing --source codex|claude|imessage");
                std::process::exit(2);
            });
            let context = default_context_for_source(&source)?;
            let options = AdapterSampleOptions {
                limit,
                include_sensitive_text,
                session_profile,
            };
            let objects = match source.as_str() {
                "codex" => sample_codex_objects(&home, &context, &options)?,
                "claude" => sample_claude_objects(&home, &context, &options)?,
                "imessage" => sample_imessage_objects(&home, &context, &options)?,
                _ => {
                    eprintln!("unsupported source {source:?}");
                    std::process::exit(2);
                }
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "source": source,
                    "home": home,
                    "object_count": objects.len(),
                    "objects": objects,
                }))?
            );
        }
        "ingest" => {
            let source = take_option_value(&mut args, "--source").unwrap_or_else(|| {
                eprintln!("missing --source codex|claude|imessage");
                std::process::exit(2);
            });
            let cursor_file = take_option_value(&mut args, "--cursor-file")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    eprintln!("missing --cursor-file PATH");
                    std::process::exit(2);
                });
            let context = default_context_for_source(&source)?;
            let options = IncrementalIngestOptions {
                max_events,
                max_lines,
                include_sensitive_text,
                session_profile,
            };
            let mut cursors = load_cursors(&cursor_file)?;
            let batch = match source.as_str() {
                "codex" => ingest_codex_incremental(&home, &context, &options, &mut cursors)?,
                "claude" => ingest_claude_incremental(&home, &context, &options, &mut cursors)?,
                "imessage" => ingest_imessage_incremental(&home, &context, &options, &mut cursors)?,
                _ => {
                    eprintln!("unsupported source {source:?}");
                    std::process::exit(2);
                }
            };
            save_cursors(&cursor_file, &cursors)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "source": source,
                    "home": home,
                    "cursor_file": cursor_file,
                    "batch": batch,
                }))?
            );
        }
        "migrate" => {
            let database_url = database_url
                .or_else(|| std::env::var("DATABASE_URL").ok())
                .unwrap_or_else(|| {
                    eprintln!("missing --database-url URL or DATABASE_URL");
                    std::process::exit(2);
                });
            let report = onecontext_memory_db::migrations::apply_bundled_migrations(&database_url)?;
            println!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "applied_count": report.applied.len(),
                    "skipped_count": report.skipped.len(),
                    "applied": report.applied,
                    "skipped": report.skipped,
                }))?
            );
        }
        _ => {
            print_usage();
            std::process::exit(2);
        }
    }

    Ok(())
}

fn print_usage() {
    eprintln!(
        "usage:\n  onecontext-memory-db probe [--home PATH]\n  onecontext-memory-db sample --source codex|claude|imessage [--limit N] [--profile hot_memory|compact_audit|forensic] [--include-sensitive-text] [--home PATH]\n  onecontext-memory-db ingest --source codex|claude|imessage --cursor-file PATH [--max-events N] [--max-lines N] [--profile hot_memory|compact_audit|forensic] [--include-sensitive-text] [--home PATH]\n  onecontext-memory-db migrate [--database-url URL]\n\nlegacy profile aliases accepted: messages-only, messages-and-compact-tools"
    );
}

fn load_cursors(path: &PathBuf) -> Result<LocalIngestCursors, Box<dyn std::error::Error>> {
    if !path.exists() {
        return Ok(LocalIngestCursors::default());
    }
    let bytes = std::fs::read(path)?;
    if bytes.is_empty() {
        return Ok(LocalIngestCursors::default());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn save_cursors(
    path: &PathBuf,
    cursors: &LocalIngestCursors,
) -> Result<(), Box<dyn std::error::Error>> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_vec_pretty(cursors)?)?;
    Ok(())
}

fn parse_session_profile(value: &str) -> Result<SessionIngestProfile, Box<dyn std::error::Error>> {
    match value {
        "hot_memory" | "hot-memory" | "messages-only" | "messages_only" => {
            Ok(SessionIngestProfile::HotMemory)
        }
        "compact_audit"
        | "compact-audit"
        | "messages-and-compact-tools"
        | "messages_and_compact_tools" => Ok(SessionIngestProfile::CompactAudit),
        "forensic" => Ok(SessionIngestProfile::Forensic),
        other => Err(format!(
            "unknown --profile {other:?}; expected hot_memory, compact_audit, or forensic"
        )
        .into()),
    }
}

fn take_option_value(args: &mut Vec<String>, name: &str) -> Option<String> {
    let index = args.iter().position(|arg| arg == name)?;
    args.remove(index);
    if index >= args.len() {
        return None;
    }
    Some(args.remove(index))
}

fn take_flag(args: &mut Vec<String>, name: &str) -> bool {
    if let Some(index) = args.iter().position(|arg| arg == name) {
        args.remove(index);
        true
    } else {
        false
    }
}
