//! Threads and reactions in the Team chat (AUR-CHAT-002, ui-ontwerp §5): which
//! actions a message offers, how a thread reads under its root and in its panel,
//! where a streaming answer goes, and how a reaction is toggled and described.
//! Pure, so it is tested without a window; `chat.rs` and `thread_panel.rs` render it.

use aurelius_domain::MessageId;
use aurelius_domain::conversation::MessageStatus;
use aurelius_domain::views::{ReactionView, ThreadSnapshot};
use aurelius_protocol::MessageView;

use crate::screens::team_feed as feed;

/// The thread panel's width beside the channel (ui-ontwerp §5).
pub const THREAD_PANEL_WIDTH: f32 = 360.;

/// At most this many monograms of who replied, beside "↳ n replies".
pub const REPLY_MONOGRAMS: usize = 3;

/// The fixed reactions of the picker (ui-ontwerp §5): no full emoji library.
pub const REACTIONS: [&str; 8] = ["✓", "👀", "👍", "🎉", "❓", "❗", "🙏", "🚀"];

/// The thread composer's placeholder.
pub const THREAD_PLACEHOLDER: &str = "Reply… · @ to mention";

/// What the server and the channel allow (features `chat_threads` / `chat_reactions`;
/// a closed channel allows neither).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Caps {
    pub threads: bool,
    pub reactions: bool,
    pub read_only: bool,
}

/// The actions of a message row besides "Create ticket…" (hover, focus, right-click).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RowActions {
    pub react: bool,
    pub reply: bool,
}

/// What a message offers (ui-ontwerp §5): a finished message of a person (not a
/// system line, not still streaming or queued) can get a reaction; a message of the
/// channel itself (not a reply: no nested threads) can start a thread. Nothing in a
/// closed channel, and nothing the server does not have.
pub fn row_actions(m: &MessageView, caps: Caps) -> RowActions {
    let open = !caps.read_only && m.system.is_none() && m.status != MessageStatus::Partial && !m.queued && m.deleted_at_ms.is_none();
    RowActions {
        react: open && caps.reactions,
        reply: open && caps.threads && m.thread_parent.is_none(),
    }
}

/// "1 reply", "3 replies".
pub fn replies(n: u32) -> String {
    if n == 1 {
        "1 reply".into()
    } else {
        format!("{n} replies")
    }
}

/// When the latest reply was written: its time today, else its day ("Yesterday", "Mon 27 Oct").
pub fn last_reply(ms: i64, offset_minutes: i64, today: i64) -> String {
    let day = feed::local_day(ms, offset_minutes);
    if day == today {
        feed::clock(ms, offset_minutes)
    } else {
        feed::day_label(day, today)
    }
}

/// The line under a root in the channel (ui-ontwerp §5): "↳ 3 replies · last 11:02".
/// None for a message without replies.
pub fn thread_line(m: &MessageView, offset_minutes: i64, today: i64) -> Option<String> {
    let count = m.thread_reply_count.filter(|n| *n > 0)?;
    Some(match m.thread_last_reply_ms {
        Some(ms) => format!("↳ {} · last {}", replies(count), last_reply(ms, offset_minutes, today)),
        None => format!("↳ {}", replies(count)),
    })
}

/// Replies in the thread the reader has not read yet.
pub fn unread(m: &MessageView) -> u32 {
    m.thread_unread.unwrap_or(0)
}

/// What a screen reader hears for the thread line: "Thread, 3 replies, 2 unread, last reply 11:02".
pub fn thread_label(m: &MessageView, offset_minutes: i64, today: i64) -> String {
    let count = m.thread_reply_count.unwrap_or(0);
    let mut label = format!("Thread, {}", replies(count));
    if unread(m) > 0 {
        label.push_str(&format!(", {} unread", unread(m)));
    }
    if let Some(ms) = m.thread_last_reply_ms {
        label.push_str(&format!(", last reply {}", last_reply(ms, offset_minutes, today)));
    }
    label
}

