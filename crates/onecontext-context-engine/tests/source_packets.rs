use onecontext_context_engine::packet_planner::{
    build_packet_plan, PacketPlannerPolicy, SourceEvent,
};
use onecontext_context_engine::source_packets::{
    materialize_source_packets, MaterializedSourcePacket, PageSnapshot, SourcePacketIndex,
    SourcePacketMaterializationRequest,
};
use onecontext_context_engine::ContextEnginePaths;
use sha2::{Digest, Sha256};
use std::fs;

#[test]
fn materializer_writes_distinct_session_shards_with_actual_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");
    let events = vec![
        big_event("2026-06-07T09:10:00Z", "session-b", "SESSION_B_ONLY_MARKER"),
        big_event("2026-06-07T09:00:00Z", "session-a", "SESSION_A_ONLY_MARKER"),
    ];
    let plan = build_packet_plan(&events, small_packet_policy(), &Default::default());
    assert_eq!(plan.total_packet_count, 2);

    let result = materialize_source_packets(
        &paths,
        &plan,
        &events,
        SourcePacketMaterializationRequest {
            run_id: "session-shards".to_string(),
            source_window_days: 3,
            current_pages: Vec::new(),
        },
    )
    .expect("materialize source packets");

    assert_eq!(result.packet_count, 2);
    assert_eq!(result.issues, Vec::<String>::new());
    let index_text = fs::read_to_string(&result.index_path).expect("source packet index");
    let index: SourcePacketIndex = serde_json::from_str(&index_text).expect("decode index");
    assert_eq!(index.packet_count, 2);
    assert_eq!(index.packets[0].packet_id, result.packets[0].packet_id);
    assert_eq!(
        index.packets[0].content_sha256,
        result.packets[0].content_sha256
    );
    assert!(index
        .cursors
        .raw_ingest_cursor_path
        .ends_with("context-engine/live/runs/session-shards/state/raw-ingest-cursor.json"));
    assert!(index
        .cursors
        .wiki_memory_cursor_path
        .ends_with("context-engine/live/runs/session-shards/state/wiki-memory-cursor.json"));
    assert!(index
        .cache
        .invalidated_by
        .contains(&"prompt_manifest.prompt_hashes".to_string()));
    assert_eq!(index.cache.key, "source_packet_hash");
    let first = &result.packets[0];
    let second = &result.packets[1];
    assert_eq!(first.session_ids, vec!["session-a"]);
    assert_eq!(second.session_ids, vec!["session-b"]);
    assert_eq!(first.event_count, 1);
    assert_eq!(second.event_count, 1);
    assert_eq!(first.target_packet_tokens, plan.target_packet_tokens);

    let first_body = fs::read_to_string(&first.path).expect("first packet body");
    let second_body = fs::read_to_string(&second.path).expect("second packet body");
    assert!(first_body.contains("SESSION_A_ONLY_MARKER"));
    assert!(!first_body.contains("SESSION_B_ONLY_MARKER"));
    assert!(second_body.contains("SESSION_B_ONLY_MARKER"));
    assert!(!second_body.contains("SESSION_A_ONLY_MARKER"));
}

