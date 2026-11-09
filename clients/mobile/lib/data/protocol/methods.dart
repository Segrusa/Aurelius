// GENERATED CODE - DO NOT EDIT.
// Run: cargo xtask gen-dart
// ignore_for_file: unnecessary_cast, unnecessary_null_aware_operator, unnecessary_non_null_assertion, constant_identifier_names, use_null_aware_elements, unintended_html_in_doc_comment

import 'models.dart';

/// The protocol major this app speaks. The server refuses another one.
const int protocolMajor = 5;

/// The wire name of every method.
abstract final class Methods {
  /// One tool call of an answer whole: the input and output that
  /// `conversation.get` and live updates shorten (feature `activity_preview`).
  static const String activityGet = 'activity.get';
  /// The activity log, newest first (R8, feature `activity_log`): every delivery, queue
  /// step and run step, narrowed by what is given. `limit`: at most 200, default 100.
  static const String activityLogList = 'activity_log.list';
  static const String agentFunctionCreate = 'agent_function.create';
  static const String agentFunctionDelete = 'agent_function.delete';
  static const String agentFunctionList = 'agent_function.list';
  /// The kill switch: `enabled: false` makes every function refuse calls.
  static const String agentFunctionSetAllEnabled = 'agent_function.set_all_enabled';
  static const String agentFunctionSetEnabled = 'agent_function.set_enabled';
  static const String agentFunctionUpdate = 'agent_function.update';
  /// The secret is in the answer once; the receipt keeps none.
  static const String apiKeyCreate = 'api_key.create';
  static const String apiKeyList = 'api_key.list';
  static const String apiKeyRevoke = 'api_key.revoke';
  static const String apiKeySetFunctions = 'api_key.set_functions';
  /// A key for an outside app (feature `app_keys`); answers `AppKeyCreated`
  /// with the secret, once: the receipt keeps none.
  static const String appKeyCreate = 'app_key.create';
  /// The keys of outside apps that use `aurelius-mcp`, newest first (feature `app_keys`).
  static const String appKeyList = 'app_key.list';
  /// Revoked at once, also for the app's open sessions; answers `AppKey`.
  static const String appKeyRevoke = 'app_key.revoke';
  static const String approvalAnswer = 'approval.answer';
  static const String approvalDecide = 'approval.decide';
  static const String approvalDecidePlan = 'approval.decide_plan';
  static const String approvalList = 'approval.list';
  /// The file the server kept of one publish (`version`, else the newest kept),
  /// so a client can show the page without a claude.ai login. Not found for an
  /// unknown URL; `snapshot` is none for pages published before servers kept them.
  static const String artifactGetContent = 'artifact.get_content';
  /// Whether runs may publish Claude Code artifacts (pages on claude.ai).
  static const String artifactGetSetting = 'artifact.get_setting';
  /// Artifacts runs published, newest first: every given filter applies, none lists all.
  static const String artifactList = 'artifact.list';
  /// Turn Claude Code artifacts on or off for every run that starts afterwards.
  static const String artifactSetEnabled = 'artifact.set_enabled';
  static const String attachmentAdd = 'attachment.add';
  static const String attachmentRemove = 'attachment.remove';
  /// Whether the safe read-only tools pass without a question (the auto-allow list).
  static const String autoAllowGetSetting = 'auto_allow.get_setting';
  /// Auto-allow of the safe read-only tools on or off, from the next tool call. On by default.
  static const String autoAllowSetEnabled = 'auto_allow.set_enabled';
  static const String backupCreate = 'backup.create';
  static const String backupList = 'backup.list';
  /// Restore at the next start (the service stops now); work stays paused until resumed.
  static const String backupRestore = 'backup.restore';
  static const String backupResume = 'backup.resume';
  /// Rename the columns and set their order (B3): all of them, left to right.
  /// `expected_version` is the board's (`board_version`: the columns change
  /// together); the answer gives the new one.
  static const String boardColumnsUpdate = 'board.columns_update';
  /// The board: its six columns left to right, each with its cards (not
  /// archived), for one project or every project.
  static const String boardGet = 'board.get';
  static const String browserGet = 'browser.get';
  static const String browserSet = 'browser.set';
  /// Claude in Chrome or Playwright, for every run that starts afterwards.
  static const String browserSetBackend = 'browser.set_backend';
  /// The channels of the Team chat (feature `team_channels`): the general
  /// one first, then one per project (archived projects too: read-only).
  static const String channelList = 'channel.list';
  /// The channels whose name or ticket number matches `query` (feature
  /// `ticket_channels`), in the order of `channel.list`, archived ones too: a
  /// closed ticket's channel stays findable. `#12` and `12` match ticket 12;
  /// other text matches the name, case not counting. A connection that does not
  /// accept `ticket_channels` gets no ticket channels here either.
  static const String channelSearch = 'channel.search';
  static const String chatRetry = 'chat.retry';
  static const String chatSend = 'chat.send';
  /// Stop a running link of the session's own login. Answers `ClaudeLogin`.
  static const String claudeLoginCancel = 'claude_login.cancel';
  /// What a running `claude_login.link` asks for: the code Claude showed after
  /// the sign-in at Anthropic, written to `claude auth login` as typed. Only
  /// the connection that started the link. A session method: no receipt, no
  /// request hash, no event, never printed or logged. Answers `ClaudeLogin`.
  static const String claudeLoginInput = 'claude_login.input';
  /// Connect the session's own Claude login: the server starts `claude auth
  /// login` for `<data>/logins/<user_id>/` and answers `ClaudeLoginLink`. What
  /// Claude asks (the sign-in link, the code) comes to this connection only as
  /// `ServerMessage::ClaudeLoginPrompt`; the code goes back with
  /// `claude_login.input`. One link per board member at a time, 10 minutes.
  static const String claudeLoginLink = 'claude_login.link';
  /// The session's own Claude login (`ClaudeLoginView`). Feature of protocol 5.
  static const String claudeLoginStatus = 'claude_login.status';
  /// Disconnect the session's own Claude login (`claude auth logout`, its folder
  /// removed). Answers `ClaudeLogin`.
  static const String claudeLoginUnlink = 'claude_login.unlink';
  /// The company in one read: employees, the org chart setting and the
  /// management team (roots).
  static const String companyGet = 'company.get';
  /// The company's background jobs (memory consolidation, B6) run on the Claude
  /// login of the board member who asks (`enabled`), or on none (`false`, only
  /// when it is theirs): without one they do not run. Only the own login can be
  /// chosen. Answers `CompanyUpdated`.
  static const String companySetBackgroundLogin = 'company.set_background_login';
  static const String companySetFollowOrgChart = 'company.set_follow_org_chart';
  static const String companyUpdate = 'company.update';
  static const String connectionAdd = 'connection.add';
  /// Where Claude Code is installed on the server.
  static const String connectionDetectRuntime = 'connection.detect_runtime';
  static const String connectionList = 'connection.list';
  static const String connectionRemove = 'connection.remove';
  static const String connectionSetDefaults = 'connection.set_defaults';
  static const String connectionVerify = 'connection.verify';
  static const String conversationDelete = 'conversation.delete';
  static const String conversationDeleteQueued = 'conversation.delete_queued';
  /// A conversation's messages and settings.
  static const String conversationGet = 'conversation.get';
  static const String conversationSendQueuedNow = 'conversation.send_queued_now';
  static const String conversationSetAgentMode = 'conversation.set_agent_mode';
  static const String conversationSetBrowserAllowed = 'conversation.set_browser_allowed';
  static const String conversationSetRun = 'conversation.set_run';
  static const String conversationSetScreenAllowed = 'conversation.set_screen_allowed';
  /// An employee's persistent chat with the user (company scope, no project):
  /// its messages and settings, as `conversation.get` answers them.
  static const String employeeChat = 'employee.chat';
  /// The reader's lasting chats with employees, the latest first (feature `employee_chats`).
  static const String employeeChats = 'employee.chats';
  static const String employeeCreate = 'employee.create';
  /// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
  static const String employeeInstructionVersions = 'employee.instruction_versions';
  /// The company's employees (not archived), oldest first.
  static const String employeeList = 'employee.list';
  static const String employeeOpenChat = 'employee.open_chat';
  /// What is in an employee's mailbox now (feature `employee_queue`).
  static const String employeeQueue = 'employee.queue';
  static const String employeeRestoreInstructions = 'employee.restore_instructions';
  /// An employee's scratchpad (feature `employee_scratchpad`).
  static const String employeeScratchpad = 'employee.scratchpad';
  static const String employeeSendMessage = 'employee.send_message';
  static const String employeeSetFolder = 'employee.set_folder';
  static const String employeeSetReportsTo = 'employee.set_reports_to';
  static const String employeeSetStatus = 'employee.set_status';
  /// The catalogue of start profiles (feature `employee_profiles`).
  static const String employeeStartProfiles = 'employee.start_profiles';
  /// The tools that can be switched per employee (feature `employee_tools`).
  static const String employeeToolOptions = 'employee.tool_options';
  static const String employeeUpdate = 'employee.update';
  /// Replay events after `after_seq`, then stream live events (and load/power pushes).
  static const String eventSubscribe = 'event.subscribe';
  /// A file kept with a message (see `MessageView::files`).
  static const String fileGet = 'file.get';
  /// A new folder `name` in the folder `parent`.
  static const String fsCreateDir = 'fs.create_dir';
  /// Folders on the server: none lists the starting points.
  static const String fsListDir = 'fs.list_dir';
  static const String functionCallList = 'function_call.list';
  static const String gitGetDiff = 'git.get_diff';
  /// Branch and uncommitted change of the folder a conversation works in; none outside a Git repository.
  static const String gitGetStatus = 'git.get_status';
  /// The company's group chat (Team chat): the user and every employee. Its
  /// messages and settings, as `conversation.get` answers them; an employee's
  /// messages carry its name as `author`. Write with `group_chat.send`.
  /// `channel` (feature `team_channels`): a project's channel from
  /// `channel.list`; none: the general channel, the one older servers have.
  static const String groupChatGet = 'group_chat.get';
  static const String groupChatSend = 'group_chat.send';
  /// `guideline.add_starter_set`: the chosen templates as guidelines without links (B7).
  /// A key whose title is in the library already is skipped.
  static const String guidelineAddStarterSet = 'guideline.add_starter_set';
  /// Who a change of this guideline reaches from their next run (B6): employees,
  /// projects and conversations that start a new session.
  static const String guidelineAffected = 'guideline.affected';
  /// `guideline.archive`: no run gets it any more; links and versions stay.
  static const String guidelineArchive = 'guideline.archive';
  /// `guideline.create`: a new guideline (version 1, source `created`), linked at once to `links`.
  static const String guidelineCreate = 'guideline.create';
  /// `guideline.decide_proposal` (B5): approve (with `title`/`text` edited by the
  /// board member, and for a new guideline the `links`; none: the suggested
  /// ones) or reject. `note` goes back to the proposer. `version_conflict` when
  /// the guideline moved on since the proposal's base version.
  static const String guidelineDecideProposal = 'guideline.decide_proposal';
  /// The set a run of this employee in this project would get now, in block
  /// order, with `total_chars` against `limit_chars` (B4). Only an employee:
  /// its own chat (employee guidelines); only a project: its guidelines alone.
  static const String guidelineEffective = 'guideline.effective';
  /// One guideline with the text of its current version, or of `version`.
  static const String guidelineGet = 'guideline.get';
  /// `guideline.link`: an employee or a project gets it from its next run.
  static const String guidelineLink = 'guideline.link';
  /// The guideline library (feature `guidelines`): every guideline without its
  /// text, alphabetical; archived ones only with `archived` (then only those).
  static const String guidelineList = 'guideline.list';
  /// Proposals of `propose_guideline` (B5), pending first, then the latest
  /// decisions; `status` filters. Apart from `proposal.list`.
  static const String guidelineProposals = 'guideline.proposals';
  /// `guideline.restore`: back from the archive, with its links (B4 and the title are checked again).
  static const String guidelineRestore = 'guideline.restore';
  /// `guideline.restore_version`: a copy of `version` as the newest (source `restored`).
  static const String guidelineRestoreVersion = 'guideline.restore_version';
  /// The templates of the starter set (B7), for "Add starter set…".
  static const String guidelineStarterSet = 'guideline.starter_set';
  /// `guideline.unlink`.
  static const String guidelineUnlink = 'guideline.unlink';
  /// `guideline.update`: a new version with what changed (source `edited`); what
  /// is left out stays. `expected_version` in the envelope is the version the
  /// editor started from (`version_conflict` otherwise).
  static const String guidelineUpdate = 'guideline.update';
  /// Every version of a guideline, newest first, with its text and source.
  static const String guidelineVersions = 'guideline.versions';
  static const String heartbeatSet = 'heartbeat.set';
  static const String ideaAdd = 'idea.add';
  static const String ideaDelete = 'idea.delete';
  static const String ideaList = 'idea.list';
  /// The ideas of every project (each keeps its own project).
  static const String ideaListAll = 'idea.list_all';
  static const String ideaSetStatus = 'idea.set_status';
  static const String ideaStart = 'idea.start';
  static const String ideaUpdate = 'idea.update';
  static const String jobList = 'job.list';
  static const String loadGet = 'load.get';
  static const String loadSetLimits = 'load.set_limits';
  /// Whether management chooses its own task mode (feature `management_chooses_mode`).
  static const String managementModeGetSetting = 'management_mode.get_setting';
  /// Management chooses its own task mode (feature `management_chooses_mode`), for every run
  /// that starts afterwards. On by default.
  static const String managementModeSetEnabled = 'management_mode.set_enabled';
  /// One Markdown file as UTF-8 text. `path` is relative to the work folder.
  static const String markdownGet = 'markdown.get';
  /// The Markdown files in a work folder, for the in-app viewer.
  static const String markdownList = 'markdown.list';
  static const String mcpAdd = 'mcp.add';
  /// The user-scope MCP servers of Claude Code and which of them every run uses.
  static const String mcpList = 'mcp.list';
  /// Finishes a login with the address the browser ended on (`…/callback?code=…&state=…`),
  /// for a browser that is not on the server's computer.
  static const String mcpLoginComplete = 'mcp.login_complete';
  /// A login started with `mcp.login_start`; kept for a while after it ends.
  static const String mcpLoginGet = 'mcp.login_get';
  /// Starts an OAuth login for an http/sse server; answers `McpLogin` with the URL to open.
  static const String mcpLoginStart = 'mcp.login_start';
  /// Forgets the server's tokens (keeps its client registration).
  static const String mcpLogout = 'mcp.logout';
  static const String mcpRemove = 'mcp.remove';
  static const String mcpSet = 'mcp.set';
  /// The MCP tools the user always allows (answered an approval with 'Always allow').
  static const String mcpAlwaysAllowList = 'mcp_always_allow.list';
  /// Take a tool off the always-allow list: its next call asks again.
  static const String mcpAlwaysAllowRemove = 'mcp_always_allow.remove';
  static const String memoryConsolidate = 'memory.consolidate';
  static const String memoryDecide = 'memory.decide';
  static const String memoryForget = 'memory.forget';
  /// What goes into a context: an employee's (own, the project if given,
  /// global) or a project conversation's (project, global; no employee). At
  /// least one of the two.
  static const String memoryGetContext = 'memory.get_context';
  /// Memories, newest first: global always, plus the scopes asked for (a
  /// project, an employee). Nothing asked: global and every employee.
  static const String memoryList = 'memory.list';
  static const String memoryPin = 'memory.pin';
  static const String memoryRemember = 'memory.remember';
  static const String memoryRevise = 'memory.revise';
  static const String messageDelete = 'message.delete';
  static const String messageEdit = 'message.edit';
  /// The earlier texts of a message, newest first (feature `message_edits`);
  /// none for a deleted one. Not for apps.
  static const String messageHistory = 'message.history';
  /// Where a message is (feature `message_locate`), for a link to it: its
  /// conversation, ticket and thread. Only a message of a channel or a ticket's
  /// discussion; any other is `not_found`, as an unknown one. Not for apps.
  static const String messageLocate = 'message.locate';
  static const String messageReact = 'message.react';
  /// A new note in a project; answers `Note`.
  static const String noteCreate = 'note.create';
  static const String noteDelete = 'note.delete';
  /// One note with its Markdown body.
  static const String noteGet = 'note.get';
  /// A project's notes (summaries), pinned first, then the last edited; `query` searches the text.
  static const String noteList = 'note.list';
  static const String noteSetPinned = 'note.set_pinned';
  /// Replace a note's body. Refused with `version_conflict` when the note is no
  /// longer at `base_revision` (saved elsewhere): load it again with `note.get`.
  static const String noteUpdate = 'note.update';
  static const String notificationDismiss = 'notification.dismiss';
  static const String notificationDismissAll = 'notification.dismiss_all';
  static const String notificationList = 'notification.list';
  static const String notificationMarkAllRead = 'notification.mark_all_read';
  /// One row read, for this board member only (feature `personal_inbox`); answers
  /// `Count`, 0 when there was nothing to read, so it can be repeated.
  static const String notificationMarkRead = 'notification.mark_read';
  /// The plugins one employee has enabled.
  static const String pluginGetForEmployee = 'plugin.get_for_employee';
  /// The plugins a project has enabled (its project assistant loads them).
  static const String pluginGetForProject = 'plugin.get_for_project';
  /// Install plugins (`name@marketplace`) and enable them for the given employees and projects.
  static const String pluginInstall = 'plugin.install';
  /// Claude Code plugins installed on the server's computer, with the employees that use each.
  static const String pluginList = 'plugin.list';
  /// Plugins offered by the marketplaces already added.
  static const String pluginListAvailable = 'plugin.list_available';
  /// Adds the marketplace behind a link (GitHub URL, `owner/repo` or git URL).
  static const String pluginPreview = 'plugin.preview';
  static const String pluginRemove = 'plugin.remove';
  /// Replace the plugins an employee has enabled; applies from its next run.
  static const String pluginSetForEmployee = 'plugin.set_for_employee';
  /// Replace the plugins a project has enabled (for its project assistant); applies from its next run.
  static const String pluginSetForProject = 'plugin.set_for_project';
  static const String pluginUpdate = 'plugin.update';
  static const String powerCancel = 'power.cancel';
  static const String powerGet = 'power.get';
  static const String powerShutdown = 'power.shutdown';
  static const String projectCreate = 'project.create';
  /// Write a readable Markdown + JSON export of a project into its folder.
  static const String projectExport = 'project.export';
  static const String projectList = 'project.list';
  static const String projectRename = 'project.rename';
  static const String projectSetDescription = 'project.set_description';
  static const String projectSetFolder = 'project.set_folder';
  static const String projectSetStatus = 'project.set_status';
  static const String projectUpdate = 'project.update';
  /// The conversations of a project, most recently active first.
  static const String projectConversationList = 'project_conversation.list';
  static const String projectConversationSend = 'project_conversation.send';
  static const String projectConversationStart = 'project_conversation.start';
  static const String proposalDecide = 'proposal.decide';
  /// Proposals of the whole company (colleagues and instructions): pending
  /// first, then the latest decisions.
  static const String proposalList = 'proposal.list';
  static const String pushConfigure = 'push.configure';
  static const String pushTest = 'push.test';
  static const String recurringTaskCreate = 'recurring_task.create';
  static const String recurringTaskDelete = 'recurring_task.delete';
  static const String recurringTaskList = 'recurring_task.list';
  static const String recurringTaskRunNow = 'recurring_task.run_now';
  static const String recurringTaskSetPaused = 'recurring_task.set_paused';
  static const String recurringTaskUpdate = 'recurring_task.update';
  /// Allow or refuse a device that entered the code. A computer that asks for
  /// full access is only allowed with `owner` (the user confirmed it).
  static const String remoteDecidePairing = 'remote.decide_pairing';
  static const String remoteDisable = 'remote.disable';
  static const String remoteEnable = 'remote.enable';
  static const String remoteGetStatus = 'remote.get_status';
  static const String remoteRevoke = 'remote.revoke';
  /// A new one-time code (5 minutes) plus the QR payload; the receipt keeps neither.
  static const String remoteStartPairing = 'remote.start_pairing';
  /// Every tool call of a run, for an opened run card (feature `run_cards`).
  static const String runGetActivity = 'run.get_activity';
  /// The context manifest of one run (context inspector).
  static const String runGetContext = 'run.get_context';
  /// What one run sent to the runtime and what each model call cost; not found
  /// unless trace mode was on for it and it is among the latest (feature `run_trace`).
  static const String runGetTrace = 'run.get_trace';
  /// The guideline versions a run had in its system text (B6), kept for every
  /// run (the trace's "Guidelines: Rust v3, …"); also without trace mode.
  static const String runGuidelines = 'run.guidelines';
  static const String runStop = 'run.stop';
  static const String scheduledMessageCancel = 'scheduled_message.cancel';
  /// Messages employees scheduled: what waits, then what went out (feature `scheduled_messages`).
  static const String scheduledMessageList = 'scheduled_message.list';
  /// A screenshot the agent took (the id is in its screenshot tool's output).
  static const String screenshotGet = 'screenshot.get';
  static const String secretSet = 'secret.set';
  static const String serverGetHealth = 'server.get_health';
  /// The server's name, its source and the computer's name (feature `server_name`).
  static const String serverGetName = 'server.get_name';
  static const String serverRestart = 'server.restart';
  /// Choose the server's name (`server.set_name`). Empty or only spaces puts the
  /// computer's name back. The same name as now changes nothing (no event).
  /// Answers `ServerNameSet`.
  static const String serverSetName = 'server.set_name';
  /// The first request of a session: who is calling. The protocol major in
  /// every request must equal the server's, else the answer is a hard error.
  static const String sessionHello = 'session.hello';
  /// The signed-in devices of a board member (none: the session's own),
  /// newest activity first (`SessionView::current` marks this one). Any board
  /// member may list another's (one role). Never a token or its hash.
  static const String sessionList = 'session.list';
  /// End one session (any board member's: one role). Its device's open
  /// connection closes at once (`ServerMessage::SignedOut`); the timeline says
  /// who revoked it. Answers `Done`. Not for apps.
  static const String sessionRevoke = 'session.revoke';
  static const String shareCreate = 'share.create';
  static const String shareList = 'share.list';
  static const String shareRevoke = 'share.revoke';
  /// The subagents running in the answers of one conversation.
  static const String subagentList = 'subagent.list';
  static const String subscriptionGet = 'subscription.get';
  static const String taskAssign = 'task.assign';
  static const String taskCancel = 'task.cancel';
  static const String taskChooseVariant = 'task.choose_variant';
  static const String taskCreate = 'task.create';
  static const String taskDiscuss = 'task.discuss';
  /// Tasks, newest first, company-wide; every given filter applies.
  static const String taskList = 'task.list';
  /// A task's variants and the judge's comparison (feature `task_variants`).
  static const String taskVariants = 'task.variants';
  static const String terminalClose = 'terminal.close';
  /// This session stops getting a terminal's output; the shell keeps running and
  /// keeps its scrollback (feature `terminal.multi`).
  static const String terminalDetach = 'terminal.detach';
  static const String terminalInput = 'terminal.input';
  /// The terminals of a conversation, oldest first (feature `terminal.multi`).
  static const String terminalList = 'terminal.list';
  /// A shell in the folder a conversation works in. Attaches to the running terminal
  /// unless `fresh`; this session then gets its output as `ServerMessage::TerminalOutput`.
  /// With `terminal_id` it attaches to exactly that terminal (its scrollback follows);
  /// with `additional` it starts another terminal beside the running ones
  /// (feature `terminal.multi`; at most 4 per conversation, 8 per server).
  static const String terminalOpen = 'terminal.open';
  static const String terminalResize = 'terminal.resize';
  /// One thread of a channel (feature `chat_threads`): `message_id` is the
  /// message that starts it (a message of a channel or a ticket's discussion
  /// that is not itself a reply), answered with the message and its replies,
  /// oldest first (`ThreadSnapshot`). The board member who reads it has read
  /// the thread (its `thread_unread` goes to none); reading the channel never
  /// does. A message without replies is a thread of none.
  static const String threadRead = 'thread.read';
  /// Who waits to pick up a thread and what is scheduled in it (U3, feature
  /// `scheduled_messages`): a ticket (`ticket_id`) or a channel or chat (`conversation_id`).
  static const String threadWaiting = 'thread.waiting';
  /// Review → Done. Only a board member; only when no linked task still works (B6).
  static const String ticketAccept = 'ticket.accept';
  /// Plan review → Doing: the owner carries the plan out.
  static const String ticketApprovePlan = 'ticket.approve_plan';
  /// Take a ticket off the board (from Done or Backlog); it stays readable.
  static const String ticketArchive = 'ticket.archive';
  /// Choose the owner of a ticket before its main task exists: a board member
  /// (instead of the dispatcher) or the dispatcher itself. Gives `ticket.dispatched`.
  ///
  /// `employee_id` none: no owner again, so the dispatcher chooses when it
  /// starts. Only in Backlog (before the start); gives `ticket.updated` with
  /// `changed: ["owner"]`, not `ticket.dispatched` (nobody was chosen).
  static const String ticketAssign = 'ticket.assign';
  /// Stop the work on a ticket: its main task is cancelled and it goes back to Backlog.
  static const String ticketCancel = 'ticket.cancel';
  /// A new ticket in Backlog (`start`: straight to Todo, as `ticket.start` would).
  static const String ticketCreate = 'ticket.create';
  static const String ticketFollow = 'ticket.follow';
  /// Who follows a ticket (feature `ticket_follow`). Not for apps.
  static const String ticketFollowers = 'ticket.followers';
  /// One ticket with its main task and every task linked to it.
  static const String ticketGet = 'ticket.get';
  /// Tickets in board order (not archived); every given filter applies. `query`
  /// searches title, description and `#number`.
  static const String ticketList = 'ticket.list';
  /// Drag a card. Within its column: only the order. To Todo: start it. To
  /// Backlog: cancel it. Done only through `ticket.accept`, and Review back to
  /// Doing only through `ticket.send_back`; out of Testing only to Backlog (cancel);
  /// other moves follow the work and are refused.
  /// The place is relative to a neighbour, so it means the same on a board
  /// filtered by project as on the whole board. A move within the same column
  /// also gives `ticket.moved` (with `from` = `to`). A card put above itself in
  /// its own column stays where it is: no event, the version is unchanged.
  /// Unknown fields (such as the old `position`) are refused.
  static const String ticketMove = 'ticket.move';
  /// The tickets of the board member of the session (feature `my_tickets`),
  /// never of one named in a param: responsible, created or followed by them,
  /// and open ones on their login or without an active responsible. Done and
  /// archived ones only with `include_done`. By priority, then the latest
  /// update; at most `MY_TICKETS_MAX`. Not for apps.
  static const String ticketMyTickets = 'ticket.my_tickets';
  /// A board member writes in a ticket's discussion (feature `ticket_discussion`).
  /// Without `@Name` it goes to the owner while the ticket is in Todo (with an
  /// owner), Doing, Plan review, Testing or Review; else it wakes nobody. `@Name` wakes those
  /// employees. Refused on Done (the discussion is read-only there). Not for apps.
  static const String ticketPost = 'ticket.post';
  /// Try the summary of a ticket on Done again after it failed (`SystemLineKind::SummaryFailed`).
  /// Only a board member; refused when the summary is already there or still being made.
  static const String ticketRetrySummary = 'ticket.retry_summary';
  /// Plan review → Doing in Plan mode: the owner makes a new plan with the feedback.
  static const String ticketRevisePlan = 'ticket.revise_plan';
  /// Review → Doing with feedback (required) for the owner.
  static const String ticketSendBack = 'ticket.send_back';
  /// Choose the board member responsible for a ticket (`ticket.set_responsible`,
  /// feature `ticket_responsible`). Its own command rather than a field of
  /// `UpdateTicket`, where "nobody" and "unchanged" would need two kinds of none.
  static const String ticketSetResponsible = 'ticket.set_responsible';
  /// The Start button: Backlog to Todo. With an employee the dispatcher is skipped.
  /// Also starts again a ticket whose main task failed.
  static const String ticketStart = 'ticket.start';
  /// Change the fields of a ticket. Left out: unchanged. The creator never
  /// changes; the owner only through `ticket.assign`; mode and model only while
  /// the main task does not run.
  static const String ticketUpdate = 'ticket.update';
  /// Run a ticket on the Claude login of the board member who asks (B4, D3-3):
  /// `TicketView::login_user` becomes that board member, never anyone else. The
  /// creator stays. Only a board member; answers `TicketChanged`.
  static const String ticketUseMyLogin = 'ticket.use_my_login';
  /// Human-readable timeline cards, newest first (keyset on `seq`).
  static const String timelineList = 'timeline.list';
  /// Whether runs keep a trace (feature `run_trace`).
  static const String traceGetSetting = 'trace.get_setting';
  /// Trace mode on or off for every run that starts afterwards (feature `run_trace`).
  /// Off also forgets the traces kept so far.
  static const String traceSetEnabled = 'trace.set_enabled';
  static const String updateApply = 'update.apply';
  static const String updateApplyApp = 'update.apply_app';
  /// Update status; `check` asks GitHub for the latest release first.
  static const String updateGet = 'update.get';
  static const String updatePrepare = 'update.prepare';
  static const String updatePrepareApp = 'update.prepare_app';
  /// A file the user uploaded with a message, task, recurring task or idea (see
  /// `MessageFileView::id`), in parts of at most `UPLOAD_CHUNK_BYTES` from
  /// `offset` (none: 0). With `max_side` an image comes back whole as a JPEG
  /// that fits that many pixels (clamped to `UPLOAD_THUMB_MAX_SIDE`); other files are refused.
  static const String uploadGet = 'upload.get';
  static const String usageGet = 'usage.get';
  /// Usage per hour of one server-local day (any moment in it).
  static const String usageGetHours = 'usage.get_hours';
  /// What a conversation or a task used, with the tasks given away from it apart
  /// (feature `run_usage`). Give one of the two; a task wins.
  static const String usageGetTotals = 'usage.get_totals';
  /// A new board member (OV-12: `invalid_username`, `weak_password` (also a
  /// password of only spaces), `username_taken`; `too_many_attempts` after five
  /// a minute on one connection or twenty an hour on the server); answers
  /// `CommandResult::UserCreated` as a command receipt. Without a session only
  /// the first board member (the onboarding): that connection then gets a
  /// session, whose token `user.login` would give (see `HelloResult::user`;
  /// the client signs in once more to get the token). After that it needs a
  /// session (any board member, one role). The server hashes the password
  /// before anything is recorded: the receipt, its request hash and the event
  /// `user.created` hold the name only. Not for apps.
  static const String userCreate = 'user.create';
  /// Disable a board member: their sessions end at once and they cannot sign
  /// in; their Claude login is disconnected; tickets keep their creator. The
  /// last active board member cannot be disabled (`last_board_member`).
  /// Answers `User`. Not for apps.
  static const String userDisable = 'user.disable';
  /// The board members, oldest first (never a password's hash), disabled ones
  /// too (`UserView::disabled_at_ms`). Works without a session (the sign-in
  /// screen may offer the names). Apps may read it too.
  static const String userList = 'user.list';
  /// Sign in (protocol 5): a right username and password give this connection a
  /// session and answer `Login` with its token, shown once (keep it for the
  /// server and give it in every hello). A wrong password, an unknown username
  /// and a disabled board member all answer `wrong_password`; after 5 failed tries
  /// for one username the server waits longer each time, up to 15 minutes
  /// (`too_many_attempts` with `retry_after_ms`). A session method: it changes
  /// only the connection and the sessions, so it has no receipt and no event.
  /// Not for apps.
  static const String userLogin = 'user.login';
  /// End this connection's session (the device must sign in again); answers
  /// `WhoAmI` without a board member. A session method.
  static const String userLogout = 'user.logout';
  /// Change a password. Own (no `user_id`, or the session's): with the old
  /// password; the other sessions end, this one stays. Another board member's
  /// (B10): a temporary password, changed at their next sign-in
  /// (`must_change_password`); all their sessions end. Like `user.create`, the
  /// passwords are in no receipt, request hash, event or log. Answers `Done`.
  static const String userSetPassword = 'user.set_password';
  /// Whether the server has board members, and this connection's session's one.
  /// Works without a session.
  static const String userWhoami = 'user.whoami';
  /// Whether tasks may be tried in several ways first (feature `task_variants`).
  static const String variantsGetSetting = 'variants.get_setting';
  /// Allow tasks to be tried in several ways first (feature `task_variants`; off by default).
  static const String variantsSetEnabled = 'variants.set_enabled';
  /// Archive ("Delete") an item: runs and employees no longer get it; its audit
  /// history stays. Answers `VaultItem`. Not for apps.
  static const String vaultArchive = 'vault.archive';
  /// The audit log, newest first (`VaultAuditPage`): the company's (nothing
  /// given), a project's, or every scope's (`all`); one item's with `item_id`.
  /// `before`: rows older than that row id; `limit` at most 200 (default 50).
  /// Never a value. Not for apps.
  static const String vaultAudit = 'vault.audit';
  /// A new vault item (feature `vault`), in the company (no `project_id`) or a
  /// project. Answers `VaultItem` (`value` only for text/url). The server
  /// encrypts the value before anything is recorded: the receipt, its request
  /// hash and the event `vault.item_created` hold no value (MU4-05 AC2).
  /// `validation` for a name or env name used in the scope, an env name that
  /// is not UPPER_SNAKE or that the runtime reserves (`PATH`, `ANTHROPIC_*`,
  /// …), a url without `https://`/`http://`, a secret under 8 characters.
  /// Board members with a session only (employees and agent tools never); not for apps.
  static const String vaultCreate = 'vault.create';
  /// The server's vault key (`VaultKeyExport`, never printed), for a restore on
  /// another machine (B8): a board member with a session that needs no new
  /// password, who gives their password again (`wrong_password`; it counts
  /// for the sign-in limits, `too_many_attempts`). Writes an audit row
  /// `key_export` and tells every board member. Not found before the first
  /// item. Not for apps.
  static const String vaultKeyExport = 'vault.key_export';
  /// The vault of a scope (feature `vault`): none, the company's items; a
  /// project, its items plus the company's apart (`VaultList::company_items`),
  /// with `overrides_company` on a project item whose env name the company
  /// has too, and the item count of every scope. Never a secret's value.
  /// Archived items are left out. Board members only; not for apps (B5).
  static const String vaultList = 'vault.list';
  /// One item's value (`VaultValue`, never printed), for a board member with a
  /// session. Writes an audit row `reveal` (show) or `copy` with the session's
  /// device. `vault_undecryptable` when the key does not fit. Not for apps.
  static const String vaultReveal = 'vault.reveal';
  /// Change an item; what is left out stays. `value`: a new value (a secret's
  /// is never sent back to the editor, so "leave empty to keep"). `kind` only
  /// between text and url: to or from secret is `vault_kind_locked` (B7).
  /// With `expected_version` in the envelope a stale edit is `version_conflict`.
  /// Answers `VaultItem`. As `vault.create`: no value in receipt, hash or event.
  static const String vaultUpdate = 'vault.update';
}

