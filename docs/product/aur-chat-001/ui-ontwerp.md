# UI design AUR-CHAT-001: Slack-style Team chat, one channel per ticket

- **Task:** AUR-CHAT-001-D (Pixel, for Orion) · **Status:** short concept, for approval · 2026-11-01
- **Builds on:** `fase-2/ui-ontwerp.md` §1–§5 (discussion, channels, @, P-21…P-27), tokens in `clients/desktop/src/theme.rs` and `clients/mobile/lib/core/theme/tokens.dart`. No new design system: only existing tokens, sizes and components.
- **Marking:** **[P1]** = phase 1 (build now) · **[P2]** = phase 2 (designed now, built later).

## 0. What exists and is reused
| Part | Desktop (`clients/desktop/src/screens/`) | Mobile (`clients/mobile/lib/`) |
|---|---|---|
| Team chat page | `group_chat.rs` `render_group_chat`, `group_scroll` | `features/team/team_page.dart` `TeamPage` |
| Channel list | `group_chat.rs` `render_channel_list` (180 px, eyebrows, `@n` ACCENT badge, unread SURFACE_2 pill, "▸ Archived (n)") | `showChannelSheet` (sheet with `Eyebrow`, `@ n` accent pill, `cell2` count, `ExpansionTile` Archived), `channels_bloc.dart` |
| Ordering, labels, badge | `discussion.rs` `ordered`, `channel_title`, `channel_label`, `badge`, `step_channel`, `new_divider`, `channel_hint` | `data/discussion.dart` `channelGroups`, `channelLabel`, `channelSemantics` |
| Message rows | `chat.rs` `render_messages` (bubbles, "New" divider), `ticket_discussion.rs` (flat row: monogram 20 px, name, kind, time, `@ you` + ACCENT left bar via `names_me`) | `chat_view.dart` `_Messages`, `_UserBubble`, `_AssistantMessage`, `SystemLineView`, `TicketPill` |
| System lines | `discussion.rs` `system_text`, `system_label`, `login_mark`; "┄ text · time ┄", 11.5 px, TEXT_2 | `SystemLineView`, `_SystemLine` |
| Composer, @ | `chat.rs` `composer`; `discussion.rs` `typing_mention`, `mention_suggestions`, `MentionPicker`, `unknown_warning` | `_Composer`, `features/team/mentions.dart` `MentionRow` |
| Ticket status | `ticket.rs` `pill` (border BORDER_STRONG, 11.5 px) | `board/board_page.dart` `_ColumnChip`, `ticket_card.dart` `_Pill` |
| Avatars | `ui::monogram` (employee), `ui::member_monogram` (board member) | `Monogram` |

**Key decision (proposal):** a ticket channel *is* the ticket's existing discussion (`TicketView::conversation_id`, AUR-MU-02). No second conversation: the channel and the Discussion tab show the same messages, read state and composer rules (`writable`, `reaches_owner`, P-21). `ChannelView` needs additive fields for this (see §7, O-1).

