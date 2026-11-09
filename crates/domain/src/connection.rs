//! Connections and configuration resolution.
//! A connection decides account, auth and billing; it never holds a secret
//! itself (only a reference into the OS credential store).

use crate::{ProviderId, DomainError, EmployeeId, ProjectId, employee::Setting, optional_text, require_text};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// Built-in deterministic test runtime; no network, no cost.
    Fake,
    /// Unmodified Claude Code CLI; the user signs in with Anthropic's own flow.
    ClaudeCode,
    /// Anthropic Messages API with the user's own API key.
    AnthropicApi,
    /// OpenAI Responses API with the user's own API key.
    OpenaiApi,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ProviderKind::Fake => "fake",
            ProviderKind::ClaudeCode => "claude_code",
            ProviderKind::AnthropicApi => "anthropic_api",
            ProviderKind::OpenaiApi => "openai_api",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "fake" => Some(Self::Fake),
            "claude_code" => Some(Self::ClaudeCode),
            "anthropic_api" => Some(Self::AnthropicApi),
            "openai_api" => Some(Self::OpenaiApi),
            _ => None,
        }
    }

    /// How the user pays; shown next to every model choice.
    pub fn billing(self) -> &'static str {
        match self {
            ProviderKind::Fake => "local, no cost",
            ProviderKind::ClaudeCode => "via your own Claude login (subscription or API, as set in Claude Code)",
            ProviderKind::AnthropicApi => "Anthropic API, your own key, pay per use",
            ProviderKind::OpenaiApi => "OpenAI API, your own key, pay per use",
        }
    }

    pub fn needs_api_key(self) -> bool {
        matches!(self, ProviderKind::AnthropicApi | ProviderKind::OpenaiApi)
    }

    pub fn auth_kind(self) -> &'static str {
        match self {
            ProviderKind::Fake => "none",
            ProviderKind::ClaudeCode => "runtime_login",
            ProviderKind::AnthropicApi | ProviderKind::OpenaiApi => "api_key",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "AddConnection")]
pub struct AddProvider {
    pub provider: ProviderKind,
    pub display_name: String,
    #[serde(default)]
    pub runtime_path: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "RemoveConnection")]
pub struct RemoveProvider {
    pub connection_id: ProviderId,
}

/// Probe a connection now (status, models) and record the result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[schemars(rename = "VerifyConnection")]
pub struct VerifyProvider {
    pub connection_id: ProviderId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum ConfigScope {
    Global,
    Project(ProjectId),
    Employee(EmployeeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Employee,
    Memory,
    /// The subagents a Claude Code run starts itself.
    Subagent,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::Employee => "employee",
            Purpose::Memory => "memory",
            Purpose::Subagent => "subagent",
        }
    }
}

/// Per-field override at one scope for one purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default, schemars::JsonSchema)]
#[schemars(rename = "RunSettings")]
pub struct RunDefaults {
    #[serde(default)]
    pub connection: Setting<ProviderId>,
    #[serde(default)]
    pub model: Setting<String>,
    /// Optional chat persona. Chat answers neutrally by default (no persona).
    #[serde(default)]
    pub persona: Setting<String>,
    /// Reasoning effort (low, medium, high, xhigh, max) where the runtime supports it.
    #[serde(default)]
    pub effort: Setting<String>,
}

/// Effort levels Claude Code accepts (`--effort`).
pub const EFFORT_LEVELS: [&str; 5] = ["low", "medium", "high", "xhigh", "max"];

/// Claude models that take no effort at all. From Claude Code's own model catalogue.
const MODELS_WITHOUT_EFFORT: [&str; 10] = [
    "claude-3-5-haiku",
    "claude-3-5-sonnet",
    "claude-3-7-sonnet",
    "claude-haiku-4-5",
    "claude-mythos-5",
    "claude-opus-4-0",
    "claude-opus-4-1",
    "claude-opus-4-5",
    "claude-sonnet-4-0",
    "claude-sonnet-4-5",
];

