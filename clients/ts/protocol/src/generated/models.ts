// GENERATED CODE - DO NOT EDIT.
// Run: cargo xtask gen-ts

/**
 * What an employee's runs may do with files and commands (C3), next to its mode:
 * the level says which tools a run has, the mode which of those are asked first.
 */
export type AccessLevel =
  /**
   * Every work tool, as a run has without a level of its own.
   */
  | "inherit"
  /**
   * Also changes files; no commands or web.
   */
  | "read_edit"
  /**
   * Reads and searches the folder; no edits, commands or web.
   */
  | "read_only"
  /**
   * A level of a newer server this client does not know. A run treats it as read-only.
   */
  | "unknown"
;

/**
 * One run that really runs now (feature `activity_overview`).
 */
export interface ActivityBusy {
  /**
   * None: the project assistant, which is no employee.
   */
  employee_id?: string | null;
  employee_name: string;
  /**
   * It was paused during this run: the run finishes, nothing new starts.
   */
  employee_paused?: boolean;
  since_ms: number;
  work: ActivityWork;
}

/**
 * One delivery, queue step or run step (R8; feature `activity_log`).
 */
export interface ActivityLogEntry {
  at_ms: number;
  /**
   * delivery, queue or run. Text, not an enum: a later value only shows as it is.
   */
  category: string;
  /**
   * A short code or a tool's name; never the text of a message or a tool's input.
   */
  detail?: string | null;
  employee_id?: string | null;
  employee_name?: string | null;
  hop?: number | null;
  place?: ThreadPlace | null;
  run_id?: string | null;
  seq: number;
  /**
   * What happened (delivered, claimed, started, tool, ...). Text for the same reason.
   */
  step: string;
  thread_key?: string | null;
}

/**
 * `activity_log.list`: the newest rows that match, newest first.
 */
export interface ActivityLogPage {
  as_of_seq: number;
  entries: ActivityLogEntry[];
  /**
   * There are older rows: ask again with `before_seq` as the last row's `seq`.
   */
  has_more?: boolean;
}

/**
 * `activity.overview` (feature `activity_overview`): who is busy, what waits and
 * on what, and the state of each ticket with work going or held up. A ticket
 * that is not listed has nothing going on.
 */
export interface ActivityOverview {
  as_of_seq: number;
  busy: ActivityBusy[];
  tickets: TicketActivity[];
  waiting: ActivityWaiting[];
}

/**
 * What an employee is doing now (U6): the dot in the list. Derived, never stored; a
 * paused employee is in `status`.
 */
export type ActivityState =
  /**
   * Nothing for it to do.
   */
  | "idle"
  /**
   * There is work, but nothing runs yet (its turn, a usage limit, the machine).
   */
  | "queued"
  /**
   * A state of a newer server this client does not know.
   */
  | "unknown"
  /**
   * It waits for a board member: an approval, or an answer to a question.
   */
  | "waiting"
  /**
   * A run of it is going.
   */
  | "working"
;

/**
 * One tool use during an answer, shown at its place in the text.
 */
export interface ActivityView {
  call_id: string;
  input: unknown;
  /**
   * Byte position in the answer text where it happened.
   */
  offset: number;
  /**
   * `None` while running.
   */
  ok?: boolean | null;
  /**
   * Result text (capped); `None` while running.
   */
  output?: string | null;
  /**
   * The whole output's length in bytes, when `output` was shortened.
   */
  output_bytes?: number | null;
  /**
   * The `Task`/`Agent` call of the subagent that made this call (its
   * `AgentView::call_id`); none for the employee's own calls (feature
   * `subagent_steps`). Older servers leave it out.
   */
  parent_call_id?: string | null;
  /**
   * The runtime's tool name, e.g. "Bash", "Edit", "WebSearch".
   */
  tool: string;
  /**
   * `conversation.get` and live updates shorten the output and long strings
   * in the input (`crate::activity`); `activity.get` has all of it
   * (feature `activity_preview`). Older servers leave it out.
   */
  truncated?: boolean;
}

/**
 * One piece of work in line (feature `activity_overview`): one per employee and
 * thread, since what waits in one thread becomes one run.
 */
export interface ActivityWaiting {
  employee_id?: string | null;
  employee_name: string;
  since_ms: number;
  /**
   * slot (its employee is busy), machine (a free slot on this machine), paused,
   * question, approval, limit, login, later or handoff. Text, not an enum.
   */
  waits_for: string;
  work: ActivityWork;
}

/**
 * What an employee works on or waits with (feature `activity_overview`). With
 * `private` set it is another board member's chat: every other field is empty,
 * so neither its title nor where it is leaves the server.
 */
export interface ActivityWork {
  channel_id?: string | null;
  /**
   * A chat of the reader's own with an employee.
   */
  chat_id?: string | null;
  /**
   * "#21 Fix login", a task's title, a channel's name; empty when `private`.
   */
  label?: string;
  private?: boolean;
  task_id?: string | null;
  ticket_id?: string | null;
}

export interface AddConnection {
  default_model?: string | null;
  display_name: string;
  provider: ProviderKind;
  runtime_path?: string | null;
}

export interface AddIdea {
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  /**
   * Files on the server to attach (feature `idea_attachments`): copied into
   * the project's uploads area, as with a task's `files`.
   */
  files?: string[];
  kind?: IdeaKind;
  note?: string | null;
  project_id: string;
  title: string;
}

export type AgentMode =
  /**
   * File and artifact edits in granted folders go ahead; external actions still ask.
   */
  | "accept_edits"
  /**
   * Ask before every change (the default).
   */
  | "ask"
  /**
   * Everything goes ahead without asking, except what is always forbidden.
   */
  | "auto"
  /**
   * Only read and plan; no changes. Tasks stop after planning until you approve.
   */
  | "plan"
;

/**
 * One agent at work for an employee.
 */
export interface AgentView {
  /**
   * A subagent: the `Task`/`Agent` call that started it, so its steps can be
   * picked out of `run.get_activity` by `ActivityView::parent_call_id`
   * (feature `subagent_steps`). Older servers leave it out.
   */
  call_id?: string | null;
  /**
   * The employee's conversation this work shows in (a chat, or the task's).
   */
  conversation_id?: string | null;
  detail: string;
  /**
   * A subagent: its latest step, e.g. "Grep · poll_git_status".
   */
  doing?: string | null;
  /**
   * conversation | subagent | delegated
   */
  kind: string;
  name: string;
  /**
   * A subagent: what it was asked (shortened).
   */
  prompt?: string | null;
  run_id?: string | null;
  since_ms: number;
  /**
   * A subagent: how many tools it used so far.
   */
  steps: number;
  task_id?: string | null;
}

/**
 * The ideas of every project; each idea carries its own `project_id`.
 */
export interface AllIdeasSnapshot {
  ideas: IdeaView[];
}

/**
 * The user's answers to a question a runtime asked (Claude Code's
 * AskUserQuestion), keyed by the question text: the chosen option label,
 * several labels joined by ", ", or the user's own words.
 */
export interface AnswerQuestion {
  answers: { [key: string]: string };
  approval_id: string;
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
}

export interface AppError {
  code: ErrorCode;
  /**
   * `looks_like_secret`: the names of the vault items the text holds the value of.
   */
  matched_items?: string[];
  /**
   * User-facing explanation.
   */
  message: string;
  /**
   * `too_many_attempts`: how long until the next try is taken.
   */
  retry_after_ms?: number | null;
  /**
   * `guideline_too_large`: the characters, the limit and the combination that overflows.
   */
  too_large?: GuidelineTooLarge | null;
}

export type ApprovalStatus =
  | "approved"
  | "consumed"
  | "expired"
  | "invalidated"
  | "pending"
  | "rejected"
;

/**
 * Everything the user needs to judge one exact action.
 */
export interface ApprovalView {
  account: string;
  action_digest: string;
  /**
   * Whether clients may offer 'Always allow' (an MCP tool of the user's
   * servers or Claude in Chrome); answered with `DecideApproval::always_allow`.
   */
  always_allow_eligible?: boolean;
  arguments: unknown;
  conversation_id?: string | null;
  /**
   * The discussion (a channel, a ticket's) that woke the run (feature `run_cards`).
   * Left out for other runs and by older servers.
   */
  discussion_id?: string | null;
  effect: string;
  expires_at_ms: number;
  id: string;
  reasons: string[];
  requested_at_ms: number;
  /**
   * Who asked: the employee's name, or "Chat".
   */
  requester: string;
  resources: string[];
  run_id: string;
  status: ApprovalStatus;
  /**
   * The task, when a task's conversation asked.
   */
  task_id?: string | null;
  /**
   * The ticket it waits on: of the run's task, or of the discussion that woke
   * the run (feature `run_cards`). Left out for other runs and by older servers.
   */
  ticket_id?: string | null;
  tool: string;
}

export interface ApprovalsSnapshot {
  approvals: ApprovalView[];
  as_of_seq: number;
}

/**
 * An HTML file an employee is writing, before its `Write` call is complete: the
 * app shows it as it grows, as Claude Desktop does with an artifact. Display
 * only, never stored; the call itself arrives in `activity` once it is whole.
 */
export interface ArtifactDraft {
  /**
   * The `Write` call's id (`ActivityView::call_id` once it is whole).
   */
  call_id: string;
  /**
   * The content written so far (at most `ARTIFACT_DRAFT_MAX_BYTES`).
   */
  content: string;
  /**
   * The file's path as the employee wrote it.
   */
  path: string;
}

/**
 * The user gives a task to an employee (or takes it back: a draft). Allowed
 * while nothing runs: draft, queued or waiting.
 */
export interface AssignTask {
  employee_id?: string | null;
  task_id: string;
}

/**
 * The board member who started a session chooses who takes it, while the
 * dispatcher still chooses or after it chose nobody (feature `session_dispatch`).
 * The chosen employee answers the first message at once.
 */
export interface AssignWorkSession {
  conversation_id: string;
  employee_id: string;
}

/**
 * Snapshot a user-picked file into a conversation.
 */
export interface AttachFile {
  conversation_id: string;
  path: string;
}

export interface AttachmentView {
  byte_length: number;
  id: string;
  kind: string;
  name: string;
  truncated: boolean;
}

/**
 * `auto_allow.get_setting` and `auto_allow.set_enabled`: whether the safe read-only tools
 * (`tools`) pass without a question in every mode but Plan. On by default.
 */
export interface AutoAllowSetting {
  enabled: boolean;
  /**
   * The tools the setting covers, for display; fixed by the server.
   */
  tools?: string[];
}

export interface BackupView {
  app_version: string;
  bytes: number;
  created_at_ms: number;
  files: number;
  id: string;
  /**
   * "auto" or "manual"
   */
  kind: string;
  /**
   * Forgotten content was removed from this backup afterwards.
   */
  scrubbed: boolean;
}

export interface BackupsSnapshot {
  backups: BackupView[];
  error?: string | null;
  folder: string;
  /**
   * Set after a restore: background work waits until resumed.
   */
  paused?: string | null;
  restore_pending?: string | null;
}

/**
 * One column with its cards, in order.
 */
export interface BoardColumnCards {
  column: BoardColumnView;
  tickets: TicketView[];
}

export interface BoardColumnEdit {
  column_id: string;
  name: string;
}

export interface BoardColumnView {
  id: string;
  kind: ColumnKind;
  name: string;
  /**
   * 0 is the leftmost column.
   */
  position: number;
  version: number;
}

/**
 * `board.get`: the seven columns left to right, each with its cards (not archived).
 */
export interface BoardView {
  as_of_seq: number;
  columns: BoardColumnCards[];
  /**
   * The project filter it was read with; none: every project (B4).
   */
  project_id?: string | null;
}

/**
 * How a run drives the browser when the user allowed it for an employee.
 */
export type BrowserBackend =
  /**
   * The Claude in Chrome extension in the user's own browser profile.
   */
  | "claude_in_chrome"
  /**
   * Playwright MCP attached to a browser window of its own (an Aurelius
   * profile in Chrome or Edge, kept between runs, so logins stay). Its tools
   * follow the mode like other MCP tools and parallel runs each get a tab.
   */
  | "playwright"
;

/**
 * The browser that carries the Claude in Chrome extension.
 */
export type BrowserChoice =
  /**
   * Whichever is running, else the one that has the extension.
   */
  | "automatic"
  | "chrome"
  | "edge"
;

/**
 * `browser.get` and `browser.set`.
 */
export interface BrowserSetting {
  /**
   * How runs drive the browser. Always sent by servers that know the choice;
   * absent from older servers, which only have Claude in Chrome.
   */
  backend?: BrowserBackend | null;
  /**
   * What the user chose.
   */
  choice: BrowserChoice;
  /**
   * `AURELIUS_BROWSER` is set on the server and wins over `choice`.
   */
  env_override?: BrowserChoice | null;
}

/**
 * `scheduled_message.cancel`: a message an employee scheduled does not go out.
 */
export interface CancelScheduledMessage {
  message_id: string;
}

export interface CancelTask {
  task_id: string;
}

export type Capability =
  /**
   * Decide concrete approvals; switch a conversation to Auto.
   */
  | "approve"
  /**
   * Chats and project conversations: send, retry, stop, attach, share.
   */
  | "chat"
  /**
   * A paired computer with full access: connections and keys, folders on
   * the server, backups, updates and remote access itself.
   */
  | "owner"
  /**
   * Every query and the event stream.
   */
  | "read"
  /**
   * Projects, employees, tasks, plans and memory.
   */
  | "task_control"
;

/**
 * What a channel is (`ChannelView::kind`, feature `ticket_channels`).
 */
export type ChannelKind =
  /**
   * The general channel (no project).
   */
  | "general"
  /**
   * A project's channel.
   */
  | "project"
  /**
   * A ticket's channel: its discussion.
   */
  | "ticket"
  /**
   * A kind of a newer server this client does not know.
   */
  | "unknown"
;

/**
 * A channel of the Team chat (`channel.list`, feature `team_channels`): the
 * general one (no project), one per project, and (feature `ticket_channels`,
 * only for a connection that accepts it) one per ticket. A closed ticket's
 * channel (ticket on Done or archived) reports `unread` and `mentions` 0, so old
 * closed tickets never light up the Team badge; an archived project's channel
 * still counts.
 */
export interface ChannelView {
  /**
   * The channel can only be read: its project is archived, or (a ticket's
   * channel) the ticket is on Done or archived.
   */
  archived: boolean;
  conversation_id: string;
  /**
   * What the channel is (feature `ticket_channels`); left out for General and by older servers.
   */
  kind?: ChannelKind;
  last_activity_ms?: number | null;
  /**
   * The board member who reads the list is a member of the channel (a
   * ticket's channel: its creator, owner, the Owner, or who joined later).
   */
  member?: boolean;
  /**
   * How many board members and employees are members (a ticket's channel).
   */
  member_count?: number | null;
  /**
   * Of those, the ones that name a board member with `@Name` (B8: the badge counts these).
   */
  mentions: number;
  /**
   * "General" or the project's name.
   */
  name: string;
  /**
   * None: the general channel.
   */
  project_id?: string | null;
  /**
   * The highest message ordinal the reader had read before this read (the
   * line "New messages" goes after it); none: never read.
   */
  read_through?: number | null;
  /**
   * A ticket's channel: the kind of the column the ticket is in.
   */
  ticket_column?: ColumnKind | null;
  /**
   * A ticket's channel: the ticket.
   */
  ticket_id?: string | null;
  /**
   * A ticket's channel: the ticket's number (`#N`).
   */
  ticket_number?: number | null;
  /**
   * Messages since the last one read; 0 for a closed ticket's channel.
   */
  unread: number;
}

/**
 * `channel.list`: the general channel first, then one per project by name.
 */
export interface ChannelsSnapshot {
  as_of_seq: number;
  channels: ChannelView[];
}

/**
 * `claude_login.input`. The text (a one-time code) is never printed.
 */
export interface ClaudeLoginInputParams {
  link_id: string;
  text: string;
}

/**
 * `ServerMessage::ClaudeLoginPrompt`: the sign-in link to open and/or the
 * question for the code, as `claude` printed them.
 */
export interface ClaudeLoginPrompt {
  /**
   * Claude waits for the code: send it with `claude_login.input`.
   */
  asks_code: boolean;
  /**
   * The link ended without a login: a fixed text of Aurelius (never Claude's
   * output), shown with "Try again". A link that worked ends with the event
   * `claude_login.updated` instead.
   */
  error?: string | null;
  link_id: string;
  /**
   * The Anthropic sign-in page to open (`https://claude.com/…/oauth/authorize?…`).
   */
  url?: string | null;
}

/**
 * The state of a board member's own Claude login (`<data>/logins/<user_id>/`).
 */
export type ClaudeLoginStatus =
  /**
   * Claude asked to sign in again (a 401 in a run): Reconnect.
   */
  | "expired"
  | "linked"
  /**
   * Never connected, or disconnected: the board member's tickets wait (`needs: login`).
   */
  | "missing"
;

/**
 * `claude_login.status`, `HelloResult::claude_login`: the session's own login.
 * Never a token or a path; the email and the plan come from `claude auth status`.
 */
export interface ClaudeLoginView {
  /**
   * This login also runs the company's background jobs (memory
   * consolidation, B6): `company.set_background_login`.
   */
  background: boolean;
  checked_at_ms?: number | null;
  email?: string | null;
  /**
   * A `claude_login.link` of this board member is running (one at a time).
   */
  link_id?: string | null;
  linked_at_ms?: number | null;
  /**
   * e.g. "max", "pro"; none when unknown.
   */
  plan?: string | null;
  status: ClaudeLoginStatus;
  user_id: string;
}

/**
 * One local branch.
 */
export interface CodeBranch {
  /**
   * Commits on this branch that main lacks (`main..branch`); 0 for main.
   */
  ahead: number;
  at_ms: number;
  author: string;
  /**
   * Commits on main this branch lacks (`branch..main`); 0 for main.
   */
  behind: number;
  /**
   * Its own commits (`main..branch`), newest first, at most `CODE_BRANCH_COMMITS_MAX`.
   */
  commits?: CodeCommitLine[];
  deletions?: number;
  /**
   * The size of `main...branch` (what a merge would bring): files, lines added, removed.
   */
  files_changed?: number;
  /**
   * Its newest commit.
   */
  head: string;
  insertions?: number;
  /**
   * The repository's main branch.
   */
  is_main?: boolean;
  /**
   * Every commit of it is in main (git's view: a fresh branch at main counts too; see `open`).
   */
  merged?: boolean;
  name: string;
  /**
   * Its ticket is not on Done (nor archived) or its work session is not deleted: work on
   * it may still come, also when nothing is ahead of main yet. The Code tab never offers to
   * remove or delete such a branch, and Clean up merged leaves it alone.
   */
  open?: boolean;
  /**
   * Who works on it: the ticket's owner, else the employee of the work session.
   */
  owner?: CodeOwner | null;
  /**
   * Its newest commit: subject, author name and time.
   */
  subject: string;
  /**
   * The ticket it belongs to: a quick task's branch, a work session's worktree, or a
   * ticket number in its name (`feat/ticket-77-…`, `fix/77-…`) of this project.
   */
  ticket?: CodeTicket | null;
  /**
   * Its upstream (`origin/…`); for main: how far it is from it after a fetch.
   */
  upstream?: string | null;
  upstream_ahead?: number;
  upstream_behind?: number;
  /**
   * An employee works on it now: a run of its ticket or work session is live or queued.
   */
  working?: boolean;
  /**
   * The worktree it is checked out in; none: no worktree.
   */
  worktree_path?: string | null;
}

/**
 * One commit of the graph.
 */
export interface CodeCommit {
  at_ms: number;
  author: string;
  /**
   * The listed local branches whose newest commit this is.
   */
  branches?: string[];
  /**
   * The full hash; the client shows the first seven characters.
   */
  hash: string;
  /**
   * Its parents' full hashes, the first parent first (two or more: a merge).
   */
  parents?: string[];
  subject: string;
}

/**
 * A commit of a branch's own list.
 */
export interface CodeCommitLine {
  hash: string;
  subject: string;
}

/**
 * Who works on a branch.
 */
export interface CodeOwner {
  employee_id: string;
  name: string;
}

/**
 * The ticket a branch belongs to.
 */
export interface CodeTicket {
  number: number;
  ticket_id: string;
  title: string;
}

/**
 * The git state of one project's folder.
 */
export interface CodeView {
  /**
   * The local branches, main first, then the newest last commit first.
   */
  branches?: CodeBranch[];
  /**
   * The newest commits of main and every listed branch, newest first (date order),
   * at most `CODE_COMMITS_MAX`. A parent outside the list is cut off in the graph.
   */
  commits?: CodeCommit[];
  /**
   * When the last `git fetch` ended (the time of `FETCH_HEAD`); none: never.
   */
  fetched_at_ms?: number | null;
  /**
   * Whether the repository has a remote at all (no remote: no Fetch).
   */
  has_remote?: boolean;
  /**
   * The repository's main branch (`main`, else `master`); none: see `problem`.
   */
  main_branch?: string | null;
  /**
   * Why there is nothing to show, in words a board member can act on (no folder,
   * not a git repository, git missing, no main branch). The lists are empty then.
   */
  problem?: string | null;
  project_id: string;
  /**
   * The project's folder (the main checkout).
   */
  repo_path: string;
  /**
   * Every worktree git knows, the main checkout first.
   */
  worktrees?: CodeWorktree[];
}

/**
 * One worktree of the repository.
 */
export interface CodeWorktree {
  /**
   * Its branch; none: a detached HEAD.
   */
  branch?: string | null;
  /**
   * How many files have uncommitted changes (may be more than `changed_files` names).
   */
  changed_count?: number;
  /**
   * Uncommitted changes (tracked and untracked files), at most `CODE_FILES_MAX` named.
   */
  changed_files?: string[];
  /**
   * The commit it is on.
   */
  head: string;
  /**
   * The project's folder itself; never removed from here.
   */
  is_main?: boolean;
  /**
   * Git no longer finds its folder (`git worktree prune` removes it).
   */
  missing?: boolean;
  path: string;
  /**
   * An employee works in it now: its branch is `working`, or (a detached HEAD) a run of
   * the work session in this folder is live or queued. It is not removed then.
   */
  working?: boolean;
}

/**
 * What a column does. The seven kinds always exist, once each.
 */
export type ColumnKind =
  /**
   * New tickets; nothing starts from here by itself.
   */
  | "backlog"
  /**
   * The main task runs or waits (for a colleague, an approval, input).
   */
  | "doing"
  /**
   * Accepted by a board member; read-only.
   */
  | "done"
  /**
   * A plan waits for a board member (Plan mode only).
   */
  | "plan_review"
  /**
   * The main task reported; a board member accepts it or sends it back.
   */
  | "review"
  /**
   * The main task reported; the tester runs the full test suite once before
   * Review (protocol 5, feature `board_testing`). Only with a company tester:
   * without one, cards go from Doing to Review as before.
   */
  | "testing"
  /**
   * Started: waiting for the dispatcher or for the owner's turn.
   */
  | "todo"
;

/**
 * Every business mutation of the domain enters the system as one of these
 * commands. `Command::method` gives its protocol method name.
 */
