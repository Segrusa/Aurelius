# Ticket session — UI design (ticket #70)

Status: design, ready to build. Owner: Pixel (UI/UX). Client: desktop (`clients/ts/app`).
Spec: `user-stories.md` in this folder (TS-01 … TS-07, BR1 … BR6). This document says
*where* everything goes and *which existing component* it is; it adds no behaviour that
the stories do not ask for. Words, tokens and components stay the ones the app has.

## 1. Principle

A ticket session is a work session (`screens/session.tsx`) with three extras: a column with
its gate, several employees in one timeline and the ticket's data on the right. So the
screen has the same three parts as a work session, in the same places:

| Part | Work session | Ticket session |
|---|---|---|
| Head | title + place | `#N` + title, column chip, owner · responsible · project, panel toggle, More |
| Centre | `EmployeeChat` feed + `PromptBox` + run chips | the ticket's channel feed (`ChannelBody`), `ProgressCard`, **gate**, `PromptBox` + run chips |
| Right | `RunAside` "Session properties" | `RunAside` "Ticket properties" with tabs Properties / Activity |

Everything that is a decision sits between the timeline and the input, so the board member
reads, decides and types in one vertical line. Everything that is data sits on the right.

## 2. Route and files

| Item | Spec |
|---|---|
| Route | `/ticket/$number` with search `{ message?: string; thread?: string }` (`app.tsx`). |
| Screen | `screens/ticket-session.tsx` → `TicketSessionScreen({ number, message, thread })`, key = number. |
| Logic | `screens/ticket-session-logic.ts` (pure, with `ticket-session-logic.test.ts`), see §9. |
| Key context | `TicketSession`. It binds what `TeamChat` binds for a channel (`StopGeneration`, `RetryAnswer`, `ShowContext`) and the gate keys (§5.4). No new chord. |
| Window title | `#N title` (as other pages set theirs). |
| Sidebar | Unchanged (D2). No row is highlighted except **Board** while a ticket session is open, so the user knows where they came from. |

### 2.1 From number to ticket
`ticket.get` takes an id; the URL has a number (unique on the server, `tickets.rs` takes
`MAX(number)+1`). Resolve in this order, all with existing reads:
1. `channel.list` (already cached as `["messages","channels"]`): the channel with
   `ticket_number === N` gives `ticket_id` and `conversation_id`. Covers archived tickets.
2. Otherwise `ticket.list { query: "#N" }` (the server matches `#N` on the number exactly).
3. Otherwise **not found** (§7).

Then `useTicket(id)` as today. The resolved id is cached per host so a reload does not
re-search. No protocol change.

## 3. Layout

### 3.1 Default (a ticket in Doing, side panel open, window ≥ 1100 px)

