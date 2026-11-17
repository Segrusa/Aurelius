# Ticket session — user stories and acceptance criteria

Status: functional spec, approved as a plan by Sebastien on 2026-11-17. Owner: Nova (BA).
Next: Pixel (`ui-ontwerp.md`) and Flux (desktop build), coordinated by Orion.
Client: desktop (`clients/ts/app`). Mobile is a later ticket.

## 1. Why

A work session (`/session/:conversationId`) is one place where you talk, see what runs and
adjust the run. A ticket is split over three places today: its channel in the Team chat
(`chat.tsx` `ChannelBody` + `OverviewPanel`), the ticket panel on the Board
(`ticket-panel.tsx`) and the panel in History. The board member has to switch between
them to talk, follow progress and decide.

Goal: **a ticket opens as a session**. It is one screen per ticket with the conversation, the
progress, the decision of the moment and the ticket data. What sets it apart from a work
session:

- **status and workflow**: the column, plus plan review, review, accept, send back and cancel;
- **several employees**: delegations and tasks of different employees in one timeline;
- **ticket data**: priority, labels, deadline, responsible, owner, followers and files.

## 2. Decisions (Sebastien, 2026-11-17)

| # | Decision |
|---|---|
| D1 | A ticket gets its own session screen, instead of the Team chat channel and the board panel. |
| D2 | The sidebar session list (work sessions) stays as it is. Tickets do not go into it. |
| D3 | The lead answers, as in the ticket thread today (see BR1). |
| D4 | The ticket session replaces the channel. Ticket channels leave the Team chat list, and the conversation stays the same one. |

## 3. Business rules

- **BR1 Who answers.** The server behaviour stays the same (`crates/application/src/conversation/discussion.rs`):
  - A message without an @Name goes to the **owner** while `owner_answers` holds. That is: there is an owner, the ticket is not archived, and the column is Todo, Doing, PlanReview, Testing or Review.
  - An `@Name` wakes the employees it names.
  - Otherwise, the hint line "nobody answers" appears.
  - *Assumption:* "the lead" in D3 means this. The responsible board member decides; they do not answer as an employee.
- **BR2 Same conversation.** The ticket session reads and writes the ticket's existing conversation. Writing goes through `group_chat.send` with `channel`. There is no new conversation, no migration and no data loss.
- **BR3 Team chat without tickets.**
  - `channel.list` may still return ticket channels, but the Team chat list does not show them: no Tickets section, and no archived tickets under Archived.
  - Searching `#12` or `12` in the Team chat opens `/ticket/12`.
  - The open and close lines in General link to the ticket session.
- **BR4 Rights unchanged.** Accept, send back, approve or revise a plan, cancel, start, assign, set responsible and archive keep their RPC, their rights and their confirmations. Only where they appear changes.
- **BR5 Read-only.** When a ticket is Done or archived, the session is read-only: the composer and the gates go away, and the properties and the timeline stay readable. Leaving Done (send back or reopen) makes the session writable again.
- **BR6 Old links keep working.** `/chat?channel=<ticket channel>[&message=][&thread=]` and `/board?ticket=<id>` or `/history?ticket=<id>` redirect to `/ticket/:number` and keep `message` and `thread`.

## 4. User stories

### TS-01 Open a ticket as a session
**As** a board member, **I want** every ticket to open in one session screen, **so that** I do not have to switch between the board, the Team chat and History.

Acceptance criteria
1. There is a route `/ticket/:number` that shows the ticket session.
2. These open `/ticket/:number`:
   - a Board card (click and keyboard);
   - a palette result for a ticket or a ticket channel;
   - a ticket link (`links.tsx`);
   - an Inbox notification about a ticket;
   - "Open ticket" on a chat line, in the overview, in the task run, in ideas, in scheduled and in the employee inbox;
   - a Continue row in New session;
   - "Open full screen" in the chat dock.
3. After creating a ticket (the New ticket dialog, Give work, or New session in Ticket or Quick task mode), the app opens `/ticket/:number` of the new ticket.
4. A notification about a message opens the session scrolled to that message (`&message=`). A thread reply opens that thread.
5. Old links redirect according to BR6.

### TS-02 Talk in the ticket session
**As** a board member, **I want** to type in the ticket session as in a work session, **so that** the work continues without a detour.

Acceptance criteria
1. The input is the shared PromptBox (`ui/prompt-box.tsx`): text, photos, files and a folder, as in a work session.
2. The placeholder is "Message <owner> (owner)…". Without an owner, or outside the columns from BR1, it says who answers or that nobody answers (the same text as `ownerAnswers` today).
3. `@Name` and `@Everyone` work as they do in the ticket channel today.
4. Stop (`run.stop`) and Retry work as they do in the ticket channel.
5. Under the PromptBox are the run chips **Mode** and **Model** (`ticket.update {run}`), as in the board panel today.

### TS-03 Follow several employees in one timeline
**As** a board member, **I want** to see in one timeline who did what, **so that** I understand the progress without opening tasks.