export type Command =
  /**
   * Review → Done. Only a board member; only when no linked task still works (B6).
   */
  | ({ type: "accept_ticket"; confirm_secret?: boolean; note?: string | null; ticket_id: string })
  | ({ type: "add_connection" } & AddConnection)
  | ({ type: "add_idea" } & AddIdea)
  /**
   * `guideline.add_starter_set`: the chosen templates as guidelines without links (B7).
   * A key whose title is in the library already is skipped.
   */
  | ({ type: "add_starter_guidelines"; keys: string[] })
  | ({ type: "answer_question" } & AnswerQuestion)
  /**
   * Plan review → Doing: the owner carries the plan out.
   */
  | ({ type: "approve_plan"; decision: PlanApproval; feedback?: string | null; ticket_id: string })
  /**
   * `guideline.archive`: no run gets it any more; links and versions stay.
   */
  | ({ type: "archive_guideline"; guideline_id: string })
  /**
   * Take a ticket off the board (from Done or Backlog); it stays readable.
   */
  | ({ type: "archive_ticket"; ticket_id: string })
  | ({ type: "assign_task" } & AssignTask)
  /**
   * Choose the owner of a ticket before its main task exists: a board member
   * (instead of the dispatcher) or the dispatcher itself. Gives `ticket.dispatched`.
   *
   * `employee_id` none: no owner again, so the dispatcher chooses when it
   * starts. Only in Backlog (before the start); gives `ticket.updated` with
   * `changed: ["owner"]`, not `ticket.dispatched` (nobody was chosen).
   */
  | ({ type: "assign_ticket"; employee_id?: string | null; reason?: string | null; ticket_id: string })
  | ({ type: "assign_work_session" } & AssignWorkSession)
  | ({ type: "attach_file" } & AttachFile)
  | ({ type: "cancel_scheduled_message" } & CancelScheduledMessage)
  | ({ type: "cancel_task" } & CancelTask)
  /**
   * Stop the work on a ticket: its main task is cancelled and it goes back to Backlog.
   */
  | ({ type: "cancel_ticket"; ticket_id: string })
  | ({ type: "consolidate_memory" } & ConsolidateMemory)
  | ({ type: "create_employee" } & CreateEmployee)
  /**
   * `guideline.create`: a new guideline (version 1, source `created`), linked at once to `links`.
   */
  | ({ type: "create_guideline"; links?: GuidelineTarget[]; text: string; title: string })
  | ({ type: "create_project" } & CreateProject)
  | ({ type: "create_task" } & CreateTask)
  | ({ type: "create_task_schedule" } & CreateTaskSchedule)
  /**
   * A new ticket in Backlog (`start`: straight to Todo, as `ticket.start` would).
   */
  | ({ type: "create_ticket"; confirm_secret?: boolean; deadline_ms?: number | null; description?: string | null; files?: string[]; from_idea?: string | null; from_message?: string | null; kind?: TicketKind | null; labels?: string[]; owner_employee_id?: string | null; priority?: TicketPriority; project_id: string; responsible_user_id?: string | null; run?: TaskRun; start?: boolean; title: string })
  | ({ type: "decide_approval" } & DecideApproval)
  /**
   * `guideline.decide_proposal` (B5): approve (with `title`/`text` edited by the
   * board member, and for a new guideline the `links`; none: the suggested
   * ones) or reject. `note` goes back to the proposer. `version_conflict` when
   * the guideline moved on since the proposal's base version.
   */
  | ({ type: "decide_guideline_proposal"; approve: boolean; links?: GuidelineTarget[] | null; note?: string | null; proposal_id: string; text?: string | null; title?: string | null })
  | ({ type: "decide_memory" } & DecideMemory)
  | ({ type: "decide_plan" } & DecidePlan)
  | ({ type: "decide_proposal" } & DecideProposal)
  | ({ type: "delete_conversation" } & DeleteConversation)
  | ({ type: "delete_idea" } & DeleteIdea)
  | ({ type: "delete_message" } & DeleteMessage)
  | ({ type: "delete_queued_message" } & DeleteQueuedMessage)
  | ({ type: "delete_task_schedule" } & DeleteTaskSchedule)
  | ({ type: "dispatch_work_session" } & DispatchWorkSession)
  | ({ type: "edit_message" } & EditMessage)
  | ({ type: "follow_ticket" } & FollowTicket)
  | ({ type: "forget_memory" } & ForgetMemory)
  /**
   * `guideline.link`: an employee or a project gets it from its next run.
   */
  | ({ type: "link_guideline"; guideline_id: string; target: GuidelineTarget })
  /**
   * Drag a card. Within its column: only the order. To Todo: start it. To
   * Backlog: cancel it. Done only through `ticket.accept`, and Review back to
   * Doing only through `ticket.send_back`; out of Testing only to Backlog (cancel);
   * other moves follow the work and are refused.
   * The place is relative to a neighbour, so it means the same on a board
   * filtered by project as on the whole board. A move within the same column
   * also gives `ticket.moved` (with `from` = `to`). A card put above itself in
   * its own column stays where it is: no event, the version is unchanged.
   * Unknown fields (such as the old `position`) are refused.
   */
  | ({ type: "move_ticket"; before_ticket_id?: string | null; column_id: string; ticket_id: string })
  | ({ type: "open_employee_chat" } & OpenEmployeeChat)
  | ({ type: "pin_memory" } & PinMemory)
  /**
   * A board member writes in a ticket's discussion (feature `ticket_discussion`).
   * Without `@Name` it goes to the owner while the ticket is in Todo (with an
   * owner), Doing, Plan review, Testing or Review; else it wakes nobody. `@Name` wakes those
   * employees. Refused on Done (the discussion is read-only there). Not for apps.
   */
  | ({ type: "post_ticket_message"; confirm_secret?: boolean; files?: string[]; images?: string[]; text: string; thread_parent?: string | null; ticket_id: string })
  | ({ type: "react_to_message" } & ReactToMessage)
  | ({ type: "remember" } & Remember)
  | ({ type: "remove_attachment" } & RemoveAttachment)
  | ({ type: "remove_connection" } & RemoveConnection)
  | ({ type: "rename_project" } & RenameProject)
  | ({ type: "restore_employee_instructions" } & RestoreEmployeeInstructions)
  /**
   * `guideline.restore`: back from the archive, with its links (B4 and the title are checked again).
   */
  | ({ type: "restore_guideline"; guideline_id: string })
  /**
   * `guideline.restore_version`: a copy of `version` as the newest (source `restored`).
   */
  | ({ type: "restore_guideline_version"; guideline_id: string; version: number })
  | ({ type: "retry_reply" } & RetryReply)
  | ({ type: "retry_session_summary" } & RetrySessionSummary)
  /**
   * Try the summary of a ticket on Done again after it failed (`SystemLineKind::SummaryFailed`).
   * Only a board member; refused when the summary is already there or still being made.
   */
  | ({ type: "retry_ticket_summary"; ticket_id: string })
  | ({ type: "revise_memory" } & ReviseMemory)
  /**
   * Plan review → Doing in Plan mode: the owner makes a new plan with the feedback.
   */
  | ({ type: "revise_plan"; confirm_secret?: boolean; feedback: string; ticket_id: string })
  | ({ type: "run_task_schedule_now" } & RunTaskScheduleNow)
  /**
   * Review → Doing with feedback (required) for the owner.
   */
  | ({ type: "send_back_ticket"; confirm_secret?: boolean; feedback: string; ticket_id: string })
  | ({ type: "send_chat_message" } & SendChatMessage)
  | ({ type: "send_employee_message" } & SendEmployeeMessage)
  | ({ type: "send_group_message" } & SendGroupMessage)
  | ({ type: "send_project_message" } & SendProjectMessage)
  | ({ type: "send_queued_now" } & SendQueuedNow)
  | ({ type: "set_agent_mode" } & SetAgentMode)
  /**
   * The company's background jobs (memory consolidation, B6) run on the Claude
   * login of the board member who asks (`enabled`), or on none (`false`, only
   * when it is theirs): without one they do not run. Only the own login can be
   * chosen. Answers `CompanyUpdated`.
   */
  | ({ type: "set_background_login"; enabled: boolean })
  | ({ type: "set_browser_allowed" } & SetBrowserAllowed)
  | ({ type: "set_conversation_run" } & SetConversationRun)
  | ({ type: "set_employee_folder" } & SetEmployeeFolder)
  | ({ type: "set_employee_reports_to" } & SetEmployeeReportsTo)
  | ({ type: "set_employee_status" } & SetEmployeeStatus)
  | ({ type: "set_follow_org_chart" } & SetFollowOrgChart)
  | ({ type: "set_idea_status" } & SetIdeaStatus)
  | ({ type: "set_project_description" } & SetProjectDescription)
  | ({ type: "set_project_folder" } & SetProjectFolder)
  | ({ type: "set_project_status" } & SetProjectStatus)
  | ({ type: "set_run_defaults" } & SetRunDefaults)
  | ({ type: "set_screen_allowed" } & SetScreenAllowed)
  /**
   * Choose the server's name (`server.set_name`). Empty or only spaces puts the
   * computer's name back. The same name as now changes nothing (no event).
   * Answers `ServerNameSet`.
   */
  | ({ type: "set_server_name"; name: string })
  | ({ type: "set_task_schedule_paused" } & SetTaskSchedulePaused)
  /**
   * Choose the board member responsible for a ticket (`ticket.set_responsible`,
   * feature `ticket_responsible`). Its own command rather than a field of
   * `UpdateTicket`, where "nobody" and "unchanged" would need two kinds of none.
   */
  | ({ type: "set_ticket_responsible"; responsible_user_id?: string | null; ticket_id: string })
  | ({ type: "start_idea" } & StartIdea)
  | ({ type: "start_project_conversation" } & StartProjectConversation)
  /**
   * The Start button: Backlog to Todo. With an employee the dispatcher is skipped.
   * Also starts again a ticket whose main task failed.
   */
  | ({ type: "start_ticket"; employee_id?: string | null; ticket_id: string })
  | ({ type: "start_work_session" } & StartWorkSession)
  | ({ type: "stop_run" } & StopRun)
  /**
   * `guideline.unlink`.
   */
  | ({ type: "unlink_guideline"; guideline_id: string; target: GuidelineTarget })
  /**
   * Rename the columns and set their order (B3): all of them, left to right.
   * `expected_version` is the board's (`board_version`: the columns change
   * together); the answer gives the new one.
   */
  | ({ type: "update_board_columns"; columns: BoardColumnEdit[] })
  | ({ type: "update_company" } & UpdateCompany)
  | ({ type: "update_employee" } & UpdateEmployee)
  /**
   * `guideline.update`: a new version with what changed (source `edited`); what
   * is left out stays. `expected_version` in the envelope is the version the
   * editor started from (`version_conflict` otherwise).
   */
  | ({ type: "update_guideline"; guideline_id: string; text?: string | null; title?: string | null })
  | ({ type: "update_idea" } & UpdateIdea)
  | ({ type: "update_project" } & UpdateProject)
  | ({ type: "update_task_schedule" } & UpdateTaskSchedule)
  /**
   * Change the fields of a ticket. Left out: unchanged. The creator never
   * changes; the owner only through `ticket.assign`; mode and model only while
   * the main task does not run.
   */
  | ({ type: "update_ticket"; add_files?: string[] | null; clear_deadline?: boolean; confirm_secret?: boolean; deadline_ms?: number | null; description?: string | null; design_url?: string | null; labels?: string[] | null; priority?: TicketPriority | null; run?: TaskRun | null; ticket_id: string; title?: string | null })
  /**
   * Run a ticket on the Claude login of the board member who asks (B4, D3-3):
   * `TicketView::login_user` becomes that board member, never anyone else. The
   * creator stays. Only a board member; answers `TicketChanged`.
   */
  | ({ type: "use_my_login"; ticket_id: string })
  | ({ type: "verify_connection" } & VerifyConnection)
;

/**
 * The answer of `command.applied`.
 */
export interface CommandApplied {
  /**
   * True: a write with this key committed. False: none did (yet).
   */
  applied: boolean;
}

export type CommandResult =
  | ({ type: "agent_mode_set"; conversation_id: string; mode: AgentMode })
  | ({ type: "approval_decided"; approval_id: string; ok: boolean; output: string; status: ApprovalStatus })
  | ({ type: "attachment_added"; attachment_id: string; conversation_id: string })
  | ({ type: "attachment_removed"; attachment_id: string })
  /**
   * The board's new version (`ticket::board_version`).
   */
  | ({ type: "board_columns_updated"; version: number })
  | ({ type: "browser_allowed_set"; allowed: boolean; conversation_id: string })
  | ({ type: "chat_message_sent"; conversation_id: string; message_id: string; run_id: string })
  | ({ type: "company_updated"; version: number })
  | ({ type: "connection_added"; connection_id: string })
  | ({ type: "connection_removed"; connection_id: string })
  | ({ type: "conversation_deleted"; conversation_id: string })
  | ({ type: "conversation_run_set"; conversation_id: string })
  | ({ type: "employee_chat_opened"; conversation_id: string })
  | ({ type: "employee_created"; conversation_id: string; employee_id: string })
  | ({ type: "employee_folder_set"; employee_id: string; folder?: string | null })
  /**
   * An earlier version of an employee's instructions was put back as `version` (the new current one).
   */
  | ({ type: "employee_instructions_restored"; employee_id: string; version: number })
  | ({ type: "employee_message_sent"; conversation_id: string; message_id: string; run_id: string })
  | ({ type: "employee_reports_to_set"; employee_id: string; reports_to?: string | null })
  | ({ type: "employee_status_changed"; employee_id: string; status: LifecycleStatus })
  | ({ type: "employee_updated"; employee_id: string; version: number })
  | ({ type: "follow_org_chart_set"; follow: boolean })
  /**
   * A message in the group chat, with the employees it woke (each answers in a run of its own).
   */
  | ({ type: "group_message_sent"; conversation_id: string; message_id: string; run_ids: string[]; to: string[] })
  /**
   * Every guideline write but a proposal's decision and the starter set: the
   * guideline and its current version now.
   */
  | ({ type: "guideline_changed"; guideline_id: string; version: number })
  /**
   * `guideline.decide_proposal`; `guideline_id`: the guideline that got the
   * new version (or was made), none for a rejection.
   */
  | ({ type: "guideline_proposal_decided"; guideline_id?: string | null; proposal_id: string; status: ProposalStatus })
  | ({ type: "idea_added"; idea_id: string })
  | ({ type: "idea_deleted"; idea_id: string })
  | ({ type: "idea_started"; idea_id: string; task_id: string })
  | ({ type: "idea_updated"; idea_id: string })
  /**
   * `queued` is false when a consolidation of that memory is already waiting or running.
   */
  | ({ type: "memory_consolidation_queued"; queued: boolean })
  | ({ type: "memory_decided"; memory_id: string; status: MemoryStatus })
  | ({ type: "memory_forgotten"; memory_id: string })
  | ({ type: "memory_stored"; memory_id: string })
  /**
   * `message.delete` (feature `message_edits`).
   */
  | ({ type: "message_deleted"; conversation_id: string; deleted_at_ms: number; message_id: string })
  /**
   * `message.edit` (feature `message_edits`).
   */
  | ({ type: "message_edited"; conversation_id: string; edited_at_ms: number; message_id: string })
  /**
   * `message.react` (feature `chat_reactions`): the reaction is as asked now.
   * `changed` is false when it already was (on twice, off when not there).
   */
  | ({ type: "message_reacted"; changed: boolean; conversation_id: string; emoji: string; message_id: string; on: boolean })
  /**
   * A new, empty project conversation.
   */
  | ({ type: "project_conversation_started"; conversation_id: string; project_id: string })
  | ({ type: "project_created"; project_id: string; version: number })
  | ({ type: "project_description_set"; project_id: string; version: number })
  | ({ type: "project_folder_set"; folder: string; project_id: string })
  /**
   * A message in a project conversation was accepted and is being answered.
   */
  | ({ type: "project_message_sent"; conversation_id: string; message_id: string; project_id: string; run_id: string })
  | ({ type: "project_renamed"; project_id: string; version: number })
  | ({ type: "project_status_changed"; project_id: string; status: LifecycleStatus })
  | ({ type: "project_updated"; project_id: string; version: number })
  /**
   * `employee_id` is the employee that was created when the proposal was approved.
   */
  | ({ type: "proposal_decided"; employee_id?: string | null; proposal_id: string; status: ProposalStatus })
  /**
   * A waiting message was removed and its queued run cancelled.
   */
  | ({ type: "queued_message_deleted"; conversation_id: string; message_id: string; run_id: string })
  /**
   * A waiting message's run starts now; the answers ahead of it were stopped.
   */
  | ({ type: "queued_message_sent_now"; conversation_id: string; message_id: string; run_id: string })
  | ({ type: "reply_retried"; run_id: string })
  | ({ type: "run_defaults_set" })
  | ({ type: "scheduled_message_cancelled"; message_id: string })
  | ({ type: "screen_allowed_set"; allowed: boolean; conversation_id: string })
  /**
   * `server.set_name`: the name clients show now and its version (unchanged
   * when the name was the same).
   */
  | ({ type: "server_name_set"; name: string; version: number })
  /**
   * A work session's summary for the History was asked for (`work_session.retry_summary`).
   */
  | ({ type: "session_summary_asked"; conversation_id: string })
  /**
   * `guideline.add_starter_set`: the guidelines made (keys already in the library are skipped).
   */
  | ({ type: "starter_guidelines_added"; guideline_ids: string[] })
  | ({ type: "stop_requested"; already_finished: boolean; run_id: string })
  | ({ type: "task_assigned"; task_id: string })
  | ({ type: "task_cancelled"; task_id: string })
  | ({ type: "task_created"; task_id: string })
  /**
   * Also the answer to update, pause and resume.
   */
  | ({ type: "task_schedule_created"; schedule_id: string })
  | ({ type: "task_schedule_deleted"; schedule_id: string })
  | ({ type: "task_schedule_ran"; schedule_id: string; task_id: string })
  | ({ type: "task_schedule_updated"; schedule_id: string })
  /**
   * The answer to every other ticket write: where the ticket is now.
   */
  | ({ type: "ticket_changed"; column: ColumnKind; ticket_id: string; version: number })
  | ({ type: "ticket_created"; number: number; ticket_id: string })
  /**
   * `ticket.follow` (feature `ticket_follow`): the reader follows it as asked
   * now. `changed` is false when they already did (or did not).
   */
  | ({ type: "ticket_follow_changed"; changed: boolean; on: boolean; ticket_id: string })
  /**
   * A message in a ticket's discussion, with the employees it woke (or the
   * owner it went to: `run_ids` is empty when the main task already runs and takes it).
   */
  | ({ type: "ticket_message_posted"; conversation_id: string; message_id: string; run_ids: string[]; ticket_id: string; to: string[] })
  /**
   * A new board member (`user.create`).
   */
  | ({ type: "user_created"; user_id: string })
  | ({ type: "verification_started"; connection_id: string })
  /**
   * A work session was made; the dispatcher chooses who answers its first message.
   */
  | ({ type: "work_session_dispatching"; conversation_id: string; message_id: string; project_id: string })
  /**
   * A work session was made and its first message is being answered.
   */
  | ({ type: "work_session_started"; conversation_id: string; message_id: string; project_id: string; run_id: string })
;

/**
 * `compaction.get_setting` / `compaction.set_threshold` (feature `compaction_setting`):
 * above how many tokens of context a conversation's session is compacted before its
 * next turn. The limits travel along so a client needs no copy of them.
 */
export interface CompactionSetting {
  default_tokens: number;
  max_tokens: number;
  min_tokens: number;
  /**
   * The threshold in tokens; 0: never compacted by Aurelius (Claude Code still
   * compacts by itself when its context window is full).
   */
  threshold_tokens: number;
}

/**
 * The company in one read (`company.get`): employees, the org chart setting
 * and its roots (the management team).
 */
export interface CompanySnapshot {
  as_of_seq: number;
  /**
   * Whose Claude login runs the background jobs (B6); none: they do not run.
   */
  background_login?: UserView | null;
  /**
   * The company itself: its name and default dispatcher.
   */
  company: CompanyView;
  /**
   * Employees that are not archived, oldest first.
   */
  employees: EmployeeSummary[];
  /**
   * "Follow the org chart" (server setting, on by default).
   */
  follow_org_chart: boolean;
  /**
   * Active employees without a boss, oldest first: the management team.
   */
  roots: string[];
}

export interface CompanyView {
  /**
   * Chooses the owner of a started ticket without one, unless its project
   * names its own. None: such tickets wait in Todo for a board member.
   */
  dispatcher_employee_id?: string | null;
  id: string;
  name: string;
  /**
   * Runs the full test suite once while a card is in Testing, before Review
   * (feature `board_testing`). None: no Testing step, cards go from Doing to
   * Review as before. Left out by older servers.
   */
  tester_employee_id?: string | null;
  version: number;
}

export type ConfigScope =
  | ({ kind: "employee"; id: string })
  | ({ kind: "global" })
  | ({ kind: "project"; id: string })
;

export interface ConnectionView {
  auth_kind: string;
  billing: string;
  default_model?: string | null;
  detail?: string | null;
  display_name: string;
  has_secret: boolean;
  id: string;
  last_verified_ms?: number | null;
  models: string[];
  provider: string;
  runtime_path?: string | null;
  /**
   * disconnected | ready | needs_auth | unavailable
   */
  status: string;
}

export interface ConnectionsSnapshot {
  as_of_seq: number;
  connections: ConnectionView[];
  defaults: DefaultsView[];
}

/**
 * Merge a scope's memories into short points now, instead of waiting for
 * the background job (which starts by itself once the scope grows).
 */
export interface ConsolidateMemory {
  scope: MemoryScope;
}

/**
 * One line of a context manifest: what was considered and why it was (not) used.
 */
export interface ContextItemView {
  hash: string;
  id: string;
  included: boolean;
  label: string;
  reason: string;
  score?: number | null;
  /**
   * rules | identity | request | turn | summary | memory | source | guideline
   * (one item per guideline in the run's block, feature `guidelines`; `id` is
   * `g:<id>@<version>`, never left out by the budget: `included` false only
   * when B4's safety net dropped it).
   */
  section: string;
  tokens: number;
}

export interface ContextView {
  budget_tokens: number;
  items: ContextItemView[];
  run_id: string;
  /**
   * The runtime keeps its own history (resume); Aurelius does not see every token.
   */
  runtime_managed: boolean;
  scopes: string[];
  used_tokens: number;
}

export interface ConversationSummary {
  created_at_ms: number;
  id: string;
  message_count: number;
  title: string;
  updated_at_ms: number;
}

export interface CreateEmployee {
  /**
   * What its runs may do with files and commands (C3); none: `Inherit`.
   */
  access_level?: AccessLevel | null;
  /**
   * What other employees read to know when to hand work to this one (P7).
   * None or empty: their prompt shows the first line of the instructions.
   */
  description?: string | null;
  /**
   * Optional own workspace folder: the work folder of its chat.
   */
  folder?: string | null;
  instructions?: string | null;
  name: string;
  /**
   * Who this employee reports to (not archived); none: a root (the management team).
   */
  reports_to?: string | null;
  role: string;
  /**
   * A key of `employee.start_profiles` (C6). What this command leaves out comes from there.
   */
  start_profile?: string | null;
  /**
   * The position on the org chart ("Head of engineering"); free text, at most 80 characters.
   */
  title?: string | null;
  /**
   * Which tools and MCP servers it has and which ask first (C2); none: all of its level.
   */
  tool_policy?: ToolPolicy | null;
}

export interface CreateProject {
  description?: string | null;
  /**
   * The project's folder; without one Aurelius creates a default folder.
   */
  folder?: string | null;
  name: string;
}

export interface CreateTask {
  acceptance_criteria?: string[];
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  /**
   * Without an employee the task stays a draft.
   */
  employee_id?: string | null;
  /**
   * Files of any type (images too) to copy into the work folder's
   * git-ignored uploads area: host paths, or this device's upload inbox
   * (protocol 1.14; at most 8, 25 MB each).
   */
  files?: string[];
  goal: string;
  project_id: string;
  /**
   * How the task's conversation runs; unset fields follow the employee.
   */
  run?: TaskRun;
  title: string;
}

/**
 * A new recurring task for an employee.
 */
export interface CreateTaskSchedule {
  acceptance_criteria?: string[];
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  employee_id: string;
  /**
   * Files to attach to every task made from it (host paths, or this
   * device's upload inbox); copied when the schedule is saved.
   */
  files?: string[];
  /**
   * Kept as typed (newlines included), apart from surrounding blank space.
   */
  goal: string;
  /**
   * Start paused: nothing is made until it is resumed.
   */
  paused?: boolean;
  project_id: string;
  recurrence: Recurrence;
  /**
   * Mode, model and effort of the tasks; unset fields follow the employee.
   */
  run?: TaskRun;
  title: string;
}

/**
 * The user's decision on exactly one action. `action_digest` must match the
 * digest the user was shown; a changed action is a new request.
 */
export interface DecideApproval {
  action_digest: string;
  /**
   * With `approve`: also allow this MCP tool from now on, in every run (see
   * `always_allow_eligible`), and approve its other pending asks.
   */
  always_allow?: boolean;
  approval_id: string;
  approve: boolean;
}

/**
 * Accept or reject a proposed memory.
 */
export interface DecideMemory {
  accept: boolean;
  memory_id: string;
}

/**
 * The user's decision on a plan waiting in the inbox.
 */
export interface DecidePlan {
  approval_id: string;
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  decision: PlanDecision;
  feedback?: string | null;
}

/**
 * The user's decision. `edits` replaces the proposed fields of a colleague
 * proposal (approve and adjust); `instructions` replaces the proposed text of
 * an instruction proposal. `note` goes back to the proposer, usually with a
 * rejection. Approving an instruction proposal whose base version is no longer
 * the employee's current one is refused (`version_conflict`).
 */
export interface DecideProposal {
  approve: boolean;
  edits?: ProposalFields | null;
  /**
   * Instruction proposals: the text to apply instead of the proposed one.
   */
  instructions?: string | null;
  note?: string | null;
  proposal_id: string;
}

export interface DefaultsView {
  purpose: Purpose;
  scope: ConfigScope;
  settings: RunSettings;
}

/**
 * Delete a conversation: it disappears and its content (messages, attachments,
 * shared context) is erased. Employee conversations are archived with the employee.
 */
export interface DeleteConversation {
  conversation_id: string;
}

/**
 * Delete an idea for good, with its attachments (feature `idea_attachments`).
 * A task it became stays, with its own copies of the files.
 */
export interface DeleteIdea {
  idea_id: string;
}

/**
 * A board member deletes their own message in a channel or a ticket's discussion
 * (`message.delete`, feature `message_edits`): a line stays that says so. For a
 * message still waiting for its answer see `conversation::DeleteQueuedMessage`.
 * The text stays in the audit log: deleting does not erase a secret.
 */
export interface DeleteMessage {
  message_id: string;
}

/**
 * Delete a user message that still waits for its answer (1.9): its queued
 * run is cancelled and the message is removed. Refused once that run started.
 */
export interface DeleteQueuedMessage {
  conversation_id: string;
  message_id: string;
}

export interface DeleteTaskSchedule {
  schedule_id: string;
}

/**
 * One page of a design: the newest kept HTML file with its name in the conversation.
 */
export interface DesignPageView {
  at_ms: number;
  file: FileRef;
  /**
   * The message the file was kept with (`file.get` reads it by `file.id`).
   */
  message_id: string;
}

/**
 * One design in `design.list`: the kept HTML pages of one conversation of a project
 * (a work session, a ticket's discussion, a task), and where it was made.
 */
export interface DesignView {
  conversation_id: string;
  /**
   * A design session (feature `design_sessions`): listed from its start, also before its first page.
   */
  design?: boolean;
  /**
   * `session`, `ticket`, `task` or `project`: where a client opens the conversation.
   */
  kind: string;
  /**
   * When its newest page was kept.
   */
  last_at_ms: number;
  /**
   * One per file name, in the order the pages first came.
   */
  pages: DesignPageView[];
  project_id: string;
  /**
   * The task of a task's conversation.
   */
  task_id?: string | null;
  /**
   * The ticket of a ticket's discussion, by number.
   */
  ticket_number?: number | null;
  /**
   * The conversation's title (a session's, a ticket's or a task's).
   */
  title: string;
}

