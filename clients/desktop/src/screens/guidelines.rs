//! Guidelines (feature `guidelines`; docs/product/guidelines, ui-ontwerp P-51..P-66).
//!
//! * Employees › Guidelines: the library (active or archived, with the open
//!   proposals as ◆n), an editor page with Write / Preview / Side by side, the
//!   counter against 8 000 and the largest block a run would get against 16 000
//!   (orange from 80 %, Save off above), History with a diff and Restore,
//!   Archive and Restore from the archive (GL-01..GL-04, GL-12, GL-16).
//! * Links… : employees grouped by whom they report to ("Reports to Titan · 4 of
//!   7"), and projects; a box that would push a run over 16 000 is off before the
//!   click (from `guideline.effective`; GL-05, GL-06, P-54, P-56). The same dialog
//!   the other way round from an employee or a project (Manage links…).
//! * What an employee gets per project and what a project adds (GL-07, P-55).
//! * Proposals of `propose_guideline` in the Inbox, beside the instruction proposals:
//!   the diff against the version they rest on, Approve (also edited, and for a
//!   new one with its links), Reject with a note (GL-13, GL-14, P-61, P-62).
//! * The trace's "Guidelines: Rust v3, …" (`run.guidelines`, GL-11, P-59).
//! * The empty library with New guideline and Add starter set…, and once the
//!   banner about the starter set, until Got it (per board member, kept locally; GL-18).
//!
//! Only a board member sees what changes something; the server refuses the rest
//! anyway. E14: a preview never shows HTML and never loads an image or anything
//! else from outside; a link shows its address as text (see [`safe_markdown`]).
//!
//! The pure logic is in `logic` (with its tests), the state and what the window does in
//! `state`; the library and the editor in `view`, the other places guidelines show in
//! `pages` (an employee, a project, the trace, the Inbox) and the dialogs in `dialogs`.

mod dialogs;
mod logic;
mod pages;
mod state;
mod view;

pub use logic::{READ_ONLY, context_guideline};
pub use state::{Guidelines, GuidelinesView};
