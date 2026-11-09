//! Hops and handoffs (R4, R5): what an employee writes wakes nobody, `hand_off` does;
//! the hop goes on through handoffs and delegations, and a board member resets it; a
//! handoff from someone with unread items waits until they have caught up.

use super::*;
use crate::run::inbox::tests_support::use_real_mailbox;
use aurelius_domain::group_chat::TICKET_MAX_HOPS;
use aurelius_domain::views::system_action;

/// What the discussion's messages read, in order.
fn texts(s: &Setup, d: ConversationId) -> Vec<String> {
    messages(s, d).into_iter().map(|m| m.text).collect()
}

fn inbox_count(s: &Setup, state: &'static str) -> i64 {
    s.service.store().read(move |c| Ok(c.query_row("SELECT COUNT(*) FROM employee_inbox WHERE state = ?1", [state], |r| r.get(0))?)).unwrap()
}

fn claim(s: &Setup) -> Option<RunId> {
    s.service.claim_next_work("t", 45_000, 8).unwrap().map(|(job, _)| RunId::parse(&job.id).unwrap())
}

fn finish(s: &Setup, run: RunId) {
    s.service
        .store()
        .write(move |tx| {
            aurelius_storage::queries::complete_job(tx, &run.to_string(), 1)?;
            aurelius_storage::conversations::set_run_status(tx, run, "succeeded", None)
        })
        .unwrap();
}


#[test]
fn a_name_in_what_an_employee_writes_wakes_nobody_and_a_board_members_does() {
    let s = setup();
    let general = general(&s);
    let (to, runs) = send(&s, None, "@Ann what do you think?").unwrap();
    assert_eq!(to, vec![s.ann], "a board member's @Name wakes");
    let before = woken(&s, s.bo).len();
    let (ok, text) = post_in_channel(&s, runs[0], None, "I would ask @Bo about that.");
    assert!(ok && text.contains("a name only refers to Bo") && text.contains("hand_off"), "{text}");
    assert_eq!(woken(&s, s.bo).len(), before, "a post wakes nobody");
    // The post is shown, as any other message.
    assert!(texts(&s, general).iter().any(|t| t == "I would ask @Bo about that."));
}

