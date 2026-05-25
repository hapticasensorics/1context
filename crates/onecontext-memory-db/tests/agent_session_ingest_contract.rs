use chrono::{DateTime, Duration, SecondsFormat, Utc};
use onecontext_memory_db::{
    codex_agent_ingest::{
        compile_codex_rollout_jsonl, parse_codex_rollout_jsonl, reduce_codex_rollout_records,
    },
    emit_agent_session_objects,
    migrations::apply_bundled_migrations_with_client,
    write_objects::{
        plan_write_objects, write_objects_with_client, PerceptionObjectInput, WriteObjectsRequest,
    },
    AgentIngestProfile,
};
use postgres::{Client, NoTls};
use serde_json::{json, Value};
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[test]
fn codex_hot_memory_emits_less_than_compact_audit_without_tool_objects() {
    let hot = compile_and_emit(&codex_rollout_lines(), AgentIngestProfile::HotMemory);
    let compact = compile_and_emit(&codex_rollout_lines(), AgentIngestProfile::CompactAudit);

    assert!(
        hot.len() < compact.len(),
        "hot_memory should emit fewer perception objects than compact_audit"
    );
    assert_eq!(kind_count(&hot, "agent_tool_summary"), 0);
    assert_eq!(kind_count(&compact, "agent_tool_summary"), 2);
    assert_eq!(kind_count(&hot, "agent_session"), 1);
    assert_eq!(kind_count(&hot, "agent_turn"), 1);
    assert_eq!(kind_count(&hot, "agent_message"), 2);
    assert_eq!(kind_count(&hot, "agent_prompt_snapshot"), 1);
    assert!(hot
        .iter()
        .all(|record| record.source_id == AgentSourceIds::CODEX));
}

#[test]
fn emitted_codex_objects_plan_cleanly_for_the_fast_writer() {
    let records = compile_and_emit(&codex_rollout_lines(), AgentIngestProfile::CompactAudit);
    let request = write_request(records.clone(), 30);
    let plan = plan_write_objects(&request).unwrap();

    assert_eq!(plan.record_count, records.len());
    assert_eq!(plan.leader_count, records.len());
    assert_eq!(plan.same_batch_duplicate_count, 0);
    assert_eq!(plan.chunk_count, 1);
    assert!(plan.receipts.iter().all(|receipt| receipt.inserted));
    assert!(plan
        .receipts
        .iter()
        .zip(records.iter())
        .all(|(receipt, record)| receipt.source_record_key == record.source_record_key));
}

#[test]
fn emitted_codex_objects_are_series_aware_for_v2_writer() {
    let records = compile_and_emit(&codex_rollout_lines(), AgentIngestProfile::CompactAudit);

    assert!(!records.is_empty());
    assert!(records
        .iter()
        .all(|record| record.series_kind == "codex_session"));
    assert!(records
        .iter()
        .all(|record| record.series_key == "codex:session:session-contract"));
    assert!(records.iter().all(|record| record
        .series_display_name
        .as_deref()
        .is_some_and(|display_name| display_name.starts_with("Codex session"))));
    assert!(records
        .iter()
        .all(|record| record.series_parent_key.is_none()));
}

#[test]
fn prompt_snapshot_references_model_visible_item_ids_and_objects() {
    let records = compile_and_emit(&codex_rollout_lines(), AgentIngestProfile::CompactAudit);
    let snapshot = records
        .iter()
        .find(|record| record.kind == "agent_prompt_snapshot")
        .expect("prompt snapshot emitted");
    let input_item_ids = snapshot.payload["input_item_ids"].as_array().unwrap();
    let input_object_ids = snapshot.payload["input_object_ids"].as_array().unwrap();

    assert_eq!(snapshot.payload["prompt_text"], Value::Null);
    assert_eq!(snapshot.payload["input_item_count"], json!(1));
    assert_eq!(input_item_ids.len(), 1);
    assert_eq!(input_object_ids.len(), 1);
    assert_eq!(
        snapshot.payload["input_ref_materialization"]["policy"],
        "complete_edges"
    );
    assert_eq!(
        snapshot.payload["input_ref_materialization"]["explicit_prompt_input_edge_count"],
        json!(1)
    );
    assert!(snapshot.edges.iter().any(
        |edge| edge.edge_kind == "references" && edge.metadata["agent_edge"] == "prompt_input"
    ));
}

