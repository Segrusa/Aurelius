//! Threads and reactions in the discussion engine (AUR-CHAT-002): a reply goes
//! into the thread of a message of the same channel, the answers it wakes come
//! back into that thread (also the owner's, through the main task), and a
//! reaction is once per board member and emoji, not in a closed channel.

use super::*;
use aurelius_domain::MessageId;
use aurelius_domain::group_chat::ReactToMessage;
use aurelius_domain::views::MessageView;

fn thread(s: &Setup, conversation: ConversationId, root: MessageId) -> Vec<MessageView> {
    let st = s.service.store().clone();
    s.service.store().read(move |c| aurelius_storage::conversations::list_thread(c, conversation, root, &|k| st.blobs().read(k).ok().and_then(|b| String::from_utf8(b).ok()))).unwrap()
}

fn reply(s: &Setup, channel: ConversationId, parent: MessageId, text: &str) -> Result<(MessageId, Vec<EmployeeId>, Vec<RunId>), AppError> {
    match exec(s, Command::SendGroupMessage(SendGroupMessage { confirm_secret: false, text: text.into(), images: vec![], files: vec![], channel: Some(channel), thread_parent: Some(parent) }))? {
        CommandResult::GroupMessageSent { message_id, to, run_ids, .. } => Ok((message_id, to, run_ids)),
        other => panic!("{other:?}"),
    }
}

fn ticket_reply(s: &Setup, ticket: TicketId, parent: MessageId, text: &str) -> Result<(MessageId, Vec<EmployeeId>, Vec<RunId>), AppError> {
    match exec(s, Command::PostTicketMessage(ticket::PostTicketMessage { confirm_secret: false, ticket_id: ticket, text: text.into(), images: vec![], files: vec![], thread_parent: Some(parent) }))? {
        CommandResult::TicketMessagePosted { message_id, to, run_ids, .. } => Ok((message_id, to, run_ids)),
        other => panic!("{other:?}"),
    }
}

fn mirror(s: &Setup, employee: EmployeeId, run: RunId, text: &'static str) {
    let st = s.service.store().clone();
    let chat = aurelius_storage_main_chat(s, employee);
    s.service.store().write(move |tx| discussion::mirror_answer(tx, &st, run, chat, text, None, &[], None)).unwrap();
}

fn react(s: &Setup, message: MessageId, emoji: &str, on: bool) -> Result<bool, AppError> {
    match exec(s, Command::ReactToMessage(ReactToMessage { message_id: message, emoji: emoji.into(), on }))? {
        CommandResult::MessageReacted { changed, .. } => Ok(changed),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_reply_wakes_into_its_thread_and_the_answer_and_later_posts_land_there() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let root = sent_message(&s, Some(orbit), "Plan for the release?");
    let (reply_id, to, runs) = reply(&s, orbit, root, "@Ann what do you think?").unwrap();
    assert_eq!(to, vec![s.ann]);
    // The reply is in the thread, not in the channel's own list.
    assert!(!messages(&s, orbit).iter().any(|m| m.id == reply_id));
    assert_eq!(thread(&s, orbit, root).iter().map(|m| m.id).collect::<Vec<_>>(), [root, reply_id]);
    // Ann's run knows it answers in that thread, and was told so with the thread as context.
    let marking = s.service.store().read(|c| aurelius_storage::conversations::group_run(c, runs[0])).unwrap().unwrap();
    assert_eq!((marking.conversation_id, marking.thread_parent), (orbit, Some(root)));
    let update = messages(&s, aurelius_storage_main_chat(&s, s.ann)).into_iter().last().unwrap();
    assert!(update.text.contains("This is a reply in a thread of the #Orbit channel"), "{}", update.text);
    assert!(update.text.contains("Plan for the release?"), "the thread's root is in the context: {}", update.text);
    // Its answer is copied into the thread.
    mirror(&s, s.ann, runs[0], "Ship on Friday.");
    let answer = thread(&s, orbit, root).into_iter().find(|m| m.text == "Ship on Friday.").expect("in the thread");
    assert_eq!((answer.author.as_deref(), answer.thread_parent), (Some("Ann"), Some(root)));
    assert!(!messages(&s, orbit).iter().any(|m| m.text == "Ship on Friday."));
    // A post of that run goes into the thread too, and says so.
    let (ok, text) = post_in_channel(&s, runs[0], None, "And a changelog.");
    assert!(ok && text.starts_with("Posted in the thread in the #Orbit channel."), "{text}");
    assert!(thread(&s, orbit, root).iter().any(|m| m.text == "And a changelog."));
    // A reply that wakes nobody hears why in the thread, not in the channel.
    exec(&s, Command::SetEmployeeStatus(aurelius_domain::employee::SetEmployeeStatus { employee_id: s.bo, status: aurelius_domain::employee::LifecycleStatus::Paused })).unwrap();
    reply(&s, orbit, root, "@Bo and you?").unwrap();
    assert!(thread(&s, orbit, root).iter().any(|m| m.system.is_some() && m.text.contains("Bo is paused")));
    assert!(!messages(&s, orbit).iter().any(|m| m.text.contains("Bo is paused")));
}

#[test]
fn a_reply_answers_a_message_of_the_same_channel_that_is_not_itself_a_reply() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let in_general = sent_message(&s, None, "In General");
    let root = sent_message(&s, Some(orbit), "Root");
    let (first, _, _) = reply(&s, orbit, root, "A reply").unwrap();
    // Another channel's message, a reply, an unknown message: validation, for every client.
    for parent in [in_general, first, MessageId::new()] {
        let err = reply(&s, orbit, parent, "x").unwrap_err();
        assert_eq!(err.code, ErrorCode::Validation, "{parent}");
    }
    assert!(reply(&s, orbit, first, "x").unwrap_err().message.contains("reply itself"));
    assert_eq!(thread(&s, orbit, root).len(), 2, "nothing was stored");
}