/**
 * `design.list`: the designs of one project or of all, the latest first (at most `DESIGNS_MAX`).
 */
export interface DesignsSnapshot {
  as_of_seq: number;
  designs: DesignView[];
}

/**
 * `device.list`: the session's own paired devices, newest first.
 */
export interface DevicesSnapshot {
  devices: RemoteClientView[];
}

export interface DirEntryView {
  name: string;
  path: string;
}

/**
 * `fs.list_dir`: the folders in `path` (never files).
 */
export interface DirListing {
  entries: DirEntryView[];
  parent?: string | null;
  /**
   * The listed folder; `None` when only the starting points are listed.
   */
  path?: string | null;
  /**
   * Starting points: the projects folder, home, drives.
   */
  roots: DirEntryView[];
  /**
   * More folders than were sent.
   */
  truncated: boolean;
}

/**
 * `discussion.audience` (feature `everyone_mention`): whom a board member's
 * `@Everyone` in one discussion would reach, so a client can say so before it
 * sends: it wakes the `coordinator`, the others only know of it. Never the asking
 * board member: they are not told of their own message.
 */
export interface DiscussionAudience {
  /**
   * Board members `@Everyone` tells in the Inbox (and makes follow a ticket).
   */
  board_members: number;
  /**
   * Their usernames.
   */
  board_usernames: string[];
  /**
   * The one employee `@Everyone` wakes, who answers or hands over: the ticket's
   * owner, else the dispatcher, else the CEO, the first that is active. None
   * when there is nobody, or from a server that still wakes all `employees`.
   */
  coordinator?: string | null;
  /**
   * Their names, in the order of the company.
   */
  employee_names: string[];
  /**
   * Active employees in the discussion, who know of an `@Everyone`: it is in what
   * they read when next woken there. It does not wake them (see `coordinator`).
   */
  employees: number;
  /**
   * Employees in the discussion who are paused.
   */
  paused: number;
  /**
   * Their names.
   */
  paused_names: string[];
}

/**
 * Start a work session and let the dispatcher choose who takes it (feature
 * `session_dispatch`), as it chooses the owner of a ticket: the same as
 * `StartWorkSession` without an employee. Until it has chosen, the session's
 * employee is the dispatcher and nothing more can be written in it.
 */
export interface DispatchWorkSession {
  /**
   * See `StartWorkSession::confirm_secret`.
   */
  confirm_secret?: boolean;
  /**
   * See `StartWorkSession::design`.
   */
  design?: boolean;
  /**
   * See `StartWorkSession::files`.
   */
  files?: string[];
  /**
   * See `StartWorkSession::images`.
   */
  images?: string[];
  /**
   * See `StartWorkSession::own_branch`.
   */
  own_branch?: boolean;
  project_id: string;
  /**
   * See `StartWorkSession::run`; it applies to the chosen employee's runs.
   */
  run?: TaskRun;
  text: string;
  /**
   * See `StartWorkSession::worktree`.
   */
  worktree?: boolean;
}

/**
 * A board member changes the text of their own message in a channel or a
 * ticket's discussion (`message.edit`, feature `message_edits`). It wakes nobody.
 * The earlier text stays in the audit log: editing does not erase a secret.
 */
export interface EditMessage {
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  message_id: string;
  text: string;
}

/**
 * One guideline as a run gets it.
 */
export interface EffectiveGuideline {
  chars: number;
  id: string;
  level: GuidelineLevel;
  text: string;
  title: string;
  version: number;
}

/**
 * `guideline.effective`: the set a run of this employee in this project would
 * get, in block order, and how it relates to the limit (B4).
 */
export interface EffectiveGuidelines {
  /**
   * Left out by the safety net at run start (over the limit; employee ones first).
   */
  dropped?: string[];
  employee_id?: string | null;
  guidelines: EffectiveGuideline[];
  limit_chars: number;
  project_id?: string | null;
  total_chars: number;
}

/**
 * One of a board member's lasting chats with an employee (feature `employee_chats`).
 */
export interface EmployeeChatView {
  conversation_id: string;
  employee_id: string;
  employee_name: string;
  last_activity_ms: number;
  /**
   * The start of the latest message, for a list of chats.
   */
  preview?: string | null;
  /**
   * What the employee wrote since the board member last read it.
   */
  unread: number;
}

/**
 * `employee.chats`: the reader's chats that have something in them, the latest first.
 */
export interface EmployeeChatsSnapshot {
  as_of_seq: number;
  chats: EmployeeChatView[];
}

/**
 * One version of an employee's instructions (feature `instruction_proposals`).
 */
export interface EmployeeInstructionVersionView {
  created_at_ms: number;
  /**
   * Empty: the employee had no instructions.
   */
  instructions: string;
  /**
   * The approved proposal this version came from.
   */
  proposal_id?: string | null;
  /**
   * The version a restore put back.
   */
  restored_from?: number | null;
  /**
   * `initial` (as it was when versions began), `created`, `edited` (employee.update),
   * `proposal` (an approved proposal), `restored`.
   */
  source: string;
  version: number;
}

export interface EmployeeInstructionVersionsSnapshot {
  /**
   * The version in use now.
   */
  current_version: number;
  employee_id: string;
  /**
   * Newest first.
   */
  versions: EmployeeInstructionVersionView[];
}

/**
 * `employee.queue`: what is in an employee's mailbox, what runs first.
 */
export interface EmployeeQueue {
  as_of_seq: number;
  employee_id: string;
  items: EmployeeQueueItem[];
}

/**
 * One item of an employee's mailbox (U6; feature `employee_queue`).
 */
export interface EmployeeQueueItem {
  created_at_ms: number;
  hop: number;
  id: string;
  /**
   * message, task, resume, scheduled. Text, not an enum.
   */
  kind: string;
  not_before_ms: number;
  place: ThreadPlace;
  /**
   * The run that works on it; none while it waits.
   */
  run_id?: string | null;
  /**
   * board, employee or system.
   */
  source: string;
  /**
   * claimed (a run works on it), queued or held.
   */
  state: string;
  thread_key: string;
  /**
   * What holds it back besides its turn: paused, question, limit, later, machine or handoff.
   */
  waits_for?: string | null;
}

/**
 * What a new run of one employee uses, as the run itself would resolve it.
 */
export interface EmployeeRunLine {
  /**
   * The effort chosen in the employee's chat; it goes before the rest in its chats.
   */
  chat_effort?: string | null;
  /**
   * The model chosen in the employee's chat; it goes before the rest in its chats.
   */
  chat_model?: string | null;
  /**
   * The display name of the connection, if there is one.
   */
  connection?: string | null;
  /**
   * The effort passed to the run; none: the run gets no effort and the model's own applies.
   */
  effort?: string | null;
  effort_source: RunValueSource;
  employee_id: string;
  model?: string | null;
  /**
   * The model's own effort when none is passed, where that is known.
   */
  model_default_effort?: string | null;
  model_source: RunValueSource;
  name: string;
  /**
   * What is stored for the employee itself: a change sends it back with only the effort changed.
   */
  own: RunSettings;
  status: LifecycleStatus;
  /**
   * The levels this employee's model can use; empty: it has no effort setting.
   */
  supported_efforts: string[];
  /**
   * A chosen effort the model cannot use, so runs leave it out.
   */
  unsupported_effort?: string | null;
}

/**
 * `employee.run_overview` (feature `employee_run_overview`).
 */
export interface EmployeeRunsView {
  /**
   * The stored company-wide default for employee runs.
   */
  company: RunSettings;
  company_model?: string | null;
  company_model_default_effort?: string | null;
  /**
   * The levels the company's default model can use.
   */
  company_supported_efforts: string[];
  employees: EmployeeRunLine[];
}

export interface EmployeeSummary {
  /**
   * What its runs may do with files and commands (C3); left out when `Inherit`.
   */
  access_level?: AccessLevel;
  /**
   * What it is doing now (U6, derived, never stored); left out when idle.
   */
  activity_state?: ActivityState;
  agent_mode: AgentMode;
  /**
   * What runs for this employee right now: its own steps,
   * subagents and sub-tasks delegated to colleagues (derived, never stored).
   */
  agents: AgentView[];
  /**
   * The agent may use the user's browser (shares their logins); off by default.
   */
  browser_allowed: boolean;
  /**
   * Its persistent chat with the user (company scope, no project; `employee.chat`).
   */
  conversation_id: string;
  /**
   * Its chats, most recent first: the persistent one (`conversation_id`, it
   * carries mode and memory summaries) and older ones from before
   * protocol 3 (history). Task conversations are not listed.
   */
  conversations: ConversationSummary[];
  /**
   * What other employees read about it (P7). Left out when it has none: their
   * prompt then shows the first line of its instructions.
   */
  description?: string | null;
  /**
   * Own workspace folder, if any: the work folder of its chat (in a task the project folder applies).
   */
  folder?: string | null;
  id: string;
  instructions?: string | null;
  name: string;
  /**
   * Projects it has open tasks in (derived, never stored).
   */
  projects_active: string[];
  /**
   * Who this employee reports to; none: a root of the org chart (the management team).
   */
  reports_to?: string | null;
  role: string;
  /**
   * The agent may take screenshots without asking each time; off by default (it then asks).
   */
  screen_allowed: boolean;
  /**
   * It works on one task at a time; the others wait their turn.
   */
  sequential_tasks: boolean;
  /**
   * Its own provider, model and effort (C1); left out when it inherits them all.
   * The same as `connection.list` has for its scope, without needing that read.
   */
  settings?: RunSettings | null;
  status: LifecycleStatus;
  title?: string | null;
  /**
   * Its tools, MCP servers and what asks first (C2); left out when it has all of its level.
   */
  tool_policy?: ToolPolicy;
  version: number;
  /**
   * Derived presence: a run of this employee is active (never stored).
   */
  working: boolean;
}

export interface EmployeesSnapshot {
  as_of_seq: number;
  employees: EmployeeSummary[];
}

export type ErrorCode =
  /**
   * A ticket, board or company write needs an active board member on the
   * connection (a session: `user.login`, the first `user.create`, or
   * `HelloParams::session_token`). The second lock behind `unauthenticated`.
   */
  | "board_member_required"
  /**
   * A write in a channel that can only be read (feature `ticket_channels`):
   * its ticket is on Done or archived, or its project is archived. Only a
   * connection whose `HelloParams::accepts` names `ticket_channels` gets this
   * code; any other gets `validation` with the same message, since an older
   * client cannot read a code it does not know.
   */
  | "channel_archived"
  /**
   * The caller lacks the right for this method.
   */
  | "forbidden"
  | "frame_too_large"
  /**
   * `guideline.create` / `update` / `restore` / an approval: another guideline
   * that is not archived has this title (case does not count).
   */
  | "guideline_title_taken"
  /**
   * A guideline write (save, restore, link, approve) would give a run more
   * than 16 000 characters of guidelines (B4): `AppError::too_large` names the
   * combination of employee and project. Nothing was saved.
   */
  | "guideline_too_large"
  | "idempotency_conflict"
  | "incompatible_protocol"
  | "internal"
  | "invalid_request"
  /**
   * `user.create`: 3 to 32 letters, digits, `.`, `_` or `-`.
   */
  | "invalid_username"
  /**
   * `user.disable`: the last active board member cannot be disabled.
   */
  | "last_board_member"
  /**
   * A board member's message (an employee's chat, `ticket.post`, the Team
   * chat) holds the value of a vault secret (B10): nothing was stored.
   * `AppError::matched_items` names the items (never a value); send again
   * with `confirm_secret` to store it with the value redacted.
   */
  | "looks_like_secret"
  | "not_found"
  /**
   * The session's board member signed in with a temporary password (set by
   * another board member): only `user.set_password` (own), `user.logout`,
   * `user.whoami` and `session.hello` work until it is changed.
   */
  | "password_change_required"
  | "shutting_down"
  | "storage_unavailable"
  /**
   * `user.login` / `user.create`: too many tries; `AppError::retry_after_ms`
   * says when the next one is taken ("Too many attempts. Try again in 0:30.").
   */
  | "too_many_attempts"
  /**
   * Protocol 5: this connection has no (valid) session. Everything but
   * `session.hello`, `user.whoami`, `user.login`, `user.list` and the first
   * `user.create` answers this; the client shows its sign-in screen.
   */
  | "unauthenticated"
  /**
   * The server does not know this method (an older server); the connection stays open.
   */
  | "unknown_method"
  /**
   * `user.create`: another board member has this name (case does not count).
   */
  | "username_taken"
  | "validation"
  /**
   * `vault.update`: a secret stays a secret and nothing becomes one (B7);
   * make a new item and archive the old one. Text and url may trade places.
   */
  | "vault_kind_locked"
  /**
   * `vault.reveal` / `vault.key_export`: the server's vault key is missing or
   * does not fit this value (a restore without the key, MU4-04 AC3).
   */
  | "vault_undecryptable"
  | "version_conflict"
  /**
   * `user.create` / `user.set_password`: the password is shorter than 12 characters.
   */
  | "weak_password"
  /**
   * `user.login`: the username or the password is wrong (one answer for both,
   * also for a disabled board member); `user.set_password`: the old password is wrong.
   */
  | "wrong_password"
;

export interface Event {
  /**
   * The task, employee, conversation, … the event is about.
   */
  aggregate_id: string;
  at_ms: number;
  kind: EventKind;
  project_id?: string | null;
  /**
   * Position in the log; replay with `event.subscribe { after_seq }`.
   */
  seq: number;
}

export type EventKind =
  | "approval.allowed_always"
  | "approval.allowed_by_mode"
  | "approval.answered"
  | "approval.approved"
  | "approval.consumed"
  | "approval.expired"
  | "approval.invalidated"
  | "approval.plan_approved"
  | "approval.plan_rejected"
  | "approval.plan_revision_requested"
  | "approval.rejected"
  | "approval.requested"
  | "attachment.added"
  | "attachment.removed"
  | "board.columns_updated"
  | "claude_login.updated"
  | "company.converted"
  | "company.updated"
  | "config.changed"
  | "connection.changed"
  | "conversation.created"
  | "conversation.deleted"
  | "conversation.settings_changed"
  | "conversation.started"
  | "employee.archived"
  | "employee.config_changed"
  | "employee.created"
  | "employee.folder_set"
  | "employee.instructions_proposed"
  | "employee.paused"
  | "employee.proposal_decided"
  | "employee.proposed"
  | "employee.reports_to_set"
  | "employee.resumed"
  | "folder.changed"
  | "guideline.archived"
  | "guideline.created"
  | "guideline.linked"
  | "guideline.proposal_decided"
  | "guideline.proposed"
  | "guideline.restored"
  | "guideline.unlinked"
  | "guideline.updated"
  | "host.write"
  | "idea.added"
  | "idea.deleted"
  | "idea.started"
  | "idea.status_changed"
  | "idea.ticket_created"
  | "idea.updated"
  | "mcp.auth_changed"
  | "mcp.login_finished"
  | "memory.accepted"
  | "memory.consolidated"
  | "memory.consolidation_requested"
  | "memory.extracted"
  | "memory.forgotten"
  | "memory.pinned"
  | "memory.proposed"
  | "memory.reinforced"
  | "memory.rejected"
  | "memory.remembered"
  | "memory.revised"
  | "memory.summarized"
  | "memory.superseded"
  | "message.assistant_committed"
  | "message.deleted"
  | "message.edited"
  | "message.interrupted"
  | "message.reacted"
  | "message.sent_now"
  | "message.user_committed"
  | "project.archived"
  | "project.created"
  | "project.folder_set"
  | "project.follow_org_chart"
  | "project.paused"
  | "project.updated"
  | "recurring_task.changed"
  | "run.cancel_requested"
  | "run.cancelled"
  | "run.checkpointed"
  | "run.completed"
  | "run.failed"
  | "run.queued"
  | "run.source_fetched"
  | "run.started"
  | "run.usage_reported"
  | "run.waiting"
  | "server.renamed"
  | "settings.follow_org_chart"
  | "task.assigned"
  | "task.cancelled"
  | "task.created"
  | "task.delegated"
  | "task.failed"
  | "task.handed_off"
  | "task.job_requeued"
  | "task.mode_chosen"
  | "task.posted"
  | "task.queued"
  | "task.report_ready"
  | "task.resumed"
  | "task.run_changed"
  | "task.started"
  | "task.succeeded"
  | "task.waiting"
  | "ticket.accepted"
  | "ticket.archived"
  | "ticket.cancelled"
  | "ticket.created"
  | "ticket.dispatched"
  | "ticket.follow_changed"
  | "ticket.moved"
  | "ticket.plan_approved"
  | "ticket.plan_review"
  | "ticket.plan_revised"
  | "ticket.review"
  | "ticket.sent_back"
  | "ticket.started"
  | "ticket.tested"
  | "ticket.updated"
  | "user.created"
  | "user.disabled"
  | "vault.item_archived"
  | "vault.item_created"
  | "vault.item_updated"
  | "wake.requested"
;

/**
 * A file kept with a message: what an agent made or mentioned (a screenshot,
 * a report, a log). Images show in the app; other files open on the device.
 */
export interface FileRef {
  id: string;
  media_type: string;
  name: string;
  size: number;
  /**
   * An HTML design's own `<title>`, so clients show a design by its title instead of its
   * file name. Absent on other files, on a page without a title and from older servers.
   */
  title?: string | null;
}

/**
 * Whose work folder a file query looks in.
 */
export type FileScope =
  /**
   * Where a conversation works: a task's (its workspace, else the project
   * folder), a project conversation's (the project folder), an employee
   * chat's (the employee's own folder).
   */
  | ({ kind: "conversation"; conversation_id: string })
  /**
   * The project's folder.
   */
  | ({ kind: "project"; project_id: string })
  /**
   * A task's own workspace, else where its conversation works.
   */
  | ({ kind: "task"; task_id: string })
;

/**
 * The board member of the session starts (`on`) or stops following a ticket
 * (`ticket.follow`, feature `ticket_follow`).
 */
export interface FollowTicket {
  on: boolean;
  ticket_id: string;
}

/**
 * One board member who follows a ticket (`ticket.followers`, feature `ticket_follow`).
 */
export interface FollowerView {
  /**
   * A disabled board member stays listed but gets no notifications.
   */
  disabled: boolean;
  followed_at_ms: number;
  /**
   * Why they follow: `created`, `responsible`, `mentioned` or `manual`. Text,
   * not an enum, so a new reason never breaks an older client.
   */
  reason: string;
  user_id: string;
  username: string;
}

/**
 * Remove from all future context and the search index; content is erased.
 */
export interface ForgetMemory {
  memory_id: string;
}

/**
 * `guideline.affected`: who a change reaches, for "applies from the next run
 * of N employees" (B6). A run gets it at its next start; a conversation whose
 * session had another version starts a new session then.
 */
export interface GuidelineAffected {
  /**
   * Conversations whose current session holds a version of it (they start a new session).
   */
  conversations: number;
  /**
   * Active employees that get it: linked themselves, or working in a linked project.
   */
  employees: number;
  guideline_id: string;
  /**
   * Linked projects that are not archived.
   */
  projects: number;
}

/**
 * `guideline.get`: the guideline as the list shows it, plus one version with
 * its text (the current one unless a `version` was asked).
 */
export interface GuidelineDetail {
  guideline: GuidelineView;
  version: GuidelineVersionView;
}

/**
 * The level a guideline has in a run's block (B3: employee first, then project;
 * a guideline linked to both is in it once, at project level).
 */
export type GuidelineLevel =
  | "employee"
  | "project"
;

/**
 * A proposal from `propose_guideline` (`guideline.proposals`).
 */
export interface GuidelineProposalView {
  /**
   * The version the proposal rests on (None for a new guideline).
   */
  base_version?: number | null;
  created_at_ms: number;
  decided_at_ms?: number | null;
  decided_by?: UserRef | null;
  decision_note?: string | null;
  evidence?: ProposalEvidence[];
  /**
   * None: a new guideline.
   */
  guideline_id?: string | null;
  id: string;
  origin_conversation_id?: string | null;
  proposed_by: string;
  reason: string;
  run_id?: string | null;
  status: ProposalStatus;
  /**
   * For a new guideline: the links the proposer suggests (themselves and the
   * run's project by default); the board member may change them.
   */
  suggested_links?: GuidelineTarget[];
  text: string;
  title: string;
}

/**
 * `guideline.proposals`: pending first, then the latest decisions.
 */
export interface GuidelineProposals {
  proposals: GuidelineProposalView[];
}

/**
 * How a version came about.
 */
export type GuidelineSource =
  | "created"
  | "edited"
  /**
   * An approved `propose_guideline`.
   */
  | "proposal"
  /**
   * `guideline.restore_version`: a copy of an older version as the newest.
   */
  | "restored"
  /**
   * The starter set (B7).
   */
  | "seed"
;

/**
 * What a guideline is linked to: an employee or a project, never both (B2).
 */
export type GuidelineTarget =
  | ({ kind: "employee"; id: string })
  | ({ kind: "project"; id: string })
;

/**
 * `guideline_too_large` (B4): a write would give one combination of an
 * employee and a project (or a project alone, or an employee without a
 * project) more than `limit` characters of guidelines. The first combination
 * that overflows; nothing was saved.
 */
export interface GuidelineTooLarge {
  chars: number;
  employee_id?: string | null;
  limit: number;
  project_id?: string | null;
}

/**
 * One version with its text (`guideline.get`, `guideline.versions`).
 */
export interface GuidelineVersionView {
  /**
   * None: the seed.
   */
  by?: UserRef | null;
  created_at_ms: number;
  guideline_id: string;
  proposal_id?: string | null;
  restored_from?: number | null;
  source: GuidelineSource;
  text: string;
  title: string;
  version: number;
}

/**
 * `guideline.versions`: every version, newest first.
 */
export interface GuidelineVersions {
  guideline_id: string;
  versions: GuidelineVersionView[];
}

/**
 * One guideline in the library list (`guideline.list`), without its text.
 */
export interface GuidelineView {
  archived?: boolean;
  /**
   * Characters of the current text.
   */
  chars: number;
  id: string;
  links: GuidelineTarget[];
  /**
   * Pending proposals for this guideline.
   */
  open_proposals?: number;
  title: string;
  updated_at_ms: number;
  updated_by?: UserRef | null;
  /**
   * The current version (the envelope's `expected_version`).
   */
  version: number;
}

/**
 * `guideline.list`: the library, without texts. Archived ones only with `archived`.
 */
export interface GuidelinesSnapshot {
  guidelines: GuidelineView[];
}

export interface HealthResult {
  active_jobs: number;
  daemon_version: string;
  host_id: string;
  last_seq: number;
  pid: number;
  started_at_ms: number;
}

export interface HelloParams {
  /**
   * What this client can take that older clients cannot (`client_feature`).
   * Servers without such a feature ignore the name.
   */
  accepts?: string[];
  client_name: string;
  client_version: string;
  /**
   * The session this device got from `user.login` (or the first
   * `user.create`) for this server: valid, it makes its board member this
   * connection's (`HelloResult::user`); unknown, revoked or expired, the
   * hello still works without a board member and the client shows its
   * sign-in screen. Ignored for apps. Never printed.
   */
  session_token?: string | null;
}

export interface HelloResult {
  /**
   * The server has at least one board member; false: the onboarding
   * ("Create the first board member") is all a client shows.
   */
  board_members_exist?: boolean;
  capabilities: Capability[];
  /**
   * The session's own Claude login; none without a session.
   */
  claude_login?: ClaudeLoginView | null;
  /**
   * The paired device behind a remote connection.
   */
  client_id?: string | null;
  daemon_version: string;
  /**
   * `dev` or `production`; a client never talks to the other environment.
   */
  environment: string;
  /**
   * Additions to protocol 3.0 this server has (see `FEATURES`); a client hides
   * what is missing, since an unknown method is not a clean error. Older servers leave it out.
   */
  features?: string[];
  host_id: string;
  /**
   * The server's computer name.
   */
  host_name: string;
  /**
   * The session's board member must change a temporary password first.
   */
  must_change_password?: boolean;
  /**
   * The server's OS: `windows`, `macos` or `linux`.
   */
  os: string;
  protocol: number;
  /**
   * This connection came in over the network.
   */
  remote: boolean;
  /**
   * The board member of this connection's session (`HelloParams::session_token`
   * when it is valid); none: the client shows its sign-in screen (or the
   * onboarding without board members).
   */
  user?: UserView | null;
}

/**
 * One finished ticket in the History.
 */
export interface HistoryEntry {
  /**
   * When the ticket went to Done (for a quick task: after its merge).
   */
  done_at_ms: number;
  number: number;
  /**
   * The ticket's owner (an archived employee keeps its name); none: "—".
   */
  owner_employee_id?: string | null;
  owner_name?: string | null;
  project_id: string;
  project_name: string;
  /**
   * What the ticket delivered (through the vault redactor), when `summary_state` is `ready`.
   */
  summary?: string | null;
  summary_state: HistorySummaryState;
  ticket_id: string;
  title: string;
}

/**
 * `ticket.history`: finished tickets, newest first.
 */
export interface HistoryPage {
  as_of_seq: number;
  entries: HistoryEntry[];
  /**
   * There are older rows: ask again with the last row's `done_at_ms` and `number`.
   */
  has_more?: boolean;
}

/**
 * Where a row's summary stands, as the History shows it.
 */