#[test]
fn live_codex_write_objects_retry_dedupes_and_reads_back() {
    let Some(database_url) = live_database_url() else {
        eprintln!(
            "skipping live agent session writeObjects contract; set ONECONTEXT_MEMORY_DB_TEST_URL"
        );
        return;
    };

    let records = compile_and_emit(&codex_rollout_lines(), AgentIngestProfile::HotMemory);
    let request = write_request(records, 40);

    let mut client = Client::connect(&database_url, NoTls).unwrap();
    apply_bundled_migrations_with_client(&mut client).unwrap();

    let started = Instant::now();
    let first = write_objects_with_client(&mut client, &request).unwrap();
    let insert_elapsed = started.elapsed();
    assert_eq!(first.record_count, request.records.len());
    assert_eq!(first.inserted_count, request.records.len());

    let started = Instant::now();
    let retry = write_objects_with_client(&mut client, &request).unwrap();
    let retry_elapsed = started.elapsed();
    assert_eq!(retry.record_count, request.records.len());
    assert_eq!(retry.inserted_count, 0);
    assert_eq!(retry.duplicate_count, request.records.len());
    assert!(retry.receipts.iter().all(|receipt| !receipt.inserted));

    let object_ids = first
        .receipts
        .iter()
        .map(|receipt| Uuid::parse_str(&receipt.object_id).unwrap())
        .collect::<Vec<_>>();
    let read_back = client
        .query_one(
            r#"
            SELECT count(*)::bigint
            FROM perception.objects
            WHERE object_id = ANY($1)
            "#,
            &[&object_ids],
        )
        .unwrap()
        .get::<_, i64>("count");
    assert_eq!(read_back as usize, request.records.len());

    let series_back = client
        .query_one(
            r#"
            SELECT
              count(DISTINCT objects.series_id)::bigint AS series_count,
              min(series.series_kind) AS series_kind,
              min(series.series_key) AS series_key,
              count(source_records.source_record_id)::bigint AS source_record_count
            FROM perception.objects objects
            JOIN perception.series series
              ON series.series_id = objects.series_id
            JOIN perception.source_records source_records
              ON source_records.object_id = objects.object_id
             AND source_records.series_id = objects.series_id
            WHERE objects.object_id = ANY($1)
            "#,
            &[&object_ids],
        )
        .unwrap();
    assert_eq!(series_back.get::<_, i64>("series_count"), 1);
    assert_eq!(series_back.get::<_, String>("series_kind"), "codex_session");
    assert_eq!(
        series_back.get::<_, String>("series_key"),
        "codex:session:session-contract"
    );
    assert_eq!(
        series_back.get::<_, i64>("source_record_count") as usize,
        request.records.len()
    );

    println!(
        "agent_session_live_write rows={} insert_ms={} retry_dedupe_ms={}",
        request.records.len(),
        insert_elapsed.as_millis(),
        retry_elapsed.as_millis()
    );

    run_mutable_container_live_benchmark(&mut client, run_seed() + 50_000);
}

#[test]
fn synthetic_agent_session_ingest_benchmark_reports_parse_reduce_emit_and_write_plan() {
    let event_count = synthetic_bench_event_count();
    let jsonl = lines_to_jsonl(&synthetic_codex_lines(event_count));

    let started = Instant::now();
    let parsed = parse_codex_rollout_jsonl("memory://synthetic-codex-bench", &jsonl).unwrap();
    let parse_elapsed = started.elapsed();

    let started = Instant::now();
    let reduced = reduce_codex_rollout_records(
        "memory://synthetic-codex-bench",
        &parsed,
        AgentIngestProfile::CompactAudit,
    );
    let reduce_elapsed = started.elapsed();

    let started = Instant::now();
    let emitted = emit_agent_session_objects(&reduced).unwrap();
    let emission_elapsed = started.elapsed();

    let started = Instant::now();
    let request = write_request(emitted.clone(), 50);
    let plan = plan_write_objects(&request).unwrap();
    let write_plan_elapsed = started.elapsed();

    assert_eq!(parsed.len(), event_count * 3 + 1);
    assert_eq!(kind_count(&emitted, "agent_message"), event_count);
    assert_eq!(kind_count(&emitted, "agent_tool_summary"), event_count * 2);
    assert_eq!(kind_count(&emitted, "agent_session"), 1);
    assert_eq!(plan.leader_count, emitted.len());

    println!(
        "agent_session_ingest_benchmark input_events={event_count} parse_ms={} reduce_ms={} emission_ms={} write_plan_ms={} emitted={} leaders={}",
        parse_elapsed.as_millis(),
        reduce_elapsed.as_millis(),
        emission_elapsed.as_millis(),
        write_plan_elapsed.as_millis(),
        emitted.len(),
        plan.leader_count
    );
}

