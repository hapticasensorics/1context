use std::path::PathBuf;

use super::paths::ManagedPgPaths;

pub const MANAGED_POSTGRES_PORT: u16 = 15432;
pub const MANAGED_POSTGRES_DATABASE: &str = "onecontext_memory";
pub const MANAGED_POSTGRES_APP_USER: &str = "onecontext";
pub const MANAGED_POSTGRES_APP_PASSWORD: &str = "onecontext_dev";
pub const MANAGED_POSTGRES_SUPERUSER: &str = "postgres";
pub const MANAGED_POSTGRES_CONNECT_TIMEOUT_SECS: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPostgresConfigPlan {
    pub postgresql_conf: PathBuf,
    pub pg_hba_conf: PathBuf,
    pub socket_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub port: u16,
    pub shared_preload_libraries: Vec<String>,
    pub dynamic_library_paths: Vec<String>,
}

impl ManagedPostgresConfigPlan {
    pub fn for_paths(paths: &ManagedPgPaths) -> Self {
        Self {
            postgresql_conf: paths.pgdata.join("postgresql.conf"),
            pg_hba_conf: paths.pgdata.join("pg_hba.conf"),
            socket_dir: paths.socket_dir.clone(),
            logs_dir: paths.logs_dir.clone(),
            port: MANAGED_POSTGRES_PORT,
            shared_preload_libraries: vec![
                "timescaledb".to_string(),
                "pg_stat_statements".to_string(),
            ],
            dynamic_library_paths: vec!["$libdir".to_string()],
        }
    }

    pub fn for_paths_with_bundle_prefix(
        paths: &ManagedPgPaths,
        bundle_prefix: impl Into<PathBuf>,
    ) -> Self {
        let mut plan = Self::for_paths(paths);
        plan.dynamic_library_paths.push(
            bundle_prefix
                .into()
                .join("lib/postgresql")
                .display()
                .to_string(),
        );
        plan
    }

    pub fn database_url(&self) -> String {
        format!(
            "host={} port={} dbname={} user={} password={} connect_timeout={}",
            postgres_conninfo_value(&self.socket_dir.display().to_string()),
            self.port,
            postgres_conninfo_value(MANAGED_POSTGRES_DATABASE),
            postgres_conninfo_value(MANAGED_POSTGRES_APP_USER),
            postgres_conninfo_value(MANAGED_POSTGRES_APP_PASSWORD),
            MANAGED_POSTGRES_CONNECT_TIMEOUT_SECS
        )
    }

    pub fn render_postgresql_conf(&self) -> String {
        let socket_dir = postgres_conf_literal(&self.socket_dir);
        let logs_dir = postgres_conf_literal(&self.logs_dir);
        let pg_hba_conf = postgres_conf_literal(&self.pg_hba_conf);
        let preload_libraries = self.shared_preload_libraries.join(",");
        let dynamic_library_path = self
            .dynamic_library_paths
            .iter()
            .map(|path| postgres_conf_string_literal(path))
            .collect::<Vec<_>>()
            .join(":");
        format!(
            "# Managed by 1Context.\n\
listen_addresses = ''\n\
port = {}\n\
unix_socket_directories = '{}'\n\
unix_socket_permissions = 0700\n\
hba_file = '{}'\n\
shared_preload_libraries = '{}'\n\
dynamic_library_path = '{}'\n\
logging_collector = on\n\
log_directory = '{}'\n\
log_filename = 'postgresql-%Y-%m-%d_%H%M%S.log'\n\
log_rotation_age = '1d'\n\
log_truncate_on_rotation = on\n\
ssl = off\n",
            self.port, socket_dir, pg_hba_conf, preload_libraries, dynamic_library_path, logs_dir
        )
    }

    pub fn render_pg_hba_conf(&self) -> String {
        format!(
            "# Managed by 1Context.\n\
local   all             {superuser}                                trust\n\
local   all             {app_user}                                 scram-sha-256\n\
local   all             all                                        reject\n",
            superuser = MANAGED_POSTGRES_SUPERUSER,
            app_user = MANAGED_POSTGRES_APP_USER
        )
    }
}

fn postgres_conf_literal(path: &std::path::Path) -> String {
    path.display().to_string().replace('\'', "''")
}

fn postgres_conf_string_literal(value: &str) -> String {
    value.replace('\'', "''")
}

fn postgres_conninfo_value(value: &str) -> String {
    format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
}
