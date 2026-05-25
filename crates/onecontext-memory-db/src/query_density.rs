use std::time::Instant;

use chrono::{DateTime, SecondsFormat, Utc};
use postgres::Client;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::read_viewport::{
    clean_text_vec, parse_optional_timestamp_field, parse_uuid_field, parse_uuid_vec,
    summarize_explain_plan, ExplainPlanSummary, ObjectFiltersRequest, ReadQueryError, ReadResult,
    TimeRangeRequest,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct QueryDensityRequest {
    pub user_id: String,
    pub time: Option<TimeRangeRequest>,
    pub bucket: String,
    pub filters: ObjectFiltersRequest,
    pub explain: bool,
}

impl Default for QueryDensityRequest {
    fn default() -> Self {
        Self {
            user_id: String::new(),
            time: None,
            bucket: "1m".to_string(),
            filters: ObjectFiltersRequest::default(),
            explain: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct QueryDensityResponse {
    pub ok: bool,
    pub bucket: String,
    pub buckets: Vec<DensityBucket>,
    pub explain: Option<ExplainPlanSummary>,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DensityBucket {
    pub bucket_start: String,
    pub lane_id: String,
    pub kind: String,
    pub role: String,
    pub privacy_class: String,
    pub object_count: i64,
    pub avg_importance: Option<f64>,
}

#[derive(Debug, Clone)]
struct ParsedDensityRequest {
    user_id: Uuid,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
    lane_ids: Vec<Uuid>,
    kinds: Vec<String>,
    roles: Vec<String>,
    privacy_classes: Vec<String>,
}

pub fn query_density(
    client: &mut Client,
    request: &QueryDensityRequest,
) -> ReadResult<QueryDensityResponse> {
    let started = Instant::now();
    let parsed = parse_density_request(request)?;
    let rows = client.query(
        DENSITY_SQL,
        &[
            &parsed.user_id,
            &parsed.start,
            &parsed.end,
            &parsed.lane_ids,
            &parsed.kinds,
            &parsed.roles,
            &parsed.privacy_classes,
        ],
    )?;
    let buckets = rows
        .into_iter()
        .map(|row| {
            let bucket_start: DateTime<Utc> = row.get("bucket_start");
            let lane_id: Uuid = row.get("lane_id");
            DensityBucket {
                bucket_start: bucket_start.to_rfc3339_opts(SecondsFormat::Micros, true),
                lane_id: lane_id.to_string(),
                kind: row.get("kind"),
                role: row.get("role"),
                privacy_class: row.get("privacy_class"),
                object_count: row.get("object_count"),
                avg_importance: row.get("avg_importance"),
            }
        })
        .collect();
    let explain = if request.explain {
        Some(explain_density(client, request, false)?)
    } else {
        None
    };

    Ok(QueryDensityResponse {
        ok: true,
        bucket: "1m".to_string(),
        buckets,
        explain,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

pub fn explain_density(
    client: &mut Client,
    request: &QueryDensityRequest,
    include_raw: bool,
) -> ReadResult<ExplainPlanSummary> {
    let parsed = parse_density_request(request)?;
    let explain_sql = format!("EXPLAIN (FORMAT JSON) {DENSITY_SELECT_SQL}");
    let row = client.query_one(
        &explain_sql,
        &[
            &parsed.user_id,
            &parsed.start,
            &parsed.end,
            &parsed.lane_ids,
            &parsed.kinds,
            &parsed.roles,
            &parsed.privacy_classes,
        ],
    )?;
    let raw: Value = row.get(0);
    Ok(summarize_explain_plan(
        "density_1m",
        raw,
        include_raw,
        vec!["perception.object_density_1m"],
    ))
}

fn parse_density_request(request: &QueryDensityRequest) -> ReadResult<ParsedDensityRequest> {
    if request.bucket.trim().is_empty() || request.bucket.trim() != "1m" {
        return Err(ReadQueryError::InvalidRequest(format!(
            "unsupported density bucket {:?}; V0 supports only 1m",
            request.bucket
        )));
    }
    if request.filters.include_invalid {
        return Err(ReadQueryError::InvalidRequest(
            "queryDensity does not support filters.include_invalid because object_density_1m contains only current objects"
                .to_string(),
        ));
    }
    if !request.filters.source_ids.is_empty() || !request.filters.source_types.is_empty() {
        return Err(ReadQueryError::InvalidRequest(
            "queryDensity uses coarse density buckets and does not support source filters"
                .to_string(),
        ));
    }
    let start = request
        .time
        .as_ref()
        .map(|time| time.start.as_str())
        .filter(|value| !value.trim().is_empty());
    let end = request
        .time
        .as_ref()
        .map(|time| time.end.as_str())
        .filter(|value| !value.trim().is_empty());

    Ok(ParsedDensityRequest {
        user_id: parse_uuid_field("user_id", &request.user_id)?,
        start: parse_optional_timestamp_field("time.start", start)?,
        end: parse_optional_timestamp_field("time.end", end)?,
        lane_ids: parse_uuid_vec("filters.lane_ids", &request.filters.lane_ids)?,
        kinds: clean_text_vec(&request.filters.kinds),
        roles: clean_text_vec(&request.filters.roles),
        privacy_classes: clean_text_vec(&request.filters.privacy_classes),
    })
}

const DENSITY_SELECT_SQL: &str = r#"
SELECT
  d.bucket_start,
  d.lane_id,
  d.kind,
  d.role,
  d.privacy_class,
  d.object_count::bigint AS object_count,
  d.avg_importance::double precision AS avg_importance
FROM perception.object_density_1m d
WHERE d.user_id = $1
  AND ($2::timestamptz IS NULL OR d.bucket_start >= $2::timestamptz)
  AND ($3::timestamptz IS NULL OR d.bucket_start < $3::timestamptz)
  AND ($4::uuid[] = '{}'::uuid[] OR d.lane_id = ANY($4::uuid[]))
  AND ($5::text[] = '{}'::text[] OR d.kind = ANY($5::text[]))
  AND ($6::text[] = '{}'::text[] OR d.role = ANY($6::text[]))
  AND ($7::text[] = '{}'::text[] OR d.privacy_class = ANY($7::text[]))
ORDER BY d.bucket_start ASC, d.lane_id ASC, d.kind ASC
"#;

const DENSITY_SQL: &str = DENSITY_SELECT_SQL;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_1m_bucket() {
        let request = QueryDensityRequest {
            user_id: "10000000-0000-0000-0000-000000000001".to_string(),
            bucket: "15m".to_string(),
            ..Default::default()
        };

        assert!(parse_density_request(&request).is_err());
    }

    #[test]
    fn density_rejects_source_filters_to_stay_coarse() {
        let request = QueryDensityRequest {
            user_id: "10000000-0000-0000-0000-000000000001".to_string(),
            filters: ObjectFiltersRequest {
                source_types: vec!["codex".to_string()],
                ..ObjectFiltersRequest::default()
            },
            ..Default::default()
        };

        let error = parse_density_request(&request).unwrap_err().to_string();

        assert!(error.contains("source filters"));
        assert!(!DENSITY_SELECT_SQL.contains("source_type"));
        assert!(DENSITY_SELECT_SQL.contains("d.kind = ANY($5::text[])"));
    }

    #[test]
    fn density_rejects_include_invalid_instead_of_ignoring_it() {
        let request = QueryDensityRequest {
            user_id: "10000000-0000-0000-0000-000000000001".to_string(),
            filters: ObjectFiltersRequest {
                include_invalid: true,
                ..ObjectFiltersRequest::default()
            },
            ..Default::default()
        };

        let error = parse_density_request(&request).unwrap_err().to_string();

        assert!(error.contains("include_invalid"));
    }

    #[test]
    fn density_stays_coarse_without_series_grouping() {
        assert!(!DENSITY_SELECT_SQL.contains("series_id"));
        assert!(DENSITY_SELECT_SQL.contains("d.lane_id"));
        assert!(!DENSITY_SELECT_SQL.contains("d.source_id"));
    }
}
