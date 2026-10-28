//! Departments: a named group of employees with a head and its own memory
//! scope. An employee belongs to one department or none. Archiving a
//! department keeps its employees working (they leave it) and clears its head.

use crate::{DepartmentId, DomainError, EmployeeId, optional_text, require_text};
use serde::{Deserialize, Serialize};

pub const NAME_MAX: usize = 80;
pub const DESCRIPTION_MAX: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct CreateDepartment {
    /// Unique among the active departments (case-insensitive).
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The employee who leads it (not archived); it also joins the department.
    #[serde(default)]
    pub head_id: Option<EmployeeId>,
}

/// Name, description and head as given: `None` or an empty description clears it,
/// `None` as head leaves the department without one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct UpdateDepartment {
    pub department_id: DepartmentId,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub head_id: Option<EmployeeId>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ArchiveDepartment {
    pub department_id: DepartmentId,
}

/// Validated department fields: (name, description).
pub fn validate_fields(name: &str, description: Option<&str>) -> Result<(String, Option<String>), DomainError> {
    Ok((require_text("name", name, NAME_MAX)?, optional_text("description", description, DESCRIPTION_MAX)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_department_needs_a_name_and_may_describe_itself() {
        assert_eq!(validate_fields("  Engineering ", Some("  ")).unwrap(), ("Engineering".to_string(), None));
        assert!(validate_fields(" ", None).is_err());
        assert!(validate_fields("Ops", Some(&"x".repeat(DESCRIPTION_MAX + 1))).is_err());
    }
}
