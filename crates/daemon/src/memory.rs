//! Memory jobs: cheap extraction after a finished task, and
//! conversation summaries with exact coverage. Both run on the employee's
//! own connection, text-only, and never block the user's work.

use crate::Shared;
use crate::tasks::complete_text;
use aurelius_domain::memory::{ConsolidationOutput, Extraction, MAX_LESSONS, POINT_MAX_CHARS, validate_candidates, validate_consolidation, validate_lessons};
use aurelius_domain::task::extract_json;
use aurelius_domain::{ConversationId, RunId, TaskId};
use markers::{CONSOLIDATE_MARKER, MEMORY_MARKER, SUMMARY_MARKER};
use aurelius_storage::queries;
use std::sync::Arc;

/// The first line of each memory prompt. The fake runtime of the tests answers by it;
/// in production it is only a plain label.
#[cfg(feature = "testing")]
mod markers {
    pub use aurelius_runtime::fake::{CONSOLIDATE_MARKER, MEMORY_MARKER, SUMMARY_MARKER};
}
#[cfg(not(feature = "testing"))]
mod markers {
    pub const MEMORY_MARKER: &str = "Task: record memory.";
    pub const SUMMARY_MARKER: &str = "Task: summarize.";
    pub const CONSOLIDATE_MARKER: &str = "Task: consolidate memory.";
}

async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    tokio::task::spawn_blocking(f).await.ok()
}

async fn close(shared: &Arc<Shared>, job: &queries::JobRow) {
    let store = shared.store().clone();
    let (id, epoch) = (job.id.clone(), job.lease_epoch);
    let _ = blocking(move || store.write(move |tx| queries::complete_job(tx, &id, epoch))).await;
}

/// Retry later with backoff; give up after the job's attempts (memory is optional).
async fn retry_or_drop(shared: &Arc<Shared>, job: &queries::JobRow, error: String) {
    tracing::warn!(job = %job.id, %error, "memory job failed");
    let store = shared.store().clone();
    let (id, epoch) = (job.id.clone(), job.lease_epoch);
    let base = shared.backoff_base_ms;
    let _ = blocking(move || {
        store.write(move |tx| {
            let (attempt, max) = queries::job_attempts(tx, &id)?;
            if attempt >= max {
                queries::kill_job(tx, &id, epoch, "memory_failed")
            } else {
                let delay = aurelius_domain::scheduler::backoff_ms(base, attempt, &id);
                queries::reschedule_job(tx, &id, epoch, aurelius_storage::now_ms() + delay, "memory_retry", true)
            }
        })
    })
    .await;
}

pub async fn run_extraction(shared: Arc<Shared>, job: queries::JobRow) {
    let Some(task) = job.payload["task_id"].as_str().and_then(|t| TaskId::parse(t).ok()) else {
        close(&shared, &job).await;
        return;
    };
    let s = shared.clone();
    let input = match blocking(move || s.service.extraction_input(task)).await {
        Some(Ok(Some(input))) => input,
        Some(Ok(None)) => {
            close(&shared, &job).await;
            return;
        }
        Some(Err(e)) => return retry_or_drop(&shared, &job, e.to_string()).await,
        None => return,
    };
    let sources = input.sources.iter().map(|(label, _, what)| format!("{label}: {what}")).collect::<Vec<_>>().join("\n");
    let existing = if input.existing.is_empty() {
        "(none)".to_string()
    } else {
        input.existing.iter().take(40).map(|e| format!("- [{}] {}", e.kind.label(), e.content)).collect::<Vec<_>>().join("\n")
    };
    // Lessons only when the gate found signals (what went wrong or was corrected);
    // otherwise the prompt is as it always was.
    let lessons_rule = if input.signals.is_empty() {
        String::new()
    } else {
        format!(
            " Signals S1..Sn say what went wrong or was corrected in this task. From them, give at most {MAX_LESSONS} lessons in \"lessons\": \
             a concrete rule for a later task, when it applies (\"applies_when\") and the signals it rests on (\"evidence\"). \
             Only a lesson a signal shows; none for chance events or when nothing general can be learned. \
             Scope employee for the employee's own way of working, project for something every colleague should know. \
             A lesson that is already in existing memory may be given again: it then counts as confirmed."
        )
    };
    let lessons_json = if input.signals.is_empty() { String::new() } else { ",\"lessons\":[{\"applies_when\":\"...\",\"lesson\":\"one sentence\",\"evidence\":[\"S1\"],\"scope\":\"employee|project\"}]".to_string() };
    let system = format!(
        "{}\n{MEMORY_MARKER}\nYou record what is worth remembering from a finished task. Only facts from the sources; \
         no secrets or personal data; no repetition of existing memory.{lessons_rule} \
         Answer only with JSON: {{\"items\":[{{\"kind\":\"episodic|project_knowledge|decision\",\"content\":\"one sentence\",\"source\":\"E1\",\"scope\":\"employee|project\",\"importance\":0.5,\"confidence\":0.7}}]{lessons_json}}}. \
         Empty list if there is nothing.",
        input.employee_system
    );
    let mut prompt = format!("Sources:\n{sources}\n\nExisting memory:\n{existing}");
    if !input.signals.is_empty() {
        let signals = input.signals.iter().map(|(label, _, what)| format!("{label}: {what}")).collect::<Vec<_>>().join("\n");
        prompt.push_str(&format!("\n\nSignals:\n{signals}"));
    }
    let output = match complete_text(&shared, &input.config, RunId::new(), system, prompt).await {
        Ok((text, _)) => text,
        Err(e) => return retry_or_drop(&shared, &job, e.to_string()).await,
    };
    let extraction: Extraction = extract_json(&output).unwrap_or_default();
    let labels: Vec<&str> = input.sources.iter().map(|s| s.0.as_str()).collect();
    let verdicts = validate_candidates(&extraction, &labels, &input.existing);
    let signal_labels: Vec<&str> = input.signals.iter().map(|s| s.0.as_str()).collect();
    let lessons = validate_lessons(&extraction.lessons, &signal_labels, &input.lessons);
    let (s, id, epoch) = (shared.clone(), job.id.clone(), job.lease_epoch);
    if let Some(Ok(notices)) = blocking(move || s.service.record_extraction(&id, epoch, &input, verdicts, lessons)).await {
        shared.publish(notices);
    }
}