/// Who replied under `root` (the server's `thread_repliers`, newest first, at most
/// `REPLY_MONOGRAMS`): the names for the monograms beside the thread line, with whether
/// it is a board member (a name that is none of `employees`).
pub fn repliers(root: &MessageView, employees: &[&str]) -> Vec<(String, bool)> {
    root.thread_repliers.iter().take(REPLY_MONOGRAMS).map(|name| (name.clone(), !employees.contains(&name.as_str()))).collect()
}

/// The panel's title (ui-ontwerp §5): "Thread · #42-login-per-board-member".
pub fn thread_title(channel_title: &str) -> String {
    format!("Thread · #{}", channel_title.trim_start_matches('#').trim_start())
}

/// "── 3 replies ──" between the root and its replies; none without replies.
pub fn replies_divider(n: usize) -> Option<String> {
    (n > 0).then(|| replies(u32::try_from(n).unwrap_or(u32::MAX)))
}

/// A thread can be read, not answered: the server says so, or the channel is closed.
pub fn thread_read_only(snapshot: Option<&ThreadSnapshot>, channel_read_only: bool) -> bool {
    channel_read_only || snapshot.is_some_and(|s| s.read_only)
}

/// The thread was read (`thread.read`): its root in the channel takes the counters
/// the server sent and has no unread replies any more (the snapshot's root still
/// has the count from before the read).
pub fn mark_thread_read(messages: &mut [MessageView], snapshot: &ThreadSnapshot) {
    if let Some(root) = messages.iter_mut().find(|m| m.id == snapshot.root.id) {
        root.thread_reply_count = snapshot.root.thread_reply_count;
        root.thread_last_reply_ms = snapshot.root.thread_last_reply_ms;
        root.thread_unread = None;
    }
}

// ---- Streams ---------------------------------------------------------------------

/// Where an answer streaming in the open channel shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamTo {
    /// In the channel's list (not in a thread).
    Channel,
    /// In the open thread panel.
    Thread,
    /// In another thread: nowhere until it is a reply (its root's counters follow).
    Elsewhere,
}

/// An answer with `thread_parent` streams into its thread, never into the channel.
pub fn stream_to(thread_parent: Option<MessageId>, open_thread: Option<MessageId>) -> StreamTo {
    match thread_parent {
        None => StreamTo::Channel,
        Some(root) if open_thread == Some(root) => StreamTo::Thread,
        Some(_) => StreamTo::Elsewhere,
    }
}

/// A stream a view of the channel itself (the ticket's Discussion tab) shows: not an
/// answer that goes into a thread.
pub fn in_channel(stream: &aurelius_protocol::StreamDelta) -> bool {
    stream.thread_parent.is_none()
}

// ---- Reactions -------------------------------------------------------------------

/// A click on `emoji` (a pill or the picker): `on` adds the reader's reaction, off
/// takes it back.
pub fn reaction_on(reactions: &[ReactionView], emoji: &str) -> bool {
    !reactions.iter().any(|r| r.emoji == emoji && r.mine)
}

/// The reader's reaction as asked, shown at once (the server's `message.reacted`
/// follows and the list is read again). `me` is the reader's name in `by`.
pub fn apply_reaction(reactions: &mut Vec<ReactionView>, emoji: &str, on: bool, me: Option<&str>) {
    let at = reactions.iter().position(|r| r.emoji == emoji);
    match (at, on) {
        (Some(i), true) if !reactions[i].mine => {
            let r = &mut reactions[i];
            r.count += 1;
            r.mine = true;
            r.by.extend(me.map(str::to_owned));
        }
        (None, true) => reactions.push(ReactionView {
            emoji: emoji.to_string(),
            count: 1,
            mine: true,
            by: me.map(str::to_owned).into_iter().collect(),
        }),
        (Some(i), false) if reactions[i].mine => {
            let r = &mut reactions[i];
            r.count = r.count.saturating_sub(1);
            r.mine = false;
            if let Some(name) = me
                && let Some(j) = r.by.iter().rposition(|b| b == name)
            {
                r.by.remove(j);
            }
            if r.count == 0 {
                reactions.remove(i);
            }
        }
        _ => {}
    }
}

