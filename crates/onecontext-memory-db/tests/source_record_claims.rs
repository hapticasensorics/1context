use chrono::{DateTime, Duration, SecondsFormat, Utc};
use onecontext_memory_db::{
    bootstrap_current_schema_with_client,
    write_objects::{
        canonical_source_hash, write_objects_with_client, PerceptionObjectInput,
        WriteObjectsRequest, WriteObjectsResponse,
    },
};
use postgres::{Client, NoTls};
use serde_json::json;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[test]
fn live_perception_source_record_claims_handle_duplicates_conflicts_and_retries() {
    let Some(database_url) = live_database_url() else {
        eprintln!("skipping live perception claim test; set ONECONTEXT_MEMORY_DB_TEST_URL");
        return;
    };

    let mut client = Client::connect(&database_url, NoTls).unwrap();
    bootstrap_current_schema_with_client(&mut client).unwrap();

    let run_seed = run_seed();
    let user_id = uuid_for(run_seed, 1);
    let source_id = uuid_for(run_seed, 2);
    let lane_id = uuid_for(run_seed, 3);

    let first = perception_object(
        &user_id,
        &source_id,
        &lane_id,
        "claims.duplicate",
        "source:duplicate",
        1,
        "first",
    );
    let written = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 20),
        &[first.clone(), first.clone()],
    )
    .unwrap();

    assert_eq!(written.receipts.len(), 2);
    assert_eq!(written.receipts[0].object_id, written.receipts[1].object_id);
    assert!(written.receipts[0].inserted);
    assert!(!written.receipts[1].inserted);
    assert_eq!(
        source_record_count(&mut client, &source_id, "source:duplicate"),
        1
    );
    assert_eq!(object_count(&mut client, &written.receipts[0].object_id), 1);

    let existing = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 21),
        std::slice::from_ref(&first),
    )
    .unwrap();
    assert_eq!(
        existing.receipts[0].object_id,
        written.receipts[0].object_id
    );
    assert!(!existing.receipts[0].inserted);

    let conflict = perception_object(
        &user_id,
        &source_id,
        &lane_id,
        "claims.duplicate",
        "source:duplicate",
        1,
        "changed",
    );
    let conflict_error = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 22),
        std::slice::from_ref(&conflict),
    )
    .unwrap_err();
    assert_eq!(conflict_error.code(), "SOURCE_IDENTITY_HASH_CONFLICT");
    let (conflict_count, conflict_metadata) =
        source_record_conflict_diagnostics(&mut client, &source_id, "source:duplicate");
    assert_eq!(conflict_count, 1);
    assert_eq!(
        conflict_metadata["source_identity_conflict"]["source_record_key"],
        "source:duplicate"
    );
    assert!(conflict_metadata["source_identity_conflict"]["existing_hash"].is_string());
    assert!(conflict_metadata["source_identity_conflict"]["competing_hash"].is_string());

    let partial_a = perception_object(
        &user_id,
        &source_id,
        &lane_id,
        "claims.partial",
        "partial:a",
        10,
        "a",
    );
    let partial_b = perception_object(
        &user_id,
        &source_id,
        &lane_id,
        "claims.partial",
        "partial:b",
        11,
        "b",
    );
    let partial_conflict = perception_object(
        &user_id,
        &source_id,
        &lane_id,
        "claims.duplicate",
        "source:duplicate",
        1,
        "partial-conflict",
    );
    let mut partial_request = write_objects_request(
        &user_id,
        &uuid_for(run_seed, 4),
        vec![partial_a.clone(), partial_b.clone(), partial_conflict],
    );
    partial_request.chunk_size = Some(2);

    let partial_error = write_objects_with_client(&mut client, &partial_request).unwrap_err();
    assert_eq!(partial_error.code(), "SOURCE_IDENTITY_HASH_CONFLICT");
    assert_eq!(source_record_count(&mut client, &source_id, "partial:a"), 1);
    assert_eq!(source_record_count(&mut client, &source_id, "partial:b"), 1);
    let (conflict_count, _) =
        source_record_conflict_diagnostics(&mut client, &source_id, "source:duplicate");
    assert_eq!(conflict_count, 2);

    let retry = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 23),
        &[partial_a.clone(), partial_b.clone()],
    )
    .unwrap();
    assert_eq!(retry.receipts.len(), 2);
    assert!(retry.receipts.iter().all(|written| !written.inserted));

    run_synthetic_benchmark(&database_url, run_seed + 10_000, benchmark_row_count());
}