```
┌ sidebar ┐┌───────────────────────────────────────────────────────────────┬────────────────────────┐
│         ││ #70  A ticket opens as its own session   ◐ Doing    [▥] [⋯]  │ Ticket properties      │
│ New     ││ owner Pixel · responsible You · Aurelius                     │ [Properties] Activity  │
│ Inbox   ││───────────────────────────────────────────────────────────────│                        │
│ Board ● ││                    ── Today ──                                │ Priority   Normal ▾    │
│ …       ││  Sebastien · board member                                     │ Owner      Pixel ▾     │
│         ││  Pixel: design first, then Flux builds.                      │ Responsible You ▾      │
│         ││  ▸ 3 earlier updates                                          │ Labels     ui, desktop │
│         ││  ⇢ Orion delegated "Build ticket session" to Flux  Open task →│ Deadline   None        │
│         ││  Pixel · UI/UX Designer                                       │ Project    Aurelius    │
│         ││  ui-ontwerp.md is committed on feat/ticket-70-…               │ Runs on    Your login  │
│         ││  ● Flux is writing…                       (LiveRow)           │ Created by sebastien · │
│         ││  ┌ Approval: Flux wants to run npm test ─── Allow  Deny ┐     │ Followers  S, O  +1    │
│         ││  └────────────────────────────────────────────────────────┘   │ ────────────────────── │
│         ││                                                               │ Description        ✎   │
│         ││ ┌ ● Flux · Building the head ··········· 3/7 ▾ ┐ ProgressCard│ Every ticket opens …   │
│         ││ └───────────────────────────────────────────────┘             │ ────────────────────── │
│         ││ ┌ StatusBox (only when the column has something to say) ┐     │ Files                  │
│         ││ └────────────────────────────────────────────────────────┘    │ [spec.md] Add files…   │
│         ││ ┌────────────────────────────────────────────────────────┐    │ ────────────────────── │
│         ││ │ Message Pixel (owner)… · @ to mention                  │    │ Tasks (3)              │
│         ││ │ +                         [Mode ▾] [Model ▾]      (↑)  │    │ ▸ Main task  Pixel  ●  │
│         ││ └────────────────────────────────────────────────────────┘    │   Build …    Flux   ●  │
│         ││  @Name asks that employee; without a name, Pixel (owner) …    │ ────────────────────── │
│         ││                                                               │ Work · Files written · │
│         ││                                                               │ Guidelines · Usage     │
└─────────┘└───────────────────────────────────────────────────────────────┴────────────────────────┘
```