export type HistorySummaryState =
  /**
   * The ticket went to Done before the History existed and its project memory
   * has no summary of it ("No summary (finished before History)").
   */
  | "before_history"
  /**
   * It failed or gave nothing ("Summary not available"); a board member may try again.
   */
  | "not_available"
  /**
   * `summary` holds the text.
   */
  | "ready"
  /**
   * The extraction is queued or running ("Summarizing…").
   */
  | "summarizing"
;

/**
 * What a host write answers.
 */
export type HostResult =
  | ({ type: "auto_allow_setting" } & AutoAllowSetting)
  | ({ type: "backups" } & BackupsSnapshot)
  | ({ type: "browser" } & BrowserSetting)
  /**
   * `claude_login.cancel` / `claude_login.unlink`: the login as it is now.
   */
  | ({ type: "claude_login" } & ClaudeLoginView)
  /**
   * `claude_login.link`: the link that runs; its prompts follow as
   * `ServerMessage::ClaudeLoginPrompt` with this id.
   */
  | ({ type: "claude_login_link"; link_id: string })
  /**
   * `code.*`: the project's git state after the write.
   */
  | ({ type: "code" } & CodeView)
  | ({ type: "compaction_setting" } & CompactionSetting)
  /**
   * Notifications changed.
   */
  | ({ type: "count"; count: number })
  | ({ type: "dir_listing" } & DirListing)
  | ({ type: "done" })
  | ({ type: "exported"; json_path: string; markdown_path: string })
  | ({ type: "load" } & LoadSnapshot)
  | ({ type: "management_mode_setting" } & ManagementModeSetting)
  | ({ type: "mcp_always_allow" } & McpAlwaysAllow)
  | ({ type: "mcp_login" } & McpLogin)
  | ({ type: "mcp_servers" } & McpServers)
  /**
   * A replayed receipt has empty codes.
   */
  | ({ type: "pairing_code" } & PairingCode)
  | ({ type: "power" } & PowerState)
  | ({ type: "remote" } & RemoteStatus)
  | ({ type: "shutting_down" })
  | ({ type: "trace_setting" } & TraceSetting)
  | ({ type: "update" } & UpdateStatus)
  /**
   * `user.disable`: the board member as it is now.
   */
  | ({ type: "user" } & UserView)
  /**
   * `vault.create` / `vault.update` / `vault.archive`: the item as it is now.
   * A replayed receipt has no value: read it with `vault.list` or `vault.reveal`.
   */
  | ({ type: "vault_item" } & VaultItemView)
;

/**
 * Writes only the host can do. Secrets in parameters are never printed.
 */
export type HostWrite =
  /**
   * Auto-allow of the safe read-only tools on or off, from the next tool call. On by default.
   */
  | ({ method: "auto_allow.set_enabled"; params: { enabled: boolean } })
  | ({ method: "backup.create" })
  /**
   * Restore at the next start (the service stops now); work stays paused until resumed.
   */
  | ({ method: "backup.restore"; params: { id: string } })
  | ({ method: "backup.resume" })
  | ({ method: "browser.set"; params: { choice: BrowserChoice } })
  /**
   * Claude in Chrome or Playwright, for every run that starts afterwards.
   */
  | ({ method: "browser.set_backend"; params: { backend: BrowserBackend } })
  /**
   * Stop a running link of the session's own login. Answers `ClaudeLogin`.
   */
  | ({ method: "claude_login.cancel"; params: { link_id: string } })
  /**
   * Connect the session's own Claude login: the server starts `claude auth
   * login` for `<data>/logins/<user_id>/` and answers `ClaudeLoginLink`. What
   * Claude asks (the sign-in link, the code) comes to this connection only as
   * `ServerMessage::ClaudeLoginPrompt`; the code goes back with
   * `claude_login.input`. One link per board member at a time, 10 minutes.
   */
  | ({ method: "claude_login.link" })
  /**
   * Disconnect the session's own Claude login (`claude auth logout`, its folder
   * removed). Answers `ClaudeLogin`.
   */
  | ({ method: "claude_login.unlink" })
  /**
   * Delete a local branch (not main, not one checked out in a worktree, not one an
   * employee works on now). A branch with commits main lacks needs `force` (the
   * client asked the board member first: that work is gone). Answers `Code`.
   */
  | ({ method: "code.delete_branch"; params: { branch: string; force?: boolean; project_id: string } })
  /**
   * `git fetch --prune` of the project's repository (feature `code_view`, ticket #77);
   * answers `HostResult::Code`. Never a prompt for credentials: a remote that wants
   * one fails with git's reason.
   */
  | ({ method: "code.fetch"; params: { project_id: string } })
  /**
   * Merge a local branch into the project's main (`--no-ff`), in the project's folder.
   * Refused when the folder is not on main, main has uncommitted changes, is in another
   * merge, or the branch has nothing ahead; a conflict is aborted, so main stays as it
   * was (`validation` with git's reason and the conflicting files). Answers `Code`.
   */
  | ({ method: "code.merge"; params: { branch: string; project_id: string } })
  /**
   * Clean up merged: remove every clean worktree of a merged branch nobody works on,
   * then delete every merged branch without a worktree (never main), then
   * `git worktree prune`. Answers `Code`.
   */
  | ({ method: "code.prune_merged"; params: { project_id: string } })
  /**
   * Remove a worktree (not the project's folder). Refused when it has uncommitted
   * changes or an employee works in it now. The branch stays. Answers `Code`.
   */
  | ({ method: "code.remove_worktree"; params: { path: string; project_id: string } })
  /**
   * Above how many tokens of context a session is compacted before its next turn
   * (feature `compaction_setting`), from the next run on; 0: never.
   */
  | ({ method: "compaction.set_threshold"; params: { threshold_tokens: number } })
  /**
   * Remove one of the session's own paired devices (feature `own_devices`): its
   * sessions end and its connections close. Another board member's device is
   * `not_found`. Answers `Done`.
   */
  | ({ method: "device.revoke"; params: { client_id: string } })
  /**
   * A new folder `name` in the folder `parent`.
   */
  | ({ method: "fs.create_dir"; params: { name: string; parent: string } })
  | ({ method: "load.set_limits"; params: LoadLimitsUpdate })
  /**
   * Management chooses its own task mode (feature `management_chooses_mode`), for every run
   * that starts afterwards. On by default.
   */
  | ({ method: "management_mode.set_enabled"; params: { enabled: boolean } })
  | ({ method: "mcp.add"; params: McpAddParams })
  /**
   * Finishes a login with the address the browser ended on (`…/callback?code=…&state=…`),
   * for a browser that is not on the server's computer.
   */
  | ({ method: "mcp.login_complete"; params: { login_id: string; redirect_url: string } })
  /**
   * Starts an OAuth login for an http/sse server; answers `McpLogin` with the URL to open.
   */
  | ({ method: "mcp.login_start"; params: { name: string } })
  /**
   * Forgets the server's tokens (keeps its client registration).
   */
  | ({ method: "mcp.logout"; params: { name: string } })
  | ({ method: "mcp.remove"; params: { name: string } })
  | ({ method: "mcp.set"; params: { enabled: string[] } })
  /**
   * Take a tool off the always-allow list: its next call asks again.
   */
  | ({ method: "mcp_always_allow.remove"; params: { tool: string } })
  | ({ method: "notification.dismiss"; params: { id: string } })
  | ({ method: "notification.dismiss_all" })
  | ({ method: "notification.mark_all_read" })
  /**
   * One row read, for this board member only (feature `personal_inbox`); answers
   * `Count`, 0 when there was nothing to read, so it can be repeated.
   */
  | ({ method: "notification.mark_read"; params: { id: string } })
  | ({ method: "power.cancel" })
  | ({ method: "power.shutdown"; params: { when: ShutdownWhen } })
  /**
   * Write a readable Markdown + JSON export of a project into its folder.
   */
  | ({ method: "project.export"; params: { project_id: string } })
  | ({ method: "push.configure"; params: PushConfig })
  | ({ method: "push.test"; params: { client_id: string } })
  /**
   * Allow or refuse a device that entered the code. A computer that asks for
   * full access is only allowed with `owner` (the user confirmed it).
   */
  | ({ method: "remote.decide_pairing"; params: { approve: boolean; owner: boolean; pairing_id: string } })
  | ({ method: "remote.disable" })
  | ({ method: "remote.enable" })
  | ({ method: "remote.revoke"; params: { client_id: string } })
  /**
   * A new one-time code (5 minutes) plus the QR payload; the receipt keeps neither.
   */
  | ({ method: "remote.start_pairing" })
  | ({ method: "secret.set"; params: { connection_id: string; secret: string } })
  | ({ method: "server.restart" })
  /**
   * End one session (any board member's: one role). Its device's open
   * connection closes at once (`ServerMessage::SignedOut`); the timeline says
   * who revoked it. Answers `Done`. Not for apps.
   */
  | ({ method: "session.revoke"; params: { session_id: string } })
  /**
   * Trace mode on or off for every run that starts afterwards (feature `run_trace`).
   * Off also forgets the traces kept so far.
   */
  | ({ method: "trace.set_enabled"; params: { enabled: boolean } })
  | ({ method: "update.apply" })
  | ({ method: "update.apply_app" })
  | ({ method: "update.prepare" })
  | ({ method: "update.prepare_app" })
  /**
   * A new board member (OV-12: `invalid_username`, `weak_password` (also a
   * password of only spaces), `username_taken`; `too_many_attempts` after five
   * a minute on one connection or twenty an hour on the server); answers
   * `CommandResult::UserCreated` as a command receipt. Without a session only
   * the first board member (the onboarding): that connection then gets a
   * session, whose token `user.login` would give (see `HelloResult::user`;
   * the client signs in once more to get the token). After that it needs a
   * session (any board member, one role). The server hashes the password
   * before anything is recorded: the receipt, its request hash and the event
   * `user.created` hold the name only. Not for apps.
   */
  | ({ method: "user.create"; params: UserCreateParams })
  /**
   * Disable a board member: their sessions end at once and they cannot sign
   * in; their Claude login is disconnected; tickets keep their creator. The
   * last active board member cannot be disabled (`last_board_member`).
   * Answers `User`. Not for apps.
   */
  | ({ method: "user.disable"; params: { user_id: string } })
  /**
   * Change a password. Own (no `user_id`, or the session's): with the old
   * password; the other sessions end, this one stays. Another board member's
   * (B10): a temporary password, changed at their next sign-in
   * (`must_change_password`); all their sessions end. Like `user.create`, the
   * passwords are in no receipt, request hash, event or log. Answers `Done`.
   */
  | ({ method: "user.set_password"; params: SetPasswordParams })
  /**
   * Archive ("Delete") an item: runs and employees no longer get it; its audit
   * history stays. Answers `VaultItem`. Not for apps.
   */
  | ({ method: "vault.archive"; params: { item_id: string } })
  /**
   * A new vault item (feature `vault`), in the company (no `project_id`) or a
   * project. Answers `VaultItem` (`value` only for text/url). The server
   * encrypts the value before anything is recorded: the receipt, its request
   * hash and the event `vault.item_created` hold no value (MU4-05 AC2).
   * `validation` for a name or env name used in the scope, an env name that
   * is not UPPER_SNAKE or that the runtime reserves (`PATH`, `ANTHROPIC_*`,
   * …), a url without `https://`/`http://`, a secret under 8 characters.
   * Board members with a session only (employees and agent tools never); not for apps.
   */
  | ({ method: "vault.create"; params: VaultCreateParams })
  /**
   * Change an item; what is left out stays. `value`: a new value (a secret's
   * is never sent back to the editor, so "leave empty to keep"). `kind` only
   * between text and url: to or from secret is `vault_kind_locked` (B7).
   * With `expected_version` in the envelope a stale edit is `version_conflict`.
   * Answers `VaultItem`. As `vault.create`: no value in receipt, hash or event.
   */
  | ({ method: "vault.update"; params: VaultUpdateParams })
;

/**
 * What an idea is about: something new or better, or something broken.
 */
export type IdeaKind =
  | "bug"
  | "feature"
;

export type IdeaStatus =
  /**
   * Done: it became a task, or the user marked it done.
   */
  | "done"
  /**
   * Not doing it.
   */
  | "dropped"
  /**
   * Still an idea.
   */
  | "open"
;

/**
 * One idea: noted by the user or an employee; it may have become a task.
 */
export interface IdeaView {
  /**
   * Files attached to the idea, each with an `id` for `upload.get` (feature
   * `idea_attachments`); empty, and left out on older servers.
   */
  attachments?: MessageFileView[];
  /**
   * "You", or the employee who noted it.
   */
  author: string;
  created_at_ms: number;
  /**
   * The employee who noted it; None when the user did.
   */
  employee_id?: string | null;
  id: string;
  /**
   * Feature or bug;
   */
  kind: IdeaKind;
  note?: string | null;
  project_id: string;
  status: IdeaStatus;
  /**
   * The task it became, and that task's title and status.
   */
  task_id?: string | null;
  task_status?: TaskStatus | null;
  /**
   * The ticket made from it and that ticket's number (feature `ticket_from_idea`);
   * left out when there is none.
   */
  ticket_id?: string | null;
  ticket_number?: number | null;
  title: string;
  updated_at_ms: number;
}

/**
 * A project's ideas.
 */
export interface IdeasSnapshot {
  ideas: IdeaView[];
  project_id: string;
}

export type JobState =
  | "cancelled"
  | "dead"
  | "done"
  | "leased"
  | "ready"
;

export interface JobSummary {
  /**
   * What it works on: the conversation of an answer, or the task.
   */
  conversation_id?: string | null;
  created_at_ms: number;
  id: string;
  kind: string;
  label: string;
  /**
   * While it runs: since when and the tokens so far.
   */
  live?: RunProgress | null;
  /**
   * A ready job that waits: until when (none when it waits for the user
   * without a time, e.g. "auth"), and why ("quota", "auth", "transient", ...).
   */
  not_before_ms?: number | null;
  progress_permille: number;
  reason?: string | null;
  state: JobState;
  task_id?: string | null;
}

export interface JobsSnapshot {
  as_of_seq: number;
  jobs: JobSummary[];
}

/**
 * `vault.key_export`. The password is never printed.
 */
export interface KeyExportParams {
  /**
   * The signed-in board member's own password, asked again.
   */
  password: string;
}

export interface LessonEvidence {
  /**
   * S1..S8 as the extraction saw it.
   */
  label: string;
  text: string;
}

/**
 * A lesson's place in `locator_json.lesson`: when it applies, its evidence
 * (label and signal text), how often later tasks confirmed it and from which tasks.
 */
export interface LessonLocator {
  applies_when: string;
  evidence?: LessonEvidence[];
  /**
   * How often a later task found the same lesson again.
   */
  reinforced?: number;
  tasks?: string[];
}

export type LifecycleStatus =
  | "active"
  | "archived"
  | "paused"
;

/**
 * What a finished task's follow-up run does (`TaskView::live_run`).
 */
export type LiveRun =
  /**
   * Its run waits in the queue and has not been claimed yet.
   */
  | "queued"
  /**
   * A run is going (created, starting, running, waiting or cancelling).
   */
  | "running"
;

export type LoadGate =
  | "held"
  | "open"
;

export interface LoadHold {
  cpu_pct?: number | null;
  kind: LoadHoldKind;
  mem_free_mb?: number | null;
  running?: number | null;
}

export type LoadHoldKind =
  | "cpu"
  /**
   * `max_runs` is 0: every new run waits.
   */
  | "disabled_all"
  | "max_runs"
  | "memory"
;

/**
 * When a new run may start.
 */
export interface LoadLimits {
  /**
   * Hold new runs from this CPU use (%)…
   */
  cpu_high: number;
  /**
   * …until it is below this again.
   */
  cpu_low: number;
  /**
   * Whether CPU and memory count (the run limit always does).
   */
  enabled: boolean;
  /**
   * Runs at the same time; 0 holds every new run.
   */
  max_runs: number;
  mem_min_mb: number;
}

/**
 * `load/set_limits`: `None` leaves a limit as it is.
 */
export interface LoadLimitsUpdate {
  cpu_high?: number | null;
  cpu_low?: number | null;
  enabled?: boolean | null;
  max_runs?: number | null;
  mem_min_mb?: number | null;
}

/**
 * The machine's load and the run admission (`load.get`, `load_changed`).
 */
export interface LoadSnapshot {
  /**
   * Whether CPU and memory are measured at all (only for the standard data
   * folder; elsewhere only `max_runs` counts).
   */
  checks_active: boolean;
  /**
   * All cores together, 0–100, averaged over ~10 s.
   */
  cpu_pct: number;
  gate: LoadGate;
  /**
   * Runs that could start but wait for the machine.
   */
  held: number;
  /**
   * Why new runs wait (`gate` is `held`).
   */
  hold?: LoadHold | null;
  limits: LoadLimits;
  mem_free_mb: number;
  mem_total_mb: number;
  /**
   * Runs going now (chat answers and task starts).
   */
  running: number;
  sampled_at_ms: number;
}

/**
 * `user.login`. The password is never printed.
 */
export interface LoginParams {
  /**
   * How the device shows in `session.list` ("Desktop · dev-laptop").
   */
  device_name: string;
  password: string;
  username: string;
}

/**
 * `user.login`: what the client keeps for the server. The token is shown
 * here once; the server keeps only its SHA-256. Never printed.
 */
export interface LoginResult {
  /**
   * When the session ends at the latest (90 days); 30 days without use end it earlier.
   */
  expires_at_ms: number;
  /**
   * A temporary password: change it with `user.set_password` first.
   */
  must_change_password: boolean;
  /**
   * Give it in every hello (`HelloParams::session_token`) on this device.
   */
  session_token: string;
  user: UserView;
}

/**
 * `management_mode.get_setting` and `management_mode.set_enabled` (feature
 * `management_chooses_mode`): whether management (the CEO and their direct reports) chooses
 * its own task mode, Auto or Plan, when the user picked none. On by default.
 */
export interface ManagementModeSetting {
  enabled: boolean;
}

export interface MarkdownFileContent {
  byte_length: number;
  modified_ms: number;
  /**
   * Relative to the work folder, with `/` (also when asked by absolute path).
   */
  path: string;
  text: string;
}

export interface MarkdownFileEntry {
  byte_length: number;
  modified_ms: number;
  /**
   * Relative to the work folder, with `/`.
   */
  path: string;
}

export interface MarkdownFileList {
  /**
   * Sorted by path.
   */
  files: MarkdownFileEntry[];
  /**
   * The work folder that was listed (empty when there is none on the server).
   */
  root: string;
  /**
   * More files than `MARKDOWN_MAX_FILES` exist; the rest is left out.
   */
  truncated: boolean;
}

/**
 * What `mcp/add` sends. Env and header values are secrets: `Debug`
 * never prints them and the server never returns them.
 */
export interface McpAddParams {
  args?: string[];
  /**
   * Stdio: the program to run.
   */
  command?: string | null;
  /**
   * Also switch it on for every run.
   */
  enable?: boolean;
  env?: { [key: string]: string };
  headers?: { [key: string]: string };
  name: string;
  transport: McpTransport;
  /**
   * Http and sse: the endpoint.
   */
  url?: string | null;
}

/**
 * `mcp_always_allow.list` and `.remove`: the MCP tools (`mcp__<server>__<tool>`)
 * the user answered with 'Always allow'. They pass without asking in every run, in
 * Ask, Accept edits and Auto; Plan mode still asks.
 */
export interface McpAlwaysAllow {
  tools: string[];
}

/**
 * The OAuth state of an MCP server, read from Claude Code's own credential store.
 */
export type McpAuthStatus =
  /**
   * Claude Code has a usable token (it refreshes it itself).
   */
  | "authorized"
  /**
   * The server asks for a login (`mcp.login_start`).
   */
  | "needs_login"
  /**
   * No OAuth: stdio, or a fixed `Authorization` header.
   */
  | "none"
  /**
   * Not known yet (never connected); a login can still be tried.
   */
  | "unknown"
;

/**
 * `mcp.login_start`, `mcp.login_complete` and `mcp.login_get`. Never holds a token.
 */
export interface McpLogin {
  /**
   * Open this in a browser. When the browser runs on the server's computer the
   * login finishes by itself; elsewhere, the browser ends on a page that does
   * not load: send that page's address to `mcp.login_complete`.
   */
  authorization_url: string;
  /**
   * Why it failed.
   */
  error?: string | null;
  expires_at_ms: number;
  login_id: string;
  /**
   * The MCP server.
   */
  name: string;
  /**
   * The `http://localhost:<port>/callback` address the browser is sent back to.
   */
  redirect_uri: string;
  state: McpLoginState;
}

/**
 * Where a login started with `mcp.login_start` stands.
 */
export type McpLoginState =
  /**
   * Not finished in time; start again.
   */
  | "expired"
  | "failed"
  /**
   * Waiting for the browser (the server's own callback) or for `mcp.login_complete`.
   */
  | "pending"
  | "succeeded"
;

/**
 * Where Claude Code keeps an MCP server the user configured.
 */
export type McpScope =
  /**
   * `~/.claude.json`, for all projects.
   */
  | "user"
;

/**
 * One MCP server of the user. Never holds env values, headers or tokens.
 */
export interface McpServerInfo {
  /**
   * Whether it needs an OAuth login (http/sse); older servers leave it out (`unknown`).
   */
  auth?: McpAuthStatus;
  /**
   * Every run uses it.
   */
  enabled: boolean;
  /**
   * The name the user gave it; it is the key for `mcp.set`.
   */
  name: string;
  scope: McpScope;
  /**
   * The command (without arguments) or the URL (without credentials or query).
   */
  summary: string;
  transport: McpTransport;
}

/**
 * `mcp.list` and the MCP writes.
 */
export interface McpServers {
  /**
   * Enabled names that Claude Code no longer has; they are skipped in runs.
   */
  missing: string[];
  servers: McpServerInfo[];
}

export type McpTransport =
  | "http"
  | "sse"
  | "stdio"
;

export interface MemoryContextItem {
  content: string;
  employee_name?: string | null;
  kind: MemoryKind;
  /**
   * The label the context builder writes, e.g. "Decision · project".
   */
  label: string;
  memory_id: string;
  pinned: boolean;
  /**
   * always | when_relevant | over_budget | duplicate
   */
  placement: string;
  /**
   * global | project | employee
   */
  scope: string;
  /**
   * Tokens of the line as it goes into the system text.
   */
  tokens: number;
}

/**
 * What goes into an employee's context without a specific question.
 */
export interface MemoryContextView {
  always_tokens: number;
  employee_id?: string | null;
  items: MemoryContextItem[];
  memory_budget_tokens: number;
  /**
   * None: an employee's chat outside projects.
   */
  project_id?: string | null;
  /**
   * Estimate of the fixed system text without memory; 0 when not known.
   */
  system_tokens: number;
}

export type MemoryKind =
  | "conversation_summary"
  | "decision"
  | "episodic"
  | "identity"
  | "project_knowledge"
;

/**
 * Where a memory lives. Global only holds what was explicitly made global.
 */
export type MemoryScope =
  | ({ kind: "employee"; id: string })
  | ({ kind: "global" })
  | ({ kind: "project"; id: string })
;

export interface MemorySnapshot {
  items: MemoryView[];
}

export interface MemorySourceView {
  at_ms: number;
  event_id: string;
  label: string;
}

export type MemoryStatus =
  | "active"
  | "forgotten"
  | "proposed"
  | "rejected"
  | "superseded"
;

export interface MemoryView {
  confidence: number;
  conflicts_with_id?: string | null;
  /**
   * Empty when forgotten (content erased).
   */
  content: string;
  created_at_ms: number;
  /**
   * user | policy | None (still proposed)
   */
  decided_by?: string | null;
  employee_id?: string | null;
  employee_name?: string | null;
  id: string;
  importance: number;
  kind: MemoryKind;
  /**
   * Set when this memory is a lesson learned from a task (feature
   * `task_lessons`): when it applies, its evidence and how often it was
   * confirmed. The kind is then `episodic`; show it as "Lesson".
   */
  lesson?: LessonLocator | null;
  pinned: boolean;
  /**
   * global | project | employee
   */
  scope: string;
  sources: MemorySourceView[];
  status: MemoryStatus;
  supersedes_id?: string | null;
}

/**
 * A file the user sent to an employee, copied into its work folder.
 */
export interface MessageFileView {
  byte_length: number;
  /**
   * Fetch it with `upload.get`. Given by the server when it reads the file's
   * message, task or recurring task (never stored); older servers leave it out.
   */
  id?: string | null;
  name: string;
  /**
   * Relative to the work folder, with `/`, e.g. `.aurelius/uploads/report.pdf`.
   */
  path: string;
}

/**
 * `message.history` (feature `message_edits`): the earlier texts of a message,
 * newest first, each through the vault redactor when it goes out.
 */
export interface MessageHistoryView {
  deleted: boolean;
  /**
   * Always empty when `deleted`: a deleted text is gone in every client, even
   * though the audit log keeps it.
   */
  versions: MessageVersionView[];
}

/**
 * `message.locate` (feature `message_locate`): where a message of a channel or
 * a ticket's discussion is, so a link to it can open the right place.
 */
export interface MessageLocation {
  conversation_id: string;
  message_id: string;
  /**
   * The message that starts the thread it replies in; none for a message of
   * the channel itself (also for a thread's root).
   */
  thread_root?: string | null;
  /**
   * The ticket whose discussion it is in; none for a channel.
   */
  ticket_id?: string | null;
  /**
   * The number of `ticket_id` (`#12`).
   */
  ticket_number?: number | null;
}

export type MessageRole =
  | "assistant"
  | "user"
;

