use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorReadPosture {
    StableLocalRecord,
    AppCache,
    OfficialApi,
    LiveObservation,
    ManualImport,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConnectorAccessMode {
    FullDiskAccess,
    AppLogin,
    ApiToken,
    BrowserExtension,
    Accessibility,
    InputMonitoring,
    UserSelectedFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceConnectorSpec {
    pub connector_key: String,
    pub display_name: String,
    pub source_type: String,
    pub default_lane_key: String,
    pub default_stream_key: String,
    pub read_posture: ConnectorReadPosture,
    #[serde(default)]
    pub access_modes: Vec<ConnectorAccessMode>,
    #[serde(default)]
    pub initial_kinds: Vec<String>,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceProbeReport {
    pub connector_key: String,
    pub status: String,
    pub readable: bool,
    pub read_posture: ConnectorReadPosture,
    #[serde(default)]
    pub discovered_locations: Vec<SourceLocationProbe>,
    #[serde(default)]
    pub diagnostics: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceLocationProbe {
    pub location_kind: String,
    pub uri: String,
    pub canonicality: String,
    pub readable: bool,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectorValidationError {
    MissingField(&'static str),
    MetadataMustBeObject,
    EmptyAccessModeSet,
}

impl std::fmt::Display for ConnectorValidationError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingField(field) => write!(formatter, "missing required field {field}"),
            Self::MetadataMustBeObject => write!(formatter, "metadata must be a JSON object"),
            Self::EmptyAccessModeSet => {
                write!(formatter, "connector must declare at least one access mode")
            }
        }
    }
}

impl std::error::Error for ConnectorValidationError {}

impl SourceConnectorSpec {
    pub fn validate(&self) -> Result<(), ConnectorValidationError> {
        require_non_empty("connector_key", &self.connector_key)?;
        require_non_empty("display_name", &self.display_name)?;
        require_non_empty("source_type", &self.source_type)?;
        require_non_empty("default_lane_key", &self.default_lane_key)?;
        require_non_empty("default_stream_key", &self.default_stream_key)?;
        if self.access_modes.is_empty() {
            return Err(ConnectorValidationError::EmptyAccessModeSet);
        }
        if !self.metadata.is_object() {
            return Err(ConnectorValidationError::MetadataMustBeObject);
        }
        Ok(())
    }
}

pub fn default_source_connectors() -> Vec<SourceConnectorSpec> {
    vec![
        SourceConnectorSpec {
            connector_key: "codex.local_sessions".to_string(),
            display_name: "Codex Sessions".to_string(),
            source_type: "codex".to_string(),
            default_lane_key: "agents.sessions".to_string(),
            default_stream_key: "codex.local".to_string(),
            read_posture: ConnectorReadPosture::StableLocalRecord,
            access_modes: vec![ConnectorAccessMode::FullDiskAccess],
            initial_kinds: vec![
                "agent_session".to_string(),
                "agent_turn".to_string(),
                "agent_message".to_string(),
                "agent_tool_summary".to_string(),
                "agent_compaction".to_string(),
                "agent_prompt_snapshot".to_string(),
            ],
            metadata: Value::Object(Default::default()),
        },
        SourceConnectorSpec {
            connector_key: "claude.local_sessions".to_string(),
            display_name: "Claude Code Sessions".to_string(),
            source_type: "claude".to_string(),
            default_lane_key: "agents.sessions".to_string(),
            default_stream_key: "claude.local".to_string(),
            read_posture: ConnectorReadPosture::StableLocalRecord,
            access_modes: vec![ConnectorAccessMode::FullDiskAccess],
            initial_kinds: vec![
                "agent_session".to_string(),
                "agent_turn".to_string(),
                "agent_message".to_string(),
                "agent_tool_summary".to_string(),
                "agent_compaction".to_string(),
                "agent_prompt_snapshot".to_string(),
            ],
            metadata: Value::Object(Default::default()),
        },
        SourceConnectorSpec {
            connector_key: "imessage.chat_db".to_string(),
            display_name: "iMessage".to_string(),
            source_type: "imessage".to_string(),
            default_lane_key: "messages.imessage".to_string(),
            default_stream_key: "imessage.local".to_string(),
            read_posture: ConnectorReadPosture::StableLocalRecord,
            access_modes: vec![ConnectorAccessMode::FullDiskAccess],
            initial_kinds: vec![
                "imessage_message".to_string(),
                "imessage_attachment".to_string(),
            ],
            metadata: Value::Object(Default::default()),
        },
        SourceConnectorSpec {
            connector_key: "slack.desktop_or_api".to_string(),
            display_name: "Slack".to_string(),
            source_type: "slack".to_string(),
            default_lane_key: "slack.messages".to_string(),
            default_stream_key: "slack.workspace".to_string(),
            read_posture: ConnectorReadPosture::OfficialApi,
            access_modes: vec![
                ConnectorAccessMode::ApiToken,
                ConnectorAccessMode::FullDiskAccess,
                ConnectorAccessMode::AppLogin,
            ],
            initial_kinds: vec![
                "slack_message".to_string(),
                "slack_thread_event".to_string(),
            ],
            metadata: Value::Object(Default::default()),
        },
        SourceConnectorSpec {
            connector_key: "linear.api".to_string(),
            display_name: "Linear".to_string(),
            source_type: "linear".to_string(),
            default_lane_key: "linear.events".to_string(),
            default_stream_key: "linear.workspace".to_string(),
            read_posture: ConnectorReadPosture::OfficialApi,
            access_modes: vec![
                ConnectorAccessMode::ApiToken,
                ConnectorAccessMode::BrowserExtension,
            ],
            initial_kinds: vec![
                "linear_issue_event".to_string(),
                "linear_comment".to_string(),
            ],
            metadata: Value::Object(Default::default()),
        },
        SourceConnectorSpec {
            connector_key: "discord.desktop_or_observed".to_string(),
            display_name: "Discord".to_string(),
            source_type: "discord".to_string(),
            default_lane_key: "discord.messages".to_string(),
            default_stream_key: "discord.local".to_string(),
            read_posture: ConnectorReadPosture::AppCache,
            access_modes: vec![
                ConnectorAccessMode::FullDiskAccess,
                ConnectorAccessMode::AppLogin,
                ConnectorAccessMode::Accessibility,
            ],
            initial_kinds: vec![
                "discord_message".to_string(),
                "discord_channel_event".to_string(),
            ],
            metadata: Value::Object(Default::default()),
        },
        SourceConnectorSpec {
            connector_key: "desktop.semantic_observation".to_string(),
            display_name: "Desktop Semantic Observation".to_string(),
            source_type: "desktop_observation".to_string(),
            default_lane_key: "memory.observations".to_string(),
            default_stream_key: "desktop.semantic".to_string(),
            read_posture: ConnectorReadPosture::LiveObservation,
            access_modes: vec![
                ConnectorAccessMode::Accessibility,
                ConnectorAccessMode::InputMonitoring,
                ConnectorAccessMode::BrowserExtension,
            ],
            initial_kinds: vec![
                "scroll_document".to_string(),
                "semantic_delta".to_string(),
                "agent_observation_packet".to_string(),
            ],
            metadata: json!({
                "downstream_of": "macos.capture",
                "raw_pixels": "evidence_refs_only",
                "assumes_metadata": ["accessibility", "input_monitoring", "browser"]
            }),
        },
        SourceConnectorSpec {
            connector_key: "terminal.sessions".to_string(),
            display_name: "Terminal Sessions".to_string(),
            source_type: "terminal".to_string(),
            default_lane_key: "terminal.output".to_string(),
            default_stream_key: "terminal.local".to_string(),
            read_posture: ConnectorReadPosture::LiveObservation,
            access_modes: vec![
                ConnectorAccessMode::Accessibility,
                ConnectorAccessMode::FullDiskAccess,
            ],
            initial_kinds: vec![
                "terminal_command".to_string(),
                "terminal_output".to_string(),
            ],
            metadata: Value::Object(Default::default()),
        },
    ]
}

fn require_non_empty(field: &'static str, value: &str) -> Result<(), ConnectorValidationError> {
    if value.trim().is_empty() {
        Err(ConnectorValidationError::MissingField(field))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_connectors_validate() {
        let connectors = default_source_connectors();
        assert!(connectors.len() >= 8);
        for connector in connectors {
            connector.validate().unwrap();
        }
    }

    #[test]
    fn semantic_observation_connector_declares_live_desktop_inputs() {
        let connector = default_source_connectors()
            .into_iter()
            .find(|connector| connector.connector_key == "desktop.semantic_observation")
            .expect("semantic observation connector exists");

        assert_eq!(
            connector.read_posture,
            ConnectorReadPosture::LiveObservation
        );
        assert_eq!(connector.default_lane_key, "memory.observations");
        assert!(connector
            .access_modes
            .contains(&ConnectorAccessMode::Accessibility));
        assert!(connector
            .access_modes
            .contains(&ConnectorAccessMode::InputMonitoring));
        assert!(connector
            .initial_kinds
            .contains(&"agent_observation_packet".to_string()));
    }

    #[test]
    fn connector_requires_access_mode() {
        let mut connector = default_source_connectors().remove(0);
        connector.access_modes.clear();
        assert_eq!(
            connector.validate(),
            Err(ConnectorValidationError::EmptyAccessModeSet)
        );
    }

    #[test]
    fn source_probe_can_represent_partial_cache() {
        let probe = SourceProbeReport {
            connector_key: "discord.desktop_or_observed".to_string(),
            status: "partial".to_string(),
            readable: true,
            read_posture: ConnectorReadPosture::AppCache,
            discovered_locations: vec![SourceLocationProbe {
                location_kind: "app_cache".to_string(),
                uri: "~/Library/Application Support/discord".to_string(),
                canonicality: "partial".to_string(),
                readable: true,
                metadata: Value::Object(Default::default()),
            }],
            diagnostics: Value::Object(Default::default()),
        };

        assert!(probe.readable);
        assert_eq!(probe.discovered_locations[0].canonicality, "partial");
    }
}
