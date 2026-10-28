//! Task variants (Harness² C, opt-in): before the real work a task is tried in
//! 2 or 3 ways, one after the other, each in a throwaway git worktree of the
//! work folder (its own conversation, sandboxed). A judge compares them; the
//! user (in Auto: the judge) chooses one, its diff goes into the work folder
//! and the task goes on as usual. The task job drives the steps
//! (`daemon::variants`); this module keeps their state.

use crate::task::task_threads::{self as thread, Speaker};
use crate::task::tasks::{TaskContext, task_event};
use crate::{CommandService, Committed, domain_error, storage_error};
use aurelius_domain::variants::{self as v, ChooseTaskVariant, Judgement, VariantPhase, VariantStatus};
use aurelius_domain::views::TaskVariantsView;
use aurelius_domain::{AppError, CommandEnvelope, CommandResult, ConversationId, DomainError, EventNotice, EmployeeId, ProjectId, RunId, TaskId, task::TaskStatus};
use aurelius_storage::variants::{self as store, SetRow, VariantRow};
use aurelius_storage::{StorageError, Store, Tx, blobs, employees, tasks as tasks_store};
use rusqlite::Connection;

/// How a variant's conversation is told to work (stable per variant: its approach
/// is in the variable tail of the system text).
pub const PROBE_GUIDANCE: &str = "This conversation is one trial of a task: the same task is tried in several ways, each in a separate copy of the work folder, and the user compares them before the real work. Do the task completely in this copy, in the way your approach says; work only in this folder. Build into this folder's own target directory. Nothing outside is allowed in a trial (pushing, publishing, delegating, MCP tools, questions to the user): it is refused, so do not try. Run the tests if there are any. Close with a short report: what you changed, what you checked and what is left.";

/// The first git repository among the folders the task's employee works in.
pub(crate) fn git_repo(conn: &Connection, project: ProjectId, employee: Option<EmployeeId>) -> Result<Option<String>, StorageError> {
    let mut folders = vec![aurelius_storage::files::project_folder(conn, project)?];
    if let Some(m) = employee {
        folders.push(aurelius_storage::files::employee_folder(conn, m)?);
    }
    Ok(folders.into_iter().flatten().find(|f| std::path::Path::new(f).join(".git").exists()))
}

/// A task that asks for variants may be made: the setting is on, it has a
/// employee, and that employee works in a git repository.
pub(crate) fn check_new(tx: &Tx<'_>, run: &aurelius_domain::task::RunSettings, project: ProjectId, employee: Option<EmployeeId>) -> Result<Result<(), AppError>, StorageError> {
    if run.variants.unwrap_or(1) <= 1 {
        return Ok(Ok(()));
    }
    let refuse = |detail: &str| Ok(Err(domain_error(DomainError::OutOfRange { field: "variants".into(), detail: detail.into() })));
    if !store::enabled(tx)? {
        return refuse("variants are off on this server; turn them on in Settings (task variants)");
    }
    if employee.is_none() {
        return refuse("a task tried in several ways needs an employee");
    }
    if git_repo(tx, project, employee)?.is_none() {
        return refuse("variants need a work folder that is a git repository (the project's or the employee's folder)");
    }
    Ok(Ok(()))
}

/// A task's variants as the task job finds them.
#[derive(Debug, Clone)]
pub struct VariantsState {
    pub set: SetRow,
    pub variants: Vec<VariantRow>,
    /// The task works in Auto: the judge chooses.
    pub auto: bool,
}

/// What the propose step needs.
#[derive(Debug, Clone)]
pub struct HintsInput {
    pub count: u8,
    pub title: String,
    pub goal: String,
    /// Lessons (Harness² A) the employee and the project learned.
    pub lessons: Vec<String>,
    pub config: crate::run::connections::ResolvedRunConfig,
}

