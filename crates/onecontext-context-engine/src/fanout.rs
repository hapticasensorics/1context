//! Fanout expansion for scheduled jobs.
//!
//! Fanout converts a phase job such as `selected_packets` or `refreshed_days`
//! into concrete work units. It does not evaluate model output.

use crate::artifacts::ArtifactHandle;
use crate::scheduler::ScheduledJob;
use crate::source_packets::MaterializedSourcePacket;
use crate::{safe_run_id, CONTEXT_ENGINE_SCHEMA_VERSION};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanoutContext {
    pub run_id: String,
    pub selected_packets: Vec<MaterializedSourcePacket>,
    pub refreshed_days: Vec<String>,
    pub available_artifacts: Vec<ArtifactHandle>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConcreteJobUnit {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub unit_id: String,
    pub phase_id: String,
    pub job_id: String,
    pub agent_id: String,
    pub fanout: String,
    pub status: String,
    pub bindings: BTreeMap<String, String>,
    pub unit_scope: BTreeMap<String, String>,
    pub packet_id: Option<String>,
    pub date: Option<String>,
    pub hour: Option<String>,
    pub required_artifacts: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FanoutExpansion {
    pub schema_version: u32,
    pub kind: String,
    pub run_id: String,
    pub job_id: String,
    pub unit_count: usize,
    pub units: Vec<ConcreteJobUnit>,
    pub issues: Vec<String>,
}

pub fn expand_job_fanout(job: &ScheduledJob, context: &FanoutContext) -> FanoutExpansion {
    let mut issues = Vec::new();
    let units = match job.fanout.as_str() {
        "selected_packets" => context
            .selected_packets
            .iter()
            .map(|packet| unit_for_packet(job, context, packet))
            .collect(),
        "hours_with_multiple_scribe_artifacts" => hour_units_with_multiple_packets(job, context),
        "refreshed_days" => refreshed_days(context)
            .into_iter()
            .map(|day| unit_for_day(job, context, &day))
            .collect(),
        "historian_questions" => question_units(job, context),
        "once_per_refresh" | "once" => vec![unit_for_once(job, context)],
        other => {
            issues.push(format!("unsupported fanout strategy {other}"));
            Vec::new()
        }
    };

    FanoutExpansion {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.fanout_expansion.v1".to_string(),
        run_id: safe_run_id(&context.run_id),
        job_id: job.job_id.clone(),
        unit_count: units.len(),
        units,
        issues,
    }
}

fn unit_for_packet(
    job: &ScheduledJob,
    context: &FanoutContext,
    packet: &MaterializedSourcePacket,
) -> ConcreteJobUnit {
    let mut bindings = job.bindings.clone();
    bindings.insert("date".to_string(), packet.date.clone());
    bindings.insert("hour".to_string(), packet.hour.clone());
    bindings.insert("packet_id".to_string(), packet.packet_id.clone());
    bindings.insert("source_packet_path".to_string(), packet.path.clone());
    bindings.insert(
        "source_packet_hash".to_string(),
        packet.content_sha256.clone(),
    );
    let unit_scope = BTreeMap::from([
        ("packet.id".to_string(), packet.packet_id.clone()),
        ("packet.date".to_string(), packet.date.clone()),
        ("packet.hour".to_string(), packet.hour.clone()),
        ("packet.path".to_string(), packet.path.clone()),
        (
            "packet.content_sha256".to_string(),
            packet.content_sha256.clone(),
        ),
        ("run.id".to_string(), safe_run_id(&context.run_id)),
    ]);
    ConcreteJobUnit {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.concrete_job_unit.v1".to_string(),
        run_id: safe_run_id(&context.run_id),
        unit_id: safe_run_id(&format!("{}-{}", job.job_id, packet.packet_id)),
        phase_id: job.phase_id.clone(),
        job_id: job.job_id.clone(),
        agent_id: job.agent_id.clone(),
        fanout: job.fanout.clone(),
        status: "planned".to_string(),
        bindings,
        unit_scope,
        packet_id: Some(packet.packet_id.clone()),
        date: Some(packet.date.clone()),
        hour: Some(packet.hour.clone()),
        required_artifacts: job.required_artifacts.clone(),
    }
}

fn unit_for_day(job: &ScheduledJob, context: &FanoutContext, day: &str) -> ConcreteJobUnit {
    let mut bindings = job.bindings.clone();
    bindings.insert("date".to_string(), day.to_string());
    let unit_scope = BTreeMap::from([
        ("day.date".to_string(), day.to_string()),
        ("run.id".to_string(), safe_run_id(&context.run_id)),
    ]);
    ConcreteJobUnit {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.concrete_job_unit.v1".to_string(),
        run_id: safe_run_id(&context.run_id),
        unit_id: safe_run_id(&format!("{}-{day}", job.job_id)),
        phase_id: job.phase_id.clone(),
        job_id: job.job_id.clone(),
        agent_id: job.agent_id.clone(),
        fanout: job.fanout.clone(),
        status: "planned".to_string(),
        bindings,
        unit_scope,
        packet_id: None,
        date: Some(day.to_string()),
        hour: None,
        required_artifacts: job.required_artifacts.clone(),
    }
}

fn unit_for_question(
    job: &ScheduledJob,
    context: &FanoutContext,
    question: &ArtifactHandle,
) -> ConcreteJobUnit {
    let run_id = safe_run_id(&context.run_id);
    let mut bindings = job.bindings.clone();
    bindings.insert("question_id".to_string(), question.key.clone());
    bindings.insert(
        "question_artifact_id".to_string(),
        question.artifact_id.clone(),
    );
    bindings.insert("question_path".to_string(), question.path.clone());
    if let Some((date, hour)) = parse_question_key_date_hour(&question.key) {
        bindings.insert("date".to_string(), date.clone());
        bindings.insert("hour".to_string(), hour.clone());
    }
    if bindings.get("parent_entry").map(String::as_str) == Some("question.parent_entry") {
        bindings.insert("parent_entry".to_string(), question.key.clone());
    }
    let mut unit_scope = BTreeMap::from([
        ("run.id".to_string(), run_id.clone()),
        ("question.id".to_string(), question.key.clone()),
        ("question.path".to_string(), question.path.clone()),
        (
            "question.artifact_id".to_string(),
            question.artifact_id.clone(),
        ),
    ]);
    if let Some((date, hour)) = parse_question_key_date_hour(&question.key) {
        unit_scope.insert("question.date".to_string(), date.clone());
        unit_scope.insert("question.hour".to_string(), hour.clone());
    }
    ConcreteJobUnit {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.concrete_job_unit.v1".to_string(),
        run_id,
        unit_id: safe_run_id(&format!("{}-{}", job.job_id, question.key)),
        phase_id: job.phase_id.clone(),
        job_id: job.job_id.clone(),
        agent_id: job.agent_id.clone(),
        fanout: job.fanout.clone(),
        status: "planned".to_string(),
        bindings,
        unit_scope,
        packet_id: None,
        date: parse_question_key_date_hour(&question.key).map(|(date, _)| date),
        hour: parse_question_key_date_hour(&question.key).map(|(_, hour)| hour),
        required_artifacts: job.required_artifacts.clone(),
    }
}

fn unit_for_once(job: &ScheduledJob, context: &FanoutContext) -> ConcreteJobUnit {
    let mut bindings = job.bindings.clone();
    let run_id = safe_run_id(&context.run_id);
    bindings.insert("run_id".to_string(), run_id.clone());
    let mut unit_scope = BTreeMap::from([("run.id".to_string(), run_id.clone())]);
    if let Some(day) = latest_refreshed_day(context) {
        unit_scope.insert("run.latest_refreshed_day".to_string(), day.clone());
        bindings.insert("latest_refreshed_day".to_string(), day.clone());
        if bindings.get("era").map(String::as_str) == Some("run.latest_refreshed_day") {
            bindings.insert("era".to_string(), day);
        }
    }
    ConcreteJobUnit {
        schema_version: CONTEXT_ENGINE_SCHEMA_VERSION,
        kind: "onecontext.context_engine.concrete_job_unit.v1".to_string(),
        run_id: safe_run_id(&context.run_id),
        unit_id: safe_run_id(&format!("{}-{}", job.job_id, context.run_id)),
        phase_id: job.phase_id.clone(),
        job_id: job.job_id.clone(),
        agent_id: job.agent_id.clone(),
        fanout: job.fanout.clone(),
        status: "planned".to_string(),
        bindings,
        unit_scope,
        packet_id: None,
        date: None,
        hour: None,
        required_artifacts: job.required_artifacts.clone(),
    }
}

fn question_units(job: &ScheduledJob, context: &FanoutContext) -> Vec<ConcreteJobUnit> {
    context
        .available_artifacts
        .iter()
        .filter(|artifact| {
            matches!(
                artifact.kind.as_str(),
                "hourly_answer_question" | "historian_question_mail"
            )
        })
        .map(|artifact| unit_for_question(job, context, artifact))
        .collect()
}

fn parse_question_key_date_hour(key: &str) -> Option<(String, String)> {
    let mut parts = key.split(['-', '_', ':']);
    let year = parts.next()?;
    let month = parts.next()?;
    let day = parts.next()?;
    let hour = parts.next()?;
    if year.len() == 4
        && month.len() == 2
        && day.len() == 2
        && hour.len() == 2
        && year.chars().all(|ch| ch.is_ascii_digit())
        && month.chars().all(|ch| ch.is_ascii_digit())
        && day.chars().all(|ch| ch.is_ascii_digit())
        && hour.chars().all(|ch| ch.is_ascii_digit())
    {
        Some((format!("{year}-{month}-{day}"), hour.to_string()))
    } else {
        None
    }
}

fn refreshed_days(context: &FanoutContext) -> Vec<String> {
    if !context.refreshed_days.is_empty() {
        return context.refreshed_days.clone();
    }
    context
        .selected_packets
        .iter()
        .map(|packet| packet.date.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn latest_refreshed_day(context: &FanoutContext) -> Option<String> {
    refreshed_days(context).into_iter().max()
}

fn hour_units_with_multiple_packets(
    job: &ScheduledJob,
    context: &FanoutContext,
) -> Vec<ConcreteJobUnit> {
    let mut by_hour = BTreeMap::<(String, String), Vec<&MaterializedSourcePacket>>::new();
    for packet in &context.selected_packets {
        by_hour
            .entry((packet.date.clone(), packet.hour.clone()))
            .or_default()
            .push(packet);
    }
    by_hour
        .into_iter()
        .filter(|(_, packets)| packets.len() > 1)
        .map(|((date, hour), _)| {
            let mut unit = unit_for_once(job, context);
            let hour_id = format!("{date}-{hour}");
            unit.unit_id = safe_run_id(&format!("{}-{hour_id}", job.job_id));
            unit.bindings.insert("date".to_string(), date.clone());
            unit.bindings.insert("hour".to_string(), hour.clone());
            unit.bindings.insert("hour_id".to_string(), hour_id.clone());
            unit.unit_scope.insert("hour.id".to_string(), hour_id);
            unit.unit_scope
                .insert("hour.date".to_string(), date.clone());
            unit.unit_scope
                .insert("hour.hour".to_string(), hour.clone());
            unit.date = Some(date);
            unit.hour = Some(hour);
            unit
        })
        .collect()
}
