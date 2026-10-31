//! Protocol 4 between the daemon and its clients (desktop, mobile, the ctl tool).
//!
//! * Every method is `<area>.<verb>`: reads are `list` or `get`, writes are verbs.
//! * Every write goes through one pipeline: an idempotency key in the envelope,
//!   a receipt, an audit event and the right the method requires.
//! * Events are a typed enum (`EventKind`).
//! * The handshake checks the major only (`PROTOCOL_MAJOR`); there are no
//!   minors. Additive features are named in the hello's `features` (`FEATURES`).
//! * Internal methods of the agent bridge live on their own channel (`bridge`).
//! * The types derive a JSON Schema; `cargo xtask gen-dart` turns it into the
//!   Dart types of the mobile app, and a test fails when the checked-in output drifts.
//!
//! Frames are a 4-byte big-endian length plus UTF-8 JSON (`aurelius_host::frame`).
//! Only `daemon`, `client` and `remote-gateway` depend on this crate.

pub mod app;
pub mod bridge;
pub mod events;
pub mod method;
pub mod remote;
pub mod rpc;
pub mod views;
pub mod write;

#[cfg(test)]
mod tickets_tests;

pub use aurelius_domain::{AppError as ProtocolError, CommandResult, ErrorCode, EventKind};
pub use bridge::{BridgeCall, BridgeOutcome, BridgeRequest, BridgeResponse, EmployeeTaskOp};
pub use events::Event;
pub use method::{HelloParams, Method, UserUseParams, client_feature};
pub use rpc::{Call, FEATURE_TERMINAL_MULTI, FEATURES, HelloResult, Outcome, PROTOCOL_MAJOR, Request, ResponseBody, ServerMessage, feature, incompatible_message, reject_unparsed};
pub use views::*;
pub use write::{HostResult, HostWrite, Receipt, UserCreateParams, WriteMethod, WriteResult};