/// What the judge needs of one variant.
#[derive(Debug, Clone)]
pub struct JudgeItem {
    pub n: u8,
    pub hint: String,
    pub status: VariantStatus,
    pub files: u32,
    pub insertions: u32,
    pub deletions: u32,
    pub diff: String,
    pub report: String,
    pub errors: u32,
    pub tests: Option<String>,
    pub tokens: u64,
}

/// What a variant's run left, collected from its worktree.
#[derive(Debug, Clone, Default)]
pub struct Outcome {
    pub files: u32,
    pub insertions: u32,
    pub deletions: u32,
    pub diff: String,
    pub truncated: bool,
}

/// What a variant's runs did with their tools: failed calls, the last test output.
fn checks(conn: &Connection, conversation: ConversationId) -> Result<v::ProbeChecks, StorageError> {
    let errors: i64 = conn.query_row(
        "SELECT COUNT(*) FROM run_activity a JOIN runs r ON r.id = a.run_id WHERE r.conversation_id = ?1 AND a.ok = 0",
        [conversation.to_string()],
        |r| r.get(0),
    )?;
    let tests: Option<String> = conn
        .query_row(
            "SELECT a.output FROM run_activity a JOIN runs r ON r.id = a.run_id
             WHERE r.conversation_id = ?1 AND a.tool IN ('Bash','PowerShell') AND a.output IS NOT NULL
               AND (lower(json_extract(a.input_json, '$.command')) LIKE '%test%' OR lower(json_extract(a.input_json, '$.command')) LIKE '%build%'
                    OR lower(json_extract(a.input_json, '$.command')) LIKE '%check%')
             ORDER BY a.started_at_ms DESC LIMIT 1",
            [conversation.to_string()],
            |r| r.get(0),
        )
        .ok();
    let tests = tests.map(|t| {
        let t = t.trim();
        let n = t.chars().count();
        if n > 1_500 { format!("…{}", t.chars().skip(n - 1_500).collect::<String>()) } else { t.to_owned() }
    });
    Ok(v::ProbeChecks { errors: errors.max(0) as u32, tests })
}

/// The task's variants end here (stopped or cancelled): what still waits or runs is
/// cancelled, the set is stopped. Returns the notices; nothing when there is no
/// set or it is over already.
pub(crate) fn stop(tx: &Tx<'_>, task: TaskId, why: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(set) = store::load_set(tx, task)? else { return Ok(vec![]) };
    if set.phase.is_over() {
        return Ok(vec![]);
    }
    for variant in store::variants(tx, task)? {
        store::end_variant(tx, task, variant.n, VariantStatus::Cancelled)?;
    }
    store::set_phase(tx, task, VariantPhase::Stopped, None)?;
    let Some(row) = tasks_store::load_task(tx, task)? else { return Ok(vec![]) };
    Ok(vec![task_event(tx, task, row.project_id, "task.variants_stopped", serde_json::json!({ "reason": why }))?])
}

/// A run ended: when it is a variant's, the variant ends with it and the task
/// job takes the next step. `None` when the run is not a variant's.
pub(crate) fn on_probe_run_finished(tx: &Tx<'_>, st: &Store, run: RunId, completed: bool, failed: Option<&str>, answer: &str) -> Result<Option<Vec<EventNotice>>, StorageError> {
    let Some(probe) = store::probe_of_run(tx, run)? else { return Ok(None) };
    let task = probe.task_id;
    let status = match (completed, failed) {
        (true, _) => VariantStatus::Done,
        (false, Some(_)) => VariantStatus::Failed,
        (false, None) => VariantStatus::Cancelled,
    };
    if !store::end_variant(tx, task, probe.n, status)? {
        return Ok(Some(vec![]));
    }
    if !answer.trim().is_empty() {
        let blob = st.blobs().put(answer.trim().as_bytes(), "text/markdown; charset=utf-8")?;
        blobs::register(tx, &blob)?;
        tx.execute("UPDATE task_variants SET report_blob_id = ?3 WHERE task_id = ?1 AND n = ?2", rusqlite::params![task.to_string(), probe.n, blob.id.to_string()])?;
    }
    let Some(row) = tasks_store::load_task(tx, task)? else { return Ok(Some(vec![])) };
    let mut notices = vec![task_event(tx, task, row.project_id, "task.variant_probed", serde_json::json!({ "n": probe.n, "status": status.as_str() }))?];
    if status == VariantStatus::Cancelled && store::phase(tx, task)? == Some(VariantPhase::Probing) {
        // Stopping one trial stops them all: the task waits for the user, its work folder as it was.
        notices.extend(stop(tx, task, "stopped")?);
        if !row.status.is_terminal() {
            notices.extend(wait_for_user(tx, st, task, "The trials were stopped; the work folder is as it was. Write here to go on with the task in the usual way.")?);
        }
    }
    crate::task::task_runs::wake_task_job(tx, task, false)?;
    Ok(Some(notices))
}