export type MessageStatus =
  | "complete"
  | "failed"
  | "interrupted"
  | "partial"
;

/**
 * One earlier text of an edited message (`MessageHistoryView::versions`).
 */
export interface MessageVersionView {
  /**
   * When an edit replaced this text.
   */
  replaced_at_ms: number;
  text: string;
}

export interface MessageView {
  /**
   * Tools the runtime used while answering (commands, edits, web searches).
   */
  activity: ActivityView[];
  /**
   * For an `approval` update: the approval it asks for (decided here or in the inbox).
   */
  approval_id?: string | null;
  /**
   * Persona that answered (e.g. "Marcus"), if any.
   */
  author?: string | null;
  /**
   * The board member who wrote a user message in a discussion (a ticket's,
   * a channel's; feature `ticket_discussion`). Left out for other messages
   * and by older servers.
   */
  author_user?: UserView | null;
  /**
   * The run behind this message waits for a board member (feature `run_cards`):
   * a question or an approval is open, or the post ended a ticket's main task on
   * a question (the rule of `group_chat::asks_question`). Only on the run's last
   * message in the list. Left out when false and by older servers.
   */
  awaiting_user?: boolean;
  created_at_ms: number;
  /**
   * A board member's answer to a run's questions or decision on its plan (feature
   * `run_decisions`), on the `status` update that notes it, so a client can show it
   * as a card without reading `text` apart. Left out for other messages, for
   * decisions noted before this field existed and by older servers.
   */
  decision?: RunDecision | null;
  /**
   * When the message was deleted (feature `message_edits`): `text` is the
   * line that says so. Left out when it was not and by older servers.
   */
  deleted_at_ms?: number | null;
  /**
   * The reading board member may edit and delete this message (feature
   * `message_edits`), per reader like `ReactionView::mine`. Left out when false.
   */
  editable?: boolean;
  /**
   * When the message was last edited (feature `message_edits`). Left out when
   * it never was and by older servers.
   */
  edited_at_ms?: number | null;
  /**
   * Files the message mentions, kept with it (open them with `query/file`).
   */
  files: FileRef[];
  id: string;
  /**
   * An employee's answer in the group chat: the mode its run started in.
   * Left out for other messages and by older servers.
   */
  mode?: AgentMode | null;
  ordinal: number;
  /**
   * "Connection · model" that produced an assistant message.
   */
  produced_by?: string | null;
  /**
   * A user message whose answer has not started yet: it waits behind the
   * answer running in its conversation. Such a message can be deleted
   * (`DeleteQueuedMessage`) or sent now (`SendQueuedNow`). Left out when false.
   */
  queued: boolean;
  /**
   * The reactions on the message (feature `chat_reactions`), one entry per
   * emoji, in the order each emoji was first used. Left out when there are
   * none and by older servers.
   */
  reactions?: ReactionView[];
  /**
   * For retried answers: the user message this answers again.
   */
  reply_to?: string | null;
  role: MessageRole;
  /**
   * What a run card under this message shows (feature `run_cards`): on a post in a
   * discussion and on a `Reported` line, the last one of its run in the list. With
   * `usage` for the tokens. Left out for other messages and by older servers.
   */
  run?: RunBrief | null;
  /**
   * Run that produced an assistant message.
   */
  run_id?: string | null;
  /**
   * Web sources fetched for this answer, cited as [n].
   */
  sources: SourceView[];
  status: MessageStatus;
  /**
   * A system line of Aurelius in a discussion (feature `ticket_discussion`):
   * what happened, shown as a compact grey line; `text` is its readable form.
   * Such a line never wakes anyone. Left out for other messages and by older servers.
   */
  system?: SystemLine | null;
  text: string;
  /**
   * A message that starts a thread (feature `chat_threads`): when its latest reply was written.
   */
  thread_last_reply_ms?: number | null;
  /**
   * A reply in a thread (feature `chat_threads`): the message of the channel
   * it answers (the thread's root, never itself a reply). Replies are only in
   * `thread.read`, never in the channel's own list. Left out for a message of
   * the channel itself and by older servers.
   */
  thread_parent?: string | null;
  /**
   * A message that starts a thread (feature `chat_threads`): who replied, so
   * a client can show them under the root. The names (board members'
   * usernames, employees' names, as in `ReactionView::by`) of at most
   * `THREAD_REPLIERS_MAX` (3) distinct repliers, most recent first. Left out when
   * it has no replies and by older servers.
   */
  thread_repliers?: string[];
  /**
   * A message that starts a thread (feature `chat_threads`): how many replies it has.
   * Left out when it has none and by older servers.
   */
  thread_reply_count?: number | null;
  /**
   * A message that starts a thread (feature `chat_threads`): how many of its
   * replies the reading board member has not read yet (others' replies since
   * their last `thread.read` of it). Reading the channel does not change it.
   * Left out when none are unread, for an app, and by older servers.
   */
  thread_unread?: number | null;
  /**
   * A task update in the task's conversation, not an answer: brief |
   * finding | question | status. `author` says from whom (a
   * subagent, a colleague).
   */
  update?: string | null;
  /**
   * Files the user uploaded with this message into the work folder.
   */
  uploads: MessageFileView[];
  /**
   * The tokens the answer's run used, once its runtime reported them (feature
   * `run_usage`). Left out for user messages and by older servers.
   */
  usage?: TokenUsage | null;
}

export interface MessagesSnapshot {
  agent_mode: AgentMode;
  as_of_seq: number;
  attachments: AttachmentView[];
  browser_allowed: boolean;
  conversation_id: string;
  /**
   * The employee of a task conversation or an employee chat.
   */
  employee_id?: string | null;
  messages: MessageView[];
  /**
   * The project of a task or project conversation; none for an employee chat.
   */
  project_id?: string | null;
  /**
   * An older employee chat (converted from protocol 2): history, it takes no messages.
   */
  read_only?: boolean;
  /**
   * Model and effort chosen for this conversation (unset: the employee's).
   */
  run: TaskRun;
  /**
   * The agent may take screenshots without asking each time; off by default (it then asks).
   */
  screen_allowed: boolean;
}

export type Method =
  /**
   * One tool call of an answer whole: the input and output that
   * `conversation.get` and live updates shorten (feature `activity_preview`).
   */
  | ({ method: "activity.get"; params: { call_id: string; message_id: string } })
  /**
   * Who is busy and what waits, on what (feature `activity_overview`). Another
   * board member's private chat is only counted, with its employee.
   */
  | ({ method: "activity.overview" })
  /**
   * The activity log, newest first (R8, feature `activity_log`): every delivery, queue
   * step and run step, narrowed by what is given. `limit`: at most 200, default 100.
   * `project_id` (feature `workspace_filters`): only threads of that project's tickets,
   * channels and tasks; a board member's private chat has no project.
   */
  | ({ method: "activity_log.list"; params: { before_seq?: number | null; category?: string | null; employee_id?: string | null; limit?: number | null; project_id?: string | null; run_id?: string | null; thread_key?: string | null; ticket_number?: number | null } })
  /**
   * Pending approvals, then the latest decisions. `project_id` (feature
   * `workspace_filters`): only those of runs in that project.
   */
  | ({ method: "approval.list"; params: { project_id?: string | null } })
  /**
   * Whether the safe read-only tools pass without a question (the auto-allow list).
   */
  | ({ method: "auto_allow.get_setting" })
  | ({ method: "backup.list" })
  /**
   * The board: its six columns left to right, each with its cards (not
   * archived), for one project or every project.
   */
  | ({ method: "board.get"; params: { project_id?: string | null } })
  | ({ method: "browser.get" })
  /**
   * The channels of the Team chat (feature `team_channels`): the general
   * one first, then one per project (archived projects too: read-only).
   */
  | ({ method: "channel.list" })
  /**
   * The channels whose name or ticket number matches `query` (feature
   * `ticket_channels`), in the order of `channel.list`, archived ones too: a
   * closed ticket's channel stays findable. `#12` and `12` match ticket 12;
   * other text matches the name, case not counting. A connection that does not
   * accept `ticket_channels` gets no ticket channels here either.
   */
  | ({ method: "channel.search"; params: { query: string } })
  /**
   * What a running `claude_login.link` asks for: the code Claude showed after
   * the sign-in at Anthropic, written to `claude auth login` as typed. Only
   * the connection that started the link. A session method: no receipt, no
   * request hash, no event, never printed or logged. Answers `ClaudeLogin`.
   */
  | ({ method: "claude_login.input"; params: ClaudeLoginInputParams })
  /**
   * The session's own Claude login (`ClaudeLoginView`). Feature of protocol 5.
   */
  | ({ method: "claude_login.status" })
  /**
   * The Code tab of a workspace (feature `code_view`, ticket #77): the git state of
   * the project's folder (worktrees, local branches, recent commits). A folder that is
   * no git repository answers a view with `problem` set, not an error. The same
   * rights as `board.get`.
   */
  | ({ method: "code.get"; params: { project_id: string } })
  /**
   * Whether the server committed the write sent with `idempotency_key` (feature
   * `command_applied`), answered with `CommandApplied`. A client asks this after
   * the connection dropped or timed out under a write it will not resend, so it
   * can tell "it arrived, the answer was lost" from "it never arrived". Only yes
   * or no: the receipt's result stays with the connection that wrote it.
   */
  | ({ method: "command.applied"; params: { idempotency_key: string } })
  /**
   * Above how many tokens a session is compacted (feature `compaction_setting`).
   */
  | ({ method: "compaction.get_setting" })
  /**
   * The company in one read: employees, the org chart setting and the
   * management team (roots).
   */
  | ({ method: "company.get" })
  /**
   * Where Claude Code is installed on the server.
   */
  | ({ method: "connection.detect_runtime" })
  | ({ method: "connection.list" })
  /**
   * A conversation's messages and settings.
   */
  | ({ method: "conversation.get"; params: { conversation_id: string } })
  /**
   * The designs of a project, or of all without one: the kept HTML pages of each
   * conversation, the latest first (feature `designs`).
   */
  | ({ method: "design.list"; params: { project_id?: string | null } })
  /**
   * The devices the session's board member paired, newest first (feature
   * `own_devices`). Never another board member's. A session method.
   */
  | ({ method: "device.list" })
  /**
   * Whom `@Everyone` would reach in a discussion (feature `everyone_mention`),
   * answered with `DiscussionAudience`, for the reader's own message (the
   * reader is never counted). `channel`: a channel or a ticket's discussion
   * (its conversation id, as in `channel.list`); none: the general channel.
   * Read-only; `@Everyone` itself is plain text in `group_chat.send` / `ticket.post`.
   */
  | ({ method: "discussion.audience"; params: { channel?: string | null } })
  /**
   * An employee's persistent chat with the user (company scope, no project):
   * its messages and settings, as `conversation.get` answers them.
   */
  | ({ method: "employee.chat"; params: { employee_id: string } })
  /**
   * The reader's lasting chats with employees, the latest first (feature `employee_chats`).
   */
  | ({ method: "employee.chats" })
  /**
   * The versions of an employee's instructions, newest first (feature `instruction_proposals`).
   */
  | ({ method: "employee.instruction_versions"; params: { employee_id: string } })
  /**
   * The company's employees (not archived), oldest first.
   */
  | ({ method: "employee.list" })
  /**
   * What is in an employee's mailbox now (feature `employee_queue`).
   */
  | ({ method: "employee.queue"; params: { employee_id: string } })
  /**
   * The model and effort a new run of each employee uses (feature `employee_run_overview`).
   */
  | ({ method: "employee.run_overview" })
  /**
   * An employee's scratchpad (feature `employee_scratchpad`).
   */
  | ({ method: "employee.scratchpad"; params: { employee_id: string } })
  /**
   * The catalogue of start profiles (feature `employee_profiles`).
   */
  | ({ method: "employee.start_profiles" })
  /**
   * The tools that can be switched per employee (feature `employee_tools`).
   */
  | ({ method: "employee.tool_options" })
  /**
   * Replay events after `after_seq`, then stream live events (and load/power pushes).
   */
  | ({ method: "event.subscribe"; params: { after_seq: number } })
  /**
   * A file kept with a message (see `MessageView::files`).
   */
  | ({ method: "file.get"; params: { id: string } })
  /**
   * Folders on the server: none lists the starting points.
   */
  | ({ method: "fs.list_dir"; params: { path?: string | null } })
  /**
   * The company's group chat (Team chat): the user and every employee. Its
   * messages and settings, as `conversation.get` answers them; an employee's
   * messages carry its name as `author`. Write with `group_chat.send`.
   * `channel` (feature `team_channels`): a project's channel from
   * `channel.list`; none: the general channel, the one older servers have.
   */
  | ({ method: "group_chat.get"; params: { channel?: string | null } })
  /**
   * Who a change of this guideline reaches from their next run (B6): employees,
   * projects and conversations that start a new session.
   */
  | ({ method: "guideline.affected"; params: { guideline_id: string } })
  /**
   * The set a run of this employee in this project would get now, in block
   * order, with `total_chars` against `limit_chars` (B4). Only an employee:
   * its own chat (employee guidelines); only a project: its guidelines alone.
   */
  | ({ method: "guideline.effective"; params: { employee_id?: string | null; project_id?: string | null } })
  /**
   * One guideline with the text of its current version, or of `version`.
   */
  | ({ method: "guideline.get"; params: { guideline_id: string; version?: number | null } })
  /**
   * The guideline library (feature `guidelines`): every guideline without its
   * text, alphabetical; archived ones only with `archived` (then only those).
   */
  | ({ method: "guideline.list"; params: { archived?: boolean } })
  /**
   * Proposals of `propose_guideline` (B5), pending first, then the latest
   * decisions; `status` filters. Apart from `proposal.list`.
   */
  | ({ method: "guideline.proposals"; params: { status?: ProposalStatus | null } })
  /**
   * The templates of the starter set (B7), for "Add starter set…".
   */
  | ({ method: "guideline.starter_set" })
  /**
   * Every version of a guideline, newest first, with its text and source.
   */
  | ({ method: "guideline.versions"; params: { guideline_id: string } })
  | ({ method: "idea.list"; params: { project_id: string } })
  /**
   * The ideas of every project (each keeps its own project).
   */
  | ({ method: "idea.list_all"; params: {  } })
  | ({ method: "job.list" })
  | ({ method: "load.get" })
  /**
   * Whether management chooses its own task mode (feature `management_chooses_mode`).
   */
  | ({ method: "management_mode.get_setting" })
  /**
   * One Markdown file as UTF-8 text. `path` is relative to the work folder.
   */
  | ({ method: "markdown.get"; params: { path: string; scope: FileScope } })
  /**
   * The Markdown files in a work folder, for the in-app viewer.
   */
  | ({ method: "markdown.list"; params: { scope: FileScope } })
  /**
   * The user-scope MCP servers of Claude Code and which of them every run uses.
   */
  | ({ method: "mcp.list" })
  /**
   * A login started with `mcp.login_start`; kept for a while after it ends.
   */
  | ({ method: "mcp.login_get"; params: { login_id: string } })
  /**
   * The MCP tools the user always allows (answered an approval with 'Always allow').
   */
  | ({ method: "mcp_always_allow.list" })
  /**
   * What goes into a context: an employee's (own, the project if given,
   * global) or a project conversation's (project, global; no employee). At
   * least one of the two.
   */
  | ({ method: "memory.get_context"; params: { employee_id?: string | null; project_id?: string | null } })
  /**
   * Memories, newest first: global always, plus the scopes asked for (a
   * project, an employee). Nothing asked: global and every employee.
   */
  | ({ method: "memory.list"; params: { employee_id?: string | null; project_id?: string | null } })
  /**
   * The earlier texts of a message, newest first (feature `message_edits`);
   * none for a deleted one. Not for apps.
   */
  | ({ method: "message.history"; params: { message_id: string } })
  /**
   * Where a message is (feature `message_locate`), for a link to it: its
   * conversation, ticket and thread. Only a message of a channel or a ticket's
   * discussion; any other is `not_found`, as an unknown one. Not for apps.
   */
  | ({ method: "message.locate"; params: { message_id: string } })
  /**
   * The notifications of the board member, newest first. `project_id` (feature
   * `workspace_filters`): only those about that project's tickets, tasks or
   * conversations; `unread` then counts only those too.
   */
  | ({ method: "notification.list"; params: { project_id?: string | null } })
  | ({ method: "power.get" })
  | ({ method: "project.list" })
  /**
   * The conversations of a project, most recently active first.
   */
  | ({ method: "project_conversation.list"; params: { project_id: string } })
  /**
   * Proposals of the whole company (colleagues and instructions): pending
   * first, then the latest decisions.
   */
  | ({ method: "proposal.list" })
  | ({ method: "recurring_task.list"; params: { project_id?: string | null } })
  | ({ method: "remote.get_status" })
  /**
   * Every tool call of a run, for an opened run card (feature `run_cards`).
   */
  | ({ method: "run.get_activity"; params: { run_id: string } })
  /**
   * The context manifest of one run (context inspector).
   */
  | ({ method: "run.get_context"; params: { run_id: string } })
  /**
   * What one run sent to the runtime and what each model call cost; not found
   * unless trace mode was on for it and it is among the latest (feature `run_trace`).
   */
  | ({ method: "run.get_trace"; params: { run_id: string } })
  /**
   * The guideline versions a run had in its system text (B6), kept for every
   * run (the trace's "Guidelines: Rust v3, …"); also without trace mode.
   */
  | ({ method: "run.guidelines"; params: { run_id: string } })
  /**
   * Messages employees scheduled: what waits, then what went out (feature `scheduled_messages`).
   * `project_id` (feature `workspace_filters`): only those in a ticket or channel of that project.
   */
  | ({ method: "scheduled_message.list"; params: { project_id?: string | null } })
  /**
   * A screenshot the agent took (the id is in its screenshot tool's output).
   */
  | ({ method: "screenshot.get"; params: { id: string } })
  | ({ method: "server.get_health" })
  /**
   * The server's name, its source and the computer's name (feature `server_name`).
   */
  | ({ method: "server.get_name" })
  /**
   * The first request of a session: who is calling. The protocol major in
   * every request must equal the server's, else the answer is a hard error.
   */
  | ({ method: "session.hello"; params: HelloParams })
  /**
   * The signed-in devices of a board member (none: the session's own),
   * newest activity first (`SessionView::current` marks this one). Any board
   * member may list another's (one role). Never a token or its hash.
   */
  | ({ method: "session.list"; params: { user_id?: string | null } })
  /**
   * The subagents running in the answers of one conversation.
   */
  | ({ method: "subagent.list"; params: { conversation_id: string } })
  | ({ method: "subscription.get"; params: { refresh: boolean } })
  /**
   * Tasks, newest first, company-wide; every given filter applies.
   */
  | ({ method: "task.list"; params: { employee_id?: string | null; project_id?: string | null; state?: TaskListState | null } })
  /**
   * One thread of a channel (feature `chat_threads`): `message_id` is the
   * message that starts it (a message of a channel or a ticket's discussion
   * that is not itself a reply), answered with the message and its replies,
   * oldest first (`ThreadSnapshot`). The board member who reads it has read
   * the thread (its `thread_unread` goes to none); reading the channel never
   * does. A message without replies is a thread of none.
   */
  | ({ method: "thread.read"; params: { message_id: string } })
  /**
   * Who waits to pick up a thread and what is scheduled in it (U3, feature
   * `scheduled_messages`): a ticket (`ticket_id`) or a channel or chat (`conversation_id`).
   */
  | ({ method: "thread.waiting"; params: { conversation_id?: string | null; ticket_id?: string | null } })
  /**
   * Who follows a ticket (feature `ticket_follow`). Not for apps.
   */
  | ({ method: "ticket.followers"; params: { ticket_id: string } })
  /**
   * One ticket with its main task and every task linked to it.
   */
  | ({ method: "ticket.get"; params: { ticket_id: string } })
  /**
   * The History (feature `ticket_history`, ticket #57): tickets on Done, the newest
   * Done first, each with the summary of what it delivered. Every given filter
   * applies: `employee_id` is the ticket's owner, `from_ms`/`to_ms` the period of its
   * Done (`to_ms` excluded), `query` looks in the title and the summary. Keyset
   * paging: the next page passes the last row's `done_at_ms` and `number` as
   * `before_done_at_ms`/`before_number`. `limit`: at most 100, default 50. The same
   * rights as `board.get` (B3); an archived ticket or project stays in it.
   */
  | ({ method: "ticket.history"; params: { before_done_at_ms?: number | null; before_number?: number | null; employee_id?: string | null; from_ms?: number | null; limit?: number | null; project_id?: string | null; query?: string | null; to_ms?: number | null } })
  /**
   * Tickets in board order (not archived); every given filter applies. `query`
   * searches title, description and `#number`.
   */
  | ({ method: "ticket.list"; params: { column?: ColumnKind | null; project_id?: string | null; query?: string | null } })
  /**
   * The tickets of the board member of the session (feature `my_tickets`),
   * never of one named in a param: responsible, created or followed by them,
   * and open ones on their login or without an active responsible. Done and
   * archived ones only with `include_done`. By priority, then the latest
   * update; at most `MY_TICKETS_MAX`. Not for apps.
   */
  | ({ method: "ticket.my_tickets"; params: { include_done?: boolean; project_id?: string | null } })
  /**
   * Human-readable timeline cards, newest first (keyset on `seq`).
   */
  | ({ method: "timeline.list"; params: TimelineQuery })
  /**
   * Whether runs keep a trace (feature `run_trace`).
   */
  | ({ method: "trace.get_setting" })
  /**
   * Update status; `check` asks GitHub for the latest release first.
   */
  | ({ method: "update.get"; params: { check: boolean } })
  /**
   * A file the user uploaded with a message, task, recurring task or idea (see
   * `MessageFileView::id`), in parts of at most `UPLOAD_CHUNK_BYTES` from
   * `offset` (none: 0). With `max_side` an image comes back whole as a JPEG
   * that fits that many pixels (clamped to `UPLOAD_THUMB_MAX_SIDE`); other files are refused.
   */
  | ({ method: "upload.get"; params: { id: string; max_side?: number | null; offset?: number | null } })
  | ({ method: "usage.get" })
  /**
   * Usage per hour of one server-local day (any moment in it).
   */
  | ({ method: "usage.get_hours"; params: { day_start_ms: number } })
  /**
   * What a conversation or a task used, with the tasks given away from it apart
   * (feature `run_usage`). Give one of the two; a task wins.
   */
  | ({ method: "usage.get_totals"; params: { conversation_id?: string | null; task_id?: string | null; ticket_id?: string | null } })
  /**
   * The board members, oldest first (never a password's hash), disabled ones
   * too (`UserView::disabled_at_ms`). Works without a session (the sign-in
   * screen may offer the names). Apps may read it too.
   */
  | ({ method: "user.list" })
  /**
   * Sign in (protocol 5): a right username and password give this connection a
   * session and answer `Login` with its token, shown once (keep it for the
   * server and give it in every hello). A wrong password, an unknown username
   * and a disabled board member all answer `wrong_password`; after 5 failed tries
   * for one username the server waits longer each time, up to 15 minutes
   * (`too_many_attempts` with `retry_after_ms`). A session method: it changes
   * only the connection and the sessions, so it has no receipt and no event.
   * Not for apps.
   */
  | ({ method: "user.login"; params: LoginParams })
  /**
   * End this connection's session (the device must sign in again); answers
   * `WhoAmI` without a board member. A session method.
   */
  | ({ method: "user.logout" })
  /**
   * Whether the server has board members, and this connection's session's one.
   * Works without a session.
   */
  | ({ method: "user.whoami" })
  /**
   * The audit log, newest first (`VaultAuditPage`): the company's (nothing
   * given), a project's, or every scope's (`all`); one item's with `item_id`.
   * `before`: rows older than that row id; `limit` at most 200 (default 50).
   * Never a value. Not for apps.
   */
  | ({ method: "vault.audit"; params: { all?: boolean; before?: number | null; item_id?: string | null; limit?: number | null; project_id?: string | null } })
  /**
   * The server's vault key (`VaultKeyExport`, never printed), for a restore on
   * another machine (B8): a board member with a session that needs no new
   * password, who gives their password again (`wrong_password`; it counts
   * for the sign-in limits, `too_many_attempts`). Writes an audit row
   * `key_export` and tells every board member. Not found before the first
   * item. Not for apps.
   */
  | ({ method: "vault.key_export"; params: KeyExportParams })
  /**
   * The vault of a scope (feature `vault`): none, the company's items; a
   * project, its items plus the company's apart (`VaultList::company_items`),
   * with `overrides_company` on a project item whose env name the company
   * has too, and the item count of every scope. Never a secret's value.
   * Archived items are left out. Board members only; not for apps (B5).
   */
  | ({ method: "vault.list"; params: { project_id?: string | null } })
  /**
   * One item's value (`VaultValue`, never printed), for a board member with a
   * session. Writes an audit row `reveal` (show) or `copy` with the session's
   * device. `vault_undecryptable` when the key does not fit. Not for apps.
   */
  | ({ method: "vault.reveal"; params: { item_id: string; purpose: RevealPurpose } })
  /**
   * Work sessions in the History (feature `session_history`): every board member's
   * sessions that were not deleted, the latest activity first, each with the summary
   * of what was done in it. Every given filter applies: `employee_id` is the employee
   * who works in it, `from_ms`/`to_ms` the period of its last activity (`to_ms`
   * excluded), `query` looks in the title and the summary. Keyset paging: the next
   * page passes the last row's `last_activity_ms` and `conversation_id` as
   * `before_last_activity_ms`/`before_conversation_id`. `limit`: at most 100, default
   * 50. The same rights as `ticket.history`.
   */
  | ({ method: "work_session.history"; params: { before_conversation_id?: string | null; before_last_activity_ms?: number | null; employee_id?: string | null; from_ms?: number | null; limit?: number | null; project_id?: string | null; query?: string | null; to_ms?: number | null } })
  /**
   * The reader's work sessions, the latest activity first (feature `work_sessions`);
   * `project_id`: only those of that project.
   */
  | ({ method: "work_session.list"; params: { project_id?: string | null } })