## 1. Sidebar (channel list) **[P1]**
```
┌ CHANNELS ──────────────┐   180 px, as now (render_channel_list)
│ # general              │
│ # aurelius           3 │   ← unread: SEMIBOLD + TEXT + SURFACE_2 count pill
│ # segrusa-web       @2 │   ← mention: ACCENT pill "@2" (replaces the count)
│ TICKETS                │   ← newest activity first (last_activity_ms desc)
│ # 52-audit-export   @1 │
│ # 42-login-per-memb… 5 │   ← truncated; full title in tooltip + header
│ # 47-vault-rotation    │   ← read: regular weight, TEXT_2
│ ▸ Archived (14)        │   ← collapsed by default (channels_archived_open = false)
└────────────────────────┘
```
- **Channels:** `# general` first, then the active project channels A–Z (today's PROJECTS eyebrow merges into CHANNELS; the list was getting two short groups).
- **Tickets:** one row per ticket that has a discussion and is not Done/archived, sorted by `last_activity_ms` (newest first; same timestamp → ticket number desc). The list does not re-sort while the pointer hovers it, so a row never jumps under the click.
- **Name:** `# <id>-<slug>` — id without `#`, slug = title lowercased, non-alphanumerics → `-`, max 24 chars, e.g. `# 42-login-per-board-member` → shows `# 42-login-per-board-mem…` truncated by the row. Aria label: "Ticket 42, Login per board member, 5 unread, 1 mention".
- **Archived:** archived projects **and** tickets on Done or archived (their discussion is read-only, B7), each A–Z / newest first. Row suffix stays "read-only".
- **Badges** (unchanged, P-24): mention = ACCENT pill `@n` (never colour alone); otherwise unread = SURFACE_2 pill with the count. The open channel shows no badge. Sidebar item Team chat keeps mentions + dot.
- **Optional, ≥ 30 tickets:** a "Show more (n)" row after 15 tickets; Ctrl+K "Go to #42…" finds all.
- Keys as now: Alt+↑/↓ walks Channels → Tickets → Archived (only when expanded).

## 2. Message list **[P1]**
The Team chat leaves the bubble layout of `chat.rs` and uses the flat row of `ticket_discussion.rs` in every channel (Slack-like, everyone left-aligned, yours too).
```
──────────────── Today ────────────────          ← day separator: eyebrow, BORDER lines
(A) Alexandra · board member            10:02   ← monogram 20 px, name MEDIUM 12.5, kind TEXT_2, time 11 TEXT_2
    Please keep the argon2 params configurable.
    Also add a test for the defaults.            ← grouped: same author < 5 min, no header
┄ Forge moved #42 Doing → Review · 10:41 ┄      ← system line (compact)
─────────────── New messages ──────────────      ← ACCENT line + ACCENT_TEXT (was "New")
▌(F) Forge — Backend Developer   @ you  10:44   ← own mention: ACCENT left bar + ACCENT_HALO bg
▌    Done. @Alexandra the cost is in config.     ← own @name = ACCENT_HALO pill, ACCENT_TEXT, SEMIBOLD
(O) Orion — Project Manager             10:46
    @Forge please also update the docs.          ← other @name = ACCENT_TEXT, no background
```
- **Grouping:** a message joins the previous one when the author is the same, < 5 min apart, and nothing lies between (no day separator, New line or system line). A grouped row has no monogram/name; its text aligns with the name (28 px indent). Desktop shows its time (11 px, MUTED) in the 28 px gutter on hover; mobile shows none.
- **Day separator:** "Today", "Yesterday", else "Mon 27 Oct" (year added when not this year), centred eyebrow 11 px TEXT_2 between 1 px BORDER lines; `chat.rs` already has the day maths (`days_ago`).
- **New messages:** as `new_divider` / `channel_new_at` today, label "New messages"; stays until the channel is left.
- **Own mentions** (stronger): existing `@ you` tag + 2 px ACCENT left bar, **plus** ACCENT_HALO row background. Others' mentions: only the `@Name` text in ACCENT_TEXT. Aria "…, mentions you" as now.
- **Ticket events** (system lines, `system_text`): one line, 11.5 px TEXT_2, centred "┄ … · time ┄", with a shape before the text so colour is never the only signal:

| Event | Line |
|---|---|
| Status (Moved, Done) | `→ Forge moved #42 Doing → Review · 10:41` |
| Approve | `✓ Sébastien approved the plan · 10:50` |
| Send back / Pas aan | `↩ Sébastien sent #42 back to Doing · 10:52` — the feedback itself follows as Sébastien's normal message (AUR-MU-02 decision 1) |
| Run result | `▶ Run finished · ✓ 128 passed` or `▶ Run failed · ✕ 2 failed  [Open task]` (failure text NEGATIVE + ✕) |
| Delegated / reported / summary | as in `fase-2 §1` |

  More than 3 system lines in a row fold into `┄ 5 events · Show ┄` (click expands; the Activity tab keeps the full timeline, P-22).
- Streaming answers, plan cards, review cards and "Forge is answering…" stay as in the discussion.

## 3. Channel header **[P1]**
```
┌──────────────────────────────────────────────────────────────────────┐  h 68 px as now
│ # 42-login-per-board-member                          [Open ticket ↗] │  ← Outline button
│ [Review] · Login per board member · (F) Forge  (S) Schema            │  ← ticket.rs pill · title · assignees
└──────────────────────────────────────────────────────────────────────┘
```
- Line 2: status chip = the board column (`ticket.rs` `pill`), the full title (truncates first), then assignee(s): owner first with 16 px monograms + names; > 2 → "+2" with the names in a tooltip. "Forge answering…" (pulse dot) stays right, before the button.
- **Open ticket** opens `Panel::Ticket` on the Ticket tab (desktop), the ticket page (mobile). Project channels keep today's line ("Project channel · you and n employees").
- **Archived / Done ticket:** the composer is replaced by a read-only bar at the bottom (where today's `channel_hint` note sits): SURFACE background, 1 px BORDER top, 12.5 px TEXT_2: *"#42 is Done · read-only."* or *"This project is archived · read-only."* + link **Open ticket**. No composer, no @ list, no hover action "Reply in thread".

## 4. Composer **[P1]**
- The existing `composer` (desktop) / `_Composer` (mobile), placeholder from `channel_hint` / `composer_hint`: "Message #42-login-per… · @ to mention" (P-21 hint "Message Forge (owner)…" in ticket channels).
- **Markdown** as today: rendered after sending (`message_markdown`); no toolbar. `**bold**`, `` `code` ``, lists and links render in the stream.
- **Enter** sends; **Shift+Enter** inserts a new line (desktop). Mobile: Return = new line, the send button sends (platform habit).
- **@-autocomplete** as built (`mention_suggestions`, `MentionPicker`, `MentionRow`): a list above the caret on desktop (↑↓, Enter/Tab, Esc), chips above the keyboard on mobile; employees in org order, then board members; `unknown_warning` under the composer.

## 5. Phase 2: threads and reactions **[P2]**
```
Desktop, hover on a message (top right of the row, SURFACE, 1 px BORDER, radius 6):
  [☺ React] [↳ Reply in thread] [🔗 Copy link] [⌁ Ticket] [⋯]

(F) Forge — Backend Developer           10:44 ┃ Thread · #42-login…        × ┃  ← side panel 360 px,
    Done. The cost is in config.              ┃ (F) Forge  10:44               ┃    like side_panels.rs
    [✓ 2] [👀 1] [☺+]                          ┃     Done. The cost is in config┃
    ↳ 3 replies · last 11:02                  ┃ ── 3 replies ──                ┃
                                              ┃ (A) Alexandra  10:58 …         ┃
                                              ┃ [ Reply… · @ to mention     ] ┃
```
- **Hover actions** (desktop): shown on hover and on keyboard focus of the row; same actions in the right-click menu (where "Create ticket…" is today). Mobile: long press → action sheet (React, Reply in thread, Copy link, Create ticket).
- **Copy link:** `aurelius://channel/<conversation>/<message>`; opening it scrolls to the message and flashes it ACCENT_HALO for 1.5 s.
- **Thread:** desktop = side panel on the right (360 px, the existing side-panel frame), the channel stays visible; mobile = its own pushed screen "Thread · #42-…" with the root message on top. Under the root in the channel: `↳ 3 replies · last 11:02` (ACCENT_TEXT, 12 px) + up to 3 reply monograms. Thread replies do not appear in the channel (no "also send to channel" in P2). Unread thread replies that mention you count in the channel's `@` badge.
- **Reactions row:** under the message text, pills `[✓ 2]` radius 10, SURFACE_2, 11.5 px; your own reaction = ACCENT_HALO bg + ACCENT border. `[☺+]` opens a small picker of 8 fixed reactions (✓ 👀 👍 🎉 ❓ ❗ 🙏 🚀), no full emoji library. Tooltip / long press shows who reacted. Employees may react ✓/👀 (e.g. "seen, working on it").

## 6. Mobile adaptation **[P1]**
The sheet `showChannelSheet` becomes the **first screen of the Team tab** (Slack mobile): a channel list; tapping a row pushes the chat screen with its own back button. The channel pill on the chat screen goes away.
```
┌─────────────────────────────────┐      ┌─────────────────────────────────┐
│ Team                            │      │ ‹ Team  # 42-login-per…  [Open] │ ← GlassAppBar
│ CHANNELS                        │      │ [Review] · Forge, Schema        │ ← _ColumnChip + names
│ # general                       │      │ ───────── Today ─────────       │
│ # aurelius              [ 3 ]   │      │ (A) Alexandra · 10:02           │
│ TICKETS                         │      │ Please keep the argon2 params…  │
│ # 52-audit-export      [@ 1]    │      │ ┄ → Forge moved to Review ┄     │
│   Atlas: Can we add an ex… · 2m │      │ ──── New messages ────          │
│ # 42-login-per-board-…  [ 5 ]   │      │ ▌(F) Forge · @ you · 10:44      │
│   [Review] Forge: Done. @…· 1h  │      │ ╭─────────────────────────────╮ │
│ ▸ Archived (14)                 │      │ │ Message #42… · @        [↑] │ │
└─────────────────────────────────┘      └─────────────────────────────────┘
```
- Rows: `ListTile` as in `showChannelSheet` (≥ 44 pt), title bold when unread, trailing `@ n` accent pill and/or `cell2` count. Ticket rows get a subtitle: the status as a small `_Pill` + last message preview + `Fmt.ago(lastActivityMs)` (exists). Sections with `Eyebrow`; Archived = `ExpansionTile`, collapsed by default.
- Tab **Team** keeps the badge with the number of mentions. A push for a mention opens the chat screen on that message, with the list underneath in the back stack.
- Chat screen: the same rules as §2 (flat rows, grouping, separators); header line 2 shows status + assignees; **Open** = ticket page. Archived: the read-only bar instead of `_Composer` (`readOnlyNote` exists).
- Tablet/landscape ≥ 700 pt: list (300 pt) and chat side by side, like desktop.

## 7. Open points (for Orion / Titan)
| ID | Point | Proposal |
|---|---|---|
| O-1 | `ChannelView` lacks ticket data | Additive fields `ticket_id`, `ticket_title`, `column`, `assignees`, and `kind` (general/project/ticket) in the next protocol freeze; `channel.list` then lists ticket discussions too. Titan decides. |
| O-2 | Shift+Enter today = "queue behind the running answer" (`keymap.rs` SendNow note) | In the Team chat Shift+Enter becomes a new line; queueing there is not needed (posts wake employees, they do not interrupt a run). Alt+Enter keeps "Send now". Conflict to confirm by Titan. |
| O-3 | When does a Done ticket leave Tickets? | Directly on Done → Archived (read-only anyway). Alternative: 24 h later. |
| O-4 | Run results in the discussion | Needs a system line kind for run end (not additive for older clients, see AUR-MU-04 decision 1) → next freeze; until then only Moved/Plan/Review/Done lines. |
| O-5 | Many tickets | "Show more" after 15 rows; mute per channel stays an idea (P-25). |

## 8. Decisions on the open points (Titan, after the freeze 199d958)
| ID | Decision |
|---|---|
| O-1 | Done in the freeze: `ChannelView` has `kind`, `ticket_id`, `ticket_number`, `ticket_column`, `member`, `member_count`, `read_through` and `archived`; `channel.list` lists ticket channels to clients that accept `ticket_channels`. The name `#<n>-<slug>` comes from the server. Title and assignees for the header come from `ticket.get` when the channel opens (no extra fields). Search is the separate read `channel.search { query }`. |
| O-2 | Accepted: in the Team chat (General, project and ticket channels) Enter sends and Shift+Enter is a new line; Alt+Enter keeps "Send now". Employee and task chats keep today's keys. |
| O-3 | A ticket channel moves to Archived as soon as the server says `archived` (Done or archived ticket); no 24 h delay. |
| O-4 | No new line kind. A run result is `SystemLineKind::Reported` (task id; the text says reported or failed), a test verdict carries `action` `tested_passed` / `tested_failed`; decisions carry `approved`, `adjusted`, `sent_back`; General's lines carry `opened` / `closed` with `ticket_id` + `channel_id` (the link). Render by `action` when present, else by `kind`. |
| O-5 | Accepted: "Show more" after 15 rows in Tickets and in Archived. |
| Reopen | No write can move a ticket out of Done today. The engine unarchives the channel (with a "reopened" line) when a ticket leaves Done; a reopen write is a product decision for later. Clients must handle `archived` turning false. |
| Writes to an archived channel | Refused with error code `channel_archived` (clients that accept `ticket_channels`); show the read-only bar, never a raw error. |