/// The task waits for the user in its own conversation (opened if it has none yet), with `note`.
fn wait_for_user(tx: &Tx<'_>, st: &Store, task: TaskId, note: &str) -> Result<Vec<EventNotice>, StorageError> {
    let Some(row) = tasks_store::load_task(tx, task)? else { return Ok(vec![]) };
    let Some(employee) = row.employee_id else { return Ok(vec![]) };
    let mut notices = Vec::new();
    let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
    let brief = format!("**{}**\n\n**Goal**\n\n{}\n", row.title, read(&row.goal_key).unwrap_or_default().trim());
    let (conversation, _, opened) = thread::open_returning(tx, st, task, employee, &brief)?;
    notices.extend(opened);
    if let Some(conversation) = conversation {
        notices.push(thread::post_to(tx, st, conversation, task, Speaker::Update { kind: "status", from: "Aurelius" }, note)?);
    }
    if row.status != TaskStatus::Waiting {
        crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("input"))?;
        notices.push(task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": "stopped" }))?);
    }
    Ok(notices)
}

impl CommandService {
    /// The task's variants, made now when the task asks for some and has none yet
    /// (the task starts running); `None` when it asks for none.
    pub fn variants_state(&self, ctx: &TaskContext, epoch: i64) -> Result<Option<VariantsState>, AppError> {
        let (task, project, employee) = (ctx.task_id, ctx.project_id, ctx.employee_id);
        self.store
            .write(move |tx| {
                let run = tasks_store::task_run(tx, task)?;
                let count = run.variants.unwrap_or(1);
                if count <= 1 {
                    return Ok(None);
                }
                // The task's own mode, else the employee's: in Auto the judge chooses.
                let mode = match run.mode {
                    Some(mode) => Some(mode),
                    None => {
                        let main: Option<String> = tx
                            .query_row("SELECT agent_mode FROM conversations WHERE employee_id = ?1 AND kind = 'employee' ORDER BY created_at_ms LIMIT 1", [employee.map(|m| m.to_string())], |r| r.get(0))
                            .ok();
                        main.map(|m| aurelius_domain::autonomy::AgentMode::parse(&m))
                    }
                };
                let auto = mode == Some(aurelius_domain::autonomy::AgentMode::Auto);
                if store::load_set(tx, task)?.is_none() {
                    if !aurelius_storage::queries::holds_lease(tx, &task.to_string(), epoch)? {
                        return Ok(None);
                    }
                    let repo = git_repo(tx, project, employee)?;
                    store::create_set(tx, task, count, repo.as_deref().unwrap_or(""))?;
                    if repo.is_none() {
                        // The folder is no git repository (any more): the task goes on as usual.
                        store::set_phase(tx, task, VariantPhase::Failed, Some("the work folder is not a git repository"))?;
                    } else if let Some(row) = tasks_store::load_task(tx, task)? {
                        if row.status == TaskStatus::Queued {
                            crate::task::task_line::set_status(tx, task, TaskStatus::Running, None)?;
                        }
                    }
                }
                let Some(set) = store::load_set(tx, task)? else { return Ok(None) };
                Ok(Some(VariantsState { variants: store::variants(tx, task)?, set, auto }))
            })
            .map_err(storage_error)
    }


