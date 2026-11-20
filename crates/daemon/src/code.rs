//! The Code tab (ticket #77, feature `code_view`): `code.get` reads a project's git state
//! and the `code.*` host writes change it on a board member's request. Never a push.
//! Git runs through `quick`'s helpers (no prompt, no inherited `GIT_DIR`), and every
//! write takes `quick`'s git lock, so it never overlaps a quick task's merge.

use crate::Shared;
use aurelius_domain::ProjectId;
use aurelius_domain::code::CodeView;
use aurelius_protocol::{ErrorCode, HostWrite, ProtocolError};
use std::sync::Arc;

/// `code.get`: the project's git state.
pub async fn view(_shared: &Arc<Shared>, _project: ProjectId) -> Result<CodeView, ProtocolError> {
    Err(ProtocolError::new(ErrorCode::Internal, "code.get is not built yet"))
}

/// A `code.*` host write, then the view as it is after it.
pub async fn write(_shared: &Arc<Shared>, _write: HostWrite) -> Result<CodeView, ProtocolError> {
    Err(ProtocolError::new(ErrorCode::Internal, "code writes are not built yet"))
}