fn compile_and_emit(lines: &[String], profile: AgentIngestProfile) -> Vec<PerceptionObjectInput> {
    let session = compile_codex_rollout_jsonl(
        "memory://codex-agent-session-contract.jsonl",
        &lines_to_jsonl(lines),
        profile,
    )
    .unwrap();
    emit_agent_session_objects(&session).unwrap()
}

fn write_request(records: Vec<PerceptionObjectInput>, seed_offset: u128) -> WriteObjectsRequest {
    WriteObjectsRequest {
        user_id: "00000000-0000-0000-0000-000000000001".to_string(),
        write_id: uuid_for(run_seed(), seed_offset),
        atomicity: None,
        records,
        chunk_size: None,
    }
}

fn kind_count(records: &[PerceptionObjectInput], kind: &str) -> usize {
    records.iter().filter(|record| record.kind == kind).count()
}

fn codex_rollout_lines() -> Vec<String> {
    codex_rollout_lines_for_session("session-contract")
}

fn codex_rollout_lines_for_session(session_id: &str) -> Vec<String> {
    vec![
        session_meta_line(session_id),
        response_message_line(1, "user", "Please run the contract."),
        turn_context_line(2, "turn-1"),
        function_call_line(3, "call-1", "cargo test -p onecontext-memory-db"),
        function_call_output_line(4, "call-1", "ok"),
        response_message_line(5, "assistant", "The contract passed."),
    ]
}

fn synthetic_codex_lines(event_count: usize) -> Vec<String> {
    let mut lines = vec![session_meta_line("session-bench")];
    for index in 0..event_count {
        let base = index * 3;
        lines.push(response_message_line(
            base + 1,
            "assistant",
            &format!("Synthetic answer {index}"),
        ));
        lines.push(function_call_line(
            base + 2,
            &format!("call-{index}"),
            &format!("echo {index}"),
        ));
        lines.push(function_call_output_line(
            base + 3,
            &format!("call-{index}"),
            "ok",
        ));
    }
    lines
}

fn session_meta_line(session_id: &str) -> String {
    json!({
        "timestamp": timestamp(0),
        "type": "session_meta",
        "payload": {
            "id": session_id,
            "cwd": "/tmp/onecontext-agent-contract"
        }
    })
    .to_string()
}

fn response_message_line(offset_seconds: usize, role: &str, text: &str) -> String {
    json!({
        "timestamp": timestamp(offset_seconds as i64),
        "type": "response_item",
        "payload": {
            "type": "message",
            "role": role,
            "content": [{"type": "output_text", "text": text}]
        }
    })
    .to_string()
}

fn turn_context_line(offset_seconds: usize, turn_id: &str) -> String {
    json!({
        "timestamp": timestamp(offset_seconds as i64),
        "type": "turn_context",
        "payload": {
            "turn_id": turn_id,
            "cwd": "/tmp/onecontext-agent-contract",
            "model": "gpt-5",
            "tools": [{"name": "shell"}],
            "user_instructions": "contract fixture"
        }
    })
    .to_string()
}

fn function_call_line(offset_seconds: usize, call_id: &str, command: &str) -> String {
    json!({
        "timestamp": timestamp(offset_seconds as i64),
        "type": "response_item",
        "payload": {
            "type": "function_call",
            "name": "exec_command",
            "call_id": call_id,
            "arguments": json!({
                "cmd": command,
                "workdir": "/tmp/onecontext-agent-contract"
            }).to_string()
        }
    })
    .to_string()
}

fn function_call_output_line(offset_seconds: usize, call_id: &str, output: &str) -> String {
    json!({
        "timestamp": timestamp(offset_seconds as i64),
        "type": "response_item",
        "payload": {
            "type": "function_call_output",
            "call_id": call_id,
            "output": output
        }
    })
    .to_string()
}

fn timestamp(offset_seconds: i64) -> String {
    let base = DateTime::<Utc>::from_timestamp(1_779_000_000, 0).unwrap();
    (base + Duration::seconds(offset_seconds)).to_rfc3339_opts(SecondsFormat::Micros, true)
}

