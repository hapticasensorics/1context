use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SchemaSource {
    GeneratedJsonSchema,
    GeneratedTypeScript,
    BundledFallback,
    Missing,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaRegistryStatus {
    pub source: SchemaSource,
    pub codex_version: Option<String>,
    pub schema_dir: Option<String>,
    pub generated_at: Option<DateTime<Utc>>,
    pub usable: bool,
    pub warnings: Vec<String>,
}

impl SchemaRegistryStatus {
    pub fn is_usable(&self) -> bool {
        self.usability_issues().is_empty()
    }

    pub fn usability_issues(&self) -> Vec<String> {
        let mut issues = Vec::new();

        if self.source == SchemaSource::Missing {
            issues.push("schema source is missing".to_string());
        }

        if !self.usable {
            issues.push("schema registry status is marked unusable".to_string());
        }

        if self.schema_dir.is_none() && self.source != SchemaSource::Missing {
            issues.push("schema directory is not recorded".to_string());
        }

        if self.codex_version.is_none() && self.source != SchemaSource::Missing {
            issues.push("codex version is not recorded".to_string());
        }

        if self.generated_at.is_none()
            && matches!(
                self.source,
                SchemaSource::GeneratedJsonSchema | SchemaSource::GeneratedTypeScript
            )
        {
            issues.push("schema generation timestamp is not recorded".to_string());
        }

        issues.extend(self.warnings.iter().cloned());
        issues
    }

    pub fn generated_json_schema(
        codex_version: impl Into<String>,
        schema_dir: impl Into<String>,
        generated_at: DateTime<Utc>,
    ) -> Self {
        Self {
            source: SchemaSource::GeneratedJsonSchema,
            codex_version: Some(codex_version.into()),
            schema_dir: Some(schema_dir.into()),
            generated_at: Some(generated_at),
            usable: true,
            warnings: Vec::new(),
        }
    }

    pub fn missing(warnings: Vec<String>) -> Self {
        Self {
            source: SchemaSource::Missing,
            codex_version: None,
            schema_dir: None,
            generated_at: None,
            usable: false,
            warnings,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchemaGenerationCommandPlan {
    pub program: String,
    pub args: Vec<String>,
    pub output_dir: String,
    pub executes_in_unit_tests: bool,
}

impl SchemaGenerationCommandPlan {
    pub fn generate_json_schema(output_dir: impl Into<String>) -> Self {
        Self::new("generate-json-schema", output_dir)
    }

    pub fn generate_type_script(output_dir: impl Into<String>) -> Self {
        Self::new("generate-ts", output_dir)
    }

    fn new(subcommand: impl Into<String>, output_dir: impl Into<String>) -> Self {
        let output_dir = output_dir.into();

        Self {
            program: "codex".to_string(),
            args: vec![
                "app-server".to_string(),
                subcommand.into(),
                "--out".to_string(),
                output_dir.clone(),
            ],
            output_dir,
            executes_in_unit_tests: false,
        }
    }

    pub fn as_command_line(&self) -> Vec<String> {
        let mut command_line = Vec::with_capacity(self.args.len() + 1);
        command_line.push(self.program.clone());
        command_line.extend(self.args.iter().cloned());
        command_line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_json_schema_status_is_usable() {
        let generated_at = DateTime::parse_from_rfc3339("2026-05-25T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let status = SchemaRegistryStatus::generated_json_schema(
            "codex-test",
            "schemas/codex",
            generated_at,
        );

        assert!(status.is_usable());
        assert!(status.usability_issues().is_empty());
    }

    #[test]
    fn missing_schema_status_is_not_usable() {
        let status = SchemaRegistryStatus::missing(vec!["codex cli not found".to_string()]);

        assert!(!status.is_usable());
        assert_eq!(
            status.usability_issues(),
            vec![
                "schema source is missing".to_string(),
                "schema registry status is marked unusable".to_string(),
                "codex cli not found".to_string(),
            ]
        );
    }

    #[test]
    fn unusable_generated_status_reports_issues() {
        let status = SchemaRegistryStatus {
            source: SchemaSource::GeneratedJsonSchema,
            codex_version: Some("codex-test".to_string()),
            schema_dir: None,
            generated_at: None,
            usable: false,
            warnings: vec!["schema parse failed".to_string()],
        };

        assert!(!status.is_usable());
        assert_eq!(
            status.usability_issues(),
            vec![
                "schema registry status is marked unusable".to_string(),
                "schema directory is not recorded".to_string(),
                "schema generation timestamp is not recorded".to_string(),
                "schema parse failed".to_string(),
            ]
        );
    }

    #[test]
    fn plans_generate_json_schema_command_without_executing_it() {
        let plan = SchemaGenerationCommandPlan::generate_json_schema("schemas/generated");

        assert_eq!(plan.program, "codex");
        assert_eq!(
            plan.args,
            vec![
                "app-server",
                "generate-json-schema",
                "--out",
                "schemas/generated"
            ]
        );
        assert_eq!(
            plan.as_command_line(),
            vec![
                "codex",
                "app-server",
                "generate-json-schema",
                "--out",
                "schemas/generated"
            ]
        );
        assert!(!plan.executes_in_unit_tests);
    }

    #[test]
    fn plans_generate_type_script_command_without_executing_it() {
        let plan = SchemaGenerationCommandPlan::generate_type_script("schemas/generated-ts");

        assert_eq!(
            plan.as_command_line(),
            vec![
                "codex",
                "app-server",
                "generate-ts",
                "--out",
                "schemas/generated-ts"
            ]
        );
        assert!(!plan.executes_in_unit_tests);
    }
}
