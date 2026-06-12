use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

pub mod agent_execution;
pub mod artifacts;
pub mod conditions;
pub mod fanout;
pub mod harness_executor;
pub mod memoryd_client;
pub mod orchestration;
pub mod packet_planner;
pub mod receipt_bridge;
pub mod run_state;
pub mod runtime_executor;
pub mod scheduler;
pub mod scheduler_bindings;
pub mod source_packets;
pub mod wiki_marker;
pub mod wiki_publish;

pub const CONTEXT_ENGINE_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContextEnginePaths {
    pub root: PathBuf,
    pub context_engine: PathBuf,
    pub live: PathBuf,
    pub user_wiki: PathBuf,
    pub packs: PathBuf,
    pub orchestrators: PathBuf,
    pub agents: PathBuf,
    pub agent_directory: PathBuf,
    pub agent_directory_agents_file: PathBuf,
    pub agent_directory_leases_file: PathBuf,
    pub agent_harness: PathBuf,
    pub agent_policies: PathBuf,
    pub mail: PathBuf,
    pub mail_messages: PathBuf,
    pub mail_bodies: PathBuf,
    pub mail_mailboxes: PathBuf,
    pub mail_threads: PathBuf,
    pub mail_deliveries_file: PathBuf,
    pub mail_claims_file: PathBuf,
    pub mail_idempotency_file: PathBuf,
    pub mail_injection_receipts_file: PathBuf,
    pub mail_control_events_file: PathBuf,
    pub mail_dead_letter_file: PathBuf,
    pub mail_mutation_lock_file: PathBuf,
    pub wiki_company_mail_thread: PathBuf,
    pub notifications: PathBuf,
    pub notification_outbox_file: PathBuf,
    pub notification_attempts_file: PathBuf,
    pub notification_cursors: PathBuf,
    pub state: PathBuf,
    pub state_harness: PathBuf,
    pub state_codex_app_server: PathBuf,
    pub state_codex_app_server_threads: PathBuf,
    pub state_scheduler: PathBuf,
    pub state_scheduler_dependency_graph: PathBuf,
    pub state_runs: PathBuf,
    pub state_runs_wiki_company: PathBuf,
    pub state_current_run_file: PathBuf,
    pub state_run_events_file: PathBuf,
    pub state_cleanup_receipts_file: PathBuf,
    pub state_migration_receipts_file: PathBuf,
    pub runs: PathBuf,
    pub artifacts: PathBuf,
    pub artifacts_content: PathBuf,
    pub archive: PathBuf,
    pub archive_failed_runs: PathBuf,
    pub process_logs: PathBuf,
    pub codex_app_server_events_file: PathBuf,
    pub wiki_memory_cache: PathBuf,
    pub tmp: PathBuf,
}

