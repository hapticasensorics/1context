use std::fmt;

use postgres::{Client, NoTls};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqlMigration {
    pub version: u16,
    pub name: &'static str,
    pub sql: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationApplyReport {
    pub applied: Vec<&'static str>,
    pub skipped: Vec<&'static str>,
}

#[derive(Debug)]
pub enum MigrationRunnerError {
    Postgres {
        step: String,
        source: postgres::Error,
    },
}

impl fmt::Display for MigrationRunnerError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Postgres { step, source } => {
                write!(formatter, "postgres failed while {step}: {source}")
            }
        }
    }
}

impl std::error::Error for MigrationRunnerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Postgres { source, .. } => Some(source),
        }
    }
}

pub const MIGRATIONS: &[SqlMigration] = &[
    SqlMigration {
        version: 1,
        name: "001_extensions",
        sql: include_str!("../migrations/001_extensions.sql"),
    },
    SqlMigration {
        version: 2,
        name: "002_schemas",
        sql: include_str!("../migrations/002_schemas.sql"),
    },
    SqlMigration {
        version: 3,
        name: "003_app_users",
        sql: include_str!("../migrations/003_app_users.sql"),
    },
    SqlMigration {
        version: 4,
        name: "004_perception_support_tables",
        sql: include_str!("../migrations/004_perception_support_tables.sql"),
    },
    SqlMigration {
        version: 5,
        name: "005_perception_objects",
        sql: include_str!("../migrations/005_perception_objects.sql"),
    },
    SqlMigration {
        version: 6,
        name: "006_perception_indexes",
        sql: include_str!("../migrations/006_perception_indexes.sql"),
    },
    SqlMigration {
        version: 7,
        name: "007_perception_edges",
        sql: include_str!("../migrations/007_perception_edges.sql"),
    },
    SqlMigration {
        version: 8,
        name: "008_perception_density",
        sql: include_str!("../migrations/008_perception_density.sql"),
    },
    SqlMigration {
        version: 9,
        name: "009_perception_embeddings",
        sql: include_str!("../migrations/009_perception_embeddings.sql"),
    },
    SqlMigration {
        version: 10,
        name: "010_perception_source_cursors",
        sql: include_str!("../migrations/010_perception_source_cursors.sql"),
    },
    SqlMigration {
        version: 11,
        name: "011_perception_projections",
        sql: include_str!("../migrations/011_perception_projections.sql"),
    },
];

pub fn migration_by_name(name: &str) -> Option<&'static SqlMigration> {
    MIGRATIONS.iter().find(|migration| migration.name == name)
}

pub fn migration_sql_bundle() -> String {
    MIGRATIONS
        .iter()
        .map(|migration| {
            format!(
                "-- onecontext-memory-db migration {:03}: {}\n{}\n",
                migration.version, migration.name, migration.sql
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn migration_schema_table_sql() -> &'static str {
    r#"
CREATE SCHEMA IF NOT EXISTS app;
CREATE TABLE IF NOT EXISTS app.schema_migrations (
  version INT PRIMARY KEY,
  name TEXT NOT NULL UNIQUE,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
"#
}

pub fn migration_applied_sql(migration: &SqlMigration) -> String {
    format!(
        "SELECT EXISTS (SELECT 1 FROM app.schema_migrations WHERE version = {} AND name = {});",
        migration.version,
        sql_string_literal(migration.name)
    )
}

pub fn migration_mark_applied_sql(migration: &SqlMigration) -> String {
    format!(
        "INSERT INTO app.schema_migrations (version, name) VALUES ({}, {}) ON CONFLICT (version) DO NOTHING;",
        migration.version,
        sql_string_literal(migration.name)
    )
}

pub fn apply_bundled_migrations(
    database_url: &str,
) -> Result<MigrationApplyReport, MigrationRunnerError> {
    let mut client =
        Client::connect(database_url, NoTls).map_err(|source| MigrationRunnerError::Postgres {
            step: "connecting to database".to_string(),
            source,
        })?;
    apply_bundled_migrations_with_client(&mut client)
}

pub fn apply_bundled_migrations_with_client(
    client: &mut Client,
) -> Result<MigrationApplyReport, MigrationRunnerError> {
    client
        .batch_execute(migration_schema_table_sql())
        .map_err(|source| MigrationRunnerError::Postgres {
            step: "creating app.schema_migrations".to_string(),
            source,
        })?;

    let mut report = MigrationApplyReport {
        applied: Vec::new(),
        skipped: Vec::new(),
    };

    for migration in MIGRATIONS {
        let row = client
            .query_one(&migration_applied_sql(migration), &[])
            .map_err(|source| MigrationRunnerError::Postgres {
                step: format!("checking migration {}", migration.name),
                source,
            })?;
        if row.get::<_, bool>(0) {
            report.skipped.push(migration.name);
            continue;
        }

        client
            .batch_execute(migration.sql)
            .map_err(|source| MigrationRunnerError::Postgres {
                step: format!("applying migration {}", migration.name),
                source,
            })?;
        client
            .batch_execute(&migration_mark_applied_sql(migration))
            .map_err(|source| MigrationRunnerError::Postgres {
                step: format!("recording migration {}", migration.name),
                source,
            })?;
        report.applied.push(migration.name);
    }

    Ok(report)
}

fn sql_string_literal(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_ordered_and_nonempty() {
        assert!(!MIGRATIONS.is_empty());
        for (index, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(migration.version as usize, index + 1);
            assert!(migration
                .name
                .starts_with(&format!("{:03}_", migration.version)));
            assert!(!migration.sql.trim().is_empty());
        }
    }

    #[test]
    fn bundle_preserves_migration_boundaries() {
        let bundle = migration_sql_bundle();
        assert!(bundle.contains("001_extensions"));
        assert!(!bundle.contains("capture."));
        assert!(bundle.contains("perception.objects"));
        assert!(bundle.contains("perception.series"));
        assert!(bundle.contains("perception.source_records"));
        assert!(bundle.contains("perception.object_density_1m"));
        assert!(bundle.contains("perception.timeline_projections"));
    }

    #[test]
    fn migration_runner_uses_schema_migration_contract() {
        let migration = &MIGRATIONS[0];
        assert!(migration_schema_table_sql().contains("app.schema_migrations"));
        assert!(migration_applied_sql(migration).contains("WHERE version = 1"));
        assert!(migration_mark_applied_sql(migration).contains("001_extensions"));
    }

    #[test]
    fn sql_literals_escape_single_quotes() {
        assert_eq!(
            sql_string_literal("agent's migration"),
            "'agent''s migration'"
        );
    }
}
