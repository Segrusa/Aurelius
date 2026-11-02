//! The Team chat as a Slack-style feed (AUR-CHAT-001, ui-ontwerp §2): which
//! messages group under one header, where the day separators and the line "New
//! messages" go, how `@names` are marked and which shape a system line gets.
//! Pure, so it is tested without a window; `chat.rs` renders it.

use aurelius_domain::MessageId;
use aurelius_domain::conversation::MessageRole;
use aurelius_domain::group_chat::GROUP_KIND;
use aurelius_domain::views::{SystemLine, SystemLineKind, system_action};
use aurelius_protocol::MessageView;

/// A message joins the one before it (no header of its own) when the same author wrote it within this long.
pub const GROUP_WINDOW_MS: i64 = 5 * 60 * 1000;

const DAY_MS: i64 = 86_400_000;

// ---- Grouping and separators -----------------------------------------------------

/// Who wrote a message, as far as grouping goes: a board member by id, an
/// employee by name. None for what never groups: a system line, a task update
/// card, a server note without an author.
pub fn author_key(m: &MessageView) -> Option<String> {
    if m.system.is_some() {
        return None;
    }
    if let Some(user) = &m.author_user {
        return Some(format!("member:{}", user.id));
    }
    match m.role {
        MessageRole::User => Some("member".into()),
        MessageRole::Assistant if m.update.as_deref().is_none_or(|k| k == GROUP_KIND) => m
            .author
            .as_deref()
            .map(str::trim)
            .filter(|a| !a.is_empty())
            .map(|a| format!("employee:{a}")),
        MessageRole::Assistant => None,
    }
}

/// The local calendar day of `ms` (days since 1970-01-01), `offset_minutes` east of UTC.
pub fn local_day(ms: i64, offset_minutes: i64) -> i64 {
    (ms + offset_minutes * 60_000).div_euclid(DAY_MS)
}

/// "10:02": the time of day of `ms` beside a message (its day is in the separator above).
pub fn clock(ms: i64, offset_minutes: i64) -> String {
    let minutes = (ms + offset_minutes * 60_000)
        .div_euclid(60_000)
        .rem_euclid(24 * 60);
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// A day separator's label (ui-ontwerp §2): "Today", "Yesterday", else "Mon 27 Oct",
/// with the year when it is not this year.
pub fn day_label(day: i64, today: i64) -> String {
    match today - day {
        0 => "Today".into(),
        1 => "Yesterday".into(),
        _ => {
            const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
            const MONTHS: [&str; 12] = [
                "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
            ];
            let (year, month, date) = civil(day);
            // 1970-01-01 was a Thursday; both indexes are in range by construction.
            let weekday = WEEKDAYS[(day + 4).rem_euclid(7) as usize];
            let month = MONTHS[(month - 1) as usize];
            if year == civil(today).0 {
                format!("{weekday} {date} {month}")
            } else {
                format!("{weekday} {date} {month} {year}")
            }
        }
    }
}

/// Year, month (1–12) and day of the month of a day since 1970-01-01 (Howard Hinnant's algorithm).
fn civil(day: i64) -> (i64, i64, i64) {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let date = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(month <= 2), month, date)
}

/// What goes before a message of the feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Before {
    /// A day separator: the first message, or the first of another day.
    pub day: bool,
    /// The line "New messages".
    pub new: bool,
    /// No header: it continues the message before it.
    pub grouped: bool,
}

/// Before `messages[i]` (ui-ontwerp §2): a message is grouped when the same author
/// wrote the one before less than `GROUP_WINDOW_MS` earlier and nothing lies
/// between them (no day separator, no "New messages", no system line).
/// `day_of` gives a moment's local day.
pub fn before(
    messages: &[MessageView],
    i: usize,
    new_at: Option<MessageId>,
    day_of: impl Fn(i64) -> i64,
) -> Before {
    let Some(m) = messages.get(i) else {
        return Before::default();
    };
    let previous = i.checked_sub(1).and_then(|p| messages.get(p));
    let day = previous.is_none_or(|p| day_of(p.created_at_ms) != day_of(m.created_at_ms));
    let new = new_at == Some(m.id);
    let grouped = !day
        && !new
        && previous.is_some_and(|p| {
            let gap = m.created_at_ms - p.created_at_ms;
            author_key(m).is_some_and(|a| author_key(p).as_ref() == Some(&a))
                && (0..GROUP_WINDOW_MS).contains(&gap)
        });
    Before { day, new, grouped }
}

