//! Source packet planning.
//!
//! Port target for `memory-core/src/onectx/memory/wiki_memory_plan.py`.
//! This module should query source metadata, split recent and backfill windows,
//! estimate token budgets, and decide which scribe packets need fresh work.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const DEFAULT_USABLE_CONTEXT_TOKENS: u32 = 258_400;
pub const DEFAULT_CONTEXT_FRACTION: f64 = 0.62;
pub const DEFAULT_MAX_PACKETS_PER_RUN: usize = 20;
pub const DEFAULT_RECENT_PRIORITY_DAYS: i64 = 3;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEvent {
    pub ts: String,
    pub session_id: String,
    pub kind: String,
    pub text: String,
    #[serde(default)]
    pub source: Option<String>,
    #[serde(default)]
    pub cwd: Option<String>,
    #[serde(default)]
    pub project_key: Option<String>,
    #[serde(default)]
    pub char_count: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiMemoryPacket {
    pub packet_id: String,
    pub packet_kind: String,
    pub date: String,
    pub hour: String,
    pub shard_index: usize,
    pub shard_count: usize,
    pub session_ids: Vec<String>,
    pub event_count: usize,
    pub char_count: u32,
    pub estimated_tokens: u32,
    pub cached: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PacketPlannerPolicy {
    pub window_days: u32,
    pub usable_context_tokens: u32,
    pub context_fraction: f64,
    pub max_packets_per_run: usize,
    pub recent_priority_days: i64,
}

impl Default for PacketPlannerPolicy {
    fn default() -> Self {
        Self {
            window_days: 30,
            usable_context_tokens: DEFAULT_USABLE_CONTEXT_TOKENS,
            context_fraction: DEFAULT_CONTEXT_FRACTION,
            max_packets_per_run: DEFAULT_MAX_PACKETS_PER_RUN,
            recent_priority_days: DEFAULT_RECENT_PRIORITY_DAYS,
        }
    }
}

impl PacketPlannerPolicy {
    pub fn target_packet_tokens(&self) -> u32 {
        let fraction = self.context_fraction.clamp(0.25, 0.9);
        (self.usable_context_tokens as f64 * fraction)
            .floor()
            .max(2_000.0) as u32
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiMemoryPacketPlan {
    pub mode: String,
    pub selection_strategy: String,
    pub usable_context_tokens: u32,
    pub context_fraction: f64,
    pub target_packet_tokens: u32,
    pub active_day_count: usize,
    pub active_hour_count: usize,
    pub total_packet_count: usize,
    pub selected_packet_count: usize,
    pub packets: Vec<WikiMemoryPacket>,
    pub selected_packets: Vec<WikiMemoryPacket>,
}

pub fn build_packet_plan(
    events: &[SourceEvent],
    policy: PacketPlannerPolicy,
    cached_packet_ids: &BTreeSet<String>,
) -> WikiMemoryPacketPlan {
    let target_tokens = policy.target_packet_tokens();
    let mut by_hour = BTreeMap::<(String, String), Vec<SourceEvent>>::new();
    let mut resolved_events = events
        .iter()
        .filter_map(|event| event_day_hour(event).map(|key| (key, event.clone())))
        .collect::<Vec<_>>();
    resolved_events.sort_by(|(_, left), (_, right)| left.ts.cmp(&right.ts));

    for (key, event) in resolved_events {
        by_hour.entry(key).or_default().push(event);
    }

    let mut packets = Vec::new();
    for ((date, hour), hour_events) in by_hour {
        packets.extend(packetize_hour(
            date,
            hour,
            hour_events,
            target_tokens,
            cached_packet_ids,
        ));
    }

    let mode = if policy.window_days > 2 {
        "catch_up_backfill"
    } else {
        "daily_maintenance"
    };
    let selection_strategy = if mode == "catch_up_backfill" {
        "recent_three_days_first_then_oldest_to_newest"
    } else {
        "oldest_to_newest"
    };
    let selected_packets = select_packets_for_run(
        &packets,
        mode,
        policy.max_packets_per_run,
        policy.recent_priority_days,
    );
    let active_days = packets
        .iter()
        .map(|packet| packet.date.as_str())
        .collect::<BTreeSet<_>>();
    let active_hours = packets
        .iter()
        .map(|packet| (packet.date.as_str(), packet.hour.as_str()))
        .collect::<BTreeSet<_>>();

    WikiMemoryPacketPlan {
        mode: mode.to_string(),
        selection_strategy: selection_strategy.to_string(),
        usable_context_tokens: policy.usable_context_tokens,
        context_fraction: policy.context_fraction.clamp(0.25, 0.9),
        target_packet_tokens: target_tokens,
        active_day_count: active_days.len(),
        active_hour_count: active_hours.len(),
        total_packet_count: packets.len(),
        selected_packet_count: selected_packets.len(),
        packets,
        selected_packets,
    }
}

pub fn select_packets_for_run(
    packets: &[WikiMemoryPacket],
    mode: &str,
    max_packets: usize,
    recent_priority_days: i64,
) -> Vec<WikiMemoryPacket> {
    let uncached = packets
        .iter()
        .filter(|packet| !packet.cached)
        .cloned()
        .collect::<Vec<_>>();
    if mode != "catch_up_backfill" || recent_priority_days <= 0 {
        return uncached.into_iter().take(max_packets).collect();
    }

    let Some(cutoff) = recent_priority_cutoff(packets, recent_priority_days) else {
        return uncached.into_iter().take(max_packets).collect();
    };
    let mut recent = Vec::new();
    let mut older = Vec::new();
    for packet in uncached {
        if packet.date >= cutoff {
            recent.push(packet);
        } else {
            older.push(packet);
        }
    }
    recent.into_iter().chain(older).take(max_packets).collect()
}

fn recent_priority_cutoff(
    packets: &[WikiMemoryPacket],
    recent_priority_days: i64,
) -> Option<String> {
    let latest = packets.iter().map(|packet| packet.date.as_str()).max()?;
    let parsed = chrono::NaiveDate::parse_from_str(latest, "%Y-%m-%d").ok()?;
    Some((parsed - Duration::days(recent_priority_days - 1)).to_string())
}

fn packetize_hour(
    date: String,
    hour: String,
    events: Vec<SourceEvent>,
    target_tokens: u32,
    cached_packet_ids: &BTreeSet<String>,
) -> Vec<WikiMemoryPacket> {
    if estimate_tokens(&events) <= target_tokens {
        return vec![packet_for_events(
            date,
            hour,
            1,
            1,
            "hour",
            events,
            cached_packet_ids,
        )];
    }

    let mut by_session = BTreeMap::<String, Vec<SourceEvent>>::new();
    for event in events {
        by_session
            .entry(if event.session_id.is_empty() {
                "unknown".to_string()
            } else {
                event.session_id.clone()
            })
            .or_default()
            .push(event);
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
            packet_for_events(
                date.clone(),
                hour.clone(),
                index + 1,
                shard_count,
                "hour_part",
                rows,
                cached_packet_ids,
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

fn packet_for_events(
    date: String,
    hour: String,
    shard_index: usize,
    shard_count: usize,
    packet_kind: &str,
    events: Vec<SourceEvent>,
    cached_packet_ids: &BTreeSet<String>,
) -> WikiMemoryPacket {
    let session_ids = events
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
        .collect::<Vec<_>>();
    let packet_id = format!(
        "{}T{}-{:02}-of-{:02}-{}",
        date, hour, shard_index, shard_count, packet_kind
    );
    WikiMemoryPacket {
        cached: cached_packet_ids.contains(&packet_id),
        packet_id,
        packet_kind: packet_kind.to_string(),
        date,
        hour,
        shard_index,
        shard_count,
        session_ids,
        event_count: events.len(),
        char_count: char_count(&events),
        estimated_tokens: estimate_tokens(&events),
    }
}

pub fn estimate_tokens(events: &[SourceEvent]) -> u32 {
    let chars = char_count(events);
    let count = events.len() as u32;
    (chars / 4 + count * 16).max(1)
}

fn char_count(events: &[SourceEvent]) -> u32 {
    events
        .iter()
        .map(|event| event.char_count.unwrap_or(event.text.len() as u32))
        .sum()
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
