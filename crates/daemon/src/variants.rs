//! Task variants (Harness² C): the task job's steps before the real work.
//! Propose (the cheap model gives k approach hints), probe (each variant runs in
//! its own git worktree, one after the other: the end of one run wakes the task
//! job for the next), judge (compares diffs, errors, tests, reports and tokens),
//! then compose: the user's choice, or in Auto the judge's, is applied to the
//! work folder and the other worktrees go at once. Each pass derives what to do
//! from durable state, so a restart goes on where it was.

use crate::Shared;
use crate::tasks::complete_text;
use aurelius_application::task::tasks::TaskContext;
use aurelius_application::task::variants::{JudgeItem, Outcome, VariantsState};
use aurelius_domain::variants::{HintsOutput, JudgeOutput, VariantPhase, VariantStatus, pick_hints, validate_judgement};
use aurelius_domain::{RunId, TaskId, task::extract_json};
use markers::{JUDGE_MARKER, VARIANTS_MARKER};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[cfg(feature = "testing")]
mod markers {
    pub use aurelius_runtime::fake::{JUDGE_MARKER, VARIANTS_MARKER};
}
#[cfg(not(feature = "testing"))]
mod markers {
    pub const VARIANTS_MARKER: &str = "Task: propose approaches.";
    pub const JUDGE_MARKER: &str = "Task: compare trials.";
}

/// A trial's diff is kept (for the comparison card and the judge) up to this size.
const DIFF_KEEP_BYTES: usize = 200_000;
/// What the judge sees of one trial's diff and report.
const JUDGE_DIFF_CHARS: usize = 12_000;
const JUDGE_REPORT_CHARS: usize = 3_000;

/// What the task job does after the variants' step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Next {
    /// No variants: the task goes on as usual.
    GoOn,
    /// The variants are over: the task goes on as usual, told what came of them.
    After,
    /// A variant runs, or the user chooses: the job ends; it is woken again.
    Wait,
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

async fn state(shared: &Arc<Shared>, ctx: &TaskContext, epoch: i64) -> Option<VariantsState> {
    let (s, c) = (shared.clone(), ctx.clone());
    match blocking(move || s.service.variants_state(&c, epoch)).await {
        Some(Ok(mut state)) => {
            if let Some(state) = state.as_mut() {
                shared.publish(std::mem::take(&mut state.notices));
            }
            state
        }
        Some(Err(e)) => {
            tracing::warn!(task = %ctx.task_id, error = %e.message, "could not read the task's variants");
            None
        }
        None => None,
    }
}

async fn fail(shared: &Arc<Shared>, task: TaskId, epoch: i64, why: String) {
    tracing::warn!(%task, %why, "task variants failed; the task goes on as usual");
    let s = shared.clone();
    if let Some(Ok(n)) = blocking(move || s.service.set_variants_phase(task, epoch, VariantPhase::Failed, Some(why))).await {
        shared.publish(n);
    }
}