#[test]
fn hand_off_wakes_the_colleague_and_reads_as_an_ordinary_line() {
    let s = setup();
    let general = general(&s);
    let (_, runs) = send(&s, None, "@Ann hi").unwrap();
    let (ok, text) = hand_off(&s, runs[0], "@bo", "Review PR 12 on branch fix/x, and report what you find.");
    assert!(ok && text.contains("Handed over to Bo") && text.contains("Bo will answer"), "{text}");
    let posted = messages(&s, general).into_iter().rev().find(|m| m.text.starts_with("Ann handed over to Bo")).expect("a line in the channel");
    assert_eq!(posted.text, "Ann handed over to Bo\n\nReview PR 12 on branch fix/x, and report what you find.");
    assert_eq!(posted.author.as_deref(), Some("Ann"));
    let (run, conversation, hop) = last_woken(&s, s.bo);
    assert_eq!((conversation, hop), (general, 1));
    // The handoff reaches Bo unchanged, quoted, and stands on its own.
    let update = wake_text(&s, s.bo);
    assert!(update.starts_with(r#"Ann handed this over to you (JSON-quoted): "Review PR 12 on branch fix/x, and report what you find.""#), "{update}");
    assert_eq!(aurelius_storage_hop(&s, run), 1);
    // Refusals that leave nothing behind.
    assert!(!hand_off(&s, runs[0], "Nobody", "x").0);
    assert!(!hand_off(&s, runs[0], "Ann", "x").0, "not to yourself");
    assert!(!hand_off(&s, runs[0], "Bo", "   ").0, "a handoff says something");
}

fn aurelius_storage_hop(s: &Setup, run: RunId) -> u32 {
    s.service.store().read(move |c| aurelius_storage::inbox::run_hop(c, run)).unwrap()
}

#[test]
fn a_ticket_thread_allows_more_hops_than_a_channel() {
    let s = setup();
    let t = new_ticket(&s, "Chain", None);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let (_, runs) = post(&s, t, "@Ann start").unwrap();
    let mut run = runs[0];
    let names = [("Bo", s.bo), ("Ann", s.ann)];
    for hop in 1..=TICKET_MAX_HOPS {
        let (name, who) = names[(hop as usize + 1) % 2];
        let (ok, text) = hand_off(&s, run, name, "Your turn.");
        assert!(ok, "hop {hop}: {text}");
        let (next, conversation, at) = last_woken(&s, who);
        assert_eq!((conversation, at), (d, hop));
        run = next;
    }
    // Ann got hop 8 and cannot pass it on.
    let (ok, text) = hand_off(&s, run, "Bo", "Once more.");
    assert!(!ok && text.contains("Limit reached") && text.contains("board member"), "{text}");
    // A board member speaks: the count starts over for what the chain does next.
    let (_, runs) = post(&s, t, "@Cy go on, this is fine").unwrap();
    assert!(hand_off(&s, runs[0], "Atlas", "Finish it.").0);
    assert_eq!(last_woken(&s, s.ceo).2, 1);
    // Also when the board member spoke while the run was going, naming someone else.
    let run = last_woken(&s, s.ceo).0;
    post(&s, t, "@Bo just so you know").unwrap();
    assert!(hand_off(&s, run, "Cy", "Back to you.").0);
    assert_eq!(last_woken(&s, s.cy).2, 1, "reset");
}

#[test]
fn editing_or_deleting_a_board_members_message_is_no_hop_and_no_reset() {
    let s = setup();
    let t = new_ticket(&s, "Edit", None);
    let (_, runs) = post(&s, t, "@Ann start").unwrap();
    assert!(hand_off(&s, runs[0], "Bo", "Over to you.").0);
    let bo = last_woken(&s, s.bo).0;
    assert_eq!(last_woken(&s, s.bo).2, 1);
    assert!(hand_off(&s, bo, "Ann", "Back to you.").0);
    let ann = last_woken(&s, s.ann).0;
    assert_eq!(last_woken(&s, s.ann).2, 2);
    // A board member's edit and delete of the message that started it change nothing.
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let first = messages(&s, d).into_iter().find(|m| m.text == "@Ann start").unwrap().id;
    let before = (woken(&s, s.ann).len(), woken(&s, s.bo).len());
    exec(&s, Command::EditMessage(aurelius_domain::group_chat::EditMessage { message_id: first, text: "@Ann start, and @Bo".into(), confirm_secret: false })).unwrap();
    exec(&s, Command::DeleteMessage(aurelius_domain::group_chat::DeleteMessage { message_id: first })).unwrap();
    assert_eq!((woken(&s, s.ann).len(), woken(&s, s.bo).len()), before, "they wake nobody");
    assert!(hand_off(&s, ann, "Bo", "And back.").0);
    assert_eq!(last_woken(&s, s.bo).2, 3, "no reset, no extra hop");
}

#[test]
fn a_handoff_is_held_while_the_sender_has_unread_items_and_delivered_unchanged() {
    let s = setup();
    use_real_mailbox(s.ann);
    use_real_mailbox(s.bo);
    let t = new_ticket(&s, "Held", None);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let (_, runs) = post(&s, t, "@Ann look at the login").unwrap();
    let ann_run = claim(&s).expect("Ann starts");
    assert_eq!(ann_run, runs[0]);
    // A board member's next message waits behind the run Ann is on.
    post(&s, t, "@Ann also the logout").unwrap();
    assert_eq!(inbox_count(&s, "queued"), 1);

    let (ok, text) = hand_off(&s, ann_run, "Bo", "Review the login change on branch x.");
    assert!(ok && text.contains("unread") && text.contains("Bo gets"), "{text}");
    // Held: the discussion says so, Bo has nothing to run yet.
    assert_eq!(inbox_count(&s, "held"), 1);
    assert!(woken(&s, s.bo).is_empty());
    let line = messages(&s, d).into_iter().rev().find_map(|m| m.system.clone().map(|l| (l, m.text))).unwrap();
    assert_eq!((line.0.kind, line.0.action.as_deref()), (SystemLineKind::Limit, Some(system_action::HANDOFF_HELD)));
    assert_eq!(line.1, "Handoff held: Bo gets this from Ann once Ann has read the new messages in this thread.");
    // A held handoff counts for the run's limit.
    assert!(hand_off(&s, ann_run, "Bo", "Second.").0);
    let (ok, text) = hand_off(&s, ann_run, "Bo", "Third.");
    assert!(!ok && text.contains("in this run"), "{text}");
    assert_eq!(inbox_count(&s, "held"), 2);

    // Not released while Ann's run is still going, nor while the item is unread.
    assert!(s.service.release_handoffs().unwrap().is_empty());
    assert_eq!(inbox_count(&s, "held"), 2);
    finish(&s, ann_run);
    assert!(s.service.release_handoffs().unwrap().is_empty(), "the unread item is still waiting");
    let caught_up = claim(&s).expect("Ann reads what waited");
    assert!(s.service.release_handoffs().unwrap().is_empty(), "the run that reads it still goes");
    finish(&s, caught_up);

    // Caught up: both go out, in order, unchanged, each once, at their own hop.
    let released = s.service.release_handoffs().unwrap();
    assert!(!released.is_empty());
    assert_eq!(inbox_count(&s, "held"), 0);
    let texts = texts(&s, d);
    assert!(texts.iter().any(|t| t == "Ann handed over to Bo\n\nReview the login change on branch x."), "{texts:?}");
    assert!(texts.iter().any(|t| t == "Ann handed over to Bo\n\nSecond."));
    assert_eq!(inbox_count(&s, "queued"), 2, "Bo's two items wait in its mailbox");
    assert!(s.service.release_handoffs().unwrap().is_empty(), "delivered once");
    let bo_run = claim(&s).expect("Bo reads them both");
    let job = s.service.store().read(move |c| aurelius_storage::queries::job_row(c, &bo_run.to_string())).unwrap();
    assert_eq!(job.payload["group"]["hop"], 1, "the hop it was held at");
    assert!(wake_text(&s, s.bo).contains("Second."));
}

#[test]
fn a_held_handoff_for_a_paused_colleague_leaves_a_line_and_wakes_nobody() {
    let s = setup();
    use_real_mailbox(s.ann);
    let t = new_ticket(&s, "Paused", None);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    post(&s, t, "@Ann look").unwrap();
    let ann_run = claim(&s).unwrap();
    post(&s, t, "@Ann more").unwrap();
    assert!(hand_off(&s, ann_run, "Bo", "Review.").1.contains("unread"));
    exec(&s, Command::SetEmployeeStatus(aurelius_domain::employee::SetEmployeeStatus { employee_id: s.bo, status: aurelius_domain::employee::LifecycleStatus::Paused })).unwrap();
    finish(&s, ann_run);
    let next = claim(&s).unwrap();
    finish(&s, next);
    s.service.release_handoffs().unwrap();
    assert_eq!(inbox_count(&s, "held"), 0);
    assert!(system_lines(&s, d).iter().any(|(_, t)| t.contains("handoff from Ann to Bo was not delivered") && t.contains("paused")));
    assert!(woken(&s, s.bo).is_empty());
}

#[test]
fn a_delegation_carries_the_hop_and_a_result_coming_back_counts_none() {
    let s = setup();
    let t = new_ticket(&s, "Delegate", None);
    let (_, runs) = post(&s, t, "@Ann split this up").unwrap();
    assert!(hand_off(&s, runs[0], "Atlas", "Please split this up.").0);
    let (atlas_run, _, hop) = last_woken(&s, s.ceo);
    assert_eq!(hop, 1);
    // The delegated task carries one hop more than the run that gave it.
    let (ok, text) = act(&s, atlas_run, EmployeeAction::Delegate { employee: "Cy".into(), title: "Port".into(), goal: "Port the module; report the commit.".into(), criteria: vec![], ticket: Some(t.to_string()) });
    assert!(ok, "{text}");
    let task = s.service.store().read(|c| Ok(c.query_row("SELECT id FROM tasks WHERE title = 'Port'", [], |r| r.get::<_, String>(0))?)).unwrap();
    let hop = s.service.store().read(move |c| aurelius_storage::inbox::task_hop(c, TaskId::parse(&task).unwrap())).unwrap();
    assert_eq!(hop, 2, "one past the hop of the run that delegated");
}

#[test]
fn a_chain_of_delegations_stops_at_the_tickets_limit() {
    let s = setup();
    let (_, runs) = send(&s, None, "@Atlas start").unwrap();
    let run = runs[0];
    s.service
        .store()
        .write(move |tx| Ok(tx.execute("UPDATE jobs SET payload_json = json_set(payload_json, '$.hop', ?2) WHERE id = ?1", rusqlite::params![run.to_string(), TICKET_MAX_HOPS])?))
        .unwrap();
    let t = new_ticket(&s, "Too far", None);
    let (ok, text) = act(&s, run, EmployeeAction::Delegate { employee: "Bo".into(), title: "One more".into(), goal: "x".into(), criteria: vec![], ticket: Some(t.to_string()) });
    assert!(!ok && text.contains("Delegating further") && text.contains("board member"), "{text}");
}

#[test]
fn the_scoped_view_of_a_colleague_shows_what_was_handed_over_to_it() {
    let s = setup();
    let t = new_ticket(&s, "Scoped", None);
    let (_, runs) = post(&s, t, "@Ann look at the login").unwrap();
    assert!(hand_off(&s, runs[0], "Bo", "Please look at the session cookie.").0);
    let (_, runs) = post(&s, t, "@Ann also the logout").unwrap();
    assert!(hand_off(&s, runs[0], "Cy", "Take the logout.").0);
    let update = wake_text(&s, s.cy);
    assert!(!update.contains("session cookie"), "{update}");
    assert!(update.contains("Take the logout"), "the handoff itself: {update}");
    assert!(hand_off(&s, last_woken(&s, s.bo).0, "Cy", "Also check the session cookie for logout.").0);
    let update = wake_text(&s, s.cy);
    assert!(update.contains("Take the logout") && update.contains("Scoped view"), "handed to Cy earlier, in the view: {update}");
}

/// Bo's run ends, as the dispatcher's end of a run does for the ticket (`ending`: how).
fn colleague_ends(s: &Setup, run: RunId, broken: bool) {
    let st = s.service.store().clone();
    s.service
        .store()
        .write(move |tx| {
            tx.execute("UPDATE runs SET status = ?2 WHERE id = ?1", [run.to_string(), if broken { "failed" } else { "succeeded" }.to_owned()])?;
            crate::task::task_runs::run_settled(tx, &st, run, broken)
        })
        .unwrap();
}

fn review_lines(s: &Setup, d: ConversationId) -> usize {
    system_lines(s, d).iter().filter(|(k, _)| *k == SystemLineKind::Review).count()
}

#[test]
fn a_ticket_stays_in_doing_while_a_handoff_is_open_and_reports_once_when_it_is_done() {
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let (t, root, conversation, run) = started_ticket(&s);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    // The owner hands the review to Bo and ends its run, as Flux did on ticket #41.
    assert!(hand_off(&s, run, "Bo", "Review branch fix/x.").0);
    end_run(&s, run, "Committed the fix; Trace reviews it now. Nothing else is open on my side.");
    assert_eq!(task_status(&s, root), TaskStatus::Waiting);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Doing, "the work lies with a colleague");
    assert_eq!(review_lines(&s, d), 0, "no 'ready for review' before the review is in");

    // Bo is done: the owner is woken to deal with the outcome, the ticket is still in Doing.
    let (bo_run, _, _) = last_woken(&s, s.bo);
    colleague_ends(&s, bo_run, false);
    assert_eq!(task_status(&s, root), TaskStatus::Running);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Doing);
    assert_eq!(review_lines(&s, d), 0);
    let told = messages(&s, conversation).into_iter().last().unwrap().text;
    assert!(told.contains("colleagues you handed work to are done"), "{told}");

    // The owner's last run reports: now, and only now, the ticket goes to Review, with one line.
    let next = s.service.store().read(move |c| aurelius_storage::conversations::unfinished_runs_in(c, conversation)).unwrap()[0].0;
    end_run(&s, next, "The review passed; the fix is ready.");
    assert_eq!(task_status(&s, root), TaskStatus::Review);
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Review);
    assert_eq!(review_lines(&s, d), 1);
}

