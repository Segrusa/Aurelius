// GENERATED CODE - DO NOT EDIT.
// Run: cargo xtask gen-dart
// ignore_for_file: unnecessary_cast, unnecessary_null_aware_operator, unnecessary_non_null_assertion, constant_identifier_names, use_null_aware_elements, unintended_html_in_doc_comment

import 'models.dart';

/// The protocol major this app speaks. The server refuses another one.
const int protocolMajor = 3;

/// The wire name of every method.
abstract final class Methods {
  /// One tool call of an answer whole: the input and output that
  /// `conversation.get` and live updates shorten (feature `activity_preview`).
  static const String activityGet = 'activity.get';
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
  static const String browserGet = 'browser.get';
  static const String browserSet = 'browser.set';
  /// Claude in Chrome or Playwright, for every run that starts afterwards.
  static const String browserSetBackend = 'browser.set_backend';
  static const String chatRetry = 'chat.retry';
  static const String chatSend = 'chat.send';
  /// The company in one read: departments, employees, the org chart setting
  /// and the management team (roots), plus the conversion report if any.
  static const String companyGet = 'company.get';
  static const String companySetFollowOrgChart = 'company.set_follow_org_chart';
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
  static const String departmentArchive = 'department.archive';
  static const String departmentCreate = 'department.create';
  /// The departments by name; archived ones too with `archived`.
  static const String departmentList = 'department.list';
  static const String departmentUpdate = 'department.update';
  /// An employee's persistent chat with the user (company scope, no project):
  /// its messages and settings, as `conversation.get` answers them.
  static const String employeeChat = 'employee.chat';
  static const String employeeCreate = 'employee.create';
  /// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
  static const String employeeInstructionVersions = 'employee.instruction_versions';
  /// The company's employees (not archived), oldest first; optionally of one department.
  static const String employeeList = 'employee.list';
  static const String employeeRestoreInstructions = 'employee.restore_instructions';
  static const String employeeSendMessage = 'employee.send_message';
  static const String employeeSetDepartment = 'employee.set_department';
  static const String employeeSetFolder = 'employee.set_folder';
  static const String employeeSetReportsTo = 'employee.set_reports_to';
  static const String employeeSetStatus = 'employee.set_status';
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
  /// What goes into a context: an employee's (own, its department, the
  /// project if given, global), a project conversation's (project, global; no
  /// employee) or a department's alone. At least one of the three.
  static const String memoryGetContext = 'memory.get_context';
  /// Memories, newest first: global always, plus the scopes asked for (a
  /// project, an employee, a department). An employee also brings its
  /// department's. Nothing asked: global and every department and employee.
  static const String memoryList = 'memory.list';
  static const String memoryPin = 'memory.pin';
  static const String memoryRemember = 'memory.remember';
  static const String memoryRevise = 'memory.revise';
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
  /// The conversations of a project, most recently active first.
  static const String projectConversationList = 'project_conversation.list';
  static const String projectConversationSend = 'project_conversation.send';
  static const String projectConversationStart = 'project_conversation.start';
  static const String proposalDecide = 'proposal.decide';
  /// Proposals of the whole company (colleagues and instructions): pending
  /// first, then the latest decisions; optionally of one department.
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
  /// The context manifest of one run (context inspector).
  static const String runGetContext = 'run.get_context';
  /// What one run sent to the runtime and what each model call cost; not found
  /// unless trace mode was on for it and it is among the latest (feature `run_trace`).
  static const String runGetTrace = 'run.get_trace';
  static const String runStop = 'run.stop';
  /// A screenshot the agent took (the id is in its screenshot tool's output).
  static const String screenshotGet = 'screenshot.get';
  static const String secretSet = 'secret.set';
  static const String serverGetHealth = 'server.get_health';
  static const String serverRestart = 'server.restart';
  /// The first request of a session: who is calling. The protocol major in
  /// every request must equal the server's, else the answer is a hard error.
  static const String sessionHello = 'session.hello';
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
  /// Whether tasks may be tried in several ways first (feature `task_variants`).
  static const String variantsGetSetting = 'variants.get_setting';
  /// Allow tasks to be tried in several ways first (feature `task_variants`; off by default).
  static const String variantsSetEnabled = 'variants.set_enabled';
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

Map<String, dynamic> browserGet() => <String, dynamic>{
  'method': Methods.browserGet,
};

/// The company in one read: departments, employees, the org chart setting
/// and the management team (roots), plus the conversion report if any.
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

/// The departments by name; archived ones too with `archived`.
Map<String, dynamic> departmentList({bool? archived}) => <String, dynamic>{
  'method': Methods.departmentList,
  'params': <String, dynamic>{
    if (archived != null) 'archived': archived,
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

/// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
Map<String, dynamic> employeeInstructionVersions({required String employeeId}) => <String, dynamic>{
  'method': Methods.employeeInstructionVersions,
  'params': <String, dynamic>{
    'employee_id': employeeId,
  },
};

/// The company's employees (not archived), oldest first; optionally of one department.
Map<String, dynamic> employeeList({String? departmentId}) => <String, dynamic>{
  'method': Methods.employeeList,
  'params': <String, dynamic>{
    if (departmentId != null) 'department_id': departmentId,
  },
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

/// What goes into a context: an employee's (own, its department, the
/// project if given, global), a project conversation's (project, global; no
/// employee) or a department's alone. At least one of the three.
Map<String, dynamic> memoryGetContext({String? departmentId, String? employeeId, String? projectId}) => <String, dynamic>{
  'method': Methods.memoryGetContext,
  'params': <String, dynamic>{
    if (departmentId != null) 'department_id': departmentId,
    if (employeeId != null) 'employee_id': employeeId,
    if (projectId != null) 'project_id': projectId,
  },
};

/// Memories, newest first: global always, plus the scopes asked for (a
/// project, an employee, a department). An employee also brings its
/// department's. Nothing asked: global and every department and employee.
Map<String, dynamic> memoryList({String? departmentId, String? employeeId, String? projectId}) => <String, dynamic>{
  'method': Methods.memoryList,
  'params': <String, dynamic>{
    if (departmentId != null) 'department_id': departmentId,
    if (employeeId != null) 'employee_id': employeeId,
    if (projectId != null) 'project_id': projectId,
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
/// first, then the latest decisions; optionally of one department.
Map<String, dynamic> proposalList({String? departmentId}) => <String, dynamic>{
  'method': Methods.proposalList,
  'params': <String, dynamic>{
    if (departmentId != null) 'department_id': departmentId,
  },
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

/// The first request of a session: who is calling. The protocol major in
/// every request must equal the server's, else the answer is a hard error.
Map<String, dynamic> sessionHello(HelloParams params) => <String, dynamic>{
  'method': Methods.sessionHello,
  'params': params.toJson(),
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
Map<String, dynamic> taskList({String? departmentId, String? employeeId, String? projectId, TaskListState? state}) => <String, dynamic>{
  'method': Methods.taskList,
  'params': <String, dynamic>{
    if (departmentId != null) 'department_id': departmentId,
    if (employeeId != null) 'employee_id': employeeId,
    if (projectId != null) 'project_id': projectId,
    if (state != null) 'state': state?.toJson(),
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
Map<String, dynamic> usageGetTotals({String? conversationId, String? taskId}) => <String, dynamic>{
  'method': Methods.usageGetTotals,
  'params': <String, dynamic>{
    if (conversationId != null) 'conversation_id': conversationId,
    if (taskId != null) 'task_id': taskId,
  },
};

/// Whether tasks may be tried in several ways first (feature `task_variants`).
Map<String, dynamic> variantsGetSetting() => <String, dynamic>{
  'method': Methods.variantsGetSetting,
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

/// Allow tasks to be tried in several ways first (feature `task_variants`; off by default).
Map<String, dynamic> variantsSetEnabled({required bool enabled}) => <String, dynamic>{
  'method': Methods.variantsSetEnabled,
  'params': <String, dynamic>{
    'enabled': enabled,
  },
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

Map<String, dynamic> chatRetry(RetryReply params) => <String, dynamic>{
  'method': Methods.chatRetry,
  'params': params.toJson(),
};

Map<String, dynamic> chatSend(SendChatMessage params) => <String, dynamic>{
  'method': Methods.chatSend,
  'params': params.toJson(),
};

Map<String, dynamic> companySetFollowOrgChart(SetFollowOrgChart params) => <String, dynamic>{
  'method': Methods.companySetFollowOrgChart,
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

Map<String, dynamic> departmentArchive(ArchiveDepartment params) => <String, dynamic>{
  'method': Methods.departmentArchive,
  'params': params.toJson(),
};

Map<String, dynamic> departmentCreate(CreateDepartment params) => <String, dynamic>{
  'method': Methods.departmentCreate,
  'params': params.toJson(),
};

Map<String, dynamic> departmentUpdate(UpdateDepartment params) => <String, dynamic>{
  'method': Methods.departmentUpdate,
  'params': params.toJson(),
};

Map<String, dynamic> employeeCreate(CreateEmployee params) => <String, dynamic>{
  'method': Methods.employeeCreate,
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

Map<String, dynamic> employeeSetDepartment(SetEmployeeDepartment params) => <String, dynamic>{
  'method': Methods.employeeSetDepartment,
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
