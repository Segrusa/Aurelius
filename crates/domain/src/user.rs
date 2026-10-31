//! Board members (Fase 1: identification, not authentication). A connection has
//! at most one active board member; ticket, board and company writes need one.
//!
//! The password never enters the domain: the protocol hashes it (argon2id)
//! before a command exists, so it is in no command, receipt hash or event.

use crate::{DomainError, UserId};
use serde::{Deserialize, Serialize};

pub const USERNAME_MIN: usize = 3;
pub const USERNAME_MAX: usize = 32;
pub const PASSWORD_MIN: usize = 12;
/// Longer input only costs hashing time.
pub const PASSWORD_MAX: usize = 1_024;

/// A board member's name (OV-12): 3 to 32 letters, digits, `.`, `_` or `-`;
/// unique on the server regardless of case (the database decides that).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Username(String);

impl Username {
    pub fn parse(text: &str) -> Result<Self, DomainError> {
        let text = text.trim();
        let count = text.chars().count();
        if !(USERNAME_MIN..=USERNAME_MAX).contains(&count) {
            return Err(DomainError::InvalidUsername(format!("A name has {USERNAME_MIN} to {USERNAME_MAX} characters.")));
        }
        if !text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-')) {
            return Err(DomainError::InvalidUsername("Use only letters, digits, '.', '_' and '-'.".into()));
        }
        Ok(Self(text.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// At least 12 characters (OV-12), not only spaces. Checked before hashing.
pub fn check_password(password: &str) -> Result<(), DomainError> {
    let count = password.chars().count();
    if count > 0 && password.trim().is_empty() {
        return Err(DomainError::WeakPassword("A password of only spaces is too weak.".into()));
    }
    if count < PASSWORD_MIN {
        return Err(DomainError::WeakPassword(format!("A password has at least {PASSWORD_MIN} characters.")));
    }
    if count > PASSWORD_MAX {
        return Err(DomainError::WeakPassword(format!("A password has at most {PASSWORD_MAX} characters.")));
    }
    Ok(())
}

/// A board member as every client sees it; never the password's hash.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UserView {
    pub id: UserId,
    pub username: String,
    pub created_at_ms: i64,
    /// None: made in the onboarding, without an active board member.
    pub created_by_user: Option<UserId>,
}

/// `user.list`, oldest first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UsersSnapshot {
    pub users: Vec<UserView>,
}

/// `user.whoami`: what the onboarding gate needs (also in the hello).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct WhoAmI {
    pub board_members_exist: bool,
    /// The active board member of this connection.
    pub user: Option<UserView>,
}

/// The domain side of `user.create`: the name only. The password's hash is
/// passed beside it, straight to storage.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreateUser {
    pub username: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usernames_follow_ov_12() {
        assert_eq!(Username::parse("  grace ").unwrap().as_str(), "grace");
        for ok in ["abc", "a.b-c_d", "A1234567890123456789012345678901"] {
            assert!(Username::parse(ok).is_ok(), "{ok}");
        }
        for bad in ["", "ab", "a b c", "élodie", "x@y.z", &"a".repeat(33)] {
            assert!(matches!(Username::parse(bad), Err(DomainError::InvalidUsername(_))), "{bad}");
        }
    }

    #[test]
    fn passwords_have_at_least_twelve_characters() {
        assert!(matches!(check_password("short-pass1"), Err(DomainError::WeakPassword(_))));
        assert!(check_password("twelve-chars").is_ok());
        assert!(check_password(&"x".repeat(PASSWORD_MAX + 1)).is_err());
    }

    #[test]
    fn a_password_of_only_spaces_is_weak() {
        for blank in [" ".repeat(12), "\t".repeat(20), " \n ".repeat(6)] {
            assert!(matches!(check_password(&blank), Err(DomainError::WeakPassword(_))), "{blank:?}");
        }
        assert!(check_password("  twelve chars  ").is_ok());
    }
}