Acceptance criteria
1. The timeline shows:
   - messages (with the author's name);
   - live runs (`LiveRow`), approvals (`ApprovalCard`) and threads;
   - system lines (delegated, reported, moved, plan, review, tested, done, summary, limit).
2. Consecutive update lines fold into "N earlier updates" (`foldUpdates`), as they do now.
3. A system line with a task has "Open task →". The link opens the task run.
4. The `ProgressCard` is directly above the input when the ticket is not read-only.

### TS-04 Decide where I read
**As** the responsible board member, **I want** the decision of the moment right above the input, **so that** I can decide while I read the conversation.

Acceptance criteria
1. Above the input is the gate of the current column:
   - **Plan review**: `PlanGate` (Approve, Approve and accept edits, Request changes, Reject).
   - **Review**: `ReviewGate` (report, suggested replies, Accept, Send back to Doing).
   - **Other columns**: `StatusBox`. That covers the login wait, failed (Start again or Back to Backlog), backlog (Start or Delete), todo (choose an employee), doing stopped (Try again), testing (Open test task), done (summary and retry) and the branch/merge state of a quick task.
2. Every action uses the same RPC and confirmation as the board panel (BR4).
3. After an action, the session stays open. The column chip, the gate and the timeline update without a reload.
4. "Open discussion" in `ReviewGate` is unnecessary in the session and goes away.

### TS-05 See and edit the ticket data
**As** a board member, **I want** the ticket data in the side panel, **so that** I can edit it without the Board.

Acceptance criteria
1. The head shows:
   - `#N` and the title, which you can edit (`ticket.update` with `expectedVersion`);
   - the column chip;
   - "owner X · responsible Y";
   - the project.
2. The head's More menu holds:
   - Move to <column>;
   - Follow or Unfollow;
   - Responsible (me, another member, or no one);
   - Copy link (a link to `/ticket/:number`).
3. The side panel "Ticket properties" (built from `run-side.tsx`: `RunAside`, `SideProperties`, `SideRow`, `SideSection`) shows:
   - priority, owner, mode and model, labels, deadline, project, design URL (only with that feature), responsible, runs on, created by, followers and files (Add files…, drop or paste);
   - the task tree (who works on what, with status, and a link to the task);
   - files written, guidelines in the runs and usage, summed over all of the ticket's runs;
   - the description (editable) and the feedback box;
   - an Activity tab.
4. You can close and reopen the side panel. The choice is remembered, as `OVERVIEW_KEY` is today.

### TS-06 Team chat without ticket channels
**As** a board member, **I want** the Team chat to show only the general and project channels, **so that** the list stays calm and a ticket lives in one place.

Acceptance criteria
1. The Team chat list (`chat-logic.ts` `sections()`) shows no open or archived ticket channels.
2. Search in the Team chat and in the palette still finds tickets by number and title. Choosing one opens `/ticket/:number`.
3. The chat dock's ticket list opens a compact ticket session, or links to `/ticket/:number` (see open point O2).

### TS-07 Read-only and reopening
**As** a board member, **I want** a closed ticket to stay readable, **so that** I can look up what happened.

Acceptance criteria
1. Done or archived shows the read-only bar ("#N is Done/archived · read-only.") instead of the input and the gate.
2. The properties and the timeline stay readable. Writes in the side panel follow what the board panel allows today on a Done ticket.
3. Leaving Done (for example, send back) makes the session writable again without a reload.

## 5. Edge cases

| Case | Expected |
|---|---|
| Server without `ticket_channels` (no conversation) | The session shows the head, the gate and the properties. In place of the timeline: "This server has no ticket conversation." The input is hidden. |
| Unknown or deleted number | "Ticket #N not found." with a link to the Board. |
| Ticket of another workspace or project | Opens anyway. The head shows the project. |
| Quick task ticket | Same screen. The `StatusBox` shows the branch/merge state, and accept and cancel keep their quick confirmations. |
| Ticket waits for a Claude login | The `StatusBox` shows "Connect Claude login" and `ticket.use_my_login`, as today. |
| Offline or reading failed | `NoConnectionNote` or the error with **Try again**, never endless "Loading…". |
| Many messages (2,000+) | The timeline stays virtualised, as in the channel today (perf e2e). |
| Two board members in the same ticket | Both see the same timeline live. A decision by one updates the other's gate. |

## 6. Server

No protocol change is expected. `ticket.get`, `channel.list`, `channel.search`, the
messages of the conversation and the `ticket.*` writes already exist.

To be decided by Titan: summed usage and files written per ticket. If the client cannot add
them up reliably from the task runs, an additive read follows (for example `ticket.usage`)
with a new feature in `FEATURES`. The client hides the block when the server lacks that
feature.

## 7. Out of scope (follow-ups)

- Mobile (`team/channel_screen.dart`, `board/ticket_page.dart`): a separate ticket after desktop is accepted.
- Tickets in the sidebar session list (D2).
- Deleting `TicketPanel` as code. It can stay as an internal component, but no route opens it as a panel any more.

## 8. Open points (not blocking; the proposal is the default)

| # | Question | Proposal |
|---|---|---|
| O1 | Does a Board card click go straight to the session, or first to a preview panel? | Straight to the session. |
| O2 | Should the chat dock (popout) keep its ticket list? | Yes, as a compact ticket session without a side panel. |

## 9. Verification once built

- vitest for `ticket-session-logic`:
  - the gate per column;
  - the redirect of old links (BR6);
  - `sections()` without ticket channels;
  - the placeholder per BR1 state.
- `tsc --noEmit` and `biome check` are green.
- Playwright e2e with the fake server:
  1. Creating a ticket opens `/ticket/N`.
  2. A message gets an answer from the owner.
  3. Review → Accept → the session is read-only.
  4. An old `/chat?channel=` link redirects, and so does `/board?ticket=`.
  5. The Team chat list has no ticket channels.
- Screenshots of the end result in `target/<ticket>/`.
