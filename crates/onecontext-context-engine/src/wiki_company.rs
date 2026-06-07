//! Wiki-company dry-run composition.
//!
//! This module turns the loaded pack and run request into an inspectable plan:
//! source packet policy, harness turn previews, routing, and publish intent.

use crate::harness_executor::{build_harness_turn_request, HarnessTurnRequest};
use crate::memoryd_client::ensure_memory_storage_gate;
use crate::memoryd_client::query_recent_density;
use crate::pack::WikiCompanyPackConfig;
use crate::packet_planner::{build_packet_plan, PacketPlannerPolicy, WikiMemoryPacketPlan};
use crate::{ContextEnginePaths, WikiCompanyRunRequest};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WikiCompanyDryRun {
    pub source_metadata: DryRunSourceMetadata,
    pub packet_plan: WikiMemoryPacketPlan,
    pub harness_previews: Vec<HarnessTurnPreview>,
    pub route_count: usize,
    pub publish_intent: WikiPublishIntent,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DryRunSourceMetadata {
    pub status: String,
    pub provider: String,
    pub executable: Option<String>,
    pub storage_gate_status: String,
    pub storage_status: Option<String>,
    pub storage_ready: bool,
    pub recent_history_ready: bool,
    pub recent_backfill_status: Option<String>,
    pub empty_fallback_allowed: bool,
    pub bucket_count: usize,
    pub object_count: i64,
    pub active_day_count: usize,
    pub active_hour_count: usize,
    pub error: Option<String>,
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HarnessTurnPreview {
    pub job_id: String,
    pub agent_id: String,
    pub unit_id: String,
    pub harness_id: String,
    pub transport: String,
    pub model: String,
    pub reasoning_effort: String,
    pub prompt_part_count: usize,
    pub prompt_bytes: usize,
    pub input_count: usize,
    pub output_count: usize,
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub requires_final_message: bool,
    pub requires_talk_append: bool,
    pub requires_mail_delivery: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WikiPublishIntent {
    pub publisher: String,
    pub source_root: String,
    pub site_root: String,
    pub after: String,
    pub trigger: String,
}

pub fn build_wiki_company_dry_run(
    paths: &ContextEnginePaths,
    pack: &WikiCompanyPackConfig,
    request: &WikiCompanyRunRequest,
) -> Result<WikiCompanyDryRun, String> {
    let packet_policy = PacketPlannerPolicy {
        window_days: request.source_window_days,
        ..PacketPlannerPolicy::default()
    };
    let storage_gate = ensure_memory_storage_gate(paths, request.source_window_days);
    if !storage_gate.ready_for_source_packets() && !storage_gate.fallback_allowed {
        return Err(storage_gate.failure_message());
    }
    let density = query_recent_density(paths, request.source_window_days);
    let packet_plan = build_packet_plan(&density.source_events, packet_policy, &BTreeSet::new());
    let mut previews = Vec::new();
    for job in &pack.jobs {
        let turn = build_harness_turn_request(paths, pack, &request.run_id, &job.id)?;
        previews.push(preview_from_turn(&turn));
    }
    Ok(WikiCompanyDryRun {
        source_metadata: DryRunSourceMetadata {
            status: density.status,
            provider: density.provider,
            executable: density.executable,
            storage_gate_status: storage_gate.status,
            storage_status: storage_gate.storage_status,
            storage_ready: storage_gate.storage_ready,
            recent_history_ready: storage_gate.recent_history_ready,
            recent_backfill_status: storage_gate.recent_backfill_status,
            empty_fallback_allowed: storage_gate.fallback_allowed,
            bucket_count: density.bucket_count,
            object_count: density.object_count,
            active_day_count: density.active_day_count,
            active_hour_count: density.active_hour_count,
            error: density.error.or(storage_gate.error),
            note: "Dry-run requires onecontext-memoryd storage readiness before source packet planning; empty source fallback is available only when ONECONTEXT_ALLOW_EMPTY_MEMORY_FALLBACK is enabled.".to_string(),
        },
        packet_plan,
        route_count: previews
            .iter()
            .map(|preview| preview.to.len() + preview.cc.len())
            .sum(),
        harness_previews: previews,
        publish_intent: WikiPublishIntent {
            publisher: "onecontext-wiki-core".to_string(),
            source_root: paths.user_wiki.join("source").display().to_string(),
            site_root: paths.user_wiki.join("site").display().to_string(),
            after: "all required harness receipts are complete".to_string(),
            trigger: request.trigger.clone(),
        },
    })
}

fn preview_from_turn(turn: &HarnessTurnRequest) -> HarnessTurnPreview {
    HarnessTurnPreview {
        job_id: turn.job.id.clone(),
        agent_id: turn.agent.id.clone(),
        unit_id: turn.unit_id.clone(),
        harness_id: turn.harness.id.clone(),
        transport: turn.harness.transport.clone(),
        model: turn.agent.model.clone(),
        reasoning_effort: turn.agent.reasoning_effort.clone(),
        prompt_part_count: turn.prompt_bundle.len(),
        prompt_bytes: turn.prompt_bundle.iter().map(|prompt| prompt.bytes).sum(),
        input_count: turn.job.inputs.len(),
        output_count: turn.job.outputs.len(),
        to: turn.talk_report.to.clone(),
        cc: turn.talk_report.cc.clone(),
        requires_final_message: turn.required_receipts.require_final_message,
        requires_talk_append: turn.required_receipts.require_talk_append,
        requires_mail_delivery: turn.required_receipts.require_mail_delivery,
    }
}
