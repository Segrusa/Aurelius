//! The guidelines of a chat run (B3, impactanalyse §3, SEC0 §1): which set it
//! gets, the block in its system text, its items in the context manifest, the
//! version set its session keeps (B6) and its snapshot in `run_guidelines`.
//!
//! An employee's run gets its own guidelines and those of its project: the
//! task's, or the channel's or discussion's that woke it; its own chat without
//! a project only its own. The project assistant gets the project's only.
//! Triage, memory extraction and the ticket summary never come here.

use aurelius_domain::guideline::{self as gl, EffectiveGuideline};
use aurelius_domain::{ConversationId, EmployeeId, EventNotice, ProjectId, RunId};
use aurelius_domain::views::{SystemLine, SystemLineKind};
use aurelius_storage::{StorageError, Tx as Transaction, guidelines as store, settings};

/// What a run has of the guidelines, kept with its plan until it starts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunSet {
    /// As in its system text (a title or text the redactor changed, changed), in block order.
    pub guidelines: Vec<EffectiveGuideline>,
    /// What the safety net left out (B4), in the order it went.
    pub left_out: Vec<EffectiveGuideline>,
    /// The guidelines the redactor changed (E6): title, and the vault items' names.
    pub redacted: Vec<(String, Vec<String>)>,
}

impl RunSet {
    /// The line the board members get when the block left something out or
    /// redacted a vault value; never the value. None when there is nothing to say.
    pub fn notice(&self) -> Option<String> {
        let mut parts = Vec::new();
        if !self.left_out.is_empty() {
            let titles: Vec<String> = self.left_out.iter().map(|g| format!("“{}”", g.title)).collect();
            parts.push(format!(
                "Guidelines left out of this run because together they are over {} characters: {}. Shorten or unlink a guideline.",
                gl::MAX_RUN_CHARS,
                titles.join(", ")
            ));
        }
        for (title, names) in &self.redacted {
            parts.push(format!("The guideline “{title}” holds the value of a vault item ({}); this run got it redacted. Change the guideline.", names.join(", ")));
        }
        (!parts.is_empty()).then(|| parts.join(" "))
    }
}

/// The guidelines of a run as `prepare_chat_run` uses them.
#[derive(Debug, Clone, Default)]
pub struct ForRun {
    /// The `## Guidelines` block, empty without guidelines.
    pub block: String,
    /// One manifest item per guideline (section `guideline`), required.
    pub items: Vec<crate::memory::context::GuidelineItem>,
    /// `<id>@<version>` per guideline in the block; `#redacted` behind one the redactor changed (B6).
    pub version_set: Vec<String>,
    pub run: RunSet,
}

/// The employee and project whose guidelines a run gets; none for a run without either.
pub fn set_of(employee_row: Option<(EmployeeId, Option<ProjectId>)>, project_conversation: Option<ProjectId>, channel_project: Option<ProjectId>) -> Option<(Option<EmployeeId>, Option<ProjectId>)> {
    match (project_conversation, employee_row) {
        // The project assistant: the project's only.
        (Some(project), _) => Some((None, Some(project))),
        // An employee: its task's project, or the channel's or discussion's that woke it.
        (None, Some((employee, project))) => Some((Some(employee), project.or(channel_project))),
        (None, None) => None,
    }
}

/// The run's set after the safety net (B4) and the redactor (E6), its block and its manifest items.
pub fn for_run(
    conn: &rusqlite::Connection,
    employee_row: Option<(EmployeeId, Option<ProjectId>)>,
    project_conversation: Option<ProjectId>,
    channel_project: Option<ProjectId>,
) -> Result<ForRun, StorageError> {
    let Some((employee, project)) = set_of(employee_row, project_conversation, channel_project) else { return Ok(ForRun::default()) };
    let (mut set, left_out) = gl::safety_net(store::effective(conn, employee, project)?);
    let redactor = aurelius_runtime::redact::current();
    let mut redacted = Vec::new();
    let mut flags = Vec::new();
    for g in &mut set.guidelines {
        let mut names = redactor.names_in(&g.title);
        for name in redactor.names_in(&g.text) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        flags.push(!names.is_empty());
        if !names.is_empty() {
            g.title = redactor.redact(&g.title).into_owned();
            g.text = redactor.redact(&g.text).into_owned();
            redacted.push((g.title.clone(), names));
        }
    }
    let project_name = project.and_then(|p| crate::queries_project_name(conn, p));
    let titles: Vec<String> = left_out.iter().map(|g| redactor.redact(&g.title).into_owned()).collect();
    let block = set.render(project_name.as_deref(), &titles);
    let mut items: Vec<crate::memory::context::GuidelineItem> = set
        .guidelines
        .iter()
        .zip(&flags)
        .map(|(g, redacted)| crate::memory::context::GuidelineItem {
            id: format!("g:{}@{}", g.id, g.version),
            label: g.title.clone(),
            text: g.text.clone(),
            included: true,
            reason: if *redacted { "redacted".into() } else { "required".into() },
        })
        .collect();
    items.extend(left_out.iter().zip(&titles).map(|(g, title)| crate::memory::context::GuidelineItem {
        id: format!("g:{}@{}", g.id, g.version),
        label: title.clone(),
        text: redactor.redact(&g.text).into_owned(),
        included: false,
        reason: "over the guideline limit".into(),
    }));
    let version_set = set.version_set().into_iter().zip(&flags).map(|(v, redacted)| if *redacted { format!("{v}#redacted") } else { v }).collect();
    let left_out = left_out.into_iter().zip(titles).map(|(g, title)| EffectiveGuideline { title, ..g }).collect();
    Ok(ForRun { block, items, version_set, run: RunSet { guidelines: set.guidelines, left_out, redacted } })
}

/// The settings key of the last notice a conversation got (only a change is posted again).
fn notice_key(conversation: ConversationId) -> String {
    format!("guideline_notice:{conversation}")
}

/// At the run's start, in its transaction: the snapshot of what its system
/// text has (B6), and the board members' notice in its conversation when the
/// block left something out or redacted (once until it changes).
pub(crate) fn record(tx: &Transaction<'_>, run: RunId, conversation: ConversationId, set: &RunSet) -> Result<Vec<EventNotice>, StorageError> {
    store::insert_for_run(tx, run, &set.guidelines)?;
    let notice = set.notice();
    let key = notice_key(conversation);
    let now = serde_json::to_string(&notice)?;
    if settings::get(tx, &key)?.as_deref() == Some(now.as_str()) || (notice.is_none() && settings::get(tx, &key)?.is_none()) {
        return Ok(Vec::new());
    }
    settings::put(tx, &key, &now, aurelius_storage::now_ms())?;
    match notice {
        Some(text) => Ok(vec![crate::conversation::discussion::system_line(tx, conversation, &SystemLine::new(SystemLineKind::Limit), Some(&text))?]),
        None => Ok(Vec::new()),
    }
}