// Reads and session methods.

/// One tool call of an answer whole: the input and output that
/// `conversation.get` and live updates shorten (feature `activity_preview`).
Map<String, dynamic> activityGet({required String callId, required String messageId}) => <String, dynamic>{
  'method': Methods.activityGet,
  'params': <String, dynamic>{
    'call_id': callId,
    'message_id': messageId,
  },
};

/// The activity log, newest first (R8, feature `activity_log`): every delivery, queue
/// step and run step, narrowed by what is given. `limit`: at most 200, default 100.
Map<String, dynamic> activityLogList({int? beforeSeq, String? category, String? employeeId, int? limit, String? runId, String? threadKey, int? ticketNumber}) => <String, dynamic>{
  'method': Methods.activityLogList,
  'params': <String, dynamic>{
    if (beforeSeq != null) 'before_seq': beforeSeq,
    if (category != null) 'category': category,
    if (employeeId != null) 'employee_id': employeeId,
    if (limit != null) 'limit': limit,
    if (runId != null) 'run_id': runId,
    if (threadKey != null) 'thread_key': threadKey,
    if (ticketNumber != null) 'ticket_number': ticketNumber,
  },
};

Map<String, dynamic> agentFunctionList({String? projectId}) => <String, dynamic>{
  'method': Methods.agentFunctionList,
  'params': <String, dynamic>{
    if (projectId != null) 'project_id': projectId,
  },
};

