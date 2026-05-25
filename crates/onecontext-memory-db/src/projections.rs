use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use postgres::{Client, Row};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::read_viewport::{
    clean_text_vec, parse_timestamp_field, parse_uuid_field, ReadQueryError, ReadResult,
    TimeRangeRequest,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ListTimelineProjectionsRequest {
    pub user_id: String,
    pub projection_kinds: Vec<String>,
    pub statuses: Vec<String>,
    pub limit: i64,
}

impl Default for ListTimelineProjectionsRequest {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            projection_kinds: Vec::new(),
            statuses: Vec::new(),
            limit: 500,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct QueryProjectionItemsRequest {
    pub user_id: String,
    pub projection_id: String,
    pub time: TimeRangeRequest,
    pub limit: i64,
    pub include_projection: bool,
}

impl Default for QueryProjectionItemsRequest {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            projection_id: String::new(),
            time: TimeRangeRequest {
                start: String::new(),
                end: String::new(),
            },
            limit: 5_000,
            include_projection: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ListTimelineProjectionsResponse {
    pub ok: bool,
    pub projections: Vec<TimelineProjection>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryProjectionItemsResponse {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection: Option<TimelineProjection>,
    pub items: Vec<TimelineProjectionItem>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineProjection {
    pub projection_id: String,
    pub projection_key: String,
    pub display_name: String,
    pub projection_kind: String,
    pub definition: Value,
    pub definition_hash: String,
    pub status: String,
    pub policy: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_min_event_start: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_max_event_end: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub built_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub invalidated_at: Option<String>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TimelineProjectionItem {
    pub projection_id: String,
    pub object_id: String,
    pub object_event_start: String,
    pub series_id: String,
    pub base_lane_id: String,
    pub display_lane_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_group_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projection_rule_key: Option<String>,
    pub event_start: String,
    pub event_end: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rank: Option<f32>,
    pub collapsed: bool,
    pub metadata: Value,
}

pub fn list_timeline_projections(
    client: &mut Client,
    request: &ListTimelineProjectionsRequest,
) -> ReadResult<ListTimelineProjectionsResponse> {
    let started = Instant::now();
    let user_id = parse_uuid_field("user_id", &request.user_id)?;
    let projection_kinds = clean_text_vec(&request.projection_kinds);
    let statuses = clean_text_vec(&request.statuses);
    let limit = request.limit.clamp(1, 1_000);

    let rows = client.query(
        LIST_TIMELINE_PROJECTIONS_SQL,
        &[&user_id, &projection_kinds, &statuses, &limit],
    )?;
    let projections = rows.iter().map(projection_from_row).collect();

    Ok(ListTimelineProjectionsResponse {
        ok: true,
        projections,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

pub fn query_projection_items(
    client: &mut Client,
    request: &QueryProjectionItemsRequest,
) -> ReadResult<QueryProjectionItemsResponse> {
    let started = Instant::now();
    let user_id = parse_uuid_field("user_id", &request.user_id)?;
    let projection_id = parse_uuid_field("projection_id", &request.projection_id)?;
    let start = parse_timestamp_field("time.start", &request.time.start)?;
    let end = parse_timestamp_field("time.end", &request.time.end)?;
    if end <= start {
        return Err(ReadQueryError::InvalidRequest(
            "time.end must be after time.start".to_string(),
        ));
    }
    let limit = request.limit.clamp(1, 10_000);

    let projection = if request.include_projection {
        client
            .query_opt(PROJECTION_BY_ID_SQL, &[&user_id, &projection_id])?
            .map(|row| projection_from_row(&row))
    } else {
        None
    };
    let rows = client.query(
        QUERY_PROJECTION_ITEMS_SQL,
        &[&user_id, &projection_id, &start, &end, &limit],
    )?;
    let items = rows.iter().map(projection_item_from_row).collect();

    Ok(QueryProjectionItemsResponse {
        ok: true,
        projection,
        items,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

fn projection_from_row(row: &Row) -> TimelineProjection {
    let projection_id: Uuid = row.get("projection_id");
    TimelineProjection {
        projection_id: projection_id.to_string(),
        projection_key: row.get("projection_key"),
        display_name: row.get("display_name"),
        projection_kind: row.get("projection_kind"),
        definition: row.get("definition"),
        definition_hash: row.get("definition_hash"),
        status: row.get("status"),
        policy: row.get("policy"),
        source_min_event_start: optional_timestamp(row.get("source_min_event_start")),
        source_max_event_end: optional_timestamp(row.get("source_max_event_end")),
        built_at: optional_timestamp(row.get("built_at")),
        invalidated_at: optional_timestamp(row.get("invalidated_at")),
        metadata: row.get("metadata"),
    }
}

fn projection_item_from_row(row: &Row) -> TimelineProjectionItem {
    let projection_id: Uuid = row.get("projection_id");
    let object_id: Uuid = row.get("object_id");
    let object_event_start: DateTime<Utc> = row.get("object_event_start");
    let series_id: Uuid = row.get("series_id");
    let base_lane_id: Uuid = row.get("base_lane_id");
    let event_start: DateTime<Utc> = row.get("event_start");
    let event_end: DateTime<Utc> = row.get("event_end");

    TimelineProjectionItem {
        projection_id: projection_id.to_string(),
        object_id: object_id.to_string(),
        object_event_start: format_timestamp(object_event_start),
        series_id: series_id.to_string(),
        base_lane_id: base_lane_id.to_string(),
        display_lane_key: row.get("display_lane_key"),
        display_group_key: row.get("display_group_key"),
        projection_rule_key: row.get("projection_rule_key"),
        event_start: format_timestamp(event_start),
        event_end: format_timestamp(event_end),
        rank: row.get("rank"),
        collapsed: row.get("collapsed"),
        metadata: row.get("metadata"),
    }
}

fn optional_timestamp(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(format_timestamp)
}

fn format_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339_opts(SecondsFormat::Micros, true)
}

const LIST_TIMELINE_PROJECTIONS_SQL: &str = r#"
SELECT
  projection_id,
  projection_key,
  display_name,
  projection_kind,
  definition,
  definition_hash,
  status,
  policy,
  source_min_event_start,
  source_max_event_end,
  built_at,
  invalidated_at,
  metadata
FROM perception.timeline_projections
WHERE user_id = $1
  AND ($2::text[] = '{}'::text[] OR projection_kind = ANY($2::text[]))
  AND ($3::text[] = '{}'::text[] OR status = ANY($3::text[]))
ORDER BY updated_at DESC, projection_id ASC
LIMIT $4
"#;

const PROJECTION_BY_ID_SQL: &str = r#"
SELECT
  projection_id,
  projection_key,
  display_name,
  projection_kind,
  definition,
  definition_hash,
  status,
  policy,
  source_min_event_start,
  source_max_event_end,
  built_at,
  invalidated_at,
  metadata
FROM perception.timeline_projections
WHERE user_id = $1
  AND projection_id = $2
"#;

const QUERY_PROJECTION_ITEMS_SQL: &str = r#"
SELECT
  i.projection_id,
  i.object_id,
  i.object_event_start,
  i.series_id,
  i.base_lane_id,
  i.display_lane_key,
  i.display_group_key,
  i.projection_rule_key,
  i.event_start,
  i.event_end,
  i.rank,
  i.collapsed,
  i.metadata
FROM perception.timeline_projection_items i
JOIN perception.objects o
  ON o.object_id = i.object_id
 AND o.event_start = i.object_event_start
 AND o.user_id = i.user_id
WHERE i.user_id = $1
  AND i.projection_id = $2
  AND i.event_start < $4
  AND i.event_end > $3
  AND i.event_range && tstzrange($3, $4, '[)')
  AND o.valid_to IS NULL
ORDER BY i.display_lane_key ASC, i.event_start ASC, i.object_id ASC
LIMIT $5
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projection_definition_query_exposes_rebuild_metadata() {
        assert!(LIST_TIMELINE_PROJECTIONS_SQL.contains("definition_hash"));
        assert!(LIST_TIMELINE_PROJECTIONS_SQL.contains("status"));
        assert!(LIST_TIMELINE_PROJECTIONS_SQL.contains("policy"));
        assert!(LIST_TIMELINE_PROJECTIONS_SQL.contains("source_min_event_start"));
        assert!(LIST_TIMELINE_PROJECTIONS_SQL.contains("source_max_event_end"));
    }

    #[test]
    fn projection_items_query_exposes_series_lane_and_rule_keys() {
        assert!(QUERY_PROJECTION_ITEMS_SQL.contains("i.series_id"));
        assert!(QUERY_PROJECTION_ITEMS_SQL.contains("i.base_lane_id"));
        assert!(QUERY_PROJECTION_ITEMS_SQL.contains("i.projection_rule_key"));
        assert!(QUERY_PROJECTION_ITEMS_SQL.contains("o.valid_to IS NULL"));
    }

    #[test]
    fn projection_item_time_range_must_be_ordered() {
        let request = QueryProjectionItemsRequest {
            user_id: "10000000-0000-0000-0000-000000000001".to_string(),
            projection_id: "20000000-0000-0000-0000-000000000001".to_string(),
            time: TimeRangeRequest {
                start: "2026-05-25T10:00:00Z".to_string(),
                end: "2026-05-25T09:00:00Z".to_string(),
            },
            ..Default::default()
        };

        let start = parse_timestamp_field("time.start", &request.time.start).unwrap();
        let end = parse_timestamp_field("time.end", &request.time.end).unwrap();

        assert!(end <= start);
    }
}
