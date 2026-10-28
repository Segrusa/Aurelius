//! Agent functions: the host that lets the remote gateway hand calls from apps
//! (API keys) to the service. The user's own methods (`agent_function.*`,
//! `api_key.*`, `function_call.list`) are in `api`.

use crate::Shared;
use aurelius_domain::agent_function::{CallAccepted, CallError, CallState};
use aurelius_remote_gateway::FunctionHost;
use std::sync::{Arc, Weak};

/// Weak: the hub lives inside `Shared`.
struct Host(Weak<Shared>);

impl FunctionHost for Host {
    fn call(&self, verifier: &str, slug: &str, params: &serde_json::Map<String, serde_json::Value>) -> Result<CallAccepted, CallError> {
        let Some(shared) = self.0.upgrade() else { return Err(CallError::Busy) };
        let (accepted, notices) = shared.service.call_agent_function(verifier, slug, params)?;
        shared.publish(notices);
        Ok(accepted)
    }

    fn state(&self, verifier: &str, call_id: &str) -> Result<CallState, CallError> {
        let Some(shared) = self.0.upgrade() else { return Err(CallError::Busy) };
        shared.service.agent_function_call_state(verifier, call_id)
    }
}

/// After `Shared` exists: connect the gateway to the service.
pub fn start(shared: &Arc<Shared>) {
    shared.remote.set_functions(Arc::new(Host(Arc::downgrade(shared))));
}