/// Who reacted, for the pill's tooltip: "✓ Alexandra, you and Forge", with "and n
/// more" when the server named fewer than reacted.
pub fn reaction_tip(r: &ReactionView, me: Option<&str>) -> String {
    let mut names: Vec<String> = r
        .by
        .iter()
        .map(|b| if Some(b.as_str()) == me { "you".to_string() } else { b.clone() })
        .collect();
    if names.is_empty() && r.mine {
        names.push("you".into());
    }
    let more = (r.count as usize).saturating_sub(names.len());
    let who = match (names.split_last(), more) {
        (None, _) => return format!("{} · {}", r.emoji, r.count),
        (Some((last, [])), 0) => last.clone(),
        (Some((last, rest)), 0) => format!("{} and {last}", rest.join(", ")),
        (Some(_), n) => format!("{} and {n} more", names.join(", ")),
    };
    format!("{} {who}", r.emoji)
}

/// What a screen reader hears for a pill: "✓, 2 reactions, including yours".
pub fn reaction_label(r: &ReactionView) -> String {
    let count = if r.count == 1 { "1 reaction".to_string() } else { format!("{} reactions", r.count) };
    if r.mine {
        format!("{}, {count}, including yours", r.emoji)
    } else {
        format!("{}, {count}", r.emoji)
    }
}

