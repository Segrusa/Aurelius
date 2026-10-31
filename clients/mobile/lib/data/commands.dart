import 'run_config.dart';
import 'views.dart';

/// Writes a phone may send, as requests for `AureliusRepository.write`:
/// short names over the generated method functions. Folder, connection and
/// credential writes stay on the desktop.
abstract final class Commands {
  static Map<String, dynamic> retryReply(String conversationId, String userMessageId) =>
      chatRetry(RetryReply(conversationId: conversationId, userMessageId: userMessageId));

  static Map<String, dynamic> stopRun(String runId) => runStop(StopRun(runId: runId));

  /// A message still waiting behind a running answer.
  static Map<String, dynamic> deleteQueuedMessage(String conversationId, String messageId) =>
      conversationDeleteQueued(DeleteQueuedMessage(conversationId: conversationId, messageId: messageId));

  static Map<String, dynamic> sendQueuedNow(String conversationId, String messageId) =>
      conversationSendQueuedNow(SendQueuedNow(conversationId: conversationId, messageId: messageId));

  static Map<String, dynamic> deleteConversation(String conversationId) => conversationDelete(DeleteConversation(conversationId: conversationId));

  static Map<String, dynamic> setAgentMode(String conversationId, AgentMode mode) =>
      conversationSetAgentMode(SetAgentMode(conversationId: conversationId, mode: mode));

  /// Model and effort for this conversation alone; each `null` follows the
  /// employee (never changes the employee's or the global default).
  static Map<String, dynamic> setConversationRun(String conversationId, {String? model, String? effort}) =>
      conversationSetRun(SetConversationRun(conversationId: conversationId, model: model, effort: effort));

  /// The agent may use the user's own browser (Claude in Chrome). Turning it on
  /// needs the Approve right, like Auto mode.
  static Map<String, dynamic> setBrowserAllowed(String conversationId, bool allowed) =>
      conversationSetBrowserAllowed(SetBrowserAllowed(allowed: allowed, conversationId: conversationId));

  /// The agent may take screenshots without asking each time. Off, it asks per
  /// screenshot. Turning it on needs the Approve right, like the browser.
  static Map<String, dynamic> setScreenAllowed(String conversationId, bool allowed) =>
      conversationSetScreenAllowed(SetScreenAllowed(allowed: allowed, conversationId: conversationId));

  static Map<String, dynamic> removeAttachment(String attachmentId) => attachmentRemove(RemoveAttachment(attachmentId: attachmentId));

  /// A message to an employee in its own chat or one of its task conversations.
  /// `files`: upload paths of files sent with the message; the PC copies them
  /// into the employee's work folder.
  static Map<String, dynamic> sendEmployeeMessage(String employeeId, String text, {required String conversationId, List<String> images = const [], List<String> files = const [], bool interrupt = false}) =>
      employeeSendMessage(SendEmployeeMessage(
        conversationId: conversationId,
        files: files,
        images: images,
        interrupt: interrupt ? true : null,
        employeeId: employeeId,
        text: text,
      ));

  /// A message of the user in the group chat (Team): `@Name` wakes those
  /// employees, else the CEO answers.
  static Map<String, dynamic> sendGroupMessage(String text, {List<String> images = const [], List<String> files = const []}) =>
      groupChatSend(SendGroupMessage(files: files, images: images, text: text));

  /// A new conversation of the project. Auto, browser and screen need the Approve right.
  static Map<String, dynamic> startProjectConversation(String projectId, {AgentMode? mode, bool browserAllowed = false, bool screenAllowed = false}) =>
      projectConversationStart(StartProjectConversation(
        agentMode: mode,
        browserAllowed: browserAllowed ? true : null,
        projectId: projectId,
        screenAllowed: screenAllowed ? true : null,
      ));