;

/**
 * `ticket.my_tickets` (feature `my_tickets`): the tickets of the board member
 * of the session, at most `MY_TICKETS_MAX`. The client makes its tabs and
 * counters from this one answer.
 */
export interface MyTicketsView {
  /**
   * Which of them the reader follows: `TicketView` is the same for every reader.
   */
  following: string[];
  tickets: TicketView[];
}

export interface NotificationView {
  /**
   * The channel or discussion the row is about (feature `personal_inbox`).
   */
  conversation_id?: string | null;
  created_at_ms: number;
  id: string;
  /**
   * How many later events this one row stands for (feature `personal_inbox`).
   * Left out when none.
   */
  merged_count?: number;
  /**
   * The message the row is about (feature `personal_inbox`).
   */
  message_id?: string | null;
  /**
   * The row waits for the reader, worked out when it is read so that it follows
   * a handover (feature `personal_inbox`). Left out when false.
   */
  needs_you?: boolean;
  read: boolean;
  /**
   * Why the reader gets the row (feature `personal_inbox`): `responsible`,
   * `mention`, `plan_review`, `review`, `sent_back`, `failed` or `login`. Text,
   * not an enum: a client shows an unknown reason as a plain row, so a new one
   * never breaks `notification.list` on an older client.
   */
  reason?: string | null;
  /**
   * action | warning | info
   */
  severity: string;
  subject: string;
  target_type: string;
  task_id?: string | null;
  /**
   * The ticket the row is about (feature `personal_inbox`). Left out otherwise and by older servers.
   */
  ticket_id?: string | null;
  /**
   * The number of `ticket_id` (`#12`).
   */
  ticket_number?: number | null;
  title: string;
}

export interface NotificationsSnapshot {
  notifications: NotificationView[];
  unread: number;
}

/**
 * `employee.open_chat`: the acting board member's lasting chat with this employee,
 * made the first time (feature `employee_chats`).
 */
export interface OpenEmployeeChat {
  employee_id: string;
}

export type Outcome =
  | ({ error: AppError })
  | ({ ok: ResponseBody })
;

/**
 * A fresh one-time pairing code; `qr` is the JSON the desktop renders as a QR code.
 */
export interface PairingCode {
  code: string;
  expires_at_ms: number;
  /**
   * The same pairing as text to paste into Aurelius on another computer
   * (`aurelius-pair:` + base64url of the payload JSON).
   */
  pairing_text: string;
  qr: string;
}

export interface PairingRequestView {
  device_name: string;
  /**
   * A computer asking for full access (owner).
   */
  owner: boolean;
  pairing_id: string;
  platform: string;
  requested_at_ms: number;
}

/**
 * A requested shutdown of the server's PC.
 */
export interface PendingShutdown {
  /**
   * When the PC powers off; set while the countdown runs, `None` while
   * `when_idle` still waits for work to finish.
   */
  deadline_ms?: number | null;
  requested_at_ms: number;
  when: ShutdownWhen;
}

export interface PinMemory {
  memory_id: string;
  pinned: boolean;
}

/**
 * How a plan is approved: carried out in Auto, or with edits accepted one by one.
 */
export type PlanApproval =
  | "approve"
  | "approve_edits"
;

/**
 * What the user decided about a plan Claude Code proposed (ExitPlanMode).
 */
export type PlanDecision =
  /**
   * Go ahead; the work continues in auto mode.
   */
  | "approve"
  /**
   * Go ahead; edits go through, everything else is asked (accept-edits mode).
   */
  | "approve_edits"
  /**
   * No: the agent stops (feedback optional).
   */
  | "reject"
  /**
   * Not yet: the agent revises the plan with the user's feedback (required).
   */
  | "revise"
;

export interface PlanLimitView {
  label: string;
  percent: number;
  resets?: string | null;
}

/**
 * `power.get`, `power.shutdown`, `power.cancel` and `power_changed`.
 */
export interface PowerState {
  pending?: PendingShutdown | null;
}

/**
 * A conversation in a project's Conversations tab.
 */
export interface ProjectConversationSummary {
  created_at_ms: number;
  id: string;
  message_count: number;
  title: string;
  updated_at_ms: number;
  /**
   * An answer is queued or running in it.
   */
  working: boolean;
}

export interface ProjectConversationsSnapshot {
  as_of_seq: number;
  /**
   * Most recently active first.
   */
  conversations: ProjectConversationSummary[];
  project_id: string;
}

export type ProjectStatus =
  | "active"
  | "archived"
  | "paused"
;

export interface ProjectSummary {
  created_at_ms: number;
  description?: string | null;
  /**
   * The project's own dispatcher for tickets; none: the company's (B5).
   */
  dispatcher_employee_id?: string | null;
  /**
   * Employees with an open task in this project (derived; employees are company-wide).
   */
  employees_active: number;
  /**
   * The project's own folder on disk.
   */
  folder?: string | null;
  id: string;
  name: string;
  status: ProjectStatus;
  /**
   * The workspace's styleguide, a Claude Design artifact (feature `design_links`). A
   * link only: runs read the artifact themselves. Left out when none and by older servers.
   */
  styleguide_url?: string | null;
  version: number;
}

export interface ProjectsSnapshot {
  /**
   * Subscribe after this cursor to stay consistent.
   */
  as_of_seq: number;
  projects: ProjectSummary[];
}

/**
 * One reference an instruction proposal rests on: a task or a memory (a lesson).
 */
export interface ProposalEvidence {
  /**
   * The task's or the memory's id.
   */
  id: string;
  /**
   * `task` | `memory`
   */
  kind: string;
  /**
   * The task's title or the memory's text (shortened), as the user sees it.
   */
  label: string;
}

/**
 * The employee that would be created.
 */
export interface ProposalFields {
  instructions: string;
  name: string;
  /**
   * Who the new employee reports to; the proposer unless said otherwise.
   */
  reports_to?: string | null;
  role: string;
  title?: string | null;
}

/**
 * What a proposal is about.
 */
export type ProposalKind =
  /**
   * A new colleague (`propose_colleague`).
   */
  | "colleague"
  /**
   * New instructions for an existing employee (`propose_instructions`).
   */
  | "instructions"
;

export type ProposalStatus =
  | "approved"
  | "pending"
  | "rejected"
  /**
   * The proposer was archived before the user decided.
   */
  | "withdrawn"
;

/**
 * A colleague an employee proposed; the user decides.
 */
export interface ProposalView {
  /**
   * The version of the target's instructions the proposal was made on.
   */
  base_version?: number | null;
  created_at_ms: number;
  /**
   * The employee created when the proposal was approved.
   */
  created_employee_id?: string | null;
  /**
   * The target's instructions now (show the diff against `instructions`).
   */
  current_instructions?: string | null;
  /**
   * The target's current version; when it differs from `base_version`, the
   * instructions changed since and approving is refused (`version_conflict`).
   */
  current_version?: number | null;
  decided_at_ms?: number | null;
  /**
   * The user's note on the decision (it goes back to the proposer).
   */
  decision_note?: string | null;
  /**
   * The tasks and memories (lessons) the proposal rests on.
   */
  evidence?: ProposalEvidence[];
  id: string;
  instructions: string;
  /**
   * What it proposes (feature `instruction_proposals`; older servers: a colleague).
   */
  kind?: ProposalKind;
  name: string;
  proposed_by: string;
  proposed_by_name: string;
  /**
   * Why the proposer wants this colleague.
   */
  reason: string;
  reports_to?: string | null;
  reports_to_name?: string | null;
  role: string;
  status: ProposalStatus;
  /**
   * Instruction proposals: the employee whose instructions would change (the
   * proposer itself or a direct report). `name`/`role` are that employee's,
   * `instructions` the proposed text.
   */
  target_employee_id?: string | null;
  target_employee_name?: string | null;
  title?: string | null;
}

export interface ProposalsSnapshot {
  as_of_seq: number;
  /**
   * Pending first, then the latest decisions.
   */
  proposals: ProposalView[];
}

export type ProviderKind =
  /**
   * Anthropic Messages API with the user's own API key.
   */
  | "anthropic_api"
  /**
   * Unmodified Claude Code CLI; the user signs in with Anthropic's own flow.
   */
  | "claude_code"
  /**
   * Built-in deterministic test runtime; no network, no cost.
   */
  | "fake"
  /**
   * OpenAI Responses API with the user's own API key.
   */
  | "openai_api"
;

export type Purpose =
  | "employee"
  | "memory"
  /**
   * The subagents a Claude Code run starts itself.
   */
  | "subagent"
;

/**
 * `push.configure`. `key_p8` is the APNs auth key file content; none keeps
 * the stored key. The key goes to the OS credential store, never to SQLite.
 */
export interface PushConfig {
  bundle_id: string;
  key_id: string;
  key_p8?: string | null;
  team_id: string;
}

export interface PushStatus {
  /**
   * An APNs key is stored and the ids are set.
   */
  apns: boolean;
  bundle_id?: string | null;
  key_id?: string | null;
  last_error?: string | null;
  team_id?: string | null;
}

/**
 * One answered question of a run (`RunDecision::answers`).
 */
export interface QuestionAnswer {
  answer: string;
  question: string;
}

/**
 * A board member adds (`on`) or takes back their reaction with `emoji` on a
 * message of a channel or a ticket's discussion that is not closed
 * (`message.react`, feature `chat_reactions`). Idempotent: on twice is on once.
 */
export interface ReactToMessage {
  emoji: string;
  message_id: string;
  on: boolean;
}

/**
 * One emoji on a message (`MessageView::reactions`, feature `chat_reactions`):
 * how many reacted with it, whether the reader did, and who.
 */
export interface ReactionView {
  /**
   * The names of who reacted (board members' usernames, employees' names), oldest first.
   */
  by?: string[];
  count: number;
  emoji: string;
  /**
   * The board member who reads the message reacted with it.
   */
  mine: boolean;
}

/**
 * The answer to every write.
 */
export interface Receipt {
  committed_seq: number;
  idempotency_key: string;
  /**
   * True when this is the receipt of an earlier identical write.
   */
  replayed: boolean;
  result: WriteResult;
}

/**
 * When a schedule is due. Weekdays count from Monday = 0 to Sunday = 6;
 * `at_minute` is minutes since local midnight (09:30 = 570).
 */
export type Recurrence =
  | ({ kind: "daily"; at_minute: number })
  | ({ kind: "every_hours"; hours: number })
  | ({ kind: "weekly"; at_minute: number; weekdays: number[] })
;

/**
 * The user writes a memory directly: active at once, the command is its source.
 */
export interface Remember {
  content: string;
  kind: MemoryKind;
  pinned?: boolean;
  scope: MemoryScope;
}

export interface RemoteClientView {
  capabilities: Capability[];
  connected: boolean;
  created_at_ms: number;
  id: string;
  last_seen_ms?: number | null;
  name: string;
  /**
   * The board member who paired it (feature `own_devices`); none: a device paired
   * before devices had an owner and never signed in on, which only the server's
   * Remote access settings show.
   */
  paired_by?: string | null;
  /**
   * ios | android | windows | macos | linux
   */
  platform: string;
  /**
   * The device registered for push notifications.
   */
  push: boolean;
}

/**
 * Settings → Remote access.
 */
export interface RemoteStatus {
  clients: RemoteClientView[];
  enabled: boolean;
  error?: string | null;
  /**
   * Devices that entered a valid code and wait for "Allow" on the desktop.
   */
  pending: PairingRequestView[];
  /**
   * Loopback port of the gateway (Tailscale Serve forwards to it).
   */
  port: number;
  push: PushStatus;
  tailscale: TailscaleStatus;
  /**
   * What a phone connects to, e.g. `https://desktop.tail1234.ts.net:8443`.
   */
  url?: string | null;
}

export interface RemoveAttachment {
  attachment_id: string;
}

export interface RemoveConnection {
  connection_id: string;
}

export interface RenameProject {
  name: string;
  project_id: string;
}

export type ResponseBody =
  /**
   * `activity.get`: one tool call whole.
   */
  | ({ type: "activity"; value: ActivityView })
  /**
   * `activity_log.list`.
   */
  | ({ type: "activity_log"; value: ActivityLogPage })
  /**
   * `activity.overview`.
   */
  | ({ type: "activity_overview"; value: ActivityOverview })
  | ({ type: "all_ideas"; value: AllIdeasSnapshot })
  | ({ type: "approvals"; value: ApprovalsSnapshot })
  | ({ type: "auto_allow_setting"; value: AutoAllowSetting })
  | ({ type: "backups"; value: BackupsSnapshot })
  | ({ type: "board"; value: BoardView })
  | ({ type: "browser"; value: BrowserSetting })
  | ({ type: "channels"; value: ChannelsSnapshot })
  | ({ type: "claude_login"; value: ClaudeLoginView })
  /**
   * `code.get`.
   */
  | ({ type: "code"; value: CodeView })
  /**
   * `command.applied`.
   */
  | ({ type: "command_applied"; value: CommandApplied })
  | ({ type: "compaction_setting"; value: CompactionSetting })
  | ({ type: "company"; value: CompanySnapshot })
  | ({ type: "connections"; value: ConnectionsSnapshot })
  | ({ type: "context"; value: ContextView })
  /**
   * `design.list`.
   */
  | ({ type: "designs"; value: DesignsSnapshot })
  /**
   * `device.list`.
   */
  | ({ type: "devices"; value: DevicesSnapshot })
  | ({ type: "dir_listing"; value: DirListing })
  /**
   * `discussion.audience`.
   */
  | ({ type: "discussion_audience"; value: DiscussionAudience })
  /**
   * `guideline.effective`.
   */
  | ({ type: "effective_guidelines"; value: EffectiveGuidelines })
  | ({ type: "employee_chats"; value: EmployeeChatsSnapshot })
  | ({ type: "employee_instruction_versions"; value: EmployeeInstructionVersionsSnapshot })
  /**
   * `employee.queue`.
   */
  | ({ type: "employee_queue"; value: EmployeeQueue })
  | ({ type: "employee_runs"; value: EmployeeRunsView })
  | ({ type: "employee_scratchpad"; value: ScratchpadView })
  /**
   * `employee.start_profiles`.
   */
  | ({ type: "employee_start_profiles"; value: StartProfiles })
  /**
   * `employee.tool_options`.
   */
  | ({ type: "employee_tool_options"; value: ToolOptions })
  | ({ type: "employees"; value: EmployeesSnapshot })
  /**
   * A file kept with a message.
   */
  | ({ type: "file"; value: { data_base64: string; id: string; media_type: string; name: string } })
  /**
   * `guideline.get`.
   */
  | ({ type: "guideline"; value: GuidelineDetail })
  /**
   * `guideline.affected`.
   */
  | ({ type: "guideline_affected"; value: GuidelineAffected })
  /**
   * `guideline.proposals`.
   */
  | ({ type: "guideline_proposals"; value: GuidelineProposals })
  /**
   * `guideline.versions`.
   */
  | ({ type: "guideline_versions"; value: GuidelineVersions })
  /**
   * `guideline.list`.
   */
  | ({ type: "guidelines"; value: GuidelinesSnapshot })
  | ({ type: "health"; value: HealthResult })
  | ({ type: "hello"; value: HelloResult })
  /**
   * `ticket.history`.
   */
  | ({ type: "history"; value: HistoryPage })
  | ({ type: "ideas"; value: IdeasSnapshot })
  | ({ type: "jobs"; value: JobsSnapshot })
  | ({ type: "load"; value: LoadSnapshot })
  /**
   * `user.login`: the session token, once.
   */
  | ({ type: "login"; value: LoginResult })
  | ({ type: "management_mode_setting"; value: ManagementModeSetting })
  | ({ type: "markdown_file"; value: MarkdownFileContent })
  | ({ type: "markdown_files"; value: MarkdownFileList })
  | ({ type: "mcp_always_allow"; value: McpAlwaysAllow })
  | ({ type: "mcp_login"; value: McpLogin })
  | ({ type: "mcp_servers"; value: McpServers })
  | ({ type: "memory"; value: MemorySnapshot })
  | ({ type: "memory_context"; value: MemoryContextView })
  /**
   * `message.history`.
   */
  | ({ type: "message_history"; value: MessageHistoryView })
  /**
   * `message.locate`.
   */
  | ({ type: "message_location"; value: MessageLocation })
  | ({ type: "messages"; value: MessagesSnapshot })
  /**
   * `ticket.my_tickets`.
   */
  | ({ type: "my_tickets"; value: MyTicketsView })
  | ({ type: "notifications"; value: NotificationsSnapshot })
  | ({ type: "power"; value: PowerState })
  | ({ type: "project_conversations"; value: ProjectConversationsSnapshot })
  | ({ type: "projects"; value: ProjectsSnapshot })
  | ({ type: "proposals"; value: ProposalsSnapshot })
  /**
   * The answer to every write.
   */
  | ({ type: "receipt"; value: Receipt })
  | ({ type: "remote"; value: RemoteStatus })
  | ({ type: "run_activity"; value: RunActivity })
  /**
   * `run.guidelines`.
   */
  | ({ type: "run_guidelines"; value: RunGuidelines })
  | ({ type: "run_trace"; value: RunTrace })
  /**
   * `None`: Claude Code was not found on the server.
   */
  | ({ type: "runtime_detected"; value: { claude_code?: string | null } })
  | ({ type: "scheduled_messages"; value: ScheduledMessagesSnapshot })
  /**
   * A screenshot the agent took, as JPEG.
   */
  | ({ type: "screenshot"; value: { id: string; jpeg_base64: string } })
  /**
   * `server.get_name`.
   */
  | ({ type: "server_name"; value: ServerNameView })
  /**
   * `work_session.history`.
   */
  | ({ type: "session_history"; value: SessionHistoryPage })
  | ({ type: "sessions"; value: SessionsSnapshot })
  /**
   * `guideline.starter_set`.
   */
  | ({ type: "starter_set"; value: StarterSet })
  | ({ type: "subagents"; value: AgentView[] })
  | ({ type: "subscribed"; value: { replayed_through_seq: number } })
  | ({ type: "subscription"; value: SubscriptionUsage })
  | ({ type: "task_schedules"; value: TaskSchedulesSnapshot })
  | ({ type: "tasks"; value: TasksSnapshot })
  /**
   * `thread.read`.
   */
  | ({ type: "thread"; value: ThreadSnapshot })
  | ({ type: "thread_waiting"; value: ThreadWaitingSnapshot })
  | ({ type: "ticket"; value: TicketDetail })
  /**
   * `ticket.followers`.
   */
  | ({ type: "ticket_followers"; value: TicketFollowersView })
  | ({ type: "tickets"; value: TicketsSnapshot })
  | ({ type: "timeline"; value: TimelinePage })
  | ({ type: "trace_setting"; value: TraceSetting })
  | ({ type: "update"; value: UpdateStatus })
  /**
   * A file the user uploaded (a part of it, or its thumbnail).
   */
  | ({ type: "upload"; value: UploadChunk })
  | ({ type: "usage"; value: UsageSnapshot })
  | ({ type: "usage_hours"; value: UsageHours })
  | ({ type: "usage_totals"; value: UsageTotals })
  | ({ type: "users"; value: UsersSnapshot })
  /**
   * `vault.list`.
   */
  | ({ type: "vault"; value: VaultList })
  /**
   * `vault.audit`.
   */
  | ({ type: "vault_audit"; value: VaultAuditPage })
  /**
   * `vault.key_export`: never printed.
   */
  | ({ type: "vault_key"; value: VaultKeyExport })
  /**
   * `vault.reveal`: never printed.
   */
  | ({ type: "vault_value"; value: VaultValue })
  | ({ type: "who_am_i"; value: WhoAmI })
  /**
   * `work_session.list`.
   */
  | ({ type: "work_sessions"; value: WorkSessionsSnapshot })
;

/**
 * Put an earlier version of an employee's instructions back. It becomes a new
 * version (history is never rewritten), and the employee's conversations start
 * a new session with it.
 */
export interface RestoreEmployeeInstructions {
  employee_id: string;
  /**
   * The version to put back (`EmployeeInstructionVersionView::version`).
   */
  version: number;
}

/**
 * Ask for a new answer version; the previous answer is kept.
 */
export interface RetryReply {
  conversation_id: string;
  user_message_id: string;
}

/**
 * A board member asks for a session's summary in the History now (feature
 * `session_history`): after it failed, for a session without one, or to bring
 * it up to date. Any board member may, as the History shows every session.
 */
export interface RetrySessionSummary {
  conversation_id: string;
}

/**
 * Why `vault.reveal` is asked: written to the audit log as `reveal` or `copy`.
 */
export type RevealPurpose =
  | "copy"
  | "show"
;

/**
 * Replace the content with a new version; the old one is superseded, not erased.
 */
export interface ReviseMemory {
  content: string;
  memory_id: string;
}

/**
 * `run.get_activity`: every tool call of a run, for an opened run card (feature `run_cards`).
 */
export interface RunActivity {
  calls: ActivityView[];
  /**
   * The answer the calls are kept with (`activity.get` reads one whole); none without calls.
   */
  message_id?: string | null;
  run_id: string;
}

/**
 * What the run card of a post shows while folded (`MessageView::run`).
 */
export interface RunBrief {
  /**
   * The employee whose run it was, for its scratchpad (feature `employee_scratchpad`).
   */
  employee_id?: string | null;
  failed_calls: number;
  /**
   * `runs.status`: created | starting | running | waiting | cancelling | succeeded |
   * failed | cancelled | unknown. A string, so a new state never breaks a client.
   */
  status: string;
  tool_calls: number;
}

/**
 * What a board member decided on a run's questions or plan (`MessageView::decision`).
 * `MessageView::text` stays the readable sentence for clients that do not know it.
 */
export interface RunDecision {
  /**
   * For `answered`: each question with its answer, in the order they were noted.
   */
  answers?: QuestionAnswer[];
  /**
   * For a plan decision: what the board member wrote with it, if anything.
   */
  feedback?: string | null;
  /**
   * One of `run_decision`. A string, not an enum, so a new kind never breaks an
   * older client: it shows `text` instead.
   */
  kind: string;
}

/**
 * A run's snapshot row (`run_guidelines`, B6): what it had in its system text.
 */
export interface RunGuideline {
  chars: number;
  guideline_id: string;
  level: GuidelineLevel;
  run_id: string;
  title: string;
  version: number;
}

/**
 * `run.guidelines`: the versions a run had in its system text (B6), in block
 * order. Kept for every run, also without trace mode; empty for a run without
 * guidelines and for runs from before guidelines.
 */
export interface RunGuidelines {
  guidelines: RunGuideline[];
  run_id: string;
}

/**
 * Something running now: since when, and the output tokens so far (thinking,
 * text and tool input; estimated while a message streams).
 */
export interface RunProgress {
  /**
   * The run is compacting the conversation's context before its turn (feature
   * `compact_status`); absent on older servers, which show it as plain work.
   */
  compacting?: boolean;
  output_tokens: number;
  started_at_ms: number;
}

/**
 * Per-field override at one scope for one purpose.
 */
export interface RunSettings {
  connection?: Setting;
  /**
   * Reasoning effort (low, medium, high, xhigh, max) where the runtime supports it.
   */
  effort?: Setting2;
  model?: Setting2;
  /**
   * Optional chat persona. Chat answers neutrally by default (no persona).
   */
  persona?: Setting2;
}

/**
 * Make a task from the schedule right now (also while paused, and even when
 * an earlier one is still open). The next scheduled time does not change.
 */
export interface RunTaskScheduleNow {
  schedule_id: string;
}

/**
 * `run.get_trace`: what one run sent to the runtime and what each of its model calls cost
 * (feature `run_trace`). Kept only while trace mode is on, for the latest runs.
 */
export interface RunTrace {
  /**
   * The command line the runtime was started with (secrets left out); empty when it did not say.
   */
  args: string[];
  conversation_id?: string | null;
  created_at_ms: number;
  effort?: string | null;
  images: TraceImage[];
  mcp_servers: string[];
  /**
   * The employee's mode at work (manual | acceptEdits | auto | plan).
   */
  mode?: string | null;
  model: string;
  /**
   * Sizes of the parts of the system text and the prompt.
   */
  parts: TracePart[];
  /**
   * text | web | work: what the runtime's own tools may do.
   */
  profile: string;
  run_id: string;
  /**
   * none | new | continue | resume: `continue` goes on in the conversation's session
   * with only the new messages; `new` sends the whole conversation (a rebuild).
   */
  session: string;
  /**
   * Every model call of the run, in order.
   */
  steps: TraceStep[];
  /**
   * The full system text (appended to the runtime's own on every run, also a continued one).
   */
  system_text: string;
  /**
   * The tools the runtime loaded (their definitions are input of every model call).
   */
  tools: string[];
  /**
   * The run's own report (none until it ended).
   */
  usage?: TokenUsage | null;
  /**
   * The prompt as it went in.
   */
  user_turn: string;
}

/**
 * Where an employee's effective model or effort comes from (`employee.run_overview`).
 */
export type RunValueSource =
  /**
   * The company-wide default.
   */
  | "company"
  /**
   * The default model of the connection.
   */
  | "connection"
  /**
   * Chosen for this employee.
   */
  | "employee"
  /**
   * Nothing is chosen: the runtime decides (the model's own effort).
   */
  | "runtime"
