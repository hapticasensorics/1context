use onecontext_context_engine::CONTEXT_ENGINE_SCHEMA_VERSION;
use serde_json::json;
use std::env;

fn main() {
    match run() {
        Ok(payload) => {
            println!("{}", serde_json::to_string_pretty(&payload).unwrap());
        }
        Err(error) => {
            eprintln!(
                "{}",
                serde_json::to_string_pretty(&json!({
                    "schema_version": CONTEXT_ENGINE_SCHEMA_VERSION,
                    "status": "error",
                    "surface": "context_engine",
                    "error": { "message": error },
                }))
                .unwrap()
            );
            std::process::exit(1);
        }
    }
}

fn run() -> Result<serde_json::Value, String> {
    let mut args: Vec<String> = env::args().skip(1).collect();
    let command = args
        .first()
        .cloned()
        .unwrap_or_else(|| "describe".to_string());
    if !args.is_empty() {
        args.remove(0);
    }
    match command.as_str() {
        "describe" => Ok(json!({
            "schema_version": CONTEXT_ENGINE_SCHEMA_VERSION,
            "status": "ok",
            "surface": "context_engine",
            "commands": ["describe", "update-wiki"],
            "release_boundary": {
                "context_engine": "native_release_owner",
                "orchestrator": "onecontext-context-engine",
                "model_transport": "codex_app_server",
            }
        })),
        "update-wiki" => update_wiki(args),
        "--help" | "-h" | "help" => Ok(json!({
            "usage": "onecontext-context-engine describe | update-wiki --root <1Context-root> [--run-id ID] [--trigger NAME] [--execute-agents|--no-agents] [--max-concurrent N] [--source-window-days N] [--mode recent-first|incremental|backfill|dry-run] [--json]"
        })),
        other => Err(format!("unknown command {other:?}")),
    }
}

fn update_wiki(args: Vec<String>) -> Result<serde_json::Value, String> {
    let mut root: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = Some(required_value(&args, index, "--root")?);
            }
            "--run-id" => {
                index += 1;
                required_value(&args, index, "--run-id")?;
            }
            "--trigger" => {
                index += 1;
                required_value(&args, index, "--trigger")?;
            }
            "--max-concurrent" => {
                index += 1;
                parse_positive_u32(
                    &required_value(&args, index, "--max-concurrent")?,
                    "--max-concurrent",
                )?;
            }
            "--source-window-days" => {
                index += 1;
                parse_positive_u32(
                    &required_value(&args, index, "--source-window-days")?,
                    "--source-window-days",
                )?;
            }
            "--mode" => {
                index += 1;
                parse_mode(&required_value(&args, index, "--mode")?)?;
            }
            "--no-agents" | "--execute-agents" | "--json" => {}
            other => return Err(format!("unknown update-wiki flag {other:?}")),
        }
        index += 1;
    }
    root.ok_or_else(|| "--root is required".to_string())?;
    Err("wiki update unavailable until the FSM DSL runner lands".to_string())
}

fn required_value(args: &[String], index: usize, flag: &str) -> Result<String, String> {
    args.get(index)
        .filter(|value| !value.starts_with("--"))
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

fn parse_positive_u32(value: &str, flag: &str) -> Result<u32, String> {
    let parsed = value
        .parse::<u32>()
        .map_err(|_| format!("{flag} must be a positive integer"))?;
    if parsed == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(parsed)
}

fn parse_mode(value: &str) -> Result<(), String> {
    match value {
        "recent-first" | "recent_first" | "recent-first-then-backfill" | "incremental"
        | "backfill" | "dry-run" | "dry_run" => Ok(()),
        other => Err(format!("unknown mode {other:?}")),
    }
}