/// Where "New messages" goes when a channel opens (ui-ontwerp §2): before the first
/// message after `read_through`, the reader's last read ordinal as the channel list
/// had it before opening (opening marks it read). Only while something is unread.
/// Without `read_through` (an older server, a channel never read) the `unread`
/// last messages are the new ones, as before.
pub fn new_messages_at(ordinals: &[u32], read_through: Option<u32>, unread: u32) -> Option<usize> {
    if unread == 0 {
        return None;
    }
    match read_through {
        Some(read) => ordinals.iter().position(|o| *o > read),
        None => crate::screens::discussion::new_divider(ordinals.len(), unread),
    }
}

// ---- Mentions --------------------------------------------------------------------

/// `text` with each `@Name` of someone in `people` as a link that goes nowhere
/// (`#@Name`, which `markdown_link` ignores), so the Markdown shows it in the link
/// colour; the reader's own name (`me`) bold as well (ui-ontwerp §2). Code, code
/// blocks, existing links and e-mail addresses stay as they are.
pub fn mark_mentions(text: &str, people: &[&str], me: Option<&str>) -> String {
    let mut out = String::with_capacity(text.len() + 16);
    let mut fence = false;
    for line in text.split_inclusive('\n') {
        let fenced = line.trim_start().starts_with("```") || line.trim_start().starts_with("~~~");
        if fenced || fence {
            fence ^= fenced;
            out.push_str(line);
            continue;
        }
        let (mut code, mut bracket, mut previous) = (false, 0usize, None::<char>);
        let mut chars = line.char_indices().peekable();
        while let Some((i, ch)) = chars.next() {
            match ch {
                '`' => code = !code,
                '[' if !code => bracket += 1,
                ']' if !code => bracket = bracket.saturating_sub(1),
                '@' if !code
                    && bracket == 0
                    && previous.is_none_or(|p| p.is_whitespace() || p == '(') =>
                {
                    let rest = &line[i + 1..];
                    let len = rest
                        .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == '-' || c == '.'))
                        .unwrap_or(rest.len());
                    let name = rest[..len].trim_end_matches('.');
                    if let Some(known) = people
                        .iter()
                        .find(|p| p.eq_ignore_ascii_case(name))
                        .filter(|_| !name.is_empty())
                    {
                        let mine = me.is_some_and(|me| me.eq_ignore_ascii_case(known));
                        let link = format!("[@{name}](#@{name})");
                        out.push_str(&if mine { format!("**{link}**") } else { link });
                        // Skip the name (ASCII-safe: it ends on a character boundary).
                        while chars.peek().is_some_and(|(j, _)| *j <= i + name.len()) {
                            chars.next();
                        }
                        previous = name.chars().next_back();
                        continue;
                    }
                }
                _ => {}
            }
            out.push(ch);
            previous = Some(ch);
        }
    }
    out
}

// ---- System lines ----------------------------------------------------------------

/// How a system line reads: the shape before its text (never colour alone) and
/// whether it reports a failure (NEGATIVE text).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LineMark {
    pub mark: &'static str,
    pub failed: bool,
}

/// The shape of a system line (ui-ontwerp §2, Titan's O-4): by its `action` when the
/// server sent one, else by its `kind`. A run result is `Reported`; its text says
/// whether the run failed.
pub fn line_mark(line: &SystemLine, text: &str) -> LineMark {
    let quiet = |mark| LineMark {
        mark,
        failed: false,
    };
    match line.action.as_deref() {
        Some(system_action::OPENED) => quiet("+ "),
        Some(system_action::CLOSED) => quiet("■ "),
        Some(system_action::REOPENED) => quiet("↺ "),
        Some(system_action::APPROVED) => quiet("✓ "),
        Some(system_action::ADJUSTED | system_action::SENT_BACK) => quiet("↩ "),
        Some(system_action::TESTED_PASSED) => quiet("▶ ✓ "),
        Some(system_action::TESTED_FAILED) => LineMark {
            mark: "▶ ✕ ",
            failed: true,
        },
        Some(system_action::JOINED) => quiet("+ "),
        _ => match line.kind {
            SystemLineKind::Moved => quiet("→ "),
            SystemLineKind::Done => quiet("■ "),
            SystemLineKind::Plan | SystemLineKind::Review => quiet("◆ "),
            SystemLineKind::Delegated => quiet("↳ "),
            SystemLineKind::Reported if text.to_lowercase().contains("failed") => LineMark {
                mark: "▶ ✕ ",
                failed: true,
            },
            SystemLineKind::Reported => quiet("▶ "),
            SystemLineKind::SummaryFailed => LineMark {
                mark: "",
                failed: true,
            },
            _ => quiet(""),
        },
    }
}

