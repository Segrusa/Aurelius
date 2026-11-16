# Desktop app changelog

The desktop app (`aurelius.exe`), released as `desktop-v<version>`.
`scripts/release.ps1 -Product desktop` takes a release's notes from its
section here; they become the GitHub release text and appear under "What's
new" for the app in Settings → App.

The server and the desktop app are released separately. They work together
as long as their protocol major matches. Releases up to v0.6.0 were combined:
see [CHANGELOG.md](../../CHANGELOG.md).

## v0.9.3

- **Ideas become tickets.** Right-click an idea and choose "Create ticket…" (Ctrl+T): the New ticket form opens filled in from the idea, its files go along, and the idea is marked done with a link to the ticket. "Start as a task" is gone. Needs a server with `ticket_from_idea`.
- **No more New task.** The screens no longer create tasks: "New task" became "New ticket", and Give work on the Dashboard now makes a started ticket for the employee you chose. Recurring tasks stay. On mobile, a task dictated in CarPlay becomes a ticket in Backlog.
- **Notifications instead of the Inbox.** The Inbox page is gone. A bell next to "Session" (Ctrl+I) opens a popout with what waits for you first and what happened after it; a click leads to the channel, ticket or conversation where the question was asked. Something new shows for a moment as a toast at the top right: hovering keeps it, a click opens it. A proposal for a colleague or guideline opens in a dialog.
- **Mode in the Team chat.** An employee's answer shows the mode it was given in (for example "◇ Plan") next to its name, while it streams and once it is done (mobile: "Plan mode" in the answer's header). Needs a server that sends it.
- **Team chat.** One group chat with all your employees, right under Dashboard (mobile: the Team tab). `@Name` asks those employees; without a name the CEO answers. Answers stream in live with their author, and you can stop or approve them in place. An employee's page no longer has its own chat: it shows the employee's settings and tasks, and task conversations stay writable. Needs a server with `group_chat.get`.
- **Linux install script.** `scripts/install-linux.sh` installs Aurelius on Linux.

**A cleaner Aurelius (wave 1).** This release only removes things from the screens. Nothing is deleted on the server: all data (notes, shares, artifacts, timelines, project conversations, project plugins, functions) stays stored and keeps working, and the features themselves stay. Ideas are untouched. Nothing is migrated yet; server clean-up follows in wave 2.

- **Navigation.** The sidebar has 5 items plus a cog and at most 5 pinned projects under Projects. Employees has tabs List / Org chart / Departments; Projects has tabs Projects / Ideas. A project page has 3 tabs. Memory has no page of its own: open it from the project menu (⋯ › Memory) or from the app menu / command palette (Memory…, with a project picker). "Add employee…" stays in the app menu and palette and opens the Employees dialog.
- **Settings.** One entry with 4 sections and an Advanced section of 8 pages, among them Plugins (project assignments stay visible and editable, in a Project column), MCP & tools (now including the Claude artifacts switch), App keys, Recurring schedules and Trace. Artifact links in chat still open.
- **Gone from the screens** (data and server stay): Notes, Agent functions, Shares, Artifacts lists, the Variants picker, Activity / Timeline / Running, the Documents list, Diff, Terminal, the Ctrl-shortcuts overlay, project conversations and Discuss in project, the Follow-org-chart switch, the Managers section and its card menu, and the project settings dialog (name, description and folder are under Edit…). Export and Open in new window stay.
- **Shortcuts removed:** Alt+1…8, Go Activity, Toggle Terminal, Diff, Show shortcuts / F1. Go Org chart, Departments and Ideas open the matching tab.
- **Flow.** The last chosen mode per employee is the default and shows as a chip in Give work. While an answer runs, Enter is Send now and Shift+Enter queues the message. Opening a task marks its result notification as read; only Needs you and Failed count on the badge.
- **Mobile.** 4 tabs; More holds Projects, Ideas, Company and Settings. Settings has 7 rows plus Advanced, where recurring tasks are a read-only list (create them on desktop).

