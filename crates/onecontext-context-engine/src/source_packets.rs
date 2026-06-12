//! Source packet materialization.
//!
//! The packet planner decides which bounded windows should run. This module
//! writes those selections into durable packet artifacts that agents can cite
//! and hash.

use crate::packet_planner::{estimate_tokens, SourceEvent, WikiMemoryPacket, WikiMemoryPacketPlan};
use crate::{safe_run_id, ContextEnginePaths, CONTEXT_ENGINE_SCHEMA_VERSION};
use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PageSnapshot {
    pub slug: String,
    pub source_path: String,
    pub content_sha256: String,
    pub excerpt: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePacketMaterializationRequest {
    pub run_id: String,
    pub source_window_days: u32,
    pub current_pages: Vec<PageSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterializedSourcePacket {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub packet_id: String,
    pub packet_kind: String,
    pub date: String,
    pub hour: String,
    pub shard_index: usize,
    pub shard_count: usize,
    pub event_count: usize,
    pub char_count: u32,
    pub session_ids: Vec<String>,
    pub session_count: usize,
    pub object_ids: Vec<String>,
    pub source_ids: Vec<String>,
    pub source_types: Vec<String>,
    pub source_keys: Vec<String>,
    pub source_record_ids: Vec<String>,
    pub source_record_keys: Vec<String>,
    pub source_record_hashes: Vec<String>,
    pub series_ids: Vec<String>,
    pub series_kinds: Vec<String>,
    pub series_keys: Vec<String>,
    pub series_display_names: Vec<String>,
    pub sources: Vec<String>,
    pub roles: Vec<String>,
    pub privacy_classes: Vec<String>,
    pub body_types: Vec<String>,
    pub project_keys: Vec<String>,
    pub cwd_values: Vec<String>,
    pub estimated_tokens: u32,
    pub target_packet_tokens: u32,
    pub source_window_days: u32,
    pub page_snapshot_count: usize,
    pub page_snapshots: Vec<PageSnapshot>,
    pub path: String,
    pub metadata_path: String,
    pub cache_path: String,
    pub source_packet_hash: String,
    pub content_sha256: String,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePacketMaterializationResult {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub index_path: String,
    pub packet_count: usize,
    pub packets: Vec<MaterializedSourcePacket>,
    pub issues: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePacketIndex {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub packet_count: usize,
    pub packets: Vec<SourcePacketIndexEntry>,
    pub cursors: SourcePacketCursorPaths,
    pub cache: SourcePacketCachePolicy,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePacketIndexEntry {
    pub packet_id: String,
    pub packet_kind: String,
    pub date: String,
    pub hour: String,
    pub shard_index: usize,
    pub shard_count: usize,
    pub event_count: usize,
    pub session_count: usize,
    pub source_packet_hash: String,
    pub content_sha256: String,
    pub path: String,
    pub metadata_path: String,
    pub cache_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePacketCursorPaths {
    pub raw_ingest_cursor_path: String,
    pub wiki_memory_cursor_path: String,
    pub packet_cache_index_path: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePacketCachePolicy {
    pub key: String,
    pub invalidated_by: Vec<String>,
}

pub fn materialize_source_packets(
    paths: &ContextEnginePaths,
    plan: &WikiMemoryPacketPlan,
    events: &[SourceEvent],
    request: SourcePacketMaterializationRequest,
) -> Result<SourcePacketMaterializationResult, String> {
    let run_id = safe_run_id(&request.run_id);
    let packet_root = paths.run_source_packets(&run_id);
    let cache_root = paths
        .run_state_dir(&run_id)
        .join("wiki-memory-cache")
        .join("scribe-packets");
    fs::create_dir_all(&packet_root)
        .map_err(|error| format!("failed to create {}: {error}", packet_root.display()))?;
    fs::create_dir_all(&cache_root)
        .map_err(|error| format!("failed to create {}: {error}", cache_root.display()))?;

    let mut packets = Vec::new();
    let mut issues = packet_event_index_issues(events);
    let packet_events_by_id = packet_event_index(plan, events);
    for packet in &plan.selected_packets {
        let packet_events = packet_events_by_id
            .get(&packet.packet_id)
            .cloned()
            .unwrap_or_default();
        issues.extend(packet_metadata_issues(packet, &packet_events));
        if packet_events.is_empty() && packet.event_count > 0 {
            issues.push(format!(
                "selected packet {} reconstructed no source events but planner expected {}",
                packet.packet_id, packet.event_count
            ));
        }
        match materialize_one_packet(
            &packet_root,
            &cache_root,
            packet,
            &packet_events,
            &request,
            &run_id,
            plan.target_packet_tokens,
        ) {
            Ok(packet) => packets.push(packet),
            Err(error) => issues.push(error),
        }
    }

    let index_path = packet_root.join("index.json");
    let index = source_packet_index(paths, &run_id, &packets);
    let index_json = serde_json::to_vec_pretty(&index)
        .map_err(|error| format!("failed to encode source packet index: {error}"))?;
    fs::write(&index_path, index_json)
        .map_err(|error| format!("failed to write {}: {error}", index_path.display()))?;

    Ok(SourcePacketMaterializationResult {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.source_packet_materialization.v1".to_string(),
        run_id,
        index_path: index_path.display().to_string(),
        packet_count: packets.len(),
        packets,
        issues,
    })
}

fn source_packet_index(
    paths: &ContextEnginePaths,
    run_id: &str,
    packets: &[MaterializedSourcePacket],
) -> SourcePacketIndex {
    SourcePacketIndex {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.source_packet_index.v1".to_string(),
        run_id: run_id.to_string(),
        packet_count: packets.len(),
        packets: packets
            .iter()
            .map(|packet| SourcePacketIndexEntry {
                packet_id: packet.packet_id.clone(),
                packet_kind: packet.packet_kind.clone(),
                date: packet.date.clone(),
                hour: packet.hour.clone(),
                shard_index: packet.shard_index,
                shard_count: packet.shard_count,
                event_count: packet.event_count,
                session_count: packet.session_count,
                source_packet_hash: packet.source_packet_hash.clone(),
                content_sha256: packet.content_sha256.clone(),
                path: packet.path.clone(),
                metadata_path: packet.metadata_path.clone(),
                cache_path: packet.cache_path.clone(),
            })
            .collect(),
        cursors: SourcePacketCursorPaths {
            raw_ingest_cursor_path: paths
                .run_state_dir(run_id)
                .join("raw-ingest-cursor.json")
                .display()
                .to_string(),
            wiki_memory_cursor_path: paths
                .run_state_dir(run_id)
                .join("wiki-memory-cursor.json")
                .display()
                .to_string(),
            packet_cache_index_path: paths
                .run_state_dir(run_id)
                .join("packet-cache-index.json")
                .display()
                .to_string(),
        },
        cache: SourcePacketCachePolicy {
            key: "source_packet_hash".to_string(),
            invalidated_by: vec![
                "source_packet.source_packet_hash".to_string(),
                "prompt_manifest.source_hashes".to_string(),
                "prompt_manifest.prompt_hashes".to_string(),
            ],
        },
    }
}

fn materialize_one_packet(
    packet_root: &Path,
    cache_root: &Path,
    packet: &WikiMemoryPacket,
    packet_events: &[SourceEvent],
    request: &SourcePacketMaterializationRequest,
    run_id: &str,
    target_packet_tokens: u32,
) -> Result<MaterializedSourcePacket, String> {
    let body = packet_markdown(packet, &packet_events, request);
    let content_sha256 = sha256_hex(body.as_bytes());
    let created_at = Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true);
    let packet_path = packet_root.join(format!("{}.md", safe_run_id(&packet.packet_id)));
    let metadata_path = packet_root.join(format!("{}.json", safe_run_id(&packet.packet_id)));
    let cache_path = cache_root.join(format!("{content_sha256}.json"));
    fs::write(&packet_path, body)
        .map_err(|error| format!("failed to write {}: {error}", packet_path.display()))?;

    let session_ids = session_ids(packet_events);
    let materialized = MaterializedSourcePacket {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.materialized_source_packet.v1".to_string(),
        run_id: run_id.to_string(),
        packet_id: packet.packet_id.clone(),
        packet_kind: packet.packet_kind.clone(),
        date: packet.date.clone(),
        hour: packet.hour.clone(),
        shard_index: packet.shard_index,
        shard_count: packet.shard_count,
        event_count: packet_events.len(),
        char_count: char_count(packet_events),
        session_count: session_ids.len(),
        session_ids,
        object_ids: optional_values(packet_events, |event| event.object_id.as_deref()),
        source_ids: optional_values(packet_events, |event| event.source_id.as_deref()),
        source_types: optional_values(packet_events, |event| event.source_type.as_deref()),
        source_keys: optional_values(packet_events, |event| event.source_key.as_deref()),
        source_record_ids: optional_values(packet_events, |event| {
            event.source_record_id.as_deref()
        }),
        source_record_keys: optional_values(packet_events, |event| {
            event.source_record_key.as_deref()
        }),
        source_record_hashes: optional_values(packet_events, |event| {
            event.source_record_hash.as_deref()
        }),
        series_ids: optional_values(packet_events, |event| event.series_id.as_deref()),
        series_kinds: optional_values(packet_events, |event| event.series_kind.as_deref()),
        series_keys: optional_values(packet_events, |event| event.series_key.as_deref()),
        series_display_names: optional_values(packet_events, |event| {
            event.series_display_name.as_deref()
        }),
        sources: optional_values(packet_events, |event| event.source.as_deref()),
        roles: optional_values(packet_events, |event| event.role.as_deref()),
        privacy_classes: optional_values(packet_events, |event| event.privacy_class.as_deref()),
        body_types: optional_values(packet_events, |event| event.body_type.as_deref()),
        project_keys: optional_values(packet_events, |event| event.project_key.as_deref()),
        cwd_values: optional_values(packet_events, |event| event.cwd.as_deref()),
        estimated_tokens: estimate_tokens(packet_events),
        target_packet_tokens,
        source_window_days: request.source_window_days,
        page_snapshot_count: request.current_pages.len(),
        page_snapshots: request.current_pages.clone(),
        path: packet_path.display().to_string(),
        metadata_path: metadata_path.display().to_string(),
        cache_path: cache_path.display().to_string(),
        source_packet_hash: content_sha256.clone(),
        content_sha256,
        created_at,
    };
    let metadata = serde_json::to_vec_pretty(&materialized)
        .map_err(|error| format!("failed to encode packet metadata: {error}"))?;
    fs::write(&metadata_path, metadata)
        .map_err(|error| format!("failed to write {}: {error}", metadata_path.display()))?;
    Ok(materialized)
}

fn packet_markdown(
    packet: &WikiMemoryPacket,
    events: &[SourceEvent],
    request: &SourcePacketMaterializationRequest,
) -> String {
    let mut out = String::new();
    out.push_str("# Bounded Scribe Source Packet\n\n");
    out.push_str(
        "This raw Perception transcript packet is for hourly scribe jobs only. \
Downstream roles should read scribe artifacts instead of raw history.\n\n",
    );
    out.push_str("## Packet\n\n");
    out.push_str(&format!("- packet_id: `{}`\n", packet.packet_id));
    out.push_str(&format!("- date: `{}`\n", packet.date));
    out.push_str(&format!("- hour: `{}`\n", packet.hour));
    out.push_str(&format!("- packet_kind: `{}`\n", packet.packet_kind));
    out.push_str(&format!(
        "- shard: `{}` of `{}`\n",
        packet.shard_index, packet.shard_count
    ));
    out.push_str(&format!("- event_count: `{}`\n", events.len()));
    out.push_str(&format!(
        "- session_count: `{}`\n",
        session_ids(events).len()
    ));
    out.push_str(&format!("- char_count: `{}`\n", char_count(events)));
    out.push_str(&format!(
        "- estimated_tokens: `{}`\n",
        estimate_tokens(events)
    ));
    out.push_str(&format!(
        "- source_window_days: `{}`\n\n",
        request.source_window_days
    ));
    if !request.current_pages.is_empty() {
        out.push_str("## Current Wiki Page Snapshots\n\n");
        for page in &request.current_pages {
            out.push_str(&format!(
                "### `{}`\n\n- path: `{}`\n- content_sha256: `{}`\n\n{}\n\n",
                page.slug, page.source_path, page.content_sha256, page.excerpt
            ));
        }
    }
    out.push_str("## Source Events\n\n");
    for event in events {
        out.push_str(&format!(
            "### {} {}\n\nsession_id: {}\nkind: {}\nrole: {}\nprivacy_class: {}\nbody_type: {}\nobject_id: {}\nsource_id: {}\nsource_type: {}\nsource_key: {}\nsource_record_key: {}\nsource_record_hash: {}\nseries_key: {}\nsource: {}\ncwd: {}\n\n{}\n\n",
            event.ts,
            event.project_key.clone().unwrap_or_default(),
            event.session_id,
            event.kind,
            event.role.clone().unwrap_or_default(),
            event.privacy_class.clone().unwrap_or_default(),
            event.body_type.clone().unwrap_or_default(),
            event.object_id.clone().unwrap_or_default(),
            event.source_id.clone().unwrap_or_default(),
            event.source_type.clone().unwrap_or_default(),
            event.source_key.clone().unwrap_or_default(),
            event.source_record_key.clone().unwrap_or_default(),
            event.source_record_hash.clone().unwrap_or_default(),
            event.series_key.clone().unwrap_or_default(),
            event.source.clone().unwrap_or_default(),
            event.cwd.clone().unwrap_or_default(),
            event.text
        ));
    }
    out
}

fn packet_event_index(
    plan: &WikiMemoryPacketPlan,
    events: &[SourceEvent],
) -> BTreeMap<String, Vec<SourceEvent>> {
    let mut by_hour = BTreeMap::<(String, String), Vec<SourceEvent>>::new();
    for event in events {
        if let Some(key) = event_day_hour(event) {
            by_hour.entry(key).or_default().push(event.clone());
        }
    }

    let mut by_packet = BTreeMap::new();
    for ((date, hour), mut hour_events) in by_hour {
        hour_events.sort_by(|left, right| left.ts.cmp(&right.ts));
        for (packet_id, packet_events) in
            packetize_hour_events(&date, &hour, hour_events, plan.target_packet_tokens)
        {
            by_packet.insert(packet_id, packet_events);
        }
    }
    by_packet
}

fn packet_event_index_issues(events: &[SourceEvent]) -> Vec<String> {
    let invalid_count = events
        .iter()
        .filter(|event| event_day_hour(event).is_none())
        .count();
    if invalid_count == 0 {
        Vec::new()
    } else {
        vec![format!(
            "ignored {invalid_count} source events with unparsable RFC3339 timestamps during packet materialization"
        )]
    }
}

fn packetize_hour_events(
    date: &str,
    hour: &str,
    events: Vec<SourceEvent>,
    target_tokens: u32,
) -> Vec<(String, Vec<SourceEvent>)> {
    if estimate_tokens(&events) <= target_tokens {
        return vec![(packet_id_for(date, hour, 1, 1, "hour"), events)];
    }

    let mut by_session = BTreeMap::<String, Vec<SourceEvent>>::new();
    for event in events {
        let session_id = if event.session_id.is_empty() {
            "unknown".to_string()
        } else {
            event.session_id.clone()
        };
        by_session.entry(session_id).or_default().push(event);
    }

    let mut event_chunks = Vec::new();
    for (_, mut rows) in by_session {
        rows.sort_by(|left, right| left.ts.cmp(&right.ts));
        if estimate_tokens(&rows) <= target_tokens {
            event_chunks.push(rows);
        } else {
            event_chunks.extend(split_event_rows(rows, target_tokens));
        }
    }

    let shard_count = event_chunks.len().max(1);
    event_chunks
        .into_iter()
        .enumerate()
        .map(|(index, rows)| {
            (
                packet_id_for(date, hour, index + 1, shard_count, "hour_part"),
                rows,
            )
        })
        .collect()
}

fn split_event_rows(rows: Vec<SourceEvent>, target_tokens: u32) -> Vec<Vec<SourceEvent>> {
    let mut chunks = Vec::new();
    let mut current = Vec::new();
    let mut current_tokens = 0;
    for row in rows {
        let row_tokens = estimate_tokens(std::slice::from_ref(&row));
        if !current.is_empty() && current_tokens + row_tokens > target_tokens {
            chunks.push(current);
            current = Vec::new();
            current_tokens = 0;
        }
        current.push(row);
        current_tokens += row_tokens;
    }
    if !current.is_empty() {
        chunks.push(current);
    }
    chunks
}

fn packet_id_for(
    date: &str,
    hour: &str,
    shard_index: usize,
    shard_count: usize,
    packet_kind: &str,
) -> String {
    format!(
        "{}T{}-{:02}-of-{:02}-{}",
        date, hour, shard_index, shard_count, packet_kind
    )
}

fn event_day_hour(event: &SourceEvent) -> Option<(String, String)> {
    let parsed = DateTime::parse_from_rfc3339(&event.ts)
        .ok()?
        .with_timezone(&Utc);
    Some((
        parsed.format("%Y-%m-%d").to_string(),
        parsed.format("%H").to_string(),
    ))
}

fn packet_metadata_issues(packet: &WikiMemoryPacket, events: &[SourceEvent]) -> Vec<String> {
    let mut issues = Vec::new();
    let actual_session_ids = session_ids(events);
    let actual_char_count = char_count(events);
    let actual_estimated_tokens = estimate_tokens(events);
    if packet.event_count != events.len() {
        issues.push(format!(
            "packet {} planner event_count {} did not match materialized event_count {}",
            packet.packet_id,
            packet.event_count,
            events.len()
        ));
    }
    if packet.session_ids != actual_session_ids {
        issues.push(format!(
            "packet {} planner session_ids {:?} did not match materialized session_ids {:?}",
            packet.packet_id, packet.session_ids, actual_session_ids
        ));
    }
    if packet.char_count != actual_char_count {
        issues.push(format!(
            "packet {} planner char_count {} did not match materialized char_count {}",
            packet.packet_id, packet.char_count, actual_char_count
        ));
    }
    if packet.estimated_tokens != actual_estimated_tokens {
        issues.push(format!(
            "packet {} planner estimated_tokens {} did not match materialized estimated_tokens {}",
            packet.packet_id, packet.estimated_tokens, actual_estimated_tokens
        ));
    }
    issues
}

fn session_ids(events: &[SourceEvent]) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| {
            if event.session_id.is_empty() {
                None
            } else {
                Some(event.session_id.clone())
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn char_count(events: &[SourceEvent]) -> u32 {
    events
        .iter()
        .map(|event| event.char_count.unwrap_or(event.text.len() as u32))
        .sum()
}

fn optional_values<'a>(
    events: &'a [SourceEvent],
    value: impl Fn(&'a SourceEvent) -> Option<&'a str>,
) -> Vec<String> {
    events
        .iter()
        .filter_map(|event| {
            let value = value(event)?.trim();
            if value.is_empty() {
                None
            } else {
                Some(value.to_string())
            }
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
