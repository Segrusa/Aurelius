# Cleanup: features and screens that go (2026-11-25)

Analysis by Nova at the request of the Owner. Decisions by the Owner on 2026-11-25:
- **Remove completely from the client code:** Ideas, Recurring tasks / Scheduled (including `/scheduled`), the `/tasks` list, and the small hidden flags (Trace, EmployeeTools, RunScratchpad, MemoryProposals, SequentialTasks, ChatAttachments, ActivityInSidebar).
- **Vault:** stays hidden. It is neither shown nor removed. Runs keep using `vault_list`/`vault_read`.
- **Conceptual overlap:** this goes to Pixel first for a design proposal (OPR-5). Nothing is built before the Owner chooses.
- **Dead code** (OPR-1..3): may go without any further decision.

The overall rules for every ticket below:
- Protocol major 5 stays: no method, field or event is removed from the wire. Server handlers that nobody uses answer with a "gone" error (like `TEAM_CHAT_GONE`). Server data and migrations stay untouched, and no migration drops data.
- No behaviour changes beyond what the ticket names. UI text is English and uses the same words on desktop and mobile.
- Each ticket gets its own worktree and branch (`fix/opr-N-…`) from main. Run only targeted tests. Merge and push only with the Owner's approval.

Execution order: OPR-1, OPR-2 and OPR-3 can run in parallel, because their file sets don't overlap. OPR-4 comes after OPR-1, because both touch `clients/ts/app`. OPR-5 is design work only.

---

## OPR-1 — Desktop: remove dead code and fix the Team chat links
**Owner role:** Frontend (Flux). **Files:** `clients/ts/app/src/**`.

| # | What | Where |
|---|---|---|
| 1 | Remove `TicketPanel`, `TicketDecision`, `Activity` and `Followers`: no code outside the file uses them. Also remove the `session === false` branch ("Open discussion" and the `discussion` read). | `screens/ticket-panel.tsx` (≈ line 383, 448–552, 1101, 1124–end) |
| 2 | Today `workHref` builds `/chat?channel=…` and `/talk/…`, and a click lands silently on `/new`. Instead, a channel row for a ticket opens `/ticket/N`, and a session row opens `/session/$id`. Rows that have no target are not clickable (the cursor and hover show it). | `screens/activity-bar-logic.ts` |
| 3 | Remove the General/project-channel code from the chat: `ChannelBody`, the "Project channel" text, the `"general"` draft key, and `group_chat.get` with `channel: null`. Keep what the ticket session (`TicketSessionBody`) uses. | `screens/chat.tsx`, `data.ts` |
| 4 | The bell rows for conversations that are not work sessions open something or are not shown. They never just close the dropdown without doing anything. | `screens/inbox-bell.tsx:181-186` |
| 5 | Fix labels that no longer match the screens. The keymap descriptions "Go to Ideas (Projects tab)" and "Go to Guidelines (Employees tab)" must name the right screen. `/projects` gets a title that matches its content (Workspaces). | `keys/keymap.ts`, `screens/projects.tsx` |
| 6 | Remove the redirect routes `/chat`, `/talk/$id` and `/inbox` only if nothing in the app builds them any more. Keep `/my-tickets` and `/activity`, because old links still use them. | `app.tsx` |

**Acceptance criteria**
- `rg "TicketPanel|ChannelBody|/talk/|\"/chat\\?"` in `clients/ts/app/src` finds nothing (tests excluded).
- A vitest for `workHref` proves that every row type gives either a `/ticket/`, `/session/` or `/tasks/` target, or `null`, and never `/chat` or `/talk`.
- `tsc`, biome and vitest are green. The Playwright specs for the ticket, board, bell and activity bar are adapted and have been run, with the output in the report.
- Report the LOC before and after.

## OPR-2 — Mobile: remove dead code and fix `/updates`
**Owner role:** Fullstack (Bridge). **Files:** `clients/mobile/lib/**`, except `lib/data/protocol`.