;

/**
 * A message an employee scheduled (`schedule_message`, R7; feature `scheduled_messages`).
 */
export interface ScheduledMessageView {
  /**
   * The channel of the Team chat it goes to.
   */
  channel_id?: string | null;
  created_at_ms: number;
  due_at_ms: number;
  from_employee: string;
  from_name: string;
  id: string;
  /**
   * Whose Claude login its run uses: the one of the run that planned it.
   */
  login_name?: string | null;
  /**
   * Why it did not go out.
   */
  outcome?: string | null;
  /**
   * Where it goes, in words ("the discussion of ticket #21").
   */
  place: string;
  /**
   * The run it started.
   */
  run_id?: string | null;
  /**
   * scheduled | delivered | cancelled | not_delivered. A string, so a new state never breaks a client.
   */
  state: string;
  text: string;
  ticket_id?: string | null;
  to_employee: string;
  to_name: string;
}

/**
 * `scheduled_message.list`: what waits, soonest first, then the latest that went out.
 */
export interface ScheduledMessagesSnapshot {
  as_of_seq: number;
  messages: ScheduledMessageView[];
}

/**
 * `employee.scratchpad`: an employee's own note (C7; feature `employee_scratchpad`).
 */
export interface ScratchpadView {
  employee_id: string;
  text: string;
  /**
   * When it last changed; none when it was never written.
   */
  updated_at_ms?: number | null;
}

/**
 * Send a user message in an existing project conversation (protocol 3.1:
 * there are no standalone chats any more, so the conversation is required).
 */
export interface SendChatMessage {
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  conversation_id: string;
  /**
   * Files of any type that belong to this message (feature `chat_files`): files on
   * the server, copied into the conversation's work folder as with an employee message.
   */
  files?: string[];
  /**
   * Images that belong to this message: files on the server (a remote
   * device's own uploads), kept with the message.
   */
  images?: string[];
  /**
   * "Send now": stop the conversation's running (and queued) answers in
   * the same step, so this message is answered right away.
   */
  interrupt?: boolean;
  text: string;
}

export interface SendEmployeeMessage {
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  /**
   * The conversation the message goes to: one of its tasks' conversations,
   * or its persistent chat (`EmployeeSummary::conversation_id`).
   */
  conversation_id: string;
  employee_id: string;
  /**
   * Files of any type to copy into the conversation's work folder (under
   * `.aurelius/uploads/`): host paths, or this device's upload inbox.
   */
  files?: string[];
  /**
   * Images that belong to this message (see `SendChatMessage::images`).
   */
  images?: string[];
  /**
   * "Send now" (see `SendChatMessage::interrupt`).
   */
  interrupt?: boolean;
  text: string;
}

/**
 * A message of the user in the group chat (`group_chat.send`).
 */
export interface SendGroupMessage {
  /**
   * The channel (feature `team_channels`): a project's, or none for the general one.
   */
  channel?: string | null;
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  /**
   * Files of any type for the employees (host paths, or this device's upload inbox).
   */
  files?: string[];
  /**
   * Images that belong to this message (see `SendChatMessage::images`).
   */
  images?: string[];
  text: string;
  /**
   * A reply in the thread of this message of the channel (feature
   * `chat_threads`): a message of the same channel that is not itself a reply.
   */
  thread_parent?: string | null;
}

/**
 * A user message in a project conversation; without `conversation_id` a new
 * conversation is made. Messages in the read-only conversations (those of
 * employees from before 1.18) are refused.
 */
export interface SendProjectMessage {
  agent_mode?: AgentMode | null;
  browser_allowed?: boolean;
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  conversation_id?: string | null;
  /**
   * Files of any type to copy into the project folder (under
   * `.aurelius/uploads/`): host paths, or this device's upload inbox.
   */
  files?: string[];
  /**
   * Images that belong to this message (see `SendChatMessage::images`).
   */
  images?: string[];
  /**
   * "Send now" (see `SendChatMessage::interrupt`).
   */
  interrupt?: boolean;
  project_id: string;
  screen_allowed?: boolean;
  text: string;
}

/**
 * "Send now" for a user message that still waits (1.9): the answers running
 * or queued ahead of it stop and its own answer starts at once; the messages
 * queued after it keep their place. Refused once that run started.
 */
export interface SendQueuedNow {
  conversation_id: string;
  message_id: string;
}

/**
 * Daemon → client.
 */
export type ServerMessage =
  /**
   * What `claude auth login` asks, only to the connection that started the
   * link (`claude_login.link`); never an event, never logged.
   */
  | ({ type: "claude_login_prompt" } & ClaudeLoginPrompt)
  /**
   * A committed event (durable); replayed from the log after `event.subscribe`.
   */
  | ({ type: "event" } & Event)
  /**
   * The machine's load or the admission changed. While runs wait, at most every 2 s.
   */
  | ({ type: "load_changed" } & LoadSnapshot)
  /**
   * The pending shutdown changed.
   */
  | ({ type: "power_changed" } & PowerState)
  | ({ type: "response"; error?: AppError | null; ok?: ResponseBody | null; request_id: number })
  /**
   * The session of this connection ended (`user.logout`, `session.revoke`, a
   * new password, `user.disable`, expiry); the server closes the connection
   * right after. The client shows "You were signed out on this device." and
   * its sign-in screen (a logout of its own needs no message).
   */
  | ({ type: "signed_out"; reason: SignOutReason })
  /**
   * Ephemeral streamed text; after a reconnect the client refetches instead.
   */
  | ({ type: "stream" } & StreamDelta)
  /**
   * Remote clients only: streamed text as an append.
   */
  | ({ type: "stream_append" } & StreamAppend)
;

/**
 * `server.get_name`: the name clients show, and where it comes from.
 */
export interface ServerNameView {
  /**
   * The name a board member chose; none: the computer's name is used.
   */
  chosen?: string | null;
  /**
   * The computer's name, the default ("Default: ‹computer_name›").
   */
  computer_name: string;
  /**
   * What clients show: the chosen name, else the computer's name. The same
   * as `HelloResult::host_name`.
   */
  name: string;
  /**
   * Counts the changes; 0 before the first one.
   */
  version: number;
}

/**
 * Where the dispatcher is with a session (`WorkSessionView::dispatch`).
 */
export type SessionDispatch =
  /**
   * Its job runs: the session waits for its choice.
   */
  | "choosing"
  /**
   * It chose nobody (or could not): the board member chooses.
   */
  | "nobody"
;

/**
 * One work session in the History (feature `session_history`): on the day of its
 * last activity, with a summary of what was done in it. Every board member's
 * session is in it; only the one who started it writes in it.
 */
export interface SessionHistoryEntry {
  /**
   * The session's own branch, when it works in a worktree.
   */
  branch?: string | null;
  conversation_id: string;
  created_at_ms: number;
  employee_id: string;
  employee_name: string;
  /**
   * The newest message (else its start): the day it shows on.
   */
  last_activity_ms: number;
  project_id: string;
  project_name: string;
  /**
   * The board member who started it (the only one who writes in it).
   */
  started_by: string;
  /**
   * Their name; none when the user is gone.
   */
  started_by_name?: string | null;
  /**
   * What was done in it (through the vault redactor), when `summary_state` is `ready`.
   */
  summary?: string | null;
  /**
   * `before_history` here: no summary was asked for yet ("No summary yet").
   */
  summary_state: HistorySummaryState;
  title: string;
}

/**
 * `work_session.history`: sessions, the latest activity first.
 */
export interface SessionHistoryPage {
  as_of_seq: number;
  entries: SessionHistoryEntry[];
  /**
   * There are older rows: ask again with the last row's `last_activity_ms` and `conversation_id`.
   */
  has_more?: boolean;
}

/**
 * One signed-in device of a board member (`session.list`).
 */
export interface SessionView {
  /**
   * The paired device; none: the desktop on the server itself (local IPC).
   */
  client_id?: string | null;
  created_at_ms: number;
  /**
   * The session of the connection that asked.
   */
  current: boolean;
  /**
   * What the device called itself at `user.login`.
   */
  device_name: string;
  /**
   * The absolute end (90 days); 30 days without use end it earlier.
   */
  expires_at_ms: number;
  id: string;
  last_seen_ms: number;
  user_id: string;
}

/**
 * `session.list`: newest activity first; never a token or its hash.
 */
export interface SessionsSnapshot {
  sessions: SessionView[];
  user_id: string;
}

export interface SetAgentMode {
  conversation_id: string;
  mode: AgentMode;
}

/**
 * Allow or forbid the agent to use the user's own browser (Claude in Chrome).
 * The browser shares the user's logins, so this is off until the user turns it on.
 */
export interface SetBrowserAllowed {
  allowed: boolean;
  conversation_id: string;
}

/**
 * The model and effort of one conversation (a thread with an employee); each
 * left unset follows the employee's own settings.
 */
export interface SetConversationRun {
  conversation_id: string;
  effort?: string | null;
  model?: string | null;
}

/**
 * Who chooses the owner of a started ticket. `employee_id` none: for a project,
 * the company's dispatcher; for the company, no dispatcher (tickets wait in
 * Todo for a board member, B5).
 */
export interface SetDispatcher {
  employee_id?: string | null;
}

/**
 * Give an employee its own workspace folder (its chat works there), or `None`.
 */
export interface SetEmployeeFolder {
  employee_id: string;
  path?: string | null;
}

/**
 * Say who an employee reports to, or make it a root (`None`). Anyone in the
 * company who is not archived, never a cycle, at most `org::MAX_CHART_DEPTH` levels.
 */
export interface SetEmployeeReportsTo {
  employee_id: string;
  reports_to?: string | null;
}

export interface SetEmployeeStatus {
  employee_id: string;
  status: LifecycleStatus;
}

/**
 * The server setting "Follow the org chart" (on by default): delegation stays
 * in the employee's own subtree, hand-overs go to the boss or to peers, and
 * "Needs you" goes to the boss first.
 */
export interface SetFollowOrgChart {
  follow: boolean;
}

export interface SetIdeaStatus {
  idea_id: string;
  status: IdeaStatus;
}

/**
 * `user.set_password`. No password is ever printed.
 */
export interface SetPasswordParams {
  new_password: string;
  /**
   * The current password; required for the own one.
   */
  old_password?: string | null;
  /**
   * Another board member (a temporary password); none: the session's own.
   */
  user_id?: string | null;
}

/**
 * Change a project's description; an empty (or blank) text clears it.
 */
export interface SetProjectDescription {
  description: string;
  project_id: string;
}

/**
 * Point a project at another folder on disk (its agents may write there).
 */
export interface SetProjectFolder {
  path: string;
  project_id: string;
}

export interface SetProjectStatus {
  project_id: string;
  status: LifecycleStatus;
}

export interface SetRunDefaults {
  purpose: Purpose;
  scope: ConfigScope;
  settings: RunSettings;
}

/**
 * Allow the agent to look at the user's screen (screenshots of the desktop or
 * one window) without asking each time. Off by default: the agent may still
 * ask for a screenshot, and the user decides in the inbox.
 */
export interface SetScreenAllowed {
  allowed: boolean;
  conversation_id: string;
}

/**
 * Pause (`true`) or resume (`false`). Resuming plans the next occurrence
 * from now; what was due while paused is not made up.
 */
export interface SetTaskSchedulePaused {
  paused: boolean;
  schedule_id: string;
}

/**
 * Who runs the full test suite once for a card in Testing (feature
 * `board_testing`). `employee_id` none: no tester, so no Testing step: cards
 * go from Doing to Review as before.
 */
export interface SetTester {
  employee_id?: string | null;
}

/**
 * A field that can inherit, hold a value, or be explicitly disabled.
 */
export type Setting =
  | ({ mode: "disabled" })
  | ({ mode: "inherit" })
  | ({ mode: "value"; value: string })
;

/**
 * A field that can inherit, hold a value, or be explicitly disabled.
 */
export type Setting2 =
  | ({ mode: "disabled" })
  | ({ mode: "inherit" })
  | ({ mode: "value"; value: string })
;

export type ShutdownWhen =
  /**
   * After the grace countdown (60 s).
   */
  | "now"
  /**
   * Once no run is running or queued and no task is running or queued.
   */
  | "when_idle"
;

/**
 * Why the server ended a connection's session (`ServerMessage::SignedOut`).
 */
export type SignOutReason =
  /**
   * The board member was disabled.
   */
  | "disabled"
  /**
   * 30 days without use, or 90 days in all.
   */
  | "expired"
  /**
   * `user.logout` on this device.
   */
  | "logged_out"
  /**
   * The password changed (on another device, or reset by another board member).
   */
  | "password_changed"
  /**
   * `session.revoke` (this or another device), or the paired device was revoked.
   */
  | "revoked"
;

export interface SourceView {
  error?: string | null;
  n: number;
  title?: string | null;
  url: string;
}

/**
 * Turn an idea into a task for this employee (its title and note become the
 * task's title and goal); the idea is then done and points at the task.
 */
export interface StartIdea {
  employee_id: string;
  idea_id: string;
  /**
   * Mode, model and effort of the task; what is left out follows the employee.
   */
  run?: TaskRun;
}

/**
 * One entry of the catalogue in the repo (`employee.start_profiles`, C6).
 */
export interface StartProfile {
  access_level?: AccessLevel;
  description: string;
  /**
   * The employees (not archived) that started from this profile.
   */
  hired?: string[];
  instructions: string;
  /**
   * Stable key for `employee.create` (`engineer`, `chief_of_staff`, ...).
   */
  key: string;
  label: string;
  /**
   * A suggested name; another employee may already have it.
   */
  name: string;
  role: string;
  /**
   * One sentence for the picker.
   */
  summary: string;
  title?: string | null;
  tool_policy?: ToolPolicy;
}

/**
 * `employee.start_profiles` (feature `employee_profiles`).
 */
export interface StartProfiles {
  profiles: StartProfile[];
}

/**
 * A new, empty project conversation. It is named after its first message.
 */
export interface StartProjectConversation {
  agent_mode?: AgentMode | null;
  browser_allowed?: boolean;
  project_id: string;
  screen_allowed?: boolean;
}

/**
 * Start a work session with its first message.
 */
export interface StartWorkSession {
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  /**
   * A design session (feature `design_sessions`): the employee makes HTML pages,
   * and the app opens the session with its preview.
   */
  design?: boolean;
  employee_id: string;
  /**
   * Files that belong to the first message (feature `chat_files`, see
   * `SendChatMessage::files`). Not with `worktree`: that folder does not exist yet.
   */
  files?: string[];
  /**
   * Images that belong to the first message (see `SendChatMessage::images`).
   */
  images?: string[];
  /**
   * Work in the project's folder itself, on a branch of its own that the daemon
   * makes from what the folder has checked out (feature `session_own_branch`). Needs
   * a project folder that is a git repository; not with `worktree`. Each run is
   * refused while the folder has uncommitted changes on another branch, or while
   * another run works in it, because checking out changes the folder for everyone.
   */
  own_branch?: boolean;
  project_id: string;
  /**
   * Mode, model and effort it starts with (empty: the employee's defaults).
   */
  run?: TaskRun;
  text: string;
  /**
   * Work in a worktree of its own instead of the project's folder. Needs a
   * project folder that is a git repository with a main branch.
   */
  worktree?: boolean;
}

/**
 * `guideline.starter_set`.
 */
export interface StarterSet {
  templates: StarterTemplate[];
}

/**
 * One template of the starter set (B7, `guideline.starter_set`).
 */
export interface StarterTemplate {
  chars: number;
  /**
   * A non-archived guideline with this title is in the library already.
   */
  in_library?: boolean;
  /**
   * Stable key for `guideline.add_starter_set` (`rust_style`, …).
   */
  key: string;
  /**
   * What it is meant for: employees (how the team works) or a project (one repository).
   */
  level: GuidelineLevel;
  text: string;
  title: string;
}

/**
 * Stop one run. Never affects other runs.
 */
export interface StopRun {
  run_id: string;
}

/**
 * Ephemeral streamed text for remote clients: `text` continues the message
 * at character `offset`. `offset == 0` carries the full text so far.
 */
export interface StreamAppend {
  /**
   * The tools used so far, sent only when they changed since the last
   * append for this message (offsets are UTF-8 bytes, as in `MessageView`).
   */
  activity?: ActivityView[] | null;
  /**
   * `activity` holds only new or changed calls (see `StreamDelta::activity_changes`).
   */
  activity_changes?: boolean;
  conversation_id: string;
  /**
   * A design being written (see `StreamDelta::draft`).
   */
  draft?: ArtifactDraft | null;
  message_id: string;
  /**
   * A group chat answer: the mode its run started in (see `StreamDelta::mode`).
   */
  mode?: AgentMode | null;
  offset: number;
  /**
   * How long it runs and the tokens so far (see `StreamDelta`).
   */
  progress?: RunProgress | null;
  run_id: string;
  text: string;
  /**
   * An answer streaming into a thread: its root (see `StreamDelta::thread_parent`).
   */
  thread_parent?: string | null;
}

/**
 * Ephemeral streamed text. Not domain truth: after reconnect the client
 * refetches messages instead of relying on missed deltas.
 */
export interface StreamDelta {
  /**
   * All tool use so far (idempotent too), each call shortened (feature
   * `activity_preview`). With `activity_changes`: only the calls new or
   * changed since this session's previous update for the message.
   */
  activity: ActivityView[];
  /**
   * `activity` holds changes only, to merge by `call_id` (new calls go at
   * the end); `false` means the full list, as always. Only a session whose
   * hello `accepts` `activity_changes` gets `true`, and never on the first
   * update of a message in that session (that one carries every call).
   */
  activity_changes?: boolean;
  conversation_id: string;
  /**
   * A design the answer is writing now (feature `artifact_drafts`): only on the
   * update that brings it, and only to a connection that accepts
   * `artifact_drafts`. Left out elsewhere and by older servers.
   */
  draft?: ArtifactDraft | null;
  message_id: string;
  /**
   * A group chat answer streaming: the mode its run started in. Left out
   * elsewhere and by older servers.
   */
  mode?: AgentMode | null;
  /**
   * How long it runs and how many tokens it wrote so far.
   */
  progress?: RunProgress | null;
  run_id: string;
  /**
   * Full text so far (idempotent; a dropped delta loses nothing).
   */
  text: string;
  /**
   * An employee's answer streaming into a thread (feature `chat_threads`):
   * the thread's root message in `conversation_id`. Only connections that
   * accept `chat_threads` get such an update. Left out elsewhere and by older servers.
   */
  thread_parent?: string | null;
}

export interface SubscriptionUsage {
  /**
   * Why there is no data (no Claude Code connection, not signed in, …).
   */
  error?: string | null;
  fetched_at_ms: number;
  limits: PlanLimitView[];
  /**
   * Whose Claude login this is (protocol 5: `subscription.get` answers the
   * session's own; nobody sees another's usage, B12). None: the server's
   * own login, until runs move to the board members' logins.
   */
  login_user?: string | null;
  /**
   * e.g. "max"; None when unknown.
   */
  plan?: string | null;
  /**
   * The full `/usage` text (shown when parsing found nothing).
   */
  raw: string;
}

export interface SubtaskUsage {
  /**
   * Who works on it.
   */
  employee_name?: string | null;
  task_id: string;
  title: string;
  usage: TokenUsage;
}

/**
 * A system line (`MessageView::system`). Every field but `kind` is optional and
 * only set where `SystemLineKind` names it.
 */
export interface SystemLine {
  /**
   * What exactly happened within `kind` (feature `ticket_channels`), one of
   * `system_action`: e.g. `opened` or `closed` on a ticket's line in General,
   * `approved` on a `plan` line. A client that knows none of them shows `text`,
   * which stays the readable sentence. A string, not an enum, so a new action
   * never breaks an older client. Left out by older servers.
   */
  action?: string | null;
  /**
   * Who did it: a board member's name, an employee's, or "Aurelius".
   */
  by?: string | null;
  /**
   * The ticket's channel (its discussion) the line links to (feature
   * `ticket_channels`), e.g. on the line in General when a ticket opens or closes.
   */
  channel_id?: string | null;
  conversation_id?: string | null;
  from?: string | null;
  kind: SystemLineKind;
  message_id?: string | null;
  task_id?: string | null;
  ticket_id?: string | null;
  to?: string | null;
}

/**
 * What a system line in a discussion says happened (`MessageView::system`).
 */
export type SystemLineKind =
  /**
   * The ticket was made from the message `message_id` in `conversation_id`.
   */
  | "created_from"
  /**
   * `from` gave the task `task_id` to `to`.
   */
  | "delegated"
  /**
   * `by` put the ticket on Done.
   */
  | "done"
  /**
   * A mention woke nobody: a loop limit was reached (`to`: who was not woken).
   */
  | "limit"
  /**
   * `by` moved the ticket from the column `from` to `to`.
   */
  | "moved"
  /**
   * The owner's plan for `task_id` waits for a board member (approve or send back).
   */
  | "plan"
  /**
   * The task `task_id` of `from` reported (or failed: the text says so).
   */
  | "reported"
  /**
   * The ticket is in Review: the report of `task_id` waits for a board member.
   */
  | "review"
  /**
   * The summary of the ticket went to the project's memory.
   */
  | "summary"
  /**
   * The summary failed (the text says why); a board member can try again (`ticket.retry_summary`).
   */
  | "summary_failed"
  /**
   * In a chat: the ticket `ticket_id` was made from this conversation's message `message_id`.
   */
  | "ticket_created"
;

export interface TailscaleStatus {
  /**
   * MagicDNS name of this PC, without the trailing dot.
   */
  dns_name?: string | null;
  error?: string | null;
  installed: boolean;
  online: boolean;
  /**
   * Tailscale Serve forwards the remote port to the gateway.
   */
  serving: boolean;
}

/**
 * One hand-over of a task to a colleague who suits it better.
 */
export interface TaskHandoff {
  from: string;
  from_id?: string | null;
  reason: string;
  to: string;
  to_id?: string | null;
}

/**
 * Which tasks `task.list` gives by state.
 */
export type TaskListState =
  /**
   * Succeeded, failed or cancelled.
   */
  | "ended"
  /**
   * Waiting for the user (input, an approval) or in review.
   */
  | "needs_you"
  /**
   * Not ended: draft, queued, running, waiting, review.
   */
  | "open"
;

/**
 * The mode, connection, model and effort a task's conversation runs with.
 * Each field left out follows the employee's own settings.
 */
export interface TaskRun {
  connection_id?: string | null;
  effort?: string | null;
  mode?: AgentMode | null;
  model?: string | null;
}

export interface TaskScheduleView {
  /**
   * Copies in the work folder, attached to every task made from it.
   */
  attachments: MessageFileView[];
  created_at_ms: number;
  criteria: string[];
  employee_id: string;
  employee_name: string;
  goal: string;
  id: string;
  /**
   * When it last made a task, and which one.
   */
  last_run_ms?: number | null;
  /**
   * When a due run was skipped because the previous task was still open.
   */
  last_skipped_ms?: number | null;
  last_task_id?: string | null;
  /**
   * When it is due next; none while paused.
   */
  next_run_ms?: number | null;
  /**
   * The task from this schedule that is still open, if any.
   */
  open_task_id?: string | null;
  paused: boolean;
  project_id: string;
  recurrence: Recurrence;
  run: TaskRun;
  title: string;
}

/**
 * Recurring tasks. Times are epoch milliseconds; the recurrence is
 * evaluated in the server's local time zone.
 */
export interface TaskSchedulesSnapshot {
  schedules: TaskScheduleView[];
}

export type TaskStatus =
  | "cancelled"
  | "draft"
  | "failed"
  | "queued"
  | "review"
  | "running"
  | "succeeded"
  | "waiting"
;

export interface TaskView {
  /**
   * Files attached when the task was made, in the work folder's uploads
   * area. Left out when there are none.
   */
  attachments: MessageFileView[];
  /**
   * Sub-tasks this task's employee delegated to colleagues.
   */
  children: string[];
  /**
   * The conversation of the task with its current employee (the task is its
   * context; plan, findings and results come back there).
   */
  conversation_id?: string | null;
  created_at_ms: number;
  criteria: string[];
  delegated_by?: string | null;
  employee_id?: string | null;
  employee_name?: string | null;
  goal: string;
  /**
   * Hand-overs between employees, oldest first.
   */
  handoffs: TaskHandoff[];
  id: string;
  /**
   * A finished task (succeeded, failed or cancelled) in whose conversation a
   * run goes on or waits to start, after the user (or a manager) wrote to it.
   * The status does not change: clients show such a task as working or
   * queued, not as done. A task in Review has it too: Review only says the
   * report is in, this says the employee is answering a message about it.
   * Derived, never stored; left out when there is none and for other tasks
   * that are not finished (their `status` says it). Older servers leave it out.
   */
  live_run?: LiveRun | null;
  /**
   * The task this one was delegated from (a sub-task), and its employee.
   */
  parent_task_id?: string | null;
  /**
   * Actions (and questions) of this task waiting for the user.
   */
  pending_approvals: number;
  project_id: string;
  result?: string | null;
  /**
   * Mode, connection, model and effort chosen for the task (unset: the employee's).
   */
  run: TaskRun;
  /**
   * The recurring task this one was made from.
   */
  schedule_id?: string | null;
  /**
   * A `scheduled` queued task: when it may start (none when not known).
   */
  starts_at_ms?: number | null;
  status: TaskStatus;
  /**
   * The ticket this task works for (a main task or a delegation below it), and its number.
   */
  ticket_id?: string | null;
  ticket_number?: number | null;
  title: string;
  updated_at_ms: number;
  /**
   * Why a waiting task waits. A queued task that does not start now has one
   * too (derived, never stored): `in_line` (its employee works on one task at
   * a time and another goes first) or `scheduled` (its start is planned for
   * later, see `starts_at_ms`). Such a task is not at work: leave it out of
   * active and at-work counts. Older servers leave it out for queued tasks.
   */
  waiting_reason?: string | null;
  /**
   * While it waits for the usage limit ("quota"): when its run goes again.
   */
  waiting_until_ms?: number | null;
}

