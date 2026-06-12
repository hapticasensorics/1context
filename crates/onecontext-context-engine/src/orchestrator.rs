use crate::ContextEnginePaths;
use onecontext_agent_mail::MailAddress;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub const WIKI_COMPANY_ORCHESTRATOR_ID: &str = "wiki-company-orchestrator-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiCompanyOrchestrator {
    pub id: String,
    pub root: String,
    pub orchestrator_file: String,
    pub phases_file: String,
    pub packet_policy_file: String,
    pub routing_file: String,
    pub receipts_file: String,
    pub pack_id: String,
    pub active_harness: String,
    pub config_file_count: usize,
}

pub fn wiki_company_orchestrator(paths: &ContextEnginePaths) -> WikiCompanyOrchestrator {
    let config_files = [
        &paths.orchestrator_file,
        &paths.orchestrator_phases_file,
        &paths.packet_policy_file,
        &paths.routing_file,
        &paths.receipts_file,
    ];
    WikiCompanyOrchestrator {
        id: WIKI_COMPANY_ORCHESTRATOR_ID.to_string(),
        root: paths.wiki_company_orchestrator.display().to_string(),
        orchestrator_file: paths.orchestrator_file.display().to_string(),
        phases_file: paths.orchestrator_phases_file.display().to_string(),
        packet_policy_file: paths.packet_policy_file.display().to_string(),
        routing_file: paths.routing_file.display().to_string(),
        receipts_file: paths.receipts_file.display().to_string(),
        pack_id: crate::WIKI_COMPANY_PACK_ID.to_string(),
        active_harness: "codex-app-server".to_string(),
        config_file_count: config_files.iter().filter(|path| path.is_file()).count(),
    }
}

