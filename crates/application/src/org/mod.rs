//! The organisation of the company: the org chart and proposals, employees,
//! departments and project folders.

pub mod folders;
pub mod employee;
pub mod department;
mod commands;

pub(crate) use commands::*;