/// The picker's eight reactions, each with whether the reader gave it already.
pub fn picker(reactions: &[ReactionView]) -> Vec<(&'static str, bool)> {
    REACTIONS.iter().map(|e| (*e, !reaction_on(reactions, e))).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::ConversationId;
    use aurelius_domain::conversation::MessageRole;

    fn msg(text: &str) -> MessageView {
        MessageView {
            edited_at_ms: None, deleted_at_ms: None, editable: false,
            id: MessageId::new(),
            ordinal: 1,
            role: MessageRole::Assistant,
            status: MessageStatus::Complete,
            text: text.into(),
            created_at_ms: 0,
            run_id: None,
            reply_to: None,
            produced_by: None,
            author: Some("Forge".into()),
            sources: vec![],
            activity: vec![],
            update: None,
            approval_id: None,
            files: vec![],
            uploads: vec![],
            queued: false,
            usage: None,
            mode: None,
            author_user: None,
            system: None,
            thread_parent: None,
            thread_reply_count: None,
            thread_last_reply_ms: None,
            thread_unread: None,
            thread_repliers: vec![],
            reactions: vec![],
        }
    }

    fn reaction(emoji: &str, by: &[&str], mine: bool) -> ReactionView {
        ReactionView {
            emoji: emoji.into(),
            count: by.len() as u32,
            mine,
            by: by.iter().map(|b| b.to_string()).collect(),
        }
    }

    const ALL: Caps = Caps {
        threads: true,
        reactions: true,
        read_only: false,
    };

    #[test]
    fn a_message_offers_react_and_reply_only_where_the_server_and_the_channel_allow() {
        let m = msg("Done.");
        assert_eq!(row_actions(&m, ALL), RowActions { react: true, reply: true });
        assert_eq!(row_actions(&m, Caps::default()), RowActions::default());
        assert_eq!(row_actions(&m, Caps { threads: false, ..ALL }), RowActions { react: true, reply: false });
        assert_eq!(row_actions(&m, Caps { reactions: false, ..ALL }), RowActions { react: false, reply: true });
        // A closed channel: neither (ui-ontwerp §3).
        assert_eq!(row_actions(&m, Caps { read_only: true, ..ALL }), RowActions::default());
    }

    #[test]
    fn a_reply_a_system_line_and_a_streaming_answer_start_no_thread() {
        let reply = MessageView { thread_parent: Some(MessageId::new()), ..msg("Yes") };
        assert_eq!(row_actions(&reply, ALL), RowActions { react: true, reply: false });
        let streaming = MessageView { status: MessageStatus::Partial, ..msg("Look") };
        assert_eq!(row_actions(&streaming, ALL), RowActions::default());
        let queued = MessageView { queued: true, ..msg("Later") };
        assert_eq!(row_actions(&queued, ALL), RowActions::default());
        let line = MessageView {
            system: Some(aurelius_domain::views::SystemLine {
                kind: aurelius_domain::views::SystemLineKind::Moved,
                task_id: None,
                from: None,
                to: None,
                by: None,
                ticket_id: None,
                conversation_id: None,
                message_id: None,
                action: None,
                channel_id: None,
            }),
            ..msg("moved")
        };
        assert_eq!(row_actions(&line, ALL), RowActions::default());
    }

    #[test]
    fn the_thread_line_counts_replies_and_says_when_the_last_came() {
        let today = feed::local_day(1_700_000_000_000, 0);
        let root = MessageView {
            thread_reply_count: Some(3),
            thread_last_reply_ms: Some(1_700_000_000_000),
            ..msg("Root")
        };
        let at = feed::clock(1_700_000_000_000, 0);
        assert_eq!(thread_line(&root, 0, today).as_deref(), Some(format!("↳ 3 replies · last {at}").as_str()));
        let one = MessageView { thread_reply_count: Some(1), ..root.clone() };
        assert_eq!(thread_line(&one, 0, today), Some(format!("↳ 1 reply · last {at}")));
        // Yesterday's reply says its day, not a time that reads as today.
        assert_eq!(thread_line(&root, 0, today + 1).as_deref(), Some("↳ 3 replies · last Yesterday"));
        assert_eq!(thread_line(&msg("Alone"), 0, today), None);
        assert_eq!(thread_line(&MessageView { thread_reply_count: Some(0), ..msg("x") }, 0, today), None);
    }

    #[test]
    fn the_thread_label_names_unread_replies() {
        let today = feed::local_day(0, 0);
        let root = MessageView {
            thread_reply_count: Some(3),
            thread_unread: Some(2),
            ..msg("Root")
        };
        assert_eq!(unread(&root), 2);
        assert_eq!(thread_label(&root, 0, today), "Thread, 3 replies, 2 unread");
        assert_eq!(unread(&msg("x")), 0);
    }

    #[test]
    fn repliers_are_the_servers_newest_three_members_told_apart() {
        let root = MessageView { thread_repliers: vec!["Pulse".into(), "alexandra".into(), "Forge".into(), "Titan".into()], ..msg("root") };
        assert_eq!(repliers(&root, &["Forge", "Pulse", "Titan"]), [("Pulse".to_string(), false), ("alexandra".to_string(), true), ("Forge".to_string(), false)]);
        assert!(repliers(&msg("no replies"), &["Forge"]).is_empty());
    }

    #[test]
    fn the_panel_is_titled_by_its_channel() {
        assert_eq!(thread_title("# 42-login-per-board-member"), "Thread · #42-login-per-board-member");
        assert_eq!(thread_title("# general"), "Thread · #general");
        assert_eq!(replies_divider(0), None);
        assert_eq!(replies_divider(3).as_deref(), Some("3 replies"));
    }

    #[test]
    fn a_thread_is_read_only_when_the_server_or_the_channel_says_so() {
        let snapshot = |read_only| ThreadSnapshot {
            as_of_seq: 1,
            conversation_id: ConversationId::new(),
            root: msg("Root"),
            replies: vec![],
            read_only,
        };
        assert!(!thread_read_only(Some(&snapshot(false)), false));
        assert!(thread_read_only(Some(&snapshot(true)), false));
        assert!(thread_read_only(Some(&snapshot(false)), true));
        assert!(!thread_read_only(None, false));
    }

    #[test]
    fn reading_a_thread_clears_its_unread_marker_in_the_channel() {
        let root = MessageView {
            thread_reply_count: Some(2),
            thread_unread: Some(2),
            ..msg("Root")
        };
        let other = MessageView { thread_unread: Some(1), ..msg("Other") };
        let mut channel = vec![root.clone(), other.clone()];
        let snapshot = ThreadSnapshot {
            as_of_seq: 1,
            conversation_id: ConversationId::new(),
            root: MessageView {
                thread_reply_count: Some(3),
                thread_last_reply_ms: Some(9),
                ..root.clone()
            },
            replies: vec![],
            read_only: false,
        };
        mark_thread_read(&mut channel, &snapshot);
        assert_eq!((channel[0].thread_reply_count, channel[0].thread_last_reply_ms, channel[0].thread_unread), (Some(3), Some(9), None));
        assert_eq!(channel[1].thread_unread, Some(1));
    }

    #[test]
    fn a_thread_answer_streams_into_its_panel_never_into_the_channel() {
        let (root, other) = (MessageId::new(), MessageId::new());
        assert_eq!(stream_to(None, Some(root)), StreamTo::Channel);
        assert_eq!(stream_to(Some(root), Some(root)), StreamTo::Thread);
        assert_eq!(stream_to(Some(other), Some(root)), StreamTo::Elsewhere);
        assert_eq!(stream_to(Some(root), None), StreamTo::Elsewhere);
    }

    #[test]
    fn a_thread_answer_is_not_a_stream_of_the_channel() {
        let delta = aurelius_protocol::StreamDelta { run_id: aurelius_domain::RunId::new(), conversation_id: ConversationId::new(), message_id: MessageId::new(), text: "On it".into(), activity: vec![], progress: None, activity_changes: false, mode: None, thread_parent: None };
        assert!(in_channel(&delta));
        assert!(!in_channel(&aurelius_protocol::StreamDelta { thread_parent: Some(MessageId::new()), ..delta }));
    }

    #[test]
    fn a_click_adds_a_reaction_or_takes_the_readers_back() {
        let mut list = vec![reaction("✓", &["forge"], false)];
        assert!(reaction_on(&list, "✓"));
        apply_reaction(&mut list, "✓", true, Some("alexandra"));
        assert_eq!(list, [ReactionView { emoji: "✓".into(), count: 2, mine: true, by: vec!["forge".into(), "alexandra".into()] }]);
        assert!(!reaction_on(&list, "✓"));
        // Twice on stays one reaction (the server is idempotent too).
        apply_reaction(&mut list, "✓", true, Some("alexandra"));
        assert_eq!(list[0].count, 2);
        apply_reaction(&mut list, "✓", false, Some("alexandra"));
        assert_eq!(list, [reaction("✓", &["forge"], false)]);
        // A new emoji goes last; the last reaction taken back removes its pill.
        apply_reaction(&mut list, "🚀", true, Some("alexandra"));
        assert_eq!(list.iter().map(|r| r.emoji.as_str()).collect::<Vec<_>>(), ["✓", "🚀"]);
        apply_reaction(&mut list, "🚀", false, Some("alexandra"));
        assert_eq!(list.len(), 1);
        // Off on someone else's reaction changes nothing.
        apply_reaction(&mut list, "✓", false, Some("alexandra"));
        assert_eq!(list, [reaction("✓", &["forge"], false)]);
    }

    #[test]
    fn the_tooltip_says_who_reacted_with_you_for_the_reader() {
        assert_eq!(reaction_tip(&reaction("✓", &["forge"], false), Some("alexandra")), "✓ forge");
        assert_eq!(reaction_tip(&reaction("✓", &["forge", "alexandra"], true), Some("alexandra")), "✓ forge and you");
        assert_eq!(reaction_tip(&reaction("👀", &["a", "b", "c"], false), None), "👀 a, b and c");
        let short = ReactionView { count: 4, ..reaction("👍", &["a", "b"], false) };
        assert_eq!(reaction_tip(&short, None), "👍 a, b and 2 more");
        let nameless = ReactionView { count: 2, ..reaction("🎉", &[], false) };
        assert_eq!(reaction_tip(&nameless, None), "🎉 · 2");
    }

    #[test]
    fn a_pill_reads_its_count_and_whether_it_is_yours() {
        assert_eq!(reaction_label(&reaction("✓", &["a", "b"], true)), "✓, 2 reactions, including yours");
        assert_eq!(reaction_label(&reaction("👀", &["a"], false)), "👀, 1 reaction");
    }

    #[test]
    fn the_picker_has_exactly_the_eight_fixed_reactions_with_yours_marked() {
        let list = vec![reaction("👀", &["me"], true), reaction("✓", &["forge"], false)];
        let shown = picker(&list);
        assert_eq!(shown.iter().map(|(e, _)| *e).collect::<Vec<_>>(), ["✓", "👀", "👍", "🎉", "❓", "❗", "🙏", "🚀"]);
        assert_eq!(shown.iter().filter(|(_, mine)| *mine).map(|(e, _)| *e).collect::<Vec<_>>(), ["👀"]);
    }
}
