//! The server's name (feature `server_name`): what every client shows for this
//! server. A board member chooses it; without a choice it is the computer's
//! name. Only the shown name changes: the OS hostname, the tailnet name, the
//! URL and the host id stay, so pairings keep working.

use crate::DomainError;
use serde::{Deserialize, Serialize};

/// The longest name a board member may choose, in characters after trimming.
pub const SERVER_NAME_MAX: usize = 40;

/// Choose the server's name (`server.set_name`). Empty or only spaces puts the
/// computer's name back. The same name as now changes nothing (no event).
/// Answers `ServerNameSet`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct SetServerName {
    pub name: String,
}

impl SetServerName {
    /// The name to store: trimmed, or none for the computer's name.
    pub fn valid_name(&self) -> Result<Option<String>, DomainError> {
        valid_server_name(&self.name)
    }
}

/// `server.get_name`: the name clients show, and where it comes from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ServerNameView {
    /// What clients show: the chosen name, else the computer's name. The same
    /// as `HelloResult::host_name`.
    pub name: String,
    /// The name a board member chose; none: the computer's name is used.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chosen: Option<String>,
    /// The computer's name, the default ("Default: ‹computer_name›").
    pub computer_name: String,
    /// Counts the changes; 0 before the first one.
    pub version: u64,
}

impl ServerNameView {
    /// The view of a stored choice on a computer called `computer_name`.
    pub fn of(chosen: Option<String>, computer_name: String, version: u64) -> Self {
        Self { name: shown_name(chosen.as_deref(), &computer_name), chosen, computer_name, version }
    }
}

/// What clients show: the chosen name, else the computer's name.
pub fn shown_name(chosen: Option<&str>, computer_name: &str) -> String {
    chosen.unwrap_or(computer_name).to_owned()
}

/// A name a board member typed (A-05): 1 to `SERVER_NAME_MAX` characters after
/// trimming, without control characters or line breaks; emoji and accents are
/// fine. Empty after trimming: none, the computer's name (A-03).
pub fn valid_server_name(name: &str) -> Result<Option<String>, DomainError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.chars().count() > SERVER_NAME_MAX {
        return Err(DomainError::TooLong { field: "name".into(), max: SERVER_NAME_MAX });
    }
    if trimmed.chars().any(refused_char) {
        return Err(DomainError::ServerName("The server's name cannot contain line breaks, control or invisible characters.".into()));
    }
    Ok(Some(trimmed.to_owned()))
}

/// Control characters, line breaks that are not (U+2028/U+2029), and the
/// invisible and bidi format characters: a name of only those looks empty, a
/// bidi override turns the shown name around.
fn refused_char(c: char) -> bool {
    c.is_control() || matches!(c, '\u{2028}' | '\u{2029}' | '\u{200B}'..='\u{200F}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_trimmed_and_may_have_emoji_and_accents() {
        assert_eq!(valid_server_name("  Thuis-PC ").unwrap().as_deref(), Some("Thuis-PC"));
        assert_eq!(valid_server_name("Kantoor é 🏠").unwrap().as_deref(), Some("Kantoor é 🏠"));
    }

    #[test]
    fn empty_or_only_spaces_means_the_computers_name() {
        assert_eq!(valid_server_name("").unwrap(), None);
        assert_eq!(valid_server_name("   \t ").unwrap(), None);
    }

    #[test]
    fn forty_characters_pass_and_forty_one_are_refused() {
        let forty = "é".repeat(SERVER_NAME_MAX);
        assert_eq!(valid_server_name(&forty).unwrap(), Some(forty.clone()));
        let err = valid_server_name(&format!("{forty}x")).unwrap_err();
        assert_eq!(err, DomainError::TooLong { field: "name".into(), max: SERVER_NAME_MAX });
        assert!(err.to_string().contains("40"), "the reason names the limit");
    }

    #[test]
    fn line_breaks_and_control_characters_are_refused() {
        for name in ["Kan\ntoor", "Kan\rtoor", "Kan\u{7}toor", "Kan\u{2028}toor", "Kan\u{85}toor", "Kan\ttoor"] {
            assert!(matches!(valid_server_name(name), Err(DomainError::ServerName(_))), "{name:?}");
        }
    }

    #[test]
    fn invisible_and_bidi_characters_are_refused() {
        for name in ["\u{200B}", "\u{202E}gnp.exe", "a\u{2066}b", "\u{FEFF}", "Kan\u{200D}toor"] {
            assert!(matches!(valid_server_name(name), Err(DomainError::ServerName(_))), "{name:?}");
        }
    }

    #[test]
    fn the_view_shows_the_chosen_name_else_the_computers() {
        assert_eq!(ServerNameView::of(Some("Kantoor".into()), "desktop-1".into(), 2).name, "Kantoor");
        let default = ServerNameView::of(None, "desktop-1".into(), 0);
        assert_eq!((default.name.as_str(), default.chosen), ("desktop-1", None));
    }

    #[test]
    fn set_name_refuses_unknown_fields() {
        assert!(serde_json::from_value::<SetServerName>(serde_json::json!({ "name": "x", "version": 1 })).is_err());
    }
}
