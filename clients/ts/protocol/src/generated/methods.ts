// GENERATED CODE - DO NOT EDIT.
// Run: cargo xtask gen-ts

import type * as M from "./models";

/** The protocol major this app speaks. The server refuses another one. */
export const PROTOCOL_MAJOR = 5;

/** `read`: reads and session methods. `host`: writes only the host can do. `command`: domain commands. */
export type MethodGroup = "read" | "host" | "command";

/** The wire name of every method, with its group. */
export const METHODS = {
  "activity.get": "read",
  "activity.overview": "read",
  "activity_log.list": "read",
  "approval.answer": "command",
  "approval.decide": "command",
  "approval.decide_plan": "command",
  "approval.list": "read",
  "attachment.add": "command",
  "attachment.remove": "command",
  "auto_allow.get_setting": "read",
  "auto_allow.set_enabled": "host",
  "backup.create": "host",
  "backup.list": "read",
  "backup.restore": "host",
  "backup.resume": "host",
  "board.columns_update": "command",
  "board.get": "read",
  "browser.get": "read",
  "browser.set": "host",
  "browser.set_backend": "host",
  "channel.list": "read",
  "channel.search": "read",
  "chat.retry": "command",
  "chat.send": "command",
  "claude_login.cancel": "host",
  "claude_login.input": "read",
  "claude_login.link": "host",
  "claude_login.status": "read",
  "claude_login.unlink": "host",
  "code.delete_branch": "host",
  "code.diff": "read",
  "code.fetch": "host",
  "code.get": "read",
  "code.merge": "host",
  "code.prune_merged": "host",
  "code.remove_worktree": "host",
  "code.reply": "command",
  "code.resolve_thread": "command",
  "code.review": "read",
  "code.set_viewed": "command",
  "code.start_thread": "command",
  "code.submit_review": "command",
  "command.applied": "read",
  "compaction.get_setting": "read",
  "compaction.set_threshold": "host",
  "company.get": "read",
  "company.set_background_login": "command",
  "company.set_follow_org_chart": "command",
  "company.update": "command",
  "connection.add": "command",
  "connection.detect_runtime": "read",
  "connection.list": "read",
  "connection.remove": "command",
  "connection.set_defaults": "command",
  "connection.verify": "command",
  "conversation.delete": "command",
  "conversation.delete_queued": "command",
  "conversation.get": "read",
  "conversation.send_queued_now": "command",
  "conversation.set_agent_mode": "command",
  "conversation.set_browser_allowed": "command",
  "conversation.set_run": "command",
  "conversation.set_screen_allowed": "command",
  "conversation.side_ask": "read",
  "conversation.side_cancel": "read",
  "design.list": "read",
  "device.list": "read",
  "device.revoke": "host",
  "discussion.audience": "read",
  "employee.chat": "read",
  "employee.chats": "read",
  "employee.create": "command",
  "employee.instruction_versions": "read",
  "employee.list": "read",
  "employee.open_chat": "command",
  "employee.queue": "read",
  "employee.restore_instructions": "command",
  "employee.run_overview": "read",
  "employee.scratchpad": "read",
  "employee.send_message": "command",
  "employee.set_folder": "command",
  "employee.set_reports_to": "command",
  "employee.set_status": "command",
  "employee.start_profiles": "read",
  "employee.tool_options": "read",
  "employee.update": "command",
  "event.subscribe": "read",
  "file.get": "read",
  "fs.create_dir": "host",
  "fs.list_dir": "read",
  "group_chat.get": "read",
  "group_chat.send": "command",
  "guideline.add_starter_set": "command",
  "guideline.affected": "read",
  "guideline.archive": "command",
  "guideline.create": "command",
  "guideline.decide_proposal": "command",
  "guideline.effective": "read",
  "guideline.get": "read",
  "guideline.link": "command",
  "guideline.list": "read",
  "guideline.proposals": "read",
  "guideline.restore": "command",
  "guideline.restore_version": "command",
  "guideline.starter_set": "read",
  "guideline.unlink": "command",
  "guideline.update": "command",
  "guideline.versions": "read",
  "idea.add": "command",
  "idea.delete": "command",
  "idea.list": "read",
  "idea.list_all": "read",
  "idea.set_status": "command",
  "idea.start": "command",
  "idea.update": "command",
  "job.list": "read",
  "load.get": "read",
  "load.set_limits": "host",
  "management_mode.get_setting": "read",
  "management_mode.set_enabled": "host",
  "markdown.get": "read",
  "markdown.list": "read",
  "mcp.add": "host",
  "mcp.list": "read",
  "mcp.login_complete": "host",
  "mcp.login_get": "read",
  "mcp.login_start": "host",
  "mcp.logout": "host",
  "mcp.remove": "host",
  "mcp.set": "host",
  "mcp_always_allow.list": "read",
  "mcp_always_allow.remove": "host",
  "memory.consolidate": "command",
  "memory.decide": "command",
  "memory.forget": "command",
  "memory.get_context": "read",
  "memory.list": "read",
  "memory.pin": "command",
  "memory.remember": "command",
  "memory.revise": "command",
  "message.delete": "command",
  "message.edit": "command",
  "message.history": "read",
  "message.locate": "read",
  "message.react": "command",
  "notification.dismiss": "host",
  "notification.dismiss_all": "host",
  "notification.list": "read",
  "notification.mark_all_read": "host",
  "notification.mark_read": "host",
  "power.cancel": "host",
  "power.get": "read",
  "power.shutdown": "host",
  "project.create": "command",
  "project.export": "host",
  "project.list": "read",
  "project.rename": "command",
  "project.set_description": "command",
  "project.set_folder": "command",
  "project.set_status": "command",
  "project.update": "command",
  "project_conversation.list": "read",
  "project_conversation.send": "command",
  "project_conversation.start": "command",
  "proposal.decide": "command",
  "proposal.list": "read",
  "push.configure": "host",
  "push.test": "host",
  "recurring_task.create": "command",
  "recurring_task.delete": "command",
  "recurring_task.list": "read",
  "recurring_task.run_now": "command",
  "recurring_task.set_paused": "command",
  "recurring_task.update": "command",
  "remote.decide_pairing": "host",
  "remote.disable": "host",
  "remote.enable": "host",
  "remote.get_status": "read",
  "remote.revoke": "host",
  "remote.start_pairing": "host",
  "run.get_activity": "read",
  "run.get_context": "read",
  "run.get_trace": "read",
  "run.guidelines": "read",
  "run.stop": "command",
  "scheduled_message.cancel": "command",
  "scheduled_message.list": "read",
  "screenshot.get": "read",
  "secret.set": "host",
  "server.get_health": "read",
  "server.get_name": "read",
  "server.restart": "host",
  "server.set_name": "command",
  "session.hello": "read",
  "session.list": "read",
  "session.revoke": "host",
  "subagent.list": "read",
  "subscription.get": "read",
  "task.assign": "command",
  "task.cancel": "command",
  "task.create": "command",
  "task.list": "read",
  "thread.read": "read",
  "thread.waiting": "read",
  "ticket.accept": "command",
  "ticket.approve_plan": "command",
  "ticket.archive": "command",
  "ticket.assign": "command",
  "ticket.cancel": "command",
  "ticket.create": "command",
  "ticket.follow": "command",
  "ticket.followers": "read",
  "ticket.get": "read",
  "ticket.history": "read",
  "ticket.list": "read",
  "ticket.move": "command",
  "ticket.my_tickets": "read",
  "ticket.post": "command",
  "ticket.retry_summary": "command",
  "ticket.revise_plan": "command",
  "ticket.send_back": "command",
  "ticket.set_responsible": "command",
  "ticket.start": "command",
  "ticket.update": "command",
  "ticket.use_my_login": "command",
  "timeline.list": "read",
  "trace.get_setting": "read",
  "trace.set_enabled": "host",
  "update.apply": "host",
  "update.apply_app": "host",
  "update.get": "read",
  "update.prepare": "host",
  "update.prepare_app": "host",
  "upload.get": "read",
  "usage.get": "read",
  "usage.get_hours": "read",
  "usage.get_totals": "read",
  "user.create": "host",
  "user.disable": "host",
  "user.list": "read",
  "user.login": "read",
  "user.logout": "read",
  "user.rail_pins": "read",
  "user.set_password": "host",
  "user.set_rail_pins": "command",
  "user.whoami": "read",
  "vault.archive": "host",
  "vault.audit": "read",
  "vault.create": "host",
  "vault.key_export": "read",
  "vault.list": "read",
  "vault.reveal": "read",
  "vault.update": "host",
  "work_session.assign": "command",
  "work_session.dispatch": "command",
  "work_session.history": "read",
  "work_session.list": "read",
  "work_session.retry_summary": "command",
  "work_session.start": "command",
} as const satisfies Record<string, MethodGroup>;