fn lines_to_jsonl(lines: &[String]) -> String {
    format!("{}\n", lines.join("\n"))
}

fn synthetic_bench_event_count() -> usize {
    std::env::var("ONECONTEXT_AGENT_SESSION_BENCH_EVENTS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(250)
}

fn live_database_url() -> Option<String> {
    std::env::var("ONECONTEXT_MEMORY_DB_TEST_URL").ok()
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

fn run_mutable_container_live_benchmark(client: &mut Client, seed: u128) {
    let lines = codex_rollout_lines_for_session(&format!("session-mutable-{seed}"));
    let records = compile_and_emit(&lines, AgentIngestProfile::HotMemory)
        .into_iter()
        .filter(|record| matches!(record.kind.as_str(), "agent_session" | "agent_turn"))
        .collect::<Vec<_>>();
    assert_eq!(kind_count(&records, "agent_session"), 1);
    assert_eq!(kind_count(&records, "agent_turn"), 1);

    let request = write_request(records.clone(), seed + 10);
    let started = Instant::now();
    let inserted = write_objects_with_client(client, &request).unwrap();
    let insert_elapsed = started.elapsed();
    assert_eq!(inserted.record_count, 2);
    assert_eq!(inserted.inserted_count, 2);

    let started = Instant::now();
    let exact_duplicate = write_objects_with_client(client, &request).unwrap();
    let exact_duplicate_elapsed = started.elapsed();
    assert_eq!(exact_duplicate.record_count, 2);
    assert_eq!(exact_duplicate.inserted_count, 0);
    assert_eq!(exact_duplicate.duplicate_count, 2);
    assert!(exact_duplicate
        .receipts
        .iter()
        .all(|receipt| !receipt.inserted));

    let changed_records = records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            let mut changed = record.clone();
            changed.payload["mutable_container_revision"] = json!(2);
            changed.display_text = Some(format!("Mutable container revision {}", index + 1));
            changed
        })
        .collect::<Vec<_>>();
    assert!(changed_records
        .iter()
        .zip(records.iter())
        .all(|(changed, original)| changed.event_start == original.event_start));

    let changed_request = write_request(changed_records.clone(), seed + 20);
    let started = Instant::now();
    let changed_update = write_objects_with_client(client, &changed_request).unwrap();
    let changed_update_elapsed = started.elapsed();
    assert_eq!(changed_update.record_count, 2);
    assert_eq!(changed_update.inserted_count, 0);
    assert_eq!(changed_update.duplicate_count, 2);
    assert!(changed_update
        .receipts
        .iter()
        .all(|receipt| !receipt.inserted));

    for (index, (receipt, record)) in changed_update
        .receipts
        .iter()
        .zip(changed_records.iter())
        .enumerate()
    {
        let row = client
            .query_one(
                r#"
                SELECT
                  source_records.source_record_hash,
                  source_records.conflict_count::bigint AS conflict_count,
                  objects.source_record_hash AS object_hash,
                  objects.payload,
                  objects.display_text
                FROM perception.source_records source_records
                JOIN perception.objects objects
                  ON objects.object_id = source_records.object_id
                 AND objects.event_start = source_records.object_event_start
                WHERE source_records.source_id::text = $1
                  AND source_records.source_record_key = $2
                "#,
                &[&record.source_id, &record.source_record_key],
            )
            .unwrap();
        let source_hash: String = row.get("source_record_hash");
        let object_hash: String = row.get("object_hash");
        let payload: Value = row.get("payload");
        let display_text: Option<String> = row.get("display_text");

        assert_eq!(receipt.object_id, inserted.receipts[index].object_id);
        assert_eq!(source_hash, object_hash);
        assert_eq!(row.get::<_, i64>("conflict_count"), 0);
        assert_eq!(payload["mutable_container_revision"], json!(2));
        assert_eq!(display_text, record.display_text);
    }

    println!(
        "agent_session_mutable_container_benchmark rows={} cold_insert_ms={} exact_duplicate_ms={} changed_update_ms={}",
        records.len(),
        insert_elapsed.as_millis(),
        exact_duplicate_elapsed.as_millis(),
        changed_update_elapsed.as_millis()
    );
}

struct AgentSourceIds;

impl AgentSourceIds {
    const CODEX: &'static str = "10000000-0000-0000-0000-000000000001";
}
