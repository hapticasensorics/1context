//! App-managed private PostgreSQL support for Perception DB.

pub mod config;
pub mod error;
pub mod health;
pub mod manifest;
pub mod paths;
pub mod process;
pub mod supervisor;

pub use config::{
    ManagedPostgresConfigPlan, MANAGED_POSTGRES_APP_PASSWORD, MANAGED_POSTGRES_APP_USER,
    MANAGED_POSTGRES_CONNECT_TIMEOUT_SECS, MANAGED_POSTGRES_DATABASE, MANAGED_POSTGRES_PORT,
    MANAGED_POSTGRES_SUPERUSER,
};
pub use error::ManagedPostgresError;
pub use health::{
    probe_managed_postgres_read_only, probe_managed_postgres_read_only_with_config,
    required_extension_requirements, ManagedPostgresExtensionRequirement,
    ManagedPostgresReadOnlyHealth, ManagedPostgresReadOnlyState,
};
pub use manifest::{
    current_manifest_arch, ManagedPgManifest, ManagedPgManifestBins, ManagedPgManifestExtensions,
    ManagedPgManifestFile, EXPECTED_POSTGRES_MAJOR, MANIFEST_FILE_NAME,
    REQUIRED_MANAGED_POSTGRES_EXTENSIONS, REQUIRED_MANAGED_POSTGRES_PRELOAD_LIBRARIES,
};
pub use paths::{
    default_app_support_dir, default_managed_pg_prefix, ManagedPgPathConfig, ManagedPgPaths,
    APP_SUPPORT_DIR_ENV, MANAGED_PG_PREFIX_ENV,
};
pub use process::{ManagedPostgresCommandOutput, ManagedPostgresCommandPlan};
pub use supervisor::{
    ensure_managed_postgres_ready, ensure_managed_postgres_ready_with_config,
    ensure_managed_postgres_ready_with_options, ensure_managed_postgres_ready_with_runner,
    ensure_managed_postgres_ready_with_runner_and_bootstrapper, ManagedPostgresCommandRunner,
    ManagedPostgresEnsureOptions, ManagedPostgresReadyState, ManagedPostgresSchemaBootstrapSummary,
    ManagedPostgresSchemaBootstrapper, StdManagedPostgresCommandRunner,
    StdManagedPostgresSchemaBootstrapper,
};

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    #[test]
    fn managed_paths_follow_application_support_contract() {
        let root = PathBuf::from("/tmp/onecontext-app-support-test");
        let paths = ManagedPgPaths::from_app_support(&root);

        assert_eq!(paths.app_support, root);
        assert_eq!(paths.pgdata, paths.postgres_root.join("pgdata"));
        assert!(paths.socket_dir.starts_with(std::env::temp_dir()));
        assert!(paths
            .socket_dir
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("onecontext-pg-")));
        assert_eq!(paths.backups_dir, paths.postgres_root.join("backups"));
        assert_eq!(
            paths.bootstrap_lock,
            paths.bootstrap_dir.join("bootstrap.lock")
        );
    }

    #[test]
    fn read_only_probe_reports_missing_bundle() {
        let temp = temp_root("missing-bundle");
        let config = ManagedPgPathConfig {
            app_support_dir: Some(temp.join("Application Support/1Context")),
            managed_pg_prefix: Some(temp.join("managed-postgres/macos-arm64")),
        };

        let health = probe_managed_postgres_read_only_with_config(&config);

        assert_eq!(health.state, ManagedPostgresReadOnlyState::BundleMissing);
        assert!(!health.ready);
        assert!(health.safe_to_retry);
    }

    #[test]
    fn manifest_validation_accepts_fake_executable_prefix() {
        let temp = temp_root("valid-prefix");
        let prefix = temp.join("managed-postgres/macos-arm64");
        create_fake_prefix(&prefix, true);

        let manifest = ManagedPgManifest::load_and_validate(&prefix).expect("valid manifest");

        assert_eq!(manifest.postgres_major, EXPECTED_POSTGRES_MAJOR);
        assert_eq!(manifest.arch, current_manifest_arch());
        assert_eq!(
            manifest.postgres_bin,
            std::fs::canonicalize(&prefix)
                .expect("canonical prefix")
                .join("bin/postgres")
        );
        assert_eq!(
            manifest.required_extensions,
            REQUIRED_MANAGED_POSTGRES_EXTENSIONS
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            manifest.required_preload_libraries,
            REQUIRED_MANAGED_POSTGRES_PRELOAD_LIBRARIES
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn read_only_probe_reports_cluster_uninitialized_after_valid_bundle() {
        let temp = temp_root("cluster-uninitialized");
        let prefix = temp.join("managed-postgres/macos-arm64");
        create_fake_prefix(&prefix, true);
        let config = ManagedPgPathConfig {
            app_support_dir: Some(temp.join("Application Support/1Context")),
            managed_pg_prefix: Some(prefix),
        };

        let health = probe_managed_postgres_read_only_with_config(&config);

        assert_eq!(
            health.state,
            ManagedPostgresReadOnlyState::ClusterUninitialized
        );
        assert_eq!(health.postgres_major, Some(EXPECTED_POSTGRES_MAJOR));
        assert_eq!(health.timescale_version.as_deref(), Some("2.test"));
    }

    #[test]
    fn manifest_validation_rejects_non_executable_binary() {
        let temp = temp_root("invalid-prefix");
        let prefix = temp.join("managed-postgres/macos-arm64");
        create_fake_prefix(&prefix, false);

        let error = ManagedPgManifest::load_and_validate(&prefix).expect_err("invalid manifest");

        assert!(
            error.to_string().contains("not executable"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn required_extensions_keep_timescale_and_stats_preloaded() {
        let requirements = required_extension_requirements();

        assert!(requirements.iter().any(|extension| {
            extension.name == "timescaledb" && extension.required && extension.preload_required
        }));
        assert!(requirements.iter().any(|extension| {
            extension.name == "pg_stat_statements"
                && extension.required
                && extension.preload_required
        }));
        assert!(requirements.iter().any(|extension| {
            extension.name == "pgcrypto" && extension.required && !extension.preload_required
        }));
    }

    fn create_fake_prefix(prefix: &Path, executable: bool) {
        fs::create_dir_all(prefix.join("bin")).expect("bin dir");
        fs::create_dir_all(prefix.join("share/postgresql/extension")).expect("extension dir");
        fs::create_dir_all(prefix.join("lib/postgresql")).expect("library dir");
        for bin in [
            "postgres",
            "initdb",
            "pg_ctl",
            "pg_isready",
            "psql",
            "createdb",
        ] {
            let path = prefix.join("bin").join(bin);
            fs::write(&path, "#!/bin/sh\n").expect("fake binary");
            set_executable(&path, executable);
        }
        for extension in REQUIRED_MANAGED_POSTGRES_EXTENSIONS {
            fs::write(
                prefix
                    .join("share/postgresql/extension")
                    .join(format!("{extension}.control")),
                format!("comment = 'fake {extension} control'\n"),
            )
            .expect("extension control");
        }
        fs::write(prefix.join("lib/postgresql/timescaledb-test.dylib"), "")
            .expect("timescale dylib");
        fs::write(
            prefix.join(MANIFEST_FILE_NAME),
            format!(
                r#"{{
  "bundle_schema": 1,
  "arch": "{}",
  "postgres_major": {},
  "postgres_version": "17.test",
  "timescale_version": "2.test",
  "build_id": "managed-pg17-test",
  "bin": {{
    "postgres": "bin/postgres",
    "initdb": "bin/initdb",
    "pg_ctl": "bin/pg_ctl",
    "pg_isready": "bin/pg_isready",
    "psql": "bin/psql",
    "createdb": "bin/createdb"
  }},
  "extension": {{
    "timescaledb_control": "share/postgresql/extension/timescaledb.control",
    "timescaledb_library_glob": "lib/postgresql/timescaledb*.dylib"
  }},
  "required_extensions": [
    "timescaledb",
    "btree_gist",
    "pgcrypto",
    "pg_trgm",
    "vector",
    "pg_stat_statements"
  ],
  "required_preload_libraries": [
    "timescaledb",
    "pg_stat_statements"
  ]
}}"#,
                current_manifest_arch(),
                EXPECTED_POSTGRES_MAJOR
            ),
        )
        .expect("manifest");
    }

    #[cfg(unix)]
    fn set_executable(path: &Path, executable: bool) {
        use std::os::unix::fs::PermissionsExt;
        let mode = if executable { 0o755 } else { 0o644 };
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).expect("permissions");
    }

    #[cfg(not(unix))]
    fn set_executable(_path: &Path, _executable: bool) {}

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "onecontext-managed-pg-{label}-{}-{nanos}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale temp root");
        }
        root
    }
}