    /// What the propose step asks the cheap model about.
    pub fn variants_hints_input(&self, ctx: &TaskContext, count: u8) -> Result<Option<HintsInput>, AppError> {
        let Some(employee) = ctx.employee_id else { return Ok(None) };
        let project = ctx.project_id;
        let config = self.resolve_for_employee(employee, aurelius_domain::connection::Purpose::Memory)?;
        let st = self.store.clone();
        let lessons = self
            .store
            .read(move |conn| {
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                // Its own lessons and the task's project's.
                let scopes = aurelius_storage::memory::employee_read_scopes(conn, employee, Some(project))?;
                let scopes: Vec<String> = [scopes.own, scopes.project].into_iter().flatten().collect();
                Ok(aurelius_storage::memory::lessons(conn, &scopes, &read)?.into_iter().map(|(_, text)| text).take(12).collect::<Vec<_>>())
            })
            .map_err(storage_error)?;
        Ok(Some(HintsInput { count, title: ctx.title.clone(), goal: ctx.goal.clone(), lessons, config }))
    }

    /// The approaches are chosen: the variants wait for their turn.
    pub fn record_variant_hints(&self, task: TaskId, epoch: i64, base_commit: &str, hints: &[String]) -> Result<Vec<EventNotice>, AppError> {
        let (base, hints) = (base_commit.to_owned(), hints.to_vec());
        self.store
            .write(move |tx| {
                if !aurelius_storage::queries::holds_lease(tx, &task.to_string(), epoch)? || store::phase(tx, task)? != Some(VariantPhase::Proposing) {
                    return Ok(vec![]);
                }
                store::set_hints(tx, task, &base, &hints)?;
                let Some(row) = tasks_store::load_task(tx, task)? else { return Ok(vec![]) };
                Ok(vec![task_event(tx, task, row.project_id, "task.variants_started", serde_json::json!({ "count": hints.len(), "hints": hints, "base_commit": base }))?])
            })
            .map_err(storage_error)
    }

    /// Variant `n` starts in `worktree`: a conversation of its own (the task's
    /// employee, Accept edits), its brief, and a run.
    pub fn start_variant(&self, ctx: &TaskContext, epoch: i64, n: u8, count: u8, worktree: &str) -> Result<Vec<EventNotice>, AppError> {
        let Some(employee) = ctx.employee_id else { return Ok(vec![]) };
        let (task, project, st) = (ctx.task_id, ctx.project_id, self.store.clone());
        let worktree = worktree.to_owned();
        let mut brief = format!(
            "**{}** (trial {n} of {count})\n\nTry this task in the approach given to you, in this separate copy of the work folder: {worktree}. Its build folder is {}/target.\n\n**Goal**\n\n{}\n",
            ctx.title,
            worktree.replace('\\', "/"),
            ctx.goal.trim()
        );
        if !ctx.criteria.is_empty() {
            brief.push_str("\n**Acceptance criteria**\n\n");
            for c in &ctx.criteria {
                brief.push_str(&format!("- {c}\n"));
            }
        }
        let title: String = format!("Trial {n}: {}", ctx.title).chars().take(80).collect();
        self.store
            .write(move |tx| {
                if !aurelius_storage::queries::holds_lease(tx, &task.to_string(), epoch)? {
                    return Ok(vec![]);
                }
                let conversation = ConversationId::new();
                if !employees::insert_thread(tx, employee, conversation, aurelius_domain::conversation::ConversationKind::Task, Some(project))? {
                    return Ok(vec![]);
                }
                // Trials work in Accept edits inside their worktree, whatever the task's mode.
                tx.execute("UPDATE conversations SET title = ?2, agent_mode = 'accept_edits' WHERE id = ?1", rusqlite::params![conversation.to_string(), title])?;
                // The trial's conversation is known before its run is queued: the run plans in its worktree.
                store::start_variant(tx, task, n, conversation, &worktree)?;
                let (message, posted) = thread::post_returning(tx, &st, conversation, task, Speaker::Update { kind: "brief", from: "Task" }, &brief)?;
                let (run, queued) = crate::run::queue::queue_run(tx, &serde_json::json!({ "task_id": task, "variant": n }), &task.to_string(), conversation, message)?;
                store::set_variant_run(tx, task, n, run)?;
                let project = ctx_project(tx, task)?;
                let probed = task_event(tx, task, project, "task.variant_probed", serde_json::json!({ "n": n, "status": "running", "conversation_id": conversation, "run_id": run }))?;
                Ok(vec![posted, queued, probed])
            })
            .map_err(storage_error)
    }