Map<String, dynamic> apiKeyList() => <String, dynamic>{
  'method': Methods.apiKeyList,
};

/// The keys of outside apps that use `aurelius-mcp`, newest first (feature `app_keys`).
Map<String, dynamic> appKeyList() => <String, dynamic>{
  'method': Methods.appKeyList,
};

Map<String, dynamic> approvalList() => <String, dynamic>{
  'method': Methods.approvalList,
};

/// The file the server kept of one publish (`version`, else the newest kept),
/// so a client can show the page without a claude.ai login. Not found for an
/// unknown URL; `snapshot` is none for pages published before servers kept them.
Map<String, dynamic> artifactGetContent({required String url, int? version}) => <String, dynamic>{
  'method': Methods.artifactGetContent,
  'params': <String, dynamic>{
    'url': url,
    if (version != null) 'version': version,
  },
};

/// Whether runs may publish Claude Code artifacts (pages on claude.ai).
Map<String, dynamic> artifactGetSetting() => <String, dynamic>{
  'method': Methods.artifactGetSetting,
};

/// Artifacts runs published, newest first: every given filter applies, none lists all.
Map<String, dynamic> artifactList({String? conversationId, String? projectId, String? taskId}) => <String, dynamic>{
  'method': Methods.artifactList,
  'params': <String, dynamic>{
    if (conversationId != null) 'conversation_id': conversationId,
    if (projectId != null) 'project_id': projectId,
    if (taskId != null) 'task_id': taskId,
  },
};

/// Whether the safe read-only tools pass without a question (the auto-allow list).
Map<String, dynamic> autoAllowGetSetting() => <String, dynamic>{
  'method': Methods.autoAllowGetSetting,
};

Map<String, dynamic> backupList() => <String, dynamic>{
  'method': Methods.backupList,
};

/// The board: its six columns left to right, each with its cards (not
/// archived), for one project or every project.
Map<String, dynamic> boardGet({String? projectId}) => <String, dynamic>{
  'method': Methods.boardGet,
  'params': <String, dynamic>{
    if (projectId != null) 'project_id': projectId,
  },
};

Map<String, dynamic> browserGet() => <String, dynamic>{
  'method': Methods.browserGet,
};

/// The channels of the Team chat (feature `team_channels`): the general
/// one first, then one per project (archived projects too: read-only).
Map<String, dynamic> channelList() => <String, dynamic>{
  'method': Methods.channelList,
};

/// The channels whose name or ticket number matches `query` (feature
/// `ticket_channels`), in the order of `channel.list`, archived ones too: a
/// closed ticket's channel stays findable. `#12` and `12` match ticket 12;
/// other text matches the name, case not counting. A connection that does not
/// accept `ticket_channels` gets no ticket channels here either.
Map<String, dynamic> channelSearch({required String query}) => <String, dynamic>{
  'method': Methods.channelSearch,
  'params': <String, dynamic>{
    'query': query,
  },
};

/// What a running `claude_login.link` asks for: the code Claude showed after
/// the sign-in at Anthropic, written to `claude auth login` as typed. Only
/// the connection that started the link. A session method: no receipt, no
/// request hash, no event, never printed or logged. Answers `ClaudeLogin`.
Map<String, dynamic> claudeLoginInput(ClaudeLoginInputParams params) => <String, dynamic>{
  'method': Methods.claudeLoginInput,
  'params': params.toJson(),
};

/// The session's own Claude login (`ClaudeLoginView`). Feature of protocol 5.
Map<String, dynamic> claudeLoginStatus() => <String, dynamic>{
  'method': Methods.claudeLoginStatus,
};

/// The company in one read: employees, the org chart setting and the
/// management team (roots).
Map<String, dynamic> companyGet() => <String, dynamic>{
  'method': Methods.companyGet,
};