export type MethodName = keyof typeof METHODS;

/** The `params` of every method; `undefined`: it takes none. */
export interface MethodParams {
  /**
   * One tool call of an answer whole: the input and output that
   * `conversation.get` and live updates shorten (feature `activity_preview`).
   */
  "activity.get": { call_id: string; message_id: string };
  /**
   * Who is busy and what waits, on what (feature `activity_overview`). Another
   * board member's private chat is only counted, with its employee.
   */
  "activity.overview": undefined;
  /**
   * The activity log, newest first (R8, feature `activity_log`): every delivery, queue
   * step and run step, narrowed by what is given. `limit`: at most 200, default 100.
   * `project_id` (feature `workspace_filters`): only threads of that project's tickets,
   * channels and tasks; a board member's private chat has no project.
   */
  "activity_log.list": { before_seq?: number | null; category?: string | null; employee_id?: string | null; limit?: number | null; project_id?: string | null; run_id?: string | null; thread_key?: string | null; ticket_number?: number | null };
  "approval.answer": M.AnswerQuestion;
  "approval.decide": M.DecideApproval;
  "approval.decide_plan": M.DecidePlan;
  /**
   * Pending approvals, then the latest decisions. `project_id` (feature
   * `workspace_filters`): only those of runs in that project.
   */
  "approval.list": { project_id?: string | null };
  "attachment.add": M.AttachFile;
  "attachment.remove": M.RemoveAttachment;
  /**
   * Whether the safe read-only tools pass without a question (the auto-allow list).
   */
  "auto_allow.get_setting": undefined;
  /**
   * Auto-allow of the safe read-only tools on or off, from the next tool call. On by default.
   */
  "auto_allow.set_enabled": { enabled: boolean };
  "backup.create": undefined;
  "backup.list": undefined;
  /**
   * Restore at the next start (the service stops now); work stays paused until resumed.
   */
  "backup.restore": { id: string };
  "backup.resume": undefined;
  /**
   * Rename the columns and set their order (B3): all of them, left to right.
   * `expected_version` is the board's (`board_version`: the columns change
   * together); the answer gives the new one.
   */
  "board.columns_update": { columns: M.BoardColumnEdit[] };
  /**
   * The board: its six columns left to right, each with its cards (not
   * archived), for one project or every project.
   */
  "board.get": { project_id?: string | null };
  "browser.get": undefined;
  "browser.set": { choice: M.BrowserChoice };
  /**
   * Claude in Chrome or Playwright, for every run that starts afterwards.
   */
  "browser.set_backend": { backend: M.BrowserBackend };
  /**
   * The channels of the Team chat (feature `team_channels`): the general
   * one first, then one per project (archived projects too: read-only).
   */
  "channel.list": undefined;
  /**
   * The channels whose name or ticket number matches `query` (feature
   * `ticket_channels`), in the order of `channel.list`, archived ones too: a
   * closed ticket's channel stays findable. `#12` and `12` match ticket 12;
   * other text matches the name, case not counting. A connection that does not
   * accept `ticket_channels` gets no ticket channels here either.
   */
  "channel.search": { query: string };
  "chat.retry": M.RetryReply;
  "chat.send": M.SendChatMessage;
  /**
   * Stop a running link of the session's own login. Answers `ClaudeLogin`.
   */
  "claude_login.cancel": { link_id: string };
  /**
   * What a running `claude_login.link` asks for: the code Claude showed after
   * the sign-in at Anthropic, written to `claude auth login` as typed. Only
   * the connection that started the link. A session method: no receipt, no
   * request hash, no event, never printed or logged. Answers `ClaudeLogin`.
   */
  "claude_login.input": M.ClaudeLoginInputParams;
  /**
   * Connect the session's own Claude login: the server starts `claude auth
   * login` for `<data>/logins/<user_id>/` and answers `ClaudeLoginLink`. What
   * Claude asks (the sign-in link, the code) comes to this connection only as
   * `ServerMessage::ClaudeLoginPrompt`; the code goes back with
   * `claude_login.input`. One link per board member at a time, 10 minutes.
   */
  "claude_login.link": undefined;
  /**
   * The session's own Claude login (`ClaudeLoginView`). Feature of protocol 5.
   */
  "claude_login.status": undefined;
  /**
   * Disconnect the session's own Claude login (`claude auth logout`, its folder
   * removed). Answers `ClaudeLogin`.
   */
  "claude_login.unlink": undefined;
  /**
   * Delete a local branch (not main, not one checked out in a worktree, not one an
   * employee works on now). A branch with commits main lacks needs `force` (the
   * client asked the board member first: that work is gone). Answers `Code`.
   */
  "code.delete_branch": { branch: string; force?: boolean; project_id: string };
  /**
   * What merging a branch would bring into main, file by file with its hunks, plus what
   * its worktree has not committed (feature `code_review`, ticket #84). Redacted.
   */
  "code.diff": { branch: string; project_id: string };
  /**
   * `git fetch --prune` of the project's repository (feature `code_view`, ticket #77);
   * answers `HostResult::Code`. Never a prompt for credentials: a remote that wants
   * one fails with git's reason.
   */
  "code.fetch": { project_id: string };
  /**
   * The Code tab of a workspace (feature `code_view`, ticket #77): the git state of
   * the project's folder (worktrees, local branches, recent commits). A folder that is
   * no git repository answers a view with `problem` set, not an error. The same
   * rights as `board.get`.
   */
  "code.get": { project_id: string };
  /**
   * Merge a local branch into the project's main (`--no-ff`), in the project's folder.
   * Refused when the folder is not on main, main has uncommitted changes, is in another
   * merge, or the branch has nothing ahead; a conflict is aborted, so main stays as it
   * was (`validation` with git's reason and the conflicting files). Answers `Code`.
   */
  "code.merge": { branch: string; project_id: string };
  /**
   * Clean up merged: remove every clean worktree of a merged branch nobody works on,
   * then delete every merged branch without a worktree (never main), then
   * `git worktree prune`. Answers `Code`.
   */
  "code.prune_merged": { project_id: string };
  /**
   * Remove a worktree (not the project's folder). Refused when it has uncommitted
   * changes or an employee works in it now. The branch stays. Answers `Code`.
   */
  "code.remove_worktree": { path: string; project_id: string };
  /**
   * `code.reply`: a reply in a thread, pending until its writer finishes their review.
   * Answers `CodeCommented`.
   */
  "code.reply": { body: string; thread_id: string };
  /**
   * `code.resolve_thread`: mark a thread resolved (it folds) or open again. Answers
   * `CodeThreadResolved`; the same state again changes nothing.
   */
  "code.resolve_thread": { resolved: boolean; thread_id: string };
  /**
   * A branch's review as the board member of the session sees it: line threads (with
   * only their own pending comments), finished reviews, and the files they viewed at
   * `head` (feature `code_review`, ticket #84). Not for apps.
   */
  "code.review": { branch: string; head?: string | null; project_id: string };
  /**
   * `code.set_viewed`: the board member of the session viewed a file of a branch at a head
   * (or not any more). A new head starts unviewed. Answers `CodeViewedSet`.
   */
  "code.set_viewed": { branch: string; head: string; path: string; project_id: string; viewed: boolean };
  /**
   * `code.start_thread`: a board member's first comment on a line of a branch's diff. It is
   * pending until they finish their review. Answers `CodeThreadStarted`.
   */
  "code.start_thread": { body: string; branch: string; commit: string; line: number; path: string; project_id: string; side: M.CodeLineSide; snippet?: string[] };
  /**
   * `code.submit_review`: finish the board member's review of a branch at `head`. Their
   * pending comments on it are sent with it. A comment or a request for changes needs a
   * summary or a pending comment; an approval needs neither. Answers `CodeReviewSubmitted`.
   * Waking the owner is the client's next write (`ticket.post`, `ticket.send_back` or
   * `chat.send`), with the text it shows the board member.
   */
  "code.submit_review": { body?: string | null; branch: string; head: string; project_id: string; verdict: M.CodeVerdict };
  /**
   * Whether the server committed the write sent with `idempotency_key` (feature
   * `command_applied`), answered with `CommandApplied`. A client asks this after
   * the connection dropped or timed out under a write it will not resend, so it
   * can tell "it arrived, the answer was lost" from "it never arrived". Only yes
   * or no: the receipt's result stays with the connection that wrote it.
   */
  "command.applied": { idempotency_key: string };
  /**
   * Above how many tokens a session is compacted (feature `compaction_setting`).
   */
  "compaction.get_setting": undefined;
  /**
   * Above how many tokens of context a session is compacted before its next turn
   * (feature `compaction_setting`), from the next run on; 0: never.
   */
  "compaction.set_threshold": { threshold_tokens: number };
  /**
   * The company in one read: employees, the org chart setting and the
   * management team (roots).
   */
  "company.get": undefined;
  /**
   * The company's background jobs (memory consolidation, B6) run on the Claude
   * login of the board member who asks (`enabled`), or on none (`false`, only
   * when it is theirs): without one they do not run. Only the own login can be
   * chosen. Answers `CompanyUpdated`.
   */
  "company.set_background_login": { enabled: boolean };
  "company.set_follow_org_chart": M.SetFollowOrgChart;
  "company.update": M.UpdateCompany;
  "connection.add": M.AddConnection;
  /**
   * Where Claude Code is installed on the server.
   */
  "connection.detect_runtime": undefined;
  "connection.list": undefined;
  "connection.remove": M.RemoveConnection;
  "connection.set_defaults": M.SetRunDefaults;
  "connection.verify": M.VerifyConnection;
  "conversation.delete": M.DeleteConversation;
  "conversation.delete_queued": M.DeleteQueuedMessage;
  /**
   * A conversation's messages and settings.
   */
  "conversation.get": { conversation_id: string };
  "conversation.send_queued_now": M.SendQueuedNow;
  "conversation.set_agent_mode": M.SetAgentMode;
  "conversation.set_browser_allowed": M.SetBrowserAllowed;
  "conversation.set_run": M.SetConversationRun;
  "conversation.set_screen_allowed": M.SetScreenAllowed;
  /**
   * A side question about a conversation (feature `side_question`), like
   * Claude Code's `/btw`: answered from what the conversation holds, without
   * tools, on the asker's own Claude login, and never stored: no message, no
   * event, the employee never sees it and its run goes on. Answers at once with
   * `SideAsked`; the text follows to this connection only as
   * `ServerMessage::SideAnswer`. Board members only; not for apps.
   */
  "conversation.side_ask": M.SideAskParams;
  /**
   * Stop a side question of this connection that is still answering. Unknown
   * or finished ids are fine (nothing to stop). Answers `SideAsked` with the
   * same id; the answer ends with a `SideAnswer` that is `done`.
   */
  "conversation.side_cancel": { side_id: string };
  /**
   * The designs of a project, or of all without one: the kept HTML pages of each
   * conversation, the latest first (feature `designs`).
   */
  "design.list": { project_id?: string | null };
  /**
   * The devices the session's board member paired, newest first (feature
   * `own_devices`). Never another board member's. A session method.
   */
  "device.list": undefined;
  /**
   * Remove one of the session's own paired devices (feature `own_devices`): its
   * sessions end and its connections close. Another board member's device is
   * `not_found`. Answers `Done`.
   */
  "device.revoke": { client_id: string };
  /**
   * Whom `@Everyone` would reach in a discussion (feature `everyone_mention`),
   * answered with `DiscussionAudience`, for the reader's own message (the
   * reader is never counted). `channel`: a channel or a ticket's discussion
   * (its conversation id, as in `channel.list`); none: the general channel.
   * Read-only; `@Everyone` itself is plain text in `group_chat.send` / `ticket.post`.
   */
  "discussion.audience": { channel?: string | null };
  /**
   * An employee's persistent chat with the user (company scope, no project):
   * its messages and settings, as `conversation.get` answers them.
   */
  "employee.chat": { employee_id: string };
  /**
   * The reader's lasting chats with employees, the latest first (feature `employee_chats`).
   */
  "employee.chats": undefined;
  "employee.create": M.CreateEmployee;
  /**
   * The versions of an employee's instructions, newest first (feature `instruction_proposals`).
   */
  "employee.instruction_versions": { employee_id: string };
  /**
   * The company's employees (not archived), oldest first.
   */
  "employee.list": undefined;
  "employee.open_chat": M.OpenEmployeeChat;
  /**
   * What is in an employee's mailbox now (feature `employee_queue`).
   */
  "employee.queue": { employee_id: string };
  "employee.restore_instructions": M.RestoreEmployeeInstructions;
  /**
   * The model and effort a new run of each employee uses (feature `employee_run_overview`).
   */
  "employee.run_overview": undefined;
  /**
   * An employee's scratchpad (feature `employee_scratchpad`).
   */
  "employee.scratchpad": { employee_id: string };
  "employee.send_message": M.SendEmployeeMessage;
  "employee.set_folder": M.SetEmployeeFolder;
  "employee.set_reports_to": M.SetEmployeeReportsTo;
  "employee.set_status": M.SetEmployeeStatus;
  /**
   * The catalogue of start profiles (feature `employee_profiles`).
   */
  "employee.start_profiles": undefined;
  /**
   * The tools that can be switched per employee (feature `employee_tools`).
   */
  "employee.tool_options": undefined;
  "employee.update": M.UpdateEmployee;
  /**
   * Replay events after `after_seq`, then stream live events (and load/power pushes).
   */
  "event.subscribe": { after_seq: number };
  /**
   * A file kept with a message (see `MessageView::files`).
   */
  "file.get": { id: string };
  /**
   * A new folder `name` in the folder `parent`.
   */
  "fs.create_dir": { name: string; parent: string };
  /**
   * Folders on the server: none lists the starting points.
   */
  "fs.list_dir": { path?: string | null };
  /**
   * The company's group chat (Team chat): the user and every employee. Its
   * messages and settings, as `conversation.get` answers them; an employee's
   * messages carry its name as `author`. Write with `group_chat.send`.
   * `channel` (feature `team_channels`): a project's channel from
   * `channel.list`; none: the general channel, the one older servers have.
   */
  "group_chat.get": { channel?: string | null };
  "group_chat.send": M.SendGroupMessage;
  /**
   * `guideline.add_starter_set`: the chosen templates as guidelines without links (B7).
   * A key whose title is in the library already is skipped.
   */
  "guideline.add_starter_set": { keys: string[] };
  /**
   * Who a change of this guideline reaches from their next run (B6): employees,
   * projects and conversations that start a new session.
   */
  "guideline.affected": { guideline_id: string };
  /**
   * `guideline.archive`: no run gets it any more; links and versions stay.
   */
  "guideline.archive": { guideline_id: string };
  /**
   * `guideline.create`: a new guideline (version 1, source `created`), linked at once to `links`.
   */
  "guideline.create": { links?: M.GuidelineTarget[]; text: string; title: string };
  /**
   * `guideline.decide_proposal` (B5): approve (with `title`/`text` edited by the
   * board member, and for a new guideline the `links`; none: the suggested
   * ones) or reject. `note` goes back to the proposer. `version_conflict` when
   * the guideline moved on since the proposal's base version.
   */
  "guideline.decide_proposal": { approve: boolean; links?: M.GuidelineTarget[] | null; note?: string | null; proposal_id: string; text?: string | null; title?: string | null };
  /**
   * The set a run of this employee in this project would get now, in block
   * order, with `total_chars` against `limit_chars` (B4). Only an employee:
   * its own chat (employee guidelines); only a project: its guidelines alone.
   */
  "guideline.effective": { employee_id?: string | null; project_id?: string | null };
  /**
   * One guideline with the text of its current version, or of `version`.
   */
  "guideline.get": { guideline_id: string; version?: number | null };
  /**
   * `guideline.link`: an employee or a project gets it from its next run.
   */
  "guideline.link": { guideline_id: string; target: M.GuidelineTarget };
  /**
   * The guideline library (feature `guidelines`): every guideline without its
   * text, alphabetical; archived ones only with `archived` (then only those).
   */
  "guideline.list": { archived?: boolean };
  /**
   * Proposals of `propose_guideline` (B5), pending first, then the latest
   * decisions; `status` filters. Apart from `proposal.list`.
   */
  "guideline.proposals": { status?: M.ProposalStatus | null };
  /**
   * `guideline.restore`: back from the archive, with its links (B4 and the title are checked again).
   */
  "guideline.restore": { guideline_id: string };
  /**
   * `guideline.restore_version`: a copy of `version` as the newest (source `restored`).
   */
  "guideline.restore_version": { guideline_id: string; version: number };
  /**
   * The templates of the starter set (B7), for "Add starter set…".
   */
  "guideline.starter_set": undefined;
  /**
   * `guideline.unlink`.
   */
  "guideline.unlink": { guideline_id: string; target: M.GuidelineTarget };
  /**
   * `guideline.update`: a new version with what changed (source `edited`); what
   * is left out stays. `expected_version` in the envelope is the version the
   * editor started from (`version_conflict` otherwise).
   */
  "guideline.update": { guideline_id: string; text?: string | null; title?: string | null };
  /**
   * Every version of a guideline, newest first, with its text and source.
   */
  "guideline.versions": { guideline_id: string };
  "idea.add": M.AddIdea;
  "idea.delete": M.DeleteIdea;
  "idea.list": { project_id: string };
  /**
   * The ideas of every project (each keeps its own project).
   */
  "idea.list_all": {  };
  "idea.set_status": M.SetIdeaStatus;
  "idea.start": M.StartIdea;
  "idea.update": M.UpdateIdea;
  "job.list": undefined;
  "load.get": undefined;
  "load.set_limits": M.LoadLimitsUpdate;
  /**
   * Whether management chooses its own task mode (feature `management_chooses_mode`).
   */
  "management_mode.get_setting": undefined;
  /**
   * Management chooses its own task mode (feature `management_chooses_mode`), for every run
   * that starts afterwards. On by default.
   */
  "management_mode.set_enabled": { enabled: boolean };
  /**
   * One Markdown file as UTF-8 text. `path` is relative to the work folder.
   */
  "markdown.get": { path: string; scope: M.FileScope };
  /**
   * The Markdown files in a work folder, for the in-app viewer.
   */
  "markdown.list": { scope: M.FileScope };
  "mcp.add": M.McpAddParams;
  /**
   * The user-scope MCP servers of Claude Code and which of them every run uses.
   */
  "mcp.list": undefined;
  /**
   * Finishes a login with the address the browser ended on (`…/callback?code=…&state=…`),
   * for a browser that is not on the server's computer.
   */
  "mcp.login_complete": { login_id: string; redirect_url: string };
  /**
   * A login started with `mcp.login_start`; kept for a while after it ends.
   */
  "mcp.login_get": { login_id: string };
  /**
   * Starts an OAuth login for an http/sse server; answers `McpLogin` with the URL to open.
   */
  "mcp.login_start": { name: string };
  /**
   * Forgets the server's tokens (keeps its client registration).
   */
  "mcp.logout": { name: string };
  "mcp.remove": { name: string };
  "mcp.set": { enabled: string[] };
  /**
   * The MCP tools the user always allows (answered an approval with 'Always allow').
   */
  "mcp_always_allow.list": undefined;
  /**
   * Take a tool off the always-allow list: its next call asks again.
   */
  "mcp_always_allow.remove": { tool: string };
  "memory.consolidate": M.ConsolidateMemory;
  "memory.decide": M.DecideMemory;
  "memory.forget": M.ForgetMemory;
  /**
   * What goes into a context: an employee's (own, the project if given,
   * global) or a project conversation's (project, global; no employee). At
   * least one of the two.
   */
  "memory.get_context": { employee_id?: string | null; project_id?: string | null };
  /**
   * Memories, newest first: global always, plus the scopes asked for (a
   * project, an employee). Nothing asked: global and every employee.
   */
  "memory.list": { employee_id?: string | null; project_id?: string | null };
  "memory.pin": M.PinMemory;
  "memory.remember": M.Remember;
  "memory.revise": M.ReviseMemory;
  "message.delete": M.DeleteMessage;
  "message.edit": M.EditMessage;
  /**
   * The earlier texts of a message, newest first (feature `message_edits`);
   * none for a deleted one. Not for apps.
   */
  "message.history": { message_id: string };
  /**
   * Where a message is (feature `message_locate`), for a link to it: its
   * conversation, ticket and thread. Only a message of a channel or a ticket's
   * discussion; any other is `not_found`, as an unknown one. Not for apps.
   */
  "message.locate": { message_id: string };
  "message.react": M.ReactToMessage;
  "notification.dismiss": { id: string };
  "notification.dismiss_all": undefined;
  /**
   * The notifications of the board member, newest first. `project_id` (feature
   * `workspace_filters`): only those about that project's tickets, tasks or
   * conversations; `unread` then counts only those too.
   */
  "notification.list": { project_id?: string | null };
  "notification.mark_all_read": undefined;
  /**
   * One row read, for this board member only (feature `personal_inbox`); answers
   * `Count`, 0 when there was nothing to read, so it can be repeated.
   */
  "notification.mark_read": { id: string };
  "power.cancel": undefined;
  "power.get": undefined;
  "power.shutdown": { when: M.ShutdownWhen };
  "project.create": M.CreateProject;
  /**
   * Write a readable Markdown + JSON export of a project into its folder.
   */
  "project.export": { project_id: string };
  "project.list": undefined;
  "project.rename": M.RenameProject;
  "project.set_description": M.SetProjectDescription;
  "project.set_folder": M.SetProjectFolder;
  "project.set_status": M.SetProjectStatus;
  "project.update": M.UpdateProject;
  /**
   * The conversations of a project, most recently active first.
   */
  "project_conversation.list": { project_id: string };
  "project_conversation.send": M.SendProjectMessage;
  "project_conversation.start": M.StartProjectConversation;
  "proposal.decide": M.DecideProposal;
  /**
   * Proposals of the whole company (colleagues and instructions): pending
   * first, then the latest decisions.
   */
  "proposal.list": undefined;
  "push.configure": M.PushConfig;
  "push.test": { client_id: string };
  "recurring_task.create": M.CreateTaskSchedule;
  "recurring_task.delete": M.DeleteTaskSchedule;
  "recurring_task.list": { project_id?: string | null };
  "recurring_task.run_now": M.RunTaskScheduleNow;
  "recurring_task.set_paused": M.SetTaskSchedulePaused;
  "recurring_task.update": M.UpdateTaskSchedule;
  /**
   * Allow or refuse a device that entered the code. A computer that asks for
   * full access is only allowed with `owner` (the user confirmed it).
   */
  "remote.decide_pairing": { approve: boolean; owner: boolean; pairing_id: string };
  "remote.disable": undefined;
  "remote.enable": undefined;
  "remote.get_status": undefined;
  "remote.revoke": { client_id: string };
  /**
   * A new one-time code (5 minutes) plus the QR payload; the receipt keeps neither.
   */
  "remote.start_pairing": undefined;
  /**
   * Every tool call of a run, for an opened run card (feature `run_cards`).
   */
  "run.get_activity": { run_id: string };
  /**
   * The context manifest of one run (context inspector).
   */
  "run.get_context": { run_id: string };
  /**
   * What one run sent to the runtime and what each model call cost; not found
   * unless trace mode was on for it and it is among the latest (feature `run_trace`).
   */
  "run.get_trace": { run_id: string };
  /**
   * The guideline versions a run had in its system text (B6), kept for every
   * run (the trace's "Guidelines: Rust v3, …"); also without trace mode.
   */
  "run.guidelines": { run_id: string };
  "run.stop": M.StopRun;
  "scheduled_message.cancel": M.CancelScheduledMessage;
  /**
   * Messages employees scheduled: what waits, then what went out (feature `scheduled_messages`).
   * `project_id` (feature `workspace_filters`): only those in a ticket or channel of that project.
   */
  "scheduled_message.list": { project_id?: string | null };
  /**
   * A screenshot the agent took (the id is in its screenshot tool's output).
   */
  "screenshot.get": { id: string };
  "secret.set": { connection_id: string; secret: string };
  "server.get_health": undefined;
  /**
   * The server's name, its source and the computer's name (feature `server_name`).
   */
  "server.get_name": undefined;
  "server.restart": undefined;
  /**
   * Choose the server's name (`server.set_name`). Empty or only spaces puts the
   * computer's name back. The same name as now changes nothing (no event).
   * Answers `ServerNameSet`.
   */
  "server.set_name": { name: string };
  /**
   * The first request of a session: who is calling. The protocol major in
   * every request must equal the server's, else the answer is a hard error.
   */
  "session.hello": M.HelloParams;
  /**
   * The signed-in devices of a board member (none: the session's own),
   * newest activity first (`SessionView::current` marks this one). Any board
   * member may list another's (one role). Never a token or its hash.
   */
  "session.list": { user_id?: string | null };
  /**
   * End one session (any board member's: one role). Its device's open
   * connection closes at once (`ServerMessage::SignedOut`); the timeline says
   * who revoked it. Answers `Done`. Not for apps.
   */
  "session.revoke": { session_id: string };
  /**
   * The subagents running in the answers of one conversation.
   */
  "subagent.list": { conversation_id: string };
  "subscription.get": { refresh: boolean };
  "task.assign": M.AssignTask;
  "task.cancel": M.CancelTask;
  "task.create": M.CreateTask;
  /**
   * Tasks, newest first, company-wide; every given filter applies.
   */
  "task.list": { employee_id?: string | null; project_id?: string | null; state?: M.TaskListState | null };
  /**
   * One thread of a channel (feature `chat_threads`): `message_id` is the
   * message that starts it (a message of a channel or a ticket's discussion
   * that is not itself a reply), answered with the message and its replies,
   * oldest first (`ThreadSnapshot`). The board member who reads it has read
   * the thread (its `thread_unread` goes to none); reading the channel never
   * does. A message without replies is a thread of none.
   */
  "thread.read": { message_id: string };
  /**
   * Who waits to pick up a thread and what is scheduled in it (U3, feature
   * `scheduled_messages`): a ticket (`ticket_id`) or a channel or chat (`conversation_id`).
   */
  "thread.waiting": { conversation_id?: string | null; ticket_id?: string | null };
  /**
   * Review → Done. Only a board member; only when no linked task still works (B6).
   */
  "ticket.accept": { confirm_secret?: boolean; note?: string | null; ticket_id: string };
  /**
   * Plan review → Doing: the owner carries the plan out.
   */
  "ticket.approve_plan": { decision: M.PlanApproval; feedback?: string | null; ticket_id: string };
  /**
   * Take a ticket off the board (from Done or Backlog); it stays readable.
   */
  "ticket.archive": { ticket_id: string };
  /**
   * Choose the owner of a ticket before its main task exists: a board member
   * (instead of the dispatcher) or the dispatcher itself. Gives `ticket.dispatched`.
   *
   * `employee_id` none: no owner again, so the dispatcher chooses when it
   * starts. Only in Backlog (before the start); gives `ticket.updated` with
   * `changed: ["owner"]`, not `ticket.dispatched` (nobody was chosen).
   */
  "ticket.assign": { employee_id?: string | null; reason?: string | null; ticket_id: string };
  /**
   * Stop the work on a ticket: its main task is cancelled and it goes back to Backlog.
   */
  "ticket.cancel": { ticket_id: string };
  /**
   * A new ticket in Backlog (`start`: straight to Todo, as `ticket.start` would).
   */
  "ticket.create": { confirm_secret?: boolean; deadline_ms?: number | null; description?: string | null; files?: string[]; from_idea?: string | null; from_message?: string | null; kind?: M.TicketKind | null; labels?: string[]; owner_employee_id?: string | null; priority?: M.TicketPriority; project_id: string; responsible_user_id?: string | null; run?: M.TaskRun; start?: boolean; title: string };
  "ticket.follow": M.FollowTicket;
  /**
   * Who follows a ticket (feature `ticket_follow`). Not for apps.
   */
  "ticket.followers": { ticket_id: string };
  /**
   * One ticket with its main task and every task linked to it.
   */
  "ticket.get": { ticket_id: string };
  /**
   * The History (feature `ticket_history`, ticket #57): tickets on Done, the newest
   * Done first, each with the summary of what it delivered. Every given filter
   * applies: `employee_id` is the ticket's owner, `from_ms`/`to_ms` the period of its
   * Done (`to_ms` excluded), `query` looks in the title and the summary. Keyset
   * paging: the next page passes the last row's `done_at_ms` and `number` as
   * `before_done_at_ms`/`before_number`. `limit`: at most 100, default 50. The same
   * rights as `board.get` (B3); an archived ticket or project stays in it.
   */
  "ticket.history": { before_done_at_ms?: number | null; before_number?: number | null; employee_id?: string | null; from_ms?: number | null; limit?: number | null; project_id?: string | null; query?: string | null; to_ms?: number | null };
  /**
   * Tickets in board order (not archived); every given filter applies. `query`
   * searches title, description and `#number`.
   */
  "ticket.list": { column?: M.ColumnKind | null; project_id?: string | null; query?: string | null };
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
  "ticket.move": { before_ticket_id?: string | null; column_id: string; ticket_id: string };
  /**
   * The tickets of the board member of the session (feature `my_tickets`),
   * never of one named in a param: responsible, created or followed by them,
   * and open ones on their login or without an active responsible. Done and
   * archived ones only with `include_done`. By priority, then the latest
   * update; at most `MY_TICKETS_MAX`. Not for apps.
   */
  "ticket.my_tickets": { include_done?: boolean; project_id?: string | null };
  /**
   * A board member writes in a ticket's discussion (feature `ticket_discussion`).
   * Without `@Name` it goes to the owner while the ticket is in Todo (with an
   * owner), Doing, Plan review, Testing or Review; else it wakes nobody. `@Name` wakes those
   * employees. Refused on Done (the discussion is read-only there). Not for apps.
   */
  "ticket.post": { confirm_secret?: boolean; files?: string[]; images?: string[]; text: string; thread_parent?: string | null; ticket_id: string };
  /**
   * Try the summary of a ticket on Done again after it failed (`SystemLineKind::SummaryFailed`).
   * Only a board member; refused when the summary is already there or still being made.
   */
  "ticket.retry_summary": { ticket_id: string };
  /**
   * Plan review → Doing in Plan mode: the owner makes a new plan with the feedback.
   */
  "ticket.revise_plan": { confirm_secret?: boolean; feedback: string; ticket_id: string };
  /**
   * Review → Doing with feedback (required) for the owner.
   */
  "ticket.send_back": { confirm_secret?: boolean; feedback: string; ticket_id: string };
  /**
   * Choose the board member responsible for a ticket (`ticket.set_responsible`,
   * feature `ticket_responsible`). Its own command rather than a field of
   * `UpdateTicket`, where "nobody" and "unchanged" would need two kinds of none.
   */
  "ticket.set_responsible": { responsible_user_id?: string | null; ticket_id: string };
  /**
   * The Start button: Backlog to Todo. With an employee the dispatcher is skipped.
   * Also starts again a ticket whose main task failed.
   */
  "ticket.start": { employee_id?: string | null; ticket_id: string };
  /**
   * Change the fields of a ticket. Left out: unchanged. The creator never
   * changes; the owner only through `ticket.assign`; mode and model only while
   * the main task does not run.
   */
  "ticket.update": { add_files?: string[] | null; clear_deadline?: boolean; confirm_secret?: boolean; deadline_ms?: number | null; description?: string | null; design_url?: string | null; labels?: string[] | null; priority?: M.TicketPriority | null; run?: M.TaskRun | null; ticket_id: string; title?: string | null };
  /**
   * Run a ticket on the Claude login of the board member who asks (B4, D3-3):
   * `TicketView::login_user` becomes that board member, never anyone else. The
   * creator stays. Only a board member; answers `TicketChanged`.
   */
  "ticket.use_my_login": { ticket_id: string };
  /**
   * Human-readable timeline cards, newest first (keyset on `seq`).
   */
  "timeline.list": M.TimelineQuery;
  /**
   * Whether runs keep a trace (feature `run_trace`).
   */
  "trace.get_setting": undefined;
  /**
   * Trace mode on or off for every run that starts afterwards (feature `run_trace`).
   * Off also forgets the traces kept so far.
   */
  "trace.set_enabled": { enabled: boolean };
  "update.apply": undefined;
  "update.apply_app": undefined;
  /**
   * Update status; `check` asks GitHub for the latest release first.
   */
  "update.get": { check: boolean };
  "update.prepare": undefined;
  "update.prepare_app": undefined;
  /**
   * A file the user uploaded with a message, task, recurring task or idea (see
   * `MessageFileView::id`), in parts of at most `UPLOAD_CHUNK_BYTES` from
   * `offset` (none: 0). With `max_side` an image comes back whole as a JPEG
   * that fits that many pixels (clamped to `UPLOAD_THUMB_MAX_SIDE`); other files are refused.
   */
  "upload.get": { id: string; max_side?: number | null; offset?: number | null };
  "usage.get": undefined;
  /**
   * Usage per hour of one server-local day (any moment in it).
   */
  "usage.get_hours": { day_start_ms: number };
  /**
   * What a conversation or a task used, with the tasks given away from it apart
   * (feature `run_usage`). Give one of the two; a task wins.
   */
  "usage.get_totals": { conversation_id?: string | null; task_id?: string | null; ticket_id?: string | null };
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
  "user.create": M.UserCreateParams;
  /**
   * Disable a board member: their sessions end at once and they cannot sign
   * in; their Claude login is disconnected; tickets keep their creator. The
   * last active board member cannot be disabled (`last_board_member`).
   * Answers `User`. Not for apps.
   */
  "user.disable": { user_id: string };
  /**
   * The board members, oldest first (never a password's hash), disabled ones
   * too (`UserView::disabled_at_ms`). Works without a session (the sign-in
   * screen may offer the names). Apps may read it too.
   */
  "user.list": undefined;
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
  "user.login": M.LoginParams;
  /**
   * End this connection's session (the device must sign in again); answers
   * `WhoAmI` without a board member. A session method.
   */
  "user.logout": undefined;
  /**
   * The workspaces the board member of the session pinned to their rail
   * (feature `rail_pins`), never another's. Not for apps.
   */
  "user.rail_pins": undefined;
  /**
   * Change a password. Own (no `user_id`, or the session's): with the old
   * password; the other sessions end, this one stays. Another board member's
   * (B10): a temporary password, changed at their next sign-in
   * (`must_change_password`); all their sessions end. Like `user.create`, the
   * passwords are in no receipt, request hash, event or log. Answers `Done`.
   */
  "user.set_password": M.SetPasswordParams;
  /**
   * `user.set_rail_pins` (feature `rail_pins`): the workspaces the board member of
   * the session pins to their rail, in rail order. Replaces the whole list, so
   * reordering and unpinning are the same write; an empty list is a choice too
   * ("none"). Answers `RailPinsSet`.
   */
  "user.set_rail_pins": { project_ids: string[] };
  /**
   * Whether the server has board members, and this connection's session's one.
   * Works without a session.
   */
  "user.whoami": undefined;
  /**
   * Archive ("Delete") an item: runs and employees no longer get it; its audit
   * history stays. Answers `VaultItem`. Not for apps.
   */
  "vault.archive": { item_id: string };
  /**
   * The audit log, newest first (`VaultAuditPage`): the company's (nothing
   * given), a project's, or every scope's (`all`); one item's with `item_id`.
   * `before`: rows older than that row id; `limit` at most 200 (default 50).
   * Never a value. Not for apps.
   */
  "vault.audit": { all?: boolean; before?: number | null; item_id?: string | null; limit?: number | null; project_id?: string | null };
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
  "vault.create": M.VaultCreateParams;
  /**
   * The server's vault key (`VaultKeyExport`, never printed), for a restore on
   * another machine (B8): a board member with a session that needs no new
   * password, who gives their password again (`wrong_password`; it counts
   * for the sign-in limits, `too_many_attempts`). Writes an audit row
   * `key_export` and tells every board member. Not found before the first
   * item. Not for apps.
   */
  "vault.key_export": M.KeyExportParams;
  /**
   * The vault of a scope (feature `vault`): none, the company's items; a
   * project, its items plus the company's apart (`VaultList::company_items`),
   * with `overrides_company` on a project item whose env name the company
   * has too, and the item count of every scope. Never a secret's value.
   * Archived items are left out. Board members only; not for apps (B5).
   */
  "vault.list": { project_id?: string | null };
  /**
   * One item's value (`VaultValue`, never printed), for a board member with a
   * session. Writes an audit row `reveal` (show) or `copy` with the session's
   * device. `vault_undecryptable` when the key does not fit. Not for apps.
   */
  "vault.reveal": { item_id: string; purpose: M.RevealPurpose };
  /**
   * Change an item; what is left out stays. `value`: a new value (a secret's
   * is never sent back to the editor, so "leave empty to keep"). `kind` only
   * between text and url: to or from secret is `vault_kind_locked` (B7).
   * With `expected_version` in the envelope a stale edit is `version_conflict`.
   * Answers `VaultItem`. As `vault.create`: no value in receipt, hash or event.
   */
  "vault.update": M.VaultUpdateParams;
  "work_session.assign": M.AssignWorkSession;
  "work_session.dispatch": M.DispatchWorkSession;
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
  "work_session.history": { before_conversation_id?: string | null; before_last_activity_ms?: number | null; employee_id?: string | null; from_ms?: number | null; limit?: number | null; project_id?: string | null; query?: string | null; to_ms?: number | null };
  /**
   * The reader's work sessions, the latest activity first (feature `work_sessions`);
   * `project_id`: only those of that project.
   */
  "work_session.list": { project_id?: string | null };
  "work_session.retry_summary": M.RetrySessionSummary;
  "work_session.start": M.StartWorkSession;
}