/// Claude models that stop at `max` without `xhigh`.
const MODELS_WITHOUT_XHIGH: [&str; 2] = ["claude-opus-4-6", "claude-sonnet-4-6"];

/// The effort Claude Code uses for a model when none is passed, where its catalogue names one.
const MODEL_DEFAULT_EFFORT: [(&str, &str); 10] = [
    ("claude-fable-5", "high"),
    ("claude-fable-5-1", "high"),
    ("claude-haiku-5-5", "medium"),
    ("claude-mythos-5-1", "high"),
    ("claude-opus-4-7", "xhigh"),
    ("claude-opus-4-8", "high"),
    ("claude-opus-5", "high"),
    ("claude-opus-5-5", "medium"),
    ("claude-sonnet-5", "high"),
    ("claude-sonnet-5-5", "medium"),
];

/// A model id without what does not change its capabilities: `[1m]` and a release date.
fn model_family(model: &str) -> &str {
    let model = model.split('[').next().unwrap_or(model);
    match model.rsplit_once('-') {
        Some((head, date)) if date.len() == 8 && date.bytes().all(|b| b.is_ascii_digit()) => head,
        _ => model,
    }
}

/// The effort levels a run on this provider and model can use. Only Claude Code
/// takes an effort (and the test runtime, which imitates it); an unknown or
/// unset model (an alias, a new release) offers every level, since Claude Code
/// decides then and refusing would block a model this table does not know yet.
pub fn supported_efforts(provider: Option<ProviderKind>, model: Option<&str>) -> Vec<&'static str> {
    if matches!(provider, Some(ProviderKind::AnthropicApi | ProviderKind::OpenaiApi)) {
        return Vec::new();
    }
    let family = model.map(model_family).unwrap_or_default();
    if MODELS_WITHOUT_EFFORT.contains(&family) {
        return Vec::new();
    }
    let no_xhigh = MODELS_WITHOUT_XHIGH.contains(&family);
    EFFORT_LEVELS.into_iter().filter(|level| !(no_xhigh && *level == "xhigh")).collect()
}

/// The effort a model runs at when Aurelius passes none, if that is known.
pub fn model_default_effort(provider: Option<ProviderKind>, model: Option<&str>) -> Option<&'static str> {
    if !matches!(provider, None | Some(ProviderKind::ClaudeCode)) || supported_efforts(provider, model).is_empty() {
        return None;
    }
    let family = model_family(model?);
    MODEL_DEFAULT_EFFORT.iter().find(|(id, _)| *id == family).map(|(_, effort)| *effort)
}