/// Where Claude Code is installed on the server.
Map<String, dynamic> connectionDetectRuntime() => <String, dynamic>{
  'method': Methods.connectionDetectRuntime,
};

Map<String, dynamic> connectionList() => <String, dynamic>{
  'method': Methods.connectionList,
};

/// A conversation's messages and settings.
Map<String, dynamic> conversationGet({required String conversationId}) => <String, dynamic>{
  'method': Methods.conversationGet,
  'params': <String, dynamic>{
    'conversation_id': conversationId,
  },
};

/// An employee's persistent chat with the user (company scope, no project):
/// its messages and settings, as `conversation.get` answers them.
Map<String, dynamic> employeeChat({required String employeeId}) => <String, dynamic>{
  'method': Methods.employeeChat,
  'params': <String, dynamic>{
    'employee_id': employeeId,
  },
};

/// The reader's lasting chats with employees, the latest first (feature `employee_chats`).
Map<String, dynamic> employeeChats() => <String, dynamic>{
  'method': Methods.employeeChats,
};

/// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
Map<String, dynamic> employeeInstructionVersions({required String employeeId}) => <String, dynamic>{
  'method': Methods.employeeInstructionVersions,
  'params': <String, dynamic>{
    'employee_id': employeeId,
  },
};

/// The company's employees (not archived), oldest first.
Map<String, dynamic> employeeList() => <String, dynamic>{
  'method': Methods.employeeList,
};

/// What is in an employee's mailbox now (feature `employee_queue`).
Map<String, dynamic> employeeQueue({required String employeeId}) => <String, dynamic>{
  'method': Methods.employeeQueue,
  'params': <String, dynamic>{
    'employee_id': employeeId,
  },
};

/// An employee's scratchpad (feature `employee_scratchpad`).
Map<String, dynamic> employeeScratchpad({required String employeeId}) => <String, dynamic>{
  'method': Methods.employeeScratchpad,
  'params': <String, dynamic>{
    'employee_id': employeeId,
  },
};

/// The catalogue of start profiles (feature `employee_profiles`).
Map<String, dynamic> employeeStartProfiles() => <String, dynamic>{
  'method': Methods.employeeStartProfiles,
};

/// The tools that can be switched per employee (feature `employee_tools`).
Map<String, dynamic> employeeToolOptions() => <String, dynamic>{
  'method': Methods.employeeToolOptions,
};

/// Replay events after `after_seq`, then stream live events (and load/power pushes).
Map<String, dynamic> eventSubscribe({required int afterSeq}) => <String, dynamic>{
  'method': Methods.eventSubscribe,
  'params': <String, dynamic>{
    'after_seq': afterSeq,
  },
};

/// A file kept with a message (see `MessageView::files`).
Map<String, dynamic> fileGet({required String id}) => <String, dynamic>{
  'method': Methods.fileGet,
  'params': <String, dynamic>{
    'id': id,
  },
};

/// Folders on the server: none lists the starting points.
Map<String, dynamic> fsListDir({String? path}) => <String, dynamic>{
  'method': Methods.fsListDir,
  'params': <String, dynamic>{
    if (path != null) 'path': path,
  },
};

Map<String, dynamic> functionCallList({String? functionId, int? limit}) => <String, dynamic>{
  'method': Methods.functionCallList,
  'params': <String, dynamic>{
    if (functionId != null) 'function_id': functionId,
    if (limit != null) 'limit': limit,
  },
};

Map<String, dynamic> gitGetDiff({required String conversationId}) => <String, dynamic>{
  'method': Methods.gitGetDiff,
  'params': <String, dynamic>{
    'conversation_id': conversationId,
  },
};

/// Branch and uncommitted change of the folder a conversation works in; none outside a Git repository.
Map<String, dynamic> gitGetStatus({required String conversationId}) => <String, dynamic>{
  'method': Methods.gitGetStatus,
  'params': <String, dynamic>{
    'conversation_id': conversationId,
  },
};

/// The company's group chat (Team chat): the user and every employee. Its
/// messages and settings, as `conversation.get` answers them; an employee's
/// messages carry its name as `author`. Write with `group_chat.send`.
/// `channel` (feature `team_channels`): a project's channel from
/// `channel.list`; none: the general channel, the one older servers have.
Map<String, dynamic> groupChatGet({String? channel}) => <String, dynamic>{
  'method': Methods.groupChatGet,
  'params': <String, dynamic>{
    if (channel != null) 'channel': channel,
  },
};

/// Who a change of this guideline reaches from their next run (B6): employees,
/// projects and conversations that start a new session.
Map<String, dynamic> guidelineAffected({required String guidelineId}) => <String, dynamic>{
  'method': Methods.guidelineAffected,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
  },
};

/// The set a run of this employee in this project would get now, in block
/// order, with `total_chars` against `limit_chars` (B4). Only an employee:
/// its own chat (employee guidelines); only a project: its guidelines alone.
Map<String, dynamic> guidelineEffective({String? employeeId, String? projectId}) => <String, dynamic>{
  'method': Methods.guidelineEffective,
  'params': <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    if (projectId != null) 'project_id': projectId,
  },
};

/// One guideline with the text of its current version, or of `version`.
Map<String, dynamic> guidelineGet({required String guidelineId, int? version}) => <String, dynamic>{
  'method': Methods.guidelineGet,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
    if (version != null) 'version': version,
  },
};

/// The guideline library (feature `guidelines`): every guideline without its
/// text, alphabetical; archived ones only with `archived` (then only those).
Map<String, dynamic> guidelineList({bool? archived}) => <String, dynamic>{
  'method': Methods.guidelineList,
  'params': <String, dynamic>{
    if (archived != null) 'archived': archived,
  },
};

/// Proposals of `propose_guideline` (B5), pending first, then the latest
/// decisions; `status` filters. Apart from `proposal.list`.
Map<String, dynamic> guidelineProposals({ProposalStatus? status}) => <String, dynamic>{
  'method': Methods.guidelineProposals,
  'params': <String, dynamic>{
    if (status != null) 'status': status.toJson(),
  },
};

/// The templates of the starter set (B7), for "Add starter set…".
Map<String, dynamic> guidelineStarterSet() => <String, dynamic>{
  'method': Methods.guidelineStarterSet,
};

/// Every version of a guideline, newest first, with its text and source.
Map<String, dynamic> guidelineVersions({required String guidelineId}) => <String, dynamic>{
  'method': Methods.guidelineVersions,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
  },
};

Map<String, dynamic> ideaList({required String projectId}) => <String, dynamic>{
  'method': Methods.ideaList,
  'params': <String, dynamic>{
    'project_id': projectId,
  },
};

/// The ideas of every project (each keeps its own project).
Map<String, dynamic> ideaListAll() => <String, dynamic>{
  'method': Methods.ideaListAll,
  'params': <String, dynamic>{},
};

Map<String, dynamic> jobList() => <String, dynamic>{
  'method': Methods.jobList,
};

Map<String, dynamic> loadGet() => <String, dynamic>{
  'method': Methods.loadGet,
};

/// Whether management chooses its own task mode (feature `management_chooses_mode`).
Map<String, dynamic> managementModeGetSetting() => <String, dynamic>{
  'method': Methods.managementModeGetSetting,
};

/// One Markdown file as UTF-8 text. `path` is relative to the work folder.
Map<String, dynamic> markdownGet({required String path, required FileScope scope}) => <String, dynamic>{
  'method': Methods.markdownGet,
  'params': <String, dynamic>{
    'path': path,
    'scope': scope.toJson(),
  },
};

/// The Markdown files in a work folder, for the in-app viewer.
Map<String, dynamic> markdownList({required FileScope scope}) => <String, dynamic>{
  'method': Methods.markdownList,
  'params': <String, dynamic>{
    'scope': scope.toJson(),
  },
};

/// The user-scope MCP servers of Claude Code and which of them every run uses.
Map<String, dynamic> mcpList() => <String, dynamic>{
  'method': Methods.mcpList,
};

/// A login started with `mcp.login_start`; kept for a while after it ends.
Map<String, dynamic> mcpLoginGet({required String loginId}) => <String, dynamic>{
  'method': Methods.mcpLoginGet,
  'params': <String, dynamic>{
    'login_id': loginId,
  },
};

/// The MCP tools the user always allows (answered an approval with 'Always allow').
Map<String, dynamic> mcpAlwaysAllowList() => <String, dynamic>{
  'method': Methods.mcpAlwaysAllowList,
};

/// What goes into a context: an employee's (own, the project if given,
/// global) or a project conversation's (project, global; no employee). At
/// least one of the two.
Map<String, dynamic> memoryGetContext({String? employeeId, String? projectId}) => <String, dynamic>{
  'method': Methods.memoryGetContext,
  'params': <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    if (projectId != null) 'project_id': projectId,
  },
};

/// Memories, newest first: global always, plus the scopes asked for (a
/// project, an employee). Nothing asked: global and every employee.
Map<String, dynamic> memoryList({String? employeeId, String? projectId}) => <String, dynamic>{
  'method': Methods.memoryList,
  'params': <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    if (projectId != null) 'project_id': projectId,
  },
};

/// The earlier texts of a message, newest first (feature `message_edits`);
/// none for a deleted one. Not for apps.
Map<String, dynamic> messageHistory({required String messageId}) => <String, dynamic>{
  'method': Methods.messageHistory,
  'params': <String, dynamic>{
    'message_id': messageId,
  },
};

/// Where a message is (feature `message_locate`), for a link to it: its
/// conversation, ticket and thread. Only a message of a channel or a ticket's
/// discussion; any other is `not_found`, as an unknown one. Not for apps.
Map<String, dynamic> messageLocate({required String messageId}) => <String, dynamic>{
  'method': Methods.messageLocate,
  'params': <String, dynamic>{
    'message_id': messageId,
  },
};

/// One note with its Markdown body.
Map<String, dynamic> noteGet({required String noteId}) => <String, dynamic>{
  'method': Methods.noteGet,
  'params': <String, dynamic>{
    'note_id': noteId,
  },
};

/// A project's notes (summaries), pinned first, then the last edited; `query` searches the text.
Map<String, dynamic> noteList({required String projectId, String? query}) => <String, dynamic>{
  'method': Methods.noteList,
  'params': <String, dynamic>{
    'project_id': projectId,
    if (query != null) 'query': query,
  },
};

Map<String, dynamic> notificationList() => <String, dynamic>{
  'method': Methods.notificationList,
};

/// The plugins one employee has enabled.
Map<String, dynamic> pluginGetForEmployee({required String employeeId}) => <String, dynamic>{
  'method': Methods.pluginGetForEmployee,
  'params': <String, dynamic>{
    'employee_id': employeeId,
  },
};

/// The plugins a project has enabled (its project assistant loads them).
Map<String, dynamic> pluginGetForProject({required String projectId}) => <String, dynamic>{
  'method': Methods.pluginGetForProject,
  'params': <String, dynamic>{
    'project_id': projectId,
  },
};

/// Claude Code plugins installed on the server's computer, with the employees that use each.
Map<String, dynamic> pluginList() => <String, dynamic>{
  'method': Methods.pluginList,
};

/// Plugins offered by the marketplaces already added.
Map<String, dynamic> pluginListAvailable() => <String, dynamic>{
  'method': Methods.pluginListAvailable,
};

Map<String, dynamic> powerGet() => <String, dynamic>{
  'method': Methods.powerGet,
};

Map<String, dynamic> projectList() => <String, dynamic>{
  'method': Methods.projectList,
};

/// The conversations of a project, most recently active first.
Map<String, dynamic> projectConversationList({required String projectId}) => <String, dynamic>{
  'method': Methods.projectConversationList,
  'params': <String, dynamic>{
    'project_id': projectId,
  },
};

/// Proposals of the whole company (colleagues and instructions): pending
/// first, then the latest decisions.
Map<String, dynamic> proposalList() => <String, dynamic>{
  'method': Methods.proposalList,
};

Map<String, dynamic> recurringTaskList({String? projectId}) => <String, dynamic>{
  'method': Methods.recurringTaskList,
  'params': <String, dynamic>{
    if (projectId != null) 'project_id': projectId,
  },
};

Map<String, dynamic> remoteGetStatus() => <String, dynamic>{
  'method': Methods.remoteGetStatus,
};

/// Every tool call of a run, for an opened run card (feature `run_cards`).
Map<String, dynamic> runGetActivity({required String runId}) => <String, dynamic>{
  'method': Methods.runGetActivity,
  'params': <String, dynamic>{
    'run_id': runId,
  },
};

/// The context manifest of one run (context inspector).
Map<String, dynamic> runGetContext({required String runId}) => <String, dynamic>{
  'method': Methods.runGetContext,
  'params': <String, dynamic>{
    'run_id': runId,
  },
};

/// What one run sent to the runtime and what each model call cost; not found
/// unless trace mode was on for it and it is among the latest (feature `run_trace`).
Map<String, dynamic> runGetTrace({required String runId}) => <String, dynamic>{
  'method': Methods.runGetTrace,
  'params': <String, dynamic>{
    'run_id': runId,
  },
};

/// The guideline versions a run had in its system text (B6), kept for every
/// run (the trace's "Guidelines: Rust v3, …"); also without trace mode.
Map<String, dynamic> runGuidelines({required String runId}) => <String, dynamic>{
  'method': Methods.runGuidelines,
  'params': <String, dynamic>{
    'run_id': runId,
  },
};

/// Messages employees scheduled: what waits, then what went out (feature `scheduled_messages`).
Map<String, dynamic> scheduledMessageList() => <String, dynamic>{
  'method': Methods.scheduledMessageList,
};

