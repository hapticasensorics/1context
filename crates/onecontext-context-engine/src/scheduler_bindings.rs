//! Scheduler binding expression classification and deterministic handles.
//!
//! This resolver intentionally stops before any agent execution. It resolves
//! static wiki/runtime handles and reports packet, day, hour, run, and artifact
//! values as runtime-required unless the caller supplies that context.

use crate::ContextEnginePaths;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingExpression {
    pub raw: String,
    pub kind: BindingExpressionKind,
    pub references: Vec<BindingReference>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingExpressionKind {
    Literal,
    Field { scope: String, field: String },
    WikiPage { slug: String, handle: String },
    WikiConceptsDir,
    ContextEngineScratch,
    Artifact { name: String, argument: String },
    ArtifactCollection { name: String, argument: String },
    List(Vec<BindingExpression>),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingReference {
    pub namespace: String,
    pub name: String,
    pub argument: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BindingResolution {
    pub expression: BindingExpression,
    pub status: BindingResolutionStatus,
    pub value: Option<BindingValue>,
    pub reason: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingResolutionStatus {
    Resolved,
    RuntimeRequired,
    Unsupported,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingValue {
    Scalar(String),
    List(Vec<String>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BindingResolutionContext {
    pub run_id: String,
    pub latest_refreshed_day: Option<String>,
    pub context_engine_root: PathBuf,
    pub user_wiki_root: PathBuf,
    pub artifact_root: PathBuf,
    pub pages: BTreeMap<String, WikiPageHandles>,
    pub packet: Option<PacketBindingContext>,
    pub hour: Option<HourBindingContext>,
    pub day: Option<DayBindingContext>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WikiPageHandles {
    pub source: PathBuf,
    pub talk: PathBuf,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PacketBindingContext {
    pub id: String,
    pub date: String,
    pub hour: String,
    pub path: String,
    pub content_sha256: String,
    pub hour_has_multiple_scribe_artifacts: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HourBindingContext {
    pub id: String,
    pub date: String,
    pub hour: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DayBindingContext {
    pub date: String,
}

impl BindingResolutionContext {
    pub fn from_paths(paths: &ContextEnginePaths, run_id: impl Into<String>) -> Self {
        let user_wiki_root = paths.user_wiki.clone();
        Self {
            run_id: run_id.into(),
            latest_refreshed_day: None,
            context_engine_root: paths.context_engine.clone(),
            artifact_root: paths.context_engine.join("artifacts"),
            pages: discover_page_handles(&user_wiki_root),
            user_wiki_root,
            packet: None,
            hour: None,
            day: None,
        }
    }
}

pub fn parse_binding_expression(raw: impl AsRef<str>) -> BindingExpression {
    let raw = raw.as_ref().trim().to_string();
    let parts = split_top_level_commas(&raw);
    if parts.len() > 1 {
        let items = parts
            .iter()
            .map(|part| parse_binding_expression(part))
            .collect::<Vec<_>>();
        let references = items
            .iter()
            .flat_map(|item| item.references.clone())
            .collect::<Vec<_>>();
        return BindingExpression {
            raw,
            kind: BindingExpressionKind::List(items),
            references,
        };
    }

    if let Some((slug, handle)) = parse_wiki_page_handle(&raw) {
        return BindingExpression {
            raw,
            kind: BindingExpressionKind::WikiPage {
                slug: slug.clone(),
                handle: handle.clone(),
            },
            references: vec![BindingReference {
                namespace: "wiki.page".to_string(),
                name: handle,
                argument: Some(slug),
            }],
        };
    }

    if raw == "wiki.concepts.dir" {
        return BindingExpression {
            raw,
            kind: BindingExpressionKind::WikiConceptsDir,
            references: vec![BindingReference {
                namespace: "wiki.concepts".to_string(),
                name: "dir".to_string(),
                argument: None,
            }],
        };
    }

    if raw == "context_engine.scratch" {
        return BindingExpression {
            raw,
            kind: BindingExpressionKind::ContextEngineScratch,
            references: vec![BindingReference {
                namespace: "context_engine".to_string(),
                name: "scratch".to_string(),
                argument: None,
            }],
        };
    }

    if let Some((name, argument)) = parse_namespace_call(&raw, "artifact.") {
        return BindingExpression {
            raw,
            kind: BindingExpressionKind::Artifact {
                name: name.clone(),
                argument: argument.clone(),
            },
            references: vec![BindingReference {
                namespace: "artifact".to_string(),
                name,
                argument: Some(argument),
            }],
        };
    }

    if let Some((name, argument)) = parse_namespace_call(&raw, "artifacts.") {
        return BindingExpression {
            raw,
            kind: BindingExpressionKind::ArtifactCollection {
                name: name.clone(),
                argument: argument.clone(),
            },
            references: vec![BindingReference {
                namespace: "artifacts".to_string(),
                name,
                argument: Some(argument),
            }],
        };
    }

    if let Some((scope, field)) = raw.split_once('.') {
        if is_identifierish(scope) && is_identifierish(field) {
            let scope = scope.to_string();
            let field = field.to_string();
            return BindingExpression {
                raw,
                kind: BindingExpressionKind::Field {
                    scope: scope.clone(),
                    field: field.clone(),
                },
                references: vec![BindingReference {
                    namespace: scope,
                    name: field,
                    argument: None,
                }],
            };
        }
    }

    BindingExpression {
        raw,
        kind: BindingExpressionKind::Literal,
        references: Vec::new(),
    }
}

pub fn resolve_binding(
    raw: impl AsRef<str>,
    context: &BindingResolutionContext,
) -> BindingResolution {
    let expression = parse_binding_expression(raw);
    resolve_binding_expression(&expression, context)
}

pub fn resolve_binding_expression(
    expression: &BindingExpression,
    context: &BindingResolutionContext,
) -> BindingResolution {
    match &expression.kind {
        BindingExpressionKind::Literal => {
            resolved(expression, BindingValue::Scalar(expression.raw.clone()))
        }
        BindingExpressionKind::Field { scope, field } => {
            resolve_field(expression, scope, field, context)
        }
        BindingExpressionKind::WikiPage { slug, handle } => {
            resolve_wiki_page(expression, slug, handle, context)
        }
        BindingExpressionKind::WikiConceptsDir => resolved(
            expression,
            BindingValue::Scalar(
                context
                    .user_wiki_root
                    .join("source/families/reference/topics")
                    .display()
                    .to_string(),
            ),
        ),
        BindingExpressionKind::ContextEngineScratch => resolved(
            expression,
            BindingValue::Scalar(
                context
                    .context_engine_root
                    .join("scratch")
                    .display()
                    .to_string(),
            ),
        ),
        BindingExpressionKind::Artifact { name, argument } => {
            resolve_artifact(expression, name, argument, false, context)
        }
        BindingExpressionKind::ArtifactCollection { name, argument } => {
            resolve_artifact(expression, name, argument, true, context)
        }
        BindingExpressionKind::List(items) => resolve_list(expression, items, context),
    }
}

fn resolve_field(
    expression: &BindingExpression,
    scope: &str,
    field: &str,
    context: &BindingResolutionContext,
) -> BindingResolution {
    let value = match scope {
        "packet" => {
            let Some(packet) = &context.packet else {
                return runtime_required(expression, "packet context required");
            };
            match field {
                "id" => Some(packet.id.clone()),
                "date" => Some(packet.date.clone()),
                "hour" => Some(packet.hour.clone()),
                "path" => Some(packet.path.clone()),
                "content_sha256" => Some(packet.content_sha256.clone()),
                "hour_has_multiple_scribe_artifacts" => packet
                    .hour_has_multiple_scribe_artifacts
                    .map(|value| value.to_string()),
                _ => None,
            }
        }
        "hour" => {
            let Some(hour) = &context.hour else {
                return runtime_required(expression, "hour context required");
            };
            match field {
                "id" => Some(hour.id.clone()),
                "date" => Some(hour.date.clone()),
                "hour" => Some(hour.hour.clone()),
                _ => None,
            }
        }
        "day" => {
            let Some(day) = &context.day else {
                return runtime_required(expression, "day context required");
            };
            match field {
                "date" => Some(day.date.clone()),
                _ => None,
            }
        }
        "run" => match field {
            "id" => Some(context.run_id.clone()),
            "latest_refreshed_day" => context.latest_refreshed_day.clone(),
            _ => None,
        },
        _ => None,
    };

    match value {
        Some(value) => resolved(expression, BindingValue::Scalar(value)),
        None => runtime_required(
            expression,
            format!("{scope}.{field} is supplied by scheduler runtime state"),
        ),
    }
}

fn resolve_wiki_page(
    expression: &BindingExpression,
    slug: &str,
    handle: &str,
    context: &BindingResolutionContext,
) -> BindingResolution {
    let Some(handles) = context.pages.get(slug) else {
        return runtime_required(expression, format!("page {slug} must be provisioned"));
    };
    match handle {
        "source" => resolved(
            expression,
            BindingValue::Scalar(handles.source.display().to_string()),
        ),
        "talk" => resolved(
            expression,
            BindingValue::Scalar(handles.talk.display().to_string()),
        ),
        "prior" => runtime_required(
            expression,
            "prior page selection depends on refresh/run history",
        ),
        _ => runtime_required(
            expression,
            format!("wiki.page handle {handle} is supplied by wiki runtime"),
        ),
    }
}

fn resolve_artifact(
    expression: &BindingExpression,
    name: &str,
    argument: &str,
    collection: bool,
    context: &BindingResolutionContext,
) -> BindingResolution {
    let argument_value = match resolve_argument(argument, context) {
        ArgumentResolution::Resolved(value) => value,
        ArgumentResolution::RuntimeRequired(reason) => return runtime_required(expression, reason),
        ArgumentResolution::Unsupported(reason) => return unsupported(expression, reason),
    };
    let base = context.artifact_root.join(name);
    let path = if collection {
        base.join(argument_value)
    } else {
        base.join(format!("{argument_value}.json"))
    };
    resolved(expression, BindingValue::Scalar(path.display().to_string()))
}

fn resolve_argument(argument: &str, context: &BindingResolutionContext) -> ArgumentResolution {
    let stripped = strip_matching_quotes(argument.trim());
    if stripped != argument.trim() {
        return ArgumentResolution::Resolved(stripped.to_string());
    }
    let expression = parse_binding_expression(stripped);
    match resolve_binding_expression(&expression, context) {
        BindingResolution {
            status: BindingResolutionStatus::Resolved,
            value: Some(BindingValue::Scalar(value)),
            ..
        } => ArgumentResolution::Resolved(value),
        BindingResolution {
            status: BindingResolutionStatus::RuntimeRequired,
            reason,
            ..
        } => ArgumentResolution::RuntimeRequired(
            reason.unwrap_or_else(|| format!("runtime value required for {argument}")),
        ),
        BindingResolution {
            status: BindingResolutionStatus::Resolved,
            value: Some(BindingValue::List(_)),
            ..
        } => ArgumentResolution::Unsupported(format!("{argument} resolved to a list")),
        _ => ArgumentResolution::Unsupported(format!("unsupported artifact argument {argument}")),
    }
}

enum ArgumentResolution {
    Resolved(String),
    RuntimeRequired(String),
    Unsupported(String),
}

fn resolve_list(
    expression: &BindingExpression,
    items: &[BindingExpression],
    context: &BindingResolutionContext,
) -> BindingResolution {
    let mut values = Vec::new();
    let mut runtime_reasons = Vec::new();
    for item in items {
        let resolution = resolve_binding_expression(item, context);
        match (resolution.status, resolution.value) {
            (BindingResolutionStatus::Resolved, Some(BindingValue::Scalar(value))) => {
                values.push(value);
            }
            (BindingResolutionStatus::Resolved, Some(BindingValue::List(list))) => {
                values.extend(list);
            }
            (BindingResolutionStatus::RuntimeRequired, _) => {
                runtime_reasons.push(
                    resolution
                        .reason
                        .unwrap_or_else(|| format!("runtime value required for {}", item.raw)),
                );
            }
            (BindingResolutionStatus::Unsupported, _) => {
                return unsupported(
                    expression,
                    resolution
                        .reason
                        .unwrap_or_else(|| format!("unsupported expression {}", item.raw)),
                );
            }
            _ => runtime_reasons.push(format!("runtime value required for {}", item.raw)),
        }
    }
    if runtime_reasons.is_empty() {
        resolved(expression, BindingValue::List(values))
    } else {
        runtime_required(expression, runtime_reasons.join("; "))
    }
}

fn resolved(expression: &BindingExpression, value: BindingValue) -> BindingResolution {
    BindingResolution {
        expression: expression.clone(),
        status: BindingResolutionStatus::Resolved,
        value: Some(value),
        reason: None,
    }
}

fn runtime_required(
    expression: &BindingExpression,
    reason: impl Into<String>,
) -> BindingResolution {
    BindingResolution {
        expression: expression.clone(),
        status: BindingResolutionStatus::RuntimeRequired,
        value: None,
        reason: Some(reason.into()),
    }
}

fn unsupported(expression: &BindingExpression, reason: impl Into<String>) -> BindingResolution {
    BindingResolution {
        expression: expression.clone(),
        status: BindingResolutionStatus::Unsupported,
        value: None,
        reason: Some(reason.into()),
    }
}

fn parse_wiki_page_handle(raw: &str) -> Option<(String, String)> {
    let rest = raw.strip_prefix("wiki.page(")?;
    let close = rest.find(')')?;
    let slug = strip_matching_quotes(rest[..close].trim()).to_string();
    let suffix = rest[close + 1..].strip_prefix('.')?.trim();
    if slug.is_empty() || !is_identifierish(suffix) {
        return None;
    }
    Some((slug, suffix.to_string()))
}

fn parse_namespace_call(raw: &str, prefix: &str) -> Option<(String, String)> {
    let rest = raw.strip_prefix(prefix)?;
    let open = rest.find('(')?;
    let close = rest.rfind(')')?;
    if close <= open || close + 1 != rest.len() {
        return None;
    }
    let name = rest[..open].trim();
    let argument = rest[open + 1..close].trim();
    if name.is_empty() || argument.is_empty() || !is_identifierish(name) {
        return None;
    }
    Some((name.to_string(), argument.to_string()))
}

fn split_top_level_commas(raw: &str) -> Vec<String> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0_i32;
    let mut quote = None;
    for (idx, ch) in raw.char_indices() {
        if let Some(active) = quote {
            if ch == active {
                quote = None;
            }
            continue;
        }
        match ch {
            '\'' | '"' => quote = Some(ch),
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            ',' if depth == 0 => {
                parts.push(raw[start..idx].trim().to_string());
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    parts.push(raw[start..].trim().to_string());
    parts
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
}

fn strip_matching_quotes(raw: &str) -> &str {
    if raw.len() >= 2
        && ((raw.starts_with('\'') && raw.ends_with('\''))
            || (raw.starts_with('"') && raw.ends_with('"')))
    {
        &raw[1..raw.len() - 1]
    } else {
        raw
    }
}

fn is_identifierish(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
}

fn discover_page_handles(user_wiki_root: &Path) -> BTreeMap<String, WikiPageHandles> {
    let mut pages = BTreeMap::new();
    let families = user_wiki_root.join("source/families");
    let Ok(groups) = fs::read_dir(families) else {
        return pages;
    };
    for group in groups.flatten() {
        let Ok(children) = fs::read_dir(group.path()) else {
            continue;
        };
        for child in children.flatten() {
            let family_file = child.path().join("family.toml");
            if !family_file.is_file() {
                continue;
            }
            if let Some(family) = read_family_file(&family_file) {
                let root = child.path();
                pages.insert(
                    family.slug.clone(),
                    WikiPageHandles {
                        source: root.join("source").join(format!("{}.md", family.slug)),
                        talk: root.join("talk").join(format!("{}.talk", family.slug)),
                    },
                );
            }
        }
    }
    pages
}

#[derive(Deserialize)]
struct FamilyFile {
    slug: String,
}

fn read_family_file(path: &Path) -> Option<FamilyFile> {
    let text = fs::read_to_string(path).ok()?;
    toml::from_str(&text).ok()
}