    /// Keep what variant `n` left in its worktree (diff and its size) and what its tools did.
    pub fn record_variant_outcome(&self, task: TaskId, n: u8, outcome: &Outcome) -> Result<(), AppError> {
        let diff = if outcome.diff.is_empty() { None } else { Some(self.store.blobs().put(outcome.diff.as_bytes(), "text/x-diff; charset=utf-8").map_err(storage_error)?) };
        let outcome = outcome.clone();
        self.store
            .write(move |tx| {
                let conversation = store::variants(tx, task)?.into_iter().find(|v| v.n == n).and_then(|v| v.conversation_id);
                let checks = match conversation {
                    Some(c) => checks(tx, c)?,
                    None => Default::default(),
                };
                if let Some(blob) = &diff {
                    blobs::register(tx, blob)?;
                }
                let diff_id = diff.as_ref().map(|b| b.id.to_string());
                store::record_outcome(tx, task, n, (outcome.files, outcome.insertions, outcome.deletions), diff_id.as_deref(), outcome.truncated, None, checks.errors, checks.tests.as_deref())?;
                Ok(())
            })
            .map_err(storage_error)
    }

    /// Everything the judge compares.
    pub fn variants_for_judge(&self, task: TaskId) -> Result<Vec<JudgeItem>, AppError> {
        let st = self.store.clone();
        self.store
            .read(move |conn| {
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                let mut out = Vec::new();
                for v in store::variants(conn, task)? {
                    let tokens = match v.conversation_id {
                        Some(c) => {
                            let u = aurelius_storage::usage::conversation_totals(conn, c)?.own;
                            u.input_tokens + u.output_tokens + u.cached_tokens
                        }
                        None => 0,
                    };
                    out.push(JudgeItem {
                        n: v.n,
                        hint: v.hint,
                        status: v.status,
                        files: v.files,
                        insertions: v.insertions,
                        deletions: v.deletions,
                        diff: v.diff_key.as_deref().and_then(read).unwrap_or_default(),
                        report: v.report_key.as_deref().and_then(read).unwrap_or_default(),
                        errors: v.errors,
                        tests: v.tests,
                        tokens,
                    });
                }
                Ok(out)
            })
            .map_err(storage_error)
    }

    /// The judge ranked the variants. In Auto its pick is taken at once; otherwise
    /// the task waits for the user's choice (Needs you).
    pub fn record_judgement(&self, task: TaskId, epoch: i64, judgement: &Judgement, auto: bool) -> Result<Vec<EventNotice>, AppError> {
        let judgement = judgement.clone();
        self.store
            .write(move |tx| {
                if !aurelius_storage::queries::holds_lease(tx, &task.to_string(), epoch)? || store::phase(tx, task)? != Some(VariantPhase::Judging) {
                    return Ok(vec![]);
                }
                store::set_judgement(tx, task, &judgement.order, &judgement.notes, judgement.compose.as_deref())?;
                let Some(row) = tasks_store::load_task(tx, task)? else { return Ok(vec![]) };
                let count = store::variants(tx, task)?.len();
                let mut notices = vec![task_event(tx, task, row.project_id, "task.variants_ready", serde_json::json!({ "count": count, "judge_pick": judgement.pick(), "auto": auto }))?];
                if auto {
                    store::choose(tx, task, judgement.pick(), "judge")?;
                } else if row.status == TaskStatus::Running {
                    crate::task::task_line::set_status(tx, task, TaskStatus::Waiting, Some("approval"))?;
                    notices.push(task_event(tx, task, row.project_id, "task.waiting", serde_json::json!({ "reason": "variants" }))?);
                }
                Ok(notices)
            })
            .map_err(storage_error)
    }