#[test]
fn live_perception_write_objects_exact_and_mutable_container_fast_paths() {
    let Some(database_url) = live_database_url() else {
        eprintln!("skipping live perception fast path test; set ONECONTEXT_MEMORY_DB_TEST_URL");
        return;
    };

    let mut client = Client::connect(&database_url, NoTls).unwrap();
    bootstrap_current_schema_with_client(&mut client).unwrap();

    let run_seed = run_seed();
    let user_id = uuid_for(run_seed, 101);
    let source_id = uuid_for(run_seed, 102);
    let lane_id = uuid_for(run_seed, 103);

    let mut session = perception_object(
        &user_id,
        &source_id,
        &lane_id,
        "agent_session_ir.v1",
        "agent/codex/session-fast-path",
        100,
        "session-v1",
    );
    session.kind = "agent_session".to_string();
    session.series_kind = "agent_session".to_string();
    session.series_key = "agent-session:fast-path".to_string();
    session.payload = json!({"session": "fast-path", "version": 1});

    let first = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 120),
        std::slice::from_ref(&session),
    )
    .unwrap();
    assert!(first.receipts[0].inserted);
    let object_id = first.receipts[0].object_id.clone();
    let baseline_state = relation_state(
        &mut client,
        &user_id,
        &source_id,
        &lane_id,
        &session.series_key,
        &object_id,
    );

    let exact = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 121),
        std::slice::from_ref(&session),
    )
    .unwrap();
    assert_eq!(exact.receipts[0].object_id, object_id);
    assert!(!exact.receipts[0].inserted);
    assert_eq!(
        exact.receipts[0].dedupe_reason.as_deref(),
        Some("existing_source_record")
    );
    assert_eq!(
        source_record_seen_count(&mut client, &source_id, &session.source_record_key),
        2
    );
    assert_eq!(
        relation_state(
            &mut client,
            &user_id,
            &source_id,
            &lane_id,
            &session.series_key,
            &object_id,
        ),
        baseline_state
    );

    let mut changed_session = session.clone();
    changed_session.text_value = Some("session-v2".to_string());
    changed_session.display_text = Some("Mutable session container v2".to_string());
    changed_session.payload = json!({"session": "fast-path", "version": 2});
    let mutable = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 122),
        std::slice::from_ref(&changed_session),
    )
    .unwrap();
    assert_eq!(mutable.receipts[0].object_id, object_id);
    assert!(!mutable.receipts[0].inserted);
    assert_eq!(
        source_record_seen_count(&mut client, &source_id, &session.source_record_key),
        3
    );
    assert_eq!(
        source_record_hash(&mut client, &source_id, &session.source_record_key),
        canonical_source_hash(&changed_session).unwrap()
    );
    assert_eq!(
        object_text_value(&mut client, &object_id),
        Some("session-v2".to_string())
    );
    assert_eq!(
        relation_state(
            &mut client,
            &user_id,
            &source_id,
            &lane_id,
            &session.series_key,
            &object_id,
        ),
        baseline_state
    );
    let original_object_event_start =
        source_record_object_event_start(&mut client, &source_id, &session.source_record_key);

    let mut shifted_session = changed_session.clone();
    shifted_session.event_start = timestamp(95);
    shifted_session.event_end = timestamp(104);
    shifted_session.text_value = Some("session-v3".to_string());
    shifted_session.display_text = Some("Mutable session container v3".to_string());
    shifted_session.payload = json!({"session": "fast-path", "version": 3});
    let shifted = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 123),
        std::slice::from_ref(&shifted_session),
    )
    .unwrap();
    assert_eq!(shifted.receipts[0].object_id, object_id);
    assert!(!shifted.receipts[0].inserted);
    assert_eq!(
        source_record_seen_count(&mut client, &source_id, &session.source_record_key),
        4
    );
    assert_eq!(
        source_record_hash(&mut client, &source_id, &session.source_record_key),
        canonical_source_hash(&shifted_session).unwrap()
    );
    assert_eq!(
        source_record_object_event_start(&mut client, &source_id, &session.source_record_key),
        original_object_event_start
    );
    assert_eq!(
        object_text_value(&mut client, &object_id),
        Some("session-v3".to_string())
    );
    assert_eq!(object_count(&mut client, &object_id), 1);

    let mut backwards_session = shifted_session.clone();
    backwards_session.event_start = timestamp(50);
    backwards_session.event_end = timestamp(51);
    backwards_session.text_value = Some("session-v4".to_string());
    backwards_session.display_text = Some("Mutable session container v4".to_string());
    backwards_session.payload = json!({"session": "fast-path", "version": 4});
    let backwards = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 124),
        std::slice::from_ref(&backwards_session),
    )
    .unwrap();
    assert_eq!(backwards.receipts[0].object_id, object_id);
    assert!(!backwards.receipts[0].inserted);
    assert_eq!(
        source_record_object_event_start(&mut client, &source_id, &session.source_record_key),
        original_object_event_start
    );
    assert!(
        object_event_end(&mut client, &object_id) > original_object_event_start,
        "mutable update event_end must remain after the preserved object_event_start"
    );
    assert_eq!(
        object_text_value(&mut client, &object_id),
        Some("session-v4".to_string())
    );
    assert_eq!(object_count(&mut client, &object_id), 1);
    assert_eq!(
        relation_state(
            &mut client,
            &user_id,
            &source_id,
            &lane_id,
            &session.series_key,
            &object_id,
        ),
        baseline_state
    );

    let immutable = perception_object(
        &user_id,
        &source_id,
        &lane_id,
        "claims.immutable",
        "immutable:bypass",
        200,
        "immutable-v1",
    );
    write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 124),
        std::slice::from_ref(&immutable),
    )
    .unwrap();

    let mut renamed_mutable = immutable.clone();
    renamed_mutable.kind = "agent_session".to_string();
    renamed_mutable.text_value = Some("immutable-v2".to_string());
    renamed_mutable.payload = json!({"renamed": true});
    let error = write_perception_objects(
        &mut client,
        &user_id,
        &uuid_for(run_seed, 125),
        std::slice::from_ref(&renamed_mutable),
    )
    .unwrap_err();
    assert_eq!(error.code(), "SOURCE_IDENTITY_HASH_CONFLICT");
    let (conflict_count, _) =
        source_record_conflict_diagnostics(&mut client, &source_id, "immutable:bypass");
    assert_eq!(conflict_count, 1);
}