pub async fn run_summary(shared: Arc<Shared>, job: queries::JobRow) {
    let Some(conversation) = job.payload["conversation_id"].as_str().and_then(|t| ConversationId::parse(t).ok()) else {
        close(&shared, &job).await;
        return;
    };
    let s = shared.clone();
    let input = match blocking(move || s.service.summary_input(conversation)).await {
        Some(Ok(Some(input))) if !input.turns.is_empty() => input,
        Some(Ok(_)) => {
            close(&shared, &job).await;
            return;
        }
        Some(Err(e)) => return retry_or_drop(&shared, &job, e.to_string()).await,
        None => return,
    };
    let system = format!(
        "{SUMMARY_MARKER}\nSummarize this conversation concisely for later use: decisions, open questions, agreements and relevant facts. \
         Only what is in the text; no secrets. At most 12 lines."
    );
    let mut prompt = String::new();
    if let Some(previous) = &input.previous {
        prompt.push_str(&format!("Earlier summary:\n{previous}\n\n"));
    }
    prompt.push_str("New messages:\n");
    for (ordinal, who, text, _) in &input.turns {
        prompt.push_str(&format!("[{ordinal}] {who}: {text}\n"));
    }
    let text = match complete_text(&shared, &input.config, RunId::new(), system, prompt).await {
        Ok((text, _)) if !text.trim().is_empty() => text,
        Ok(_) => return retry_or_drop(&shared, &job, "empty summary".into()).await,
        Err(e) => return retry_or_drop(&shared, &job, e.to_string()).await,
    };
    let (s, id, epoch) = (shared.clone(), job.id.clone(), job.lease_epoch);
    if let Some(Ok(notices)) = blocking(move || s.service.record_summary(&id, epoch, &input, &text)).await {
        shared.publish(notices);
    }
}

/// Merge a scope's memories into short points.
/// The model only proposes; `validate_consolidation` decides what is applied.
pub async fn run_consolidation(shared: Arc<Shared>, job: queries::JobRow) {
    let Some(scope) = job.payload["scope_id"].as_str().map(str::to_owned) else {
        close(&shared, &job).await;
        return;
    };
    let s = shared.clone();
    let input = match blocking(move || s.service.consolidation_input(&scope)).await {
        Some(Ok(Some(input))) => input,
        Some(Ok(None)) => {
            close(&shared, &job).await;
            return;
        }
        Some(Err(e)) => return retry_or_drop(&shared, &job, e.to_string()).await,
        None => return,
    };
    let system = format!(
        "{CONSOLIDATE_MARKER}\nYou keep a team's memory short. Below are memories M1..Mn (decisions, project knowledge, episodes). \
         Merge them into a short list of points: one sentence per decision or fact, at most {POINT_MAX_CHARS} characters, \
         without implementation details such as file names, functions or tests. Merge duplicates and keep only the newest \
         version of a decision that was replaced. Every point lists in \"sources\" the labels of all memories it replaces. \
         Put outdated memories that no point replaces in \"dropped\". Leave out memories that are fine as they are: they stay. \
         Only what is in the memories; no secrets. Write each point in the language of its memories. \
         Answer only with JSON: {{\"points\":[{{\"kind\":\"decision|project_knowledge|episodic\",\"content\":\"...\",\"sources\":[\"M1\",\"M4\"],\"importance\":0.6}}],\"dropped\":[\"M7\"]}}."
    );
    let prompt = input.items.iter().map(|m| format!("{} [{}] {}", m.label, m.kind.label(), m.content.trim().replace('\n', " "))).collect::<Vec<_>>().join("\n");
    let output = match complete_text(&shared, &input.config, RunId::new(), system, prompt).await {
        Ok((text, _)) => text,
        Err(e) => return retry_or_drop(&shared, &job, e.to_string()).await,
    };
    let Some(parsed) = extract_json::<ConsolidationOutput>(&output) else {
        return retry_or_drop(&shared, &job, "consolidation was not valid JSON".into()).await;
    };
    let plan = validate_consolidation(&parsed, &input.items);
    tracing::info!(job = %job.id, items = input.items.len(), points = plan.points.len(), dropped = plan.dropped.len(), review = plan.needs_review, "memory consolidated");
    let (s, id, epoch) = (shared.clone(), job.id.clone(), job.lease_epoch);
    if let Some(Ok(notices)) = blocking(move || s.service.record_consolidation(&id, epoch, &input, &plan)).await {
        shared.publish(notices);
    }
}