- **Settings → Folders is gone**, with its F key. It listed manual folder grants, which were never used. Project and employee work folders are unaffected (choose them in the project or employee dialog). Needs no server change; a server from before this release still has the methods but the app no longer calls them.
- **App keys: audience and webhook.** Settings → MCP & tools → Apps (MCP): a new key can reach everyone or only the management (CEO, CTO, CMO level; no approve scope), and can post its events to a webhook URL. The webhook secret (`whsec_…`) is shown once, like the key secret. The list shows both. Needs a server with `management_channel` / `webhooks`.

## v0.8.2

Long messages fold, and the open chat does less work while an answer streams. Works with server 0.7.x and 0.8.x.

- **Long messages**: messages longer than 20 lines show their first 20 lines with Show more (N lines) / Show less, for your own messages, answers and task updates. Every line counts, blank ones too, and a code block cut in the middle is closed. An answer you watched stream stays unfolded.
- **Open chat**: while an answer streams in the open conversation, only that answer repaints; finished messages are reused instead of rebuilt on every update.
- **Memory**: long answers leave less layout memory behind.

## v0.8.1

Less CPU and memory while answers stream. Works with server 0.7.x and 0.8.x.

- **Streams elsewhere**: an answer streaming in a conversation you don't have open no longer repaints the whole window (about 3–4% of a core down to about 1%).
- **Fewer repaints**: the app only repaints when something actually changed, and the preview pane refetches messages only when a run starts or ends instead of every half second.
- **Animations**: pulsing dots and other endless animations no longer rebuild the open conversation on every frame.
- **Memory**: long answers leave less memory behind (about 32 MB down to about 22 MB per long answer).

## v0.8.0

App keys, idea attachments and a faster chat input. Works with server 0.7.x; app keys and idea attachments need server 0.8.0 and stay hidden on older servers.

- **App keys**: Settings shows the keys of external apps for `aurelius-mcp` (scopes, limits, revoke), and tasks show which app created them.
- **Ideas**: the note is a multi-line field, ideas take files and images, and an idea can be deleted. Its files carry over when it becomes a task.
- **Live questions**: the open chat reloads on `task.posted`, so questions, plans and approval requests appear at once.
- **Typing lag**: typing in a long conversation no longer rebuilds every message on each keystroke or streamed delta.

## v0.7.4

Project settings, links you can click and projects in their own window. Works with server 0.7.x; project-wide plugins and editing the description need server 0.7.4 and stay hidden on older servers.

- **Project settings** (Ctrl+Shift+, or the project menu): a General page to edit the project's description, plus the project-wide plugins (loaded by every manager in the project) and each manager's own extras. A manager's own plugin stays removable when the project also has it.
- **Links**: links in task reports, plan and run panels and web search results open in the browser; only http(s) and mailto links leave the app. Hovering a link shows its URL bottom-left, like a browser.
- **Project in its own window**: "Open in new window" from the project list menu, the project menu, the palette or Ctrl/middle-click. One window per project (opening it again focuses it), with its own title and remembered size and position on the display that holds it.
- **Narrow project header**: the project tabs scroll at narrow widths, so Documents, the folder button and ⋯ stay visible.

## v0.7.3

Notes, plugins per project, artifacts inside the app, terminal tabs and a live preview. Works with server 0.7.x; notes, project plugins, artifact snapshots, upload thumbnails and terminal tabs need server 0.7.3 and stay hidden on older servers.