/// One pass of the variants of the task the job holds.
pub(crate) async fn step(shared: &Arc<Shared>, ctx: &TaskContext, epoch: i64) -> Next {
    let task = ctx.task_id;
    for _ in 0..8 {
        let Some(state) = state(shared, ctx, epoch).await else { return Next::GoOn };
        let repo = state.set.repo.clone().map(PathBuf::from);
        match state.set.phase {
            VariantPhase::Done | VariantPhase::Failed | VariantPhase::Stopped => {
                remove_unneeded(shared, task).await;
                return Next::After;
            }
            VariantPhase::Ready => return Next::Wait,
            VariantPhase::Proposing => {
                let Some(repo) = repo else { return Next::After };
                if let Err(why) = propose(shared, ctx, epoch, &state, &repo).await {
                    fail(shared, task, epoch, why).await;
                }
            }
            VariantPhase::Probing => {
                let Some(repo) = repo else { return Next::After };
                // Collect what finished trials left.
                for v in state.variants.iter().filter(|v| !v.collected && matches!(v.status, VariantStatus::Done | VariantStatus::Failed | VariantStatus::Cancelled)) {
                    collect(shared, task, v.n, v.worktree.as_deref()).await;
                }
                if state.variants.iter().any(|v| v.status == VariantStatus::Running) {
                    return Next::Wait;
                }
                match state.variants.iter().find(|v| v.status == VariantStatus::Waiting) {
                    Some(next) => {
                        let base = state.set.base_commit.clone().unwrap_or_default();
                        let path = aurelius_tools::worktree::probe_path(&repo, &task.to_string(), next.n);
                        let (r, p) = (repo.clone(), path.clone());
                        match blocking(move || aurelius_tools::worktree::add(&r, &p, &base)).await {
                            Some(Ok(())) => {}
                            Some(Err(e)) => {
                                fail(shared, task, epoch, format!("could not make the worktree of trial {}: {e}", next.n)).await;
                                continue;
                            }
                            None => return Next::Wait,
                        }
                        let (s, c, n, count, wt) = (shared.clone(), ctx.clone(), next.n, state.set.count, path.display().to_string());
                        match blocking(move || s.service.start_variant(&c, epoch, n, count, &wt)).await {
                            Some(Ok(notices)) => shared.publish(notices),
                            other => tracing::warn!(%task, ?other, "could not start a trial"),
                        }
                        return Next::Wait;
                    }
                    None => {
                        let s = shared.clone();
                        if let Some(Ok(n)) = blocking(move || s.service.set_variants_phase(task, epoch, VariantPhase::Judging, None)).await {
                            shared.publish(n);
                        }
                    }
                }
            }
            VariantPhase::Judging => judge(shared, task, epoch, state.auto).await,
            VariantPhase::Applying => {
                let Some(repo) = repo else { return Next::After };
                // The others are not needed any more: they go at once.
                remove_unneeded(shared, task).await;
                let chosen = state.set.chosen.and_then(|n| state.variants.iter().find(|v| v.n == n)).and_then(|v| v.worktree.clone());
                let applied = match chosen {
                    Some(worktree) => {
                        let (r, wt) = (repo.clone(), PathBuf::from(worktree));
                        blocking(move || aurelius_tools::worktree::change(&wt).and_then(|(patch, _)| aurelius_tools::worktree::apply(&r, &patch)))
                            .await
                            .unwrap_or_else(|| Err(aurelius_tools::git::GitError("stopped".into())))
                            .map_err(|e| e.0)
                    }
                    None => Err("the chosen trial's worktree is gone".to_string()),
                };
                let s = shared.clone();
                if let Some(Ok(n)) = blocking(move || s.service.finish_variants(task, epoch, applied)).await {
                    shared.publish(n);
                }
            }
        }
    }
    Next::Wait
}

/// Propose: k short approaches from the task and the lessons learned.
async fn propose(shared: &Arc<Shared>, ctx: &TaskContext, epoch: i64, state: &VariantsState, repo: &Path) -> Result<(), String> {
    let r = repo.to_path_buf();
    let base = blocking(move || aurelius_tools::git::head(&r)).await.ok_or("stopped")?.map_err(|e| format!("the work folder has no commit to start from ({e})"))?;
    let (s, c, count) = (shared.clone(), ctx.clone(), state.set.count);
    let input = match blocking(move || s.service.variants_hints_input(&c, count)).await {
        Some(Ok(Some(input))) => input,
        other => return Err(format!("no employee to think of approaches ({other:?})")),
    };
    let system = format!(
        "{VARIANTS_MARKER}\nA task will be tried in {count} different ways side by side, each in its own copy of the work folder, before the best is kept. \
         Give {count} short, clearly different approaches (one sentence each, at most 160 characters), e.g. plan first, tests first, smallest change. \
         Use the lessons when they apply. Answer only with JSON: {{\"hints\":[\"...\"]}}."
    );
    let lessons = if input.lessons.is_empty() { "(none)".to_string() } else { input.lessons.iter().map(|l| format!("- {l}")).collect::<Vec<_>>().join("\n") };
    let prompt = format!("Count: {count}\nTask: {}\nGoal: {}\n\nLessons:\n{lessons}", input.title, input.goal.trim());
    // A failed call is no reason to stop: the default approaches are fine.
    let output = complete_text(shared, &input.config, RunId::new(), system, prompt).await.map(|(t, _)| t).unwrap_or_default();
    let hints = pick_hints(&extract_json::<HintsOutput>(&output).unwrap_or_default(), count);
    let s = shared.clone();
    let task = ctx.task_id;
    match blocking(move || s.service.record_variant_hints(task, epoch, &base, &hints)).await {
        Some(Ok(n)) => shared.publish(n),
        other => return Err(format!("could not keep the approaches ({other:?})")),
    }
    Ok(())
}