fn live_database_url() -> Option<String> {
    std::env::var("ONECONTEXT_MEMORY_DB_TEST_URL").ok()
}

fn benchmark_row_count() -> usize {
    std::env::var("ONECONTEXT_MEMORY_DB_BENCH_ROWS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|rows| *rows > 0)
        .unwrap_or(8)
}

fn run_seed() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

fn uuid_for(seed: u128, offset: u128) -> String {
    let low_bits = (seed.wrapping_add(offset)) & 0x0fff_ffff_ffff_ffff_ffff_ffff_ffff_ffff;
    Uuid::from_u128(0x1000_0000_0000_0000_0000_0000_0000_0000 | low_bits).to_string()
}

fn write_perception_objects(
    client: &mut Client,
    user_id: &str,
    write_id: &str,
    records: &[PerceptionObjectInput],
) -> Result<WriteObjectsResponse, onecontext_memory_db::write_objects::WriteObjectsError> {
    let request = write_objects_request(user_id, write_id, records.to_vec());
    write_objects_with_client(client, &request)
}

fn write_objects_request(
    user_id: &str,
    write_id: &str,
    records: Vec<PerceptionObjectInput>,
) -> WriteObjectsRequest {
    WriteObjectsRequest {
        user_id: user_id.to_string(),
        write_id: write_id.to_string(),
        atomicity: Some("chunk".to_string()),
        records,
        chunk_size: None,
    }
}

fn timestamp(offset_seconds: i64) -> String {
    let base = DateTime::<Utc>::from_timestamp(1_779_000_000, 0).unwrap();
    (base + Duration::seconds(offset_seconds)).to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn perception_object(
    _user_id: &str,
    source_id: &str,
    lane_id: &str,
    connector_key: &str,
    source_record_id: &str,
    sequence: i64,
    text: &str,
) -> PerceptionObjectInput {
    PerceptionObjectInput {
        client_record_id: None,
        source_id: source_id.to_string(),
        source_record_key: source_record_id.to_string(),
        lane_id: lane_id.to_string(),
        series_kind: "claim_test_thread".to_string(),
        series_key: "claim-test:thread:default".to_string(),
        series_display_name: Some("Claim test thread".to_string()),
        series_parent_key: None,
        modality: Some("text".to_string()),
        kind: "claim_test_message".to_string(),
        role: "observation".to_string(),
        privacy_class: "normal".to_string(),
        event_start: timestamp(sequence),
        event_end: timestamp(sequence + 1),
        time_semantics: "interval".to_string(),
        temporal_level: "event".to_string(),
        native_resolution_ns: None,
        stored_resolution_ns: None,
        indexed_resolution_ns: None,
        display_resolution_hint_ns: None,
        time_resolution_ns: None,
        time_uncertainty_ns: None,
        alignment_confidence: Some(1.0),
        alignment_method: Some("test_fixture".to_string()),
        materialization_policy: "index_events".to_string(),
        importance_score: Some(1.0),
        blob: None,
        body_type: Some("text".to_string()),
        text_value: Some(text.to_string()),
        number_value: None,
        bool_value: None,
        payload: json!({"sequence": sequence, "text": text}),
        display_title: Some(format!("Claim test {sequence}")),
        display_text: Some(format!("Synthetic perception object {sequence}: {text}")),
        edges: Vec::new(),
        source_start_ns: None,
        source_end_ns: None,
        source_sequence: Some(sequence),
        media_start_offset_ns: None,
        media_end_offset_ns: None,
        schema_name: Some(connector_key.to_string()),
        schema_version: Some(1),
        confidence: Some(1.0),
        metadata: json!({
            "source_uri": format!("memory://claim-test/{source_record_id}"),
        }),
    }
}

fn source_record_count(client: &mut Client, source_id: &str, source_record_key: &str) -> i64 {
    client
        .query_one(
            r#"
            SELECT count(*)::bigint
            FROM perception.source_records
            WHERE source_id::text = $1
              AND source_record_key = $2
            "#,
            &[&source_id, &source_record_key],
        )
        .unwrap()
        .get("count")
}

fn source_record_seen_count(client: &mut Client, source_id: &str, source_record_key: &str) -> i64 {
    client
        .query_one(
            r#"
            SELECT seen_count::bigint
            FROM perception.source_records
            WHERE source_id::text = $1
              AND source_record_key = $2
            "#,
            &[&source_id, &source_record_key],
        )
        .unwrap()
        .get("seen_count")
}

fn source_record_hash(client: &mut Client, source_id: &str, source_record_key: &str) -> String {
    client
        .query_one(
            r#"
            SELECT source_record_hash
            FROM perception.source_records
            WHERE source_id::text = $1
              AND source_record_key = $2
            "#,
            &[&source_id, &source_record_key],
        )
        .unwrap()
        .get("source_record_hash")
}

fn source_record_object_event_start(
    client: &mut Client,
    source_id: &str,
    source_record_key: &str,
) -> DateTime<Utc> {
    client
        .query_one(
            r#"
            SELECT object_event_start
            FROM perception.source_records
            WHERE source_id::text = $1
              AND source_record_key = $2
            "#,
            &[&source_id, &source_record_key],
        )
        .unwrap()
        .get("object_event_start")
}

fn object_count(client: &mut Client, object_id: &str) -> i64 {
    client
        .query_one(
            r#"
            SELECT count(*)::bigint
            FROM perception.objects
            WHERE object_id::text = $1
            "#,
            &[&object_id],
        )
        .unwrap()
        .get("count")
}

fn object_text_value(client: &mut Client, object_id: &str) -> Option<String> {
    client
        .query_one(
            r#"
            SELECT text_value
            FROM perception.objects
            WHERE object_id::text = $1
            "#,
            &[&object_id],
        )
        .unwrap()
        .get("text_value")
}

fn object_event_end(client: &mut Client, object_id: &str) -> DateTime<Utc> {
    client
        .query_one(
            r#"
            SELECT event_end
            FROM perception.objects
            WHERE object_id::text = $1
            "#,
            &[&object_id],
        )
        .unwrap()
        .get("event_end")
}

fn relation_state(
    client: &mut Client,
    user_id: &str,
    source_id: &str,
    lane_id: &str,
    series_key: &str,
    object_id: &str,
) -> serde_json::Value {
    client
        .query_one(
            r#"
            SELECT jsonb_build_object(
              'user_count', (
                SELECT count(*)::bigint
                FROM app.users
                WHERE user_id::text = $1
              ),
              'lane_count', (
                SELECT count(*)::bigint
                FROM perception.lanes
                WHERE lane_id::text = $3
              ),
              'source_count', (
                SELECT count(*)::bigint
                FROM perception.sources
                WHERE source_id::text = $2
              ),
              'source_updated_at', (
                SELECT updated_at::text
                FROM perception.sources
                WHERE source_id::text = $2
              ),
              'blob_count', (
                SELECT count(*)::bigint
                FROM perception.blobs
                WHERE user_id::text = $1
              ),
              'series_count', (
                SELECT count(*)::bigint
                FROM perception.series
                WHERE user_id::text = $1
                  AND source_id::text = $2
                  AND series_key = $4
              ),
              'series_updated_at', (
                SELECT updated_at::text
                FROM perception.series
                WHERE user_id::text = $1
                  AND source_id::text = $2
                  AND series_key = $4
              ),
              'object_count', (
                SELECT count(*)::bigint
                FROM perception.objects
                WHERE object_id::text = $5
              ),
              'edge_count', (
                SELECT count(*)::bigint
                FROM perception.object_edges
                WHERE from_object_id::text = $5
              )
            ) AS state
            "#,
            &[&user_id, &source_id, &lane_id, &series_key, &object_id],
        )
        .unwrap()
        .get("state")
}

fn source_record_conflict_diagnostics(
    client: &mut Client,
    source_id: &str,
    source_record_key: &str,
) -> (i64, serde_json::Value) {
    let row = client
        .query_one(
            r#"
            SELECT conflict_count::bigint AS conflict_count, metadata
            FROM perception.source_records
            WHERE source_id::text = $1
              AND source_record_key = $2
            "#,
            &[&source_id, &source_record_key],
        )
        .unwrap();
    (row.get("conflict_count"), row.get("metadata"))
}

fn run_synthetic_benchmark(database_url: &str, seed: u128, row_count: usize) {
    let user_id = uuid_for(seed, 1);
    let source_id = uuid_for(seed, 2);
    let lane_id = uuid_for(seed, 3);
    let records = (0..row_count)
        .map(|index| {
            perception_object(
                &user_id,
                &source_id,
                &lane_id,
                "claims.benchmark",
                &format!("bench:{seed}:{index}"),
                index as i64,
                "benchmark",
            )
        })
        .collect::<Vec<_>>();

    let mut client = Client::connect(database_url, NoTls).unwrap();
    let started = Instant::now();
    let inserted =
        write_perception_objects(&mut client, &user_id, &uuid_for(seed, 20), &records).unwrap();
    let insert_elapsed = started.elapsed();
    assert_eq!(inserted.receipts.len(), row_count);
    assert!(inserted.receipts.iter().all(|written| written.inserted));

    let started = Instant::now();
    let deduped =
        write_perception_objects(&mut client, &user_id, &uuid_for(seed, 21), &records).unwrap();
    let dedupe_elapsed = started.elapsed();
    assert_eq!(deduped.receipts.len(), row_count);
    assert!(deduped.receipts.iter().all(|written| !written.inserted));

    println!(
        "perception_write_synthetic_benchmark rows={row_count} cold_insert_ms={} exact_duplicate_ms={}",
        insert_elapsed.as_millis(),
        dedupe_elapsed.as_millis(),
    );
}