  /// A message in a project conversation; without [conversationId] it starts
  /// a new one with the mode, browser and screen given here.
  static Map<String, dynamic> sendProjectMessage(
    String projectId,
    String text, {
    String? conversationId,
    List<String> images = const [],
    List<String> files = const [],
    bool interrupt = false,
    AgentMode? mode,
    bool browserAllowed = false,
    bool screenAllowed = false,
  }) =>
      projectConversationSend(SendProjectMessage(
        agentMode: mode,
        browserAllowed: browserAllowed ? true : null,
        conversationId: conversationId,
        files: files,
        images: images,
        interrupt: interrupt ? true : null,
        projectId: projectId,
        screenAllowed: screenAllowed ? true : null,
        text: text,
      ));

  static Map<String, dynamic> shareMessage(String conversationId, String messageId, {required String targetKind, required String targetId, required ShareMode mode}) =>
      shareCreate(ShareMessage(
        conversationId: conversationId,
        messageId: messageId,
        mode: mode,
        target: targetKind == 'employee' ? ShareTargetEmployee(id: targetId) : ShareTargetProject(id: targetId),
      ));

  static Map<String, dynamic> discussTask(String taskId) => taskDiscuss(DiscussTask(taskId: taskId));

  static Map<String, dynamic> revokeShare(String shareId) => shareRevoke(RevokeShare(shareId: shareId));

  static Map<String, dynamic> createProject(String name, {String? description}) =>
      projectCreate(CreateProject(name: name, description: description));

  static Map<String, dynamic> renameProject(String projectId, String name) => projectRename(RenameProject(name: name, projectId: projectId));

  /// Trimmed by the server; an empty description clears it (max 2000 chars).
  static Map<String, dynamic> setProjectDescription(String projectId, String description) =>
      projectSetDescription(SetProjectDescription(description: description, projectId: projectId));

  /// The folder on the PC the project's tasks run in.
  static Map<String, dynamic> setProjectFolder(String projectId, String path) => projectSetFolder(SetProjectFolder(path: path, projectId: projectId));

  static Map<String, dynamic> setProjectStatus(String projectId, LifecycleStatus status) =>
      projectSetStatus(SetProjectStatus(projectId: projectId, status: status));

  /// The company setting "Follow the org chart" (on by default). Needs the Approve right.
  static Map<String, dynamic> setFollowOrgChart(bool follow) => companySetFollowOrgChart(SetFollowOrgChart(follow: follow));

  /// A new employee of the company; the name must be unique company-wide.
  static Map<String, dynamic> createEmployee(String name, String role, {String? instructions, String? title, String? reportsTo}) =>
      employeeCreate(CreateEmployee(instructions: instructions, name: name, reportsTo: reportsTo, role: role, title: title));

  /// `sequentialTasks` only when it changes (omitted = unchanged). `title` is the
  /// employee's title in the org chart; empty clears it.
  static Map<String, dynamic> updateEmployee(String employeeId, String name, String role, {String? instructions, bool? sequentialTasks, String? title}) =>
      employeeUpdate(UpdateEmployee(instructions: instructions, employeeId: employeeId, name: name, role: role, sequentialTasks: sequentialTasks, title: title));

  /// Who an employee reports to; `null` puts it in the management team. The
  /// server refuses a cycle with a validation error.
  static Map<String, dynamic> setReportsTo(String employeeId, String? reportsTo) =>
      employeeSetReportsTo(SetEmployeeReportsTo(employeeId: employeeId, reportsTo: reportsTo));

  /// A colleague an employee proposed: approve it (the server creates the employee) or decline with a note.
  /// Needs the Approve right.
  /// For proposed instructions, [instructions] approves an edited text instead.
  static Map<String, dynamic> decideProposal(String proposalId, bool approve, {String? note, String? instructions}) =>
      proposalDecide(DecideProposal(approve: approve, instructions: instructions, note: note, proposalId: proposalId));

  static Map<String, dynamic> setEmployeeStatus(String employeeId, LifecycleStatus status) =>
      employeeSetStatus(SetEmployeeStatus(employeeId: employeeId, status: status));