- **Notes tab** per project, like Apple Notes: pinned notes first, with search.
- **Project plugins**: project menu Plugins…, projects in Add plugin and where a plugin is used in the list.
- **Artifacts in a side panel**: Markdown in the viewer, HTML in an isolated WebView2; claude.ai artifacts (Docs, Slides, Design, Dashboard) open live in an in-app claude.ai view with its own persistent profile (Settings → MCP & tools → claude.ai account).
- **Terminal**: tabs (several terminals per conversation, detached while hidden), mouse selection, word and line select and IME input.
- **Live preview pane** for managers and tasks.
- **Waiting**: a usage-limit wait reads 'Waiting for limit reset until HH:MM', never a failure; queued tasks that do not start now show In line or Scheduled (with their start) under Waiting, not Active.
- **Message from <manager>** for a message a manager sends to a task it delegated.
- Bare `.md` paths in plain text and links in your own message are clickable.
- Uploaded images show as thumbnails.
- Side panels give way in narrow windows and task detail buttons wrap; in the projects list the managers and counts shrink before the name.

## v0.7.2

Deleting a task's conversation works again. Works with server 0.7.x; the fix itself needs server 0.7.2.

- **Delete conversation.** Deleting a task's conversation from a manager's panel could close the dialog without deleting anything when an answer in it had not finished; with server 0.7.2 it is deleted and the running answer is stopped. The dialog now names the task and says a running answer gets stopped.

## v0.7.1

MCP login, Always allow for MCP tools, Claude Code artifacts, a new Settings layout and the projects as a list. Works with server 0.7.0; the MCP login, Always allow and artifacts need server 0.7.1 and stay hidden on older servers.

- **MCP login.** Settings → MCP & tools shows per remote server whether it is Connected, Needs login or Unknown, with Log in and Log out (Approve right). Log in opens the browser; on the server's own computer it finishes by itself, from another computer you paste the address the browser ended on. Open again and Cancel while it waits. Hidden on servers without MCP login.
- **Settings split.** App & updates is now three pages: MCP & tools (Agents: the browser for Claude in Chrome, MCP servers with Add and Remove, Always allowed tools), Server & PC (Server: the server's version and updates, Restart server, Shut down this PC) and App (preferences, appearance and this app's update). Each page shows only its own shortcut hints and keys; Tab goes through all pages. Ctrl+Shift+U opens Server & PC when only the server has an update, App otherwise.
- **Always allow for MCP tools.** An approval of an MCP tool has Always allow next to Reject and Approve, in the chat, on the ASKED card and in the inbox (Approve right). The tool then no longer asks, for any manager, and its other open asks disappear. Settings → MCP & tools lists the always-allowed tools with Remove.
- **Claude Code artifacts.** With Claude artifacts on (Settings → MCP & tools, Approve right), managers can publish pages to your claude.ai account. A published artifact shows as a card in the conversation with its link, and the Artifacts chip and the task panel's ARTIFACTS section list them. Publishing asks first in Ask and Accept edits.
- **Projects as a list.** The projects overview is a list of rows instead of cards: name, state (Paused, Needs you, Working, Quiet), managers and counts. At work now stays.

## v0.7.0

Protocol 2.0, released together with server 0.7.0 and the mobile app. It needs a 0.7.0 server and does not work with older servers (the server must be updated first).

- **Org chart.** Edit manager has a title and Reports to (Alt+P opens the picker; a choice that would make a cycle shows an error and is not saved). The org chart view shows the managers as a tree. The project setting Follow the org chart (Alt+G) limits delegating to the own subtree, limits handing over to the boss or peers and sends a failed or stopped task to the boss first; it is off by default.
- **Colleague proposals.** When a manager proposes a colleague, the proposal appears in the inbox between approvals and results; you accept or reject it.
- **Quota auto-resume.** A task stopped by a Claude usage limit shows as Waiting and continues by itself after the reset; nothing to do.
- **Skills from GitHub.** Settings → Agents → Plugins accepts a link to a repo or folder of standalone skills (like anthropics/skills) and installs the ones you pick.
- **Removed.** Call (voice), task diff, merge and workspace, plan feedback, per-task repeat (recurring tasks stay) and the plain manager chat.

## v0.6.13

