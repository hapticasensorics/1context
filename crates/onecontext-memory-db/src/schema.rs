use std::fmt;

use postgres::{Client, GenericClient, NoTls};
use sha2::{Digest, Sha256};

pub const CURRENT_SCHEMA_SQL: &str = include_str!("../schema/current.sql");
pub const BASELINE_SCHEMA_VERSION: i64 = 1;
pub const BASELINE_SCHEMA_NAME: &str = "baseline_current_sql";

const SCHEMA_MIGRATIONS_TABLE_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS perception.schema_migrations (
  version BIGINT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  checksum TEXT NOT NULL,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT now(),
  app_version TEXT,
  destructive_bootstrap BOOLEAN NOT NULL DEFAULT false
);
"#;

pub const REQUIRED_CURRENT_RELATIONS: &[&str] = &[
    "app.users",
    "perception.schema_migrations",
    "perception.sources",
    "perception.lanes",
    "perception.series",
    "perception.blobs",
    "perception.source_records",
    "perception.objects",
    "perception.object_edges",
    "perception.object_density_1m",
    "perception.source_cursors",
    "perception.timeline_projections",
    "perception.timeline_projection_items",
    "search.object_embeddings",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSchemaBootstrapReport {
    pub created: bool,
    pub validated_relations: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSchemaStatus {
    pub valid: bool,
    pub present_relations: Vec<&'static str>,
    pub missing_relations: Vec<&'static str>,
}

#[derive(Debug)]
pub enum CurrentSchemaError {
    Incompatible {
        reason: String,
    },
    Postgres {
        step: String,
        source: postgres::Error,
    },
}

impl fmt::Display for CurrentSchemaError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Incompatible { reason } => {
                write!(formatter, "incompatible current schema: {reason}")
            }
            Self::Postgres { step, source } => {
                write!(formatter, "postgres failed while {step}: {source}")
            }
        }
    }
}

impl std::error::Error for CurrentSchemaError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Incompatible { .. } => None,
            Self::Postgres { source, .. } => Some(source),
        }
    }
}

pub fn current_schema_sql() -> &'static str {
    CURRENT_SCHEMA_SQL
}

pub fn current_schema_checksum() -> String {
    let digest = Sha256::digest(CURRENT_SCHEMA_SQL.as_bytes());
    let mut checksum = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut checksum, "{byte:02x}").expect("writing sha256 hex to string");
    }
    checksum
}

pub fn bootstrap_current_schema(
    database_url: &str,
) -> Result<CurrentSchemaBootstrapReport, CurrentSchemaError> {
    let mut client =
        Client::connect(database_url, NoTls).map_err(|source| CurrentSchemaError::Postgres {
            step: "connecting to database".to_string(),
            source,
        })?;
    bootstrap_current_schema_with_client(&mut client)
}

pub fn bootstrap_current_schema_with_client(
    client: &mut impl GenericClient,
) -> Result<CurrentSchemaBootstrapReport, CurrentSchemaError> {
    reject_deleted_schema_surfaces(client)?;
    let before = inspect_current_schema_with_client(client)?;

    let created = if before.present_relations.is_empty() {
        client.batch_execute(CURRENT_SCHEMA_SQL).map_err(|source| {
            CurrentSchemaError::Postgres {
                step: "creating current schema".to_string(),
                source,
            }
        })?;
        record_baseline_current_schema(client, true)?;
        true
    } else if missing_only_schema_migrations(&before) {
        create_schema_migrations_ledger_with_client(client)?;
        record_baseline_current_schema(client, false)?;
        false
    } else if before.valid {
        record_baseline_current_schema(client, false)?;
        false
    } else {
        return Err(CurrentSchemaError::Incompatible {
            reason: format!(
                "partial current schema; missing relations: {}",
                before.missing_relations.join(", ")
            ),
        });
    };

    validate_current_schema_with_client(client).map(|status| CurrentSchemaBootstrapReport {
        created,
        validated_relations: status.present_relations,
    })
}

fn missing_only_schema_migrations(status: &CurrentSchemaStatus) -> bool {
    status.missing_relations.as_slice() == ["perception.schema_migrations"]
        && status
            .present_relations
            .iter()
            .any(|relation| *relation == "perception.objects")
}

fn create_schema_migrations_ledger_with_client(
    client: &mut impl GenericClient,
) -> Result<(), CurrentSchemaError> {
    client
        .batch_execute(SCHEMA_MIGRATIONS_TABLE_SQL)
        .map_err(|source| CurrentSchemaError::Postgres {
            step: "creating perception schema migrations ledger".to_string(),
            source,
        })
}