  /// `every_minutes` 0 turns the heartbeat off.
  static Map<String, dynamic> setHeartbeat(String employeeId, int everyMinutes, {int? dailyTokenBudget}) =>
      heartbeatSet(SetHeartbeat(dailyTokenBudget: dailyTokenBudget, everyMinutes: everyMinutes, employeeId: employeeId));

  /// Give a task to an employee; `null` makes it a draft again.
  static Map<String, dynamic> assignTask(String taskId, String? employeeId) => taskAssign(AssignTask(employeeId: employeeId, taskId: taskId));

  /// [run]: mode, connection_id, model, effort of the task's conversation (see [TaskRun.choices]); unset follows the employee.
  /// [files]: upload paths of attached files. [variants] 2 or 3 tries the task that many ways first (feature `task_variants`).
  static Map<String, dynamic> createTask(String projectId, String title, String goal, List<String> criteria, {String? employeeId, Map<String, String>? run, List<String> files = const [], int variants = 1}) =>
      taskCreate(CreateTask(
        acceptanceCriteria: criteria,
        files: files,
        goal: goal,
        employeeId: employeeId,
        projectId: projectId,
        run: withVariants(taskRunOf(run), variants),
        title: title,
      ));

  /// Recurring tasks.
  static Map<String, dynamic> createTaskSchedule(
    String projectId,
    String employeeId,
    String title,
    String goal,
    List<String> criteria,
    Recurrence recurrence, {
    Map<String, String>? run,
    List<String> files = const [],
    bool paused = false,
  }) =>
      recurringTaskCreate(CreateTaskSchedule(
        acceptanceCriteria: criteria,
        files: files,
        goal: goal,
        employeeId: employeeId,
        paused: paused,
        projectId: projectId,
        recurrence: recurrence,
        run: taskRunOf(run),
        title: title,
      ));

  /// `removeFiles`: paths of attachments the schedule already has.
  static Map<String, dynamic> updateTaskSchedule(
    String scheduleId,
    String employeeId,
    String title,
    String goal,
    List<String> criteria,
    Recurrence recurrence, {
    Map<String, String>? run,
    List<String> addFiles = const [],
    List<String> removeFiles = const [],
  }) =>
      recurringTaskUpdate(UpdateTaskSchedule(
        acceptanceCriteria: criteria,
        addFiles: addFiles,
        goal: goal,
        employeeId: employeeId,
        recurrence: recurrence,
        removeFiles: removeFiles,
        run: taskRunOf(run),
        scheduleId: scheduleId,
        title: title,
      ));

  static Map<String, dynamic> deleteTaskSchedule(String scheduleId) => recurringTaskDelete(DeleteTaskSchedule(scheduleId: scheduleId));

  static Map<String, dynamic> setTaskSchedulePaused(String scheduleId, bool paused) =>
      recurringTaskSetPaused(SetTaskSchedulePaused(paused: paused, scheduleId: scheduleId));

  static Map<String, dynamic> runTaskScheduleNow(String scheduleId) => recurringTaskRunNow(RunTaskScheduleNow(scheduleId: scheduleId));

  /// [files]: upload paths of attached files (feature `idea_attachments`).
  static Map<String, dynamic> addIdea(String projectId, String title, String? note, {IdeaKind? kind, List<String> files = const []}) =>
      ideaAdd(AddIdea(files: files, kind: kind, note: note, projectId: projectId, title: title));

  /// [addFiles]: upload paths; [removeFiles]: `path`s of the idea's own attachments.
  static Map<String, dynamic> updateIdea(String ideaId, String title, String? note, {IdeaKind? kind, List<String> addFiles = const [], List<String> removeFiles = const []}) =>
      ideaUpdate(UpdateIdea(addFiles: addFiles, ideaId: ideaId, kind: kind, note: note, removeFiles: removeFiles, title: title));

  static Map<String, dynamic> deleteIdea(String ideaId) => ideaDelete(DeleteIdea(ideaId: ideaId));

