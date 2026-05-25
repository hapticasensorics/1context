use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CodexHookEventName {
    SessionStart,
    UserPromptSubmit,
    PreToolUse,
    PermissionRequest,
    PostToolUse,
    Stop,
    PreCompact,
    PostCompact,
    SubagentStart,
    SubagentStop,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexHookIntentAction {
    RecordOnly,
    AddContext,
    DenyTool,
    RewriteToolInput,
    QueueInjection,
    ContinueTurn,
    SnapshotContext,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexHookHandlerType {
    Command,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexHookSourceKind {
    UserConfig,
    ProjectConfig,
    PluginBundledConfig,
    ManagedConfig,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexHookTrustStatus {
    Trusted,
    Untrusted,
    Changed,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexHookRecord {
    pub key: String,
    pub event_name: CodexHookEventName,
    pub handler_type: CodexHookHandlerType,
    pub matcher: Option<String>,
    pub command: String,
    pub source_kind: CodexHookSourceKind,
    pub source_path: Option<String>,
    pub timeout_sec: Option<u32>,
    pub status_message: Option<String>,
    pub is_managed: bool,
    pub enabled: bool,
    pub current_hash: Option<String>,
    pub trust_status: CodexHookTrustStatus,
    pub runnable: bool,
}

impl CodexHookRecord {
    pub fn is_supported_handler(&self) -> bool {
        self.handler_type == CodexHookHandlerType::Command
    }

    pub fn is_trusted_by_policy(&self) -> bool {
        self.is_managed || self.trust_status == CodexHookTrustStatus::Trusted
    }

    pub fn is_trusted_runnable(&self) -> bool {
        self.enabled && self.runnable && self.is_supported_handler() && self.is_trusted_by_policy()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexHookRegistryObservation {
    pub records: Vec<CodexHookRecord>,
    pub required_events: Vec<CodexHookEventName>,
    pub trust_state: CodexHookRegistryTrustState,
    pub observed_at: DateTime<Utc>,
}

impl CodexHookRegistryObservation {
    pub fn new(
        records: Vec<CodexHookRecord>,
        required_events: Vec<CodexHookEventName>,
        observed_at: DateTime<Utc>,
    ) -> Self {
        let trust_state = CodexHookRegistryTrustState::from_records(&records, &required_events);

        Self {
            records,
            required_events,
            trust_state,
            observed_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexHookRegistryTrustState {
    pub usable_for_mail_safety: bool,
    pub trusted_events: Vec<CodexHookEventName>,
    pub missing_required_events: Vec<CodexHookEventName>,
    pub untrusted_required_keys: Vec<String>,
    pub disabled_required_keys: Vec<String>,
    pub unrunnable_required_keys: Vec<String>,
    pub unsupported_required_keys: Vec<String>,
    pub warnings: Vec<String>,
}

impl CodexHookRegistryTrustState {
    pub fn from_records(
        records: &[CodexHookRecord],
        required_events: &[CodexHookEventName],
    ) -> Self {
        let mut trusted_events = Vec::new();
        let mut missing_required_events = Vec::new();
        let mut untrusted_required_keys = BTreeSet::new();
        let mut disabled_required_keys = BTreeSet::new();
        let mut unrunnable_required_keys = BTreeSet::new();
        let mut unsupported_required_keys = BTreeSet::new();
        let mut warnings = Vec::new();

        for event in required_events {
            let matching: Vec<&CodexHookRecord> = records
                .iter()
                .filter(|record| &record.event_name == event)
                .collect();

            if matching.is_empty() {
                missing_required_events.push(event.clone());
                warnings.push(format!("missing required hook for {event:?}"));
                continue;
            }

            let has_trusted_runnable = matching.iter().any(|record| record.is_trusted_runnable());

            if has_trusted_runnable {
                trusted_events.push(event.clone());
                continue;
            }

            for record in matching {
                if !record.enabled {
                    disabled_required_keys.insert(record.key.clone());
                }
                if !record.runnable {
                    unrunnable_required_keys.insert(record.key.clone());
                }
                if !record.is_supported_handler() {
                    unsupported_required_keys.insert(record.key.clone());
                }
                if !record.is_trusted_by_policy() {
                    untrusted_required_keys.insert(record.key.clone());
                }
            }

            warnings.push(format!(
                "required hook for {event:?} is present but not trusted and runnable"
            ));
        }

        let untrusted_required_keys: Vec<String> = untrusted_required_keys.into_iter().collect();
        let disabled_required_keys: Vec<String> = disabled_required_keys.into_iter().collect();
        let unrunnable_required_keys: Vec<String> = unrunnable_required_keys.into_iter().collect();
        let unsupported_required_keys: Vec<String> =
            unsupported_required_keys.into_iter().collect();
        let usable_for_mail_safety = missing_required_events.is_empty()
            && untrusted_required_keys.is_empty()
            && disabled_required_keys.is_empty()
            && unrunnable_required_keys.is_empty()
            && unsupported_required_keys.is_empty();

        Self {
            usable_for_mail_safety,
            trusted_events,
            missing_required_events,
            untrusted_required_keys,
            disabled_required_keys,
            unrunnable_required_keys,
            unsupported_required_keys,
            warnings,
        }
    }
}

pub fn required_mail_hook_events() -> Vec<CodexHookEventName> {
    vec![
        CodexHookEventName::SessionStart,
        CodexHookEventName::PreToolUse,
        CodexHookEventName::PostToolUse,
        CodexHookEventName::Stop,
    ]
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CodexHookIntentRefs {
    pub agent_id: Option<String>,
    pub notification_id: Option<String>,
    pub delivery_id: Option<String>,
    pub message_id: Option<String>,
    pub claim_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CodexHookIntentCodexRefs {
    pub session_id: Option<String>,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub tool_name: Option<String>,
    pub tool_use_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexHookIntent {
    pub intent_id: String,
    pub hook_event_name: CodexHookEventName,
    pub codex: CodexHookIntentCodexRefs,
    pub action: CodexHookIntentAction,
    pub refs: CodexHookIntentRefs,
    pub input_sha256: Option<String>,
    pub output_sha256: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl CodexHookIntent {
    pub fn session_start_add_context(
        intent_id: impl Into<String>,
        codex: CodexHookIntentCodexRefs,
        refs: CodexHookIntentRefs,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self::base(
            intent_id,
            CodexHookEventName::SessionStart,
            codex,
            CodexHookIntentAction::AddContext,
            refs,
            created_at,
        )
    }

    pub fn pre_tool_use_record(
        intent_id: impl Into<String>,
        codex: CodexHookIntentCodexRefs,
        refs: CodexHookIntentRefs,
        input_sha256: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let mut intent = Self::base(
            intent_id,
            CodexHookEventName::PreToolUse,
            codex,
            CodexHookIntentAction::RecordOnly,
            refs,
            created_at,
        );
        intent.input_sha256 = input_sha256;
        intent
    }

    pub fn pre_tool_use_deny(
        intent_id: impl Into<String>,
        codex: CodexHookIntentCodexRefs,
        refs: CodexHookIntentRefs,
        input_sha256: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let mut intent = Self::base(
            intent_id,
            CodexHookEventName::PreToolUse,
            codex,
            CodexHookIntentAction::DenyTool,
            refs,
            created_at,
        );
        intent.input_sha256 = input_sha256;
        intent
    }

    pub fn post_tool_use_record(
        intent_id: impl Into<String>,
        codex: CodexHookIntentCodexRefs,
        refs: CodexHookIntentRefs,
        input_sha256: Option<String>,
        output_sha256: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let mut intent = Self::base(
            intent_id,
            CodexHookEventName::PostToolUse,
            codex,
            CodexHookIntentAction::RecordOnly,
            refs,
            created_at,
        );
        intent.input_sha256 = input_sha256;
        intent.output_sha256 = output_sha256;
        intent
    }

    pub fn post_tool_use_queue_injection(
        intent_id: impl Into<String>,
        codex: CodexHookIntentCodexRefs,
        refs: CodexHookIntentRefs,
        input_sha256: Option<String>,
        output_sha256: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let mut intent = Self::base(
            intent_id,
            CodexHookEventName::PostToolUse,
            codex,
            CodexHookIntentAction::QueueInjection,
            refs,
            created_at,
        );
        intent.input_sha256 = input_sha256;
        intent.output_sha256 = output_sha256;
        intent
    }

    pub fn stop_record(
        intent_id: impl Into<String>,
        codex: CodexHookIntentCodexRefs,
        refs: CodexHookIntentRefs,
        output_sha256: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let mut intent = Self::base(
            intent_id,
            CodexHookEventName::Stop,
            codex,
            CodexHookIntentAction::RecordOnly,
            refs,
            created_at,
        );
        intent.output_sha256 = output_sha256;
        intent
    }

    pub fn stop_continue(
        intent_id: impl Into<String>,
        codex: CodexHookIntentCodexRefs,
        refs: CodexHookIntentRefs,
        output_sha256: Option<String>,
        created_at: DateTime<Utc>,
    ) -> Self {
        let mut intent = Self::base(
            intent_id,
            CodexHookEventName::Stop,
            codex,
            CodexHookIntentAction::ContinueTurn,
            refs,
            created_at,
        );
        intent.output_sha256 = output_sha256;
        intent
    }

    fn base(
        intent_id: impl Into<String>,
        hook_event_name: CodexHookEventName,
        codex: CodexHookIntentCodexRefs,
        action: CodexHookIntentAction,
        refs: CodexHookIntentRefs,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            intent_id: intent_id.into(),
            hook_event_name,
            codex,
            action,
            refs,
            input_sha256: None,
            output_sha256: None,
            created_at,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexStopGuardBudget {
    pub max_continuations: u32,
    pub used_continuations: u32,
}

impl CodexStopGuardBudget {
    pub const DEFAULT_MAX_CONTINUATIONS: u32 = 2;

    pub fn new(max_continuations: u32, used_continuations: u32) -> Self {
        Self {
            max_continuations,
            used_continuations,
        }
    }

    pub fn remaining(&self) -> u32 {
        self.max_continuations
            .saturating_sub(self.used_continuations)
    }

    pub fn can_continue(&self) -> bool {
        self.remaining() > 0
    }
}

impl Default for CodexStopGuardBudget {
    fn default() -> Self {
        Self {
            max_continuations: Self::DEFAULT_MAX_CONTINUATIONS,
            used_continuations: 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexStopGuardContext {
    pub agent_id: Option<String>,
    pub delivery_id: Option<String>,
    pub message_id: Option<String>,
    pub claim_id: Option<String>,
}

impl CodexStopGuardContext {
    pub fn has_unresolved_claim(&self) -> bool {
        self.delivery_id.is_some() || self.claim_id.is_some()
    }

    pub fn intent_refs(&self) -> CodexHookIntentRefs {
        CodexHookIntentRefs {
            agent_id: self.agent_id.clone(),
            notification_id: None,
            delivery_id: self.delivery_id.clone(),
            message_id: self.message_id.clone(),
            claim_id: self.claim_id.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexStopGuardDecision {
    AllowStop,
    ContinueTurn,
    EscalateAndAllowStop,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexStopGuardPlan {
    pub decision: CodexStopGuardDecision,
    pub action: CodexHookIntentAction,
    pub reason: String,
    pub remaining_continuations: u32,
    pub next_used_continuations: u32,
    pub warning_event_kind: Option<String>,
    pub needs_supervisor: bool,
}

impl CodexStopGuardPlan {
    pub fn plan(context: &CodexStopGuardContext, budget: &CodexStopGuardBudget) -> Self {
        if !context.has_unresolved_claim() {
            return Self {
                decision: CodexStopGuardDecision::AllowStop,
                action: CodexHookIntentAction::RecordOnly,
                reason: "no unresolved claimed delivery".to_string(),
                remaining_continuations: budget.remaining(),
                next_used_continuations: budget.used_continuations,
                warning_event_kind: None,
                needs_supervisor: false,
            };
        }

        if budget.can_continue() {
            let next_used_continuations = budget.used_continuations.saturating_add(1);
            return Self {
                decision: CodexStopGuardDecision::ContinueTurn,
                action: CodexHookIntentAction::ContinueTurn,
                reason: stop_guard_instruction(context),
                remaining_continuations: budget
                    .max_continuations
                    .saturating_sub(next_used_continuations),
                next_used_continuations,
                warning_event_kind: None,
                needs_supervisor: false,
            };
        }

        Self {
            decision: CodexStopGuardDecision::EscalateAndAllowStop,
            action: CodexHookIntentAction::RecordOnly,
            reason: "stop guard continuation budget exhausted".to_string(),
            remaining_continuations: 0,
            next_used_continuations: budget.used_continuations,
            warning_event_kind: Some("mail.warning.unclosed_claim".to_string()),
            needs_supervisor: true,
        }
    }

    pub fn stop_intent(
        &self,
        intent_id: impl Into<String>,
        codex: CodexHookIntentCodexRefs,
        refs: CodexHookIntentRefs,
        output_sha256: Option<String>,
        created_at: DateTime<Utc>,
    ) -> CodexHookIntent {
        if self.action == CodexHookIntentAction::ContinueTurn {
            CodexHookIntent::stop_continue(intent_id, codex, refs, output_sha256, created_at)
        } else {
            CodexHookIntent::stop_record(intent_id, codex, refs, output_sha256, created_at)
        }
    }
}

fn stop_guard_instruction(context: &CodexStopGuardContext) -> String {
    match &context.delivery_id {
        Some(delivery_id) => format!(
            "claimed delivery {delivery_id} is unresolved; reply and mark it done, snooze it, reject it, or defer it with a reason before stopping"
        ),
        None => "claimed mail is unresolved; reply and mark it done, snooze it, reject it, or defer it with a reason before stopping".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 25, 12, 0, 0).unwrap()
    }

    fn hook_record(key: &str, event_name: CodexHookEventName) -> CodexHookRecord {
        CodexHookRecord {
            key: key.to_string(),
            event_name,
            handler_type: CodexHookHandlerType::Command,
            matcher: None,
            command: "onecontext-codex-adapter hook".to_string(),
            source_kind: CodexHookSourceKind::ManagedConfig,
            source_path: Some("/managed/1context/hooks.json".to_string()),
            timeout_sec: Some(5),
            status_message: None,
            is_managed: true,
            enabled: true,
            current_hash: Some("sha256:abc".to_string()),
            trust_status: CodexHookTrustStatus::Trusted,
            runnable: true,
        }
    }

    #[test]
    fn registry_observation_marks_required_managed_hooks_usable() {
        let records: Vec<CodexHookRecord> = required_mail_hook_events()
            .into_iter()
            .map(|event| hook_record(&format!("{event:?}"), event))
            .collect();

        let observation =
            CodexHookRegistryObservation::new(records, required_mail_hook_events(), now());

        assert!(observation.trust_state.usable_for_mail_safety);
        assert_eq!(observation.trust_state.trusted_events.len(), 4);
        assert!(observation.trust_state.warnings.is_empty());
    }

    #[test]
    fn registry_observation_reports_missing_and_untrusted_required_hooks() {
        let mut pre_tool = hook_record("pre-tool", CodexHookEventName::PreToolUse);
        pre_tool.is_managed = false;
        pre_tool.trust_status = CodexHookTrustStatus::Changed;

        let observation = CodexHookRegistryObservation::new(
            vec![
                hook_record("session-start", CodexHookEventName::SessionStart),
                pre_tool,
                hook_record("post-tool", CodexHookEventName::PostToolUse),
            ],
            required_mail_hook_events(),
            now(),
        );

        assert!(!observation.trust_state.usable_for_mail_safety);
        assert_eq!(
            observation.trust_state.missing_required_events,
            vec![CodexHookEventName::Stop]
        );
        assert_eq!(
            observation.trust_state.untrusted_required_keys,
            vec!["pre-tool".to_string()]
        );
    }

    #[test]
    fn intent_helpers_set_expected_events_and_actions() {
        let codex = CodexHookIntentCodexRefs {
            session_id: Some("session_1".to_string()),
            thread_id: Some("thread_1".to_string()),
            turn_id: Some("turn_1".to_string()),
            tool_name: Some("wiki.mail.open".to_string()),
            tool_use_id: Some("tool_1".to_string()),
        };
        let refs = CodexHookIntentRefs {
            agent_id: Some("agent_1".to_string()),
            delivery_id: Some("delivery_1".to_string()),
            ..CodexHookIntentRefs::default()
        };

        let session = CodexHookIntent::session_start_add_context(
            "intent_session",
            codex.clone(),
            refs.clone(),
            now(),
        );
        let pre = CodexHookIntent::pre_tool_use_deny(
            "intent_pre",
            codex.clone(),
            refs.clone(),
            Some("input_hash".to_string()),
            now(),
        );
        let post = CodexHookIntent::post_tool_use_queue_injection(
            "intent_post",
            codex.clone(),
            refs.clone(),
            Some("input_hash".to_string()),
            Some("output_hash".to_string()),
            now(),
        );
        let stop = CodexHookIntent::stop_continue(
            "intent_stop",
            codex,
            refs,
            Some("last_message_hash".to_string()),
            now(),
        );

        assert_eq!(session.hook_event_name, CodexHookEventName::SessionStart);
        assert_eq!(session.action, CodexHookIntentAction::AddContext);
        assert_eq!(pre.hook_event_name, CodexHookEventName::PreToolUse);
        assert_eq!(pre.action, CodexHookIntentAction::DenyTool);
        assert_eq!(post.hook_event_name, CodexHookEventName::PostToolUse);
        assert_eq!(post.action, CodexHookIntentAction::QueueInjection);
        assert_eq!(stop.hook_event_name, CodexHookEventName::Stop);
        assert_eq!(stop.action, CodexHookIntentAction::ContinueTurn);
    }

    #[test]
    fn stop_guard_continues_unresolved_claim_while_budget_remains() {
        let context = CodexStopGuardContext {
            agent_id: Some("agent_1".to_string()),
            delivery_id: Some("delivery_1".to_string()),
            message_id: Some("message_1".to_string()),
            claim_id: Some("claim_1".to_string()),
        };
        let plan = CodexStopGuardPlan::plan(&context, &CodexStopGuardBudget::new(2, 1));

        assert_eq!(plan.decision, CodexStopGuardDecision::ContinueTurn);
        assert_eq!(plan.action, CodexHookIntentAction::ContinueTurn);
        assert_eq!(plan.remaining_continuations, 0);
        assert_eq!(plan.next_used_continuations, 2);
        assert!(plan.reason.contains("delivery_1"));
    }

    #[test]
    fn stop_guard_allows_stop_and_escalates_after_budget_exhaustion() {
        let context = CodexStopGuardContext {
            agent_id: Some("agent_1".to_string()),
            delivery_id: Some("delivery_1".to_string()),
            message_id: Some("message_1".to_string()),
            claim_id: Some("claim_1".to_string()),
        };
        let plan = CodexStopGuardPlan::plan(&context, &CodexStopGuardBudget::new(2, 2));

        assert_eq!(plan.decision, CodexStopGuardDecision::EscalateAndAllowStop);
        assert_eq!(plan.action, CodexHookIntentAction::RecordOnly);
        assert_eq!(
            plan.warning_event_kind,
            Some("mail.warning.unclosed_claim".to_string())
        );
        assert!(plan.needs_supervisor);

        let intent = plan.stop_intent(
            "intent_stop",
            CodexHookIntentCodexRefs::default(),
            context.intent_refs(),
            None,
            now(),
        );
        assert_eq!(intent.action, CodexHookIntentAction::RecordOnly);
        assert_eq!(intent.refs.delivery_id, Some("delivery_1".to_string()));
    }

    #[test]
    fn stop_guard_allows_stop_without_unresolved_claim() {
        let context = CodexStopGuardContext {
            agent_id: Some("agent_1".to_string()),
            delivery_id: None,
            message_id: None,
            claim_id: None,
        };
        let plan = CodexStopGuardPlan::plan(&context, &CodexStopGuardBudget::default());

        assert_eq!(plan.decision, CodexStopGuardDecision::AllowStop);
        assert_eq!(plan.action, CodexHookIntentAction::RecordOnly);
        assert_eq!(plan.next_used_continuations, 0);
        assert!(!plan.needs_supervisor);
    }
}