#[test]
fn a_handoff_that_fails_gives_the_ticket_back_to_its_owner_instead_of_leaving_it_in_doing() {
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let (t, root, conversation, run) = started_ticket(&s);
    assert!(hand_off(&s, run, "Bo", "Review branch fix/x.").0);
    end_run(&s, run, "Handed the review to Bo.");
    assert_eq!(task_status(&s, root), TaskStatus::Waiting);
    let (bo_run, _, _) = last_woken(&s, s.bo);
    colleague_ends(&s, bo_run, true);
    assert_eq!(task_status(&s, root), TaskStatus::Running, "the owner is on it again");
    assert_eq!(ticket_view(&s, t).column, ColumnKind::Doing);
    let told = messages(&s, conversation).into_iter().last().unwrap().text;
    assert!(told.contains("did not finish"), "{told}");
}

#[test]
fn the_owner_is_not_woken_while_a_second_colleague_still_works() {
    use aurelius_domain::task::TaskStatus;
    let s = setup();
    let (_, root, _, run) = started_ticket(&s);
    assert!(hand_off(&s, run, "Bo", "Review branch fix/x.").0);
    let (bo_run, _, _) = last_woken(&s, s.bo);
    // Bo hands on to Cy before it ends: the ticket is still not Ann's to report.
    assert!(hand_off(&s, bo_run, "Cy", "Check the cookie.").0);
    end_run(&s, run, "Handed the review to Bo.");
    colleague_ends(&s, bo_run, false);
    assert_eq!(task_status(&s, root), TaskStatus::Waiting, "Cy's run is still open");
    let (cy_run, _, _) = last_woken(&s, s.cy);
    colleague_ends(&s, cy_run, false);
    assert_eq!(task_status(&s, root), TaskStatus::Running);
}
