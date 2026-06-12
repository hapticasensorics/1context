use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppServerMethod {
    Initialize,
    ThreadStart,
    ThreadResume,
    ThreadLoadedList,
    TurnStart,
    TurnSteer,
    TurnInterrupt,
    ThreadInjectItems,
}

impl AppServerMethod {
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::Initialize => "initialize",
            Self::ThreadStart => "thread/start",
            Self::ThreadResume => "thread/resume",
            Self::ThreadLoadedList => "thread/loaded/list",
            Self::TurnStart => "turn/start",
            Self::TurnSteer => "turn/steer",
            Self::TurnInterrupt => "turn/interrupt",
            Self::ThreadInjectItems => "thread/inject_items",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppServerCapability {
    StartThread,
    ResumeThread,
    DiscoverLoadedThreads,
    StartTurn,
    SteerTurn,
    InterruptTurn,
    InjectItems,
}

impl AppServerCapability {
    pub fn required_capabilities() -> &'static [Self] {
        &[
            Self::StartThread,
            Self::ResumeThread,
            Self::DiscoverLoadedThreads,
            Self::StartTurn,
            Self::SteerTurn,
            Self::InterruptTurn,
            Self::InjectItems,
        ]
    }

    pub fn required_methods(self) -> &'static [AppServerMethod] {
        match self {
            Self::StartThread => &[AppServerMethod::Initialize, AppServerMethod::ThreadStart],
            Self::ResumeThread => &[AppServerMethod::Initialize, AppServerMethod::ThreadResume],
            Self::DiscoverLoadedThreads => &[
                AppServerMethod::Initialize,
                AppServerMethod::ThreadLoadedList,
            ],
            Self::StartTurn => &[AppServerMethod::Initialize, AppServerMethod::TurnStart],
            Self::SteerTurn => &[AppServerMethod::Initialize, AppServerMethod::TurnSteer],
            Self::InterruptTurn => &[AppServerMethod::Initialize, AppServerMethod::TurnInterrupt],
            Self::InjectItems => &[
                AppServerMethod::Initialize,
                AppServerMethod::ThreadInjectItems,
            ],
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppServerCapabilityCheck {
    pub capability: AppServerCapability,
    pub available: bool,
    pub missing_methods: BTreeSet<AppServerMethod>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppServerConnectionConfig {
    pub transport: String,
    pub endpoint: Option<String>,
    pub codex_home: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AppServerCapabilityReport {
    pub available_methods: BTreeSet<AppServerMethod>,
    pub missing_methods: BTreeSet<AppServerMethod>,
    pub missing_capabilities: BTreeSet<AppServerCapability>,
    pub generated_schema_path: Option<String>,
    pub codex_version: Option<String>,
}

impl AppServerCapabilityReport {
    pub fn from_available_methods<I>(
        available_methods: I,
        generated_schema_path: Option<String>,
        codex_version: Option<String>,
    ) -> Self
    where
        I: IntoIterator<Item = AppServerMethod>,
    {
        let available_methods = available_methods.into_iter().collect::<BTreeSet<_>>();
        let missing_methods =
            missing_methods_for(&available_methods, required_app_server_methods());
        let missing_capabilities = AppServerCapability::required_capabilities()
            .iter()
            .copied()
            .filter(|capability| {
                !has_all_methods(&available_methods, capability.required_methods())
            })
            .collect();

        Self {
            available_methods,
            missing_methods,
            missing_capabilities,
            generated_schema_path,
            codex_version,
        }
    }

    pub fn supports_all_required_methods(&self) -> bool {
        self.missing_methods.is_empty()
    }

    pub fn supports_method(&self, method: AppServerMethod) -> bool {
        self.available_methods.contains(&method)
    }

    pub fn check_capability(&self, capability: AppServerCapability) -> AppServerCapabilityCheck {
        let missing_methods =
            missing_methods_for(&self.available_methods, capability.required_methods());

        AppServerCapabilityCheck {
            capability,
            available: missing_methods.is_empty(),
            missing_methods,
        }
    }

    pub fn supported_capabilities(&self) -> BTreeSet<AppServerCapability> {
        AppServerCapability::required_capabilities()
            .iter()
            .copied()
            .filter(|capability| self.check_capability(*capability).available)
            .collect()
    }
}

pub fn required_app_server_methods() -> &'static [AppServerMethod] {
    &[
        AppServerMethod::Initialize,
        AppServerMethod::ThreadStart,
        AppServerMethod::ThreadResume,
        AppServerMethod::ThreadLoadedList,
        AppServerMethod::TurnStart,
        AppServerMethod::TurnSteer,
        AppServerMethod::TurnInterrupt,
        AppServerMethod::ThreadInjectItems,
    ]
}

fn has_all_methods(
    available_methods: &BTreeSet<AppServerMethod>,
    required_methods: &[AppServerMethod],
) -> bool {
    required_methods
        .iter()
        .all(|method| available_methods.contains(method))
}

fn missing_methods_for(
    available_methods: &BTreeSet<AppServerMethod>,
    required_methods: &[AppServerMethod],
) -> BTreeSet<AppServerMethod> {
    required_methods
        .iter()
        .copied()
        .filter(|method| !available_methods.contains(method))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_required_methods() -> Vec<AppServerMethod> {
        required_app_server_methods().to_vec()
    }

    #[test]
    fn reports_all_required_methods_available() {
        let report = AppServerCapabilityReport::from_available_methods(
            all_required_methods(),
            Some("schemas/codex".to_string()),
            Some("codex-test".to_string()),
        );

        assert!(report.supports_all_required_methods());
        assert!(report.missing_methods.is_empty());
        assert!(report.missing_capabilities.is_empty());
        assert_eq!(
            report.supported_capabilities(),
            AppServerCapability::required_capabilities()
                .iter()
                .copied()
                .collect()
        );
    }

    #[test]
    fn reports_missing_required_methods() {
        let report = AppServerCapabilityReport::from_available_methods(
            [
                AppServerMethod::Initialize,
                AppServerMethod::ThreadStart,
                AppServerMethod::TurnStart,
                AppServerMethod::ThreadInjectItems,
            ],
            None,
            None,
        );

        assert!(!report.supports_all_required_methods());
        assert_eq!(
            report.missing_methods,
            BTreeSet::from([
                AppServerMethod::ThreadResume,
                AppServerMethod::ThreadLoadedList,
                AppServerMethod::TurnSteer,
                AppServerMethod::TurnInterrupt,
            ])
        );
    }

    #[test]
    fn reports_missing_capability_from_required_methods() {
        let report = AppServerCapabilityReport::from_available_methods(
            [
                AppServerMethod::Initialize,
                AppServerMethod::ThreadStart,
                AppServerMethod::TurnStart,
                AppServerMethod::ThreadInjectItems,
            ],
            None,
            None,
        );

        assert_eq!(
            report.missing_capabilities,
            BTreeSet::from([
                AppServerCapability::ResumeThread,
                AppServerCapability::DiscoverLoadedThreads,
                AppServerCapability::SteerTurn,
                AppServerCapability::InterruptTurn,
            ])
        );

        let check = report.check_capability(AppServerCapability::ResumeThread);
        assert!(!check.available);
        assert_eq!(
            check.missing_methods,
            BTreeSet::from([AppServerMethod::ThreadResume])
        );
    }

    #[test]
    fn initialize_is_required_for_every_capability() {
        let report = AppServerCapabilityReport::from_available_methods(
            [
                AppServerMethod::ThreadStart,
                AppServerMethod::ThreadResume,
                AppServerMethod::ThreadLoadedList,
                AppServerMethod::TurnStart,
                AppServerMethod::TurnSteer,
                AppServerMethod::TurnInterrupt,
                AppServerMethod::ThreadInjectItems,
            ],
            None,
            None,
        );

        for capability in AppServerCapability::required_capabilities() {
            let check = report.check_capability(*capability);
            assert!(!check.available);
            assert!(check.missing_methods.contains(&AppServerMethod::Initialize));
        }
    }
}