#[derive(Clone, Debug)]
pub struct WikiCompanyOrchestratorConfig {
    pub orchestrator: OrchestratorConfigFile,
    pub phases: PhasesFile,
    pub packet_policy: PacketPolicyFile,
    pub routing: RoutingFile,
    pub receipts: ReceiptsFile,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OrchestratorConfigFile {
    pub id: String,
    pub pack_id: String,
    pub active_harness: String,
    pub execution_history: String,
    pub mail_thread: String,
    pub default_mode: String,
    #[serde(default)]
    pub modes: Vec<String>,
    pub concurrency: ConcurrencyConfig,
    pub model_context: ModelContextConfig,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ConcurrencyConfig {
    pub default_max_concurrent_agents: u32,
    pub max_concurrent_agents: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ModelContextConfig {
    pub default_source_context_fraction: f64,
    pub default_reasoning_effort: String,
    pub specialist_reasoning_effort: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PhasesFile {
    #[serde(default)]
    pub phases: Vec<PhaseConfig>,
    #[serde(default)]
    pub phase_jobs: Vec<PhaseJobConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PhaseConfig {
    pub id: String,
    #[serde(default)]
    pub label: Option<String>,
    pub owner: String,
    pub durable_receipt: String,
    pub reads_raw_history: bool,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub strategy: Option<String>,
    #[serde(default)]
    pub completion: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PhaseJobConfig {
    pub phase_id: String,
    pub job_id: String,
    #[serde(default)]
    pub fanout: Option<String>,
    #[serde(default)]
    pub when: Option<String>,
    #[serde(default)]
    pub max_concurrent: Option<u32>,
    #[serde(default)]
    pub required_artifacts: Vec<String>,
    #[serde(default)]
    pub required_mail: Vec<String>,
    #[serde(default)]
    pub bindings: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PacketPolicyFile {
    pub windows: PacketWindows,
    pub context_budget: ContextBudget,
    #[serde(default)]
    pub packet_shape: PacketShape,
    #[serde(default)]
    pub cache: PacketCache,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PacketWindows {
    pub recent_first_days: u32,
    pub backfill_days: u32,
    pub backfill_order: String,
    #[serde(default)]
    pub incremental_unit: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ContextBudget {
    pub model_context_tokens: u32,
    pub source_context_fraction: f64,
    pub target_source_tokens: u32,
    #[serde(default)]
    pub min_source_tokens: Option<u32>,
    pub split_when_estimated_tokens_exceed: u32,
    #[serde(default)]
    pub reserve_for_prompt_mail_and_receipts: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PacketShape {
    #[serde(default)]
    pub raw_history_roles: Vec<String>,
    #[serde(default)]
    pub downstream_roles_read_scribe_artifacts: bool,
    #[serde(default)]
    pub drop_tool_noise_by_default: bool,
    #[serde(default)]
    pub keep_failures_commands_paths_commits_tests_and_user_decisions: bool,
    #[serde(default)]
    pub batch_adjacent_quiet_hours: bool,
    #[serde(default)]
    pub split_busy_hours_by_session_or_subhour: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PacketCache {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub skip_unchanged_scribe_packets: bool,
    #[serde(default)]
    pub rerun_downstream_when_new_scribe_artifacts_arrive: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RoutingFile {
    pub default_thread: String,
    #[serde(default)]
    pub routes: Vec<RouteConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RouteConfig {
    pub from_role: String,
    #[serde(default)]
    pub to: Vec<String>,
    #[serde(default)]
    pub cc: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ReceiptsFile {
    pub required: RequiredReceipts,
    pub final_message: FinalMessageReceipt,
    pub talk_append: TalkAppendReceipt,
    pub completion: CompletionReceipts,
}

#[derive(Clone, Debug, Deserialize)]
pub struct RequiredReceipts {
    pub harness_birth_certificate: bool,
    pub harness_turn_start: bool,
    pub context_injection_receipt: bool,
    #[serde(default)]
    pub adapter_events: bool,
    pub final_message: bool,
    pub talk_append: bool,
    pub mail_delivery: bool,
    pub harness_turn_complete: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct FinalMessageReceipt {
    pub path: String,
    pub non_empty: bool,
    #[serde(default)]
    pub fields: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct TalkAppendReceipt {
    pub delivery_mode: String,
    pub thread_id: String,
    #[serde(default)]
    pub from: Option<String>,
    #[serde(default)]
    pub cc_page_mailbox: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct CompletionReceipts {
    pub do_not_count_codex_exit_as_done: bool,
    pub require_final_message_and_harness_receipt: bool,
    pub require_talk_or_mail_receipt: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRoute {
    pub from_role: String,
    pub thread_id: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OrchestratorValidationReport {
    pub orchestrator_id: String,
    pub phase_count: usize,
    pub route_count: usize,
    pub issues: Vec<String>,
}

impl OrchestratorValidationReport {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

pub fn load_wiki_company_orchestrator_config(
    paths: &ContextEnginePaths,
) -> Result<WikiCompanyOrchestratorConfig, String> {
    Ok(WikiCompanyOrchestratorConfig {
        orchestrator: read_toml(&paths.orchestrator_file)?,
        phases: read_toml(&paths.orchestrator_phases_file)?,
        packet_policy: read_toml(&paths.packet_policy_file)?,
        routing: read_toml(&paths.routing_file)?,
        receipts: read_toml(&paths.receipts_file)?,
    })
}

pub fn validate_wiki_company_orchestrator(
    config: &WikiCompanyOrchestratorConfig,
) -> OrchestratorValidationReport {
    let mut issues = Vec::new();
    if config.orchestrator.id != WIKI_COMPANY_ORCHESTRATOR_ID {
        issues.push(format!(
            "orchestrator id {:?} does not match expected {:?}",
            config.orchestrator.id, WIKI_COMPANY_ORCHESTRATOR_ID
        ));
    }
    if config.orchestrator.pack_id != crate::WIKI_COMPANY_PACK_ID {
        issues.push(format!(
            "orchestrator pack_id {:?} does not match expected {:?}",
            config.orchestrator.pack_id,
            crate::WIKI_COMPANY_PACK_ID
        ));
    }
    if config.orchestrator.active_harness != "codex-app-server" {
        issues.push(format!(
            "orchestrator active_harness must be codex-app-server, got {:?}",
            config.orchestrator.active_harness
        ));
    }
    if config.orchestrator.execution_history != "mail_first_until_postgres_timescale" {
        issues.push(format!(
            "orchestrator execution_history must be mail_first_until_postgres_timescale, got {:?}",
            config.orchestrator.execution_history
        ));
    }
    if config.orchestrator.mail_thread != "context-engine/live/mail/threads/wiki-company.jsonl" {
        issues.push(format!(
            "orchestrator mail_thread must point at wiki-company mail thread, got {:?}",
            config.orchestrator.mail_thread
        ));
    }
    if !config
        .orchestrator
        .modes
        .contains(&config.orchestrator.default_mode)
    {
        issues.push(format!(
            "default_mode {:?} is not listed in modes",
            config.orchestrator.default_mode
        ));
    }
    if config
        .orchestrator
        .concurrency
        .default_max_concurrent_agents
        == 0
    {
        issues.push("default_max_concurrent_agents must be greater than zero".to_string());
    }
    if config
        .orchestrator
        .concurrency
        .default_max_concurrent_agents
        > config.orchestrator.concurrency.max_concurrent_agents
    {
        issues.push(
            "default_max_concurrent_agents must not exceed max_concurrent_agents".to_string(),
        );
    }
    if !(0.0..=1.0).contains(
        &config
            .orchestrator
            .model_context
            .default_source_context_fraction,
    ) {
        issues.push("default_source_context_fraction must be between 0 and 1".to_string());
    }

    let phase_ids = config
        .phases
        .phases
        .iter()
        .map(|phase| phase.id.as_str())
        .collect::<BTreeSet<_>>();
    for required in [
        "import_perception",
        "plan_scribe_packets",
        "wake_scribes",
        "historian_questions",
        "hourly_answers",
        "for_you_editor",
        "specialists",
        "curators",
        "publish",
    ] {
        if !phase_ids.contains(required) {
            issues.push(format!("missing orchestrator phase {required}"));
        }
    }
    for phase in &config.phases.phases {
        if phase.durable_receipt != "context-engine/live/mail/threads/wiki-company.jsonl" {
            issues.push(format!(
                "phase {} durable_receipt must be wiki-company mail thread, got {:?}",
                phase.id, phase.durable_receipt
            ));
        }
        for depends_on in &phase.depends_on {
            if !phase_ids.contains(depends_on.as_str()) {
                issues.push(format!(
                    "phase {} depends on unknown phase {}",
                    phase.id, depends_on
                ));
            }
        }
    }
    for phase_job in &config.phases.phase_jobs {
        if !phase_ids.contains(phase_job.phase_id.as_str()) {
            issues.push(format!(
                "phase job {} references unknown phase {}",
                phase_job.job_id, phase_job.phase_id
            ));
        }
    }

    if config.packet_policy.windows.recent_first_days != 3 {
        issues.push(format!(
            "recent_first_days must be 3, got {}",
            config.packet_policy.windows.recent_first_days
        ));
    }
    if config.packet_policy.windows.backfill_days != 30 {
        issues.push(format!(
            "backfill_days must be 30, got {}",
            config.packet_policy.windows.backfill_days
        ));
    }
    if config.packet_policy.context_budget.source_context_fraction <= 0.0
        || config.packet_policy.context_budget.source_context_fraction > 1.0
    {
        issues.push("packet source_context_fraction must be between 0 and 1".to_string());
    }
    if config.packet_policy.context_budget.target_source_tokens
        > config.packet_policy.context_budget.model_context_tokens
    {
        issues.push("target_source_tokens must not exceed model_context_tokens".to_string());
    }
    if config
        .packet_policy
        .packet_shape
        .raw_history_roles
        .is_empty()
    {
        issues.push("packet_shape.raw_history_roles must name raw-history roles".to_string());
    }
    if config.packet_policy.cache.enabled
        && config.packet_policy.cache.key.as_deref() != Some("source_packet_hash")
    {
        issues
            .push("packet cache key must be source_packet_hash when cache is enabled".to_string());
    }
    if config.routing.default_thread != "mail://wiki-company" {
        issues.push(format!(
            "routing default_thread must be mail://wiki-company, got {:?}",
            config.routing.default_thread
        ));
    }
    if config.routing.routes.is_empty() {
        issues.push("routing must define at least one route".to_string());
    }
    for route in &config.routing.routes {
        if route.to.is_empty() {
            issues.push(format!("route from {} has no recipients", route.from_role));
        }
        if route.from_role.trim().is_empty() {
            issues.push("route has empty from_role".to_string());
        }
        for address in route.to.iter().chain(route.cc.iter()) {
            match MailAddress::parse(address) {
                Ok(MailAddress::Role { .. })
                | Ok(MailAddress::List { .. })
                | Ok(MailAddress::PageMailbox { .. }) => {}
                Ok(_) => {
                    issues.push(format!(
                        "route from {} has unsupported recipient address {:?}; expected role://, list://, or mailbox://page/",
                        route.from_role, address
                    ));
                }
                Err(error) => {
                    issues.push(format!(
                        "route from {} has invalid recipient address {:?}: {error:#}",
                        route.from_role, address
                    ));
                }
            }
        }
    }

    let required = &config.receipts.required;
    if !required.harness_birth_certificate
        || !required.harness_turn_start
        || !required.context_injection_receipt
        || !required.adapter_events
        || !required.final_message
        || !required.talk_append
        || !required.mail_delivery
        || !required.harness_turn_complete
    {
        issues.push("all required receipt gates must be enabled".to_string());
    }
    if config.receipts.final_message.path != "final-message.md" {
        issues.push(format!(
            "final_message path must be final-message.md, got {:?}",
            config.receipts.final_message.path
        ));
    }
    if !config.receipts.final_message.non_empty {
        issues.push("final_message.non_empty must be true".to_string());
    }
    if config.receipts.talk_append.delivery_mode != "mail" {
        issues.push(format!(
            "talk_append delivery_mode must be mail, got {:?}",
            config.receipts.talk_append.delivery_mode
        ));
    }
    if !config.receipts.completion.do_not_count_codex_exit_as_done
        || !config
            .receipts
            .completion
            .require_final_message_and_harness_receipt
        || !config.receipts.completion.require_talk_or_mail_receipt
    {
        issues.push("completion must require receipts rather than raw Codex exit".to_string());
    }

    OrchestratorValidationReport {
        orchestrator_id: config.orchestrator.id.clone(),
        phase_count: config.phases.phases.len(),
        route_count: config.routing.routes.len(),
        issues,
    }
}

pub fn route_for_role(routing: &RoutingFile, role: &str) -> ResolvedRoute {
    let route = routing.routes.iter().find(|route| route.from_role == role);
    ResolvedRoute {
        from_role: role.to_string(),
        thread_id: routing.default_thread.clone(),
        to: route
            .map(|route| route.to.clone())
            .unwrap_or_else(|| vec!["list://wiki-company".to_string()]),
        cc: route
            .map(|route| route.cc.clone())
            .unwrap_or_else(|| vec!["list://wiki-company".to_string()]),
    }
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("failed to parse {}: {error}", path.display()))
}