/// A screenshot the agent took (the id is in its screenshot tool's output).
Map<String, dynamic> screenshotGet({required String id}) => <String, dynamic>{
  'method': Methods.screenshotGet,
  'params': <String, dynamic>{
    'id': id,
  },
};

Map<String, dynamic> serverGetHealth() => <String, dynamic>{
  'method': Methods.serverGetHealth,
};

/// The server's name, its source and the computer's name (feature `server_name`).
Map<String, dynamic> serverGetName() => <String, dynamic>{
  'method': Methods.serverGetName,
};

/// The first request of a session: who is calling. The protocol major in
/// every request must equal the server's, else the answer is a hard error.
Map<String, dynamic> sessionHello(HelloParams params) => <String, dynamic>{
  'method': Methods.sessionHello,
  'params': params.toJson(),
};

/// The signed-in devices of a board member (none: the session's own),
/// newest activity first (`SessionView::current` marks this one). Any board
/// member may list another's (one role). Never a token or its hash.
Map<String, dynamic> sessionList({String? userId}) => <String, dynamic>{
  'method': Methods.sessionList,
  'params': <String, dynamic>{
    if (userId != null) 'user_id': userId,
  },
};

Map<String, dynamic> shareList({required String projectId}) => <String, dynamic>{
  'method': Methods.shareList,
  'params': <String, dynamic>{
    'project_id': projectId,
  },
};

/// The subagents running in the answers of one conversation.
Map<String, dynamic> subagentList({required String conversationId}) => <String, dynamic>{
  'method': Methods.subagentList,
  'params': <String, dynamic>{
    'conversation_id': conversationId,
  },
};

Map<String, dynamic> subscriptionGet({required bool refresh}) => <String, dynamic>{
  'method': Methods.subscriptionGet,
  'params': <String, dynamic>{
    'refresh': refresh,
  },
};

/// Tasks, newest first, company-wide; every given filter applies.
Map<String, dynamic> taskList({String? employeeId, String? projectId, TaskListState? state}) => <String, dynamic>{
  'method': Methods.taskList,
  'params': <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    if (projectId != null) 'project_id': projectId,
    if (state != null) 'state': state.toJson(),
  },
};

/// A task's variants and the judge's comparison (feature `task_variants`).
Map<String, dynamic> taskVariants({required String taskId}) => <String, dynamic>{
  'method': Methods.taskVariants,
  'params': <String, dynamic>{
    'task_id': taskId,
  },
};

Map<String, dynamic> terminalClose({required String terminalId}) => <String, dynamic>{
  'method': Methods.terminalClose,
  'params': <String, dynamic>{
    'terminal_id': terminalId,
  },
};

/// This session stops getting a terminal's output; the shell keeps running and
/// keeps its scrollback (feature `terminal.multi`).
Map<String, dynamic> terminalDetach({required String terminalId}) => <String, dynamic>{
  'method': Methods.terminalDetach,
  'params': <String, dynamic>{
    'terminal_id': terminalId,
  },
};

Map<String, dynamic> terminalInput({required String dataBase64, required String terminalId}) => <String, dynamic>{
  'method': Methods.terminalInput,
  'params': <String, dynamic>{
    'data_base64': dataBase64,
    'terminal_id': terminalId,
  },
};

/// The terminals of a conversation, oldest first (feature `terminal.multi`).
Map<String, dynamic> terminalList({required String conversationId}) => <String, dynamic>{
  'method': Methods.terminalList,
  'params': <String, dynamic>{
    'conversation_id': conversationId,
  },
};

/// A shell in the folder a conversation works in. Attaches to the running terminal
/// unless `fresh`; this session then gets its output as `ServerMessage::TerminalOutput`.
/// With `terminal_id` it attaches to exactly that terminal (its scrollback follows);
/// with `additional` it starts another terminal beside the running ones
/// (feature `terminal.multi`; at most 4 per conversation, 8 per server).
Map<String, dynamic> terminalOpen({bool? additional, required int cols, required String conversationId, required bool fresh, required int rows, String? terminalId}) => <String, dynamic>{
  'method': Methods.terminalOpen,
  'params': <String, dynamic>{
    if (additional != null) 'additional': additional,
    'cols': cols,
    'conversation_id': conversationId,
    'fresh': fresh,
    'rows': rows,
    if (terminalId != null) 'terminal_id': terminalId,
  },
};

Map<String, dynamic> terminalResize({required int cols, required int rows, required String terminalId}) => <String, dynamic>{
  'method': Methods.terminalResize,
  'params': <String, dynamic>{
    'cols': cols,
    'rows': rows,
    'terminal_id': terminalId,
  },
};

/// One thread of a channel (feature `chat_threads`): `message_id` is the
/// message that starts it (a message of a channel or a ticket's discussion
/// that is not itself a reply), answered with the message and its replies,
/// oldest first (`ThreadSnapshot`). The board member who reads it has read
/// the thread (its `thread_unread` goes to none); reading the channel never
/// does. A message without replies is a thread of none.
Map<String, dynamic> threadRead({required String messageId}) => <String, dynamic>{
  'method': Methods.threadRead,
  'params': <String, dynamic>{
    'message_id': messageId,
  },
};

/// Who waits to pick up a thread and what is scheduled in it (U3, feature
/// `scheduled_messages`): a ticket (`ticket_id`) or a channel or chat (`conversation_id`).
Map<String, dynamic> threadWaiting({String? conversationId, String? ticketId}) => <String, dynamic>{
  'method': Methods.threadWaiting,
  'params': <String, dynamic>{
    if (conversationId != null) 'conversation_id': conversationId,
    if (ticketId != null) 'ticket_id': ticketId,
  },
};

