use std::collections::BTreeMap;
use std::fmt;
use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use postgres::{Client, Row};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

#[derive(Debug)]
pub enum ReadQueryError {
    InvalidRequest(String),
    Database(postgres::Error),
}

impl fmt::Display for ReadQueryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(formatter, "invalid read request: {message}"),
            Self::Database(error) => write!(formatter, "postgres read error: {error}"),
        }
    }
}

impl std::error::Error for ReadQueryError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Database(error) => Some(error),
            Self::InvalidRequest(_) => None,
        }
    }
}

impl From<postgres::Error> for ReadQueryError {
    fn from(value: postgres::Error) -> Self {
        Self::Database(value)
    }
}

pub type ReadResult<T> = Result<T, ReadQueryError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeRangeRequest {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(default)]
pub struct ObjectFiltersRequest {
    pub lane_ids: Vec<String>,
    pub source_ids: Vec<String>,
    pub source_types: Vec<String>,
    pub source_keys: Vec<String>,
    pub kinds: Vec<String>,
    pub roles: Vec<String>,
    pub privacy_classes: Vec<String>,
    pub include_invalid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct PaginationRequest {
    pub limit: i64,
    pub cursor: Option<String>,
}

impl Default for PaginationRequest {
    fn default() -> Self {
        Self {
            limit: 500,
            cursor: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ViewportIncludeRequest {
    pub payload: bool,
    pub blob_descriptor: bool,
    pub source_record: bool,
    pub edges_count: bool,
}

impl Default for ViewportIncludeRequest {
    fn default() -> Self {
        Self {
            payload: false,
            blob_descriptor: true,
            source_record: true,
            edges_count: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct QueryViewportRequest {
    pub user_id: String,
    pub time: Option<TimeRangeRequest>,
    pub filters: ObjectFiltersRequest,
    pub pagination: PaginationRequest,
    pub include: ViewportIncludeRequest,
    pub explain: bool,
}

impl Default for QueryViewportRequest {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            time: None,
            filters: ObjectFiltersRequest::default(),
            pagination: PaginationRequest::default(),
            include: ViewportIncludeRequest::default(),
            explain: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryViewportResponse {
    pub ok: bool,
    pub objects: Vec<PerceptionObjectSummary>,
    pub next_cursor: Option<String>,
    pub explain: Option<ExplainPlanSummary>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionObjectSummary {
    pub object_id: String,
    pub event_start: String,
    pub event_end: String,
    pub lane_id: String,
    pub source_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_display_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_default_lane_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_record_id: Option<String>,
    pub kind: String,
    pub role: String,
    pub privacy_class: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_semantics: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temporal_level: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_resolution_ns: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_uncertainty_ns: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alignment_confidence: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub importance_score: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_text_preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobDescriptorSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge_counts: Option<BTreeMap<String, i64>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlobDescriptorSummary {
    pub blob_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub byte_count: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob_state: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub safe_uri: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExplainPlanSummary {
    pub format: String,
    pub summary: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw: Option<Value>,
}

#[derive(Debug, Clone)]
pub struct ParsedReadFilters {
    pub user_id: Uuid,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub lane_ids: Vec<Uuid>,
    pub source_ids: Vec<Uuid>,
    pub source_types: Vec<String>,
    pub source_keys: Vec<String>,
    pub kinds: Vec<String>,
    pub roles: Vec<String>,
    pub privacy_classes: Vec<String>,
    pub include_invalid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveSeriesForViewportRequest {
    pub user_id: String,
    pub lane_id: String,
    pub time: TimeRangeRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveSeriesForViewportResponse {
    pub ok: bool,
    pub series: Vec<ActiveViewportSeries>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ActiveViewportSeries {
    pub series_id: String,
    pub series_kind: String,
    pub series_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_display_name: Option<String>,
    pub series_default_lane_id: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub object_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KeysetCursor {
    pub event_start: String,
    pub lane_id: String,
    pub object_id: String,
}

#[derive(Debug, Clone)]
struct ParsedKeysetCursor {
    event_start: DateTime<Utc>,
    lane_id: Uuid,
    object_id: Uuid,
}

pub fn query_viewport(
    client: &mut Client,
    request: &QueryViewportRequest,
) -> ReadResult<QueryViewportResponse> {
    let started = Instant::now();
    let parsed = parse_viewport_request(request)?;
    let cursor = parse_cursor(request.pagination.cursor.as_deref())?;
    let limit = request.pagination.limit.clamp(1, 5_000);
    let fetch_limit = limit + 1;
    let cursor_event_start = cursor.as_ref().map(|cursor| cursor.event_start);
    let cursor_lane_id = cursor.as_ref().map(|cursor| cursor.lane_id);
    let cursor_object_id = cursor.as_ref().map(|cursor| cursor.object_id);

    let rows = client.query(
        VIEWPORT_SQL,
        &[
            &parsed.user_id,
            &parsed.start,
            &parsed.end,
            &parsed.lane_ids,
            &parsed.source_ids,
            &parsed.source_types,
            &parsed.source_keys,
            &parsed.kinds,
            &parsed.roles,
            &parsed.privacy_classes,
            &parsed.include_invalid,
            &cursor_event_start,
            &cursor_lane_id,
            &cursor_object_id,
            &fetch_limit,
            &request.include.payload,
            &request.include.blob_descriptor,
            &request.include.edges_count,
        ],
    )?;

    let has_next = rows.len() as i64 > limit;
    let objects = rows
        .iter()
        .take(limit as usize)
        .map(|row| object_summary_from_row(row, &request.include, None))
        .collect::<ReadResult<Vec<_>>>()?;
    let next_cursor = if has_next {
        objects
            .last()
            .map(|object| encode_cursor(&object.event_start, &object.lane_id, &object.object_id))
            .transpose()?
    } else {
        None
    };
    let explain = if request.explain {
        Some(explain_viewport(client, request, false)?)
    } else {
        None
    };

    Ok(QueryViewportResponse {
        ok: true,
        objects,
        next_cursor,
        explain,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

pub fn explain_viewport(
    client: &mut Client,
    request: &QueryViewportRequest,
    include_raw: bool,
) -> ReadResult<ExplainPlanSummary> {
    let parsed = parse_viewport_request(request)?;
    let cursor = parse_cursor(request.pagination.cursor.as_deref())?;
    let limit = request.pagination.limit.clamp(1, 5_000);
    let cursor_event_start = cursor.as_ref().map(|cursor| cursor.event_start);
    let cursor_lane_id = cursor.as_ref().map(|cursor| cursor.lane_id);
    let cursor_object_id = cursor.as_ref().map(|cursor| cursor.object_id);
    let explain_sql = format!("EXPLAIN (FORMAT JSON) {VIEWPORT_SELECT_SQL}");
    let row = client.query_one(
        &explain_sql,
        &[
            &parsed.user_id,
            &parsed.start,
            &parsed.end,
            &parsed.lane_ids,
            &parsed.source_ids,
            &parsed.source_types,
            &parsed.source_keys,
            &parsed.kinds,
            &parsed.roles,
            &parsed.privacy_classes,
            &parsed.include_invalid,
            &cursor_event_start,
            &cursor_lane_id,
            &cursor_object_id,
            &limit,
            &request.include.payload,
            &request.include.blob_descriptor,
            &request.include.edges_count,
        ],
    )?;
    let raw: Value = row.get(0);
    Ok(summarize_explain_plan(
        "viewport_overlap",
        raw,
        include_raw,
        vec![
            "objects_user_live_time_idx",
            "objects_lane_live_time_idx",
            "objects_source_live_time_idx",
        ],
    ))
}

pub fn active_series_for_viewport(
    client: &mut Client,
    request: &ActiveSeriesForViewportRequest,
) -> ReadResult<ActiveSeriesForViewportResponse> {
    let started = Instant::now();
    let user_id = parse_uuid_field("user_id", &request.user_id)?;
    let lane_id = parse_uuid_field("lane_id", &request.lane_id)?;
    let start = parse_timestamp_field("time.start", &request.time.start)?;
    let end = parse_timestamp_field("time.end", &request.time.end)?;

    let rows = client.query(
        ACTIVE_SERIES_FOR_VIEWPORT_SQL,
        &[&user_id, &lane_id, &start, &end],
    )?;
    let series = rows
        .into_iter()
        .map(|row| {
            let series_id: Uuid = row.get("series_id");
            let first_seen_at: DateTime<Utc> = row.get("first_seen_at");
            let last_seen_at: DateTime<Utc> = row.get("last_seen_at");
            ActiveViewportSeries {
                series_id: series_id.to_string(),
                series_kind: row.get("series_kind"),
                series_key: row.get("series_key"),
                series_display_name: row.get("series_display_name"),
                series_default_lane_id: row.get::<_, Uuid>("series_default_lane_id").to_string(),
                first_seen_at: first_seen_at.to_rfc3339_opts(SecondsFormat::Micros, true),
                last_seen_at: last_seen_at.to_rfc3339_opts(SecondsFormat::Micros, true),
                object_count: row.get("object_count"),
            }
        })
        .collect();

    Ok(ActiveSeriesForViewportResponse {
        ok: true,
        series,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

pub fn parse_viewport_request(request: &QueryViewportRequest) -> ReadResult<ParsedReadFilters> {
    let time = request.time.as_ref().ok_or_else(|| {
        ReadQueryError::InvalidRequest("time.start and time.end are required".into())
    })?;
    Ok(ParsedReadFilters {
        user_id: parse_uuid_field("user_id", &request.user_id)?,
        start: Some(parse_timestamp_field("time.start", &time.start)?),
        end: Some(parse_timestamp_field("time.end", &time.end)?),
        lane_ids: parse_uuid_vec("filters.lane_ids", &request.filters.lane_ids)?,
        source_ids: parse_uuid_vec("filters.source_ids", &request.filters.source_ids)?,
        source_types: clean_text_vec(&request.filters.source_types),
        source_keys: clean_text_vec(&request.filters.source_keys),
        kinds: clean_text_vec(&request.filters.kinds),
        roles: clean_text_vec(&request.filters.roles),
        privacy_classes: clean_text_vec(&request.filters.privacy_classes),
        include_invalid: request.filters.include_invalid,
    })
}

pub fn parse_uuid_field(field: &str, value: &str) -> ReadResult<Uuid> {
    Uuid::parse_str(value.trim()).map_err(|error| {
        ReadQueryError::InvalidRequest(format!("invalid {field} UUID {value:?}: {error}"))
    })
}

pub fn parse_optional_timestamp_field(
    field: &str,
    value: Option<&str>,
) -> ReadResult<Option<DateTime<Utc>>> {
    value
        .map(|value| parse_timestamp_field(field, value))
        .transpose()
}

pub fn parse_timestamp_field(field: &str, value: &str) -> ReadResult<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .map(|value| value.with_timezone(&Utc))
        .map_err(|error| {
            ReadQueryError::InvalidRequest(format!("invalid {field} timestamp {value:?}: {error}"))
        })
}

pub fn parse_uuid_vec(field: &str, values: &[String]) -> ReadResult<Vec<Uuid>> {
    values
        .iter()
        .filter(|value| !value.trim().is_empty())
        .map(|value| parse_uuid_field(field, value))
        .collect()
}

pub fn clean_text_vec(values: &[String]) -> Vec<String> {
    values
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
        .collect()
}

pub fn encode_cursor(event_start: &str, lane_id: &str, object_id: &str) -> ReadResult<String> {
    serde_json::to_string(&KeysetCursor {
        event_start: event_start.to_string(),
        lane_id: lane_id.to_string(),
        object_id: object_id.to_string(),
    })
    .map_err(|error| ReadQueryError::InvalidRequest(format!("failed to encode cursor: {error}")))
}

fn parse_cursor(value: Option<&str>) -> ReadResult<Option<ParsedKeysetCursor>> {
    let Some(value) = value.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };
    let cursor: KeysetCursor = serde_json::from_str(value).map_err(|error| {
        ReadQueryError::InvalidRequest(format!("invalid pagination.cursor value: {error}"))
    })?;
    Ok(Some(ParsedKeysetCursor {
        event_start: parse_timestamp_field("pagination.cursor.event_start", &cursor.event_start)?,
        lane_id: parse_uuid_field("pagination.cursor.lane_id", &cursor.lane_id)?,
        object_id: parse_uuid_field("pagination.cursor.object_id", &cursor.object_id)?,
    }))
}

pub fn object_summary_from_row(
    row: &Row,
    include: &ViewportIncludeRequest,
    rank: Option<f32>,
) -> ReadResult<PerceptionObjectSummary> {
    let object_id: Uuid = row.get("object_id");
    let event_start: DateTime<Utc> = row.get("event_start");
    let event_end: DateTime<Utc> = row.get("event_end");
    let lane_id: Uuid = row.get("lane_id");
    let source_id: Uuid = row.get("source_id");
    let series_id: Option<Uuid> = row.try_get("series_id").ok().flatten();
    let source_record_id: Option<Uuid> = if include.source_record {
        row.try_get("source_record_id").ok()
    } else {
        None
    };
    let blob_id: Option<Uuid> = row.try_get("blob_id").ok().flatten();
    let blob = if include.blob_descriptor {
        blob_id.map(|blob_id| BlobDescriptorSummary {
            blob_id: blob_id.to_string(),
            content_type: row.try_get("blob_content_type").ok().flatten(),
            byte_count: row.try_get("blob_byte_count").ok().flatten(),
            blob_state: row.try_get("blob_state").ok().flatten(),
            safe_uri: row.try_get("blob_safe_uri").ok().flatten(),
        })
    } else {
        None
    };
    let edge_counts = if include.edges_count {
        jsonb_counts_to_map(row.try_get("edge_counts").ok().unwrap_or(Value::Null))
    } else {
        None
    };
    let payload = if include.payload {
        row.try_get("payload").ok()
    } else {
        None
    };

    Ok(PerceptionObjectSummary {
        object_id: object_id.to_string(),
        event_start: event_start.to_rfc3339_opts(SecondsFormat::Micros, true),
        event_end: event_end.to_rfc3339_opts(SecondsFormat::Micros, true),
        lane_id: lane_id.to_string(),
        source_id: source_id.to_string(),
        source_type: row.try_get("source_type").ok().flatten(),
        source_key: row.try_get("source_key").ok().flatten(),
        source_display_name: row.try_get("source_display_name").ok().flatten(),
        series_id: series_id.map(|value| value.to_string()),
        series_kind: row.try_get("series_kind").ok().flatten(),
        series_key: row.try_get("series_key").ok().flatten(),
        series_display_name: row.try_get("series_display_name").ok().flatten(),
        series_default_lane_id: row
            .try_get::<_, Option<Uuid>>("series_default_lane_id")
            .ok()
            .flatten()
            .map(|value| value.to_string()),
        source_record_id: source_record_id.map(|value| value.to_string()),
        kind: row.get("kind"),
        role: row.get("role"),
        privacy_class: row.get("privacy_class"),
        body_type: row.try_get("body_type").ok().flatten(),
        text_value: row.try_get("text_value").ok().flatten(),
        number_value: row.try_get("number_value").ok().flatten(),
        bool_value: row.try_get("bool_value").ok().flatten(),
        time_semantics: row.try_get("time_semantics").ok(),
        temporal_level: row.try_get("temporal_level").ok(),
        time_resolution_ns: row.try_get("time_resolution_ns").ok().flatten(),
        time_uncertainty_ns: row.try_get("time_uncertainty_ns").ok().flatten(),
        alignment_confidence: row.try_get("alignment_confidence").ok().flatten(),
        importance_score: row.try_get("importance_score").ok().flatten(),
        display_title: row.try_get("display_title").ok().flatten(),
        display_text_preview: row.try_get("display_text_preview").ok().flatten(),
        blob,
        edge_counts,
        payload,
        rank,
    })
}

pub fn summarize_explain_plan(
    sql_kind: &str,
    raw: Value,
    include_raw: bool,
    expected_indexes: Vec<&'static str>,
) -> ExplainPlanSummary {
    let plan = raw
        .as_array()
        .and_then(|items| items.first())
        .and_then(|item| item.get("Plan"))
        .cloned()
        .unwrap_or(Value::Null);
    let mut indexes = Vec::new();
    collect_index_names(&plan, &mut indexes);
    indexes.sort();
    indexes.dedup();
    ExplainPlanSummary {
        format: "json".to_string(),
        summary: json!({
            "sql_kind": sql_kind,
            "estimated_rows": plan.get("Plan Rows").and_then(Value::as_i64),
            "estimated_total_cost": plan.get("Total Cost").and_then(Value::as_f64),
            "node_type": plan.get("Node Type").and_then(Value::as_str),
            "indexes_observed": indexes,
            "indexes_expected": expected_indexes,
        }),
        raw: include_raw.then_some(raw),
    }
}

fn collect_index_names(plan: &Value, indexes: &mut Vec<String>) {
    if let Some(index_name) = plan.get("Index Name").and_then(Value::as_str) {
        indexes.push(index_name.to_string());
    }
    if let Some(children) = plan.get("Plans").and_then(Value::as_array) {
        for child in children {
            collect_index_names(child, indexes);
        }
    }
}

fn jsonb_counts_to_map(value: Value) -> Option<BTreeMap<String, i64>> {
    let object = value.as_object()?;
    let mut counts = BTreeMap::new();
    for (key, value) in object {
        if let Some(count) = value.as_i64() {
            counts.insert(key.clone(), count);
        } else if let Some(count) = value.as_u64().and_then(|value| i64::try_from(value).ok()) {
            counts.insert(key.clone(), count);
        }
    }
    Some(counts)
}

pub fn cursor_is_after(
    event_start: &str,
    lane_id: &str,
    object_id: &str,
    cursor: &str,
) -> ReadResult<bool> {
    let cursor = parse_cursor(Some(cursor))?.expect("cursor should be present");
    let event_start = parse_timestamp_field("event_start", event_start)?;
    let lane_id = parse_uuid_field("lane_id", lane_id)?;
    let object_id = parse_uuid_field("object_id", object_id)?;
    Ok((event_start, lane_id, object_id) > (cursor.event_start, cursor.lane_id, cursor.object_id))
}

const VIEWPORT_SELECT_SQL: &str = r#"
WITH viewport_objects AS MATERIALIZED (
  SELECT
    o.object_id,
    o.user_id,
    o.event_start,
    o.event_end,
    o.lane_id,
	  o.source_id,
	  source.source_type,
	  source.source_key,
	  source.display_name AS source_display_name,
	  o.series_id,
    o.source_record_id,
    o.kind,
    o.role,
    o.privacy_class,
    o.body_type,
    NULL::text AS text_value,
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
    CASE WHEN $16::bool THEN o.payload ELSE NULL::jsonb END AS payload
	  FROM perception.objects o
	  LEFT JOIN perception.sources source
	    ON source.source_id = o.source_id
	   AND source.user_id = o.user_id
	  WHERE o.user_id = $1
    AND o.event_start >= $2::timestamptz
    AND o.event_start < $3::timestamptz
    AND ($4::uuid[] = '{}'::uuid[] OR o.lane_id = ANY($4::uuid[]))
    AND ($5::uuid[] = '{}'::uuid[] OR o.source_id = ANY($5::uuid[]))
    AND (
      ($6::text[] = '{}'::text[] AND $7::text[] = '{}'::text[])
      OR (
        ($6::text[] = '{}'::text[] OR source.source_type = ANY($6::text[]))
        AND ($7::text[] = '{}'::text[] OR source.source_key = ANY($7::text[]))
      )
    )
    AND ($8::text[] = '{}'::text[] OR o.kind = ANY($8::text[]))
    AND ($9::text[] = '{}'::text[] OR o.role = ANY($9::text[]))
    AND ($10::text[] = '{}'::text[] OR o.privacy_class = ANY($10::text[]))
    AND ($11::bool OR o.valid_to IS NULL)
    AND (
      $12::timestamptz IS NULL
      OR (o.event_start, o.lane_id, o.object_id) > ($12::timestamptz, $13::uuid, $14::uuid)
    )
  ORDER BY o.event_start ASC, o.lane_id ASC, o.object_id ASC
  LIMIT $15
)
SELECT
  o.object_id,
  o.event_start,
  o.event_end,
  o.lane_id,
	  o.source_id,
	  o.source_type,
	  o.source_key,
	  o.source_display_name,
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
  o.display_text_preview,
  o.blob_id,
  CASE WHEN $16::bool THEN o.payload ELSE NULL::jsonb END AS payload,
  b.content_type AS blob_content_type,
  b.byte_count AS blob_byte_count,
  b.blob_state AS blob_state,
  b.safe_uri AS blob_safe_uri,
  COALESCE(ec.edge_counts, '{}'::jsonb) AS edge_counts
FROM viewport_objects o
JOIN perception.series ser
  ON ser.series_id = o.series_id
 AND ser.user_id = o.user_id
LEFT JOIN perception.blobs b
  ON $17::bool
 AND b.blob_id = o.blob_id
 AND b.user_id = o.user_id
LEFT JOIN LATERAL (
	  SELECT jsonb_object_agg(edge_kind, edge_count) AS edge_counts
	  FROM (
	    SELECT edge_kind, count(*)::bigint AS edge_count
	    FROM (
	      SELECT e.edge_kind
	      FROM perception.object_edges e
	      WHERE e.user_id = o.user_id
	        AND e.from_object_id = o.object_id
	      UNION ALL
	      SELECT e.edge_kind
	      FROM perception.object_edges e
	      WHERE e.user_id = o.user_id
	        AND e.to_object_id = o.object_id
	    ) edge_hits
	    GROUP BY edge_kind
	  ) edge_counts
		) ec ON $18::bool
	ORDER BY o.event_start ASC, o.lane_id ASC, o.object_id ASC
		"#;

const VIEWPORT_SQL: &str = VIEWPORT_SELECT_SQL;

const ACTIVE_SERIES_FOR_VIEWPORT_SQL: &str = r#"
SELECT
  ser.series_id,
  ser.series_kind,
  ser.series_key,
  ser.display_name AS series_display_name,
  ser.default_lane_id AS series_default_lane_id,
  min(o.event_start) AS first_seen_at,
  max(o.event_end) AS last_seen_at,
  count(*)::bigint AS object_count
FROM perception.objects o
JOIN perception.series ser
  ON ser.series_id = o.series_id
 AND ser.user_id = o.user_id
WHERE o.user_id = $1
  AND o.lane_id = $2
  AND o.event_start < $4
  AND o.event_end > $3
  AND o.valid_to IS NULL
GROUP BY ser.series_id, ser.series_kind, ser.series_key, ser.display_name, ser.default_lane_id
ORDER BY first_seen_at ASC, ser.series_id ASC
"#;

#[cfg(test)]
mod tests {
    use super::*;

    const LANE_A: &str = "10000000-0000-0000-0000-000000000001";
    const LANE_B: &str = "10000000-0000-0000-0000-000000000002";
    const OBJECT_A: &str = "20000000-0000-0000-0000-000000000001";
    const OBJECT_B: &str = "20000000-0000-0000-0000-000000000002";

    #[test]
    fn keyset_cursor_round_trips_and_excludes_seen_tuple() {
        let cursor = encode_cursor("2026-05-25T09:10:00.000000Z", LANE_A, OBJECT_A).unwrap();

        assert!(
            !cursor_is_after("2026-05-25T09:10:00.000000Z", LANE_A, OBJECT_A, &cursor).unwrap()
        );
        assert!(cursor_is_after("2026-05-25T09:10:00.000001Z", LANE_A, OBJECT_A, &cursor).unwrap());
        assert!(cursor_is_after("2026-05-25T09:10:00.000000Z", LANE_B, OBJECT_A, &cursor).unwrap());
        assert!(cursor_is_after("2026-05-25T09:10:00.000000Z", LANE_A, OBJECT_B, &cursor).unwrap());
    }

    #[test]
    fn text_filters_drop_empty_values() {
        assert_eq!(
            clean_text_vec(&["codex".into(), " ".into(), "claude".into()]),
            vec!["codex".to_string(), "claude".to_string()]
        );
    }

    #[test]
    fn viewport_payload_projection_is_include_gated() {
        assert!(VIEWPORT_SELECT_SQL
            .contains("CASE WHEN $16::bool THEN o.payload ELSE NULL::jsonb END AS payload"));
        assert!(VIEWPORT_SELECT_SQL.contains("ON $17::bool"));
        assert!(VIEWPORT_SELECT_SQL.contains(") ec ON $18::bool"));
        assert!(VIEWPORT_SELECT_SQL.contains("LIMIT $15"));
        assert!(VIEWPORT_SELECT_SQL.contains("WITH viewport_objects AS MATERIALIZED"));
        assert!(VIEWPORT_SELECT_SQL.contains("o.event_start >= $2::timestamptz"));
        assert!(VIEWPORT_SELECT_SQL.contains("o.event_start < $3::timestamptz"));
    }

    #[test]
    fn viewport_supports_source_key_filters() {
        let request = QueryViewportRequest {
            user_id: "10000000-0000-0000-0000-000000000001".to_string(),
            time: Some(TimeRangeRequest {
                start: "2026-05-25T09:10:00Z".to_string(),
                end: "2026-05-25T09:11:00Z".to_string(),
            }),
            filters: ObjectFiltersRequest {
                source_keys: vec!["codex.local_sessions".to_string()],
                ..ObjectFiltersRequest::default()
            },
            ..Default::default()
        };

        let parsed = parse_viewport_request(&request).unwrap();

        assert_eq!(parsed.source_keys, vec!["codex.local_sessions".to_string()]);
        assert!(VIEWPORT_SELECT_SQL.contains("source.source_key = ANY($7::text[])"));
        assert!(VIEWPORT_SELECT_SQL.contains("source.source_type"));
        assert!(VIEWPORT_SELECT_SQL.contains("source_key"));
        assert!(!VIEWPORT_SELECT_SQL.contains("SELECT 1\n\t        SELECT 1"));
    }

    #[test]
    fn viewport_projects_series_and_body_fields() {
        assert!(VIEWPORT_SELECT_SQL.contains("JOIN perception.series ser"));
        assert!(VIEWPORT_SELECT_SQL.contains("o.series_id"));
        assert!(VIEWPORT_SELECT_SQL.contains("ser.series_kind"));
        assert!(VIEWPORT_SELECT_SQL.contains("ser.series_key"));
        assert!(VIEWPORT_SELECT_SQL.contains("ser.display_name AS series_display_name"));
        assert!(VIEWPORT_SELECT_SQL.contains("ser.default_lane_id AS series_default_lane_id"));
        assert!(VIEWPORT_SELECT_SQL.contains("o.body_type"));
        assert!(VIEWPORT_SELECT_SQL.contains("NULL::text AS text_value"));
        assert!(VIEWPORT_SELECT_SQL.contains("left(o.display_text, 512) AS display_text_preview"));
        assert!(VIEWPORT_SELECT_SQL.contains("o.number_value"));
        assert!(VIEWPORT_SELECT_SQL.contains("o.bool_value"));
    }

    #[test]
    fn active_series_query_is_viewport_scoped() {
        assert!(ACTIVE_SERIES_FOR_VIEWPORT_SQL.contains("o.lane_id = $2"));
        assert!(ACTIVE_SERIES_FOR_VIEWPORT_SQL.contains("o.event_start < $4"));
        assert!(ACTIVE_SERIES_FOR_VIEWPORT_SQL.contains("o.event_end > $3"));
        assert!(ACTIVE_SERIES_FOR_VIEWPORT_SQL
            .contains("GROUP BY ser.series_id, ser.series_kind, ser.series_key, ser.display_name, ser.default_lane_id"));
        assert!(ACTIVE_SERIES_FOR_VIEWPORT_SQL
            .contains("ser.default_lane_id AS series_default_lane_id"));
    }
}