fn record_baseline_current_schema(
    client: &mut impl GenericClient,
    destructive_bootstrap: bool,
) -> Result<(), CurrentSchemaError> {
    let expected_checksum = current_schema_checksum();
    if let Some(row) = client
        .query_opt(
            "SELECT checksum FROM perception.schema_migrations WHERE version = $1;",
            &[&BASELINE_SCHEMA_VERSION],
        )
        .map_err(|source| CurrentSchemaError::Postgres {
            step: "checking baseline schema checksum".to_string(),
            source,
        })?
    {
        let actual_checksum: String = row.get(0);
        if actual_checksum == expected_checksum {
            return Ok(());
        }
        return Err(CurrentSchemaError::Incompatible {
            reason: format!(
                "baseline schema checksum mismatch for {BASELINE_SCHEMA_NAME}: expected {expected_checksum}, found {actual_checksum}"
            ),
        });
    }

    let app_version = env!("CARGO_PKG_VERSION");
    client
        .execute(
            r#"
INSERT INTO perception.schema_migrations
  (version, name, checksum, app_version, destructive_bootstrap)
VALUES
  ($1, $2, $3, $4, $5);
"#,
            &[
                &BASELINE_SCHEMA_VERSION,
                &BASELINE_SCHEMA_NAME,
                &expected_checksum,
                &app_version,
                &destructive_bootstrap,
            ],
        )
        .map_err(|source| CurrentSchemaError::Postgres {
            step: "recording baseline schema checksum".to_string(),
            source,
        })?;
    Ok(())
}

pub fn validate_current_schema_with_client(
    client: &mut impl GenericClient,
) -> Result<CurrentSchemaStatus, CurrentSchemaError> {
    reject_deleted_schema_surfaces(client)?;
    let status = inspect_current_schema_with_client(client)?;
    if status.valid {
        Ok(status)
    } else {
        Err(CurrentSchemaError::Incompatible {
            reason: format!(
                "missing current schema relations: {}",
                status.missing_relations.join(", ")
            ),
        })
    }
}

fn inspect_current_schema_with_client(
    client: &mut impl GenericClient,
) -> Result<CurrentSchemaStatus, CurrentSchemaError> {
    let mut present_relations = Vec::new();
    let mut missing_relations = Vec::new();
    for relation in REQUIRED_CURRENT_RELATIONS {
        if regclass_exists(client, relation)? {
            present_relations.push(*relation);
        } else {
            missing_relations.push(*relation);
        }
    }
    Ok(CurrentSchemaStatus {
        valid: missing_relations.is_empty(),
        present_relations,
        missing_relations,
    })
}

fn reject_deleted_schema_surfaces(
    client: &mut impl GenericClient,
) -> Result<(), CurrentSchemaError> {
    if regclass_exists(client, "app.schema_migrations")? {
        return Err(CurrentSchemaError::Incompatible {
            reason: "found deleted migration ledger app.schema_migrations; reset the database"
                .to_string(),
        });
    }
    if namespace_exists(client, "capture")? {
        return Err(CurrentSchemaError::Incompatible {
            reason: "found deleted capture schema; reset the database".to_string(),
        });
    }
    Ok(())
}

fn regclass_exists(
    client: &mut impl GenericClient,
    relation: &str,
) -> Result<bool, CurrentSchemaError> {
    let row = client
        .query_one("SELECT to_regclass($1) IS NOT NULL;", &[&relation])
        .map_err(|source| CurrentSchemaError::Postgres {
            step: format!("checking relation {relation}"),
            source,
        })?;
    Ok(row.get(0))
}

fn namespace_exists(
    client: &mut impl GenericClient,
    namespace: &str,
) -> Result<bool, CurrentSchemaError> {
    let row = client
        .query_one("SELECT to_regnamespace($1) IS NOT NULL;", &[&namespace])
        .map_err(|source| CurrentSchemaError::Postgres {
            step: format!("checking schema {namespace}"),
            source,
        })?;
    Ok(row.get(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_schema_is_single_perception_bootstrap() {
        let sql = current_schema_sql();

        assert!(sql.contains("CREATE SCHEMA IF NOT EXISTS perception"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.schema_migrations"));
        assert!(sql.contains("version BIGINT PRIMARY KEY"));
        assert!(sql.contains("name TEXT NOT NULL UNIQUE"));
        assert!(sql.contains("checksum TEXT NOT NULL"));
        assert!(sql.contains("destructive_bootstrap BOOLEAN NOT NULL DEFAULT false"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS perception.objects"));
        assert!(sql.contains("CREATE MATERIALIZED VIEW IF NOT EXISTS perception.object_density_1m"));
        assert!(sql.contains("CREATE TABLE IF NOT EXISTS search.object_embeddings"));
        assert!(!sql.contains("app.schema_migrations"));
        assert!(!sql.contains("capture."));
        assert!(!sql.contains("captured_objects"));
    }

    #[test]
    fn required_relations_cover_current_read_write_surface() {
        for relation in [
            "perception.objects",
            "perception.schema_migrations",
            "perception.source_records",
            "perception.series",
            "perception.object_density_1m",
            "search.object_embeddings",
        ] {
            assert!(REQUIRED_CURRENT_RELATIONS.contains(&relation));
        }
    }

    #[test]
    fn current_schema_checksum_is_stable_sha256_shape() {
        let checksum = current_schema_checksum();

        assert_eq!(checksum.len(), 64);
        assert!(checksum
            .chars()
            .all(|character| character.is_ascii_hexdigit()));
    }
}