#[test]
fn materializer_uses_shard_index_for_same_session_event_chunks() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");
    let events = vec![
        big_event("2026-06-07T10:00:00Z", "session-a", "CHUNK_ONE_MARKER"),
        big_event("2026-06-07T10:05:00Z", "session-a", "CHUNK_TWO_MARKER"),
        big_event("2026-06-07T10:10:00Z", "session-a", "CHUNK_THREE_MARKER"),
    ];
    let plan = build_packet_plan(&events, small_packet_policy(), &Default::default());
    assert_eq!(plan.total_packet_count, 3);

    let result = materialize_source_packets(
        &paths,
        &plan,
        &events,
        SourcePacketMaterializationRequest {
            run_id: "same-session-shards".to_string(),
            source_window_days: 3,
            current_pages: Vec::new(),
        },
    )
    .expect("materialize source packets");

    assert_eq!(result.packet_count, 3);
    assert_eq!(result.issues, Vec::<String>::new());
    for packet in &result.packets {
        assert_eq!(packet.session_ids, vec!["session-a"]);
        assert_eq!(packet.event_count, 1);
        assert_eq!(packet.shard_count, 3);
    }
    let bodies = result
        .packets
        .iter()
        .map(|packet| fs::read_to_string(&packet.path).expect("packet body"))
        .collect::<Vec<_>>();
    assert!(bodies[0].contains("CHUNK_ONE_MARKER"));
    assert!(!bodies[0].contains("CHUNK_TWO_MARKER"));
    assert!(!bodies[0].contains("CHUNK_THREE_MARKER"));
    assert!(bodies[1].contains("CHUNK_TWO_MARKER"));
    assert!(!bodies[1].contains("CHUNK_ONE_MARKER"));
    assert!(!bodies[1].contains("CHUNK_THREE_MARKER"));
    assert!(bodies[2].contains("CHUNK_THREE_MARKER"));
    assert!(!bodies[2].contains("CHUNK_ONE_MARKER"));
    assert!(!bodies[2].contains("CHUNK_TWO_MARKER"));
}

#[test]
fn materializer_persists_page_snapshots_and_body_hash_truth() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");
    let events = vec![event(
        "2026-06-07T11:00:00Z",
        "session-page",
        "Remember that page snapshots are part of source packet evidence.",
    )];
    let plan = build_packet_plan(&events, PacketPlannerPolicy::default(), &Default::default());
    let page = PageSnapshot {
        slug: "projects".to_string(),
        source_path: "user-wiki/source/families/work/projects/projects.md".to_string(),
        content_sha256: "page-hash-123".to_string(),
        excerpt: "Existing projects page excerpt.".to_string(),
    };

    let first = materialize_source_packets(
        &paths,
        &plan,
        &events,
        SourcePacketMaterializationRequest {
            run_id: "page-snapshot-a".to_string(),
            source_window_days: 3,
            current_pages: vec![page.clone()],
        },
    )
    .expect("materialize source packets");
    let packet = &first.packets[0];
    let body = fs::read_to_string(&packet.path).expect("packet body");
    let metadata = fs::read_to_string(&packet.metadata_path).expect("packet metadata");
    let decoded: MaterializedSourcePacket =
        serde_json::from_str(&metadata).expect("decode packet metadata");

    assert!(body.contains("Current Wiki Page Snapshots"));
    assert!(body.contains("projects"));
    assert!(body.contains("page-hash-123"));
    assert!(body.contains("Existing projects page excerpt."));
    assert_eq!(packet.content_sha256, sha256_hex(body.as_bytes()));
    assert_eq!(packet.source_packet_hash, packet.content_sha256);
    assert_eq!(decoded.content_sha256, packet.content_sha256);
    assert_eq!(decoded.source_packet_hash, packet.source_packet_hash);
    assert_eq!(decoded.page_snapshot_count, 1);
    assert_eq!(decoded.page_snapshots, vec![page]);
    assert_eq!(decoded.sources, vec!["fixture"]);
    assert_eq!(decoded.project_keys, vec!["1context"]);
    assert_eq!(decoded.cwd_values, vec!["/tmp/project"]);
    assert!(decoded.cache_path.ends_with(".json"));

    let changed = materialize_source_packets(
        &paths,
        &plan,
        &events,
        SourcePacketMaterializationRequest {
            run_id: "page-snapshot-b".to_string(),
            source_window_days: 3,
            current_pages: vec![PageSnapshot {
                slug: "projects".to_string(),
                source_path: "user-wiki/source/families/work/projects/projects.md".to_string(),
                content_sha256: "page-hash-456".to_string(),
                excerpt: "Updated projects page excerpt.".to_string(),
            }],
        },
    )
    .expect("materialize changed page snapshot");
    assert_ne!(
        first.packets[0].content_sha256,
        changed.packets[0].content_sha256
    );
}

