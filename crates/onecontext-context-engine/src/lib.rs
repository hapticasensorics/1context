use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

pub mod agent_execution;
pub mod artifacts;
pub mod harness_executor;
pub mod mail_context;
pub mod memoryd_client;
pub mod orchestrator;
pub mod pack;
pub mod packet_planner;
pub mod talk_report;
pub mod wiki_company;

pub use orchestrator::{
    wiki_company_orchestrator, WikiCompanyOrchestrator, WIKI_COMPANY_ORCHESTRATOR_ID,
};

pub const CONTEXT_ENGINE_SCHEMA_VERSION: u32 = 1;
pub const WIKI_COMPANY_RUNNER: &str = "context_engine.wiki_company";
pub const WIKI_COMPANY_PACK_ID: &str = "wiki-company-v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextEnginePaths {
    pub root: PathBuf,
    pub context_engine: PathBuf,
    pub user_wiki: PathBuf,
    pub packs: PathBuf,
    pub wiki_company_pack: PathBuf,
    pub orchestrators: PathBuf,
    pub wiki_company_orchestrator: PathBuf,
    pub orchestrator_file: PathBuf,
    pub orchestrator_phases_file: PathBuf,
    pub packet_policy_file: PathBuf,
    pub routing_file: PathBuf,
    pub receipts_file: PathBuf,
    pub pack_prompts: PathBuf,
    pub pack_jobs: PathBuf,
    pub pack_agents: PathBuf,
    pub pack_harnesses: PathBuf,
    pub pack_lived_experiences: PathBuf,
    pub linking_file: PathBuf,
    pub native_memory_file: PathBuf,
    pub providers_file: PathBuf,
    pub plugin_file: PathBuf,
    pub agents: PathBuf,
    pub agent_directory: PathBuf,
    pub agent_harness: PathBuf,
    pub agent_policies: PathBuf,
    pub mail: PathBuf,
    pub mail_mailboxes: PathBuf,
    pub mail_threads: PathBuf,
    pub wiki_company_mail_thread: PathBuf,
}

