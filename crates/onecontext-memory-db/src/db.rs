use std::collections::HashSet;
use std::fmt;

use postgres::{Client, GenericClient, NoTls};

use crate::migrations::{
    apply_bundled_migrations_with_client, MigrationApplyReport, MigrationRunnerError, MIGRATIONS,
};

pub const DATABASE_URL_ENV: &str = "ONECONTEXT_MEMORY_DB_URL";
pub const LEGACY_DATABASE_URL_ENV: &str = "ONECONTEXT_MEMORY_DATABASE_URL";
pub const FALLBACK_DATABASE_URL_ENV: &str = "DATABASE_URL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseUrl {
    pub url: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppliedMigrationState {
    pub version: u16,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationState {
    pub applied: Vec<AppliedMigrationState>,
    pub pending: Vec<&'static str>,
    pub total: usize,
}

impl MigrationState {
    pub fn is_current(&self) -> bool {
        self.pending.is_empty()
    }
}

#[derive(Debug)]
pub enum DbError {
    MissingDatabaseUrl,
    Postgres {
        step: String,
        source: postgres::Error,
    },
    Migration(MigrationRunnerError),
}

impl fmt::Display for DbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDatabaseUrl => write!(formatter, "missing database URL"),
            Self::Postgres { step, source } => {
                write!(formatter, "postgres failed while {step}: {source}")
            }
            Self::Migration(source) => write!(formatter, "{source}"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingDatabaseUrl => None,
            Self::Postgres { source, .. } => Some(source),
            Self::Migration(source) => Some(source),
        }
    }
}

impl From<MigrationRunnerError> for DbError {
    fn from(value: MigrationRunnerError) -> Self {
        Self::Migration(value)
    }
}

pub struct MemoryDatabase {
    client: Client,
}

impl MemoryDatabase {
    pub fn connect(database_url: &DatabaseUrl) -> Result<Self, DbError> {
        Self::connect_url(&database_url.url)
    }

    pub fn connect_url(database_url: &str) -> Result<Self, DbError> {
        let client = Client::connect(database_url, NoTls).map_err(|source| DbError::Postgres {
            step: "connecting to database".to_string(),
            source,
        })?;
        Ok(Self { client })
    }

    pub fn connect_from_env() -> Result<Self, DbError> {
        let database_url = resolve_database_url(None).ok_or(DbError::MissingDatabaseUrl)?;
        Self::connect(&database_url)
    }

    pub fn client_mut(&mut self) -> &mut Client {
        &mut self.client
    }

    pub fn into_inner(self) -> Client {
        self.client
    }

    pub fn apply_migrations(&mut self) -> Result<MigrationApplyReport, DbError> {
        apply_bundled_migrations_with_client(&mut self.client).map_err(DbError::from)
    }

    pub fn migration_state(&mut self) -> Result<MigrationState, DbError> {
        migration_state_with_client(&mut self.client)
    }
}

pub fn resolve_database_url(explicit_database_url: Option<&str>) -> Option<DatabaseUrl> {
    if let Some(url) = nonempty(explicit_database_url) {
        return Some(DatabaseUrl {
            url: url.to_string(),
            source: "explicit".to_string(),
        });
    }

    for key in [
        DATABASE_URL_ENV,
        LEGACY_DATABASE_URL_ENV,
        FALLBACK_DATABASE_URL_ENV,
    ] {
        if let Ok(value) = std::env::var(key) {
            if let Some(url) = nonempty(Some(&value)) {
                return Some(DatabaseUrl {
                    url: url.to_string(),
                    source: format!("env:{key}"),
                });
            }
        }
    }

    None
}

pub fn migration_state_with_client(
    client: &mut impl GenericClient,
) -> Result<MigrationState, DbError> {
    let row = client
        .query_one(
            "SELECT to_regclass('app.schema_migrations') IS NOT NULL;",
            &[],
        )
        .map_err(|source| DbError::Postgres {
            step: "checking app.schema_migrations".to_string(),
            source,
        })?;

    let has_schema_migrations = row.get::<_, bool>(0);
    if !has_schema_migrations {
        return Ok(MigrationState {
            applied: Vec::new(),
            pending: MIGRATIONS.iter().map(|migration| migration.name).collect(),
            total: MIGRATIONS.len(),
        });
    }

    let applied = client
        .query(
            "SELECT version, name FROM app.schema_migrations ORDER BY version;",
            &[],
        )
        .map_err(|source| DbError::Postgres {
            step: "reading app.schema_migrations".to_string(),
            source,
        })?
        .into_iter()
        .map(|row| {
            let version = row.get::<_, i32>(0);
            AppliedMigrationState {
                version: u16::try_from(version).unwrap_or_default(),
                name: row.get(1),
            }
        })
        .collect::<Vec<_>>();

    let applied_keys = applied
        .iter()
        .map(|migration| (migration.version, migration.name.as_str()))
        .collect::<HashSet<_>>();
    let pending = MIGRATIONS
        .iter()
        .filter(|migration| !applied_keys.contains(&(migration.version, migration.name)))
        .map(|migration| migration.name)
        .collect();

    Ok(MigrationState {
        applied,
        pending,
        total: MIGRATIONS.len(),
    })
}

pub fn redact_database_url(database_url: &str) -> String {
    if let Some(scheme_end) = database_url.find("://") {
        let credentials_start = scheme_end + 3;
        if let Some(relative_at) = database_url[credentials_start..].find('@') {
            let credentials_end = credentials_start + relative_at;
            if database_url[credentials_start..credentials_end].contains(':') {
                let mut redacted = String::with_capacity(database_url.len());
                redacted.push_str(&database_url[..credentials_start]);
                redacted.push_str("***");
                redacted.push_str(&database_url[credentials_end..]);
                return redacted;
            }
        }
    }

    database_url
        .split_whitespace()
        .map(|part| {
            if part.starts_with("password=") {
                "password=***"
            } else {
                part
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.map(str::trim).filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redact_database_url_hides_url_passwords() {
        assert_eq!(
            redact_database_url("postgres://user:secret@localhost/onecontext"),
            "postgres://***@localhost/onecontext"
        );
        assert_eq!(
            redact_database_url("host=localhost user=onecontext password=secret dbname=memory"),
            "host=localhost user=onecontext password=*** dbname=memory"
        );
    }

    #[test]
    fn migration_state_reports_manifest_total() {
        let state = MigrationState {
            applied: Vec::new(),
            pending: MIGRATIONS.iter().map(|migration| migration.name).collect(),
            total: MIGRATIONS.len(),
        };

        assert_eq!(state.total, MIGRATIONS.len());
        assert!(!state.is_current());
    }
}
