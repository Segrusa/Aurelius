//! The internal bridge channel: the one place the agent bridge (`aurelius-bridge`)
//! and `aurelius-ctl shutdown` touch the wire. One connection, one call in
//! flight (a permission ask can wait minutes); open another `BridgeClient` for
//! a concurrent call.

use crate::ClientError;
use aurelius_host::ipc::{self, BoxStream, read_frame, write_frame};
use aurelius_protocol::{BridgeCall, BridgeOutcome, BridgeRequest, BridgeResponse, PROTOCOL_MAJOR};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::Mutex;

pub use aurelius_domain::{RunId, idea::IdeaKind, memory::EmployeeMemoryOp};
pub use aurelius_protocol::EmployeeTaskOp;

/// The user's decision on a tool Claude Code asked about.
#[derive(Debug, Clone, PartialEq)]
pub struct PermissionAnswer {
    pub allow: bool,
    pub message: Option<String>,
    /// The tool input to use instead (e.g. a question with its answers).
    pub updated_input: Option<serde_json::Value>,
    /// Permission changes to apply with an allow.
    pub updated_permissions: Option<serde_json::Value>,
}

pub struct BridgeClient {
    stream: Mutex<BoxStream>,
    next_id: AtomicU64,
}

impl BridgeClient {
    /// Connect to the running daemon's bridge endpoint; never starts a daemon.
    pub async fn connect(root: &Path) -> Result<Self, ClientError> {
        let stream = ipc::connect(&aurelius_host::bridge_endpoint(root)).await.map_err(|e| ClientError::Unreachable(e.to_string()))?;
        Ok(Self { stream: Mutex::new(stream), next_id: AtomicU64::new(1) })
    }

    async fn call(&self, call: BridgeCall) -> Result<BridgeOutcome, ClientError> {
        let request_id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let mut stream = self.stream.lock().await;
        write_frame(&mut *stream, &BridgeRequest { protocol: PROTOCOL_MAJOR, request_id, call }).await.map_err(|_| ClientError::Disconnected)?;
        loop {
            match read_frame::<BridgeResponse, _>(&mut *stream).await {
                Ok(Some(response)) if response.request_id == request_id => return Ok(response.outcome),
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => return Err(ClientError::Disconnected),
            }
        }
    }

    /// Ask the user (through the daemon) to allow a tool; waits for the decision.
    pub async fn permission_ask(&self, run_id: RunId, call_id: Option<String>, tool: String, input: serde_json::Value) -> Result<PermissionAnswer, ClientError> {
        match self.call(BridgeCall::PermissionAsk { run_id, call_id, tool, input }).await? {
            BridgeOutcome::Permission { allow, message, updated_input, updated_permissions } => Ok(PermissionAnswer { allow, message, updated_input, updated_permissions }),
            other => Err(unexpected(other)),
        }
    }

    /// An employee's own memory during a run. Returns (ok, text for the model).
    pub async fn memory_run(&self, run_id: RunId, op: EmployeeMemoryOp) -> Result<(bool, String), ClientError> {
        text(self.call(BridgeCall::MemoryRun { run_id, op }).await?)
    }

    /// An employee's action during a run (delegate, hand over, ideas, reads, proposals).
    pub async fn employee_act(&self, run_id: RunId, op: EmployeeTaskOp) -> Result<(bool, String), ClientError> {
        text(self.call(BridgeCall::EmployeeAct { run_id, op }).await?)
    }

    /// Stop the daemon; the connection may drop before the answer.
    pub async fn shutdown(&self) -> Result<(), ClientError> {
        match self.call(BridgeCall::DaemonShutdown).await {
            Ok(BridgeOutcome::Done) | Err(ClientError::Disconnected) => Ok(()),
            Ok(other) => Err(unexpected(other)),
            Err(err) => Err(err),
        }
    }
}

fn text(outcome: BridgeOutcome) -> Result<(bool, String), ClientError> {
    match outcome {
        BridgeOutcome::Text { ok, text } => Ok((ok, text)),
        other => Err(unexpected(other)),
    }
}

fn unexpected(outcome: BridgeOutcome) -> ClientError {
    match outcome {
        BridgeOutcome::Error { message } => ClientError::Local(message),
        _ => ClientError::UnexpectedResponse,
    }
}

/// Stop the daemon of data folder `root` (`aurelius-ctl shutdown`).
pub async fn shutdown_daemon(root: &Path) -> Result<(), ClientError> {
    BridgeClient::connect(root).await?.shutdown().await
}