#[test]
fn a_reply_in_a_closed_channel_is_refused_like_a_message() {
    let s = setup();
    let t = new_ticket(&s, "Login", Some(s.ann));
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let root = sent_message(&s, Some(d), "Root");
    engine_puts(&s, t, ColumnKind::Done);
    let err = ticket_reply(&s, t, root, "late").unwrap_err();
    assert_eq!(err.code, ErrorCode::ChannelArchived, "the daemon sends validation to a client without ticket_channels");
    assert_eq!(reply(&s, d, root, "late").unwrap_err().code, ErrorCode::ChannelArchived);
    let orbit = channel(&s, s.project);
    let in_orbit = sent_message(&s, Some(orbit), "Before");
    exec(&s, Command::SetProjectStatus(aurelius_domain::employee::SetProjectStatus { project_id: s.project, status: aurelius_domain::employee::LifecycleStatus::Archived })).unwrap();
    assert_eq!(reply(&s, orbit, in_orbit, "late").unwrap_err().code, ErrorCode::ChannelArchived);
}

#[test]
fn a_woken_colleague_in_a_tickets_thread_answers_in_that_thread() {
    let s = setup();
    let t = new_ticket(&s, "Login", Some(s.ann));
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let root = sent_message(&s, Some(d), "The form is slow.");
    let (_, to, runs) = ticket_reply(&s, t, root, "@Bo can you profile it?").unwrap();
    assert_eq!(to, vec![s.bo]);
    mirror(&s, s.bo, runs[0], "The query is the problem.");
    assert!(thread(&s, d, root).iter().any(|m| m.text == "The query is the problem." && m.author.as_deref() == Some("Bo")));
    let (ok, text) = post_in_discussion(&s, runs[0], "Fixed in my branch.");
    assert!(ok && text.starts_with("Posted in the thread in the discussion of ticket #1."), "{text}");
    assert!(thread(&s, d, root).iter().any(|m| m.text == "Fixed in my branch."));
    assert!(!messages(&s, d).iter().any(|m| m.text == "Fixed in my branch."));
}