/// Who follows a ticket (feature `ticket_follow`). Not for apps.
Map<String, dynamic> ticketFollowers({required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketFollowers,
  'params': <String, dynamic>{
    'ticket_id': ticketId,
  },
};

/// One ticket with its main task and every task linked to it.
Map<String, dynamic> ticketGet({required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketGet,
  'params': <String, dynamic>{
    'ticket_id': ticketId,
  },
};

/// Tickets in board order (not archived); every given filter applies. `query`
/// searches title, description and `#number`.
Map<String, dynamic> ticketList({ColumnKind? column, String? projectId, String? query}) => <String, dynamic>{
  'method': Methods.ticketList,
  'params': <String, dynamic>{
    if (column != null) 'column': column.toJson(),
    if (projectId != null) 'project_id': projectId,
    if (query != null) 'query': query,
  },
};

/// The tickets of the board member of the session (feature `my_tickets`),
/// never of one named in a param: responsible, created or followed by them,
/// and open ones on their login or without an active responsible. Done and
/// archived ones only with `include_done`. By priority, then the latest
/// update; at most `MY_TICKETS_MAX`. Not for apps.
Map<String, dynamic> ticketMyTickets({bool? includeDone, String? projectId}) => <String, dynamic>{
  'method': Methods.ticketMyTickets,
  'params': <String, dynamic>{
    if (includeDone != null) 'include_done': includeDone,
    if (projectId != null) 'project_id': projectId,
  },
};

/// Human-readable timeline cards, newest first (keyset on `seq`).
Map<String, dynamic> timelineList(TimelineQuery params) => <String, dynamic>{
  'method': Methods.timelineList,
  'params': params.toJson(),
};

/// Whether runs keep a trace (feature `run_trace`).
Map<String, dynamic> traceGetSetting() => <String, dynamic>{
  'method': Methods.traceGetSetting,
};

/// Update status; `check` asks GitHub for the latest release first.
Map<String, dynamic> updateGet({required bool check}) => <String, dynamic>{
  'method': Methods.updateGet,
  'params': <String, dynamic>{
    'check': check,
  },
};

/// A file the user uploaded with a message, task, recurring task or idea (see
/// `MessageFileView::id`), in parts of at most `UPLOAD_CHUNK_BYTES` from
/// `offset` (none: 0). With `max_side` an image comes back whole as a JPEG
/// that fits that many pixels (clamped to `UPLOAD_THUMB_MAX_SIDE`); other files are refused.
Map<String, dynamic> uploadGet({required String id, int? maxSide, int? offset}) => <String, dynamic>{
  'method': Methods.uploadGet,
  'params': <String, dynamic>{
    'id': id,
    if (maxSide != null) 'max_side': maxSide,
    if (offset != null) 'offset': offset,
  },
};

Map<String, dynamic> usageGet() => <String, dynamic>{
  'method': Methods.usageGet,
};

/// Usage per hour of one server-local day (any moment in it).
Map<String, dynamic> usageGetHours({required int dayStartMs}) => <String, dynamic>{
  'method': Methods.usageGetHours,
  'params': <String, dynamic>{
    'day_start_ms': dayStartMs,
  },
};

/// What a conversation or a task used, with the tasks given away from it apart
/// (feature `run_usage`). Give one of the two; a task wins.
Map<String, dynamic> usageGetTotals({String? conversationId, String? taskId, String? ticketId}) => <String, dynamic>{
  'method': Methods.usageGetTotals,
  'params': <String, dynamic>{
    if (conversationId != null) 'conversation_id': conversationId,
    if (taskId != null) 'task_id': taskId,
    if (ticketId != null) 'ticket_id': ticketId,
  },
};

/// The board members, oldest first (never a password's hash), disabled ones
/// too (`UserView::disabled_at_ms`). Works without a session (the sign-in
/// screen may offer the names). Apps may read it too.
Map<String, dynamic> userList() => <String, dynamic>{
  'method': Methods.userList,
};

/// Sign in (protocol 5): a right username and password give this connection a
/// session and answer `Login` with its token, shown once (keep it for the
/// server and give it in every hello). A wrong password, an unknown username
/// and a disabled board member all answer `wrong_password`; after 5 failed tries
/// for one username the server waits longer each time, up to 15 minutes
/// (`too_many_attempts` with `retry_after_ms`). A session method: it changes
/// only the connection and the sessions, so it has no receipt and no event.
/// Not for apps.
Map<String, dynamic> userLogin(LoginParams params) => <String, dynamic>{
  'method': Methods.userLogin,
  'params': params.toJson(),
};

/// End this connection's session (the device must sign in again); answers
/// `WhoAmI` without a board member. A session method.
Map<String, dynamic> userLogout() => <String, dynamic>{
  'method': Methods.userLogout,
};

/// Whether the server has board members, and this connection's session's one.
/// Works without a session.
Map<String, dynamic> userWhoami() => <String, dynamic>{
  'method': Methods.userWhoami,
};

/// Whether tasks may be tried in several ways first (feature `task_variants`).
Map<String, dynamic> variantsGetSetting() => <String, dynamic>{
  'method': Methods.variantsGetSetting,
};

/// The audit log, newest first (`VaultAuditPage`): the company's (nothing
/// given), a project's, or every scope's (`all`); one item's with `item_id`.
/// `before`: rows older than that row id; `limit` at most 200 (default 50).
/// Never a value. Not for apps.
Map<String, dynamic> vaultAudit({bool? all, int? before, String? itemId, int? limit, String? projectId}) => <String, dynamic>{
  'method': Methods.vaultAudit,
  'params': <String, dynamic>{
    if (all != null) 'all': all,
    if (before != null) 'before': before,
    if (itemId != null) 'item_id': itemId,
    if (limit != null) 'limit': limit,
    if (projectId != null) 'project_id': projectId,
  },
};

/// The server's vault key (`VaultKeyExport`, never printed), for a restore on
/// another machine (B8): a board member with a session that needs no new
/// password, who gives their password again (`wrong_password`; it counts
/// for the sign-in limits, `too_many_attempts`). Writes an audit row
/// `key_export` and tells every board member. Not found before the first
/// item. Not for apps.
Map<String, dynamic> vaultKeyExport(KeyExportParams params) => <String, dynamic>{
  'method': Methods.vaultKeyExport,
  'params': params.toJson(),
};

/// The vault of a scope (feature `vault`): none, the company's items; a
/// project, its items plus the company's apart (`VaultList::company_items`),
/// with `overrides_company` on a project item whose env name the company
/// has too, and the item count of every scope. Never a secret's value.
/// Archived items are left out. Board members only; not for apps (B5).
Map<String, dynamic> vaultList({String? projectId}) => <String, dynamic>{
  'method': Methods.vaultList,
  'params': <String, dynamic>{
    if (projectId != null) 'project_id': projectId,
  },
};

/// One item's value (`VaultValue`, never printed), for a board member with a
/// session. Writes an audit row `reveal` (show) or `copy` with the session's
/// device. `vault_undecryptable` when the key does not fit. Not for apps.
Map<String, dynamic> vaultReveal({required String itemId, required RevealPurpose purpose}) => <String, dynamic>{
  'method': Methods.vaultReveal,
  'params': <String, dynamic>{
    'item_id': itemId,
    'purpose': purpose.toJson(),
  },
};

// Writes only the host can do.

Map<String, dynamic> agentFunctionCreate(CreateAgentFunction params) => <String, dynamic>{
  'method': Methods.agentFunctionCreate,
  'params': params.toJson(),
};

Map<String, dynamic> agentFunctionDelete({required String functionId}) => <String, dynamic>{
  'method': Methods.agentFunctionDelete,
  'params': <String, dynamic>{
    'function_id': functionId,
  },
};

/// The kill switch: `enabled: false` makes every function refuse calls.
Map<String, dynamic> agentFunctionSetAllEnabled({required bool enabled}) => <String, dynamic>{
  'method': Methods.agentFunctionSetAllEnabled,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
};

Map<String, dynamic> agentFunctionSetEnabled({required bool enabled, required String functionId}) => <String, dynamic>{
  'method': Methods.agentFunctionSetEnabled,
  'params': <String, dynamic>{
    'enabled': enabled,
    'function_id': functionId,
  },
};

Map<String, dynamic> agentFunctionUpdate(UpdateAgentFunction params) => <String, dynamic>{
  'method': Methods.agentFunctionUpdate,
  'params': params.toJson(),
};

/// The secret is in the answer once; the receipt keeps none.
Map<String, dynamic> apiKeyCreate({required List<String> functionIds, required String name}) => <String, dynamic>{
  'method': Methods.apiKeyCreate,
  'params': <String, dynamic>{
    'function_ids': functionIds,
    'name': name,
  },
};

Map<String, dynamic> apiKeyRevoke({required String keyId}) => <String, dynamic>{
  'method': Methods.apiKeyRevoke,
  'params': <String, dynamic>{
    'key_id': keyId,
  },
};

Map<String, dynamic> apiKeySetFunctions({required List<String> functionIds, required String keyId}) => <String, dynamic>{
  'method': Methods.apiKeySetFunctions,
  'params': <String, dynamic>{
    'function_ids': functionIds,
    'key_id': keyId,
  },
};

/// A key for an outside app (feature `app_keys`); answers `AppKeyCreated`
/// with the secret, once: the receipt keeps none.
Map<String, dynamic> appKeyCreate(CreateAppKey params) => <String, dynamic>{
  'method': Methods.appKeyCreate,
  'params': params.toJson(),
};

/// Revoked at once, also for the app's open sessions; answers `AppKey`.
Map<String, dynamic> appKeyRevoke({required String keyId}) => <String, dynamic>{
  'method': Methods.appKeyRevoke,
  'params': <String, dynamic>{
    'key_id': keyId,
  },
};

/// Turn Claude Code artifacts on or off for every run that starts afterwards.
Map<String, dynamic> artifactSetEnabled({required bool enabled}) => <String, dynamic>{
  'method': Methods.artifactSetEnabled,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
};

/// Auto-allow of the safe read-only tools on or off, from the next tool call. On by default.
Map<String, dynamic> autoAllowSetEnabled({required bool enabled}) => <String, dynamic>{
  'method': Methods.autoAllowSetEnabled,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
};

Map<String, dynamic> backupCreate() => <String, dynamic>{
  'method': Methods.backupCreate,
};

/// Restore at the next start (the service stops now); work stays paused until resumed.
Map<String, dynamic> backupRestore({required String id}) => <String, dynamic>{
  'method': Methods.backupRestore,
  'params': <String, dynamic>{
    'id': id,
  },
};

Map<String, dynamic> backupResume() => <String, dynamic>{
  'method': Methods.backupResume,
};

Map<String, dynamic> browserSet({required BrowserChoice choice}) => <String, dynamic>{
  'method': Methods.browserSet,
  'params': <String, dynamic>{
    'choice': choice.toJson(),
  },
};

/// Claude in Chrome or Playwright, for every run that starts afterwards.
Map<String, dynamic> browserSetBackend({required BrowserBackend backend}) => <String, dynamic>{
  'method': Methods.browserSetBackend,
  'params': <String, dynamic>{
    'backend': backend.toJson(),
  },
};

/// Stop a running link of the session's own login. Answers `ClaudeLogin`.
Map<String, dynamic> claudeLoginCancel({required String linkId}) => <String, dynamic>{
  'method': Methods.claudeLoginCancel,
  'params': <String, dynamic>{
    'link_id': linkId,
  },
};

/// Connect the session's own Claude login: the server starts `claude auth
/// login` for `<data>/logins/<user_id>/` and answers `ClaudeLoginLink`. What
/// Claude asks (the sign-in link, the code) comes to this connection only as
/// `ServerMessage::ClaudeLoginPrompt`; the code goes back with
/// `claude_login.input`. One link per board member at a time, 10 minutes.
Map<String, dynamic> claudeLoginLink() => <String, dynamic>{
  'method': Methods.claudeLoginLink,
};

/// Disconnect the session's own Claude login (`claude auth logout`, its folder
/// removed). Answers `ClaudeLogin`.
Map<String, dynamic> claudeLoginUnlink() => <String, dynamic>{
  'method': Methods.claudeLoginUnlink,
};

/// A new folder `name` in the folder `parent`.
Map<String, dynamic> fsCreateDir({required String name, required String parent}) => <String, dynamic>{
  'method': Methods.fsCreateDir,
  'params': <String, dynamic>{
    'name': name,
    'parent': parent,
  },
};

Map<String, dynamic> loadSetLimits(LoadLimitsUpdate params) => <String, dynamic>{
  'method': Methods.loadSetLimits,
  'params': params.toJson(),
};

/// Management chooses its own task mode (feature `management_chooses_mode`), for every run
/// that starts afterwards. On by default.
Map<String, dynamic> managementModeSetEnabled({required bool enabled}) => <String, dynamic>{
  'method': Methods.managementModeSetEnabled,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
};

Map<String, dynamic> mcpAdd(McpAddParams params) => <String, dynamic>{
  'method': Methods.mcpAdd,
  'params': params.toJson(),
};

/// Finishes a login with the address the browser ended on (`…/callback?code=…&state=…`),
/// for a browser that is not on the server's computer.
Map<String, dynamic> mcpLoginComplete({required String loginId, required String redirectUrl}) => <String, dynamic>{
  'method': Methods.mcpLoginComplete,
  'params': <String, dynamic>{
    'login_id': loginId,
    'redirect_url': redirectUrl,
  },
};

/// Starts an OAuth login for an http/sse server; answers `McpLogin` with the URL to open.
Map<String, dynamic> mcpLoginStart({required String name}) => <String, dynamic>{
  'method': Methods.mcpLoginStart,
  'params': <String, dynamic>{
    'name': name,
  },
};

/// Forgets the server's tokens (keeps its client registration).
Map<String, dynamic> mcpLogout({required String name}) => <String, dynamic>{
  'method': Methods.mcpLogout,
  'params': <String, dynamic>{
    'name': name,
  },
};

Map<String, dynamic> mcpRemove({required String name}) => <String, dynamic>{
  'method': Methods.mcpRemove,
  'params': <String, dynamic>{
    'name': name,
  },
};

Map<String, dynamic> mcpSet({required List<String> enabled}) => <String, dynamic>{
  'method': Methods.mcpSet,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
};

/// Take a tool off the always-allow list: its next call asks again.
Map<String, dynamic> mcpAlwaysAllowRemove({required String tool}) => <String, dynamic>{
  'method': Methods.mcpAlwaysAllowRemove,
  'params': <String, dynamic>{
    'tool': tool,
  },
};

/// A new note in a project; answers `Note`.
Map<String, dynamic> noteCreate({String body = '', required String projectId}) => <String, dynamic>{
  'method': Methods.noteCreate,
  'params': <String, dynamic>{
    'body': body,
    'project_id': projectId,
  },
};

Map<String, dynamic> noteDelete({required String noteId}) => <String, dynamic>{
  'method': Methods.noteDelete,
  'params': <String, dynamic>{
    'note_id': noteId,
  },
};

Map<String, dynamic> noteSetPinned({required String noteId, required bool pinned}) => <String, dynamic>{
  'method': Methods.noteSetPinned,
  'params': <String, dynamic>{
    'note_id': noteId,
    'pinned': pinned,
  },
};

/// Replace a note's body. Refused with `version_conflict` when the note is no
/// longer at `base_revision` (saved elsewhere): load it again with `note.get`.
Map<String, dynamic> noteUpdate({required int baseRevision, required String body, required String noteId}) => <String, dynamic>{
  'method': Methods.noteUpdate,
  'params': <String, dynamic>{
    'base_revision': baseRevision,
    'body': body,
    'note_id': noteId,
  },
};

Map<String, dynamic> notificationDismiss({required String id}) => <String, dynamic>{
  'method': Methods.notificationDismiss,
  'params': <String, dynamic>{
    'id': id,
  },
};

Map<String, dynamic> notificationDismissAll() => <String, dynamic>{
  'method': Methods.notificationDismissAll,
};

Map<String, dynamic> notificationMarkAllRead() => <String, dynamic>{
  'method': Methods.notificationMarkAllRead,
};

/// One row read, for this board member only (feature `personal_inbox`); answers
/// `Count`, 0 when there was nothing to read, so it can be repeated.
Map<String, dynamic> notificationMarkRead({required String id}) => <String, dynamic>{
  'method': Methods.notificationMarkRead,
  'params': <String, dynamic>{
    'id': id,
  },
};

/// Install plugins (`name@marketplace`) and enable them for the given employees and projects.
Map<String, dynamic> pluginInstall({required List<String> employees, required List<String> ids, List<String> projects = const []}) => <String, dynamic>{
  'method': Methods.pluginInstall,
  'params': <String, dynamic>{
    'employees': employees,
    'ids': ids,
    'projects': projects,
  },
};

/// Adds the marketplace behind a link (GitHub URL, `owner/repo` or git URL).
Map<String, dynamic> pluginPreview({required String source}) => <String, dynamic>{
  'method': Methods.pluginPreview,
  'params': <String, dynamic>{
    'source': source,
  },
};

Map<String, dynamic> pluginRemove({required String id}) => <String, dynamic>{
  'method': Methods.pluginRemove,
  'params': <String, dynamic>{
    'id': id,
  },
};

/// Replace the plugins an employee has enabled; applies from its next run.
Map<String, dynamic> pluginSetForEmployee({required String employeeId, required List<String> enabled}) => <String, dynamic>{
  'method': Methods.pluginSetForEmployee,
  'params': <String, dynamic>{
    'employee_id': employeeId,
    'enabled': enabled,
  },
};

/// Replace the plugins a project has enabled (for its project assistant); applies from its next run.
Map<String, dynamic> pluginSetForProject({required List<String> enabled, required String projectId}) => <String, dynamic>{
  'method': Methods.pluginSetForProject,
  'params': <String, dynamic>{
    'enabled': enabled,
    'project_id': projectId,
  },
};

Map<String, dynamic> pluginUpdate({required String id}) => <String, dynamic>{
  'method': Methods.pluginUpdate,
  'params': <String, dynamic>{
    'id': id,
  },
};

Map<String, dynamic> powerCancel() => <String, dynamic>{
  'method': Methods.powerCancel,
};

Map<String, dynamic> powerShutdown({required ShutdownWhen when}) => <String, dynamic>{
  'method': Methods.powerShutdown,
  'params': <String, dynamic>{
    'when': when.toJson(),
  },
};

/// Write a readable Markdown + JSON export of a project into its folder.
Map<String, dynamic> projectExport({required String projectId}) => <String, dynamic>{
  'method': Methods.projectExport,
  'params': <String, dynamic>{
    'project_id': projectId,
  },
};

Map<String, dynamic> pushConfigure(PushConfig params) => <String, dynamic>{
  'method': Methods.pushConfigure,
  'params': params.toJson(),
};

Map<String, dynamic> pushTest({required String clientId}) => <String, dynamic>{
  'method': Methods.pushTest,
  'params': <String, dynamic>{
    'client_id': clientId,
  },
};

/// Allow or refuse a device that entered the code. A computer that asks for
/// full access is only allowed with `owner` (the user confirmed it).
Map<String, dynamic> remoteDecidePairing({required bool approve, required bool owner, required String pairingId}) => <String, dynamic>{
  'method': Methods.remoteDecidePairing,
  'params': <String, dynamic>{
    'approve': approve,
    'owner': owner,
    'pairing_id': pairingId,
  },
};

Map<String, dynamic> remoteDisable() => <String, dynamic>{
  'method': Methods.remoteDisable,
};

Map<String, dynamic> remoteEnable() => <String, dynamic>{
  'method': Methods.remoteEnable,
};

Map<String, dynamic> remoteRevoke({required String clientId}) => <String, dynamic>{
  'method': Methods.remoteRevoke,
  'params': <String, dynamic>{
    'client_id': clientId,
  },
};

/// A new one-time code (5 minutes) plus the QR payload; the receipt keeps neither.
Map<String, dynamic> remoteStartPairing() => <String, dynamic>{
  'method': Methods.remoteStartPairing,
};

Map<String, dynamic> secretSet({required String connectionId, required String secret}) => <String, dynamic>{
  'method': Methods.secretSet,
  'params': <String, dynamic>{
    'connection_id': connectionId,
    'secret': secret,
  },
};

Map<String, dynamic> serverRestart() => <String, dynamic>{
  'method': Methods.serverRestart,
};

/// End one session (any board member's: one role). Its device's open
/// connection closes at once (`ServerMessage::SignedOut`); the timeline says
/// who revoked it. Answers `Done`. Not for apps.
Map<String, dynamic> sessionRevoke({required String sessionId}) => <String, dynamic>{
  'method': Methods.sessionRevoke,
  'params': <String, dynamic>{
    'session_id': sessionId,
  },
};

/// Trace mode on or off for every run that starts afterwards (feature `run_trace`).
/// Off also forgets the traces kept so far.
Map<String, dynamic> traceSetEnabled({required bool enabled}) => <String, dynamic>{
  'method': Methods.traceSetEnabled,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
};

Map<String, dynamic> updateApply() => <String, dynamic>{
  'method': Methods.updateApply,
};

Map<String, dynamic> updateApplyApp() => <String, dynamic>{
  'method': Methods.updateApplyApp,
};

Map<String, dynamic> updatePrepare() => <String, dynamic>{
  'method': Methods.updatePrepare,
};

Map<String, dynamic> updatePrepareApp() => <String, dynamic>{
  'method': Methods.updatePrepareApp,
};

/// A new board member (OV-12: `invalid_username`, `weak_password` (also a
/// password of only spaces), `username_taken`; `too_many_attempts` after five
/// a minute on one connection or twenty an hour on the server); answers
/// `CommandResult::UserCreated` as a command receipt. Without a session only
/// the first board member (the onboarding): that connection then gets a
/// session, whose token `user.login` would give (see `HelloResult::user`;
/// the client signs in once more to get the token). After that it needs a
/// session (any board member, one role). The server hashes the password
/// before anything is recorded: the receipt, its request hash and the event
/// `user.created` hold the name only. Not for apps.
Map<String, dynamic> userCreate(UserCreateParams params) => <String, dynamic>{
  'method': Methods.userCreate,
  'params': params.toJson(),
};

/// Disable a board member: their sessions end at once and they cannot sign
/// in; their Claude login is disconnected; tickets keep their creator. The
/// last active board member cannot be disabled (`last_board_member`).
/// Answers `User`. Not for apps.
Map<String, dynamic> userDisable({required String userId}) => <String, dynamic>{
  'method': Methods.userDisable,
  'params': <String, dynamic>{
    'user_id': userId,
  },
};

/// Change a password. Own (no `user_id`, or the session's): with the old
/// password; the other sessions end, this one stays. Another board member's
/// (B10): a temporary password, changed at their next sign-in
/// (`must_change_password`); all their sessions end. Like `user.create`, the
/// passwords are in no receipt, request hash, event or log. Answers `Done`.
Map<String, dynamic> userSetPassword(SetPasswordParams params) => <String, dynamic>{
  'method': Methods.userSetPassword,
  'params': params.toJson(),
};

/// Allow tasks to be tried in several ways first (feature `task_variants`; off by default).
Map<String, dynamic> variantsSetEnabled({required bool enabled}) => <String, dynamic>{
  'method': Methods.variantsSetEnabled,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
};

/// Archive ("Delete") an item: runs and employees no longer get it; its audit
/// history stays. Answers `VaultItem`. Not for apps.
Map<String, dynamic> vaultArchive({required String itemId}) => <String, dynamic>{
  'method': Methods.vaultArchive,
  'params': <String, dynamic>{
    'item_id': itemId,
  },
};

/// A new vault item (feature `vault`), in the company (no `project_id`) or a
/// project. Answers `VaultItem` (`value` only for text/url). The server
/// encrypts the value before anything is recorded: the receipt, its request
/// hash and the event `vault.item_created` hold no value (MU4-05 AC2).
/// `validation` for a name or env name used in the scope, an env name that
/// is not UPPER_SNAKE or that the runtime reserves (`PATH`, `ANTHROPIC_*`,
/// …), a url without `https://`/`http://`, a secret under 8 characters.
/// Board members with a session only (employees and agent tools never); not for apps.
Map<String, dynamic> vaultCreate(VaultCreateParams params) => <String, dynamic>{
  'method': Methods.vaultCreate,
  'params': params.toJson(),
};

/// Change an item; what is left out stays. `value`: a new value (a secret's
/// is never sent back to the editor, so "leave empty to keep"). `kind` only
/// between text and url: to or from secret is `vault_kind_locked` (B7).
/// With `expected_version` in the envelope a stale edit is `version_conflict`.
/// Answers `VaultItem`. As `vault.create`: no value in receipt, hash or event.
Map<String, dynamic> vaultUpdate(VaultUpdateParams params) => <String, dynamic>{
  'method': Methods.vaultUpdate,
  'params': params.toJson(),
};

// Domain commands.

Map<String, dynamic> approvalAnswer(AnswerQuestion params) => <String, dynamic>{
  'method': Methods.approvalAnswer,
  'params': params.toJson(),
};

Map<String, dynamic> approvalDecide(DecideApproval params) => <String, dynamic>{
  'method': Methods.approvalDecide,
  'params': params.toJson(),
};

Map<String, dynamic> approvalDecidePlan(DecidePlan params) => <String, dynamic>{
  'method': Methods.approvalDecidePlan,
  'params': params.toJson(),
};

Map<String, dynamic> attachmentAdd(AttachFile params) => <String, dynamic>{
  'method': Methods.attachmentAdd,
  'params': params.toJson(),
};

Map<String, dynamic> attachmentRemove(RemoveAttachment params) => <String, dynamic>{
  'method': Methods.attachmentRemove,
  'params': params.toJson(),
};

/// Rename the columns and set their order (B3): all of them, left to right.
/// `expected_version` is the board's (`board_version`: the columns change
/// together); the answer gives the new one.
Map<String, dynamic> boardColumnsUpdate({required List<BoardColumnEdit> columns}) => <String, dynamic>{
  'method': Methods.boardColumnsUpdate,
  'params': <String, dynamic>{
    'columns': columns.map((x0) => x0.toJson()).toList(),
  },
};

Map<String, dynamic> chatRetry(RetryReply params) => <String, dynamic>{
  'method': Methods.chatRetry,
  'params': params.toJson(),
};

Map<String, dynamic> chatSend(SendChatMessage params) => <String, dynamic>{
  'method': Methods.chatSend,
  'params': params.toJson(),
};

/// The company's background jobs (memory consolidation, B6) run on the Claude
/// login of the board member who asks (`enabled`), or on none (`false`, only
/// when it is theirs): without one they do not run. Only the own login can be
/// chosen. Answers `CompanyUpdated`.
Map<String, dynamic> companySetBackgroundLogin({required bool enabled}) => <String, dynamic>{
  'method': Methods.companySetBackgroundLogin,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
};

Map<String, dynamic> companySetFollowOrgChart(SetFollowOrgChart params) => <String, dynamic>{
  'method': Methods.companySetFollowOrgChart,
  'params': params.toJson(),
};

Map<String, dynamic> companyUpdate(UpdateCompany params) => <String, dynamic>{
  'method': Methods.companyUpdate,
  'params': params.toJson(),
};

Map<String, dynamic> connectionAdd(AddConnection params) => <String, dynamic>{
  'method': Methods.connectionAdd,
  'params': params.toJson(),
};

Map<String, dynamic> connectionRemove(RemoveConnection params) => <String, dynamic>{
  'method': Methods.connectionRemove,
  'params': params.toJson(),
};

Map<String, dynamic> connectionSetDefaults(SetRunDefaults params) => <String, dynamic>{
  'method': Methods.connectionSetDefaults,
  'params': params.toJson(),
};

Map<String, dynamic> connectionVerify(VerifyConnection params) => <String, dynamic>{
  'method': Methods.connectionVerify,
  'params': params.toJson(),
};

Map<String, dynamic> conversationDelete(DeleteConversation params) => <String, dynamic>{
  'method': Methods.conversationDelete,
  'params': params.toJson(),
};

Map<String, dynamic> conversationDeleteQueued(DeleteQueuedMessage params) => <String, dynamic>{
  'method': Methods.conversationDeleteQueued,
  'params': params.toJson(),
};

Map<String, dynamic> conversationSendQueuedNow(SendQueuedNow params) => <String, dynamic>{
  'method': Methods.conversationSendQueuedNow,
  'params': params.toJson(),
};

Map<String, dynamic> conversationSetAgentMode(SetAgentMode params) => <String, dynamic>{
  'method': Methods.conversationSetAgentMode,
  'params': params.toJson(),
};

Map<String, dynamic> conversationSetBrowserAllowed(SetBrowserAllowed params) => <String, dynamic>{
  'method': Methods.conversationSetBrowserAllowed,
  'params': params.toJson(),
};

Map<String, dynamic> conversationSetRun(SetConversationRun params) => <String, dynamic>{
  'method': Methods.conversationSetRun,
  'params': params.toJson(),
};

Map<String, dynamic> conversationSetScreenAllowed(SetScreenAllowed params) => <String, dynamic>{
  'method': Methods.conversationSetScreenAllowed,
  'params': params.toJson(),
};

Map<String, dynamic> employeeCreate(CreateEmployee params) => <String, dynamic>{
  'method': Methods.employeeCreate,
  'params': params.toJson(),
};

Map<String, dynamic> employeeOpenChat(OpenEmployeeChat params) => <String, dynamic>{
  'method': Methods.employeeOpenChat,
  'params': params.toJson(),
};

Map<String, dynamic> employeeRestoreInstructions(RestoreEmployeeInstructions params) => <String, dynamic>{
  'method': Methods.employeeRestoreInstructions,
  'params': params.toJson(),
};

Map<String, dynamic> employeeSendMessage(SendEmployeeMessage params) => <String, dynamic>{
  'method': Methods.employeeSendMessage,
  'params': params.toJson(),
};

Map<String, dynamic> employeeSetFolder(SetEmployeeFolder params) => <String, dynamic>{
  'method': Methods.employeeSetFolder,
  'params': params.toJson(),
};

Map<String, dynamic> employeeSetReportsTo(SetEmployeeReportsTo params) => <String, dynamic>{
  'method': Methods.employeeSetReportsTo,
  'params': params.toJson(),
};

Map<String, dynamic> employeeSetStatus(SetEmployeeStatus params) => <String, dynamic>{
  'method': Methods.employeeSetStatus,
  'params': params.toJson(),
};

Map<String, dynamic> employeeUpdate(UpdateEmployee params) => <String, dynamic>{
  'method': Methods.employeeUpdate,
  'params': params.toJson(),
};

Map<String, dynamic> groupChatSend(SendGroupMessage params) => <String, dynamic>{
  'method': Methods.groupChatSend,
  'params': params.toJson(),
};

/// `guideline.add_starter_set`: the chosen templates as guidelines without links (B7).
/// A key whose title is in the library already is skipped.
Map<String, dynamic> guidelineAddStarterSet({required List<String> keys}) => <String, dynamic>{
  'method': Methods.guidelineAddStarterSet,
  'params': <String, dynamic>{
    'keys': keys,
  },
};

/// `guideline.archive`: no run gets it any more; links and versions stay.
Map<String, dynamic> guidelineArchive({required String guidelineId}) => <String, dynamic>{
  'method': Methods.guidelineArchive,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
  },
};

/// `guideline.create`: a new guideline (version 1, source `created`), linked at once to `links`.
Map<String, dynamic> guidelineCreate({List<GuidelineTarget> links = const [], required String text, required String title}) => <String, dynamic>{
  'method': Methods.guidelineCreate,
  'params': <String, dynamic>{
    'links': links.map((x0) => x0.toJson()).toList(),
    'text': text,
    'title': title,
  },
};

/// `guideline.decide_proposal` (B5): approve (with `title`/`text` edited by the
/// board member, and for a new guideline the `links`; none: the suggested
/// ones) or reject. `note` goes back to the proposer. `version_conflict` when
/// the guideline moved on since the proposal's base version.
Map<String, dynamic> guidelineDecideProposal({required bool approve, List<GuidelineTarget>? links, String? note, required String proposalId, String? text, String? title}) => <String, dynamic>{
  'method': Methods.guidelineDecideProposal,
  'params': <String, dynamic>{
    'approve': approve,
    if (links != null) 'links': links.map((x0) => x0.toJson()).toList(),
    if (note != null) 'note': note,
    'proposal_id': proposalId,
    if (text != null) 'text': text,
    if (title != null) 'title': title,
  },
};

/// `guideline.link`: an employee or a project gets it from its next run.
Map<String, dynamic> guidelineLink({required String guidelineId, required GuidelineTarget target}) => <String, dynamic>{
  'method': Methods.guidelineLink,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
    'target': target.toJson(),
  },
};

