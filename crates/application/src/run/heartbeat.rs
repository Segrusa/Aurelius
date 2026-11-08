//! Heartbeats. Opt-in per employee. A cheap local check runs
//! first; only a concrete reason starts an employee run, missed occurrences
//! coalesce into one wake, quiet hours and a daily token budget apply, and the
//! user hears about it only when the employee has something to say.

use crate::run::events::{Ev, emit};
use crate::run::queue::queue_run_with_row;
use crate::{CommandService, Committed, domain_error, storage_error};
use aurelius_domain::{
    ActorKind, DomainError, MessageId, RunId,
    conversation::{MessageRole, MessageStatus},
    employee::SetHeartbeat,
};
use aurelius_domain::{CommandEnvelope, CommandResult, ErrorCode, EventNotice, AppError};
use aurelius_domain::scheduler::heartbeat::{self as rules, HeartbeatRule, Plan};
use aurelius_storage::{
    StorageError, blobs,
    conversations::{self as conv, NewMessage},
    heartbeats as hb, employees,
};

const SILENT_REPLY: &str = "Nothing to report";

impl CommandService {
    pub(crate) fn set_heartbeat(&self, ctx: &crate::CommandContext, envelope: &CommandEnvelope, hash: String, cmd: &SetHeartbeat) -> Result<Committed, AppError> {
        let login = ctx.login_user();
        let rule = if cmd.every_minutes == 0 {
            None
        } else {
            let mut rule = HeartbeatRule::new(cmd.every_minutes);
            rule.quiet_start_min = cmd.quiet_start_min.unwrap_or(rule.quiet_start_min);
            rule.quiet_end_min = cmd.quiet_end_min.unwrap_or(rule.quiet_end_min);
            rule.daily_token_budget = cmd.daily_token_budget.unwrap_or(rule.daily_token_budget);
            rule.validate().map_err(|m| AppError::new(ErrorCode::Validation, m))?;
            Some(rule)
        };
        let employee = cmd.employee_id;
        let every = cmd.every_minutes;
        let command_id = envelope.command_id;
        self.commit(command_id, hash, move |tx| {
            if employees::load_employee(tx, employee)?.is_none() {
                return Ok(Err(domain_error(DomainError::EmployeeNotFound(employee))));
            }
            let next_due = aurelius_storage::now_ms() + i64::from(every) * 60_000;
            hb::upsert(tx, employee, rule, next_due)?;
            // Its runs use the Claude login of the board member who turned it on (B5).
            if rule.is_some() && login.is_some() {
                hb::set_login_user(tx, employee, login.as_deref())?;
            }
            let (_, n) = emit(tx, Ev {
                aggregate_type: "heartbeat",
                aggregate_id: employee.to_string(),
                event_type: "heartbeat.changed",
                actor: ActorKind::User,
                actor_id: None,
                correlation: command_id.to_string(),
                payload: None,
                metadata: serde_json::json!({ "every_minutes": every, "rule": rule }),
                dedupe: format!("cmd:{command_id}:0"),
            })?;
            Ok(Ok((CommandResult::HeartbeatSet { employee_id: employee, every_minutes: every }, vec![n])))
        })
    }

    /// Evaluate every due heartbeat at `now_ms`. Idle ones only move their
    /// cursor and due time; a wake queues one employee run.
    pub fn evaluate_heartbeats(&self, now_ms: i64, offset_at: fn(i64) -> i64) -> Result<Vec<EventNotice>, AppError> {
        let due = self.store.read(|c| hb::due(c, now_ms)).map_err(storage_error)?;
        let mut notices = Vec::new();
        for schedule in due {
            match self.evaluate_one(schedule, now_ms, offset_at) {
                Ok(n) => notices.extend(n),
                Err(err) => tracing::warn!(error = %err.message, "heartbeat evaluation failed"),
            }
        }
        Ok(notices)
    }