- Centre column: the feed's existing column (`FEED_COLUMN`, max-width as the channel today),
  centred; ProgressCard, gate and PromptBox use the same max-width (820 px like the work
  session's `chooser`) and 24 px side padding, 16 px bottom padding.
- Vertical order at the bottom, fixed: **timeline → ProgressCard → gate → PromptBox → hint**.
  The gate is closest to the input because it is the decision of the moment (TS-04); the
  ProgressCard sits right above it (TS-03.4 "directly above the input" holds whenever there
  is no gate, which is the usual case while work runs).
- Side panel: `RunAside` with a `wide` variant: `clamp(280px, 28%, 380px)` instead of
  `clamp(240px, 24%, 340px)`, because it holds editable text. Same background, border, padding.

### 3.2 Review (gate open)

```
│  … timeline …                                                          │
│ ┌ ● In review · your decision ─────────────────────────────── ▴ ┐      │
│ │ Report (Markdown, max 8 lines)                                 │      │
│ │ … Show the whole report                                        │      │
│ │ Tell Pixel:  [Looks good, thanks] [Please also …]              │      │
│ │ [Accept  Ctrl+⏎]  [Send back to Doing]            Open run →  │      │
│ └────────────────────────────────────────────────────────────────┘      │
│ ┌ Message Pixel (owner)… · @ to mention ────────────── [Mode][Model](↑)┐│
```

- After **Send back to Doing** is pressed once, a `Feedback` field ("What needs to change?")
  opens inside the gate, as today; the second press sends `ticket.send_back`.
- **Reply to <owner>** and **Open discussion** are gone from the gate in the session: the
  PromptBox right under it *is* the reply to the owner (BR1), and the discussion *is* the
  timeline (TS-04.4). The suggested replies stay (one click = `ticket.post`, as today).
- The gate collapses (▴/▾ in its head, remembered under `ticket-session.gate.open`, default
  open) to its head line plus the button row; the buttons never hide.
- Max height of the gate: 45 % of the centre column; the report scrolls inside it. The
  report body starts clamped to 8 lines with **Show the whole report** / **Show less**.

### 3.3 Plan review

Same frame as 3.2 with `PlanGate`: head "Plan review · your decision", the plan clamped to 8
lines with **Show the whole plan**, buttons **Approve (Ctrl+⏎)**, **Approve, accept edits**,
**Request changes**, **Reject**.
- In the session the feedback field is not open from the start (two text boxes above each
  other read as one too many). It opens on the first press of **Request changes**, or with
  the text button **Add a note** next to the Approve buttons; the second press of Request
  changes sends. A filled note goes along with Approve as today (`feedback`).
- Reject keeps its `ConfirmDialog`.

### 3.4 Other columns: StatusBox

`StatusBox` as on the board panel, in the gate slot, with the same texts and buttons: login
wait (Connect Claude login / `ticket.use_my_login`), failed (Start again / Back to Backlog),
Backlog (Start / Delete), Todo (choose an employee, dispatcher line), Doing stopped (Try
again), Testing (Open test task), quick task branch/merge state. When the StatusBox has
nothing to say for the state (plain Doing with a run under way), it renders nothing: the
ProgressCard tells what runs.

### 3.5 Done or archived (read-only, BR5 / TS-07)

```
│  … timeline (readable, threads open read-only) …                      │
│ ┌ StatusBox, only if Done has an action: summary retry, merge failed ┐│
│ └─────────────────────────────────────────────────────────────────────┘│
│ ┌──────────────────────────────────────────────────────────────────────┐│
│ │ #70 is Done · read-only.                                  Send back  ││  ← readOnlyBar row
│ └──────────────────────────────────────────────────────────────────────┘│
```

- No ProgressCard, no gate, no PromptBox, no run chips. The bar is the existing
  `readOnlyRow`/`readOnlyBar` with `readOnlyBar(c, done)` text; its link "Open ticket" goes
  away (we are in it). On Done, the bar ends with **Send back** when the board panel allows
  it today (same RPC, same rights); on archived there is no action.
- Head: the title is not editable; More keeps Follow/Unfollow and Copy link only (and Move
  where the board allows it from Done).
- Side panel: everything readable; writes follow what the board panel allows on Done today
  (`editable(t)`), e.g. no Add files.
- The moment the ticket leaves Done (event → `useTicket` refresh), the session becomes the
  Doing layout without a reload (TS-07.3).

### 3.6 Narrow windows
| Width | Behaviour |
|---|---|
| ≥ 1100 px | as 3.1, panel docked when open. |
| 900–1099 px | panel docked but **closed by default on first open** of the session (user's remembered choice still wins). |
| < 900 px | `RunAside` is hidden by CSS today. The ▥ toggle opens it as an overlay sheet from the right (360 px, card shadow, scrim, Esc and click-outside close, focus moves into it and back to ▥ on close). The head's second line drops the project. |

## 4. Head

```
#70  A ticket opens as its own session   ◐ Doing                          [▥] [⋯]
owner Pixel · responsible You · Aurelius
```

- `header data-titlebar` like the channel head (`styles.top`), two lines.
- **Line 1:** `#N` in `precision-text3` (the `hash` style), then the title as `Editable`
  (`ticket.update { title }` with `expectedVersion`, as the panel) — enabled when
  `editable(t) && online`; then the column chip: `Chip` with `ColumnGlyph` + the board's
  column name (from `useBoard`, fallback `column.replace("_"," ")`). The chip is not a menu;
  moving is in More.
- **Line 2 (subtitle):** while someone runs, the existing "`Flux, Pixel answering`"; else
  `owner X · responsible Y · Project` (Y = "You" when it is me; parts that are empty are
  left out; "No owner" when there is none). Same muted style as the channel subtitle.
- **Right:**
  - ▥ `IconButton` ghost sm, label "Show ticket properties" / "Hide ticket properties",
    `aria-expanded`, `aria-controls` the aside. Remembered under `OVERVIEW_KEY`
    (`chat.ticket-overview.open`) so the existing preference carries over (TS-05.4).
  - ⋯ `Menu` label "More": exactly the board panel's `more` list and `pick` handler
    (Move to <column> via `dropEffect`, Follow/Unfollow, responsible items, Copy link),
    including the quick-task Done/Backlog `ConfirmDialog`s. Copy link copies the
    `/ticket/N` link (§8).
- `TicketOdometer` and `ThreadWaiting` (today under the panel's chips) go into the head's
  line 2 right side and above the feed respectively, as in the channel.

## 5. Centre

### 5.1 Timeline
- It is `ChannelBody`'s feed for the ticket's conversation, unchanged: messages with author
  and role, `LiveRow`, `ApprovalCard`, threads (`ThreadPanel` opens as the right panel and
  *replaces* the properties aside while open, as it replaces `OverviewPanel` today), system
  lines with "Open task →", `foldUpdates` ("N earlier updates"), day and "New messages"
  lines, flash on `?message=`, virtualised with Virtuoso.
- Empty: the existing EmptyState "Discuss the ticket here." + the BR1 sentence.
- Build note: let `ChannelBody` take slots instead of copying it: `head` (replace the channel
  header), `aboveComposer` (gate), `composerFooter` (run chips), `aside` (properties) — and
  drop its own `OverviewPanel` branch when the ticket session renders it. The Team chat keeps
  calling it without slots for general/project channels.

### 5.2 ProgressCard
Unchanged component (`ticket-overview.tsx`), fed by `useTicketProgress(ticketId, live)`.
Hidden when read-only.

### 5.3 PromptBox and run chips (TS-02)
- The channel's `Composer` with `prompt={TEAM_PROMPT}` (PromptBox: + Photos / Files /
  Folder, paste and drop), `everyone`, mentions, quick replies, Stop/Send now — as the
  ticket channel has.
- **Placeholder** from `composerPlaceholder(t)` (§9):
  - owner answers (BR1 columns, not archived, owner set): "Message Pixel (owner)… · @ to mention"
  - otherwise: "Message #70 · @ to mention"
- **Hint line** under the box: the existing `@Name asks that employee${ownerAnswers(owner)}.`
  but with the BR1 rule (not just "column ≠ done"), so in Backlog/Todo-without-owner it says
  "; without a name, no one is woken".
- **Run chips** in the PromptBox's `chips` slot (beside Send, as in the work session):
  - **Mode** `Chip name="Mode" quiet` + `Menu` with `MODES` → `ticket.update { run: {...t.run, mode} }`.
    Hidden for a quick task and when `runLocked(t)`.
  - **Model** `Chip name="Model" quiet` + `Menu` with `useModels()` → `ticket.update { run: {...t.run, model} }`.
    Hidden when `runLocked(t)`.
  - No Effort chip: the board panel has none for tickets today; adding it is a follow-up.
  - Labels as the panel: "Default"/"Owner's default" for unset.

### 5.4 Gate keys
`GateKeys` with `keyed` on: in Review `Ctrl/Cmd+Enter` = Accept (only when ready), in Plan
review = Approve. **Only while the PromptBox is empty or not focused**, because
`Ctrl+Enter` in a filled PromptBox sends the message. The KeyHint on the button shows the chord.

## 6. Side panel "Ticket properties" (TS-05.3)

`RunAside label="Ticket properties"` (wide), `TabList` "Properties | Activity" at the top
(same `TabList`/`Tab` as the board panel, `hasDivider`). Drop and paste of files on the whole
aside add files (the panel's `takeFiles`), except on Done.

**Properties tab**, top to bottom:

1. `SideProperties` (rows `SideRow`, 100 px name column). Editable values are the panel's
   `Menu`/`Editable` placed in the value cell, quiet, so the row reads as text until hovered:
   | Row | Value | Edit |
   |---|---|---|
   | Priority | `priorityLabel` | Menu `PRIORITIES` when `editable` |
   | Owner | `Mark` + name / "No owner" | Menu active employees when `ownerChoosable` (`ticket.assign`) |
   | Responsible | "You" / name / "No one" | Menu: me, other members, no one (`ticket.set_responsible`), feature `ticket_responsible` |
   | Mode · Model | "Owner's default · Opus 5.5" | read only here (chips in the PromptBox edit) |
   | Labels | `labels.join(", ")` / "None" | `Editable` + `parseLabels` |
   | Deadline | date / "None" | `Editable` + `parseDeadline` |
   | Project | name | — |
   | Design | URL | `DesignRow`, only with `DESIGN_FEATURE` |
   | Runs on | "Your Claude login" / "X's Claude login" | — |
   | Created by | `user · time` | — |
   | Followers | `Followers` component | feature `ticket_follow` |
2. **Description** (`SideSection`): `Editable multiline` with Markdown display, "No
   description." when empty. Above it, in Doing with `review_feedback`: the feedback `Box`
   with `feedbackLabel`.
3. **Files** (`SideSection`): `FileChips` + **Add files…** (not on Done).
4. **Tasks (n)** (`SideSection`): the panel's tree (`foldTree(treeOrder(tasks))`, indent
   16 px per depth), each row a button → `/tasks/$taskId`, with `taskStateLine` colour. The
   root row reads "Main task" before its title. Empty: section hidden.
5. **Work** (`SideSection`, only when there is something): Branch, Merged (mono), as
   `OverviewPanel`'s Work block.
6. **Files written**, **Guidelines in the runs**, **Usage**: `FilesWritten`, `RunGuidelines`,
   `UsageTotalsLine`, summed over all runs of the ticket. Source per Titan's decision
   (`user-stories.md` §6): either the client sums the task runs, or a new read behind a
   feature; without that feature the three blocks are hidden — never a zero or a spinner.

**Activity tab:** the board panel's `Activity` component, unchanged.

Gone from the session compared with the board panel: the Close button (the session is a
page; Back/Esc go back in history), the big title (it is in the head) and the column/owner
chips row (head + Properties).

## 7. States

| State | What shows |
|---|---|
| Resolving / loading | Head with `#N` only, `Skeleton` rows in the feed (via `useDelayed`, as the channel), aside skeleton. Offline: "Waiting for the server…". |
| Not found | Centred `EmptyState` "Ticket #N not found." with a Button **Open the Board** (`/board`). No aside. |
| Read failed | `Banner status="error" role="alert"` "This ticket did not load." + error text + **Try again** (refetch). Never an endless loading. |
| Server without `ticket_channels` | Head, gate and properties as normal. Feed area: EmptyState "This server has no ticket conversation."; no PromptBox, no run chips. |
| Offline while open | Everything readable; writes disabled (`can = status === "online"`), `NoConnectionNote` in place of the hint line. |
| Another workspace/project | Opens anyway; the project is in line 2 and in Properties. |
| Two board members | Both get the same events; gate, chip and timeline update live. A decision the other made: the gate disappears and a toast "<name> accepted #N." is **not** added (the timeline line says it). |

## 8. Entry points and redirects (TS-01, BR6)

All go to `/ticket/$number` (use the number on the view; every place listed has it or has the
ticket id, then resolve with `useTicket`). A small helper `openTicket(navigate, t)` in
`ticket-session-logic.ts`'s companion hook keeps this in one place.

| Where | File | Today | Now |
|---|---|---|---|
| Board card click / Enter | `board.tsx:642` | `/board?ticket=` | `/ticket/N` (O1: straight to the session) |
| Palette ticket / ticket channel | `palette.tsx:217,230` | board / chat | `/ticket/N` |
| Ticket link | `links.tsx:27` | board | `/ticket/N` |
| Inbox notification | `inbox.tsx:415,417,460` | board / chat | `/ticket/N` (+ `message`/`thread` when the notification has them) |
| Chat line "Open ticket", made-from-message chip | `chat-message.tsx:858,1386` | board | `/ticket/N` |
| Overview, task run, ideas, scheduled, employee inbox | `ticket-overview.tsx:424`, `task-run.tsx`, `ideas.tsx:383`, `scheduled.tsx:38`, `employee-inbox.tsx:32` | board / chat | `/ticket/N` (for a place with a ticket channel) |
| New session Continue row | `new-session.tsx:409` | board | `/ticket/N` |
| After create (New ticket, Give work, New session) | `dialog-logic.ts:106` | chat / board | `/ticket/N` |
| Chat dock "Open full screen" | `chat-dock.tsx:300` | chat | `/ticket/N` for a ticket |
| Project page channel link | `projects.tsx:945` | chat | unchanged (project channel) |

Redirects (in `app.tsx`, `beforeLoad`/a tiny resolver component, `replace: true`):
- `/chat?channel=<ticket conversation>[&message][&thread]` → `/ticket/N?message&thread`.
  Known from the cached `channel.list` (`kind === "ticket"`); a general/project channel stays.
- `/board?ticket=<id>` and `/history?ticket=<id>` → `/ticket/N` (number via `ticket.get`).
  While resolving: the Board/History page renders as normal, then replaces the URL.
- Copied app links: `ticketLink` writes `…/ticket/<number>`; `parseLink` accepts a number
  (new) and an id (old links), an id resolves via `ticket.get`.

## 9. Team chat without tickets (TS-06)
- `chat-logic.ts` `sections()`: no Tickets section, no ticket channels under Archived.
- Team chat channel search: a hit on a ticket (`#12`, `12`, title) is shown as a row
  "#12 Title" with a small `ColumnGlyph`, and opens `/ticket/12`.
- The "opened/closed #N" lines in General already link through `links`/`chat-message` → `/ticket/N`.
- Chat dock (O2 default): its ticket list stays; picking one shows the compact session in the
  dock: the timeline, ProgressCard, gate and Composer in the dock's narrow column
  (`narrow`), no side panel, head "#N title" + "Open full screen".

## 10. `ticket-session-logic.ts` (pure, tested)
| Function | Returns | Test cases |
|---|---|---|
| `resolveNumber(n, channels, listed)` | `{ ticketId, conversationId } \| "search" \| "missing"` | channel hit (incl. archived), list hit, none |
| `gateOf(t)` | `"plan" \| "review" \| "status" \| "none"` | plan_review, review, every other column, done/archived → none |
| `readOnly(t, channel)` | boolean | done, archived, open, channel read_only |
| `ownerAnswers(t)` | owner name or null (BR1) | each column, archived, no owner |
| `composerPlaceholder(t)` / `composerHint(t)` | strings §5.3 | owner answers / not |
| `sessionRedirect(path, search, channels)` | target or null | `/chat?channel=` ticket vs project, keeps message/thread; `/board?ticket=`; `/history?ticket=` |
| `headLine(t, me, answering)` | line 2 text | answering, owner+responsible, me, none |

`sections()` gets its own test that ticket channels (open and archived) are left out.

## 11. Accessibility
- The page is a `section aria-label="#70 <title>"`; the head is the `h1`.
- The aside is `aside aria-label="Ticket properties"`, its tabs a `tablist`; as an overlay
  sheet it is `role="dialog" aria-modal="true"` with focus trap.
- Gate: `section aria-label="Your decision"`; its buttons are reachable in order Accept →
  Send back → suggested replies → Open run. The collapse toggle has `aria-expanded`.
- Tab order: head (title, ▥, ⋯) → feed → ProgressCard → gate → PromptBox → chips → Send →
  aside.
- Toasts confirm writes; errors in `role="alert"` Banners. Colours and focus rings from the
  theme; the amber gate tone is the existing `Box tone="amber"`.

## 12. Screenshots to deliver (`target/ticket-70/`)
1. `doing.png` — Doing with a live run, ProgressCard and properties open.
2. `review.png` — Review gate with report and suggested replies.
3. `plan-review.png` — Plan review gate.
4. `done-readonly.png` — Done, read-only bar.
5. `narrow-sheet.png` — < 900 px with the properties sheet open.
6. `not-found.png` — `/ticket/9999`.
7. `team-chat.png` — Team chat list without ticket channels.

## 13. Out of scope
Mobile; tickets in the sidebar session list; an Effort chip for tickets; deleting
`TicketPanel` code (it may stay as an internal component, no route opens it).
