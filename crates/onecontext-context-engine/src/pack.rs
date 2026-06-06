//! Pack loading and validation.
//!
//! Port target for the useful parts of `memory-core/src/onectx/agent/launch_plan.py`:
//! load pack metadata, resolve agent/job/prompt references, and build validated
//! harness request inputs.

use crate::{ContextEnginePaths, WIKI_COMPANY_PACK_ID};
use serde::Deserialize;
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct WikiCompanyPackConfig {
    pub plugin: PluginConfig,
    pub providers: Vec<ProviderConfig>,
    pub harnesses: Vec<HarnessConfig>,
    pub agents: Vec<AgentConfig>,
    pub jobs: Vec<JobConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct PluginConfig {
    pub id: String,
    pub version: String,
    pub schema_version: String,
    pub title: String,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProvidersFile {
    #[serde(default)]
    pub providers: Vec<ProviderConfig>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProviderConfig {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub model_patterns: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct HarnessConfig {
    pub id: String,
    pub runner: String,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub default_tools: Vec<String>,
    #[serde(default)]
    pub receives: Vec<String>,
    #[serde(default)]
    pub captures: Vec<String>,
    #[serde(default)]
    pub prompt_control: HarnessPromptControl,
    #[serde(default)]
    pub runtime_policy: HarnessRuntimePolicy,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HarnessPromptControl {
    #[serde(default)]
    pub model_instructions_file: Option<String>,
    #[serde(default)]
    pub mail_context_appendix: bool,
    #[serde(default)]
    pub final_message_required: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct HarnessRuntimePolicy {
    #[serde(default)]
    pub default_native_multi_agent_v2: bool,
    #[serde(default)]
    pub agent_flag: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct AgentConfig {
    pub id: String,
    pub harness: String,
    pub provider: String,
    pub model: String,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub prompt_paths: Vec<String>,
    #[serde(default)]
    pub reference_paths: Vec<String>,
    #[serde(default)]
    pub model_policy: ModelPolicy,
    #[serde(default)]
    pub memory: AgentMemoryPolicy,
    #[serde(default)]
    pub runtime: AgentRuntimePolicy,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ModelPolicy {
    #[serde(default)]
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AgentMemoryPolicy {
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub attach: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct AgentRuntimePolicy {
    #[serde(default)]
    pub native_multi_agent_v2: Option<bool>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct JobConfig {
    pub id: String,
    pub agent: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub tools: Vec<String>,
    #[serde(default)]
    pub prompt_paths: Vec<String>,
    #[serde(default)]
    pub inputs: Vec<String>,
    #[serde(default)]
    pub outputs: Vec<String>,
    #[serde(default)]
    pub permissions: JobPermissions,
    #[serde(default)]
    pub completion: JobCompletion,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct JobPermissions {
    #[serde(default)]
    pub read: Vec<String>,
    #[serde(default)]
    pub write: Vec<String>,
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct JobCompletion {
    #[serde(default)]
    pub done: Vec<String>,
    #[serde(default)]
    pub skip: Vec<String>,
    #[serde(default)]
    pub needs_approval: Vec<String>,
    #[serde(default)]
    pub failure: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackValidationReport {
    pub pack_id: String,
    pub provider_count: usize,
    pub harness_count: usize,
    pub agent_count: usize,
    pub job_count: usize,
    pub prompt_reference_count: usize,
    pub issues: Vec<String>,
}

impl PackValidationReport {
    pub fn is_valid(&self) -> bool {
        self.issues.is_empty()
    }
}

pub fn load_wiki_company_pack(paths: &ContextEnginePaths) -> Result<WikiCompanyPackConfig, String> {
    let plugin: PluginConfig = read_toml(&paths.plugin_file)?;
    let providers_file: ProvidersFile = read_toml(&paths.providers_file)?;
    let harnesses = read_toml_dir::<HarnessConfig>(&paths.pack_harnesses)?;
    let agents = read_toml_dir::<AgentConfig>(&paths.pack_agents)?;
    let jobs = read_toml_dir::<JobConfig>(&paths.pack_jobs)?;
    Ok(WikiCompanyPackConfig {
        plugin,
        providers: providers_file.providers,
        harnesses,
        agents,
        jobs,
    })
}

pub fn validate_wiki_company_pack(
    paths: &ContextEnginePaths,
    pack: &WikiCompanyPackConfig,
) -> PackValidationReport {
    let mut issues = Vec::new();
    if pack.plugin.id != WIKI_COMPANY_PACK_ID {
        issues.push(format!(
            "plugin id {:?} does not match expected {:?}",
            pack.plugin.id, WIKI_COMPANY_PACK_ID
        ));
    }

    let providers = id_set(pack.providers.iter().map(|provider| provider.id.as_str()));
    let harnesses = id_set(pack.harnesses.iter().map(|harness| harness.id.as_str()));
    let agents = id_set(pack.agents.iter().map(|agent| agent.id.as_str()));
    let jobs = id_set(pack.jobs.iter().map(|job| job.id.as_str()));

    find_duplicates(
        "provider",
        pack.providers.iter().map(|provider| provider.id.as_str()),
        &mut issues,
    );
    find_duplicates(
        "harness",
        pack.harnesses.iter().map(|harness| harness.id.as_str()),
        &mut issues,
    );
    find_duplicates(
        "agent",
        pack.agents.iter().map(|agent| agent.id.as_str()),
        &mut issues,
    );
    find_duplicates(
        "job",
        pack.jobs.iter().map(|job| job.id.as_str()),
        &mut issues,
    );

    if !harnesses.contains("codex-app-server") {
        issues.push("missing required harness codex-app-server".to_string());
    }

    let mut prompt_reference_count = 0;
    for harness in &pack.harnesses {
        if harness.id == "codex-app-server" {
            if harness.runner != "onecontext-codex-adapter" {
                issues.push(format!(
                    "codex-app-server runner must be onecontext-codex-adapter, got {:?}",
                    harness.runner
                ));
            }
            if harness.transport.as_deref() != Some("codex-app-server") {
                issues.push(format!(
                    "codex-app-server transport must be codex-app-server, got {:?}",
                    harness.transport
                ));
            }
        }
        if let Some(prompt) = &harness.prompt_control.model_instructions_file {
            prompt_reference_count += 1;
            validate_prompt_path(paths, "harness", &harness.id, prompt, &mut issues);
        }
    }

    for provider in &pack.providers {
        if provider.id == "codex" && provider.transport.as_deref() != Some("codex-app-server") {
            issues.push(format!(
                "provider codex must use codex-app-server transport, got {:?}",
                provider.transport
            ));
        }
    }

    for agent in &pack.agents {
        if !harnesses.contains(agent.harness.as_str()) {
            issues.push(format!(
                "agent {} references missing harness {}",
                agent.id, agent.harness
            ));
        }
        if !providers.contains(agent.provider.as_str()) {
            issues.push(format!(
                "agent {} references missing provider {}",
                agent.id, agent.provider
            ));
        }
        if agent.harness != "codex-app-server" {
            issues.push(format!(
                "agent {} must use codex-app-server harness, got {}",
                agent.id, agent.harness
            ));
        }
        for prompt in agent
            .prompt_paths
            .iter()
            .chain(agent.reference_paths.iter())
        {
            prompt_reference_count += 1;
            validate_prompt_path(paths, "agent", &agent.id, prompt, &mut issues);
        }
    }

    for job in &pack.jobs {
        if !agents.contains(job.agent.as_str()) {
            issues.push(format!(
                "job {} references missing agent {}",
                job.id, job.agent
            ));
        }
        for prompt in &job.prompt_paths {
            prompt_reference_count += 1;
            validate_prompt_path(paths, "job", &job.id, prompt, &mut issues);
        }
    }

    PackValidationReport {
        pack_id: pack.plugin.id.clone(),
        provider_count: providers.len(),
        harness_count: harnesses.len(),
        agent_count: agents.len(),
        job_count: jobs.len(),
        prompt_reference_count,
        issues,
    }
}

fn validate_prompt_path(
    paths: &ContextEnginePaths,
    owner_kind: &str,
    owner_id: &str,
    prompt: &str,
    issues: &mut Vec<String>,
) {
    let Some(relative) = prompt.strip_prefix("prompts/") else {
        issues.push(format!(
            "{} {} prompt path {:?} must be pack-relative under prompts/",
            owner_kind, owner_id, prompt
        ));
        return;
    };
    let resolved = paths.pack_prompts.join(relative);
    if !resolved.is_file() {
        issues.push(format!(
            "{} {} prompt path {:?} does not resolve to {}",
            owner_kind,
            owner_id,
            prompt,
            resolved.display()
        ));
    }
}

fn read_toml<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text = fs::read_to_string(path)
        .map_err(|error| format!("failed to read {}: {error}", path.display()))?;
    toml::from_str(&text).map_err(|error| format!("failed to parse {}: {error}", path.display()))
}

fn read_toml_dir<T: for<'de> Deserialize<'de>>(dir: &Path) -> Result<Vec<T>, String> {
    let mut entries = fs::read_dir(dir)
        .map_err(|error| format!("failed to read directory {}: {error}", dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("toml"))
        .collect::<Vec<PathBuf>>();
    entries.sort();
    entries.iter().map(|path| read_toml(path)).collect()
}

fn id_set<'a>(ids: impl Iterator<Item = &'a str>) -> BTreeSet<String> {
    ids.map(ToString::to_string).collect()
}

fn find_duplicates<'a>(kind: &str, ids: impl Iterator<Item = &'a str>, issues: &mut Vec<String>) {
    let mut seen = HashMap::<String, usize>::new();
    for id in ids {
        *seen.entry(id.to_string()).or_insert(0) += 1;
    }
    for (id, count) in seen {
        if count > 1 {
            issues.push(format!("duplicate {kind} id {id:?} appears {count} times"));
        }
    }
}
