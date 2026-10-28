//! The audience of an app key (feature `management_channel`). An app with the
//! audience `management` reaches the CEO and the CEO's direct reports only; this
//! is enforced here, on the server, for every request of its session:
//!
//! * `check` runs before a request: who a task or a message is for, and which
//!   conversations it reads, must be in the management; the project assistant
//!   and the org chart are out of reach (`aurelius_protocol::app::allowed_for`).
//! * `filter` runs on the answer to a read: the employee list, the tasks, the
//!   approvals and the inbox only show the management and the app's own work.

use super::{AppPeer, storage_task};
use crate::Shared;
use aurelius_domain::app_key::{NOT_MANAGEMENT, NO_PROJECT_ASSISTANT};
use aurelius_domain::{AppKeyId, Command, EmployeeId};
use aurelius_protocol::{Call, ErrorCode, Method, ProtocolError, ResponseBody, WriteMethod};
use aurelius_storage::{StorageError, app_keys as keys};
use rusqlite::Connection;
use std::sync::Arc;

fn refused(why: &str) -> ProtocolError {
    ProtocolError::new(ErrorCode::Forbidden, why)
}

/// Whether `call` of a management app points only at the management.
pub(super) async fn check(shared: &Arc<Shared>, app: &AppPeer, call: &Call) -> Result<(), ProtocolError> {
    let (store, key, call) = (shared.store().clone(), app.key_id, call.clone());
    storage_task(move || store.read(|c| check_in(c, key, &call))).await?.map_err(|why| refused(&why))
}

fn check_in(conn: &Connection, key: AppKeyId, call: &Call) -> Result<Result<(), String>, StorageError> {
    let members = keys::management(conn)?;
    let member = |employee: EmployeeId| members.iter().any(|m| m.id == employee);
    // The employee behind a conversation: a management employee's task conversation is
    // fine, its persistent chat only when this app started it (or is writing to it now),
    // a project conversation (the project assistant's) never.
    let conversation = |id: aurelius_domain::ConversationId, writing: bool| -> Result<Result<(), String>, StorageError> {
        Ok(match keys::conversation_owner(conn, id)? {
            Some((Some(employee), _)) if !member(employee) => Err(NOT_MANAGEMENT.to_owned()),
            Some((Some(_), false)) => Ok(()),
            Some((Some(_), true)) if writing || keys::has_thread(conn, key, id)? => Ok(()),
            Some((Some(_), true)) => Err("Apps only read the conversations they started with ask_management.".to_owned()),
            Some((None, _)) => Err(NO_PROJECT_ASSISTANT.to_owned()),
            None => Err("This conversation does not exist.".to_owned()),
        })
    };
    Ok(match call {
        Call::Write(WriteMethod::Domain(command)) => match command {
            Command::CreateTask(task) => match task.employee_id {
                Some(employee) if !member(employee) => Err(NOT_MANAGEMENT.to_owned()),
                _ => Ok(()),
            },
            Command::SendEmployeeMessage(send) => {
                if !member(send.employee_id) {
                    return Ok(Err(NOT_MANAGEMENT.to_owned()));
                }
                // The conversation must be this employee's (a task's or its persistent chat).
                match keys::conversation_owner(conn, send.conversation_id)? {
                    Some((Some(employee), _)) if employee == send.employee_id => conversation(send.conversation_id, true)?,
                    _ => Err("That conversation is not this employee's.".to_owned()),
                }
            }
            Command::CancelTask(cancel) => match keys::task_employee(conn, cancel.task_id)? {
                Some(employee) if !member(employee) => Err(NOT_MANAGEMENT.to_owned()),
                _ => Ok(()),
            },
            Command::SetAgentMode(set) => conversation(set.conversation_id, false)?,
            Command::AnswerQuestion(answer) => match approval_employee(conn, answer.approval_id)? {
                Some(employee) if member(employee) => Ok(()),
                _ => Err("That question is not from the management.".to_owned()),
            },
            _ => Ok(()),
        },
        Call::Read(Method::ConversationGet { conversation_id }) => conversation(*conversation_id, false)?,
        _ => Ok(()),
    })
}

/// The employee an approval is for: the one whose conversation or run asked.
fn approval_employee(conn: &Connection, approval: aurelius_domain::ApprovalId) -> Result<Option<EmployeeId>, StorageError> {
    use rusqlite::OptionalExtension;
    let id: Option<Option<String>> = conn
        .query_row(
            "SELECT COALESCE(c.employee_id, r.employee_id) FROM approvals a JOIN runs r ON r.id = a.run_id
             LEFT JOIN conversations c ON c.id = r.conversation_id WHERE a.id = ?1",
            [approval.to_string()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(id.flatten().and_then(|e| EmployeeId::parse(&e).ok()))
}

/// The answer to a read, cut down to what a management app may see.
pub(super) async fn filter(shared: &Arc<Shared>, app: &AppPeer, body: ResponseBody) -> Result<ResponseBody, ProtocolError> {
    if !matches!(body, ResponseBody::Employees(_) | ResponseBody::Tasks(_) | ResponseBody::Approvals(_) | ResponseBody::Notifications(_)) {
        return Ok(body);
    }
    let (store, key) = (shared.store().clone(), app.key_id);
    storage_task(move || {
        store.read(|c| {
            let members = keys::management(c)?;
            let member = |employee: EmployeeId| members.iter().any(|m| m.id == employee);
            Ok(match body {
                ResponseBody::Employees(mut snapshot) => {
                    snapshot.employees.retain(|e| member(e.id));
                    ResponseBody::Employees(snapshot)
                }
                ResponseBody::Tasks(mut snapshot) => {
                    snapshot.tasks.retain(|t| t.employee_id.is_some_and(member) || t.created_by_app_key_id == Some(key));
                    ResponseBody::Tasks(snapshot)
                }
                ResponseBody::Approvals(mut snapshot) => {
                    snapshot.approvals.retain(|a| members.iter().any(|m| m.name == a.requester));
                    ResponseBody::Approvals(snapshot)
                }
                ResponseBody::Notifications(mut snapshot) => {
                    // Only results of tasks of the management (or without an employee yet).
                    let mut keep = Vec::new();
                    for n in snapshot.notifications.drain(..) {
                        let shown = match n.task_id {
                            Some(task) => keys::task_employee(c, task)?.is_none_or(member),
                            None => false,
                        };
                        if shown {
                            keep.push(n);
                        }
                    }
                    snapshot.notifications = keep;
                    ResponseBody::Notifications(snapshot)
                }
                other => other,
            })
        })
    })
    .await
}

