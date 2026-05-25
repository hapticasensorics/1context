use std::collections::BTreeSet;
use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::read_viewport::{
    clean_text_vec, object_summary_from_row, parse_timestamp_field, parse_uuid_field,
    parse_uuid_vec, PerceptionObjectSummary, ReadQueryError, ReadResult, ViewportIncludeRequest,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct QueryEdgesRequest {
    pub user_id: String,
    pub object_ids: Vec<String>,
    pub direction: String,
    pub edge_kinds: Vec<String>,
    pub limit: i64,
    pub hydrate: bool,
}

impl Default for QueryEdgesRequest {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            object_ids: Vec::new(),
            direction: "both".to_string(),
            edge_kinds: Vec::new(),
            limit: 1_000,
            hydrate: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryEdgesResponse {
    pub ok: bool,
    pub edges: Vec<PerceptionEdge>,
    pub objects: Vec<PerceptionObjectSummary>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PerceptionEdge {
    pub edge_id: String,
    pub from_object_id: String,
    pub from_object_event_start: String,
    pub to_object_id: String,
    pub to_object_event_start: String,
    pub edge_kind: String,
    pub confidence: Option<f32>,
    pub metadata: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeDirection {
    Incoming,
    Outgoing,
    Both,
}

pub fn query_edges(
    client: &mut Client,
    request: &QueryEdgesRequest,
) -> ReadResult<QueryEdgesResponse> {
    let started = Instant::now();
    let user_id = parse_uuid_field("user_id", &request.user_id)?;
    let object_ids = parse_uuid_vec("object_ids", &request.object_ids)?;
    if object_ids.is_empty() {
        return Err(ReadQueryError::InvalidRequest(
            "object_ids must contain at least one UUID".to_string(),
        ));
    }
    let direction = parse_direction(&request.direction)?;
    let edge_kinds = clean_text_vec(&request.edge_kinds);
    let limit = request.limit.clamp(1, 10_000);
    let include_incoming = matches!(direction, EdgeDirection::Incoming | EdgeDirection::Both);
    let include_outgoing = matches!(direction, EdgeDirection::Outgoing | EdgeDirection::Both);

    let rows = client.query(
        EDGES_SQL,
        &[
            &user_id,
            &object_ids,
            &include_incoming,
            &include_outgoing,
            &edge_kinds,
            &limit,
        ],
    )?;
    let edges = rows
        .into_iter()
        .map(|row| {
            let edge_id: Uuid = row.get("edge_id");
            let from_object_id: Uuid = row.get("from_object_id");
            let from_object_event_start: DateTime<Utc> = row.get("from_object_event_start");
            let to_object_id: Uuid = row.get("to_object_id");
            let to_object_event_start: DateTime<Utc> = row.get("to_object_event_start");
            PerceptionEdge {
                edge_id: edge_id.to_string(),
                from_object_id: from_object_id.to_string(),
                from_object_event_start: from_object_event_start
                    .to_rfc3339_opts(SecondsFormat::Micros, true),
                to_object_id: to_object_id.to_string(),
                to_object_event_start: to_object_event_start
                    .to_rfc3339_opts(SecondsFormat::Micros, true),
                edge_kind: row.get("edge_kind"),
                confidence: row.get("confidence"),
                metadata: row.get("metadata"),
            }
        })
        .collect::<Vec<_>>();
    let objects = if request.hydrate {
        hydrate_edge_summaries(client, &user_id, &edges)?
    } else {
        Vec::new()
    };

    Ok(QueryEdgesResponse {
        ok: true,
        edges,
        objects,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

fn hydrate_edge_summaries(
    client: &mut Client,
    user_id: &Uuid,
    edges: &[PerceptionEdge],
) -> ReadResult<Vec<PerceptionObjectSummary>> {
    let object_keys = edges
        .iter()
        .flat_map(|edge| {
            [
                (&edge.from_object_id, &edge.from_object_event_start),
                (&edge.to_object_id, &edge.to_object_event_start),
            ]
        })
        .map(|(object_id, event_start)| {
            Ok((
                parse_uuid_field("edge.object_id", object_id)?,
                parse_timestamp_field("edge.object_event_start", event_start)?,
            ))
        })
        .collect::<ReadResult<BTreeSet<_>>>()?
        .into_iter()
        .collect::<Vec<_>>();
    if object_keys.is_empty() {
        return Ok(Vec::new());
    }
    let object_ids = object_keys
        .iter()
        .map(|(object_id, _)| *object_id)
        .collect::<Vec<_>>();
    let event_starts = object_keys
        .iter()
        .map(|(_, event_start)| *event_start)
        .collect::<Vec<_>>();
    let rows = client.query(
        HYDRATE_EDGE_SUMMARIES_SQL,
        &[user_id, &object_ids, &event_starts],
    )?;
    let include = ViewportIncludeRequest {
        payload: false,
        blob_descriptor: true,
        source_record: true,
        edges_count: false,
    };
    rows.iter()
        .map(|row| object_summary_from_row(row, &include, None))
        .collect()
}

fn parse_direction(value: &str) -> ReadResult<EdgeDirection> {
    match value.trim() {
        "" | "both" => Ok(EdgeDirection::Both),
        "incoming" | "in" => Ok(EdgeDirection::Incoming),
        "outgoing" | "out" => Ok(EdgeDirection::Outgoing),
        other => Err(ReadQueryError::InvalidRequest(format!(
            "unsupported edge direction {other:?}; expected incoming, outgoing, or both"
        ))),
    }
}

const EDGES_SQL: &str = r#"
SELECT
  edge_id,
  from_object_id,
  from_object_event_start,
  to_object_id,
  to_object_event_start,
  edge_kind,
  confidence,
  metadata
FROM perception.object_edges
WHERE user_id = $1
  AND (
    ($3::bool AND to_object_id = ANY($2::uuid[]))
    OR ($4::bool AND from_object_id = ANY($2::uuid[]))
  )
  AND ($5::text[] = '{}'::text[] OR edge_kind = ANY($5::text[]))
ORDER BY created_at ASC, edge_id ASC
LIMIT $6
"#;

const HYDRATE_EDGE_SUMMARIES_SQL: &str = r#"
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
  '{}'::jsonb AS edge_counts
FROM perception.objects o
JOIN perception.series ser
  ON ser.series_id = o.series_id
 AND ser.user_id = o.user_id
LEFT JOIN perception.blobs b
  ON b.blob_id = o.blob_id
 AND b.user_id = o.user_id
WHERE o.user_id = $1
  AND (o.object_id, o.event_start) IN (
    SELECT object_id, event_start
    FROM unnest($2::uuid[], $3::timestamptz[]) AS edge_object(object_id, event_start)
  )
  AND o.valid_to IS NULL
ORDER BY o.event_start ASC, o.lane_id ASC, o.object_id ASC
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_short_direction_aliases() {
        assert_eq!(parse_direction("in").unwrap(), EdgeDirection::Incoming);
        assert_eq!(parse_direction("out").unwrap(), EdgeDirection::Outgoing);
        assert_eq!(parse_direction("").unwrap(), EdgeDirection::Both);
    }

    #[test]
    fn hydrated_edge_summaries_use_edge_event_starts_without_source_record_lookup() {
        assert!(HYDRATE_EDGE_SUMMARIES_SQL.contains("FROM perception.objects o"));
        assert!(!HYDRATE_EDGE_SUMMARIES_SQL.contains("FROM perception.source_records sr"));
        assert!(HYDRATE_EDGE_SUMMARIES_SQL.contains("(o.object_id, o.event_start) IN"));
        assert!(HYDRATE_EDGE_SUMMARIES_SQL.contains("$3::timestamptz[]"));
        assert!(HYDRATE_EDGE_SUMMARIES_SQL.contains("JOIN perception.series ser"));
        assert!(HYDRATE_EDGE_SUMMARIES_SQL.contains("o.series_id"));
        assert!(
            HYDRATE_EDGE_SUMMARIES_SQL.contains("ser.default_lane_id AS series_default_lane_id")
        );
        assert!(HYDRATE_EDGE_SUMMARIES_SQL.contains("o.body_type"));
    }

    #[test]
    fn edge_query_projects_object_event_starts() {
        assert!(EDGES_SQL.contains("from_object_event_start"));
        assert!(EDGES_SQL.contains("to_object_event_start"));
    }
}