    fn evaluate_one(&self, s: hb::Schedule, now: i64, offset_at: fn(i64) -> i64) -> Result<Vec<EventNotice>, AppError> {
        let id = s.id.clone();
        let (occurrence_ms, missed, next_due) = match rules::plan(&s.rule, s.next_due_ms.unwrap_or(now), now, offset_at) {
            Plan::Wait { next_due_ms } => {
                if Some(next_due_ms) != s.next_due_ms {
                    self.store.write(move |tx| hb::advance(tx, &id, next_due_ms, None, None)).map_err(storage_error)?;
                }
                return Ok(vec![]);
            }
            Plan::Quiet { next_due_ms } => {
                // The cursor stays: whatever happens now is looked at after quiet hours.
                self.store.write(move |tx| hb::advance(tx, &id, next_due_ms, None, Some("quiet_hours"))).map_err(storage_error)?;
                return Ok(vec![]);
            }
            Plan::Due { occurrence_ms, missed, next_due_ms } => (occurrence_ms, missed, next_due_ms),
        };
        let key = format!("{}:{occurrence_ms}", s.id);
        let day_start = rules::local_day_start(now, offset_at);
        let (busy, used, (reasons, newest), seen) = self
            .store
            .read(|c| Ok((hb::wake_active(c, &s.id)?, hb::tokens_since(c, &s.id, day_start)?, hb::reasons(c, &s)?, hb::wake_exists(c, &key)?)))
            .map_err(storage_error)?;
        if seen {
            self.store.write(move |tx| hb::advance(tx, &id, next_due, None, None)).map_err(storage_error)?;
            return Ok(vec![]);
        }
        if busy || used >= s.rule.daily_token_budget || reasons.is_empty() {
            // Busy: the running wake sees the same changes later (cursor kept).
            // Budget: wait for tomorrow's budget. Idle: nothing to look at, zero inference.
            let (cursor, waiting) = if busy {
                (None, Some("busy"))
            } else if used >= s.rule.daily_token_budget {
                (None, Some("budget"))
            } else {
                (Some(newest), None)
            };
            let next = if waiting == Some("budget") { next_due.max(day_start + 24 * 3_600_000) } else { next_due };
            self.store.write(move |tx| hb::advance(tx, &id, next, cursor, waiting)).map_err(storage_error)?;
            return Ok(vec![]);
        }

        let text = format!(
            "Heartbeat check-in. Since the last check: {}.{} Look at what changed and tell me only what needs my attention or a decision. If nothing does, reply exactly: {SILENT_REPLY}.",
            reasons.join("; "),
            if missed > 1 { format!(" ({missed} scheduled check-ins were combined into this one.)") } else { String::new() },
        );
        let text_blob = self.store.blobs().put(text.as_bytes(), "text/plain; charset=utf-8").map_err(storage_error)?;
        let employee = s.employee_id;
        let schedule_id = s.id.clone();
        let notices = self
            .store
            .write(move |tx| {
                if hb::wake_exists(tx, &key)? {
                    return Ok(vec![]);
                }
                let Some(row) = employees::load_employee(tx, employee)? else { return Ok(vec![]) };
                blobs::register(tx, &text_blob)?;
                let mut meta = serde_json::json!({ "origin": "heartbeat", "schedule_id": schedule_id, "reasons": reasons, "missed": missed });
                // On the login of the board member who turned it on (B5); none: it waits for one.
                meta["login_user"] = serde_json::json!(hb::login_user(tx, employee)?);
                let (event_id, n) = emit(tx, Ev {
                    aggregate_type: "heartbeat",
                    aggregate_id: schedule_id.clone(),
                    event_type: "wake.requested",
                    actor: ActorKind::System,
                    actor_id: Some("heartbeat".into()),
                    correlation: key.clone(),
                    payload: Some(text_blob.id.to_string()),
                    metadata: meta.clone(),
                    dedupe: format!("heartbeat:{key}"),
                })?;
                let message_id = MessageId::new();
                conv::insert_message(tx, NewMessage {
                    id: message_id,
                    conversation_id: row.conversation_id,
                    role: MessageRole::User,
                    content_blob_id: Some(text_blob.id.to_string()),
                    source_event_id: event_id,
                    status: MessageStatus::Complete,
                    run_id: None,
                    reply_to: None,
                })?;
                let (run_id, run_notice) = queue_run_with_row(tx, &meta, &key, row.conversation_id, message_id)?;
                hb::record_wake(tx, &schedule_id, run_id, &key, &reasons, missed)?;
                hb::advance(tx, &schedule_id, next_due, Some(newest), None)?;
                Ok(std::iter::once(n).chain(run_notice).collect())
            })
            .map_err(storage_error)?;
        Ok(notices)
    }
}

/// Called inside the transaction that finishes a run: record the heartbeat
/// outcome and notify only when the employee has something to say.
pub(crate) fn on_run_finished(tx: &rusqlite::Transaction<'_>, run_id: RunId, completed: bool, reply: &str) -> Result<Option<EventNotice>, StorageError> {
    let Some((schedule, employee_id, employee_name)) = hb::wake_for_run(tx, run_id)? else { return Ok(None) };
    let meaningful = completed && rules::is_meaningful(reply);
    hb::set_outcome(tx, run_id, if !completed { "failed" } else if meaningful { "reported" } else { "silent" })?;
    if !meaningful {
        return Ok(None);
    }
    let (_, n) = emit(tx, Ev {
        aggregate_type: "heartbeat",
        aggregate_id: schedule,
        event_type: "heartbeat.reported",
        actor: ActorKind::Employee,
        actor_id: Some(employee_id.clone()),
        correlation: run_id.to_string(),
        payload: None,
        metadata: serde_json::json!({ "subject": employee_name, "employee_id": employee_id }),
        dedupe: format!("run:{run_id}:heartbeat"),
    })?;
    Ok(Some(n))
}

#[cfg(test)]
mod tests {
    use crate::*;
    use aurelius_domain::employee::{CreateEmployee, SetHeartbeat};
    use aurelius_domain::project::CreateProject;
    use aurelius_domain::task::CreateTask;