| # | What | Where |
|---|---|---|
| 1 | **Bug:** `context.push('/updates')` has no route, so the user gets the go_router error page. "Update the server" on the board's empty state and in link actions should open the existing hosts/update screen (`/hosts`). If no update screen fits, the button goes away. | `features/board/board_page.dart:289`, `features/links/link_actions.dart:57` |
| 2 | Remove `features/dashboard/` (the old Home, 458 LOC) and its tests. | `features/dashboard/**`, `test/**` |
| 3 | Remove the standalone mode of `BoardPage`: every `!inProject` branch, the project filter pill, `BoardProjectChosen`, and the `/projects` and `/updates` buttons. `inProject` either disappears or is always true. | `features/board/board_page.dart`, `board_bloc.dart` |
| 4 | Remove what is left of the Discussion segment. `ticket_actions.dart:221` (Send back), `TicketPlanPage`'s "Request changes in the discussion", `inboxPlace` and `openMessageAt` stop building `?tab=discussion`: they open `/ticket/:tid` (with `&message=` when there is a message). The router keeps accepting `?tab=discussion` from old links. | `features/board/ticket_actions.dart`, `ticket_page.dart`, inbox, links |
| 5 | Update comments and formatting about Team chat and General. `inbox_rows.dart` no longer shows `# general`. | `features/chat/mentions.dart`, `markdown/mention_markdown.dart`, `markdown/links.dart`, `everyone_audience.dart`, `features/inbox/inbox_rows.dart` |
| 6 | Remove the unused helpers for features the Owner removed: `createTaskSchedule`/`updateTaskSchedule` (`data/commands.dart`, `secret_write.dart`), `ideas()`, `draftFromIdea` and `ideaCanBecomeTicket`, as long as no screen uses them. | `lib/data/**` |
| 7 | Rename the account screen `SessionsPage` (where you are signed in) to `SignedInPage`, because it clashes with `WorkSessionsPage`. The UI title stays "Signed in on". | `features/account/account.dart`, router |

**Acceptance criteria**
- A new test reads every string literal in `context.push(` and `context.go(` under `lib/` and proves that the router resolves it to an existing route (so no error page). This test fails on the old `/updates`.
- `flutter analyze` reports no more than on main (baseline 6), and `flutter test` is green. Flutter is in `~/development/flutter/bin`.
- Report the LOC before and after.

## OPR-3 — Server: unused RPCs answer "gone", legacy tools removed
**Owner role:** Backend (Forge). **Files:** `crates/**`, `docs/product/fase-2`.

| # | What |
|---|---|
| 1 | No client calls `employee.chat`, `employee.chats`, `employee.open_chat`, `project_conversation.list`, `project_conversation.start`, `idea.start` or `markdown.list`. Their handlers answer with a "gone" error, following the pattern of `TEAM_CHAT_GONE`, and the logic behind them that nothing else uses is removed. The method names, `FEATURES`, the wire types and the generated bindings stay. `project_conversation.send` stays (mobile `chat_bloc.dart` uses it). |
| 2 | Check whether `channel.list` without `ticket_channels` always answers empty. If it does, add a test for that and report it, so OPR-1 can drop the call in `data.ts:393`. |
| 3 | Check whether the `aurelius-import` binary (`crates/storage`, `import.rs`, ≈800 LOC) still has a use. It was a one-time migration from v0.6 to v0.7, and nothing in the build references it. Remove it only if `fresh_start.rs` (the Fase 1 install) does not depend on it. If that is unclear, do nothing and report back. |
| 4 | Mark `docs/product/fase-2` as historical: one line at the top saying the Team chat, #general and the project channels were removed in migration 0045. |