impl ContextEnginePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        let context_engine = root.join("context-engine");
        let live = context_engine.join("live");
        let user_wiki = root.join("user-wiki");
        let packs = context_engine.join("packs");
        let orchestrators = context_engine.join("orchestrators");
        let agents = live.join("agents");
        let mail = live.join("mail");
        let mail_messages = mail.join("messages");
        let mail_bodies = mail.join("bodies");
        let mail_mailboxes = mail.join("mailboxes");
        let mail_threads = mail.join("threads");
        let wiki_company_mail_thread = mail_threads.join("wiki-company.jsonl");
        let notifications = live.join("notifications");
        let state = live.join("state");
        let state_harness = state.join("harness");
        let state_codex_app_server = state.join("codex-app-server");
        let state_codex_app_server_threads = state_codex_app_server.join("threads");
        let state_service = state.join("service");
        let state_runs = state_service.clone();
        let state_runs_wiki_company = state_service.clone();
        let state_scheduler = state.join("scheduler");
        let runs = live.join("runs");
        let artifacts = runs.clone();
        let archive = live.join("archive");
        let archive_failed_runs = archive.join("failed-runs");
        let process_logs = state.join("process-logs");
        Self {
            agent_directory: agents.join("directory"),
            agent_directory_agents_file: agents.join("directory").join("agents.jsonl"),
            agent_directory_leases_file: agents.join("directory").join("leases.jsonl"),
            agent_harness: state_harness.clone(),
            agent_policies: agents.join("policies"),
            mail_messages,
            mail_bodies,
            mail_mailboxes,
            mail_deliveries_file: mail.join("deliveries.jsonl"),
            mail_claims_file: mail.join("claims.jsonl"),
            mail_idempotency_file: mail.join("idempotency.jsonl"),
            mail_injection_receipts_file: mail.join("injection-receipts.jsonl"),
            mail_control_events_file: mail.join("control-events.jsonl"),
            mail_dead_letter_file: mail.join("dead-letter.jsonl"),
            mail_mutation_lock_file: mail.join(".mutation.lock"),
            mail,
            mail_threads,
            wiki_company_mail_thread,
            notification_outbox_file: notifications.join("outbox.jsonl"),
            notification_attempts_file: notifications.join("attempts.jsonl"),
            notification_cursors: notifications.join("cursors"),
            notifications,
            state_scheduler_dependency_graph: state_scheduler.join("dependency-graph.json"),
            state_scheduler,
            state_current_run_file: state_runs_wiki_company.join("current-run.json"),
            state_run_events_file: state_runs_wiki_company.join("run-events.jsonl"),
            state_cleanup_receipts_file: state.join("cleanup-receipts.jsonl"),
            state_migration_receipts_file: state.join("migration-receipts.jsonl"),
            state_runs_wiki_company,
            state_runs,
            state_codex_app_server_threads,
            codex_app_server_events_file: state_codex_app_server.join("events.jsonl"),
            state_codex_app_server,
            state_harness,
            state: state.clone(),
            wiki_memory_cache: state_service.join("wiki-memory-cache"),
            tmp: live.join("tmp"),
            artifacts_content: runs.join("_content").join("sha256"),
            runs,
            artifacts,
            archive,
            archive_failed_runs,
            process_logs,
            agents,
            orchestrators,
            packs,
            context_engine,
            live,
            user_wiki,
            root,
        }
    }

    pub fn ensure_release_dirs(&self) -> std::io::Result<()> {
        for path in [
            &self.context_engine,
            &self.live,
            &self.user_wiki,
            &self.packs,
            &self.orchestrators,
            &self.agents,
            &self.agent_directory,
            &self.agent_harness,
            &self.agent_policies,
            &self.mail,
            &self.mail_messages,
            &self.mail_bodies,
            &self.mail_mailboxes,
            &self.mail_threads,
            &self.notifications,
            &self.notification_cursors,
            &self.state,
            &self.state_harness,
            &self.state_codex_app_server,
            &self.state_codex_app_server_threads,
            &self.state_scheduler,
            &self.state_runs_wiki_company,
            &self.state_runs,
            &self.runs,
            &self.artifacts,
            &self.archive,
            &self.archive_failed_runs,
            &self.process_logs,
            &self.wiki_memory_cache,
            &self.tmp,
        ] {
            fs::create_dir_all(path)?;
        }
        Ok(())
    }

    pub fn agent_tmp_dir(&self, run_id: &str, agent_id: &str) -> PathBuf {
        self.tmp
            .join(safe_run_id(run_id))
            .join("agents")
            .join(safe_run_id(agent_id))
    }

    pub fn run_root(&self, run_id: &str) -> PathBuf {
        self.runs.join(safe_run_id(run_id))
    }

    pub fn run_json_path(&self, run_id: &str) -> PathBuf {
        self.run_root(run_id).join("run.json")
    }

    pub fn run_source_packets(&self, run_id: &str) -> PathBuf {
        self.run_root(run_id).join("source-packets")
    }

    pub fn run_turns(&self, run_id: &str) -> PathBuf {
        self.run_root(run_id).join("turns")
    }

    pub fn run_artifacts(&self, run_id: &str) -> PathBuf {
        self.run_root(run_id).join("artifacts")
    }

    pub fn run_publish(&self, run_id: &str) -> PathBuf {
        self.run_root(run_id).join("publish")
    }

    pub fn run_state_dir(&self, run_id: &str) -> PathBuf {
        self.run_root(run_id).join("state")
    }

    pub fn run_mail_index_path(&self, run_id: &str) -> PathBuf {
        self.run_root(run_id).join("mail-index.jsonl")
    }

    pub fn run_receipt_hydration_path(&self, run_id: &str) -> PathBuf {
        self.run_root(run_id).join("receipt-hydration.json")
    }

    pub fn run_artifacts_content(&self, run_id: &str) -> PathBuf {
        self.run_artifacts(run_id).join("_content").join("sha256")
    }

    pub fn final_message_receipt_relative_path(
        &self,
        run_id: &str,
        operation_id: &str,
        filename: &str,
    ) -> String {
        let filename = Path::new(filename)
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.is_empty())
            .unwrap_or("final-message.md");
        format!(
            "context-engine/live/runs/{}/turns/{}/attempt-0001/{}",
            safe_run_id(run_id),
            safe_run_id(operation_id),
            filename
        )
    }

    pub fn validate_storage_layout(&self) -> StorageLayoutReport {
        let forbidden = [
            self.context_engine.join("agents"),
            self.context_engine.join("archive"),
            self.context_engine.join("decisions"),
            self.context_engine.join("inbox"),
            self.context_engine.join("jobs"),
            self.context_engine.join("ledgers"),
            self.context_engine.join("mail"),
            self.context_engine.join("notifications"),
            self.context_engine.join("observations"),
            self.context_engine.join("runs"),
            self.context_engine.join("state"),
            self.context_engine.join("tmp"),
            self.context_engine.join("wiki-memory-cache"),
            self.context_engine.join("source-packets"),
            self.context_engine.join("artifacts"),
            self.context_engine.join("prompts"),
            self.context_engine.join("proposals"),
        ];
        let forbidden_paths = forbidden
            .into_iter()
            .filter(|path| path.exists())
            .map(|path| path.display().to_string())
            .collect::<Vec<_>>();
        StorageLayoutReport {
            status: if forbidden_paths.is_empty() {
                "ok".to_string()
            } else {
                "blocked".to_string()
            },
            forbidden_paths,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageLayoutReport {
    pub status: String,
    pub forbidden_paths: Vec<String>,
}

impl StorageLayoutReport {
    pub fn is_valid(&self) -> bool {
        self.forbidden_paths.is_empty()
    }
}

pub fn safe_run_id(value: &str) -> String {
    let raw = value.trim();
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' || ch == '.' {
            out.push(ch);
        } else {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    let base = if trimmed.is_empty() {
        "wiki-company-run".to_string()
    } else {
        trimmed
    };
    let max_len = 120usize;
    let needs_hash = base != raw || base.chars().count() > max_len;
    if !needs_hash {
        return base;
    }
    let suffix = storage_id_hash(raw);
    let prefix_len = max_len.saturating_sub(suffix.len() + 1).max(1);
    let prefix = base.chars().take(prefix_len).collect::<String>();
    format!("{prefix}-{suffix}")
}

fn storage_id_hash(value: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(value.as_bytes());
    let digest = hash.finalize();
    digest
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_engine_paths_follow_storage_manifest() {
        let paths = ContextEnginePaths::new("/tmp/1Context");

        assert_eq!(paths.agent_harness, paths.state_harness);
        assert!(paths
            .agent_harness
            .ends_with("context-engine/live/state/harness"));
        assert!(paths
            .state_codex_app_server_threads
            .ends_with("context-engine/live/state/codex-app-server/threads"));
        assert!(paths
            .codex_app_server_events_file
            .ends_with("context-engine/live/state/codex-app-server/events.jsonl"));
        assert!(paths
            .state_scheduler_dependency_graph
            .ends_with("context-engine/live/state/scheduler/dependency-graph.json"));
        assert!(paths
            .state_current_run_file
            .ends_with("context-engine/live/state/service/current-run.json"));
        assert!(paths
            .state_run_events_file
            .ends_with("context-engine/live/state/service/run-events.jsonl"));
        assert!(paths
            .state_cleanup_receipts_file
            .ends_with("context-engine/live/state/cleanup-receipts.jsonl"));
        assert!(paths
            .state_migration_receipts_file
            .ends_with("context-engine/live/state/migration-receipts.jsonl"));
        assert!(paths
            .agent_directory_agents_file
            .ends_with("context-engine/live/agents/directory/agents.jsonl"));
        assert!(paths
            .agent_directory_leases_file
            .ends_with("context-engine/live/agents/directory/leases.jsonl"));
        assert!(paths
            .mail_messages
            .ends_with("context-engine/live/mail/messages"));
        assert!(paths
            .mail_bodies
            .ends_with("context-engine/live/mail/bodies"));
        assert!(paths
            .mail_deliveries_file
            .ends_with("context-engine/live/mail/deliveries.jsonl"));
        assert!(paths
            .mail_claims_file
            .ends_with("context-engine/live/mail/claims.jsonl"));
        assert!(paths
            .mail_idempotency_file
            .ends_with("context-engine/live/mail/idempotency.jsonl"));
        assert!(paths
            .mail_injection_receipts_file
            .ends_with("context-engine/live/mail/injection-receipts.jsonl"));
        assert!(paths
            .mail_control_events_file
            .ends_with("context-engine/live/mail/control-events.jsonl"));
        assert!(paths
            .mail_dead_letter_file
            .ends_with("context-engine/live/mail/dead-letter.jsonl"));
        assert!(paths
            .mail_mutation_lock_file
            .ends_with("context-engine/live/mail/.mutation.lock"));
        assert!(paths
            .notification_outbox_file
            .ends_with("context-engine/live/notifications/outbox.jsonl"));
        assert!(paths
            .notification_attempts_file
            .ends_with("context-engine/live/notifications/attempts.jsonl"));
        assert!(paths
            .notification_cursors
            .ends_with("context-engine/live/notifications/cursors"));
        assert!(paths
            .archive_failed_runs
            .ends_with("context-engine/live/archive/failed-runs"));
        assert!(paths.runs.ends_with("context-engine/live/runs"));
        assert!(paths.artifacts.ends_with("context-engine/live/runs"));
        let run_x = safe_run_id("run/x");
        let agent_y = safe_run_id("agent:y");
        let op_y_z = safe_run_id("op:y/z");
        assert!(paths
            .run_root("run/x")
            .ends_with(format!("context-engine/live/runs/{run_x}")));
        assert!(paths
            .run_json_path("run/x")
            .ends_with(format!("context-engine/live/runs/{run_x}/run.json")));
        assert!(paths
            .run_source_packets("run/x")
            .ends_with(format!("context-engine/live/runs/{run_x}/source-packets")));
        assert!(paths
            .run_turns("run/x")
            .ends_with(format!("context-engine/live/runs/{run_x}/turns")));
        assert!(paths
            .run_artifacts("run/x")
            .ends_with(format!("context-engine/live/runs/{run_x}/artifacts")));
        assert!(paths
            .run_publish("run/x")
            .ends_with(format!("context-engine/live/runs/{run_x}/publish")));
        assert!(paths
            .run_state_dir("run/x")
            .ends_with(format!("context-engine/live/runs/{run_x}/state")));
        assert!(paths
            .run_mail_index_path("run/x")
            .ends_with(format!("context-engine/live/runs/{run_x}/mail-index.jsonl")));
        assert!(paths.run_receipt_hydration_path("run/x").ends_with(format!(
            "context-engine/live/runs/{run_x}/receipt-hydration.json"
        )));
        assert!(paths.run_artifacts_content("run/x").ends_with(format!(
            "context-engine/live/runs/{run_x}/artifacts/_content/sha256"
        )));
        assert!(paths
            .process_logs
            .ends_with("context-engine/live/state/process-logs"));
        assert!(paths
            .agent_tmp_dir("run/x", "agent:y")
            .ends_with(format!("context-engine/live/tmp/{run_x}/agents/{agent_y}")));
        assert!(paths.tmp.ends_with("context-engine/live/tmp"));

        let final_message =
            paths.final_message_receipt_relative_path("run/x", "op:y/z", "../final-message.md");
        assert_eq!(
            final_message,
            format!(
                "context-engine/live/runs/{run_x}/turns/{op_y_z}/attempt-0001/final-message.md"
            )
        );
    }

    #[test]
    fn storage_layout_rejects_retired_runtime_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
        paths.ensure_release_dirs().expect("release dirs");
        assert!(paths.validate_storage_layout().is_valid());

        fs::create_dir_all(paths.context_engine.join("source-packets"))
            .expect("old source packets");
        fs::create_dir_all(paths.context_engine.join("agents/harness")).expect("old harness");
        fs::create_dir_all(paths.context_engine.join("artifacts")).expect("old artifacts");
        let report = paths.validate_storage_layout();
        assert_eq!(report.status, "blocked");
        assert_eq!(report.forbidden_paths.len(), 3);
        assert!(report
            .forbidden_paths
            .iter()
            .any(|path| path.ends_with("context-engine/source-packets")));
        assert!(report
            .forbidden_paths
            .iter()
            .any(|path| path.ends_with("context-engine/agents")));
        assert!(report
            .forbidden_paths
            .iter()
            .any(|path| path.ends_with("context-engine/artifacts")));
    }

    #[test]
    fn safe_run_id_is_collision_resistant_for_sanitized_paths() {
        assert_eq!(
            safe_run_id("official-second-run-20260608-1955"),
            "official-second-run-20260608-1955"
        );

        let colon = safe_run_id("run:a");
        let slash = safe_run_id("run/a");
        assert_ne!(colon, slash);
        assert!(colon.starts_with("run-a-"));
        assert!(slash.starts_with("run-a-"));

        let long_a = safe_run_id(&format!("{}x", "a".repeat(140)));
        let long_b = safe_run_id(&format!("{}y", "a".repeat(140)));
        assert_ne!(long_a, long_b);
        assert!(long_a.len() <= 120);
        assert!(long_b.len() <= 120);
    }
}
