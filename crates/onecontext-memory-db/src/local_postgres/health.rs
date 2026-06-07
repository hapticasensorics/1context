use std::process::Command;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::manifest::ManagedPgManifest;
use super::paths::{ManagedPgPathConfig, ManagedPgPaths};
use super::process::ManagedPostgresCommandPlan;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManagedPostgresReadOnlyState {
    ManagedPostgresNotImplemented,
    BundleMissing,
    BundleInvalid,
    ClusterUninitialized,
    Stopped,
    Ready,
}

impl ManagedPostgresReadOnlyState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManagedPostgresNotImplemented => "managed_postgres_not_implemented",
            Self::BundleMissing => "bundle_missing",
            Self::BundleInvalid => "bundle_invalid",
            Self::ClusterUninitialized => "cluster_uninitialized",
            Self::Stopped => "stopped",
            Self::Ready => "ready",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedPostgresExtensionRequirement {
    pub name: String,
    pub required: bool,
    pub preload_required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManagedPostgresReadOnlyHealth {
    pub state: ManagedPostgresReadOnlyState,
    pub ready: bool,
    pub safe_to_retry: bool,
    pub app_support_dir: PathBuf,
    pub pgdata_dir: PathBuf,
    pub socket_dir: PathBuf,
    pub bundle_prefix: PathBuf,
    pub postgres_major: Option<u16>,
    pub postgres_version: Option<String>,
    pub timescale_version: Option<String>,
    pub build_id: Option<String>,
    pub required_extensions: Vec<ManagedPostgresExtensionRequirement>,
    pub message: String,
    pub detail: Option<String>,
}

pub fn required_extension_requirements() -> Vec<ManagedPostgresExtensionRequirement> {
    [
        ("timescaledb", true),
        ("btree_gist", false),
        ("pgcrypto", false),
        ("pg_trgm", false),
        ("vector", false),
        ("pg_stat_statements", true),
    ]
    .into_iter()
    .map(
        |(name, preload_required)| ManagedPostgresExtensionRequirement {
            name: name.to_string(),
            required: true,
            preload_required,
        },
    )
    .collect()
}

pub fn probe_managed_postgres_read_only() -> ManagedPostgresReadOnlyHealth {
    probe_managed_postgres_read_only_with_config(&ManagedPgPathConfig::from_env())
}

pub fn probe_managed_postgres_read_only_with_config(
    config: &ManagedPgPathConfig,
) -> ManagedPostgresReadOnlyHealth {
    let paths = config.resolve_paths();
    let bundle_prefix = config.resolve_bundle_prefix();
    match ManagedPgManifest::load_and_validate(&bundle_prefix) {
        Ok(manifest) => {
            if !paths.pgdata.join("PG_VERSION").is_file() {
                return health(
                    &paths,
                    bundle_prefix,
                    ManagedPostgresReadOnlyState::ClusterUninitialized,
                    Some(&manifest),
                    "Managed Postgres bundle is valid, but the private cluster is not initialized.",
                    Some("Run ensureStorageReady to initialize the private cluster."),
                );
            }
            if postgres_is_ready(&manifest, &paths) {
                return health(
                    &paths,
                    bundle_prefix,
                    ManagedPostgresReadOnlyState::Ready,
                    Some(&manifest),
                    "Managed Postgres is accepting local connections.",
                    Some("Private Unix socket is healthy; TCP listeners are disabled."),
                );
            }
            health(
                &paths,
                bundle_prefix,
                ManagedPostgresReadOnlyState::Stopped,
                Some(&manifest),
                "Managed Postgres bundle and cluster are present, but the server is not accepting connections.",
                Some("Run ensureStorageReady to start the private cluster."),
            )
        }
        Err(super::error::ManagedPostgresError::BundleMissing { .. }) => health(
            &paths,
            bundle_prefix,
            ManagedPostgresReadOnlyState::BundleMissing,
            None,
            "Managed Postgres bundle is not staged.",
            Some("Set ONECONTEXT_MANAGED_PG_PREFIX or package Resources/managed-postgres/macos-arm64."),
        ),
        Err(error) => health(
            &paths,
            bundle_prefix,
            ManagedPostgresReadOnlyState::BundleInvalid,
            None,
            "Managed Postgres bundle is staged but invalid.",
            Some(&error.to_string()),
        ),
    }
}

fn postgres_is_ready(manifest: &ManagedPgManifest, paths: &ManagedPgPaths) -> bool {
    let plan = ManagedPostgresCommandPlan::readiness(manifest, paths);
    Command::new(&plan.program)
        .args(&plan.args)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn health(
    paths: &ManagedPgPaths,
    bundle_prefix: PathBuf,
    state: ManagedPostgresReadOnlyState,
    manifest: Option<&ManagedPgManifest>,
    message: impl Into<String>,
    detail: Option<&str>,
) -> ManagedPostgresReadOnlyHealth {
    ManagedPostgresReadOnlyHealth {
        state,
        ready: state == ManagedPostgresReadOnlyState::Ready,
        safe_to_retry: matches!(
            state,
            ManagedPostgresReadOnlyState::BundleMissing
                | ManagedPostgresReadOnlyState::BundleInvalid
                | ManagedPostgresReadOnlyState::ClusterUninitialized
                | ManagedPostgresReadOnlyState::Stopped
        ),
        app_support_dir: paths.app_support.clone(),
        pgdata_dir: paths.pgdata.clone(),
        socket_dir: paths.socket_dir.clone(),
        bundle_prefix,
        postgres_major: manifest.map(|manifest| manifest.postgres_major),
        postgres_version: manifest.map(|manifest| manifest.postgres_version.clone()),
        timescale_version: manifest.map(|manifest| manifest.timescale_version.clone()),
        build_id: manifest.map(|manifest| manifest.build_id.clone()),
        required_extensions: required_extension_requirements(),
        message: message.into(),
        detail: detail.map(ToOwned::to_owned),
    }
}