- **Add and remove MCP servers.** Settings → Connections has Add MCP server (stdio, http or sse; secret values stay masked and are never shown back) and Remove with confirmation per server. Needs the owner right and server 0.6.13 (protocol 1.23); hidden on older servers.

## v0.6.12

- **Plugins for managers.** Settings → Agents → Plugins lists the Claude Code plugins on the server. Add plugin takes a link (or Browse), shows the plugins in it and installs the ones you pick; update and remove from the same page. Edit manager has a toggle per plugin (none by default); a plugin that runs hooks is flagged. Needs server 0.6.12 (protocol 1.22).

## v0.6.11

- **MCP servers in Settings.** Settings → Connections lists the MCP servers you already have in Claude Code (user scope) with an on/off switch each; the enabled ones are used by every run of every manager. Needs server 0.6.11 (protocol 1.20).
- **Settings redesign.** The Settings dialog is taller and sits higher, with navigation grouped into Agents, Server and App, an accent bar on the active section, a one-line description under each title, and the server and app version in the navigation. Tab follows the visual order.
- **Agent functions.** A Functions tab per project (Alt+8): functions with usage today and this week against their limits, enable switch, create/edit/delete, recent calls that open their task, and a kill switch for all functions. Settings → API keys makes keys for your own apps (the secret and a curl example are shown once), changes the functions a key may call and revokes keys. Needs the owner right and a server with agent functions (protocol 1.20 or newer).

## v0.6.9

A Markdown viewer, a Conversations tab per project, clear all results, editing repeating tasks and copy buttons (needs a server with protocol 1.18 for the newest features; older servers keep working).

- **Markdown viewer.** A side panel lists a project's Markdown documents and shows one rendered, with Copy raw, Reload and relative `.md` links. Open it from the Documents button in the project bar or a task, the docs chip above the composer, `.md` links and "View" chips under messages, or the chips of uploaded `.md` files. Needs protocol 1.16.
- **Conversations tab per project.** Each project has a Conversations tab (Alt+7) with its own conversations, answered by an assistant of the project that can hand work to managers and add ideas. Managers now have tasks only; their earlier conversations show read-only, labelled with the manager. Needs protocol 1.18.
- **Clear all results.** The inbox's Results list has a confirmed "Clear all" (approvals stay). Older servers dismiss the results one by one.
- **Edit a repeating task.** "Edit" on a repeating task converts it into a recurring task and opens the recurring editor (Alt+R); a new task's Repeat now creates a recurring task. Needs protocol 1.15.
- **Copy buttons.** Every message, tool output and approval preview has a copy button that copies the raw text, with brief "Copied" feedback.
- **Scheduled is not Working.** Recurring and repeating tasks waiting for their next run show as Scheduled, in their own group of the thread panel, instead of Active or Working.

## v0.6.8

Files and a multi-line goal with a new task, recurring tasks, and an opaque Projects dropdown (needs a server with protocol 1.14 for the task features).

- **New task dialog.** The goal is a multi-line field (Ctrl+Enter creates the task) and keeps its line breaks. Attach files with Ctrl+O, by pasting into the goal or by dropping them on the dialog; they show as chips you can remove. Model, effort and mode can be set per task or left at the manager's default.
- **Recurring tasks.** The Tasks tab has a Recurring filter listing the project's schedules, with a detail panel to edit, pause or resume, run now and delete. A schedule repeats every N hours, daily or on chosen weekdays, in the server's local time. Older servers show that they are too old for it.
- **Projects dropdown.** The dropdown in the top bar is now an opaque popover, with a pin icon to pin or unpin a project.

## v0.6.7

Shut down the server's PC, pinned projects, a livelier home and a clearer Memory tab.