    fn ctx() -> CommandContext {
        CommandContext { principal_id: PrincipalId::new(), client_id: ClientId::new(), origin: Origin::LocalIpc, request_id: 1, app: None, user_id: None }
    }

    fn run(service: &CommandService, command: Command) -> CommandResult {
        service.execute(&ctx(), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }).unwrap().receipt.result
    }

    fn count(service: &CommandService, sql: &str) -> i64 {
        service.store().read(|c| Ok(c.query_row(sql, [], |r| r.get(0))?)).unwrap()
    }

    fn due_at(service: &CommandService) -> i64 {
        count(service, "SELECT next_due_ms FROM heartbeats")
    }

    #[test]
    fn idle_heartbeat_makes_no_runs_and_missed_occurrences_coalesce() {
        let dir = tempfile::tempdir().unwrap();
        let service = CommandService::new(std::sync::Arc::new(Store::open(dir.path()).unwrap()));
        let CommandResult::ProjectCreated { project_id, .. } = run(&service, Command::CreateProject(CreateProject { name: "Atlas".into(), description: None, folder: None })) else { panic!() };
        let CommandResult::EmployeeCreated { employee_id, .. } =
            run(&service, Command::CreateEmployee(CreateEmployee { access_level: None, description: None, name: "Alex".into(), role: "Writer".into(), title: None, reports_to: None, instructions: None, folder: None }))
        else {
            panic!()
        };
        // Quiet hours off so the test doesn't depend on the time of day.
        run(&service, Command::SetHeartbeat(SetHeartbeat { employee_id, every_minutes: 30, quiet_start_min: Some(0), quiet_end_min: Some(0), daily_token_budget: None }));
        let runs = || count(&service, "SELECT COUNT(*) FROM runs");
        let before = runs();

        // Idle for ten hours (20 occurrences): no run, the due time moves on.
        let later = due_at(&service) + 10 * 3_600_000;
        service.evaluate_heartbeats(later, |_| 0).unwrap();
        assert_eq!(runs(), before, "an idle heartbeat makes zero inference calls");
        assert!(due_at(&service) > later);

        // Something relevant happens: a task of Alex's fails (written as Alex's
        // without starting it, so the only run is the wake's).
        let CommandResult::TaskCreated { task_id, .. } = run(&service, Command::CreateTask(CreateTask { confirm_secret: false, files: vec![], run: Default::default(),
            project_id,
            employee_id: None,
            title: "Launch".into(),
            goal: "Plan the launch".into(),
            acceptance_criteria: vec![],
            depends_on: vec![],
        })) else {
            panic!()
        };
        service
            .store()
            .write(move |tx| {
                tx.execute("UPDATE tasks SET employee_id = ?2 WHERE id = ?1", [task_id.to_string(), employee_id.to_string()])?;
                aurelius_storage::events::append(tx, aurelius_storage::events::NewEvent {
                    aggregate_type: "task",
                    aggregate_id: task_id.to_string(),
                    aggregate_version: 2,
                    event_type: "task.failed",
                    project_id: Some(project_id),
                    actor_kind: aurelius_domain::ActorKind::System,
                    actor_id: None,
                    correlation_id: "test".into(),
                    causation_id: None,
                    payload_blob_id: None,
                    metadata: serde_json::json!({}),
                    dedupe_key: "test:failed".into(),
                })
            })
            .unwrap();

        // The laptop slept through 20 more occurrences: exactly one wake.
        let much_later = due_at(&service) + 10 * 3_600_000;
        service.evaluate_heartbeats(much_later, |_| 0).unwrap();
        assert_eq!(runs(), before + 1, "missed occurrences coalesce into one wake");
        assert_eq!(count(&service, "SELECT missed FROM heartbeat_wakes"), 21);
        let text: String = service
            .store()
            .read(|c| Ok(c.query_row("SELECT reasons_json FROM heartbeat_wakes", [], |r| r.get(0))?))
            .unwrap();
        assert!(text.contains("Launch") && text.contains("failed"), "{text}");

        // While that wake is still running, the next occurrence doesn't stack another.
        let next = due_at(&service) + 1;
        service.evaluate_heartbeats(next, |_| 0).unwrap();
        assert_eq!(runs(), before + 1, "no second wake while one is running");
        // Re-evaluating the same moment is a no-op (deterministic occurrence key).
        service.evaluate_heartbeats(next, |_| 0).unwrap();
        assert_eq!(count(&service, "SELECT COUNT(*) FROM heartbeat_wakes"), 1);

        // Off means off.
        run(&service, Command::SetHeartbeat(SetHeartbeat { employee_id, every_minutes: 0, quiet_start_min: None, quiet_end_min: None, daily_token_budget: None }));
        assert!(service.store().read(|c| aurelius_storage::heartbeats::due(c, i64::MAX)).unwrap().is_empty());
    }
}
