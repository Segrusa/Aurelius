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
    fn claude_code_needs_a_runtime_path() {
        let cmd = AddProvider { provider: ProviderKind::ClaudeCode, display_name: "Claude".into(), runtime_path: None, default_model: None };
        assert!(validate_add(&cmd).is_err());
    }
}