/// `guideline.restore`: back from the archive, with its links (B4 and the title are checked again).
Map<String, dynamic> guidelineRestore({required String guidelineId}) => <String, dynamic>{
  'method': Methods.guidelineRestore,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
  },
};

/// `guideline.restore_version`: a copy of `version` as the newest (source `restored`).
Map<String, dynamic> guidelineRestoreVersion({required String guidelineId, required int version}) => <String, dynamic>{
  'method': Methods.guidelineRestoreVersion,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
    'version': version,
  },
};

/// `guideline.unlink`.
Map<String, dynamic> guidelineUnlink({required String guidelineId, required GuidelineTarget target}) => <String, dynamic>{
  'method': Methods.guidelineUnlink,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
    'target': target.toJson(),
  },
};

/// `guideline.update`: a new version with what changed (source `edited`); what
/// is left out stays. `expected_version` in the envelope is the version the
/// editor started from (`version_conflict` otherwise).
Map<String, dynamic> guidelineUpdate({required String guidelineId, String? text, String? title}) => <String, dynamic>{
  'method': Methods.guidelineUpdate,
  'params': <String, dynamic>{
    'guideline_id': guidelineId,
    if (text != null) 'text': text,
    if (title != null) 'title': title,
  },
};

Map<String, dynamic> heartbeatSet(SetHeartbeat params) => <String, dynamic>{
  'method': Methods.heartbeatSet,
  'params': params.toJson(),
};

Map<String, dynamic> ideaAdd(AddIdea params) => <String, dynamic>{
  'method': Methods.ideaAdd,
  'params': params.toJson(),
};

Map<String, dynamic> ideaDelete(DeleteIdea params) => <String, dynamic>{
  'method': Methods.ideaDelete,
  'params': params.toJson(),
};

Map<String, dynamic> ideaSetStatus(SetIdeaStatus params) => <String, dynamic>{
  'method': Methods.ideaSetStatus,
  'params': params.toJson(),
};

