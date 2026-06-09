use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterPolicyReason {
    Allowed,
    MissingThread,
    MissingActiveTurn,
    AgentRetired,
    LeaseStale,
    HookUntrusted,
    ToolNotVisible,
    BodyLikeDataWouldPersist,
    CodexCapabilityMissing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdapterPolicyDecision {
    pub allowed: bool,
    pub reason: AdapterPolicyReason,
    pub message: String,
}

impl AdapterPolicyDecision {
    pub fn allow(message: impl Into<String>) -> Self {
        Self {
            allowed: true,
            reason: AdapterPolicyReason::Allowed,
            message: message.into(),
        }
    }

    pub fn deny(reason: AdapterPolicyReason, message: impl Into<String>) -> Self {
        Self {
            allowed: false,
            reason,
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRedactionCheck {
    pub allowed: bool,
    pub body_like_paths: Vec<String>,
}

impl EvidenceRedactionCheck {
    pub fn into_policy_decision(self) -> AdapterPolicyDecision {
        if self.allowed {
            AdapterPolicyDecision::allow("adapter evidence is redacted for persistence")
        } else {
            AdapterPolicyDecision::deny(
                AdapterPolicyReason::BodyLikeDataWouldPersist,
                format!(
                    "adapter evidence contains unredacted body-like data at {}",
                    self.body_like_paths.join(", ")
                ),
            )
        }
    }
}

pub fn check_evidence_redaction(evidence: &Value) -> EvidenceRedactionCheck {
    let mut body_like_paths = Vec::new();
    collect_unredacted_body_like_paths(evidence, "$", None, &mut body_like_paths);
    EvidenceRedactionCheck {
        allowed: body_like_paths.is_empty(),
        body_like_paths,
    }
}

pub fn deny_unredacted_evidence(evidence: &Value) -> AdapterPolicyDecision {
    check_evidence_redaction(evidence).into_policy_decision()
}

pub fn require_thread(thread_id: Option<&str>) -> AdapterPolicyDecision {
    match thread_id {
        Some(thread_id) if !thread_id.trim().is_empty() => {
            AdapterPolicyDecision::allow("codex thread is present")
        }
        _ => AdapterPolicyDecision::deny(
            AdapterPolicyReason::MissingThread,
            "codex thread is required for this adapter action",
        ),
    }
}

pub fn require_active_turn(active_turn_id: Option<&str>) -> AdapterPolicyDecision {
    match active_turn_id {
        Some(active_turn_id) if !active_turn_id.trim().is_empty() => {
            AdapterPolicyDecision::allow("active Codex turn is present")
        }
        _ => AdapterPolicyDecision::deny(
            AdapterPolicyReason::MissingActiveTurn,
            "active Codex turn is required for steering",
        ),
    }
}

pub fn require_tool_visible(tool_name: &str, visible_toolsets: &[String]) -> AdapterPolicyDecision {
    let Some(toolset) = infer_toolset_for_tool(tool_name) else {
        return AdapterPolicyDecision::deny(
            AdapterPolicyReason::ToolNotVisible,
            format!("{tool_name} is not a 1Context public tool"),
        );
    };

    if visible_toolsets
        .iter()
        .any(|visible| visible.trim() == toolset)
    {
        AdapterPolicyDecision::allow(format!("{tool_name} is visible through {toolset}"))
    } else {
        AdapterPolicyDecision::deny(
            AdapterPolicyReason::ToolNotVisible,
            format!("{tool_name} is not visible to the current agent"),
        )
    }
}

pub fn infer_toolset_for_tool(tool_name: &str) -> Option<&'static str> {
    let tool_name = tool_name.trim();

    if tool_name == "toolset-mail" || tool_name.starts_with("toolset-mail.") {
        return Some("toolset-mail");
    }
    if tool_name == "toolset-wiki" || tool_name.starts_with("toolset-wiki.") {
        return Some("toolset-wiki");
    }

    if tool_name.starts_with("wiki.agent.")
        || tool_name.starts_with("wiki.mail.")
        || tool_name.starts_with("wiki.notify.")
    {
        return Some("toolset-mail");
    }

    if matches!(
        tool_name,
        "wiki.status" | "wiki.list" | "wiki.validate" | "wiki.publish"
    ) || tool_name.starts_with("wiki.page.")
        || tool_name.starts_with("wiki.asset.")
        || tool_name.starts_with("wiki.reference.")
        || tool_name.starts_with("wiki.talk.")
        || tool_name.starts_with("wiki.publish.")
    {
        return Some("toolset-wiki");
    }

    let normalized_mcp_name = tool_name.replace('-', "_");
    if normalized_mcp_name.starts_with("mcp__onecontext__wiki_agent_")
        || normalized_mcp_name.starts_with("mcp__onecontext__wiki_mail_")
        || normalized_mcp_name.starts_with("mcp__onecontext__wiki_notify_")
    {
        return Some("toolset-mail");
    }
    if normalized_mcp_name.starts_with("mcp__onecontext__wiki_page_")
        || normalized_mcp_name.starts_with("mcp__onecontext__wiki_asset_")
        || normalized_mcp_name.starts_with("mcp__onecontext__wiki_reference_")
        || normalized_mcp_name.starts_with("mcp__onecontext__wiki_talk_")
        || normalized_mcp_name.starts_with("mcp__onecontext__wiki_publish")
        || matches!(
            normalized_mcp_name.as_str(),
            "mcp__onecontext__wiki_status"
                | "mcp__onecontext__wiki_list"
                | "mcp__onecontext__wiki_validate"
        )
    {
        return Some("toolset-wiki");
    }

    None
}

pub fn require_trusted_hooks(
    require_managed_hooks: bool,
    managed_hooks_installed: bool,
    project_trusted: bool,
) -> AdapterPolicyDecision {
    if require_managed_hooks && !managed_hooks_installed {
        return AdapterPolicyDecision::deny(
            AdapterPolicyReason::HookUntrusted,
            "managed hooks are required but not installed",
        );
    }

    if !managed_hooks_installed && !project_trusted {
        return AdapterPolicyDecision::deny(
            AdapterPolicyReason::HookUntrusted,
            "project hooks are not trusted for safety proof",
        );
    }

    AdapterPolicyDecision::allow("hook posture is trusted for adapter proof")
}

fn collect_unredacted_body_like_paths(
    value: &Value,
    path: &str,
    key: Option<&str>,
    body_like_paths: &mut Vec<String>,
) {
    if key.is_some_and(is_body_like_evidence_key) && !is_allowed_redacted_value(value) {
        body_like_paths.push(path.to_string());
        return;
    }

    match value {
        Value::Array(values) => {
            for (index, value) in values.iter().enumerate() {
                collect_unredacted_body_like_paths(
                    value,
                    &format!("{path}[{index}]"),
                    None,
                    body_like_paths,
                );
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                collect_unredacted_body_like_paths(
                    value,
                    &format!("{path}.{key}"),
                    Some(key),
                    body_like_paths,
                );
            }
        }
        _ => {}
    }
}

fn is_allowed_redacted_value(value: &Value) -> bool {
    matches!(value, Value::Null)
        || value
            .as_str()
            .is_some_and(|value| matches!(value, "[redacted]" | "<redacted>"))
}

fn is_body_like_evidence_key(key: &str) -> bool {
    let normalized = key.replace(['-', ' '], "_").to_ascii_lowercase();

    if normalized.ends_with("_sha256")
        || normalized.ends_with("_hash")
        || normalized.ends_with("_digest")
        || normalized.ends_with("_id")
        || normalized.ends_with("_count")
        || normalized.ends_with("_status")
        || normalized.ends_with("_kind")
        || normalized.ends_with("_summary")
    {
        return false;
    }

    matches!(
        normalized.as_str(),
        "body"
            | "mail_body"
            | "message_body"
            | "full_body"
            | "content"
            | "raw_content"
            | "message"
            | "prompt"
            | "raw_prompt"
            | "transcript"
            | "tool_output"
            | "output"
            | "secret"
            | "api_key"
            | "token"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn allows_hashes_and_redacted_body_like_fields() {
        let evidence = json!({
            "delivery_id": "delivery-1",
            "body_sha256": "abc123",
            "body": "[redacted]",
            "tool_output": null,
            "items": [{ "content": "<redacted>" }]
        });

        let check = check_evidence_redaction(&evidence);

        assert!(check.allowed);
        assert!(check.body_like_paths.is_empty());
    }

    #[test]
    fn denies_body_like_evidence_before_persistence() {
        let evidence = json!({
            "delivery_id": "delivery-1",
            "body": "the mail body should not persist",
            "nested": { "raw_prompt": "please do the thing" }
        });

        let check = check_evidence_redaction(&evidence);

        assert!(!check.allowed);
        assert_eq!(check.body_like_paths, vec!["$.body", "$.nested.raw_prompt"]);
        assert_eq!(
            check.into_policy_decision().reason,
            AdapterPolicyReason::BodyLikeDataWouldPersist
        );
    }

    #[test]
    fn requires_visible_toolset_prefix() {
        let visible = vec!["toolset-mail".to_string(), "toolset-wiki".to_string()];

        assert!(require_tool_visible("toolset-mail.open", &visible).allowed);
        assert!(require_tool_visible("wiki.mail.open", &visible).allowed);
        assert!(require_tool_visible("wiki.agent.inbox", &visible).allowed);
        assert!(require_tool_visible("wiki.page.patch_body", &visible).allowed);
        assert!(require_tool_visible("mcp__onecontext__wiki_mail_open", &visible).allowed);
        assert_eq!(
            require_tool_visible("host.dispatch", &visible).reason,
            AdapterPolicyReason::ToolNotVisible
        );
    }

    #[test]
    fn denies_valid_tool_when_its_toolset_is_not_visible() {
        let visible = vec!["toolset-wiki".to_string()];

        assert_eq!(
            require_tool_visible("wiki.mail.open", &visible).reason,
            AdapterPolicyReason::ToolNotVisible
        );
        assert!(require_tool_visible("wiki.page.open", &visible).allowed);
    }
}
