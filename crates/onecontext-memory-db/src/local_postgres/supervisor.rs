use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use crate::schema::bootstrap_current_schema;

use super::config::{
    ManagedPostgresConfigPlan, MANAGED_POSTGRES_APP_PASSWORD, MANAGED_POSTGRES_APP_USER,
    MANAGED_POSTGRES_DATABASE,
};
use super::error::ManagedPostgresError;
use super::health::{
    probe_managed_postgres_read_only_with_config, ManagedPostgresReadOnlyHealth,
    ManagedPostgresReadOnlyState,
};
use super::manifest::ManagedPgManifest;
use super::manifest::REQUIRED_MANAGED_POSTGRES_EXTENSIONS;
use super::paths::{ManagedPgPathConfig, ManagedPgPaths};
use super::process::{ManagedPostgresCommandOutput, ManagedPostgresCommandPlan};

const DEFAULT_READINESS_ATTEMPTS: u32 = 30;
const DEFAULT_READINESS_DELAY_MS: u64 = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPostgresEnsureOptions {
    pub path_config: ManagedPgPathConfig,
    pub bootstrap_schema: bool,
    pub readiness_attempts: u32,
    pub readiness_delay_ms: u64,
}

impl Default for ManagedPostgresEnsureOptions {
    fn default() -> Self {
        Self {
            path_config: ManagedPgPathConfig::from_env(),
            bootstrap_schema: true,
            readiness_attempts: DEFAULT_READINESS_ATTEMPTS,
            readiness_delay_ms: DEFAULT_READINESS_DELAY_MS,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPostgresSchemaBootstrapSummary {
    pub created: bool,
    pub validated_relations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedPostgresReadyState {
    pub health: ManagedPostgresReadOnlyHealth,
    pub database_url: String,
    pub initialized_cluster: bool,
    pub started_cluster: bool,
    pub created_app_role: bool,
    pub created_database: bool,
    pub schema_bootstrap: Option<ManagedPostgresSchemaBootstrapSummary>,
}

pub trait ManagedPostgresCommandRunner {
    fn run(
        &mut self,
        plan: &ManagedPostgresCommandPlan,
    ) -> Result<ManagedPostgresCommandOutput, ManagedPostgresError>;
}

pub trait ManagedPostgresSchemaBootstrapper {
    fn bootstrap(
        &mut self,
        database_url: &str,
    ) -> Result<ManagedPostgresSchemaBootstrapSummary, ManagedPostgresError>;
}

#[derive(Debug, Default)]
pub struct StdManagedPostgresCommandRunner;

impl ManagedPostgresCommandRunner for StdManagedPostgresCommandRunner {
    fn run(
        &mut self,
        plan: &ManagedPostgresCommandPlan,
    ) -> Result<ManagedPostgresCommandOutput, ManagedPostgresError> {
        let output = Command::new(&plan.program)
            .args(&plan.args)
            .output()
            .map_err(|source| ManagedPostgresError::io(&plan.program, source))?;
        Ok(ManagedPostgresCommandOutput {
            status: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}

#[derive(Debug, Default)]
pub struct StdManagedPostgresSchemaBootstrapper;

impl ManagedPostgresSchemaBootstrapper for StdManagedPostgresSchemaBootstrapper {
    fn bootstrap(
        &mut self,
        database_url: &str,
    ) -> Result<ManagedPostgresSchemaBootstrapSummary, ManagedPostgresError> {
        let report = bootstrap_current_schema(database_url)
            .map_err(|source| ManagedPostgresError::SchemaBootstrap { source })?;
        Ok(ManagedPostgresSchemaBootstrapSummary {
            created: report.created,
            validated_relations: report
                .validated_relations
                .into_iter()
                .map(ToString::to_string)
                .collect(),
        })
    }
}

pub fn ensure_managed_postgres_ready() -> Result<ManagedPostgresReadyState, ManagedPostgresError> {
    ensure_managed_postgres_ready_with_options(&ManagedPostgresEnsureOptions::default())
}

pub fn ensure_managed_postgres_ready_with_config(
    config: &ManagedPgPathConfig,
) -> Result<ManagedPostgresReadyState, ManagedPostgresError> {
    let options = ManagedPostgresEnsureOptions {
        path_config: config.clone(),
        ..ManagedPostgresEnsureOptions::default()
    };
    ensure_managed_postgres_ready_with_options(&options)
}

pub fn ensure_managed_postgres_ready_with_options(
    options: &ManagedPostgresEnsureOptions,
) -> Result<ManagedPostgresReadyState, ManagedPostgresError> {
    let mut runner = StdManagedPostgresCommandRunner;
    let mut bootstrapper = StdManagedPostgresSchemaBootstrapper;
    ensure_managed_postgres_ready_with_runner_and_bootstrapper(
        options,
        &mut runner,
        &mut bootstrapper,
    )
}

pub fn ensure_managed_postgres_ready_with_runner(
    options: &ManagedPostgresEnsureOptions,
    runner: &mut impl ManagedPostgresCommandRunner,
) -> Result<ManagedPostgresReadyState, ManagedPostgresError> {
    let mut bootstrapper = StdManagedPostgresSchemaBootstrapper;
    ensure_managed_postgres_ready_with_runner_and_bootstrapper(options, runner, &mut bootstrapper)
}

pub fn ensure_managed_postgres_ready_with_runner_and_bootstrapper(
    options: &ManagedPostgresEnsureOptions,
    runner: &mut impl ManagedPostgresCommandRunner,
    bootstrapper: &mut impl ManagedPostgresSchemaBootstrapper,
) -> Result<ManagedPostgresReadyState, ManagedPostgresError> {
    let paths = options.path_config.resolve_paths();
    let manifest =
        ManagedPgManifest::load_and_validate(options.path_config.resolve_bundle_prefix())?;
    paths.create_all_secure()?;
    let _lock = BootstrapLock::acquire(&paths.bootstrap_lock)?;

    let config_plan =
        ManagedPostgresConfigPlan::for_paths_with_bundle_prefix(&paths, &manifest.prefix);
    let database_url = config_plan.database_url();
    let mut initialized_cluster = false;
    let mut started_cluster = false;

    if !paths.pgdata.join("PG_VERSION").is_file() {
        run_required(
            runner,
            &ManagedPostgresCommandPlan::initdb(&manifest, &paths),
        )?;
        write_private_file(
            &paths.pgdata.join("PG_VERSION"),
            &format!("{}\n", manifest.postgres_major),
        )?;
        initialized_cluster = true;
    }

    write_private_file(
        &config_plan.postgresql_conf,
        &config_plan.render_postgresql_conf(),
    )?;
    write_private_file(&config_plan.pg_hba_conf, &config_plan.render_pg_hba_conf())?;
    write_private_file(&paths.auth_dir.join("pgpass"), &render_pgpass(&config_plan))?;

    if !wait_until_ready(runner, &manifest, &paths, 1, 0)? {
        run_required(
            runner,
            &ManagedPostgresCommandPlan::start(&manifest, &paths),
        )?;
        started_cluster = true;
    }

    if !wait_until_ready(
        runner,
        &manifest,
        &paths,
        options.readiness_attempts,
        options.readiness_delay_ms,
    )? {
        return Err(ManagedPostgresError::command_failed(
            manifest.pg_isready_bin.clone(),
            ManagedPostgresCommandPlan::readiness(&manifest, &paths).args,
            Some(1),
            "",
            "managed Postgres did not become ready before the readiness timeout",
        ));
    }

    let created_app_role = ensure_app_role(runner, &manifest, &paths)?;
    let created_database = ensure_database(runner, &manifest, &paths)?;
    ensure_required_extensions(runner, &manifest, &paths)?;

    let schema_bootstrap = if options.bootstrap_schema {
        Some(bootstrapper.bootstrap(&database_url)?)
    } else {
        None
    };

    let health = probe_managed_postgres_read_only_with_config(&options.path_config);
    if health.state != ManagedPostgresReadOnlyState::Ready {
        return Err(ManagedPostgresError::command_failed(
            manifest.pg_isready_bin.clone(),
            ManagedPostgresCommandPlan::readiness(&manifest, &paths).args,
            Some(1),
            "",
            format!(
                "managed Postgres finished bootstrap but probe state is {}",
                health.state.as_str()
            ),
        ));
    }

    Ok(ManagedPostgresReadyState {
        health,
        database_url,
        initialized_cluster,
        started_cluster,
        created_app_role,
        created_database,
        schema_bootstrap,
    })
}

fn wait_until_ready(
    runner: &mut impl ManagedPostgresCommandRunner,
    manifest: &ManagedPgManifest,
    paths: &ManagedPgPaths,
    attempts: u32,
    delay_ms: u64,
) -> Result<bool, ManagedPostgresError> {
    let attempts = attempts.max(1);
    let plan = ManagedPostgresCommandPlan::readiness(manifest, paths);
    for attempt in 0..attempts {
        let output = runner.run(&plan)?;
        if output.success() {
            return Ok(true);
        }
        if attempt + 1 < attempts && delay_ms > 0 {
            thread::sleep(Duration::from_millis(delay_ms));
        }
    }
    Ok(false)
}

fn ensure_app_role(
    runner: &mut impl ManagedPostgresCommandRunner,
    manifest: &ManagedPgManifest,
    paths: &ManagedPgPaths,
) -> Result<bool, ManagedPostgresError> {
    let exists = sql_exists(
        runner,
        &ManagedPostgresCommandPlan::psql_superuser(
            manifest,
            paths,
            "postgres",
            "SELECT 1 FROM pg_roles WHERE rolname = 'onecontext';",
        ),
    )?;
    run_required(
        runner,
        &ManagedPostgresCommandPlan::psql_superuser(
            manifest,
            paths,
            "postgres",
            "DO $$ BEGIN \
             IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'onecontext') THEN \
               CREATE ROLE onecontext NOSUPERUSER LOGIN PASSWORD 'onecontext_dev'; \
             ELSE \
               ALTER ROLE onecontext WITH NOSUPERUSER LOGIN PASSWORD 'onecontext_dev'; \
             END IF; \
             END $$;",
        ),
    )?;
    Ok(!exists)
}

fn ensure_database(
    runner: &mut impl ManagedPostgresCommandRunner,
    manifest: &ManagedPgManifest,
    paths: &ManagedPgPaths,
) -> Result<bool, ManagedPostgresError> {
    let exists = sql_exists(
        runner,
        &ManagedPostgresCommandPlan::psql_superuser(
            manifest,
            paths,
            "postgres",
            "SELECT 1 FROM pg_database WHERE datname = 'onecontext_memory';",
        ),
    )?;
    if !exists {
        if let Some(plan) = ManagedPostgresCommandPlan::createdb(manifest, paths) {
            run_required(runner, &plan)?;
        } else {
            run_required(
                runner,
                &ManagedPostgresCommandPlan::psql_superuser(
                    manifest,
                    paths,
                    "postgres",
                    "CREATE DATABASE onecontext_memory OWNER onecontext;",
                ),
            )?;
        }
        return Ok(true);
    }

    run_required(
        runner,
        &ManagedPostgresCommandPlan::psql_superuser(
            manifest,
            paths,
            "postgres",
            "ALTER DATABASE onecontext_memory OWNER TO onecontext;",
        ),
    )?;
    Ok(false)
}

fn ensure_required_extensions(
    runner: &mut impl ManagedPostgresCommandRunner,
    manifest: &ManagedPgManifest,
    paths: &ManagedPgPaths,
) -> Result<(), ManagedPostgresError> {
    let sql = REQUIRED_MANAGED_POSTGRES_EXTENSIONS
        .iter()
        .map(|extension| format!("CREATE EXTENSION IF NOT EXISTS {extension};"))
        .collect::<Vec<_>>()
        .join("\n");
    run_required(
        runner,
        &ManagedPostgresCommandPlan::psql_superuser(
            manifest,
            paths,
            MANAGED_POSTGRES_DATABASE,
            &sql,
        ),
    )?;
    Ok(())
}

fn sql_exists(
    runner: &mut impl ManagedPostgresCommandRunner,
    plan: &ManagedPostgresCommandPlan,
) -> Result<bool, ManagedPostgresError> {
    let output = run_required(runner, plan)?;
    Ok(output.stdout.trim() == "1")
}

fn run_required(
    runner: &mut impl ManagedPostgresCommandRunner,
    plan: &ManagedPostgresCommandPlan,
) -> Result<ManagedPostgresCommandOutput, ManagedPostgresError> {
    let output = runner.run(plan)?;
    if output.success() {
        return Ok(output);
    }
    Err(ManagedPostgresError::command_failed(
        plan.program.clone(),
        plan.args.clone(),
        output.status,
        output.stdout,
        output.stderr,
    ))
}

fn render_pgpass(config_plan: &ManagedPostgresConfigPlan) -> String {
    format!(
        "{}:{}:{}:{}:{}\n",
        config_plan.socket_dir.display(),
        config_plan.port,
        MANAGED_POSTGRES_DATABASE,
        MANAGED_POSTGRES_APP_USER,
        MANAGED_POSTGRES_APP_PASSWORD
    )
}

fn write_private_file(path: &Path, contents: &str) -> Result<(), ManagedPostgresError> {
    let Some(parent) = path.parent() else {
        return Err(ManagedPostgresError::io(
            path,
            std::io::Error::other("managed Postgres file has no parent directory"),
        ));
    };
    std::fs::create_dir_all(parent).map_err(|source| ManagedPostgresError::io(parent, source))?;
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|source| ManagedPostgresError::io(path, source))?;
    file.write_all(contents.as_bytes())
        .map_err(|source| ManagedPostgresError::io(path, source))?;
    set_private_file_permissions(path)?;
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(path: &Path) -> Result<(), ManagedPostgresError> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
        .map_err(|source| ManagedPostgresError::io(path, source))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_path: &Path) -> Result<(), ManagedPostgresError> {
    Ok(())
}

struct BootstrapLock {
    path: PathBuf,
}

impl BootstrapLock {
    fn acquire(path: &Path) -> Result<Self, ManagedPostgresError> {
        let Some(parent) = path.parent() else {
            return Err(ManagedPostgresError::io(
                path,
                std::io::Error::other("managed Postgres lock file has no parent directory"),
            ));
        };
        std::fs::create_dir_all(parent)
            .map_err(|source| ManagedPostgresError::io(parent, source))?;
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(path)
            .map_err(|source| match source.kind() {
                std::io::ErrorKind::AlreadyExists => ManagedPostgresError::BootstrapLocked {
                    path: path.to_path_buf(),
                },
                _ => ManagedPostgresError::io(path, source),
            })?;
        writeln!(file, "pid={}", std::process::id())
            .map_err(|source| ManagedPostgresError::io(path, source))?;
        Ok(Self {
            path: path.to_path_buf(),
        })
    }
}

impl Drop for BootstrapLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::fs;
    use std::path::Path;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use postgres::{Client, NoTls};

    use super::*;
    use crate::local_postgres::{
        current_manifest_arch, ManagedPgManifest, ManagedPgManifestBins,
        ManagedPgManifestExtensions, ManagedPgManifestFile, EXPECTED_POSTGRES_MAJOR,
        MANIFEST_FILE_NAME, REQUIRED_MANAGED_POSTGRES_EXTENSIONS,
        REQUIRED_MANAGED_POSTGRES_PRELOAD_LIBRARIES,
    };

    #[test]
    fn ensure_ready_initializes_and_starts_cluster_without_live_postgres() {
        let temp = temp_root("ensure-init");
        let prefix = temp.join("managed-postgres/macos-arm64");
        create_fake_prefix(&prefix);
        let app_support = temp.join("Application Support/1Context");
        let options = ManagedPostgresEnsureOptions {
            path_config: ManagedPgPathConfig {
                app_support_dir: Some(app_support.clone()),
                managed_pg_prefix: Some(prefix.clone()),
            },
            bootstrap_schema: false,
            readiness_attempts: 3,
            readiness_delay_ms: 0,
        };
        let mut runner = FakeRunner::new(vec![
            CommandResponse::failure("starting up"),
            CommandResponse::failure("starting up"),
            CommandResponse::success(""),
        ]);
        let mut bootstrapper = FakeBootstrapper::default();

        let report = ensure_managed_postgres_ready_with_runner_and_bootstrapper(
            &options,
            &mut runner,
            &mut bootstrapper,
        )
        .expect("managed Postgres ready");

        assert!(report.health.ready);
        assert!(report.initialized_cluster);
        assert!(report.started_cluster);
        assert!(report.created_app_role);
        assert!(report.created_database);
        assert!(report.database_url.starts_with("host='"));
        assert!(report.database_url.contains(&format!(
            "{}' port=15432",
            options.path_config.resolve_paths().socket_dir.display()
        )));
        assert!(report.database_url.contains("dbname='onecontext_memory'"));
        assert!(report.database_url.contains("user='onecontext'"));
        assert!(report.database_url.contains("password='onecontext_dev'"));
        assert!(!report.database_url.contains("127.0.0.1"));
        assert!(report.schema_bootstrap.is_none());
        assert!(app_support
            .join("Postgres/pgdata/postgresql.conf")
            .is_file());
        assert!(app_support.join("Postgres/pgdata/pg_hba.conf").is_file());
        assert!(app_support.join("Postgres/auth/pgpass").is_file());
        let conf = fs::read_to_string(app_support.join("Postgres/pgdata/postgresql.conf"))
            .expect("postgresql.conf");
        assert!(conf.contains("listen_addresses = ''"));
        assert!(conf.contains("unix_socket_permissions = 0700"));
        assert!(conf.contains("shared_preload_libraries = 'timescaledb,pg_stat_statements'"));
        assert!(conf.contains("dynamic_library_path = '$libdir:"));
        assert!(conf.contains("managed-postgres/macos-arm64/lib/postgresql"));
        let hba = fs::read_to_string(app_support.join("Postgres/pgdata/pg_hba.conf")).expect("hba");
        assert!(hba.contains("local   all             onecontext"));
        assert!(hba.contains("scram-sha-256"));
        assert!(!hba.contains("\nhost"));
        assert!(runner.saw_program("initdb"));
        assert!(runner.saw_program("pg_ctl"));
        assert!(runner.saw_program("createdb"));
        assert!(runner.saw_sql("CREATE ROLE onecontext NOSUPERUSER LOGIN PASSWORD"));
        assert!(runner.saw_sql("CREATE EXTENSION IF NOT EXISTS vector;"));
        assert!(!runner.saw_sql("CREATE ROLE onecontext SUPERUSER"));
        assert!(!runner.saw_sql("ALTER ROLE onecontext WITH SUPERUSER"));
    }

    #[test]
    fn ensure_ready_reuses_running_cluster_and_bootstraps_schema_hook() {
        let temp = temp_root("ensure-reuse");
        let prefix = temp.join("managed-postgres/macos-arm64");
        create_fake_prefix(&prefix);
        let app_support = temp.join("Application Support/1Context");
        let pgdata = app_support.join("Postgres/pgdata");
        fs::create_dir_all(&pgdata).expect("pgdata");
        fs::write(pgdata.join("PG_VERSION"), "17\n").expect("pg version");
        let options = ManagedPostgresEnsureOptions {
            path_config: ManagedPgPathConfig {
                app_support_dir: Some(app_support),
                managed_pg_prefix: Some(prefix.clone()),
            },
            bootstrap_schema: true,
            readiness_attempts: 2,
            readiness_delay_ms: 0,
        };
        fs::write(prefix.join("ready.flag"), "").expect("ready flag");
        let mut runner = FakeRunner::ready();
        let mut bootstrapper = FakeBootstrapper {
            calls: Vec::new(),
            result: Some(ManagedPostgresSchemaBootstrapSummary {
                created: true,
                validated_relations: vec!["perception.objects".to_string()],
            }),
        };

        let report = ensure_managed_postgres_ready_with_runner_and_bootstrapper(
            &options,
            &mut runner,
            &mut bootstrapper,
        )
        .expect("managed Postgres ready");

        assert!(report.health.ready);
        assert!(!report.initialized_cluster);
        assert!(!report.started_cluster);
        assert!(!report.created_app_role);
        assert!(!report.created_database);
        assert_eq!(bootstrapper.calls, vec![report.database_url.clone()]);
        assert_eq!(
            report.schema_bootstrap,
            Some(ManagedPostgresSchemaBootstrapSummary {
                created: true,
                validated_relations: vec!["perception.objects".to_string()],
            })
        );
        assert!(!runner.saw_program("initdb"));
        assert!(!runner.saw_program("createdb"));
    }

    #[test]
    fn live_ensure_is_opt_in() {
        if std::env::var_os("ONECONTEXT_MANAGED_POSTGRES_LIVE_TEST").is_none() {
            eprintln!(
                "skipping live managed Postgres test; set ONECONTEXT_MANAGED_POSTGRES_LIVE_TEST=1"
            );
            return;
        }

        let prefix = std::env::var_os("ONECONTEXT_MANAGED_PG_PREFIX")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("runtime/managed-postgres/macos-arm64"));
        let temp = temp_root("ensure-live");
        let report = ensure_managed_postgres_ready_with_config(&ManagedPgPathConfig {
            app_support_dir: Some(temp.join("Application Support/1Context")),
            managed_pg_prefix: Some(prefix),
        })
        .expect("live managed Postgres ready");

        assert!(report.health.ready);
        assert!(report.database_url.contains("onecontext_memory"));

        let mut client =
            Client::connect(&report.database_url, NoTls).expect("app role connects over socket");
        let row = client
            .query_one(
                "SELECT current_user, inet_server_addr() IS NULL FROM pg_catalog.pg_roles WHERE rolname = current_user AND NOT rolsuper;",
                &[],
            )
            .expect("app role is non-superuser on a Unix socket");
        let user: String = row.get(0);
        let socket_connection: bool = row.get(1);
        assert_eq!(user, MANAGED_POSTGRES_APP_USER);
        assert!(socket_connection);

        let extension_count: i64 = client
            .query_one(
                "SELECT count(*) FROM pg_extension WHERE extname = ANY($1)",
                &[&REQUIRED_MANAGED_POSTGRES_EXTENSIONS],
            )
            .expect("required extensions are installed")
            .get(0);
        assert_eq!(
            extension_count,
            REQUIRED_MANAGED_POSTGRES_EXTENSIONS.len() as i64
        );
        drop(client);

        let manifest = ManagedPgManifest::load_and_validate(&report.health.bundle_prefix)
            .expect("reload live manifest for cleanup");
        let paths = ManagedPgPaths::from_app_support(&report.health.app_support_dir);
        let mut runner = StdManagedPostgresCommandRunner;
        runner
            .run(&ManagedPostgresCommandPlan::stop(&manifest, &paths))
            .expect("stop live managed Postgres");
    }

    #[derive(Debug, Clone)]
    struct CommandResponse {
        status: Option<i32>,
        stdout: String,
        stderr: String,
    }

    impl CommandResponse {
        fn success(stdout: &str) -> Self {
            Self {
                status: Some(0),
                stdout: stdout.to_string(),
                stderr: String::new(),
            }
        }

        fn failure(stderr: &str) -> Self {
            Self {
                status: Some(1),
                stdout: String::new(),
                stderr: stderr.to_string(),
            }
        }
    }

    #[derive(Debug)]
    struct FakeRunner {
        readiness: VecDeque<CommandResponse>,
        commands: Vec<ManagedPostgresCommandPlan>,
        role_exists: bool,
        database_exists: bool,
    }

    impl FakeRunner {
        fn new(readiness: Vec<CommandResponse>) -> Self {
            Self {
                readiness: readiness.into(),
                commands: Vec::new(),
                role_exists: false,
                database_exists: false,
            }
        }

        fn ready() -> Self {
            Self {
                readiness: vec![
                    CommandResponse::success(""),
                    CommandResponse::success(""),
                    CommandResponse::success(""),
                ]
                .into(),
                commands: Vec::new(),
                role_exists: true,
                database_exists: true,
            }
        }

        fn saw_program(&self, suffix: &str) -> bool {
            self.commands.iter().any(|plan| {
                plan.program
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name == suffix)
            })
        }

        fn saw_sql(&self, needle: &str) -> bool {
            self.commands.iter().any(|plan| {
                plan.args
                    .windows(2)
                    .any(|window| window[0] == "-c" && window[1].contains(needle))
            })
        }
    }

    impl ManagedPostgresCommandRunner for FakeRunner {
        fn run(
            &mut self,
            plan: &ManagedPostgresCommandPlan,
        ) -> Result<ManagedPostgresCommandOutput, ManagedPostgresError> {
            self.commands.push(plan.clone());
            let program = plan
                .program
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or_default();
            let output = match program {
                "pg_isready" => self
                    .readiness
                    .pop_front()
                    .unwrap_or_else(|| CommandResponse::success("")),
                "initdb" => CommandResponse::success(""),
                "pg_ctl" => {
                    if plan.args.iter().any(|arg| arg == "start") {
                        let ready_flag = plan
                            .program
                            .parent()
                            .and_then(|bin| bin.parent())
                            .map(|prefix| prefix.join("ready.flag"))
                            .expect("ready flag path");
                        fs::write(ready_flag, "").expect("ready flag");
                    }
                    CommandResponse::success("")
                }
                "createdb" => {
                    self.database_exists = true;
                    CommandResponse::success("")
                }
                "psql" => self.run_psql(plan),
                other => {
                    return Err(ManagedPostgresError::command_failed(
                        other,
                        plan.args.clone(),
                        Some(1),
                        "",
                        "unexpected fake command",
                    ));
                }
            };
            Ok(ManagedPostgresCommandOutput {
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
            })
        }
    }

    impl FakeRunner {
        fn run_psql(&mut self, plan: &ManagedPostgresCommandPlan) -> CommandResponse {
            let sql = plan
                .args
                .windows(2)
                .find_map(|window| (window[0] == "-c").then(|| window[1].as_str()))
                .unwrap_or_default();
            if sql.contains("FROM pg_roles") {
                return if self.role_exists {
                    CommandResponse::success("1\n")
                } else {
                    CommandResponse::success("")
                };
            }
            if sql.contains("CREATE ROLE onecontext") || sql.contains("ALTER ROLE onecontext") {
                self.role_exists = true;
                return CommandResponse::success("");
            }
            if sql.contains("FROM pg_database") {
                return if self.database_exists {
                    CommandResponse::success("1\n")
                } else {
                    CommandResponse::success("")
                };
            }
            if sql.contains("CREATE DATABASE onecontext_memory")
                || sql.contains("ALTER DATABASE onecontext_memory OWNER TO onecontext")
            {
                self.database_exists = true;
                return CommandResponse::success("");
            }
            CommandResponse::success("")
        }
    }

    #[derive(Debug, Default)]
    struct FakeBootstrapper {
        calls: Vec<String>,
        result: Option<ManagedPostgresSchemaBootstrapSummary>,
    }

    impl ManagedPostgresSchemaBootstrapper for FakeBootstrapper {
        fn bootstrap(
            &mut self,
            database_url: &str,
        ) -> Result<ManagedPostgresSchemaBootstrapSummary, ManagedPostgresError> {
            self.calls.push(database_url.to_string());
            Ok(self
                .result
                .clone()
                .unwrap_or(ManagedPostgresSchemaBootstrapSummary {
                    created: false,
                    validated_relations: Vec::new(),
                }))
        }
    }

    fn create_fake_prefix(prefix: &Path) {
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
            let script = if bin == "pg_isready" {
                "#!/bin/sh\nREADY_FILE=\"$(dirname \"$0\")/../ready.flag\"\nif [ -f \"$READY_FILE\" ]; then\n  exit 0\nfi\nexit 1\n"
            } else {
                "#!/bin/sh\nexit 0\n"
            };
            fs::write(&path, script).expect("fake binary");
            set_executable(&path);
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
        let manifest = ManagedPgManifestFile {
            bundle_schema: 1,
            arch: current_manifest_arch(),
            postgres_major: EXPECTED_POSTGRES_MAJOR,
            postgres_version: "17.test".to_string(),
            timescale_version: "2.test".to_string(),
            build_id: "managed-pg17-test".to_string(),
            bin: ManagedPgManifestBins {
                postgres: "bin/postgres".to_string(),
                initdb: "bin/initdb".to_string(),
                pg_ctl: "bin/pg_ctl".to_string(),
                pg_isready: "bin/pg_isready".to_string(),
                psql: "bin/psql".to_string(),
                createdb: Some("bin/createdb".to_string()),
            },
            extension: ManagedPgManifestExtensions {
                timescaledb_control: "share/postgresql/extension/timescaledb.control".to_string(),
                timescaledb_library_glob: "lib/postgresql/timescaledb*.dylib".to_string(),
            },
            required_extensions: REQUIRED_MANAGED_POSTGRES_EXTENSIONS
                .iter()
                .map(ToString::to_string)
                .collect(),
            required_preload_libraries: REQUIRED_MANAGED_POSTGRES_PRELOAD_LIBRARIES
                .iter()
                .map(ToString::to_string)
                .collect(),
        };
        fs::write(
            prefix.join(MANIFEST_FILE_NAME),
            serde_json::to_vec_pretty(&manifest).expect("manifest json"),
        )
        .expect("manifest");
        let loaded =
            ManagedPgManifest::load_and_validate(prefix.to_path_buf()).expect("valid manifest");
        assert_eq!(loaded.postgres_major, EXPECTED_POSTGRES_MAJOR);
    }

    #[cfg(unix)]
    fn set_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;

        fs::set_permissions(path, fs::Permissions::from_mode(0o755)).expect("permissions");
    }

    #[cfg(not(unix))]
    fn set_executable(_path: &Path) {}

    fn temp_root(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "onecontext-managed-pg-supervisor-{label}-{}-{nanos}",
            std::process::id()
        ));
        if root.exists() {
            fs::remove_dir_all(&root).expect("remove stale temp root");
        }
        root
    }
}