  static Map<String, dynamic> setIdeaStatus(String ideaId, IdeaStatus status) => ideaSetStatus(SetIdeaStatus(ideaId: ideaId, status: status));

  /// The idea becomes a task for this employee. Without [mode] the task follows
  /// the employee's own mode (Auto needs the Approve right).
  /// [variants] as for [createTask].
  static Map<String, dynamic> startIdea(String ideaId, String employeeId, {AgentMode? mode, int variants = 1}) =>
      ideaStart(StartIdea(ideaId: ideaId, employeeId: employeeId, run: withVariants(mode == null ? null : TaskRun(mode: mode), variants)));

  /// Take variant [variant] of a task whose variants are ready; null takes the judge's pick. Needs the Approve right.
  static Map<String, dynamic> chooseVariant(String taskId, {int? variant}) => taskChooseVariant(ChooseTaskVariant(taskId: taskId, variant: variant));

  /// [run] with `variants` set when more than one; 1 sends nothing, as older servers expect.
  static TaskRun? withVariants(TaskRun? run, int variants) => variants <= 1
      ? run
      : TaskRun(connectionId: run?.connectionId, effort: run?.effort, mode: run?.mode, model: run?.model, variants: variants);

  static Map<String, dynamic> cancelTask(String taskId) => taskCancel(CancelTask(taskId: taskId));

  /// The run settings for chat or employee runs (model, effort) at [scope]
  /// (global by default, or one employee). The other fields go back unchanged.
  static Map<String, dynamic> setRunDefaults(Purpose purpose, RunSettings settings, {RunScope scope = RunScope.global}) =>
      connectionSetDefaults(SetRunDefaults(purpose: purpose, scope: scope.toConfig(), settings: settings));

  /// [alwaysAllow] (with approve, eligible MCP tools only): the tool passes
  /// without asking from now on, in every run, and its other pending asks go ahead.
  static Map<String, dynamic> decideApproval(String approvalId, String actionDigest, bool approve, {bool alwaysAllow = false}) =>
      approvalDecide(DecideApproval(actionDigest: actionDigest, approvalId: approvalId, approve: approve, alwaysAllow: alwaysAllow && approve));

  /// Answers to Claude's question, keyed by question text: the chosen label,
  /// several joined by ", ", or the user's own words. Skipping is a reject.
  static Map<String, dynamic> answerQuestion(String approvalId, Map<String, String> answers) =>
      approvalAnswer(AnswerQuestion(answers: answers, approvalId: approvalId));

  /// The user's decision on a plan Claude handed in: `approve` (the worker goes
  /// on in auto mode), `approve_edits` (edits go through, the rest is asked), `revise`
  /// (feedback required) or `reject` (feedback optional).
  static Map<String, dynamic> decidePlan(String approvalId, PlanDecision decision, {String? feedback}) =>
      approvalDecidePlan(DecidePlan(approvalId: approvalId, decision: decision, feedback: feedback));

  /// `scopeKind`: global | project | employee.
  static Map<String, dynamic> remember(String scopeKind, String? scopeId, MemoryKind kind, String content, {bool pinned = false}) =>
      memoryRemember(Remember(
        content: content,
        kind: kind,
        pinned: pinned,
        scope: switch (scopeKind) {
          'project' => MemoryScopeProject(id: scopeId!),
          'employee' => MemoryScopeEmployee(id: scopeId!),
          _ => const MemoryScopeGlobal(),
        },
      ));

  static Map<String, dynamic> decideMemory(String memoryId, bool accept) => memoryDecide(DecideMemory(accept: accept, memoryId: memoryId));

  static Map<String, dynamic> reviseMemory(String memoryId, String content) => memoryRevise(ReviseMemory(content: content, memoryId: memoryId));

  static Map<String, dynamic> forgetMemory(String memoryId) => memoryForget(ForgetMemory(memoryId: memoryId));

  static Map<String, dynamic> pinMemory(String memoryId, bool pinned) => memoryPin(PinMemory(memoryId: memoryId, pinned: pinned));
}
