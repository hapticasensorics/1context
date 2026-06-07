use onecontext_context_engine::packet_planner::{
    build_packet_plan, PacketPlannerPolicy, SourceEvent,
};
use std::collections::BTreeSet;

#[test]
fn planner_prioritizes_recent_three_days_then_backfill() {
    let events = vec![
        event("2026-05-07T10:00:00Z", "old", "old work"),
        event("2026-06-03T09:00:00Z", "recent-a", "recent day one"),
        event("2026-06-04T09:00:00Z", "recent-b", "recent day two"),
        event("2026-06-05T09:00:00Z", "recent-c", "recent day three"),
    ];
    let plan = build_packet_plan(
        &events,
        PacketPlannerPolicy {
            max_packets_per_run: 4,
            ..PacketPlannerPolicy::default()
        },
        &BTreeSet::new(),
    );

    assert_eq!(plan.mode, "catch_up_backfill");
    assert_eq!(
        plan.selection_strategy,
        "recent_three_days_first_then_oldest_to_newest"
    );
    assert_eq!(plan.active_day_count, 4);
    assert_eq!(plan.active_hour_count, 4);
    assert_eq!(
        dates(&plan.selected_packets),
        vec!["2026-06-03", "2026-06-04", "2026-06-05", "2026-05-07"]
    );
}

#[test]
fn planner_skips_cached_packets_and_ignores_events_without_timestamps() {
    let cached = "2026-06-05T09-01-of-01-hour".to_string();
    let events = vec![
        event("2026-06-05T09:00:00Z", "cached", "already done"),
        event("2026-06-05T10:00:00Z", "fresh", "new work"),
        SourceEvent {
            ts: String::new(),
            session_id: "missing-ts".to_string(),
            kind: "user".to_string(),
            text: "ignored".to_string(),
            source: None,
            cwd: None,
            project_key: None,
            char_count: None,
        },
    ];
    let plan = build_packet_plan(
        &events,
        PacketPlannerPolicy::default(),
        &BTreeSet::from([cached]),
    );

    assert_eq!(plan.total_packet_count, 2);
    assert_eq!(plan.selected_packet_count, 1);
    assert_eq!(plan.selected_packets[0].date, "2026-06-05");
    assert_eq!(plan.selected_packets[0].hour, "10");
}

#[test]
fn planner_splits_large_hours_by_session_and_event_chunks() {
    let mut events = Vec::new();
    events.push(big_event("2026-06-05T09:00:00Z", "session-a", 4_800));
    events.push(big_event("2026-06-05T09:05:00Z", "session-b", 4_800));
    events.push(big_event("2026-06-05T09:10:00Z", "session-b", 4_800));
    let plan = build_packet_plan(
        &events,
        PacketPlannerPolicy {
            usable_context_tokens: 40_000,
            context_fraction: 0.25,
            max_packets_per_run: 10,
            ..PacketPlannerPolicy::default()
        },
        &BTreeSet::new(),
    );

    assert_eq!(plan.target_packet_tokens, 10_000);
    assert_eq!(plan.total_packet_count, 1);

    let plan = build_packet_plan(
        &events,
        PacketPlannerPolicy {
            usable_context_tokens: 10_000,
            context_fraction: 0.25,
            max_packets_per_run: 10,
            ..PacketPlannerPolicy::default()
        },
        &BTreeSet::new(),
    );
    assert_eq!(plan.target_packet_tokens, 2_500);
    assert_eq!(plan.total_packet_count, 2);
    assert_eq!(
        plan.packets
            .iter()
            .map(|packet| packet.session_ids.join(","))
            .collect::<Vec<_>>(),
        vec!["session-a", "session-b"]
    );

    let plan = build_packet_plan(
        &events,
        PacketPlannerPolicy {
            usable_context_tokens: 8_400,
            context_fraction: 0.25,
            max_packets_per_run: 10,
            ..PacketPlannerPolicy::default()
        },
        &BTreeSet::new(),
    );
    assert_eq!(plan.target_packet_tokens, 2_100);
    assert_eq!(plan.total_packet_count, 3);
    assert!(plan
        .packets
        .iter()
        .all(|packet| packet.packet_kind == "hour_part"));
    assert!(plan
        .packets
        .iter()
        .all(|packet| packet.estimated_tokens <= 2_100));
}

#[test]
fn planner_default_context_fraction_targets_about_sixty_two_percent() {
    let policy = PacketPlannerPolicy::default();
    assert_eq!(policy.usable_context_tokens, 258_400);
    assert_eq!(policy.context_fraction, 0.62);
    assert_eq!(policy.target_packet_tokens(), 160_208);
}

fn event(ts: &str, session_id: &str, text: &str) -> SourceEvent {
    SourceEvent {
        ts: ts.to_string(),
        session_id: session_id.to_string(),
        kind: "user".to_string(),
        text: text.to_string(),
        source: Some("codex".to_string()),
        cwd: None,
        project_key: None,
        char_count: None,
    }
}

fn big_event(ts: &str, session_id: &str, char_count: u32) -> SourceEvent {
    SourceEvent {
        ts: ts.to_string(),
        session_id: session_id.to_string(),
        kind: "assistant".to_string(),
        text: "x".repeat(char_count as usize),
        source: Some("codex".to_string()),
        cwd: None,
        project_key: None,
        char_count: Some(char_count),
    }
}

fn dates(packets: &[onecontext_context_engine::packet_planner::WikiMemoryPacket]) -> Vec<&str> {
    packets.iter().map(|packet| packet.date.as_str()).collect()
}
