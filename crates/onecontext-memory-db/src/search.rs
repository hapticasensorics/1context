use std::time::Instant;

use chrono::{DateTime, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::read_viewport::{
    clean_text_vec, object_summary_from_row, parse_optional_timestamp_field, parse_uuid_field,
    parse_uuid_vec, summarize_explain_plan, ExplainPlanSummary, ObjectFiltersRequest,
    PerceptionObjectSummary, ReadQueryError, ReadResult, TimeRangeRequest, ViewportIncludeRequest,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SearchTextRequest {
    pub user_id: String,
    pub query: String,
    pub time: Option<OptionalTimeRangeRequest>,
    pub filters: ObjectFiltersRequest,
    pub limit: i64,
    pub explain: bool,
}

impl Default for SearchTextRequest {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            query: String::new(),
            time: None,
            filters: ObjectFiltersRequest::default(),
            limit: 50,
            explain: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptionalTimeRangeRequest {
    pub start: Option<String>,
    pub end: Option<String>,
}

impl From<TimeRangeRequest> for OptionalTimeRangeRequest {
    fn from(value: TimeRangeRequest) -> Self {
        Self {
            start: Some(value.start),
            end: Some(value.end),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchTextResponse {
    pub ok: bool,
    pub objects: Vec<PerceptionObjectSummary>,
    pub explain: Option<ExplainPlanSummary>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SearchSemanticRequest {
    pub user_id: String,
    pub query: String,
    pub embedding_model: Option<String>,
    pub limit: i64,
}

impl Default for SearchSemanticRequest {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            query: String::new(),
            embedding_model: None,
            limit: 20,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SearchSemanticNotReadyResponse {
    pub ok: bool,
    pub error: SearchSemanticError,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchSemanticError {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone)]
struct ParsedSearchTextRequest {
    user_id: Uuid,
    query: String,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    lane_ids: Vec<Uuid>,
    source_ids: Vec<Uuid>,
    source_types: Vec<String>,
    kinds: Vec<String>,
    roles: Vec<String>,
    privacy_classes: Vec<String>,
    include_invalid: bool,
    limit: i64,
}

pub fn search_text(
    client: &mut Client,
    request: &SearchTextRequest,
) -> ReadResult<SearchTextResponse> {
    let started = Instant::now();
    let parsed = parse_search_text_request(request)?;
    let rows = client.query(
        SEARCH_TEXT_SQL,
        &[
            &parsed.user_id,
            &parsed.query,
            &parsed.start,
            &parsed.end,
            &parsed.lane_ids,
            &parsed.source_ids,
            &parsed.source_types,
            &parsed.kinds,
            &parsed.roles,
            &parsed.privacy_classes,
            &parsed.include_invalid,
            &parsed.limit,
        ],
    )?;
    let include = ViewportIncludeRequest {
        payload: false,
        blob_descriptor: true,
        source_record: true,
        edges_count: false,
    };
    let objects = rows
        .iter()
        .map(|row| {
            let rank: f32 = row.get("rank");
            object_summary_from_row(row, &include, Some(rank))
        })
        .collect::<ReadResult<Vec<_>>>()?;
    let explain = if request.explain {
        Some(explain_search_text(client, request, false)?)
    } else {
        None
    };

    Ok(SearchTextResponse {
        ok: true,
        objects,
        explain,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

pub fn search_semantic_not_ready(
    _request: &SearchSemanticRequest,
) -> SearchSemanticNotReadyResponse {
    SearchSemanticNotReadyResponse {
        ok: false,
        error: SearchSemanticError {
            code: "SEMANTIC_INDEX_NOT_READY".to_string(),
            message: "Semantic search is available in the protocol but no embeddings have been populated.".to_string(),
        },
    }
}

pub fn explain_search_text(
    client: &mut Client,
    request: &SearchTextRequest,
    include_raw: bool,
) -> ReadResult<ExplainPlanSummary> {
    let parsed = parse_search_text_request(request)?;
    let explain_sql = format!("EXPLAIN (FORMAT JSON) {SEARCH_TEXT_SELECT_SQL}");
    let row = client.query_one(
        &explain_sql,
        &[
            &parsed.user_id,
            &parsed.query,
            &parsed.start,
            &parsed.end,
            &parsed.lane_ids,
            &parsed.source_ids,
            &parsed.source_types,
            &parsed.kinds,
            &parsed.roles,
            &parsed.privacy_classes,
            &parsed.include_invalid,
            &parsed.limit,
        ],
    )?;
    let raw: Value = row.get(0);
    Ok(summarize_explain_plan(
        "search_text",
        raw,
        include_raw,
        vec!["objects_search_vector_live_idx"],
    ))
}

fn parse_search_text_request(request: &SearchTextRequest) -> ReadResult<ParsedSearchTextRequest> {
    let query = request.query.trim().to_string();
    if query.is_empty() {
        return Err(ReadQueryError::InvalidRequest(
            "query must not be empty".to_string(),
        ));
    }
    let start = request
        .time
        .as_ref()
        .and_then(|time| time.start.as_deref())
        .filter(|value| !value.trim().is_empty());
    let end = request
        .time
        .as_ref()
        .and_then(|time| time.end.as_deref())
        .filter(|value| !value.trim().is_empty());
    Ok(ParsedSearchTextRequest {
        user_id: parse_uuid_field("user_id", &request.user_id)?,
        query,
        start: parse_optional_timestamp_field("time.start", start)?,
        end: parse_optional_timestamp_field("time.end", end)?,
        lane_ids: parse_uuid_vec("filters.lane_ids", &request.filters.lane_ids)?,
        source_ids: parse_uuid_vec("filters.source_ids", &request.filters.source_ids)?,
        source_types: clean_text_vec(&request.filters.source_types),
        kinds: clean_text_vec(&request.filters.kinds),
        roles: clean_text_vec(&request.filters.roles),
        privacy_classes: clean_text_vec(&request.filters.privacy_classes),
        include_invalid: request.filters.include_invalid,
        limit: request.limit.clamp(1, 500),
    })
}

pub fn explain_not_configured(target: &str) -> Value {
    json!({
        "ok": false,
        "target": target,
        "error": {
            "code": "DATABASE_UNCONFIGURED",
            "message": "memory.explain requires a configured Perception DB connection."
        }
    })
}

const SEARCH_TEXT_SELECT_SQL: &str = r#"
SELECT
  o.object_id,
  o.event_start,
  o.event_end,
  o.lane_id,
  o.source_id,
  o.series_id,
  ser.series_kind,
  ser.series_key,
  ser.display_name AS series_display_name,
  ser.default_lane_id AS series_default_lane_id,
  o.source_record_id,
  o.kind,
  o.role,
  o.privacy_class,
  o.body_type,
  o.text_value,
  o.number_value,
  o.bool_value,
  o.time_semantics,
  o.temporal_level,
  o.time_resolution_ns,
  o.time_uncertainty_ns,
  o.alignment_confidence,
  o.importance_score,
  o.display_title,
  left(o.display_text, 512) AS display_text_preview,
  o.blob_id,
  NULL::jsonb AS payload,
  b.content_type AS blob_content_type,
  b.byte_count AS blob_byte_count,
  b.blob_state AS blob_state,
  b.safe_uri AS blob_safe_uri,
  '{}'::jsonb AS edge_counts,
  ts_rank(o.search_vector, plainto_tsquery('simple', $2)) AS rank
FROM perception.objects o
JOIN perception.series ser
  ON ser.series_id = o.series_id
 AND ser.user_id = o.user_id
LEFT JOIN perception.sources s
  ON s.source_id = o.source_id
 AND s.user_id = o.user_id
LEFT JOIN perception.blobs b
  ON b.blob_id = o.blob_id
 AND b.user_id = o.user_id
WHERE o.user_id = $1
  AND ($11::bool OR o.valid_to IS NULL)
  AND o.search_vector @@ plainto_tsquery('simple', $2)
  AND ($3::timestamptz IS NULL OR o.event_end > $3::timestamptz)
  AND ($4::timestamptz IS NULL OR o.event_start < $4::timestamptz)
  AND ($5::uuid[] = '{}'::uuid[] OR o.lane_id = ANY($5::uuid[]))
  AND ($6::uuid[] = '{}'::uuid[] OR o.source_id = ANY($6::uuid[]))
  AND ($7::text[] = '{}'::text[] OR s.source_type = ANY($7::text[]))
  AND ($8::text[] = '{}'::text[] OR o.kind = ANY($8::text[]))
  AND ($9::text[] = '{}'::text[] OR o.role = ANY($9::text[]))
  AND ($10::text[] = '{}'::text[] OR o.privacy_class = ANY($10::text[]))
ORDER BY rank DESC, o.event_start DESC, o.object_id ASC
LIMIT $12
"#;

const SEARCH_TEXT_SQL: &str = SEARCH_TEXT_SELECT_SQL;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_search_is_a_structured_not_ready_stub() {
        let response = search_semantic_not_ready(&SearchSemanticRequest::default());

        assert!(!response.ok);
        assert_eq!(response.error.code, "SEMANTIC_INDEX_NOT_READY");
    }

    #[test]
    fn search_text_preserves_source_type_and_include_invalid_filters() {
        let request = SearchTextRequest {
            user_id: "10000000-0000-0000-0000-000000000001".to_string(),
            query: "timescale".to_string(),
            filters: ObjectFiltersRequest {
                source_types: vec!["codex".to_string()],
                include_invalid: true,
                ..ObjectFiltersRequest::default()
            },
            ..Default::default()
        };

        let parsed = parse_search_text_request(&request).unwrap();

        assert_eq!(parsed.source_types, vec!["codex".to_string()]);
        assert!(parsed.include_invalid);
        assert!(SEARCH_TEXT_SELECT_SQL.contains("s.source_type = ANY($7::text[])"));
        assert!(SEARCH_TEXT_SELECT_SQL.contains("($11::bool OR o.valid_to IS NULL)"));
        assert!(SEARCH_TEXT_SELECT_SQL.contains("JOIN perception.series ser"));
        assert!(SEARCH_TEXT_SELECT_SQL.contains("ser.default_lane_id AS series_default_lane_id"));
        assert!(SEARCH_TEXT_SELECT_SQL.contains("o.text_value"));
    }

    #[test]
    fn search_text_uses_half_open_interval_overlap() {
        assert!(SEARCH_TEXT_SELECT_SQL.contains("o.event_end > $3::timestamptz"));
        assert!(SEARCH_TEXT_SELECT_SQL.contains("o.event_start < $4::timestamptz"));
        assert!(!SEARCH_TEXT_SELECT_SQL.contains("o.event_start >= $3::timestamptz"));
    }
}