/// A system line in words: the server's text, else made from its `action`, else
/// from its kind (`system_text`), so a terse server still reads well. `ticket`:
/// the number of the ticket it is about, when known.
pub fn line_text(line: &SystemLine, text: &str, ticket: Option<u32>) -> String {
    if !text.trim().is_empty() {
        return text.trim().to_string();
    }
    let by = line
        .by
        .as_deref()
        .map(str::trim)
        .filter(|b| !b.is_empty())
        .unwrap_or("Aurelius");
    let number = ticket.map_or_else(|| "The ticket".to_string(), |n| format!("#{n}"));
    match line.action.as_deref() {
        Some(system_action::OPENED) => format!("{number} opened · {by}"),
        Some(system_action::CLOSED) => format!("{number} closed · {by}"),
        Some(system_action::REOPENED) => format!("{number} reopened · {by}"),
        Some(system_action::APPROVED) => format!("{by} approved the plan"),
        Some(system_action::ADJUSTED) => format!("{by} asked to adjust the plan"),
        Some(system_action::SENT_BACK) => format!(
            "{by} sent {} back",
            ticket.map_or_else(|| "the ticket".to_string(), |n| format!("#{n}"))
        ),
        Some(system_action::TESTED_PASSED) => "Tests passed".into(),
        Some(system_action::TESTED_FAILED) => "Tests failed".into(),
        Some(system_action::JOINED) => format!("{by} joined"),
        _ => crate::screens::discussion::system_text(line, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aurelius_domain::conversation::MessageStatus;

    fn msg(role: MessageRole, author: Option<&str>, at_ms: i64, ordinal: u32) -> MessageView {
        MessageView {
            edited_at_ms: None, deleted_at_ms: None, editable: false,
            id: MessageId::new(),
            ordinal,
            role,
            status: MessageStatus::Complete,
            text: "hi".into(),
            created_at_ms: at_ms,
            run_id: None,
            reply_to: None,
            produced_by: None,
            author: author.map(str::to_owned),
            sources: vec![],
            activity: vec![],
            update: (role == MessageRole::Assistant).then(|| GROUP_KIND.to_string()),
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

    const MIN: i64 = 60_000;
    // 2026-11-01 10:00 UTC.
    const T: i64 = 1_793_527_200_000;

    fn day(ms: i64) -> i64 {
        local_day(ms, 0)
    }

    #[test]
    fn the_same_author_within_five_minutes_is_grouped_under_one_header() {
        let list = vec![
            msg(MessageRole::Assistant, Some("Forge"), T, 1),
            msg(MessageRole::Assistant, Some("Forge"), T + 4 * MIN, 2),
            msg(MessageRole::Assistant, Some("Forge"), T + 10 * MIN, 3),
        ];
        assert!(
            !before(&list, 0, None, day).grouped,
            "the first has its header"
        );
        assert!(before(&list, 1, None, day).grouped);
        assert!(
            !before(&list, 2, None, day).grouped,
            "six minutes later: a header again"
        );
        assert_eq!(GROUP_WINDOW_MS, 5 * MIN);
    }

    #[test]
    fn another_author_a_system_line_or_the_new_messages_line_breaks_a_group() {
        let mut line = msg(MessageRole::Assistant, None, T + MIN, 2);
        line.system = Some(SystemLine::new(SystemLineKind::Moved));
        let list = vec![
            msg(MessageRole::Assistant, Some("Forge"), T, 1),
            msg(MessageRole::Assistant, Some("Orion"), T + MIN, 2),
            line,
            msg(MessageRole::Assistant, Some("Orion"), T + 2 * MIN, 4),
            msg(MessageRole::Assistant, Some("Orion"), T + 3 * MIN, 5),
        ];
        assert!(!before(&list, 1, None, day).grouped, "another author");
        assert!(!before(&list, 3, None, day).grouped, "after a system line");
        assert!(before(&list, 4, None, day).grouped);
        let new = before(&list, 4, Some(list[4].id), day);
        assert!(
            new.new && !new.grouped,
            "the New messages line starts a header again"
        );
    }

    #[test]
    fn a_board_member_groups_by_their_id_and_task_updates_never_group() {
        let user = |name: &str| aurelius_domain::user::UserView {
            id: aurelius_domain::UserId::new(),
            username: name.into(),
            created_at_ms: 0,
            created_by_user: None,
            disabled_at_ms: None,
        };
        let member = user("ada");
        let mut a = msg(MessageRole::User, None, T, 1);
        a.author_user = Some(member.clone());
        let mut b = a.clone();
        (b.id, b.created_at_ms) = (MessageId::new(), T + MIN);
        let mut other = b.clone();
        (other.id, other.author_user) = (MessageId::new(), Some(user("bo")));
        assert!(before(&[a.clone(), b.clone()], 1, None, day).grouped);
        assert!(!before(&[a, other], 1, None, day).grouped);
        let mut card = msg(MessageRole::Assistant, Some("Forge"), T, 1);
        card.update = Some("plan".into());
        assert_eq!(author_key(&card), None);
    }

    #[test]
    fn a_new_day_gets_a_separator_and_breaks_a_group() {
        let list = vec![
            msg(MessageRole::Assistant, Some("Forge"), T, 1),
            msg(MessageRole::Assistant, Some("Forge"), T + MIN, 2),
            msg(MessageRole::Assistant, Some("Forge"), T + 14 * 60 * MIN, 3),
        ];
        assert!(
            before(&list, 0, None, day).day,
            "the first message opens its day"
        );
        assert!(!before(&list, 1, None, day).day);
        let next = before(&list, 2, None, day);
        assert!(next.day && !next.grouped);
        // Midnight counts in local time: 23:30 UTC is the next day two hours east.
        let late = T + 13 * 60 * MIN + 30 * MIN;
        assert_eq!(local_day(late, 0), local_day(T, 0));
        assert_eq!(local_day(late, 120), local_day(T, 0) + 1);
    }

    #[test]
    fn day_separators_say_today_yesterday_or_the_date_with_the_year_only_when_another() {
        let today = local_day(T, 0);
        assert_eq!(day_label(today, today), "Today");
        assert_eq!(day_label(today - 1, today), "Yesterday");
        assert_eq!(day_label(today - 5, today), "Tue 27 Oct");
        assert_eq!(day_label(today - 365, today), "Sat 1 Nov 2025");
    }

    #[test]
    fn a_message_shows_its_local_time_of_day() {
        assert_eq!(clock(T, 0), "10:00");
        assert_eq!(clock(T + 7 * MIN, 60), "11:07");
        assert_eq!(
            clock(T, -11 * 60),
            "23:00",
            "west of UTC: the evening before"
        );
    }

    #[test]
    fn the_new_messages_line_goes_after_what_the_reader_had_read() {
        let ordinals = [1, 2, 3, 4, 5];
        assert_eq!(
            new_messages_at(&ordinals, Some(3), 2),
            Some(3),
            "before ordinal 4"
        );
        assert_eq!(
            new_messages_at(&ordinals, Some(3), 0),
            None,
            "nothing unread: no line"
        );
        assert_eq!(
            new_messages_at(&ordinals, Some(5), 1),
            None,
            "read to the end"
        );
        assert_eq!(
            new_messages_at(&ordinals, None, 2),
            Some(3),
            "an older server: the unread count"
        );
        assert_eq!(new_messages_at(&[], Some(0), 3), None);
    }

    #[test]
    fn known_mentions_are_marked_and_the_readers_own_is_bold() {
        let people = ["Forge", "Orion", "sebastien"];
        assert_eq!(
            mark_mentions("@Forge please, cc @sebastien.", &people, Some("sebastien")),
            "[@Forge](#@Forge) please, cc **[@sebastien](#@sebastien)**."
        );
        assert_eq!(
            mark_mentions("(@orion) hi", &people, None),
            "([@orion](#@orion)) hi"
        );
        assert_eq!(
            mark_mentions("@Nobody here", &people, None),
            "@Nobody here",
            "unknown names stay plain"
        );
    }

    #[test]
    fn mentions_in_code_links_and_mail_addresses_stay_as_written() {
        let people = ["Forge"];
        assert_eq!(
            mark_mentions("mail forge@Forge.io", &people, None),
            "mail forge@Forge.io"
        );
        assert_eq!(
            mark_mentions("run `@Forge` now", &people, None),
            "run `@Forge` now"
        );
        assert_eq!(
            mark_mentions("[@Forge](https://x)", &people, None),
            "[@Forge](https://x)"
        );
        assert_eq!(
            mark_mentions("```\n@Forge\n```\n@Forge", &people, None),
            "```\n@Forge\n```\n[@Forge](#@Forge)"
        );
        assert_eq!(
            mark_mentions("é @Forge ü", &people, None),
            "é [@Forge](#@Forge) ü"
        );
    }

    #[test]
    fn a_system_line_is_shaped_by_its_action_before_its_kind() {
        let line = |kind, action: Option<&str>| {
            let l = SystemLine::new(kind);
            match action {
                Some(a) => l.with_action(a),
                None => l,
            }
        };
        assert_eq!(
            line_mark(&line(SystemLineKind::Done, Some(system_action::CLOSED)), "").mark,
            "■ "
        );
        assert_eq!(
            line_mark(
                &line(SystemLineKind::Plan, Some(system_action::APPROVED)),
                ""
            )
            .mark,
            "✓ "
        );
        assert_eq!(
            line_mark(
                &line(SystemLineKind::Plan, Some(system_action::SENT_BACK)),
                ""
            )
            .mark,
            "↩ "
        );
        assert_eq!(
            line_mark(
                &line(SystemLineKind::Plan, Some(system_action::ADJUSTED)),
                ""
            )
            .mark,
            "↩ "
        );
        assert_eq!(
            line_mark(
                &line(SystemLineKind::Moved, Some(system_action::TESTED_PASSED)),
                ""
            ),
            LineMark {
                mark: "▶ ✓ ",
                failed: false
            }
        );
        assert_eq!(
            line_mark(
                &line(SystemLineKind::Moved, Some(system_action::TESTED_FAILED)),
                ""
            ),
            LineMark {
                mark: "▶ ✕ ",
                failed: true
            }
        );
        assert_eq!(
            line_mark(
                &line(SystemLineKind::TicketCreated, Some(system_action::OPENED)),
                ""
            )
            .mark,
            "+ "
        );
        assert_eq!(
            line_mark(&line(SystemLineKind::Moved, Some("a_newer_action")), "").mark,
            "→ ",
            "an unknown action falls back to the kind"
        );
        assert_eq!(line_mark(&line(SystemLineKind::Moved, None), "").mark, "→ ");
    }

    #[test]
    fn a_run_result_is_a_reported_line_whose_text_says_whether_it_failed() {
        let reported = SystemLine::new(SystemLineKind::Reported);
        assert_eq!(
            line_mark(&reported, "Forge reported: 128 passed"),
            LineMark {
                mark: "▶ ",
                failed: false
            }
        );
        assert_eq!(
            line_mark(&reported, "Forge's run failed: 2 failed"),
            LineMark {
                mark: "▶ ✕ ",
                failed: true
            }
        );
    }

    #[test]
    fn a_line_without_text_is_worded_from_its_action_else_its_kind() {
        let mut closed = SystemLine::new(SystemLineKind::Done).with_action(system_action::CLOSED);
        closed.by = Some("sebastien".into());
        assert_eq!(line_text(&closed, "", Some(42)), "#42 closed · sebastien");
        assert_eq!(
            line_text(&closed, "  #42 closed by the board ", Some(42)),
            "#42 closed by the board",
            "the server's text first"
        );
        let mut moved = SystemLine::new(SystemLineKind::Moved);
        (moved.from, moved.to) = (Some("Doing".into()), Some("Review".into()));
        assert_eq!(line_text(&moved, "", None), "Aurelius moved Doing → Review");
    }
}