#[test]
fn the_owners_answer_to_a_reply_that_reached_the_main_task_lands_in_that_thread() {
    let s = setup();
    let t = new_ticket(&s, "Login", Some(s.ann));
    let (_, root_conversation) = started(&s, t);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let root = sent_message(&s, Some(d), "@Bo FYI");
    // Without @: the owner gets it in the main task, saying it is a reply in a thread.
    let (_, to, runs) = ticket_reply(&s, t, root, "Which browser?").unwrap();
    assert_eq!((to, runs), (vec![s.ann], vec![]));
    let note = messages(&s, root_conversation).into_iter().last().unwrap();
    assert!(note.text.starts_with("Which browser?") && note.text.contains("From a thread in the discussion of ticket #1"), "{}", note.text);
    // The main task's run answers with post_to_discussion: into the thread.
    let run = runs_in(&s, root_conversation)[0].0;
    let (ok, text) = post_in_discussion(&s, run, "Safari only.");
    assert!(ok && text.starts_with("Posted in the thread"), "{text}");
    assert!(thread(&s, d, root).iter().any(|m| m.text == "Safari only." && m.author.as_deref() == Some("Ann")));
    // After a message in the channel itself reached it, its posts go to the channel again.
    post(&s, t, "New question in the channel.").unwrap();
    let (ok, text) = post_in_discussion(&s, run, "Answer in the channel.");
    assert!(ok && text.starts_with("Posted in the discussion"), "{text}");
    assert!(messages(&s, d).iter().any(|m| m.text == "Answer in the channel."));
}

#[test]
fn a_reaction_is_once_per_board_member_and_emoji_and_can_be_taken_back() {
    let s = setup();
    let orbit = channel(&s, s.project);
    let root = sent_message(&s, Some(orbit), "Ship it?");
    assert!(react(&s, root, "👍", true).unwrap());
    assert!(!react(&s, root, " 👍 ", true).unwrap(), "on twice is once (trimmed)");
    assert!(react(&s, root, "🚀", true).unwrap());
    let reactions = |s: &Setup| {
        let user = s.user;
        let mut list = messages(s, orbit);
        s.service.store().read(move |c| aurelius_storage::threads::decorate(c, orbit, Some(user), &mut list).map(|_| list)).unwrap().into_iter().find(|m| m.id == root).unwrap().reactions
    };
    let seen = reactions(&s);
    assert_eq!(seen.iter().map(|r| (r.emoji.as_str(), r.count, r.mine, r.by.clone())).collect::<Vec<_>>(), [("👍", 1, true, vec!["grace".to_owned()]), ("🚀", 1, true, vec!["grace".to_owned()])]);
    assert!(react(&s, root, "👍", false).unwrap());
    assert!(!react(&s, root, "👍", false).unwrap(), "off twice is off");
    assert_eq!(reactions(&s).iter().map(|r| r.emoji.as_str()).collect::<Vec<_>>(), ["🚀"]);
    // Only a change is an event (`message.reacted`).
    let events = s.service.store().read(|c| Ok(c.query_row("SELECT COUNT(*) FROM events WHERE type = 'message.reacted'", [], |r| r.get::<_, i64>(0))?)).unwrap();
    assert_eq!(events, 3);
    // Validation: an empty emoji, and a message of no channel; a board member only.
    assert_eq!(react(&s, root, "  ", true).unwrap_err().code, ErrorCode::Validation);
    send(&s, Some(orbit), "@Ann hi").unwrap();
    let in_chat = messages(&s, aurelius_storage_main_chat(&s, s.ann)).first().map(|m| m.id).expect("the update that woke Ann");
    assert_eq!(react(&s, in_chat, "👍", true).unwrap_err().code, ErrorCode::Validation);
    assert_eq!(react(&s, MessageId::new(), "👍", true).unwrap_err().code, ErrorCode::NotFound);
    let command = Command::ReactToMessage(ReactToMessage { message_id: root, emoji: "👀".into(), on: true });
    let Err(err) = s.service.execute(&ctx(None), CommandEnvelope { command_id: CommandId::new(), expected_version: None, command }) else { panic!("reacted without a board member") };
    assert_eq!(err.code, ErrorCode::BoardMemberRequired);
}

#[test]
fn a_reaction_in_a_closed_channel_is_refused() {
    let s = setup();
    let t = new_ticket(&s, "Login", None);
    let d = ticket_view(&s, t).conversation_id.unwrap();
    let root = sent_message(&s, Some(d), "Done?");
    engine_puts(&s, t, ColumnKind::Done);
    assert_eq!(react(&s, root, "✓", true).unwrap_err().code, ErrorCode::ChannelArchived);
}
