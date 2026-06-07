use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::error::ManagedPostgresError;

pub const APP_SUPPORT_DIR_ENV: &str = "ONECONTEXT_APP_SUPPORT_DIR";
pub const MANAGED_PG_PREFIX_ENV: &str = "ONECONTEXT_MANAGED_PG_PREFIX";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ManagedPgPathConfig {
    pub app_support_dir: Option<PathBuf>,
    pub managed_pg_prefix: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPgPaths {
    pub app_support: PathBuf,
    pub postgres_root: PathBuf,
    pub pgdata: PathBuf,
    pub socket_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub auth_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub bootstrap_dir: PathBuf,
    pub repair_quarantine_dir: PathBuf,
    pub bootstrap_lock: PathBuf,
    pub initialized_marker: PathBuf,
    pub config_fingerprint: PathBuf,
}

impl ManagedPgPathConfig {
    pub fn from_env() -> Self {
        Self {
            app_support_dir: env_path(APP_SUPPORT_DIR_ENV),
            managed_pg_prefix: env_path(MANAGED_PG_PREFIX_ENV),
        }
    }

    pub fn resolve_paths(&self) -> ManagedPgPaths {
        ManagedPgPaths::from_app_support(
            self.app_support_dir
                .clone()
                .unwrap_or_else(default_app_support_dir),
        )
    }

    pub fn resolve_bundle_prefix(&self) -> PathBuf {
        self.managed_pg_prefix
            .clone()
            .unwrap_or_else(default_managed_pg_prefix)
    }
}

impl ManagedPgPaths {
    pub fn resolve() -> Self {
        ManagedPgPathConfig::from_env().resolve_paths()
    }

    pub fn from_app_support(app_support: impl Into<PathBuf>) -> Self {
        let app_support = app_support.into();
        let postgres_root = app_support.join("Postgres");
        let socket_dir = short_socket_dir(&app_support);
        Self {
            pgdata: postgres_root.join("pgdata"),
            socket_dir,
            logs_dir: postgres_root.join("logs"),
            auth_dir: postgres_root.join("auth"),
            backups_dir: postgres_root.join("backups"),
            bootstrap_dir: postgres_root.join("bootstrap"),
            repair_quarantine_dir: postgres_root.join("repair-quarantine"),
            bootstrap_lock: postgres_root.join("bootstrap/bootstrap.lock"),
            initialized_marker: postgres_root.join("bootstrap/initialized.ok"),
            config_fingerprint: postgres_root.join("bootstrap/config.sha256"),
            postgres_root,
            app_support,
        }
    }

    pub fn create_all_secure(&self) -> Result<(), ManagedPostgresError> {
        for dir in self.secure_directories() {
            create_dir_private(dir)?;
        }
        Ok(())
    }

    pub fn secure_directories(&self) -> [&Path; 9] {
        [
            &self.app_support,
            &self.postgres_root,
            &self.pgdata,
            &self.socket_dir,
            &self.logs_dir,
            &self.auth_dir,
            &self.backups_dir,
            &self.bootstrap_dir,
            &self.repair_quarantine_dir,
        ]
    }
}

pub fn default_app_support_dir() -> PathBuf {
    home_dir().join("Library/Application Support/1Context")
}

pub fn default_managed_pg_prefix() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.parent()
                .and_then(|macos| macos.parent())
                .map(|contents| contents.join("Resources/managed-postgres/macos-arm64"))
        })
        .unwrap_or_else(|| PathBuf::from("release/managed-postgres/runtime/macos-arm64"))
}

fn env_path(name: &str) -> Option<PathBuf> {
    std::env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn short_socket_dir(app_support: &Path) -> PathBuf {
    let mut hasher = Sha256::new();
    hasher.update(app_support.to_string_lossy().as_bytes());
    let digest = hasher.finalize();
    let suffix = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    std::env::temp_dir().join(format!("onecontext-pg-{suffix}"))
}

fn create_dir_private(path: &Path) -> Result<(), ManagedPostgresError> {
    std::fs::create_dir_all(path).map_err(|source| ManagedPostgresError::io(path, source))?;
    set_private_dir_permissions(path)
}

#[cfg(unix)]
fn set_private_dir_permissions(path: &Path) -> Result<(), ManagedPostgresError> {
    use std::os::unix::fs::PermissionsExt;

    let permissions = std::fs::Permissions::from_mode(0o700);
    std::fs::set_permissions(path, permissions)
        .map_err(|source| ManagedPostgresError::io(path, source))
}

#[cfg(not(unix))]
fn set_private_dir_permissions(_path: &Path) -> Result<(), ManagedPostgresError> {
    Ok(())
}
