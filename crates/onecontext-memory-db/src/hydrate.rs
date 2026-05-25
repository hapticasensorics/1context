use std::collections::HashMap;
use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::edges::PerceptionEdge;
use crate::read_viewport::{
    parse_uuid_field, parse_uuid_vec, BlobDescriptorSummary, ReadQueryError, ReadResult,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HydrateObjectsRequest {
    pub user_id: String,
    pub object_ids: Vec<String>,
    pub include: HydrateIncludeRequest,
}

impl Default for HydrateObjectsRequest {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            object_ids: Vec::new(),
            include: HydrateIncludeRequest::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct HydrateIncludeRequest {
    pub payload: bool,
    pub blob_descriptor: bool,
    pub source_record: bool,
    pub edges: bool,
}

impl Default for HydrateIncludeRequest {
    fn default() -> Self {
        Self {
            payload: true,
            blob_descriptor: true,
            source_record: true,
            edges: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HydrateObjectsResponse {
    pub ok: bool,
    pub objects: Vec<HydratedObject>,
    pub missing_object_ids: Vec<String>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HydratedObject {
    pub object_id: String,
    pub event_start: String,
    pub event_end: String,
    pub lane_id: String,
    pub source_id: String,
    pub series_id: String,
    pub series_kind: String,
    pub series_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub series_display_name: Option<String>,
    pub series_default_lane_id: String,
    pub kind: String,
    pub role: String,
    pub privacy_class: String,
    pub body_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_value: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub number_value: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bool_value: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_record: Option<HydratedSourceRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blob: Option<BlobDescriptorSummary>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edges: Option<HydratedEdges>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HydratedSourceRecord {
    pub source_record_id: String,
    pub source_record_key: String,
    pub source_record_hash: String,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub seen_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct HydratedEdges {
    pub out: Vec<PerceptionEdge>,
    #[serde(rename = "in")]
    pub in_: Vec<PerceptionEdge>,
}

#[derive(Debug, Clone)]
struct SourceRecordHydration {
    source_record_id: Uuid,
    object_event_start: DateTime<Utc>,
    source_record_key: String,
    source_record_hash: String,
    first_seen_at: DateTime<Utc>,
    last_seen_at: DateTime<Utc>,
    seen_count: i64,
}

pub fn hydrate_objects(
    client: &mut Client,
    request: &HydrateObjectsRequest,
) -> ReadResult<HydrateObjectsResponse> {
    let started = Instant::now();
    let user_id = parse_uuid_field("user_id", &request.user_id)?;
    let object_ids = parse_uuid_vec("object_ids", &request.object_ids)?;
    if object_ids.is_empty() {
        return Err(ReadQueryError::InvalidRequest(
            "object_ids must contain at least one UUID".to_string(),
        ));
    }

    let source_rows = client.query(HYDRATE_SOURCE_RECORDS_SQL, &[&user_id, &object_ids])?;
    let mut source_records = HashMap::<Uuid, SourceRecordHydration>::new();
    for row in source_rows {
        let object_id: Uuid = row.get("object_id");
        source_records.insert(
            object_id,
            SourceRecordHydration {
                source_record_id: row.get("source_record_id"),
                object_event_start: row.get("object_event_start"),
                source_record_key: row.get("source_record_key"),
                source_record_hash: row.get("source_record_hash"),
                first_seen_at: row.get("first_seen_at"),
                last_seen_at: row.get("last_seen_at"),
                seen_count: row.get("seen_count"),
            },
        );
    }

    let mut by_id = HashMap::<String, HydratedObject>::new();
    if !source_records.is_empty() {
        let min_event_start = source_records
            .values()
            .map(|record| record.object_event_start)
            .min()
            .expect("source_records is not empty");
        let max_event_start = source_records
            .values()
            .map(|record| record.object_event_start)
            .max()
            .expect("source_records is not empty");
        let rows = client.query(
            HYDRATE_OBJECTS_SQL,
            &[
                &user_id,
                &min_event_start,
                &max_event_start,
                &object_ids,
                &request.include.payload,
            ],
        )?;
        for row in rows {
            let object_id: Uuid = row.get("object_id");
            let Some(source_record_data) = source_records.get(&object_id) else {
                continue;
            };
            let event_start: DateTime<Utc> = row.get("event_start");
            let event_end: DateTime<Utc> = row.get("event_end");
            let lane_id: Uuid = row.get("lane_id");
            let source_id: Uuid = row.get("source_id");
            let series_id: Uuid = row.get("series_id");
            let blob_id: Option<Uuid> = row.get("blob_id");
            let blob = if request.include.blob_descriptor {
                blob_id.map(|blob_id| BlobDescriptorSummary {
                    blob_id: blob_id.to_string(),
                    content_type: row.get("blob_content_type"),
                    byte_count: row.get("blob_byte_count"),
                    blob_state: row.get("blob_state"),
                    safe_uri: row.get("blob_safe_uri"),
                })
            } else {
                None
            };
            let source_record = request.include.source_record.then(|| HydratedSourceRecord {
                source_record_id: source_record_data.source_record_id.to_string(),
                source_record_key: source_record_data.source_record_key.clone(),
                source_record_hash: source_record_data.source_record_hash.clone(),
                first_seen_at: source_record_data
                    .first_seen_at
                    .to_rfc3339_opts(SecondsFormat::Micros, true),
                last_seen_at: source_record_data
                    .last_seen_at
                    .to_rfc3339_opts(SecondsFormat::Micros, true),
                seen_count: source_record_data.seen_count,
            });

            by_id.insert(
                object_id.to_string(),
                HydratedObject {
                    object_id: object_id.to_string(),
                    event_start: event_start.to_rfc3339_opts(SecondsFormat::Micros, true),
                    event_end: event_end.to_rfc3339_opts(SecondsFormat::Micros, true),
                    lane_id: lane_id.to_string(),
                    source_id: source_id.to_string(),
                    series_id: series_id.to_string(),
                    series_kind: row.get("series_kind"),
                    series_key: row.get("series_key"),
                    series_display_name: row.get("series_display_name"),
                    series_default_lane_id: row
                        .get::<_, Uuid>("series_default_lane_id")
                        .to_string(),
                    kind: row.get("kind"),
                    role: row.get("role"),
                    privacy_class: row.get("privacy_class"),
                    body_type: row.get("body_type"),
                    text_value: row.get("text_value"),
                    number_value: row.get("number_value"),
                    bool_value: row.get("bool_value"),
                    source_record,
                    payload: request.include.payload.then(|| row.get("payload")),
                    blob,
                    edges: request.include.edges.then(HydratedEdges::default),
                    metadata: row.get("metadata"),
                },
            );
        }
    }

    if request.include.edges {
        let edges = query_hydration_edges(client, &user_id, &object_ids)?;
        for edge in edges {
            if let Some(object) = by_id.get_mut(&edge.from_object_id) {
                if let Some(edges) = object.edges.as_mut() {
                    edges.out.push(edge.clone());
                }
            }
            if let Some(object) = by_id.get_mut(&edge.to_object_id) {
                if let Some(edges) = object.edges.as_mut() {
                    edges.in_.push(edge.clone());
                }
            }
        }
    }

    let mut objects = Vec::with_capacity(request.object_ids.len());
    let mut missing_object_ids = Vec::new();
    for requested_id in &object_ids {
        let requested_id = requested_id.to_string();
        if let Some(object) = by_id.remove(&requested_id) {
            objects.push(object);
        } else {
            missing_object_ids.push(requested_id);
        }
    }

    Ok(HydrateObjectsResponse {
        ok: true,
        objects,
        missing_object_ids,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

fn query_hydration_edges(
    client: &mut Client,
    user_id: &Uuid,
    object_ids: &[Uuid],
) -> ReadResult<Vec<PerceptionEdge>> {
    let rows = client.query(HYDRATE_EDGES_SQL, &[user_id, &object_ids])?;
    Ok(rows
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
        .collect())
}

const HYDRATE_SOURCE_RECORDS_SQL: &str = r#"
SELECT
  sr.source_record_id,
  sr.object_id,
  sr.object_event_start,
  sr.series_id,
  sr.source_record_key,
  sr.source_record_hash,
  sr.first_seen_at,
  sr.last_seen_at,
  sr.seen_count::bigint AS seen_count
FROM perception.source_records sr
WHERE sr.user_id = $1
  AND sr.object_id = ANY($2::uuid[])
	"#;

const HYDRATE_OBJECTS_SQL: &str = r#"
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
  o.kind,
  o.role,
  o.privacy_class,
  o.body_type,
  o.text_value,
  o.number_value,
	  o.bool_value,
	  CASE WHEN $5::bool THEN o.payload ELSE NULL::jsonb END AS payload,
  o.metadata,
  o.blob_id,
  b.content_type AS blob_content_type,
  b.byte_count AS blob_byte_count,
  b.blob_state AS blob_state,
  b.safe_uri AS blob_safe_uri
FROM perception.objects o
JOIN perception.series ser
  ON ser.series_id = o.series_id
 AND ser.user_id = o.user_id
LEFT JOIN perception.blobs b
  ON b.blob_id = o.blob_id
 AND b.user_id = o.user_id
WHERE o.user_id = $1
  AND o.event_start >= $2::timestamptz
  AND o.event_start <= $3::timestamptz
  AND o.object_id = ANY($4::uuid[])
  AND o.valid_to IS NULL
	"#;

const HYDRATE_EDGES_SQL: &str = r#"
SELECT
  edge_id,
  from_object_id,
  from_object_event_start,
  to_object_id,
  to_object_event_start,
  edge_kind,
  confidence,
  metadata
FROM (
  SELECT
    edge_id,
    from_object_id,
    from_object_event_start,
    to_object_id,
    to_object_event_start,
    edge_kind,
    confidence,
    metadata,
    created_at
  FROM perception.object_edges
  WHERE user_id = $1
    AND from_object_id = ANY($2::uuid[])

  UNION ALL

  SELECT
    edge_id,
    from_object_id,
    from_object_event_start,
    to_object_id,
    to_object_event_start,
    edge_kind,
    confidence,
    metadata,
    created_at
  FROM perception.object_edges
  WHERE user_id = $1
    AND to_object_id = ANY($2::uuid[])
    AND NOT (from_object_id = ANY($2::uuid[]))
) hydrated_edges
ORDER BY created_at ASC, edge_id ASC
	"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hydrate_edges_serializes_in_key_as_in_compat_field() {
        let edges = HydratedEdges {
            out: Vec::new(),
            in_: Vec::new(),
        };
        let value = serde_json::to_value(edges).unwrap();
        assert!(value.get("in").is_some());
    }

    #[test]
    fn hydrate_missing_ids_use_canonical_parsed_uuid_order() {
        let request = HydrateObjectsRequest {
            user_id: "10000000-0000-0000-0000-000000000001".to_string(),
            object_ids: vec!["AAAAAAAA-0000-0000-0000-000000000001".to_string()],
            ..Default::default()
        };
        let object_ids = parse_uuid_vec("object_ids", &request.object_ids).unwrap();

        assert_eq!(
            object_ids[0].to_string(),
            "aaaaaaaa-0000-0000-0000-000000000001"
        );
    }

    #[test]
    fn hydrate_payload_projection_is_include_gated() {
        assert!(HYDRATE_OBJECTS_SQL
            .contains("CASE WHEN $5::bool THEN o.payload ELSE NULL::jsonb END AS payload"));
    }

    #[test]
    fn hydrate_preserves_series_through_source_record_join() {
        assert!(HYDRATE_SOURCE_RECORDS_SQL.contains("FROM perception.source_records sr"));
        assert!(HYDRATE_SOURCE_RECORDS_SQL.contains("sr.object_id = ANY($2::uuid[])"));
        assert!(HYDRATE_OBJECTS_SQL.contains("o.event_start >= $2::timestamptz"));
        assert!(HYDRATE_OBJECTS_SQL.contains("o.event_start <= $3::timestamptz"));
        assert!(HYDRATE_OBJECTS_SQL.contains("o.object_id = ANY($4::uuid[])"));
        assert!(HYDRATE_OBJECTS_SQL.contains("JOIN perception.series ser"));
        assert!(HYDRATE_OBJECTS_SQL.contains("ser.default_lane_id AS series_default_lane_id"));
        assert!(HYDRATE_OBJECTS_SQL.contains("o.body_type"));
        assert!(HYDRATE_OBJECTS_SQL.contains("o.text_value"));
        assert!(HYDRATE_OBJECTS_SQL.contains("o.number_value"));
        assert!(HYDRATE_OBJECTS_SQL.contains("o.bool_value"));
    }

    #[test]
    fn hydrate_edges_project_object_event_starts() {
        assert!(HYDRATE_EDGES_SQL.contains("from_object_event_start"));
        assert!(HYDRATE_EDGES_SQL.contains("to_object_event_start"));
    }
}
