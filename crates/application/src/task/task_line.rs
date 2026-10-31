//! Tasks of an employee that works on one task at a time (`TaskPolicy::sequential_tasks`)
//! take turns, oldest first. Storage only has the queries.

use aurelius_domain::{EmployeeId, EventNotice, TaskId, task::TaskStatus};
use aurelius_storage::{StorageError, Tx, tasks as store};

/// The notices of a ticket's card that followed its main task (`ticket_line`).
/// They must reach the clients with the rest of the change, or the board shows
/// an old column (and a board member's next write conflicts on the version):
/// unused, the compiler says so, also after a `?`.
#[must_use = "the card's notices must be published with the change"]
#[derive(Debug, Default)]
pub(crate) struct CardNotices(Vec<EventNotice>);

impl CardNotices {
    pub(crate) fn into_vec(self) -> Vec<EventNotice> {
        self.0
    }
}

impl IntoIterator for CardNotices {
    type Item = EventNotice;
    type IntoIter = std::vec::IntoIter<EventNotice>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Change a task's status; when a sequential employee's task stops running, the
/// next one in line gets its turn (in the same transaction). The card of a
/// ticket whose main task this is follows (`ticket_line::on_status`); the
/// notices are those of the card.
///
/// The one place every status change goes through, so it checks the matrix
/// (`task::can_transition`): a change it does not know is a bug (a panic in
/// debug builds); in release it is logged and goes ahead, so an order nobody
/// saw yet does not block work. The same status again only changes why it waits.
#[must_use]
pub(crate) fn set_status(tx: &Tx<'_>, id: TaskId, status: TaskStatus, waiting_reason: Option<&str>) -> Result<CardNotices, StorageError> {
    let before = store::status_and_employee(tx, id)?;
    if let Some((from, _)) = before {
        if from != status && !aurelius_domain::task::can_transition(from, status) {
            tracing::warn!(task = %id, from = from.as_str(), to = status.as_str(), "task status change outside the matrix");
            debug_assert!(false, "task {id}: {} → {} is not in the matrix", from.as_str(), status.as_str());
        }
    }
    store::set_task_status(tx, id, status, waiting_reason)?;
    if let Some((TaskStatus::Running, Some(employee))) = before {
        if status != TaskStatus::Running {
            next_in_line(tx, employee)?;
        }
    }
    crate::task::ticket_line::on_status(tx, id, status, waiting_reason).map(CardNotices)
}

/// Give the task to another employee; when a running task leaves its employee,
/// the turn of that employee is free. The owner of a ticket follows its main task.
#[must_use]
pub(crate) fn set_employee(tx: &Tx<'_>, task: TaskId, employee: Option<EmployeeId>) -> Result<CardNotices, StorageError> {
    let before = store::status_and_employee(tx, task)?;
    store::set_task_employee(tx, task, employee)?;
    if let Some((TaskStatus::Running, Some(previous))) = before {
        if Some(previous) != employee {
            next_in_line(tx, previous)?;
        }
    }
    crate::task::ticket_line::on_employee(tx, task, employee).map(CardNotices)
}

/// A task of a sequential employee must wait for its turn: another of its tasks
/// is running, or an older one is queued before it.
pub(crate) fn waits_its_turn(tx: &Tx<'_>, employee: EmployeeId, task: TaskId) -> Result<bool, StorageError> {
    if !store::sequential(tx, employee)? {
        return Ok(false);
    }
    Ok(store::has_running_task(tx, employee, Some(task))? || store::queued_before(tx, employee, task)?)
}

/// The turn of a sequential employee is free (none of its tasks runs): the
/// oldest task waiting for it starts (its job goes again now).
pub(crate) fn next_in_line(tx: &Tx<'_>, employee: EmployeeId) -> Result<(), StorageError> {
    if !store::sequential(tx, employee)? || store::has_running_task(tx, employee, None)? {
        return Ok(());
    }
    if let Some(next) = store::tasks_in_line(tx, employee)?.first() {
        store::wake_job(tx, next)?;
    }
    Ok(())
}

/// The employee no longer works on one task at a time: every task waiting for
/// its turn starts.
pub(crate) fn release_line(tx: &Tx<'_>, employee: EmployeeId) -> Result<(), StorageError> {
    for task in store::tasks_in_line(tx, employee)? {
        store::wake_job(tx, &task)?;
    }
    Ok(())
}
