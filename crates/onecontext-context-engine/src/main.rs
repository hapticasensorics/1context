use onecontext_context_engine::{
    agent_execution::execute_wiki_company_agents,
    append_wiki_company_mail_receipt, build_wiki_company_plan,
    orchestrator::{load_wiki_company_orchestrator_config, validate_wiki_company_orchestrator},
    pack::{load_wiki_company_pack, validate_wiki_company_pack},
    wiki_company::build_wiki_company_dry_run,
    ContextEnginePaths, WikiCompanyRunMode, WikiCompanyRunRequest, CONTEXT_ENGINE_SCHEMA_VERSION,
};
use serde_json::json;
use std::env;
use std::path::PathBuf;

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
                "memory_core": "not_on_release_path",
                "orchestrator": "onecontext-context-engine",
                "company_pack": "wiki-company-v1",
                "company_orchestrator": "wiki-company-orchestrator-v1",
                "model_transport": "codex_app_server_via_onecontext_codex_adapter"
            }
        })),
        "update-wiki" => update_wiki(args),
        "--help" | "-h" | "help" => Ok(json!({
            "usage": "onecontext-context-engine describe | update-wiki --root <1Context-root> [--run-id ID] [--trigger NAME] [--max-concurrent N] [--source-window-days N] [--mode recent-first|incremental|backfill|dry-run] [--json]"
        })),
        other => Err(format!("unknown command {other:?}")),
    }
}

fn update_wiki(args: Vec<String>) -> Result<serde_json::Value, String> {
    let mut root: Option<PathBuf> = None;
    let mut request = WikiCompanyRunRequest::new(default_run_id());
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                index += 1;
                root = Some(PathBuf::from(required_value(&args, index, "--root")?));
            }
            "--run-id" => {
                index += 1;
                request.run_id = required_value(&args, index, "--run-id")?;
            }
            "--trigger" => {
                index += 1;
                request.trigger = required_value(&args, index, "--trigger")?;
            }
            "--max-concurrent" => {
                index += 1;
                request.max_concurrent_agents = parse_positive_u32(
                    &required_value(&args, index, "--max-concurrent")?,
                    "--max-concurrent",
                )?;
            }
            "--source-window-days" => {
                index += 1;
                request.source_window_days = parse_positive_u32(
                    &required_value(&args, index, "--source-window-days")?,
                    "--source-window-days",
                )?;
            }
            "--mode" => {
                index += 1;
                request.mode = parse_mode(&required_value(&args, index, "--mode")?)?;
            }
            "--no-agents" => {
                request.execute_agents = false;
            }
            "--execute-agents" | "--json" => {}
            other => return Err(format!("unknown update-wiki flag {other:?}")),
        }
        index += 1;
    }
    let root = root.ok_or_else(|| "--root is required".to_string())?;
    let paths = ContextEnginePaths::new(root);
    paths
        .ensure_release_dirs()
        .map_err(|error| format!("failed to create context-engine directories: {error}"))?;
    let pack = load_wiki_company_pack(&paths)?;
    let pack_report = validate_wiki_company_pack(&paths, &pack);
    if !pack_report.is_valid() {
        return Err(format!(
            "wiki-company pack validation failed: {}",
            pack_report.issues.join("; ")
        ));
    }
    let orchestrator = load_wiki_company_orchestrator_config(&paths)?;
    let orchestrator_report = validate_wiki_company_orchestrator(&orchestrator);
    if !orchestrator_report.is_valid() {
        return Err(format!(
            "wiki-company orchestrator validation failed: {}",
            orchestrator_report.issues.join("; ")
        ));
    }
    let plan = build_wiki_company_plan(&paths, request);
    let dry_run = if matches!(plan.request.mode, WikiCompanyRunMode::DryRun) {
        Some(build_wiki_company_dry_run(&paths, &pack, &plan.request)?)
    } else {
        None
    };
    let agent_execution = if plan.request.execute_agents
        && !matches!(plan.request.mode, WikiCompanyRunMode::DryRun)
    {
        Some(execute_wiki_company_agents(
            &paths,
            &pack,
            &plan.run_id,
            plan.request.max_concurrent_agents,
        )?)
    } else {
        None
    };
    let mail_receipt_path = append_wiki_company_mail_receipt(&plan)
        .map_err(|error| format!("failed to append wiki company mail receipt: {error}"))?;
    let wiki_refresh_marker = if matches!(plan.request.mode, WikiCompanyRunMode::DryRun) {
        None
    } else {
        Some(
            onecontext_context_engine::wiki_marker::append_context_engine_refresh_marker(
                &paths,
                &plan.run_id,
                &plan.request.trigger,
                plan.request.execute_agents,
            ),
        )
    };
    Ok(json!({
        "schema_version": CONTEXT_ENGINE_SCHEMA_VERSION,
        "status": if agent_execution.is_some() { "executed" } else { "planned" },
        "surface": "context_engine.update_wiki",
        "run_id": plan.run_id,
        "mail_thread": plan.mail_thread,
        "mail_receipt_path": mail_receipt_path,
        "wiki_refresh_marker": wiki_refresh_marker,
        "company_pack": plan.company_pack,
        "company_orchestrator": plan.company_orchestrator,
        "config_validation": {
            "pack": {
                "status": "ok",
                "agents": pack_report.agent_count,
                "jobs": pack_report.job_count,
                "harnesses": pack_report.harness_count,
                "prompt_references": pack_report.prompt_reference_count
            },
            "orchestrator": {
                "status": "ok",
                "phases": orchestrator_report.phase_count,
                "routes": orchestrator_report.route_count
            }
        },
        "phase_count": plan.phases.len(),
        "dry_run": dry_run,
        "agent_execution": agent_execution,
        "release_boundary": plan.release_boundary,
    }))
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

fn parse_mode(value: &str) -> Result<WikiCompanyRunMode, String> {
    match value {
        "recent-first" | "recent_first" | "recent-first-then-backfill" => {
            Ok(WikiCompanyRunMode::RecentFirstThenBackfill)
        }
        "incremental" => Ok(WikiCompanyRunMode::Incremental),
        "backfill" => Ok(WikiCompanyRunMode::Backfill),
        "dry-run" | "dry_run" => Ok(WikiCompanyRunMode::DryRun),
        other => Err(format!("unknown mode {other:?}")),
    }
}

fn default_run_id() -> String {
    format!(
        "wiki-company-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    )
}