    /// Move the set to `phase` (fenced by the task job's lease).
    pub fn set_variants_phase(&self, task: TaskId, epoch: i64, phase: VariantPhase, error: Option<String>) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                if !aurelius_storage::queries::holds_lease(tx, &task.to_string(), epoch)? {
                    return Ok(vec![]);
                }
                store::set_phase(tx, task, phase, error.as_deref())?;
                if phase != VariantPhase::Failed {
                    return Ok(vec![]);
                }
                let Some(row) = tasks_store::load_task(tx, task)? else { return Ok(vec![]) };
                Ok(vec![task_event(tx, task, row.project_id, "task.variants_failed", serde_json::json!({ "error": error }))?])
            })
            .map_err(storage_error)
    }

    /// The chosen variant's diff is in the work folder (`Ok`), or could not be
    /// applied (`Err`): the variants are over and the task goes on as usual. The
    /// chosen approach is a signal for the task's lessons (`task.variant_chosen`).
    pub fn finish_variants(&self, task: TaskId, epoch: i64, applied: Result<(), String>) -> Result<Vec<EventNotice>, AppError> {
        self.store
            .write(move |tx| {
                if !aurelius_storage::queries::holds_lease(tx, &task.to_string(), epoch)? {
                    return Ok(vec![]);
                }
                let Some(set) = store::load_set(tx, task)? else { return Ok(vec![]) };
                let Some(row) = tasks_store::load_task(tx, task)? else { return Ok(vec![]) };
                let Some(n) = set.chosen else { return Ok(vec![]) };
                let hint = store::variants(tx, task)?.into_iter().find(|v| v.n == n).map(|v| v.hint).unwrap_or_default();
                match applied {
                    Ok(()) => {
                        store::set_phase(tx, task, VariantPhase::Done, None)?;
                        Ok(vec![task_event(
                            tx,
                            task,
                            row.project_id,
                            "task.variant_chosen",
                            serde_json::json!({ "n": n, "hint": hint, "variants": set.count, "by": set.chosen_by }),
                        )?])
                    }
                    Err(error) => {
                        store::set_phase(tx, task, VariantPhase::Failed, Some(&format!("the diff of trial {n} could not be applied: {error}")))?;
                        Ok(vec![task_event(tx, task, row.project_id, "task.variants_failed", serde_json::json!({ "n": n, "error": error }))?])
                    }
                }
            })
            .map_err(storage_error)
    }

    /// A variant's worktree was removed.
    pub fn variant_worktree_removed(&self, task: TaskId, n: u8) -> Result<(), AppError> {
        self.store.write(move |tx| store::clear_worktree(tx, task, n)).map_err(storage_error)
    }

    /// Worktrees of the task's variants that are no longer needed: (n, path).
    pub fn variant_worktrees_to_remove(&self, task: TaskId) -> Result<(Option<String>, Vec<(u8, String)>), AppError> {
        self.store.read(move |conn| Ok((store::load_set(conn, task)?.and_then(|s| s.repo), store::worktrees_to_remove(conn, task)?))).map_err(storage_error)
    }

    /// The worktree of a run that is a trial of a task (its sandbox); `None` for any other run.
    pub fn probe_worktree(&self, run: RunId) -> Result<Option<String>, AppError> {
        self.store.read(move |conn| Ok(store::probe_of_run(conn, run)?.map(|p| p.worktree.unwrap_or_default()))).map_err(storage_error)
    }

    /// `task.variants`: a task's variants and the judge's comparison.
    pub fn task_variants(&self, task: TaskId) -> Result<TaskVariantsView, AppError> {
        let st = self.store.clone();
        self.store
            .read(move |conn| {
                let read = |key: &str| st.blobs().read(key).ok().and_then(|b| String::from_utf8(b).ok());
                store::view(conn, task, &read)
            })
            .map_err(storage_error)?
            .ok_or_else(|| domain_error(DomainError::OutOfRange { field: "task".into(), detail: "this task has no variants".into() }))
    }

    /// The user picks a variant (or lets the judge's pick stand): its diff goes into
    /// the work folder and the task goes on.
    pub(crate) fn choose_task_variant(&self, envelope: &CommandEnvelope, hash: String, cmd: &ChooseTaskVariant) -> Result<Committed, AppError> {
        let (task, wanted) = (cmd.task_id, cmd.variant);
        self.commit(envelope.command_id, hash, move |tx| {
            let Some(set) = store::load_set(tx, task)? else {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "task".into(), detail: "this task has no variants".into() })));
            };
            if set.phase != VariantPhase::Ready {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "task".into(), detail: format!("its variants are {}, not waiting for a choice", set.phase.as_str()) })));
            }
            let Some(n) = wanted.or(set.judge_pick) else {
                return Ok(Err(domain_error(DomainError::Required { field: "variant".into() })));
            };
            let Some(chosen) = store::variants(tx, task)?.into_iter().find(|v| v.n == n) else {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "variant".into(), detail: format!("1 to {}", set.count) })));
            };
            if chosen.status != VariantStatus::Done {
                return Ok(Err(domain_error(DomainError::OutOfRange { field: "variant".into(), detail: format!("trial {n} did not finish") })));
            }
            let by = if wanted.is_some() { "user" } else { "judge" };
            store::choose(tx, task, n, by)?;
            let Some(row) = tasks_store::load_task(tx, task)? else { return Ok(Err(domain_error(DomainError::TaskNotFound(task)))) };
            let mut notices = Vec::new();
            if row.status == TaskStatus::Waiting {
                crate::task::task_line::set_status(tx, task, TaskStatus::Queued, None)?;
                notices.push(task_event(tx, task, row.project_id, "task.queued", serde_json::json!({ "reason": "variant_chosen", "n": n }))?);
            }
            crate::task::task_runs::wake_task_job(tx, task, false)?;
            Ok(Ok((CommandResult::TaskVariantChosen { task_id: task, variant: n }, notices)))
        })
    }
}