export interface TasksSnapshot {
  as_of_seq: number;
  tasks: TaskView[];
}

/**
 * Where a thread is, as `activity_log.list` and `employee.queue` name it (R8, U6).
 */
export interface ThreadPlace {
  channel_id?: string | null;
  /**
   * A board member's chat with an employee, when it is the reader's.
   */
  chat_id?: string | null;
  /**
   * "#21 Fix login", a channel's name, "Chat with sam", or what is left of it.
   */
  label: string;
  ticket_id?: string | null;
}

/**
 * `thread.read` (feature `chat_threads`): one thread of a channel, its root
 * message and its replies oldest first, as the reading board member sees them.
 */
export interface ThreadSnapshot {
  as_of_seq: number;
  /**
   * The channel (or ticket's discussion) the thread is in.
   */
  conversation_id: string;
  /**
   * The channel is closed (an archived project's, a ticket's on Done or
   * archived): the thread can be read, not answered.
   */
  read_only?: boolean;
  /**
   * Its replies, oldest first (each with `thread_parent` set to the root).
   */
  replies?: MessageView[];
  /**
   * The message that starts the thread, with its counters (`thread_unread`
   * as it was before this read marked the thread read).
   */
  root: MessageView;
}

/**
 * An employee that waits to pick up a thread (U3).
 */
export interface ThreadWaiter {
  /**
   * How many threads of this employee go first, counting what it is doing now.
   */
  behind: number;
  employee_id: string;
  employee_name: string;
  /**
   * What holds it back besides its turn: `limit` (a usage limit), `question`
   * (the thread waits for a board member's answer). A string, so a new one never breaks a client.
   */
  reason?: string | null;
}

/**
 * `thread.waiting`: who waits to pick up a thread and what is scheduled in it (U3, R7).
 */
export interface ThreadWaitingSnapshot {
  as_of_seq: number;
  scheduled: ScheduledMessageView[];
  waiters: ThreadWaiter[];
}

/**
 * Where one ticket stands right now (feature `activity_overview`).
 */
export interface TicketActivity {
  employee_id?: string | null;
  employee_name?: string | null;
  since_ms: number;
  /**
   * running, waiting_slot, paused or waiting_other. Text, not an enum.
   */
  state: string;
  ticket_id: string;
  /**
   * With waiting_other: board_member, colleague, approval, question, limit,
   * login, later or handoff. Also slot or machine with waiting_slot.
   */
  waits_for?: string | null;
}

/**
 * `ticket.get`: the ticket, its main task and every task linked to it.
 */
export interface TicketDetail {
  as_of_seq: number;
  root_task?: TaskView | null;
  /**
   * Every task with this ticket's id (main tasks and delegations), oldest first.
   */
  tasks: TaskView[];
  ticket: TicketView;
}

/**
 * `ticket.followers` (feature `ticket_follow`): who follows a ticket.
 */
export interface TicketFollowersView {
  followers: FollowerView[];
}

/**
 * What sort of ticket it is (protocol 5, feature `quick_task`, ticket #53).
 */
export type TicketKind =
  /**
   * Triage, Plan review, delegation and Testing as the board has them.
   */
  | "normal"
  /**
   * A small piece of work: it starts in Doing with its chosen employee in Auto mode,
   * never gets triage, Plan review or Testing, cannot be delegated, works on its own
   * branch `quick/ticket-<N>` and is merged into the project's local main on Done.
   */
  | "quick"
;

/**
 * Why a ticket cannot run yet (`TicketView::needs`).
 */
export type TicketNeed =
  /**
   * `login_user` has no working Claude login (none, expired, or disabled):
   * "<name> must connect their Claude login". Another board member may take
   * it over with `ticket.use_my_login`.
   */
  | "login"
;

/**
 * A ticket's priority (B9). Normal unless chosen.
 */
export type TicketPriority =
  | "high"
  | "low"
  | "normal"
  | "urgent"
;

/**
 * A card on the board, and the fields of the ticket view.
 */
export interface TicketView {
  archived_at_ms?: number | null;
  /**
   * Files attached to the ticket (fetched with `upload.get`).
   */
  attachments: MessageFileView[];
  column: ColumnKind;
  column_id: string;
  /**
   * The ticket's discussion (feature `ticket_discussion`): read it with
   * `conversation.get`, write in it with `ticket.post`. Left out by older servers.
   */
  conversation_id?: string | null;
  created_at_ms: number;
  /**
   * The board member who made it; never changes.
   */
  created_by_user: string;
  created_by_username: string;
  deadline_ms?: number | null;
  /**
   * Markdown; none when empty.
   */
  description?: string | null;
  /**
   * The ticket's design, a Claude Design artifact (feature `design_links`). A link only:
   * its runs read the artifact themselves. Left out when none and by older servers.
   */
  design_url?: string | null;
  done_at_ms?: number | null;
  id: string;
  /**
   * `"quick"` for a quick task (feature `quick_task`, `TicketKind::as_str`); left out for a
   * normal ticket, so its view is as before and an older client shows a quick task as a
   * normal one. Text, not an enum: a view never holds an enum value an older client does not know (R1).
   */
  kind?: string | null;
  labels: string[];
  /**
   * Whose Claude login its runs use (protocol 5): the creator, unless a
   * board member took it over on their own login (`ticket.use_my_login`).
   */
  login_user: string;
  login_username: string;
  /**
   * The merge commit of a quick task accepted into the project's main (feature `quick_task`).
   */
  merge_commit?: string | null;
  /**
   * What the ticket waits for before it can run; none: nothing.
   */
  needs?: TicketNeed | null;
  /**
   * Per company, from 1: what people call it (`#12`).
   */
  number: number;
  /**
   * The employee whose main task carries the ticket; none until chosen (by
   * a board member or the dispatcher).
   */
  owner_employee_id?: string | null;
  owner_name?: string | null;
  /**
   * The order within its column, smallest first.
   */
  position: number;
  priority: TicketPriority;
  project_id: string;
  project_name: string;
  /**
   * The responsible board member is disabled: a client treats the ticket as
   * having no active responsible, without a list of board members. Left out when false.
   */
  responsible_disabled?: boolean;
  /**
   * The board member who is responsible for the ticket (feature
   * `ticket_responsible`), next to the employee who owns its main task. None:
   * nobody. Left out then and by older servers.
   */
  responsible_user_id?: string | null;
  /**
   * The name of `responsible_user_id`, so a card needs no list of board members.
   */
  responsible_username?: string | null;
  /**
   * The last feedback of a board member (send back, revise plan).
   */
  review_feedback?: string | null;
  /**
   * The current main task; earlier ones stay linked through their `ticket_id`.
   */
  root_task_id?: string | null;
  root_task_status?: TaskStatus | null;
  /**
   * Mode and model of the main task (unset: the owner's own).
   */
  run: TaskRun;
  started_at_ms?: number | null;
  title: string;
  updated_at_ms: number;
  version: number;
}

/**
 * `ticket.list`.
 */
export interface TicketsSnapshot {
  as_of_seq: number;
  tickets: TicketView[];
}

export interface TimelineCard {
  actor: string;
  at_ms: number;
  correlation_id: string;
  detail: string;
  /**
   * start | minor | decision | outcome | activity
   */
  kind: string;
  label: string;
  seq: number;
  task_id?: string | null;
  title: string;
}

export interface TimelinePage {
  cards: TimelineCard[];
  /**
   * Pass as `before_seq` for the next page; `None` when exhausted.
   */
  next_before_seq?: number | null;
}

export interface TimelineQuery {
  /**
   * Include routine runtime/tool events ("Activity").
   */
  activity: boolean;
  /**
   * Keyset cursor: cards strictly older than this sequence number.
   */
  before_seq?: number | null;
  /**
   * Only the chain with this correlation id ("Waarom?" / "Wat volgde?").
   */
  correlation_id?: string | null;
  limit: number;
  project_id?: string | null;
}

/**
 * Tokens of one run or a sum of runs (feature `run_usage`). Every model call
 * sends the whole context again: `cached_tokens` is the part read from the prompt
 * cache (cheap), `input_tokens` the rest, of which `cache_creation_tokens` was
 * written to the cache (dearer than plain input).
 */
export interface TokenUsage {
  cache_creation_tokens: number;
  cached_tokens: number;
  /**
   * Only when the provider reported a cost (micro-USD).
   */
  cost_micro_usd?: number | null;
  /**
   * Some values are not a bill (e.g. subscription: API-equivalent value).
   */
  estimate: boolean;
  input_tokens: number;
  /**
   * How many model calls those runs made (feature `run_cards`). 0: not known,
   * for usage from before the calls were counted and by older servers.
   */
  model_calls?: number;
  output_tokens: number;
  runs: number;
}

/**
 * One line of `employee.tool_options`.
 */
export interface ToolOption {
  group: string;
  label: string;
  /**
   * The access levels that have this tool at all.
   */
  levels: AccessLevel[];
  name: string;
}

/**
 * `employee.tool_options` (feature `employee_tools`).
 */
export interface ToolOptions {
  tools: ToolOption[];
}

/**
 * `EmployeeConfig::tool_policy` (C2).
 */
export interface ToolPolicy {
  /**
   * The tools that always go to a board member first, in every mode:
   * a tool of [`SWITCHABLE_TOOLS`], or a whole MCP server as `mcp__<server>__*`.
   */
  approval_tools?: string[];
  /**
   * The MCP servers it may use, of those enabled server-wide. None: all of them.
   */
  mcp_servers?: string[] | null;
  /**
   * The switchable tools it has. None: all that its level gives.
   */
  tools?: string[] | null;
}

export interface TraceImage {
  bytes: number;
  media_type: string;
  name: string;
}

export interface TracePart {
  /**
   * system | prompt
   */
  area: string;
  chars: number;
  /**
   * E.g. "Instructions", "Memory", "Earlier conversation", "New message".
   */
  label: string;
  /**
   * Estimated (characters / 4).
   */
  tokens: number;
}

/**
 * `trace.get_setting` / `trace.set_enabled`: whether runs keep a trace (feature `run_trace`).
 */
export interface TraceSetting {
  enabled: boolean;
  /**
   * Traces older than this go.
   */
  keep_days: number;
  /**
   * Traces kept at most (the newest).
   */
  keep_runs: number;
}

export interface TraceStep {
  cache_creation_tokens: number;
  cache_read_tokens: number;
  /**
   * Fresh input: neither read from nor written to the prompt cache.
   */
  input_tokens: number;
  output_tokens: number;
  subagent: boolean;
  /**
   * The tools this call asked for.
   */
  tools: string[];
}

/**
 * Change the company's name, its dispatcher or its tester; left out: unchanged.
 */
export interface UpdateCompany {
  dispatcher?: SetDispatcher | null;
  name?: string | null;
  /**
   * Feature `board_testing`.
   */
  tester?: SetTester | null;
}

export interface UpdateEmployee {
  /**
   * What its runs may do with files and commands (C3). `None` leaves it as it is.
   */
  access_level?: AccessLevel | null;
  /**
   * What other employees read about this one (P7). `None` leaves it as it is;
   * an empty text clears it.
   */
  description?: string | null;
  employee_id: string;
  instructions?: string | null;
  name: string;
  role: string;
  /**
   * Run its tasks one at a time; `None` leaves it as it is.
   */
  sequential_tasks?: boolean | null;
  /**
   * `None` leaves the title as it is; an empty text clears it.
   */
  title?: string | null;
  /**
   * Which tools and MCP servers it has and which ask first (C2). `None` leaves it
   * as it is; an empty policy is all of its level.
   */
  tool_policy?: ToolPolicy | null;
}

export interface UpdateIdea {
  /**
   * Files on the server to attach next to the ones the idea has.
   */
  add_files?: string[];
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  idea_id: string;
  /**
   * None keeps the idea's kind (older clients).
   */
  kind?: IdeaKind | null;
  note?: string | null;
  /**
   * Attachments to drop, by their `path` in `IdeaView::attachments`.
   */
  remove_files?: string[];
  title: string;
}

/**
 * Change a project's settings; left out: unchanged. `dispatcher` with no
 * employee: the company's dispatcher again.
 */
export interface UpdateProject {
  dispatcher?: SetDispatcher | null;
  project_id: string;
  /**
   * The workspace's styleguide: a Claude Design artifact link
   * (`ticket::valid_artifact_url`, feature `design_links`); an empty text clears it.
   */
  styleguide_url?: string | null;
}

export interface UpdateStatus {
  /**
   * The fields above are the server's (service and aurelius-ctl). The
   * desktop app is released separately: its own status, when this
   * computer has the app installed. Absent from older servers.
   */
  app?: UpdateStatus | null;
  available: boolean;
  checked_at_ms: number;
  current: string;
  /**
   * Only the installed production app updates itself.
   */
  enabled: boolean;
  error?: string | null;
  latest?: string | null;
  notes?: string | null;
  reason?: string | null;
  /**
   * A downloaded and verified version, ready to apply.
   */
  staged?: string | null;
}

/**
 * Replace a schedule's template and recurrence (the project stays the same).
 */
export interface UpdateTaskSchedule {
  acceptance_criteria?: string[];
  /**
   * New files to attach (as `CreateTaskSchedule::files`).
   */
  add_files?: string[];
  /**
   * A board member confirmed that the text may hold a vault secret's value
   * (after `looks_like_secret`, B10): it is stored with the value redacted.
   */
  confirm_secret?: boolean;
  employee_id: string;
  goal: string;
  recurrence: Recurrence;
  /**
   * Attachments to drop, by their `path` in `TaskScheduleView::attachments`.
   */
  remove_files?: string[];
  run?: TaskRun;
  schedule_id: string;
  title: string;
}

/**
 * `upload.get`: a part of an uploaded file from `offset`, or its thumbnail.
 */
export interface UploadChunk {
  /**
   * The whole file's size (the thumbnail's for a thumbnail).
   */
  byte_length: number;
  data_base64: string;
  /**
   * No more parts after this one: `offset` + this part's size is `byte_length`.
   */
  done: boolean;
  id: string;
  /**
   * Of the file (a thumbnail is always `image/jpeg`), guessed from its name.
   */
  media_type: string;
  name: string;
  offset: number;
  /**
   * `data_base64` is a JPEG thumbnail, not the file.
   */
  thumbnail: boolean;
}

export interface UsageDay {
  day_start_ms: number;
  tokens: number;
}

export interface UsageHour {
  /**
   * Part of `input_tokens` written to the prompt cache (feature `run_usage`).
   */
  cache_creation_tokens?: number;
  cached_tokens: number;
  cost_micro_usd?: number | null;
  estimate: boolean;
  hour_start_ms: number;
  input_tokens: number;
  output_tokens: number;
  runs: number;
}

/**
 * One server-local day in hours: always 24 entries, oldest first.
 */
export interface UsageHours {
  day_start_ms: number;
  hours: UsageHour[];
}

export interface UsageRow {
  /**
   * Part of `input_tokens` written to the prompt cache (feature `run_usage`).
   */
  cache_creation_tokens?: number;
  cached_tokens: number;
  connection: string;
  cost_micro_usd?: number | null;
  estimate: boolean;
  input_tokens: number;
  model: string;
  output_tokens: number;
  provider: string;
  runs: number;
}

export interface UsageSnapshot {
  /**
   * Tokens per day, last 14 days (oldest first).
   */
  by_day: UsageDay[];
  /**
   * Per connection and model, last 30 days.
   */
  by_source: UsageRow[];
  /**
   * Per board member whose Claude login ran it, last 30 days (feature `run_cards`).
   * Left out by older servers.
   */
  by_user?: UsageUserRow[];
  /**
   * Today, 7 days, 30 days.
   */
  windows: UsageWindow[];
}

/**
 * `usage.get_totals`: what a conversation or a task used (feature `run_usage`).
 */
export interface UsageTotals {
  /**
   * The runs of the conversation itself (a task: of its conversation).
   */
  own: TokenUsage;
  /**
   * Tasks given away from it (delegated, handed over, started from it), each
   * with its own sub-tasks included.
   */
  subtasks: SubtaskUsage[];
  /**
   * `own` plus every subtask.
   */
  total: TokenUsage;
}

/**
 * One board member's share of the usage (`UsageSnapshot::by_user`).
 */
export interface UsageUserRow {
  cache_creation_tokens: number;
  cached_tokens: number;
  cost_micro_usd?: number | null;
  estimate: boolean;
  input_tokens: number;
  /**
   * `runs.login_user`; none: runs on no one's login (an API key, a background run).
   */
  login_user?: string | null;
  model_calls?: number;
  output_tokens: number;
  runs: number;
  /**
   * That board member's name; none when there is none or they are gone.
   */
  username?: string | null;
}

export interface UsageWindow {
  /**
   * Part of `input_tokens` written to the prompt cache (feature `run_usage`).
   */
  cache_creation_tokens?: number;
  cached_tokens: number;
  /**
   * Only when the provider reported a cost (micro-USD).
   */
  cost_micro_usd?: number | null;
  /**
   * Some values are not a bill (e.g. subscription: API-equivalent value).
   */
  estimate: boolean;
  input_tokens: number;
  label: string;
  output_tokens: number;
  runs: number;
}

/**
 * `user.create`. The password is never printed.
 */
export interface UserCreateParams {
  password: string;
  username: string;
}

/**
 * A board member as an item or an audit row names them.
 */
export interface UserRef {
  id: string;
  username: string;
}

/**
 * A board member as every client sees it; never the password's hash.
 */
export interface UserView {
  created_at_ms: number;
  /**
   * None: made in the onboarding, without an active board member.
   */
  created_by_user?: string | null;
  /**
   * Disabled (`user.disable`): cannot sign in; tickets keep their creator.
   */
  disabled_at_ms?: number | null;
  id: string;
  username: string;
}

/**
 * `user.list`, oldest first.
 */
export interface UsersSnapshot {
  users: UserView[];
}

/**
 * Who did it: a board member (on a device), or an employee in a run.
 */
export type VaultActor =
  | ({ type: "board_member"; user: UserRef })
  | ({ type: "employee"; employee_id: string; name: string; run_id?: string | null; task_id?: string | null; ticket_id?: string | null })
;

/**
 * What an audit row records. Never a value, also not an old or a new one.
 */
export type VaultAuditAction =
  | "archive"
  /**
   * A board member copied it (`vault.reveal` copy).
   */
  | "copy"
  | "create"
  /**
   * A run got it as an environment variable (one row per run and item).
   */
  | "deliver"
  /**
   * Name, env name, description or kind (`changed_fields`).
   */
  | "edit"
  /**
   * A board member exported the vault key (no item).
   */
  | "key_export"
  /**
   * A new value (for a secret: "Replaced").
   */
  | "replace"
  /**
   * A board member showed the value (`vault.reveal` show).
   */
  | "reveal"
  /**
   * A run read a text or url with `vault_read`.
   */
  | "tool_read"
;

/**
 * `vault.audit`: newest first.
 */
export interface VaultAuditPage {
  /**
   * Older rows exist: ask again with `before` = the last row's id.
   */
  more: boolean;
  rows: VaultAuditRow[];
}

/**
 * One row of the audit log (append-only).
 */
export interface VaultAuditRow {
  action: VaultAuditAction;
  actor: VaultActor;
  at_ms: number;
  /**
   * `edit`: which fields changed (`name`, `env_name`, `description`, `kind`); never their values.
   */
  changed_fields?: string[];
  /**
   * Newest is highest; `vault.audit { before }` pages on it.
   */
  id: number;
  /**
   * None: a key export.
   */
  item_id?: string | null;
  /**
   * The item's name at the time (also for an archived item).
   */
  item_name?: string | null;
  /**
   * The item's scope; none: the company (or a key export).
   */
  project_id?: string | null;
  /**
   * The board member's device (the session's name, "Desktop · dev-laptop").
   */
  via?: string | null;
}

/**
 * `vault.create`. The value is never printed.
 */
export interface VaultCreateParams {
  description?: string | null;
  env_name: string;
  kind: VaultKind;
  name: string;
  /**
   * None: the company's.
   */
  project_id?: string | null;
  value: string;
}

/**
 * One item as every client sees it.
 */
export interface VaultItemView {
  created_at_ms: number;
  created_by: UserRef;
  description?: string | null;
  /**
   * How a run gets it (UPPER_SNAKE, unique in the scope): the environment
   * variable, and the name in the vault tools.
   */
  env_name: string;
  id: string;
  kind: VaultKind;
  name: string;
  /**
   * A project item whose env name a company item has too: in runs of the
   * project this one wins (pill "overrides Company", B4).
   */
  overrides_company?: boolean;
  /**
   * None: the company's.
   */
  project_id?: string | null;
  /**
   * The server's vault key is missing or does not fit this value (a restore
   * on another machine without the key, MU4-04 AC3): the client shows
   * "Cannot be decrypted" instead of a value. The rest still works.
   */
  undecryptable?: boolean;
  updated_at_ms: number;
  updated_by: UserRef;
  /**
   * The value of a text or url item; never a secret's (`vault.reveal`), and
   * none when it cannot be decrypted.
   */
  value?: string | null;
  /**
   * Goes up with every change (the envelope's `expected_version`).
   */
  version: number;
}

/**
 * `vault.key_export`: the server's vault key, to keep apart (a restore on
 * another machine needs it, B8). Never printed.
 */
export interface VaultKeyExport {
  /**
   * The key as one line: `aurelius-vault-key:v1:<env>:<key_id>:<base64url>`
   * (`<env>` `dev` or `prod`, the 32 bytes in base64url without padding).
   * What `aurelius-daemon vault-key import` reads.
   */
  key: string;
  /**
   * The key's fingerprint: what `aurelius-daemon vault-key import` checks
   * against the stored values.
   */
  key_id: string;
}

/**
 * What an item holds. Text and url may change into each other; a secret stays
 * a secret, and nothing becomes one (`vault_kind_locked`, B7).
 */
export type VaultKind =
  /**
   * Masked everywhere; reaches runs only as an environment variable.
   */
  | "secret"
  | "text"
  /**
   * `https://` or `http://`.
   */
  | "url"
;

/**
 * `vault.list`: the items of the scope asked for (by name), and for a project
 * the company's apart (a run of the project gets those too, MU4-02 AC1).
 */
export interface VaultList {
  /**
   * For a project: the company's items (empty for the company itself).
   */
  company_items?: VaultItemView[];
  /**
   * Every scope that has items, the company first (zero items included for it).
   */
  counts?: VaultScopeCount[];
  items: VaultItemView[];
  /**
   * The scope asked for; none: the company.
   */
  project_id?: string | null;
}

/**
 * How many items a scope has (the scope list of the vault screen).
 */
export interface VaultScopeCount {
  count: number;
  /**
   * None: the company.
   */
  project_id?: string | null;
}

/**
 * `vault.update`. The value is never printed.
 */
export interface VaultUpdateParams {
  /**
   * `Some("")` clears it.
   */
  description?: string | null;
  env_name?: string | null;
  item_id: string;
  kind?: VaultKind | null;
  name?: string | null;
  value?: string | null;
}

/**
 * `vault.reveal`: one item's value. Never printed.
 */
export interface VaultValue {
  item_id: string;
  value: string;
}

/**
 * Probe a connection now (status, models) and record the result.
 */
export interface VerifyConnection {
  connection_id: string;
}

/**
 * `user.whoami`: what the onboarding gate needs (also in the hello).
 */
export interface WhoAmI {
  board_members_exist: boolean;
  /**
   * The session's board member signed in with a temporary password and must
   * change it (`user.set_password`) before anything else works.
   */
  must_change_password?: boolean;
  /**
   * The board member of this connection's session.
   */
  user?: UserView | null;
}

/**
 * One work session in `work_session.list`.
 */
export interface WorkSessionView {
  branch?: string | null;
  conversation_id: string;
  created_at_ms: number;
  /**
   * A design session (feature `design_sessions`); older servers leave it out.
   */
  design?: boolean;
  /**
   * The dispatcher still chooses who takes it, or chose nobody (feature
   * `session_dispatch`); none: `employee_id` works in it. While it chooses,
   * `employee_id` is the dispatcher.
   */
  dispatch?: SessionDispatch | null;
  employee_id: string;
  employee_name: string;
  last_activity_ms: number;
  /**
   * It works in the project's folder on its own `branch` (feature
   * `session_own_branch`); older servers leave it out.
   */
  own_branch?: boolean;
  project_id: string;
  project_name: string;
  /**
   * Whether a run of it is queued or working now.
   */
  running: boolean;
  /**
   * Named after its first message, until a title made from its content
   * replaces it after the first answer.
   */
  title: string;
  /**
   * A run of it ended after the reader last opened it (with `conversation.get`), so
   * there is a finished answer they have not seen. Never while it runs; older
   * servers leave it out, and a client then shows no mark.
   */
  unseen?: boolean;
  /**
   * Its own worktree and branch; none when it works in the project's folder.
   * With `own_branch` the worktree is the project's folder itself.
   */
  worktree?: string | null;
}

/**
 * `work_session.list`: the reader's sessions, the latest activity first.
 */
export interface WorkSessionsSnapshot {
  as_of_seq: number;
  sessions: WorkSessionView[];
}

export type WriteResult =
  | ({ kind: "command"; value: CommandResult })
  | ({ kind: "host"; value: HostResult })
;