Map<String, dynamic> ideaStart(StartIdea params) => <String, dynamic>{
  'method': Methods.ideaStart,
  'params': params.toJson(),
};

Map<String, dynamic> ideaUpdate(UpdateIdea params) => <String, dynamic>{
  'method': Methods.ideaUpdate,
  'params': params.toJson(),
};

Map<String, dynamic> memoryConsolidate(ConsolidateMemory params) => <String, dynamic>{
  'method': Methods.memoryConsolidate,
  'params': params.toJson(),
};

Map<String, dynamic> memoryDecide(DecideMemory params) => <String, dynamic>{
  'method': Methods.memoryDecide,
  'params': params.toJson(),
};

Map<String, dynamic> memoryForget(ForgetMemory params) => <String, dynamic>{
  'method': Methods.memoryForget,
  'params': params.toJson(),
};

Map<String, dynamic> memoryPin(PinMemory params) => <String, dynamic>{
  'method': Methods.memoryPin,
  'params': params.toJson(),
};

Map<String, dynamic> memoryRemember(Remember params) => <String, dynamic>{
  'method': Methods.memoryRemember,
  'params': params.toJson(),
};

Map<String, dynamic> memoryRevise(ReviseMemory params) => <String, dynamic>{
  'method': Methods.memoryRevise,
  'params': params.toJson(),
};

Map<String, dynamic> messageDelete(DeleteMessage params) => <String, dynamic>{
  'method': Methods.messageDelete,
  'params': params.toJson(),
};

Map<String, dynamic> messageEdit(EditMessage params) => <String, dynamic>{
  'method': Methods.messageEdit,
  'params': params.toJson(),
};

Map<String, dynamic> messageReact(ReactToMessage params) => <String, dynamic>{
  'method': Methods.messageReact,
  'params': params.toJson(),
};

Map<String, dynamic> projectCreate(CreateProject params) => <String, dynamic>{
  'method': Methods.projectCreate,
  'params': params.toJson(),
};

Map<String, dynamic> projectRename(RenameProject params) => <String, dynamic>{
  'method': Methods.projectRename,
  'params': params.toJson(),
};

Map<String, dynamic> projectSetDescription(SetProjectDescription params) => <String, dynamic>{
  'method': Methods.projectSetDescription,
  'params': params.toJson(),
};

Map<String, dynamic> projectSetFolder(SetProjectFolder params) => <String, dynamic>{
  'method': Methods.projectSetFolder,
  'params': params.toJson(),
};

Map<String, dynamic> projectSetStatus(SetProjectStatus params) => <String, dynamic>{
  'method': Methods.projectSetStatus,
  'params': params.toJson(),
};

Map<String, dynamic> projectUpdate(UpdateProject params) => <String, dynamic>{
  'method': Methods.projectUpdate,
  'params': params.toJson(),
};

Map<String, dynamic> projectConversationSend(SendProjectMessage params) => <String, dynamic>{
  'method': Methods.projectConversationSend,
  'params': params.toJson(),
};

Map<String, dynamic> projectConversationStart(StartProjectConversation params) => <String, dynamic>{
  'method': Methods.projectConversationStart,
  'params': params.toJson(),
};

Map<String, dynamic> proposalDecide(DecideProposal params) => <String, dynamic>{
  'method': Methods.proposalDecide,
  'params': params.toJson(),
};

Map<String, dynamic> recurringTaskCreate(CreateTaskSchedule params) => <String, dynamic>{
  'method': Methods.recurringTaskCreate,
  'params': params.toJson(),
};

Map<String, dynamic> recurringTaskDelete(DeleteTaskSchedule params) => <String, dynamic>{
  'method': Methods.recurringTaskDelete,
  'params': params.toJson(),
};

Map<String, dynamic> recurringTaskRunNow(RunTaskScheduleNow params) => <String, dynamic>{
  'method': Methods.recurringTaskRunNow,
  'params': params.toJson(),
};

Map<String, dynamic> recurringTaskSetPaused(SetTaskSchedulePaused params) => <String, dynamic>{
  'method': Methods.recurringTaskSetPaused,
  'params': params.toJson(),
};

Map<String, dynamic> recurringTaskUpdate(UpdateTaskSchedule params) => <String, dynamic>{
  'method': Methods.recurringTaskUpdate,
  'params': params.toJson(),
};

Map<String, dynamic> runStop(StopRun params) => <String, dynamic>{
  'method': Methods.runStop,
  'params': params.toJson(),
};

Map<String, dynamic> scheduledMessageCancel(CancelScheduledMessage params) => <String, dynamic>{
  'method': Methods.scheduledMessageCancel,
  'params': params.toJson(),
};

/// Choose the server's name (`server.set_name`). Empty or only spaces puts the
/// computer's name back. The same name as now changes nothing (no event).
/// Answers `ServerNameSet`.
Map<String, dynamic> serverSetName({required String name}) => <String, dynamic>{
  'method': Methods.serverSetName,
  'params': <String, dynamic>{
    'name': name,
  },
};

Map<String, dynamic> shareCreate(ShareMessage params) => <String, dynamic>{
  'method': Methods.shareCreate,
  'params': params.toJson(),
};

Map<String, dynamic> shareRevoke(RevokeShare params) => <String, dynamic>{
  'method': Methods.shareRevoke,
  'params': params.toJson(),
};

Map<String, dynamic> taskAssign(AssignTask params) => <String, dynamic>{
  'method': Methods.taskAssign,
  'params': params.toJson(),
};

Map<String, dynamic> taskCancel(CancelTask params) => <String, dynamic>{
  'method': Methods.taskCancel,
  'params': params.toJson(),
};

Map<String, dynamic> taskChooseVariant(ChooseTaskVariant params) => <String, dynamic>{
  'method': Methods.taskChooseVariant,
  'params': params.toJson(),
};

Map<String, dynamic> taskCreate(CreateTask params) => <String, dynamic>{
  'method': Methods.taskCreate,
  'params': params.toJson(),
};

Map<String, dynamic> taskDiscuss(DiscussTask params) => <String, dynamic>{
  'method': Methods.taskDiscuss,
  'params': params.toJson(),
};

/// Review → Done. Only a board member; only when no linked task still works (B6).
Map<String, dynamic> ticketAccept({bool? confirmSecret, String? note, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketAccept,
  'params': <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    if (note != null) 'note': note,
    'ticket_id': ticketId,
  },
};

/// Plan review → Doing: the owner carries the plan out.
Map<String, dynamic> ticketApprovePlan({required PlanApproval decision, String? feedback, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketApprovePlan,
  'params': <String, dynamic>{
    'decision': decision.toJson(),
    if (feedback != null) 'feedback': feedback,
    'ticket_id': ticketId,
  },
};

/// Take a ticket off the board (from Done or Backlog); it stays readable.
Map<String, dynamic> ticketArchive({required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketArchive,
  'params': <String, dynamic>{
    'ticket_id': ticketId,
  },
};

/// Choose the owner of a ticket before its main task exists: a board member
/// (instead of the dispatcher) or the dispatcher itself. Gives `ticket.dispatched`.
///
/// `employee_id` none: no owner again, so the dispatcher chooses when it
/// starts. Only in Backlog (before the start); gives `ticket.updated` with
/// `changed: ["owner"]`, not `ticket.dispatched` (nobody was chosen).
Map<String, dynamic> ticketAssign({String? employeeId, String? reason, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketAssign,
  'params': <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    if (reason != null) 'reason': reason,
    'ticket_id': ticketId,
  },
};

/// Stop the work on a ticket: its main task is cancelled and it goes back to Backlog.
Map<String, dynamic> ticketCancel({required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketCancel,
  'params': <String, dynamic>{
    'ticket_id': ticketId,
  },
};

/// A new ticket in Backlog (`start`: straight to Todo, as `ticket.start` would).
Map<String, dynamic> ticketCreate({bool? confirmSecret, int? deadlineMs, String? description, List<String> files = const [], String? fromIdea, String? fromMessage, List<String> labels = const [], String? ownerEmployeeId, TicketPriority? priority, required String projectId, String? responsibleUserId, TaskRun? run, bool? start, required String title}) => <String, dynamic>{
  'method': Methods.ticketCreate,
  'params': <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    if (deadlineMs != null) 'deadline_ms': deadlineMs,
    if (description != null) 'description': description,
    'files': files,
    if (fromIdea != null) 'from_idea': fromIdea,
    if (fromMessage != null) 'from_message': fromMessage,
    'labels': labels,
    if (ownerEmployeeId != null) 'owner_employee_id': ownerEmployeeId,
    if (priority != null) 'priority': priority.toJson(),
    'project_id': projectId,
    if (responsibleUserId != null) 'responsible_user_id': responsibleUserId,
    if (run != null) 'run': run.toJson(),
    if (start != null) 'start': start,
    'title': title,
  },
};

Map<String, dynamic> ticketFollow(FollowTicket params) => <String, dynamic>{
  'method': Methods.ticketFollow,
  'params': params.toJson(),
};

/// Drag a card. Within its column: only the order. To Todo: start it. To
/// Backlog: cancel it. Done only through `ticket.accept`, and Review back to
/// Doing only through `ticket.send_back`; out of Testing only to Backlog (cancel);
/// other moves follow the work and are refused.
/// The place is relative to a neighbour, so it means the same on a board
/// filtered by project as on the whole board. A move within the same column
/// also gives `ticket.moved` (with `from` = `to`). A card put above itself in
/// its own column stays where it is: no event, the version is unchanged.
/// Unknown fields (such as the old `position`) are refused.
Map<String, dynamic> ticketMove({String? beforeTicketId, required String columnId, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketMove,
  'params': <String, dynamic>{
    if (beforeTicketId != null) 'before_ticket_id': beforeTicketId,
    'column_id': columnId,
    'ticket_id': ticketId,
  },
};

/// A board member writes in a ticket's discussion (feature `ticket_discussion`).
/// Without `@Name` it goes to the owner while the ticket is in Todo (with an
/// owner), Doing, Plan review, Testing or Review; else it wakes nobody. `@Name` wakes those
/// employees. Refused on Done (the discussion is read-only there). Not for apps.
Map<String, dynamic> ticketPost({bool? confirmSecret, List<String> files = const [], List<String> images = const [], required String text, String? threadParent, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketPost,
  'params': <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'files': files,
    'images': images,
    'text': text,
    if (threadParent != null) 'thread_parent': threadParent,
    'ticket_id': ticketId,
  },
};

/// Try the summary of a ticket on Done again after it failed (`SystemLineKind::SummaryFailed`).
/// Only a board member; refused when the summary is already there or still being made.
Map<String, dynamic> ticketRetrySummary({required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketRetrySummary,
  'params': <String, dynamic>{
    'ticket_id': ticketId,
  },
};

/// Plan review → Doing in Plan mode: the owner makes a new plan with the feedback.
Map<String, dynamic> ticketRevisePlan({bool? confirmSecret, required String feedback, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketRevisePlan,
  'params': <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'feedback': feedback,
    'ticket_id': ticketId,
  },
};

/// Review → Doing with feedback (required) for the owner.
Map<String, dynamic> ticketSendBack({bool? confirmSecret, required String feedback, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketSendBack,
  'params': <String, dynamic>{
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    'feedback': feedback,
    'ticket_id': ticketId,
  },
};

/// Choose the board member responsible for a ticket (`ticket.set_responsible`,
/// feature `ticket_responsible`). Its own command rather than a field of
/// `UpdateTicket`, where "nobody" and "unchanged" would need two kinds of none.
Map<String, dynamic> ticketSetResponsible({String? responsibleUserId, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketSetResponsible,
  'params': <String, dynamic>{
    if (responsibleUserId != null) 'responsible_user_id': responsibleUserId,
    'ticket_id': ticketId,
  },
};

/// The Start button: Backlog to Todo. With an employee the dispatcher is skipped.
/// Also starts again a ticket whose main task failed.
Map<String, dynamic> ticketStart({String? employeeId, required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketStart,
  'params': <String, dynamic>{
    if (employeeId != null) 'employee_id': employeeId,
    'ticket_id': ticketId,
  },
};

/// Change the fields of a ticket. Left out: unchanged. The creator never
/// changes; the owner only through `ticket.assign`; mode and model only while
/// the main task does not run.
Map<String, dynamic> ticketUpdate({List<String>? addFiles, bool? clearDeadline, bool? confirmSecret, int? deadlineMs, String? description, List<String>? labels, TicketPriority? priority, TaskRun? run, required String ticketId, String? title}) => <String, dynamic>{
  'method': Methods.ticketUpdate,
  'params': <String, dynamic>{
    if (addFiles != null) 'add_files': addFiles,
    if (clearDeadline != null) 'clear_deadline': clearDeadline,
    if (confirmSecret != null) 'confirm_secret': confirmSecret,
    if (deadlineMs != null) 'deadline_ms': deadlineMs,
    if (description != null) 'description': description,
    if (labels != null) 'labels': labels,
    if (priority != null) 'priority': priority.toJson(),
    if (run != null) 'run': run.toJson(),
    'ticket_id': ticketId,
    if (title != null) 'title': title,
  },
};

/// Run a ticket on the Claude login of the board member who asks (B4, D3-3):
/// `TicketView::login_user` becomes that board member, never anyone else. The
/// creator stays. Only a board member; answers `TicketChanged`.
Map<String, dynamic> ticketUseMyLogin({required String ticketId}) => <String, dynamic>{
  'method': Methods.ticketUseMyLogin,
  'params': <String, dynamic>{
    'ticket_id': ticketId,
  },
};