/// What the conversation of a task whose variants are over is told first.
pub(crate) fn variant_note(conn: &Connection, task: TaskId) -> Result<Option<String>, StorageError> {
    let Some(set) = store::load_set(conn, task)? else { return Ok(None) };
    let hint = |n: u8| store::variants(conn, task).ok().and_then(|vs| vs.into_iter().find(|v| v.n == n).map(|v| v.hint));
    Ok(match (set.phase, set.chosen) {
        (VariantPhase::Done, Some(n)) => Some(format!(
            "This task was first tried in {} ways side by side. Trial {n} (approach: {}), chosen by {}, is applied to the work folder as uncommitted changes. Check it, finish the task from there and do what is left (also anything outside the folder).",
            set.count,
            hint(n).unwrap_or_default(),
            if set.chosen_by.as_deref() == Some("judge") { "the judge" } else { "the user" }
        )),
        (VariantPhase::Failed, _) => Some(format!("Trying this task in several ways first did not work out ({}); do the task in the usual way.", set.error.unwrap_or_default())),
        _ => None,
    })
}

fn ctx_project(tx: &Tx<'_>, task: TaskId) -> Result<ProjectId, StorageError> {
    tasks_store::load_task(tx, task)?.map(|r| r.project_id).ok_or_else(|| StorageError::Rejected(format!("task {task} not found")))
}

