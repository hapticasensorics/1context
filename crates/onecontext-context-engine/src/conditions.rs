//! Condition evaluation for scheduled wiki-company jobs.

use crate::artifacts::ArtifactHandle;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionContext {
    #[serde(default)]
    pub available_artifacts: Vec<ArtifactHandle>,
    #[serde(default)]
    pub artifact_inventory_known: bool,
    #[serde(default)]
    pub facts: BTreeMap<String, bool>,
    #[serde(default)]
    pub named_flags: BTreeMap<String, bool>,
}

impl ConditionContext {
    pub fn with_known_artifacts(
        artifacts: impl IntoIterator<Item = ArtifactHandle>,
    ) -> ConditionContext {
        ConditionContext {
            available_artifacts: artifacts.into_iter().collect(),
            artifact_inventory_known: true,
            ..Default::default()
        }
    }

    pub fn with_fact(mut self, expression: impl Into<String>, value: bool) -> ConditionContext {
        self.facts.insert(expression.into(), value);
        self
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionStatus {
    True,
    False,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConditionEvaluation {
    pub expression: String,
    pub status: ConditionStatus,
    pub reason: String,
}

pub fn evaluate_condition(
    expression: Option<&str>,
    context: &ConditionContext,
) -> ConditionEvaluation {
    let Some(expression) = expression.map(str::trim).filter(|value| !value.is_empty()) else {
        return ConditionEvaluation {
            expression: String::new(),
            status: ConditionStatus::True,
            reason: "no condition".to_string(),
        };
    };

    if !is_condition_key(expression) {
        return unknown_eval(expression, "condition expression is not supported");
    }

    if let Some(value) = runtime_fact(expression, context) {
        return bool_eval(expression, value, "runtime condition fact");
    }

    if let Some(artifact_kind) = expression.strip_suffix(".ready") {
        return artifact_ready_eval(expression, artifact_kind, context);
    }

    unknown_eval(expression, "runtime condition fact was not supplied")
}

fn runtime_fact(expression: &str, context: &ConditionContext) -> Option<bool> {
    context
        .facts
        .get(expression)
        .or_else(|| context.named_flags.get(expression))
        .copied()
}

fn artifact_ready_eval(
    expression: &str,
    artifact_kind: &str,
    context: &ConditionContext,
) -> ConditionEvaluation {
    if !context.artifact_inventory_known {
        return unknown_eval(expression, "artifact inventory was not supplied");
    }
    let artifact_kinds = context
        .available_artifacts
        .iter()
        .map(|artifact| artifact.kind.as_str())
        .collect::<BTreeSet<_>>();
    bool_eval(
        expression,
        artifact_kinds.contains(artifact_kind),
        "artifact inventory",
    )
}

fn is_condition_key(value: &str) -> bool {
    value
        .split('.')
        .all(|segment| !segment.is_empty() && is_identifierish(segment))
}

fn is_identifierish(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !(first.is_ascii_alphabetic() || first == '_') {
        return false;
    }
    chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn unknown_eval(expression: &str, reason: &str) -> ConditionEvaluation {
    ConditionEvaluation {
        expression: expression.to_string(),
        status: ConditionStatus::Unknown,
        reason: reason.to_string(),
    }
}

fn bool_eval(expression: &str, value: bool, reason: &str) -> ConditionEvaluation {
    ConditionEvaluation {
        expression: expression.to_string(),
        status: if value {
            ConditionStatus::True
        } else {
            ConditionStatus::False
        },
        reason: reason.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn artifact(kind: &str) -> ArtifactHandle {
        ArtifactHandle {
            artifact_id: format!("{kind}-1"),
            kind: kind.to_string(),
            run_id: "run".to_string(),
            key: "key".to_string(),
            path: format!("/tmp/{kind}.json"),
            content_sha256: "sha256".to_string(),
            created_at: "2026-06-07T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn no_condition_is_true() {
        let evaluation = evaluate_condition(None, &ConditionContext::default());

        assert_eq!(evaluation.status, ConditionStatus::True);
        assert_eq!(evaluation.reason, "no condition");
    }

    #[test]
    fn ready_conditions_use_known_artifact_inventory() {
        let ready = evaluate_condition(
            Some("daily_editorial_artifacts.ready"),
            &ConditionContext::with_known_artifacts([artifact("daily_editorial_artifacts")]),
        );
        assert_eq!(ready.status, ConditionStatus::True);

        let missing = evaluate_condition(
            Some("daily_editorial_artifacts.ready"),
            &ConditionContext::with_known_artifacts([]),
        );
        assert_eq!(missing.status, ConditionStatus::False);

        let unknown = evaluate_condition(
            Some("daily_editorial_artifacts.ready"),
            &ConditionContext::default(),
        );
        assert_eq!(unknown.status, ConditionStatus::Unknown);
        assert_eq!(unknown.reason, "artifact inventory was not supplied");
    }

    #[test]
    fn runtime_fact_conditions_are_true_false_or_unknown() {
        let context = ConditionContext::default()
            .with_fact("packet.hour_has_multiple_scribe_artifacts", true)
            .with_fact("for_you_proposals.pending", false)
            .with_fact("recent_page_or_concept_changes", true);

        assert_eq!(
            evaluate_condition(Some("packet.hour_has_multiple_scribe_artifacts"), &context).status,
            ConditionStatus::True
        );
        assert_eq!(
            evaluate_condition(Some("for_you_proposals.pending"), &context).status,
            ConditionStatus::False
        );
        assert_eq!(
            evaluate_condition(Some("recent_page_or_concept_changes"), &context).status,
            ConditionStatus::True
        );
        assert_eq!(
            evaluate_condition(Some("your_context_proposals.pending"), &context).status,
            ConditionStatus::Unknown
        );
    }

    #[test]
    fn named_flags_remain_exact_runtime_facts() {
        let mut context = ConditionContext::default();
        context
            .named_flags
            .insert("custom_condition.ready".to_string(), true);

        let evaluation = evaluate_condition(Some("custom_condition.ready"), &context);

        assert_eq!(evaluation.status, ConditionStatus::True);
        assert_eq!(evaluation.reason, "runtime condition fact");
    }

    #[test]
    fn unsupported_expression_shapes_are_unknown() {
        let evaluation = evaluate_condition(
            Some("for_you_proposals.pending && daily_editorial_artifacts.ready"),
            &ConditionContext::default(),
        );

        assert_eq!(evaluation.status, ConditionStatus::Unknown);
        assert_eq!(evaluation.reason, "condition expression is not supported");
    }
}
