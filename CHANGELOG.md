# Changelog

Releases up to v0.6.0 combined the app and the server. From then on they are
released separately, each with its own version and changelog:
[server](docs/changelog/server.md) and [desktop app](docs/changelog/desktop.md).

## Unreleased (feat/astryx)

- **Employees as bots.** Employees get platform rules at the front of their prompt (handoffs, a thread's lead, self-contained handoffs, new messages marked, signing on external systems), a description colleagues see, and in a thread they do not lead only what is addressed to them in their prompt; the clients keep showing the whole thread. Memory no longer takes facts of one ticket. Details in [docs/changelog/server.md](docs/changelog/server.md).
- **Provider, model, effort and access per employee.** The employee dialog chooses the provider, model and effort of an employee, each with Inherit as the default and what that comes to shown beside it. A new access level limits what its runs can do: Inherit keeps every work tool, Read only reads and searches, Read and edit also changes files; without commands, the web, MCP servers or plugins. Changing the level starts a new session.
- **Run cards, interrupt cards and the odometer.** Under an employee's post and a "reported" line in a ticket's discussion and the Team chat, a run card folds out what the run did: its state (pulsing while it runs, with Cancel), the tool calls, failures and tokens, and opened every call with its result. A question or approval now also waits in the discussion where the work is talked about, with the same card as in the Inbox; a post that ends a ticket's main task on a question gets a card that answers it at once. The ticket's head and the channel's head show what the thread used so far (calls, tokens in and out, cost), and the Usage screen shows it per board member. Details in [docs/changelog/server.md](docs/changelog/server.md).
- **A mailbox per employee.** Everything that wakes an employee now lands in its mailbox: a message, a mention, a task, a heartbeat, a colleague's handoff. An employee does one run at a time, also across tickets and tasks; a run that waits for your approval leaves it free for other threads. What piles up in one thread becomes one run that reads it all and answers once. A question to a board member on a ticket parks that thread: colleagues' messages wait until the board member answers, and then the work goes on. Each ticket and channel has its own conversation for each employee, with its own session, so a mention on a ticket no longer lands in the main chat. Queued, Send now and delete work on what waits in the mailbox. Quota and login waits stay per board member.
- **Handoffs and hops.** An employee hands work over with a new `hand_off` tool: it shows in the discussion as an ordinary message ("Flux handed over to Bo", with what was asked) and wakes that colleague. A name with @ in what an employee writes now only refers to someone; it wakes nobody. A reply without a handoff ends the conversation. Your own @Name still wakes the employee, and an employee's @username of a board member still notifies them. The count of employees passing work on goes through handoffs and delegations too, up to 8 on a ticket and 3 in a channel; a board member writing resets it. A handoff from an employee with unread messages in that thread waits ("Handoff held"), and is delivered unchanged once it has read them. Details in [docs/changelog/server.md](docs/changelog/server.md).
- **Talk with an employee.** Every board member now has one lasting chat with each employee: Chat on the Employees screen, and the latest four in the sidebar with a dot when something is unread. Only you and that employee read it, and it runs on your own Claude login. The employee talks with you, makes ordinary tickets once the work is concrete and links them in its reply. When such a ticket reaches Review, it tells you so there without being asked. From a ticket, an employee can send you a message in that chat with a link to the ticket; it shows as unread and gives no notification. Details in [docs/changelog/server.md](docs/changelog/server.md).
- **Scheduled messages, a scratchpad and who waits.** An employee can leave itself or a colleague a message for later with a new `schedule_message` tool (from a minute to 30 days, on the login of the run that planned it, never on another one); it comes back through the mailbox and counts as a hop. A new Scheduled page shows what is planned and what came of it, with Cancel, next to the recurring tasks. A ticket and a chat say under their title who still waits to pick the thread up and what is scheduled for it. Every employee has a scratchpad, its own note across threads, always in its prompt; you read it on the run card and the employee page.
- **Tools per employee and start profiles.** In the employee dialog you choose which tools an employee has, per tool whether it always asks a board member first, and which of the company's MCP servers it may use. A tool that asks first does so in every mode, Auto too; the list narrows what the access level gives. Adding an employee can start from a profile (Chief of staff, Engineer, Reviewer, QA, Frontend designer) that fills in the role, instructions and tools; you change what you like. Details in [docs/changelog/server.md](docs/changelog/server.md).
- **An activity log and a page per employee.** A new Activity log page shows everything that happened to the work of employees, step by step: what reached a mailbox, how it waited, what a run did. You narrow it per employee, thread or run, or search `#21`. Message text is not in it, and someone else's private chat stays out. The employee page now has tabs: Inbox with what is running and what waits and why, Memory with its scratchpad and what it remembers, and Settings. The employee list shows what each employee is doing: Working, Waiting for you, Queued or Available. Details in [docs/changelog/server.md](docs/changelog/server.md).

## v0.6.0

The server and the desktop app now have their own releases, a manager
conversation has a built-in terminal, and running subagents are visible. This
release also contains everything prepared as v0.5.1, which was never published
on its own.

- **Separate releases.** The background server and the desktop app each have
  their own version and are updated separately in Settings → App & updates. A
  server update no longer restarts the app, and an app update leaves running
  work alone. They only need to speak the same protocol; the status bar shows
  both versions. This is the last combined release.
- **Built-in terminal.** Ctrl+` or the >_ button next to the branch chip opens
  a shell in the manager's work folder, on the server's computer.
- **Subagents in sight.** Next to the branch chip: how many subagents are
  running; a click shows what each one is doing. Subagents now run in the
  foreground, so an answer no longer stalls while one is still busy.
- **Diff panel.** A click on the branch chip, or "View diff" on a task, opens
  the changes of the working folder in a side panel.

From v0.5.1: managers know their project, and switching mode or sending now
takes effect right away.

- **Managers see the project.** In a plain conversation a manager gets an
  overview of the project's open and recent tasks, can read a task's report,
  and can look back at its own earlier conversations.
- **Mode switches reach a running answer.** Switching to Ask, Accept edits,
  Plan or Auto applies to the answer that is running, not just the next one.
- **Send now really interrupts.** The server stops the conversation's running
  answer itself, also from a remote computer; stopping an answer withdraws its
  open approvals.
- **Branch chip.** Above the composer of a manager conversation: the branch
  and its changed lines; a click copies the branch. On the phone a tap opens
  the diff.
- **Tooltips.** Icon buttons show their name and shortcut on hover; the
  project folder is an icon button.
- Stopping an answer also ends the programs it started; an answer that can't
  start no longer blocks its conversation. A new client can open Settings →
  Servers to pair without a server. macOS install, update and uninstall
  scripts.
## v0.5.0

Ideas per project, tasks that repeat, and a project overview at a glance.

- **Ideas.** Every project has an Ideas tab (Alt+6). Note an idea with Ctrl+N;
  managers note ideas there too while they work. When its time comes, start it
  as a task for a manager (Ctrl+T), edit it (F2) or drop it (Delete). Also on
  the phone.
- **Tasks that repeat.** A task can run hourly, daily or weekly, or start at a
  later moment. Choose it in the new-task dialog, or press Alt+R on a task to
  cycle off → hourly → daily → weekly.
- **Choose how a task runs.** When you make a task, pick the mode, connection,
  model and effort its conversation runs with; a conversation can also run on
  its own model, light for a quick question, heavy for planning.
- **The project overview at a glance.** Managers in a compact list, with the
  open tasks, the latest of the timeline and the open ideas beside them, each
  in a card that opens its tab. On the phone too.
- **Images in messages.** Paste, pick or photograph an image and send it with
  your message; it is scaled down to what a model reads well.
- **Wider or narrower side panels.** Drag the inner edge of a side panel; a
  double-click resets it. The width is remembered.
- Smaller project cards on the home page (four per row), without the activity
  list; plans in a conversation fold away with Alt+Shift+P.
## v0.4.0

Managers see your screen, share work with each other, and you see what runs.

- **Screenshots.** In a manager's work, the agent can look at your screen: the
  whole monitor or one window. Each screenshot is first asked in your inbox;
  turn on "Screen" for a manager (Alt+S) to let it look without asking. In the
  answer, "View" opens what it saw in a side panel (on the phone: full screen).
  Screenshots are taken on the PC that runs the server.
- **What runs now.** Click "N working" at the top (or Alt+W) for everything
  running: answers and tasks, how long they run and how many tokens they wrote.
  Click a row (or Enter) to follow it in a side panel: its tools and text as
  they come, with Stop and "Open conversation". While an answer runs, the chat
  shows "Thinking… 2m 26s · 3.9k tokens", on the phone too.
- **Plans you approve.** In plan mode the manager hands in its plan; approve
  it, ask for changes, or reject it, in the inbox or on the phone.
- **Your own browser, if you allow it.** A manager may use your Chrome (Claude
  in Chrome) once you turn "Browser" on (Alt+B). Every browser action is still
  shown to you first, also in Auto mode.
- **Managers work together.** Give a task to a manager (Alt+M in the new-task
  dialog, Alt+A in the task list). A manager can answer a task itself, hand it
  over to a better-suited colleague, or delegate parts of it and follow them
  up: it approves their plan and decides when they get stuck. You only hear
  about it when they really need you.
- **A conversation per task.** When a manager takes a task on, a conversation
  with that manager starts with the task as its context (Alt+C opens it). The
  plan, the findings of workers, subagents and colleagues, and the outcome
  come back there, kept short; the full results stay in the task. Talk to the
  manager there about the task.
- **Approvals in the task conversation.** When a worker needs your permission
  or an answer, it asks in the task's conversation too, and you decide there.
- **Files open in the app.** Files an agent names (screenshots, reports,
  exports in the project's folders) are kept with the message: images open in
  the app, other files with your own program; on the phone, images and text.
- **See who is at work.** A manager's page shows what runs for it now: its own
  steps, workers, subagents and work delegated to colleagues.
- **Managers remember.** A manager notes and recalls what it learns about your
  work. It never overrules what you decided: a contradiction becomes a
  proposal for you.
- **Switching mode mid-answer** now reaches the running answer: what the new
  mode allows goes ahead, also while it waits in your inbox.
- Visible scrollbars, layout fixes for long text, and a new app icon on the
  phone.

## v0.3.1

Tasks do real work; update the server from your phone.

- **Task workers really run on your connection.** Choosing a connection used
  to change only chat and managers: a task was planned by Claude Code, but its
  workers and the review ran on the built-in test runtime, so a task
  "succeeded" without real work. "Use this connection" now sets everything
  (chat, managers, workers, reviews, memory). On the first start of this
  version, workers, reviews and memory still on the test runtime move to your
  manager's connection and model, once; a later choice of your own stays.
- **Memory proposals** come from your connection too, no longer from the test
  runtime.
- **Update the server from any device.** On the phone: More → Updates shows
  the PC's version, the latest release and what's new; "Install and restart"
  downloads, verifies and installs it, then waits until the PC is back. Only
  releases signed with the Aurelius key are ever installed.
- **The window on the PC** tells you when another device updated the service,
  so you restart it to update the window too.

## v0.3.0

One server, every app a client of it.

- **Use Aurelius from another computer.** Your PC stays the server: projects,
  memory, keys and runtimes stay there. Aurelius on another computer (a second
  Windows PC, or a Mac) connects to it over Tailscale and works like a window
  on the PC itself. Pair it in Settings → Servers (Ctrl+Alt+S): on the server
  open Remote access (Ctrl+Alt+R), press N, then Y to copy the pairing text;
  paste it on the other computer and choose "Allow as owner" on the server.
- **Full access, only when you say so.** A paired computer may also manage the
  server: connections and API keys, folders, backups, updates and remote
  access. It only gets that when it asks for it and you confirm "Allow as
  owner"; a phone never does. Shutting the server down stays on the PC.
- **The server's folders, not yours.** From another computer, choosing a
  project folder or granting a folder opens a browser of the server's folders
  (arrows, Enter, Backspace, Ctrl+Enter to choose). Attachments are uploaded
  to the server first; Claude Code is found on the server.
- **Always clear where you are.** "on <server>" at the top and the server's
  name in the status bar. Removed on the server? The app stops and shows
  Settings → Servers instead of retrying.
- **A server without the app.** `scripts\install-production.ps1 -Role server`
  installs only the background service (starts at sign-in, remote access on);
  `-Role client` only the app. After an update or a restore the service now
  starts itself again.
- **Aurelius on the Mac** as a client of your PC: `scripts/mac-client.sh`
  (needs Xcode and Rust). Pairings are kept in the Keychain.
- **For developers:** the desktop app moved to `clients/desktop`, next to
  `clients/mobile`. Protocol 1.3; database migration 14 (a backup is made
  first). `aurelius-ctl server pair|list|forget`, `--server <name>`,
  `autostart on|off`, `remote allow --owner`.

## v0.2.4

Tasks that do the work, safely apart.

- **Workers really work.** A task's workers now change files, run commands
  and search the web through your own Claude Code, instead of only writing a
  report. What the manager's mode doesn't allow by itself is asked in the
  Inbox ("Alex · Worker A wants to run a command"); the task shows
  "Waiting for you in the Inbox" meanwhile.
- **Always isolated.** In a Git project every task gets its own branch, and
  every worker its own worktree on a branch of its own. Finished work is
  committed and merged into the task branch; your checkout is untouched.
  Workers whose changes clash redo their part on top of the merged work.
- **You merge.** A finished task shows its branch, size and commits. Merge
  into the branch it started from with the button or Ctrl+Shift+I (your
  checkout must be on that branch without uncommitted changes), or merge it
  yourself with `git merge`. The branch stays until you delete it.
- **Claude can ask you.** When a manager or worker needs a decision, its
  question appears in the Inbox with its options: choose with 1–4, Tab to the
  next question, T for your own answer, Ctrl+Enter to send, Ctrl+Backspace to
  skip (Claude then continues on its own judgment). The chat and the task show
  that they are waiting for your answer.
- **The manager looks, doesn't touch.** Planning and review can read the
  project and the changes (with the diff), but change nothing.
- **On the phone too.** The mobile app shows a task's branch, changes and
  commits, merges it, says when workers wait for you, and answers Claude's
  questions with the same options.
- **More on the phone.** Choose the model and effort for chats and managers
  (never the connection or persona), several conversations per manager,
  commands shown in place in answers, wider dialogs, and the keyboard hides
  when you scroll the chat.
- **Without Git** workers work in a separate folder per task; the project
  folder stays unchanged.
- **Clearer controls.** Drawn back and forward arrows; window controls and
  icon buttons in full contrast.
- **Fixes.** Cancelling a task stops its running workers. Task reports are
  no longer written into a Git repository as untracked files.

## v0.2.3

Managers that act, and easier navigation.

- **Managers use tools.** In their project folder (and their own workspace),
  managers can read and edit files, run commands and search the web, through
  your own Claude Code.
- **Your mode decides.**
  - Manual: changes are asked in the Inbox before anything happens.
  - Accept edits: file changes go ahead directly; commands are still asked.
  - Auto: Claude decides more itself, without bypassing permissions.
  - Plan: plans only.
  - No answer within 30 minutes counts as a no.
- **Clear approvals.** For example "Alex wants to write a file" or "… run a
  command", with exactly what will happen.
- **Commands stand apart in the conversation.** Every action appears as its
  own block between the text: `$ command` with its output, edited files, web
  searches with their results. The blocks update while the answer streams in;
  click one for the full output.
- **Web search in chat.** With Web on, the general chat really searches the
  web and opens pages, and nothing else. You can also turn it on before a new
  chat's first message.
- **Back and forward.** ← → in the title bar, Alt+← / Alt+→, or the mouse's
  back and forward buttons move through screens and chats like a browser.
- **Collapsible sidebar.** The sidebar button in the title bar, or Ctrl+B,
  hides and shows it. Your choice is remembered.
- **Fixes.**
  - A manager's mode chip shows its actual mode.
  - `aurelius-ctl` output is in English.
  - Tests no longer depend on the environment they run in.

## v0.2.2

Mobile companion and a calmer interface.

- **Mobile companion.** Remote access (Settings → Remote access), pairing a
  phone, and the mobile app's gateway.
- **Everything by mouse.**
  - ⚙ and a menu (⋯) in the title bar.
  - ⋯ and right-click menus on projects, managers, tasks, chats and memories.
  - Clickable Settings sections on the left.
  - ✕ on every panel.
- **Shortcut hints only while holding Ctrl.** You can also choose to always
  show them.
- **Model popup** with versions (Opus 5.5, Sonnet 5.5, Fable 5.1, Haiku 4.5)
  and the effort level, like Claude Desktop.
- **Several conversations per manager.** The manager's main conversation stays
  as it is.
- **Projects** instead of Company; tabs on the left, search centred.
- **Plan side panel** with Markdown.
- **Chat as Markdown.** The general chat no longer has modes.
- **Fixes.**
  - Text fields no longer cut off g, j and y.
  - Debug builds use much less disk space.