/// Why `effort` cannot be chosen for this model, in words for the board member.
/// None: it can.
pub fn effort_refusal(effort: &str, provider: Option<ProviderKind>, model: Option<&str>) -> Option<String> {
    if !EFFORT_LEVELS.contains(&effort) {
        return Some(format!("Effort must be one of {}.", EFFORT_LEVELS.join(", ")));
    }
    let supported = supported_efforts(provider, model);
    if supported.contains(&effort) {
        return None;
    }
    let name = model.unwrap_or("This connection");
    Some(if supported.is_empty() { format!("{name} has no effort setting.") } else { format!("{name} supports {}, not {effort}.", supported.join(", ")) })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct SetRunDefaults {
    pub scope: ConfigScope,
    pub purpose: Purpose,
    pub settings: RunDefaults,
}

/// Where an effective value came from, shown as "Model X · geërfd van Project Atlas".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Origin {
    pub scope: ConfigScope,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct Resolved<T> {
    pub value: Option<T>,
    pub origin: Option<Origin>,
}

/// Resolve one field: the most specific scope with `Value` or `Disabled` wins;
/// `Inherit` falls through. Order: employee → project → global.
pub fn resolve_field<T: Clone>(layers: &[(Origin, Setting<T>)]) -> Resolved<T> {
    for (origin, setting) in layers {
        match setting {
            Setting::Inherit => continue,
            Setting::Disabled => return Resolved { value: None, origin: Some(origin.clone()) },
            Setting::Value(v) => return Resolved { value: Some(v.clone()), origin: Some(origin.clone()) },
        }
    }
    Resolved { value: None, origin: None }
}

pub fn validate_add(cmd: &AddProvider) -> Result<(String, Option<String>, Option<String>), DomainError> {
    let name = require_text("display_name", &cmd.display_name, 80)?;
    let path = optional_text("runtime_path", cmd.runtime_path.as_deref(), 1_000)?;
    let model = optional_text("default_model", cmd.default_model.as_deref(), 200)?;
    if cmd.provider == ProviderKind::ClaudeCode && path.is_none() {
        return Err(DomainError::Required { field: "runtime_path".into() });
    }
    Ok((name, path, model))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn origin(label: &str) -> Origin {
        Origin { scope: ConfigScope::Global, label: label.into() }
    }

    #[test]
    fn most_specific_explicit_value_wins_and_inherit_falls_through() {
        let layers = vec![
            (origin("employee"), Setting::Inherit),
            (origin("project"), Setting::Value("model-p".to_string())),
            (origin("global"), Setting::Value("model-g".to_string())),
        ];
        let r = resolve_field(&layers);
        assert_eq!(r.value.as_deref(), Some("model-p"));
        assert_eq!(r.origin.unwrap().label, "project");
    }

    #[test]
    fn disabled_is_not_inherit() {
        let layers = vec![(origin("employee"), Setting::Disabled), (origin("global"), Setting::Value("x".to_string()))];
        let r = resolve_field(&layers);
        assert_eq!(r.value, None);
        assert_eq!(r.origin.unwrap().label, "employee");
    }

    #[test]
    fn a_model_only_offers_the_effort_levels_claude_code_has_for_it() {
        let cc = Some(ProviderKind::ClaudeCode);
        assert!(supported_efforts(cc, Some("claude-haiku-4-5-20251001")).is_empty());
        assert_eq!(supported_efforts(cc, Some("claude-opus-4-6[1m]")), ["low", "medium", "high", "max"]);
        assert_eq!(supported_efforts(cc, Some("claude-opus-5-5")), EFFORT_LEVELS);
        assert_eq!(supported_efforts(cc, Some("opus")), EFFORT_LEVELS, "an alias is Claude Code's to resolve");
        assert_eq!(supported_efforts(cc, None), EFFORT_LEVELS);
        assert!(supported_efforts(Some(ProviderKind::AnthropicApi), Some("claude-opus-5-5")).is_empty());
    }

    #[test]
    fn a_refusal_says_what_the_model_does_support() {
        let cc = Some(ProviderKind::ClaudeCode);
        assert_eq!(effort_refusal("high", cc, Some("claude-opus-5-5")), None);
        assert_eq!(effort_refusal("xhigh", cc, Some("claude-sonnet-4-6")).as_deref(), Some("claude-sonnet-4-6 supports low, medium, high, max, not xhigh."));
        assert_eq!(effort_refusal("low", cc, Some("claude-haiku-4-5")).as_deref(), Some("claude-haiku-4-5 has no effort setting."));
        assert!(effort_refusal("ludicrous", cc, None).is_some());
    }

    #[test]
    fn the_model_default_is_known_only_where_the_catalogue_names_one() {
        let cc = Some(ProviderKind::ClaudeCode);
        assert_eq!(model_default_effort(cc, Some("claude-opus-4-7")), Some("xhigh"));
        assert_eq!(model_default_effort(cc, Some("claude-mythos-5")), None, "no effort at all");
        assert_eq!(model_default_effort(cc, Some("opus")), None);
        assert_eq!(model_default_effort(Some(ProviderKind::OpenaiApi), Some("claude-opus-5-5")), None);
    }

    #[test]
    fn claude_code_needs_a_runtime_path() {
        let cmd = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude".into(), runtime_path: None, default_model: None };
        assert!(validate_add(&cmd).is_err());
    }
}
