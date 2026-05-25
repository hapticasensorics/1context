use postgres::GenericClient;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug)]
pub enum SourceCursorError {
    Database(postgres::Error),
    InvalidUuid { field: &'static str, value: String },
}

impl std::fmt::Display for SourceCursorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Database(error) => write!(formatter, "postgres cursor error: {error:?}"),
            Self::InvalidUuid { field, value } => {
                write!(formatter, "invalid {field} UUID {value:?}")
            }
        }
    }
}

impl std::error::Error for SourceCursorError {}

impl From<postgres::Error> for SourceCursorError {
    fn from(value: postgres::Error) -> Self {
        Self::Database(value)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CursorAdvanceMode {
    DbSuccess,
}

impl CursorAdvanceMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DbSuccess => "db_success",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceCursor {
    pub source_id: String,
    pub cursor_name: String,
    pub user_id: String,
    pub cursor_value: Value,
    #[serde(default)]
    pub advanced_by_write_id: Option<String>,
    pub advancement_mode: CursorAdvanceMode,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CursorAdvanceDecision {
    pub may_advance: bool,
    pub advancement_mode: Option<CursorAdvanceMode>,
}

pub fn cursor_advance_decision(db_write_succeeded: bool) -> CursorAdvanceDecision {
    if db_write_succeeded {
        return CursorAdvanceDecision {
            may_advance: true,
            advancement_mode: Some(CursorAdvanceMode::DbSuccess),
        };
    }
    CursorAdvanceDecision {
        may_advance: false,
        advancement_mode: None,
    }
}

pub fn load_source_cursor(
    client: &mut impl GenericClient,
    source_id: &str,
    cursor_name: &str,
) -> Result<Option<SourceCursor>, SourceCursorError> {
    let source_id_uuid = parse_uuid("source_id", source_id)?;
    let row = client.query_opt(
        r#"
        SELECT
          source_id::text AS source_id,
          cursor_name,
          user_id::text AS user_id,
          cursor_value,
          advanced_by_write_id::text AS advanced_by_write_id,
          advancement_mode,
          metadata
        FROM perception.source_cursors
        WHERE source_id = $1
          AND cursor_name = $2
        "#,
        &[&source_id_uuid, &cursor_name],
    )?;
    Ok(row.map(|row| SourceCursor {
        source_id: row.get("source_id"),
        cursor_name: row.get("cursor_name"),
        user_id: row.get("user_id"),
        cursor_value: row.get("cursor_value"),
        advanced_by_write_id: row.get("advanced_by_write_id"),
        advancement_mode: CursorAdvanceMode::DbSuccess,
        metadata: row.get("metadata"),
    }))
}

pub fn advance_source_cursor_after_db_success(
    client: &mut impl GenericClient,
    source_id: &str,
    cursor_name: &str,
    user_id: &str,
    cursor_value: &Value,
    write_id: &str,
    metadata: &Value,
) -> Result<(), SourceCursorError> {
    advance_source_cursor(
        client,
        source_id,
        cursor_name,
        user_id,
        cursor_value,
        write_id,
        CursorAdvanceMode::DbSuccess,
        metadata,
    )
}

fn advance_source_cursor(
    client: &mut impl GenericClient,
    source_id: &str,
    cursor_name: &str,
    user_id: &str,
    cursor_value: &Value,
    write_id: &str,
    mode: CursorAdvanceMode,
    metadata: &Value,
) -> Result<(), SourceCursorError> {
    let source_id = parse_uuid("source_id", source_id)?;
    let user_id = parse_uuid("user_id", user_id)?;
    let write_id = parse_uuid("write_id", write_id)?;
    client.execute(
        r#"
        INSERT INTO perception.source_cursors (
          source_id,
          cursor_name,
          user_id,
          cursor_value,
          advanced_by_write_id,
          advancement_mode,
          metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        ON CONFLICT (source_id, cursor_name) DO UPDATE
        SET user_id = EXCLUDED.user_id,
            cursor_value = EXCLUDED.cursor_value,
            advanced_by_write_id = EXCLUDED.advanced_by_write_id,
            advanced_at = now(),
            advancement_mode = EXCLUDED.advancement_mode,
            metadata = EXCLUDED.metadata
        "#,
        &[
            &source_id,
            &cursor_name,
            &user_id,
            cursor_value,
            &write_id,
            &mode.as_str(),
            metadata,
        ],
    )?;
    Ok(())
}

fn parse_uuid(field: &'static str, value: &str) -> Result<Uuid, SourceCursorError> {
    Uuid::parse_str(value).map_err(|_| SourceCursorError::InvalidUuid {
        field,
        value: value.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_success_allows_cursor_advance() {
        let decision = cursor_advance_decision(true);

        assert!(decision.may_advance);
        assert_eq!(
            decision.advancement_mode,
            Some(CursorAdvanceMode::DbSuccess)
        );
    }

    #[test]
    fn failed_db_blocks_cursor_advance() {
        let decision = cursor_advance_decision(false);

        assert!(!decision.may_advance);
        assert_eq!(decision.advancement_mode, None);
    }
}