**Acceptance criteria**
- For each method in #1, a test proves the gone answer, with a test name like `employee_chat_answers_gone`.
- The test that keeps `team_channels` and `employee_chats` out of `FEATURES` stays green.
- `cargo test -p daemon`, `-p application` and `-p storage`, run targeted on the touched modules, are green. `cargo xtask gen-dart --check` and gen-ts show no drift. Clippy reports no new warnings.

## OPR-4 — Desktop: remove the hidden features (Owner decision)
**Owner role:** Frontend (Flux). **Files:** `clients/ts/app/src/**`. **After OPR-1.**

Remove the client code completely, not just hide it: screens, routes, settings sections, shortcuts, palette entries, the keymap, data hooks and tests.

| Feature | What goes |
|---|---|
| Ideas | `screens/ideas.tsx`, the route `/knowledge/ideas`, the ideas tab in `projects.tsx`, and the UI for `ticket_from_idea`/`idea_attachments` |
| Recurring tasks / Scheduled | The `schedules` section in `settings-advanced.tsx`, `screens/scheduled.tsx`, the route `/scheduled`, and the "Scheduled messages" link on the employee page's Inbox tab |
| Tasks list | `screens/tasks.tsx` and the route `/tasks`, plus Ctrl+7. **Stays:** `/tasks/$taskId` (task run). Its back link goes to the employee page or the ticket the task belongs to, and switching workspace falls back to `/overview` or `/new` instead of `/tasks`. |
| Trace | The `trace` section in settings-advanced and the trace UI on runs |
| EmployeeTools, RunScratchpad | `employee-tools.tsx` and the scratchpad part of the employee page |
| MemoryProposals | The proposals part of `memory.tsx` |
| SequentialTasks, ChatAttachments, ActivityInSidebar | The code behind the flag |
| `hidden.ts` | Afterwards only `Vault` is left. The type and the list keep that one entry. |

**Business rules**
- Vault stays in the code and stays hidden (Owner decision).
- On the server, nothing is removed for these features: no migration, protocol or handler changes. Scheduled tasks that already exist keep running. **Risk:** the Owner can then no longer see, pause or delete them in the app. Before removal, Flux reports how many active recurring tasks are on the dev server, and the Owner is told. Pausing them is out of scope for this ticket.
- Old URLs (`/scheduled`, `/tasks`, `/knowledge/ideas`) redirect to a sensible screen (`/new` or `/knowledge/guidelines`) and never to an error page.

**Acceptance criteria**
- `rg "\"(Ideas|RecurringTasks|ScheduledInSidebar|TasksInSidebar|Trace|EmployeeTools|RunScratchpad|MemoryProposals|SequentialTasks|ChatAttachments|ActivityInSidebar)\"" clients/ts/app/src` finds nothing.
- `tsc`, biome and vitest are green. The Playwright specs that touch these screens are removed or adapted, and have been run.
- Report the LOC before and after.

## OPR-5 — Design: proposal for overlapping concepts (Pixel)
**Owner role:** UI/UX (Pixel). Design only: no code. The Owner chooses per point afterwards.

Deliver one design (`design/opruimen-overlap.html` or a set of screens) with a recommendation for each point:
1. **Working with an employee in four ways** (session, ticket, task run, quick task; on desktop, sessions and task runs share `EmployeeChat`). Can a task run become a read-only view of a session?
2. **Seven places for "what needs attention"**: the bell, toasts, the activity bar, Overview › Needs you/Running now, Settings › Activity log, the employee Inbox tab (actually the employee's queue), and the ticket's Activity/Timeline. Which places stay, and what does each one do?
3. **Overview vs Board vs History**: Overview has its own board with 5 lanes and a "Recently done" list. Counts plus links instead?
4. **Three lists of workspaces**: All workspaces, `/projects` and the rail. Merge them into one?
5. **Model choice in three places**: Company default model (old servers), Workspace › Model & effort (now a stub) and My defaults. Desktop and mobile both.

**Acceptance criteria:** for each point, the current situation (with screenshots), a proposal, what disappears, and the impact on mobile. Desktop and mobile use the same words.
