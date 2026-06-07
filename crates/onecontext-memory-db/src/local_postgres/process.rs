use std::path::PathBuf;

use super::config::{MANAGED_POSTGRES_PORT, MANAGED_POSTGRES_SUPERUSER};
use super::manifest::ManagedPgManifest;
use super::paths::ManagedPgPaths;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPostgresCommandPlan {
    pub program: PathBuf,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPostgresCommandOutput {
    pub status: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl ManagedPostgresCommandOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0)
    }
}

impl ManagedPostgresCommandPlan {
    pub fn start(manifest: &ManagedPgManifest, paths: &ManagedPgPaths) -> Self {
        Self {
            program: manifest.pg_ctl_bin.clone(),
            args: vec![
                "-D".to_string(),
                paths.pgdata.display().to_string(),
                "-l".to_string(),
                paths
                    .logs_dir
                    .join("postgres-supervisor.log")
                    .display()
                    .to_string(),
                "-o".to_string(),
                format!(
                    "-c config_file={}",
                    shell_single_quote(&paths.pgdata.join("postgresql.conf").display().to_string())
                ),
                "-w".to_string(),
                "start".to_string(),
            ],
        }
    }

    pub fn status(manifest: &ManagedPgManifest, paths: &ManagedPgPaths) -> Self {
        Self {
            program: manifest.pg_ctl_bin.clone(),
            args: vec![
                "-D".to_string(),
                paths.pgdata.display().to_string(),
                "status".to_string(),
            ],
        }
    }

    pub fn readiness(manifest: &ManagedPgManifest, paths: &ManagedPgPaths) -> Self {
        Self {
            program: manifest.pg_isready_bin.clone(),
            args: vec![
                "-h".to_string(),
                paths.socket_dir.display().to_string(),
                "-p".to_string(),
                MANAGED_POSTGRES_PORT.to_string(),
                "-d".to_string(),
                "postgres".to_string(),
                "-U".to_string(),
                MANAGED_POSTGRES_SUPERUSER.to_string(),
            ],
        }
    }

    pub fn initdb(manifest: &ManagedPgManifest, paths: &ManagedPgPaths) -> Self {
        Self {
            program: manifest.initdb_bin.clone(),
            args: vec![
                "-D".to_string(),
                paths.pgdata.display().to_string(),
                "-U".to_string(),
                MANAGED_POSTGRES_SUPERUSER.to_string(),
                "--auth-local=trust".to_string(),
                "--auth-host=scram-sha-256".to_string(),
                "--encoding=UTF8".to_string(),
                "--locale=C".to_string(),
            ],
        }
    }

    pub fn psql_superuser(
        manifest: &ManagedPgManifest,
        paths: &ManagedPgPaths,
        database: &str,
        sql: &str,
    ) -> Self {
        Self {
            program: manifest.psql_bin.clone(),
            args: vec![
                "-X".to_string(),
                "-A".to_string(),
                "-t".to_string(),
                "-q".to_string(),
                "-v".to_string(),
                "ON_ERROR_STOP=1".to_string(),
                "-h".to_string(),
                paths.socket_dir.display().to_string(),
                "-p".to_string(),
                MANAGED_POSTGRES_PORT.to_string(),
                "-U".to_string(),
                MANAGED_POSTGRES_SUPERUSER.to_string(),
                "-d".to_string(),
                database.to_string(),
                "-c".to_string(),
                sql.to_string(),
            ],
        }
    }

    pub fn createdb(manifest: &ManagedPgManifest, paths: &ManagedPgPaths) -> Option<Self> {
        manifest.createdb_bin.as_ref().map(|program| Self {
            program: program.clone(),
            args: vec![
                "-h".to_string(),
                paths.socket_dir.display().to_string(),
                "-p".to_string(),
                MANAGED_POSTGRES_PORT.to_string(),
                "-U".to_string(),
                MANAGED_POSTGRES_SUPERUSER.to_string(),
                "-O".to_string(),
                super::config::MANAGED_POSTGRES_APP_USER.to_string(),
                super::config::MANAGED_POSTGRES_DATABASE.to_string(),
            ],
        })
    }

    pub fn stop(manifest: &ManagedPgManifest, paths: &ManagedPgPaths) -> Self {
        Self {
            program: manifest.pg_ctl_bin.clone(),
            args: vec![
                "-D".to_string(),
                paths.pgdata.display().to_string(),
                "stop".to_string(),
                "-m".to_string(),
                "fast".to_string(),
                "-t".to_string(),
                "15".to_string(),
            ],
        }
    }
}

fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
