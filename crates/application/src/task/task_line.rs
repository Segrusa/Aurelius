//! Tasks of an employee that works on one task at a time (`TaskPolicy::sequential_tasks`)
//! take turns, oldest first. Storage only has the queries.

use aurelius_domain::{EmployeeId, TaskId, task::TaskStatus};
use aurelius_storage::{StorageError, Tx, tasks as store};

/// Change a task's status; when a sequential employee's task stops running, the
/// next one in line gets its turn (in the same transaction).
pub(crate) fn set_status(tx: &Tx<'_>, id: TaskId, status: TaskStatus, waiting_reason: Option<&str>) -> Result<(), StorageError> {
    let before = store::status_and_employee(tx, id)?;
    store::set_task_status(tx, id, status, waiting_reason)?;
    if let Some((TaskStatus::Running, Some(employee))) = before {
        if status != TaskStatus::Running {
            next_in_line(tx, employee)?;
        }
    }
    Ok(())
}

/// Give the task to another employee; when a running task leaves its employee,
/// the turn of that employee is free.
pub(crate) fn set_employee(tx: &Tx<'_>, task: TaskId, employee: Option<EmployeeId>) -> Result<(), StorageError> {
    let before = store::status_and_employee(tx, task)?;
    store::set_task_employee(tx, task, employee)?;
    if let Some((TaskStatus::Running, Some(previous))) = before {
        if Some(previous) != employee {
            next_in_line(tx, previous)?;
        }
    }
    Ok(())
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