- **Shut down the server's PC.** A command in the palette and a banner with a countdown and Cancel: shut down now (60 s countdown) or when all work is done. Needs a server with protocol 1.12 and the owner right.
- **Pinned projects in the top bar.** Pin a project to keep it as a tab after "Projects"; the chevron next to "Projects" lists all projects with a pin toggle. Pins are kept per server on this computer.
- **Home shows who is at work.** An "At work now" panel lists the managers working across all projects (click to open), and the project cards show live state (Paused, Needs you, Working, Quiet), their managers and what waits for you.
- **Memory tab: In context.** The project Memory tab shows per manager what goes into the context (always, only when relevant, does not fit the budget, with tokens and a budget bar), next to the proposals. Needs a server with protocol 1.13.

## v0.6.6

A model and effort for delegated tasks and subagents, and the time in the status bar.

- **Delegated tasks and subagents on their own model.** Settings → Connections has "Delegation defaults": Delegated tasks (Alt+D model, Alt+Shift+D effort) and Subagents (Alt+U model), saved at once. Edit manager has the same rows for that manager ("Tasks given to this manager", "Subagents of this manager"), saved with the dialog, each with a "Uses …" line showing what the next run takes and where it comes from. Inherit keeps today's behaviour. Subagents follow the manager's effort, as Claude Code takes no effort of their own. This needs a server with protocol 1.11; older servers show the Subagents rows disabled with an explanation.
- **Time in the status bar.** The current time (HH:MM) shows at the right of the status bar.
- **Settings hints wrap.** The shortcut hints at the bottom of Settings → App & updates wrap on narrow windows instead of being cut off.

## v0.6.5

A much better folder browser, waiting messages you can delete or send now, and load limits with a "Server busy" banner.

- **A much better folder browser.** The in-app browser now opens for this computer too ("System dialog…" stays as a fallback, Ctrl+O). It has folder icons, a clickable breadcrumb bar, Ctrl+L to type or paste a path, type-to-filter (Backspace shortens it, Esc clears it) and a quick-access column with the starting points and the folders your projects and managers already use. Single click selects, double click or Enter opens, Backspace goes up; the footer shows which folder "Use this folder" takes. Errors show inside the browser instead of as toasts.
- **New folder.** The folder browser has a New folder button (Ctrl+Shift+N): type a name, the folder is created on the server and the browser opens into it. This needs a server with protocol 1.10.
- **Delete or send now a waiting message.** A message you send while an answer is still running is shown muted with "Waiting · sends after the current answer" and two buttons: Send now (stops the running answer and answers this one) and Delete. This works in chats, manager and task conversations and needs a server with protocol 1.9.
- **Load limits and "Server busy".** Settings → Load limits shows the server's CPU, free memory and runs (running and waiting) live, and lets you change when new runs wait: max runs at once (0 holds every new run), hold from CPU %, release below CPU %, minimum free memory and whether CPU and memory count at all. The app checks that CPU low is below CPU high (at most 100) before saving (Ctrl+S); the limits apply at once. A conversation whose answer has not started while the server holds runs shows why, live: "Server busy: CPU 82%, waiting", "Server busy: 1.4 GB free, waiting", "Server busy: 3 runs active, waiting" or "Runs paused on the server". This needs a server with protocol 1.10; older servers get "Update the server to use load limits."

## v0.6.4

A model and effort per manager, tasks one at a time, a clearer thread panel and ideas split by who noted them.