#[test]
fn materializer_preserves_source_identity_and_privacy_metadata() {
    let temp = tempfile::tempdir().expect("tempdir");
    let paths = ContextEnginePaths::new(temp.path().join("runtime/1Context"));
    paths.ensure_release_dirs().expect("release dirs");
    let mut event = event(
        "2026-06-07T12:00:00Z",
        "chat-a",
        "Identity metadata must survive packet materialization.",
    );
    event.object_id = Some("object-001".to_string());
    event.source_id = Some("source-001".to_string());
    event.source_type = Some("imessage".to_string());
    event.source_key = Some("imessage-local".to_string());
    event.source_record_id = Some("record-001".to_string());
    event.source_record_key = Some("imessage:row:42".to_string());
    event.source_record_hash = Some("record-hash-001".to_string());
    event.series_id = Some("series-001".to_string());
    event.series_kind = Some("chat".to_string());
    event.series_key = Some("chat-a".to_string());
    event.series_display_name = Some("Messages Chat A".to_string());
    event.role = Some("user".to_string());
    event.privacy_class = Some("private".to_string());
    event.body_type = Some("text".to_string());
    let events = vec![event];
    let plan = build_packet_plan(&events, PacketPlannerPolicy::default(), &Default::default());

    let result = materialize_source_packets(
        &paths,
        &plan,
        &events,
        SourcePacketMaterializationRequest {
            run_id: "source-identity".to_string(),
            source_window_days: 3,
            current_pages: Vec::new(),
        },
    )
    .expect("materialize source packets");

    let packet = &result.packets[0];
    assert_eq!(packet.object_ids, vec!["object-001"]);
    assert_eq!(packet.source_ids, vec!["source-001"]);
    assert_eq!(packet.source_types, vec!["imessage"]);
    assert_eq!(packet.source_keys, vec!["imessage-local"]);
    assert_eq!(packet.source_record_ids, vec!["record-001"]);
    assert_eq!(packet.source_record_keys, vec!["imessage:row:42"]);
    assert_eq!(packet.source_record_hashes, vec!["record-hash-001"]);
    assert_eq!(packet.series_ids, vec!["series-001"]);
    assert_eq!(packet.series_kinds, vec!["chat"]);
    assert_eq!(packet.series_keys, vec!["chat-a"]);
    assert_eq!(packet.series_display_names, vec!["Messages Chat A"]);
    assert_eq!(packet.roles, vec!["user"]);
    assert_eq!(packet.privacy_classes, vec!["private"]);
    assert_eq!(packet.body_types, vec!["text"]);
    assert_eq!(packet.source_packet_hash, packet.content_sha256);
    let body = fs::read_to_string(&packet.path).expect("packet body");
    assert!(body.contains("object_id: object-001"));
    assert!(body.contains("privacy_class: private"));
    assert!(body.contains("source_record_hash: record-hash-001"));
}

fn small_packet_policy() -> PacketPlannerPolicy {
    PacketPlannerPolicy {
        usable_context_tokens: 10_000,
        context_fraction: 0.25,
        max_packets_per_run: 10,
        ..PacketPlannerPolicy::default()
    }
}

fn big_event(ts: &str, session_id: &str, marker: &str) -> SourceEvent {
    let text = format!("{marker} {}", "x".repeat(7_000));
    SourceEvent {
        char_count: Some(text.len() as u32),
        text,
        ..event(ts, session_id, marker)
    }
}

fn event(ts: &str, session_id: &str, text: &str) -> SourceEvent {
    SourceEvent {
        ts: ts.to_string(),
        session_id: session_id.to_string(),
        kind: "user".to_string(),
        text: text.to_string(),
        source: Some("fixture".to_string()),
        cwd: Some("/tmp/project".to_string()),
        project_key: Some("1context".to_string()),
        char_count: None,
        ..SourceEvent::default()
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