impl ContextEnginePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let context_engine = root.join("context-engine");
        let user_wiki = root.join("user-wiki");
        let packs = context_engine.join("packs");
        let wiki_company_pack = packs.join(WIKI_COMPANY_PACK_ID);
        let orchestrators = context_engine.join("orchestrators");
        let wiki_company_orchestrator = orchestrators.join(WIKI_COMPANY_ORCHESTRATOR_ID);
        let agents = context_engine.join("agents");
        let mail = context_engine.join("mail");
        let mail_threads = mail.join("threads");
        let wiki_company_mail_thread = mail_threads.join("wiki-company.jsonl");
        Self {
            orchestrator_file: wiki_company_orchestrator.join("orchestrator.toml"),
            orchestrator_phases_file: wiki_company_orchestrator.join("phases.toml"),
            packet_policy_file: wiki_company_orchestrator.join("packet-policy.toml"),
            routing_file: wiki_company_orchestrator.join("routing.toml"),
            receipts_file: wiki_company_orchestrator.join("receipts.toml"),
            pack_prompts: wiki_company_pack.join("prompts"),
            pack_jobs: wiki_company_pack.join("jobs"),
            pack_agents: wiki_company_pack.join("agents"),
            pack_harnesses: wiki_company_pack.join("harnesses"),
            pack_lived_experiences: wiki_company_pack.join("lived-experiences"),
            linking_file: wiki_company_pack.join("linking.toml"),
            native_memory_file: wiki_company_pack.join("native-memory.toml"),
            providers_file: wiki_company_pack.join("providers.toml"),
            plugin_file: wiki_company_pack.join("plugin.toml"),
            agent_directory: agents.join("directory"),
            agent_harness: agents.join("harness"),
            agent_policies: agents.join("policies"),
            mail_mailboxes: mail.join("mailboxes"),
            mail,
            mail_threads,
            wiki_company_mail_thread,
            agents,
            orchestrators,
            wiki_company_orchestrator,
            packs,
            wiki_company_pack,
            context_engine,
            user_wiki,
            root,
        }
    }

    pub fn ensure_release_dirs(&self) -> std::io::Result<()> {
        for path in [
            &self.context_engine,
            &self.user_wiki,
            &self.packs,
            &self.wiki_company_pack,
            &self.orchestrators,
            &self.wiki_company_orchestrator,
            &self.pack_prompts,
            &self.pack_jobs,
            &self.pack_agents,
            &self.pack_harnesses,
            &self.pack_lived_experiences,
            &self.agents,
            &self.agent_directory,
            &self.agent_harness,
            &self.agent_policies,
            &self.mail,
            &self.mail_mailboxes,
            &self.mail_threads,
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiCompanyRunRequest {
    pub run_id: String,
    pub trigger: String,
    pub execute_agents: bool,
    pub max_concurrent_agents: u32,
    pub source_window_days: u32,
    pub mode: WikiCompanyRunMode,
}

impl WikiCompanyRunRequest {
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            trigger: "manual".to_string(),
            execute_agents: true,
            max_concurrent_agents: 5,
            source_window_days: 3,
            mode: WikiCompanyRunMode::RecentFirstThenBackfill,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WikiCompanyRunMode {
    RecentFirstThenBackfill,
    Incremental,
    Backfill,
    DryRun,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiCompanyRunPlan {
    pub schema_version: u32,
    pub runner: String,
    pub status: String,
    pub created_at: String,
    pub root: String,
    pub context_engine: String,
    pub user_wiki: String,
    pub run_id: String,
    pub mail_thread: String,
    pub request: WikiCompanyRunRequest,
    pub company_pack: WikiCompanyPack,
    pub company_orchestrator: WikiCompanyOrchestrator,
    pub phases: Vec<WikiCompanyPhase>,
    pub release_boundary: ReleaseBoundary,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiCompanyPack {
    pub id: String,
    pub root: String,
    pub agents_dir: String,
    pub jobs_dir: String,
    pub prompts_dir: String,
    pub harnesses_dir: String,
    pub lived_experiences_dir: String,
    pub linking_file: String,
    pub native_memory_file: String,
    pub providers_file: String,
    pub plugin_file: String,
    pub agent_count: usize,
    pub job_count: usize,
    pub prompt_count: usize,
    pub harness_count: usize,
    pub lived_experience_count: usize,
    pub active_harness: String,
    pub prompt_policy: String,
    pub provenance: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiCompanyPhase {
    pub id: String,
    pub label: String,
    pub owner: String,
    pub durable_output: String,
    pub reads_raw_history: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseBoundary {
    pub orchestrator: String,
    pub memory_core_release_status: String,
    pub execution_history: String,
    pub model_transport: String,
    pub durable_mail_root: String,
    pub wiki_truth_root: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiCompanyMailReceipt {
    pub schema_version: u32,
    pub kind: String,
    pub created_at: String,
    pub thread_id: String,
    pub operation_id: String,
    pub from: String,
    pub to: String,
    pub subject: String,
    pub body: String,
    pub plan: WikiCompanyRunPlan,
}

pub fn build_wiki_company_plan(
    paths: &ContextEnginePaths,
    request: WikiCompanyRunRequest,
) -> WikiCompanyRunPlan {
    WikiCompanyRunPlan {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        runner: WIKI_COMPANY_RUNNER.to_string(),
        status: "planned".to_string(),
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        root: paths.root.display().to_string(),
        context_engine: paths.context_engine.display().to_string(),
        user_wiki: paths.user_wiki.display().to_string(),
        run_id: safe_run_id(&request.run_id),
        mail_thread: paths.wiki_company_mail_thread.display().to_string(),
        request,
        company_pack: wiki_company_pack(paths),
        company_orchestrator: wiki_company_orchestrator(paths),
        phases: default_wiki_company_phases(),
        release_boundary: ReleaseBoundary {
            orchestrator: "onecontext-context-engine".to_string(),
            memory_core_release_status: "not_on_release_path".to_string(),
            execution_history: "mail_first_until_postgres_timescale_run_history".to_string(),
            model_transport: "codex_app_server_via_onecontext_codex_adapter".to_string(),
            durable_mail_root: "context-engine/mail".to_string(),
            wiki_truth_root: "user-wiki/source".to_string(),
        },
    }
}

pub fn wiki_company_pack(paths: &ContextEnginePaths) -> WikiCompanyPack {
    WikiCompanyPack {
        id: WIKI_COMPANY_PACK_ID.to_string(),
        root: paths.wiki_company_pack.display().to_string(),
        agents_dir: paths.pack_agents.display().to_string(),
        jobs_dir: paths.pack_jobs.display().to_string(),
        prompts_dir: paths.pack_prompts.display().to_string(),
        harnesses_dir: paths.pack_harnesses.display().to_string(),
        lived_experiences_dir: paths.pack_lived_experiences.display().to_string(),
        linking_file: paths.linking_file.display().to_string(),
        native_memory_file: paths.native_memory_file.display().to_string(),
        providers_file: paths.providers_file.display().to_string(),
        plugin_file: paths.plugin_file.display().to_string(),
        agent_count: count_files_with_extension(&paths.pack_agents, "toml"),
        job_count: count_files_with_extension(&paths.pack_jobs, "toml"),
        prompt_count: count_files_with_extension(&paths.pack_prompts, "md"),
        harness_count: count_files_with_extension(&paths.pack_harnesses, "toml"),
        lived_experience_count: count_files_with_extension(&paths.pack_lived_experiences, "md"),
        active_harness: "onecontext-agent-harness + onecontext-codex-adapter".to_string(),
        prompt_policy: "preserve original donor prompts as much as possible".to_string(),
        provenance: "ported from memory-core/memory/plugins/base-memory-v1".to_string(),
    }
}

pub fn append_wiki_company_mail_receipt(plan: &WikiCompanyRunPlan) -> std::io::Result<PathBuf> {
    let path = PathBuf::from(&plan.mail_thread);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let receipt = WikiCompanyMailReceipt {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.wiki_company_mail_receipt".to_string(),
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        thread_id: "mail://wiki-company".to_string(),
        operation_id: plan.run_id.clone(),
        from: "agent://context-engine.runner".to_string(),
        to: "mailbox://wiki-company".to_string(),
        subject: format!("Wiki update planned: {}", plan.run_id),
        body: "Context Engine recorded this update in mail. File run history is intentionally disabled until Postgres/Timescale owns execution history.".to_string(),
        plan: plan.clone(),
    };
    let bytes = serde_json::to_vec(&receipt)?;
    let mut file = OpenOptions::new().create(true).append(true).open(&path)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    Ok(path)
}

fn count_files_with_extension(dir: &Path, expected: &str) -> usize {
    let Ok(entries) = fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_type()
                .map(|kind| kind.is_file())
                .unwrap_or(false)
        })
        .filter(|entry| {
            entry
                .path()
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == expected)
        })
        .count()
}

pub fn default_wiki_company_phases() -> Vec<WikiCompanyPhase> {
    vec![
        phase(
            "import_perception",
            "Import source events",
            "context-engine.runner",
            "context-engine/mail/threads/wiki-company.jsonl",
            false,
        ),
        phase(
            "plan_scribe_packets",
            "Plan bounded scribe packets",
            "context-engine.runner",
            "context-engine/mail/threads/wiki-company.jsonl",
            false,
        ),
        phase(
            "wake_scribes",
            "Wake scribe agents",
            "onecontext-agent-harness",
            "context-engine/mail/threads/wiki-company.jsonl",
            true,
        ),
        phase(
            "for_you_editor",
            "Edit proposals for For You",
            "onecontext-agent-harness",
            "context-engine/mail/threads/wiki-company.jsonl",
            false,
        ),
        phase(
            "specialists",
            "Wake biographer and librarian",
            "onecontext-agent-harness",
            "context-engine/mail/threads/wiki-company.jsonl",
            false,
        ),
        phase(
            "curators",
            "Curate accepted page changes",
            "onecontext-agent-harness",
            "context-engine/mail/threads/wiki-company.jsonl",
            false,
        ),
        phase(
            "publish",
            "Write pages and publish wiki",
            "onecontext-wiki-core",
            "user-wiki/site/.1context/page-fingerprints.json",
            false,
        ),
    ]
}

fn phase(
    id: &str,
    label: &str,
    owner: &str,
    durable_output: &str,
    reads_raw_history: bool,
) -> WikiCompanyPhase {
    WikiCompanyPhase {
        id: id.to_string(),
        label: label.to_string(),
        owner: owner.to_string(),
        durable_output: durable_output.to_string(),
        reads_raw_history,
    }
}

pub fn safe_run_id(value: &str) -> String {
    let mut out = String::new();
    for ch in value.trim().chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "wiki-company-run".to_string()
    } else {
        trimmed.chars().take(120).collect()
    }
}

pub fn read_wiki_company_mail_receipts(
    path: impl AsRef<Path>,
) -> std::io::Result<Vec<WikiCompanyMailReceipt>> {
    let text = fs::read_to_string(path)?;
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).map_err(std::io::Error::other))
        .collect()
}
