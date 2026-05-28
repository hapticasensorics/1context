use std::fmt;

use postgres::{Client, NoTls};

use crate::schema::{
    bootstrap_current_schema_with_client, validate_current_schema_with_client,
    CurrentSchemaBootstrapReport, CurrentSchemaError, CurrentSchemaStatus,
};

pub const DATABASE_URL_ENV: &str = "ONECONTEXT_MEMORY_DB_URL";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabaseUrl {
    pub url: String,
    pub source: String,
}

#[derive(Debug)]
pub enum DbError {
    MissingDatabaseUrl,
    Postgres {
        step: String,
        source: postgres::Error,
    },
    Schema(CurrentSchemaError),
}

impl fmt::Display for DbError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDatabaseUrl => write!(formatter, "missing database URL"),
            Self::Postgres { step, source } => {
                write!(formatter, "postgres failed while {step}: {source}")
            }
            Self::Schema(source) => write!(formatter, "{source}"),
        }
    }
}

impl std::error::Error for DbError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::MissingDatabaseUrl => None,
            Self::Postgres { source, .. } => Some(source),
            Self::Schema(source) => Some(source),
        }
    }
}

impl From<CurrentSchemaError> for DbError {
    fn from(value: CurrentSchemaError) -> Self {
        Self::Schema(value)
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

    pub fn bootstrap_current_schema(&mut self) -> Result<CurrentSchemaBootstrapReport, DbError> {
        bootstrap_current_schema_with_client(&mut self.client).map_err(DbError::from)
    }

    pub fn validate_current_schema(&mut self) -> Result<CurrentSchemaStatus, DbError> {
        validate_current_schema_with_client(&mut self.client).map_err(DbError::from)
    }
}

pub fn resolve_database_url(explicit_database_url: Option<&str>) -> Option<DatabaseUrl> {
    if let Some(url) = nonempty(explicit_database_url) {
        return Some(DatabaseUrl {
            url: url.to_string(),
            source: "explicit".to_string(),
        });
    }

    if let Ok(value) = std::env::var(DATABASE_URL_ENV) {
        if let Some(url) = nonempty(Some(&value)) {
            return Some(DatabaseUrl {
                url: url.to_string(),
                source: format!("env:{DATABASE_URL_ENV}"),
            });
        }
    }

    None
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
    fn resolve_database_url_prefers_explicit_url() {
        let resolved = resolve_database_url(Some("postgres://explicit")).unwrap();

        assert_eq!(resolved.url, "postgres://explicit");
        assert_eq!(resolved.source, "explicit");
    }
}