- **A default model and effort per manager.** Edit manager (on the manager's screen) has Model and Effort dropdowns (Alt+L, Alt+E). Pick a lighter model or a lower effort for a manager to use less; Inherit follows the project or global default again. Below them, "Runs on" shows what the manager's next run uses and where each part comes from, for example "Sonnet 5.5 · Medium · manager". Saved with the dialog.
- **Run tasks one at a time.** Edit manager has a "Run tasks one at a time" toggle (Alt+T). When on, the manager works on one task at a time and its other tasks wait in line. This needs a server with protocol 1.8.
- **Active and past threads.** In the manager thread panel each section (Tasks, Conversations) shows what is going on first, then a "Past" divider with today's ended threads dimmed; older ones stay under "Earlier · N". The section header shows how many are active.
- **Ideas from you and from managers.** The Ideas tab and the overview card group ideas under "From you" and "From managers", each with a count. A manager's idea has a coloured bar and "Idea from <name>". The author filter still works.
- **Choose the mode when starting an idea.** "Start as a task" on an idea has a mode section (Manager's default, Plan, Manual, Accept edits, Auto) above the managers; the task conversation starts in that mode. This needs a server with protocol 1.8.

## v0.6.3

Send files to managers, see usage per hour, restart the server, and see at a glance which tasks are done.

- **Restart the server.** Settings → App & updates has a Restart server button (key e). After a confirmation the server restarts; answers that are running are interrupted and the app reconnects by itself. This needs a server with protocol 1.7; an older server gets a message to update it first.
- **Send files to a manager.** In a manager or task conversation the File chip, Ctrl+O or drag & drop adds files of any type to a message (up to 8, 25 MB each). They are copied into the work folder under `.aurelius/uploads/` and show as chips on the message. A plain chat keeps attaching text files as before. This needs a server with protocol 1.7.
- **Usage per hour.** The days in the 14-day usage chart can be selected (today by default); below it a Per hour section shows 24 bars with input and output per hour, the day's total and buttons for the previous and next day. This needs a server with protocol 1.7.
- **Done marks on tasks.** Tasks show a clear mark: a green check with Done, Failed, Cancelled, Needs you or Working. In the manager thread panel, the task list and detail, and the header of a task conversation.
- **Older threads fold away.** The manager thread panel shows only tasks and conversations that are ongoing or changed today; the rest folds under an "Earlier · N" row.
- **Diff of a task's own branch.** View diff on a task with its own branch or folder shows the task's whole change against where it started. This needs a server with protocol 1.7.
- **Who noted an idea.** The Ideas tab shows which manager noted an idea, with a monogram, and filters on Everyone, Yours or Managers' (key 4).
- **Subagent chip follows live.** The subagent chip and the managers overview update from the running answer itself, at most once a second, instead of polling every 3 seconds.
- **A plain chat has no Git tools.** A chat without a manager no longer shows the branch chip, the diff panel or the terminal; switching to a chat closes an open diff or terminal.

## v0.6.2

Approvals say what they would do, and memory can be consolidated by hand.

- **Approvals show what they would do.** In the chat, on the Asked card and in the inbox an approval now shows the command with its description, the file with a preview or a diff of the change, or the URL or search query, instead of raw JSON. Paths show relative to the work folder; long content folds to 10 lines with "Show all". The raw request stays under "Details".
- **Consolidate memory.** A Consolidate button on a project's Memory tab merges the project's memory and that of its managers into short points right away. Merged memories move to History. This needs a server with protocol 1.6.
- **Subagent chip per conversation.** The chip shows the subagents of the conversation that is open, also in a plain chat, and no longer those of a manager's task conversation. The branch chip and the terminal button stay with manager conversations. Against a server older than 1.6 the chip works as before, for manager conversations only.
- **Dialogs no longer collapse.** Dialogs such as New idea show their content again instead of only their buttons, and scroll when they are long. The plan review buttons no longer stick out of a narrow window.

## v0.6.0

The first separate app release, built from the same code as the combined
v0.6.0.

- **Own releases.** The app has its own version and its own update button in
  Settings → App & updates (key i; key u updates the server). An app update
  leaves the server and running work alone. The status bar shows both
  versions and only turns red when the protocol does not match.
- **Built-in terminal.** Ctrl+` or the >_ button next to the branch chip opens
  a shell in the manager's work folder.
- **Subagents in sight.** Next to the branch chip: how many subagents are
  running; a click shows what each one is doing.
- **Diff panel.** A click on the branch chip, or "View diff" on a task, opens
  the changes of the working folder in a side panel.
- Icon buttons show their name and shortcut on hover.