/// Keep what a finished trial left in its worktree.
async fn collect(shared: &Arc<Shared>, task: TaskId, n: u8, worktree: Option<&str>) {
    let outcome = match worktree.map(PathBuf::from).filter(|p| p.exists()) {
        Some(path) => match blocking(move || aurelius_tools::worktree::change(&path)).await {
            Some(Ok((mut diff, (files, insertions, deletions)))) => {
                let truncated = diff.len() > DIFF_KEEP_BYTES;
                if truncated {
                    let mut cut = DIFF_KEEP_BYTES;
                    while !diff.is_char_boundary(cut) {
                        cut -= 1;
                    }
                    diff.truncate(cut);
                }
                Outcome { files, insertions, deletions, diff, truncated }
            }
            other => {
                tracing::warn!(%task, n, ?other, "could not read a trial's change");
                Outcome::default()
            }
        },
        None => Outcome::default(),
    };
    let s = shared.clone();
    if let Some(Err(e)) = blocking(move || s.service.record_variant_outcome(task, n, &outcome)).await {
        tracing::warn!(%task, n, error = %e.message, "could not keep a trial's outcome");
    }
}

fn cut(text: &str, max: usize) -> String {
    if text.chars().count() <= max { text.to_owned() } else { format!("{}\n… (cut)", text.chars().take(max).collect::<String>()) }
}

/// The judge's prompt: every trial with what it did.
fn judge_prompt(items: &[JudgeItem]) -> String {
    items
        .iter()
        .map(|v| {
            format!(
                "## Variant {}\nApproach: {}\nStatus: {}\nChange: {} files, +{} -{}\nFailed tool calls: {}\nTokens: {}\nTests or build:\n{}\nReport:\n{}\nDiff:\n{}\n",
                v.n,
                v.hint,
                v.status.as_str(),
                v.files,
                v.insertions,
                v.deletions,
                v.errors,
                v.tokens,
                v.tests.as_deref().unwrap_or("(none run)"),
                cut(v.report.trim(), JUDGE_REPORT_CHARS),
                cut(&v.diff, JUDGE_DIFF_CHARS)
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Judge: rank the finished trials without ground truth; in Auto its pick is taken.
async fn judge(shared: &Arc<Shared>, task: TaskId, epoch: i64, auto: bool) {
    let s = shared.clone();
    let items = match blocking(move || s.service.variants_for_judge(task)).await {
        Some(Ok(items)) => items,
        _ => return,
    };
    let candidates: Vec<(u8, bool)> = items.iter().filter(|v| v.status == VariantStatus::Done).map(|v| (v.n, v.files > 0)).collect();
    if candidates.is_empty() {
        return fail(shared, task, epoch, "no trial finished".into()).await;
    }
    let s = shared.clone();
    let (s2, t) = (shared.clone(), task);
    let ctx = blocking(move || s2.service.task_context(t)).await.and_then(Result::ok).flatten();
    let config = match ctx.and_then(|c| c.employee_id) {
        Some(m) => blocking(move || s.service.resolve_for_employee(m, aurelius_domain::connection::Purpose::Memory)).await.and_then(Result::ok),
        None => None,
    };
    let system = format!(
        "{JUDGE_MARKER}\nYou compare trials of the same task, each done in its own way, without a known right answer. \
         Weigh: does the change do the task, is it small and clean, did tests or the build pass, how many tool calls failed, \
         what the report says is left, and the tokens used. Rank only the variants that finished. \
         Answer only with JSON: {{\"ranking\":[{{\"variant\":1,\"why\":\"one sentence\"}}],\"compose\":\"which parts of which variants belong together, or null\"}}, best first."
    );
    let output = match config {
        Some(config) => complete_text(shared, &config, RunId::new(), system, judge_prompt(&items)).await.map(|(t, _)| t).unwrap_or_default(),
        None => String::new(),
    };
    let parsed = extract_json::<JudgeOutput>(&output).unwrap_or_default();
    let Some(judgement) = validate_judgement(&parsed, &candidates) else { return };
    let s = shared.clone();
    match blocking(move || s.service.record_judgement(task, epoch, &judgement, auto)).await {
        Some(Ok(n)) => shared.publish(n),
        other => tracing::warn!(%task, ?other, "could not keep the judgement"),
    }
}

/// Remove the worktrees of a task's trials that are no longer needed (the
/// variants are over, or another trial was chosen). Errors are logged; a
/// worktree that could not go is tried again on the next pass.
pub(crate) async fn remove_unneeded(shared: &Arc<Shared>, task: TaskId) {
    let s = shared.clone();
    let Some(Ok((Some(repo), worktrees))) = blocking(move || s.service.variant_worktrees_to_remove(task)).await else { return };
    for (n, path) in worktrees {
        let (r, p) = (PathBuf::from(&repo), PathBuf::from(&path));
        match blocking(move || aurelius_tools::worktree::remove(&r, &p)).await {
            Some(Ok(())) => {
                let s = shared.clone();
                let _ = blocking(move || s.service.variant_worktree_removed(task, n)).await;
            }
            other => tracing::warn!(%task, n, %path, ?other, "could not remove a trial's worktree"),
        }
    }
}
