import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import '../core/hosts/host.dart';
import '../core/net/connection.dart';
import '../core/net/gateway_api.dart';
import '../core/net/protocol.dart';
import 'protocol/protocol.dart';

/// The data layer for one host: typed queries and writes over the
/// connection (the generated method functions), plus the change signals
/// blocs refresh on.
class AureliusRepository {
  AureliusRepository(this.connection);

  final AureliusConnection connection;

  Host get host => connection.host;
  Stream<LinkState> get link => connection.states;
  LinkState get linkState => connection.state;
  Stream<Event> get events => connection.events;
  Stream<StreamAppend> get appends => connection.appends;

  /// Fires when the link comes (back) online: read models refetch then.
  Stream<void> get online => connection.states.where((s) => s.isOnline).map((_) {});

  /// Chat changes (messages, conversations, presence).
  Stream<Event> get chatChanges => events.where((e) => e.isChat && !e.isNoise);

  /// Anything a screen may show, without streaming bookkeeping (notes have [noteChanges]).
  Stream<Event> get changes => events.where((e) => !e.isNoise && !e.isNote);

  /// Notes created, saved, pinned or deleted (also by this phone).
  Stream<Event> get noteChanges => events.where((e) => e.isNote);

  /// The machine's load as it changes.
  Stream<LoadSnapshot> get loadChanges => connection.loads;

  /// The pending shutdown of the server's PC as it changes.
  Stream<PowerState> get powerChanges => connection.powers;

  /// Company changes (projects, employees, tasks, approvals, memory, …).
  Stream<Event> get companyChanges => events.where((e) => !e.isChat && !e.isNote);

  Future<R> _ask<R extends ResponseBody>(Map<String, dynamic> request, [Duration? timeout]) async {
    final body = timeout == null ? await connection.call(request) : await connection.call(request, timeout);
    return body as R;
  }

  /// A write the host answers with a [HostResult] (settings, plugins, power, …).
  Future<HostResult> _host(Map<String, dynamic> request, [Duration? timeout]) async {
    final receipt = await connection.write(request, retry: false, timeout: timeout);
    return (receipt.result as WriteResultHost).value;
  }

  /// The board members, oldest first ("Who's working here?").
  Future<List<UserView>> boardMembers() async => (await _ask<ResponseBodyUsers>(userList())).value.users;

  /// A board member was added (here or on another client).
  Stream<Event> get boardMemberChanges => events.where((e) => e.kind == EventKind.userCreated);

  /// Choose [userId] on this phone after the one password check (B8).
  Future<UserView> useBoardMember(String userId, String password) => connection.useBoardMember(userId, password);

  /// A new board member; returns its id. Without an active one, it becomes this phone's.
  Future<String> createBoardMember(String username, String password) => connection.createBoardMember(username, password);

  /// The Board: its six columns with their cards, for one project or all (B4).
  Future<BoardView> board({String? projectId}) async => (await _ask<ResponseBodyBoard>(boardGet(projectId: projectId))).value;

  /// One ticket with its main task and every task linked to it.
  Future<TicketDetail> ticket(String ticketId) async => (await _ask<ResponseBodyTicket>(ticketGet(ticketId: ticketId))).value;

  /// What changes a card or the columns: tickets, the columns, the tasks under
  /// tickets, projects and employees (dispatcher, names).
  Stream<Event> get boardChanges => changes.where((e) {
        final kind = e.kind.wire;
        return kind.startsWith('ticket.') || kind.startsWith('board.') || kind.startsWith('task.') || kind.startsWith('project.') || kind.startsWith('employee.') || kind.startsWith('company.') || kind.startsWith('approval.');
      });

  Future<List<ProjectSummary>> projects() async => (await _ask<ResponseBodyProjects>(projectList())).value.projects;

  /// A project's conversations, most recent first.
  Future<List<ProjectConversationSummary>> projectConversations(String projectId) async =>
      (await _ask<ResponseBodyProjectConversations>(projectConversationList(projectId: projectId))).value.conversations;

  Future<MessagesSnapshot> messages(String conversationId) async =>
      (await _ask<ResponseBodyMessages>(conversationGet(conversationId: conversationId))).value;

  /// The company's group chat (Team): the user and every employee.
  Future<MessagesSnapshot> groupChat() async => (await _ask<ResponseBodyMessages>(groupChatGet())).value;

  /// One tool call of message [messageId] whole (`activity_preview`: messages
  /// and live updates carry shortened ones). Only on demand, when the user opens it.
  Future<ActivityView> activity(String messageId, String callId) async =>
      (await _ask<ResponseBodyActivity>(activityGet(messageId: messageId, callId: callId))).value;

  /// The company's employees (archived ones left out by the server).
  Future<List<EmployeeSummary>> employees() async => (await _ask<ResponseBodyEmployees>(employeeList())).value.employees;

  /// The company in one read: employees, Follow the org chart and the management team.
  Future<CompanySnapshot> company() async => (await _ask<ResponseBodyCompany>(companyGet())).value;

  /// Colleague and instruction proposals of the whole company, pending first.
  Future<List<ProposalView>> proposals() async => (await _ask<ResponseBodyProposals>(proposalList())).value.proposals;

  /// A task's variants and the judge's comparison (feature `task_variants`).
  Future<TaskVariantsView> variantsOf(String taskId) async => (await _ask<ResponseBodyTaskVariants>(taskVariants(taskId: taskId))).value;

  /// Whether tasks may be tried in several ways; null when the server lacks
  /// `task_variants` or cannot say (the picker stays hidden then).
  Future<VariantsSetting?> variantsSetting() async {
    if (!linkState.has('task_variants')) return null;
    try {
      return (await _ask<ResponseBodyVariantsSetting>(variantsGetSetting())).value;
    } catch (_) {
      return null;
    }
  }

  /// Whether management chooses its own task mode (feature `management_chooses_mode`
  /// and the setting on); false when the server lacks it or cannot say.
  Future<bool> managementChoosesMode() async {
    if (!linkState.has('management_chooses_mode')) return false;
    try {
      return (await _ask<ResponseBodyManagementModeSetting>(managementModeGetSetting())).value.enabled;
    } catch (_) {
      return false;
    }
  }

  /// The server's jobs: the queue and what runs now.
  Future<List<JobSummary>> jobs() async => (await _ask<ResponseBodyJobs>(jobList())).value.jobs;

  Future<List<ApprovalView>> approvals() async => (await _ask<ResponseBodyApprovals>(approvalList())).value.approvals;

  /// Tasks, newest first, company-wide unless filtered.
  Future<List<TaskView>> tasks({String? projectId, String? employeeId}) async =>
      (await _ask<ResponseBodyTasks>(taskList(projectId: projectId, employeeId: employeeId))).value.tasks;

  Future<TimelinePage> timeline({String? projectId, String? correlationId, int? beforeSeq, int limit = 40, bool activity = false}) async =>
      (await _ask<ResponseBodyTimeline>(timelineList(TimelineQuery(
        activity: activity,
        beforeSeq: beforeSeq,
        correlationId: correlationId,
        limit: limit,
        projectId: projectId,
      ))))
          .value;

  /// Global memory plus the scopes asked for (a project, an employee).
  Future<List<MemoryView>> memory({String? projectId, String? employeeId}) async =>
      (await _ask<ResponseBodyMemory>(memoryList(projectId: projectId, employeeId: employeeId))).value.items;

  Future<List<ShareView>> shares(String projectId) async => (await _ask<ResponseBodyShares>(shareList(projectId: projectId))).value;

  Future<ConnectionsSnapshot> connections() async => (await _ask<ResponseBodyConnections>(connectionList())).value;

  Future<NotificationsSnapshot> notifications() async => (await _ask<ResponseBodyNotifications>(notificationList())).value;

  Future<void> markNotificationsRead() => _host(notificationMarkAllRead());

  /// Remove one notification (swiped away).
  Future<void> dismissNotification(String id) => _host(notificationDismiss(id: id));

  /// Remove every notification; the number removed.
  Future<int> dismissAllNotifications() async {
    final result = await _host(notificationDismissAll());
    return result is HostResultCount ? result.count : 0;
  }

  Future<UsageSnapshot> usage() async => (await _ask<ResponseBodyUsage>(usageGet())).value;

  /// One day's usage per hour; `dayStartMs` as in [UsageSnapshot.byDay].
  Future<UsageHours> usageHours(int dayStartMs) async => (await _ask<ResponseBodyUsageHours>(usageGetHours(dayStartMs: dayStartMs))).value;

  Future<SubscriptionUsage> subscription({bool refresh = false}) async => (await _ask<ResponseBodySubscription>(subscriptionGet(refresh: refresh))).value;

  Future<ContextView> context(String runId) async => (await _ask<ResponseBodyContext>(runGetContext(runId: runId))).value;

  /// A screenshot the agent took (JPEG), by the id in its tool output.
  Future<Uint8List> screenshot(String id) async => base64Decode((await _ask<ResponseBodyScreenshot>(screenshotGet(id: id))).jpegBase64);

  /// A project's notes, pinned first, then the last edited; [query] searches their text.
  Future<List<NoteSummary>> notes(String projectId, {String? query}) async =>
      (await _ask<ResponseBodyNotes>(noteList(projectId: projectId, query: query))).value.notes;

  Future<NoteView> note(String noteId) async => (await _ask<ResponseBodyNote>(noteGet(noteId: noteId))).value;

  Future<NoteView> createNote(String projectId, {String body = ''}) async =>
      ((await _host(noteCreate(projectId: projectId, body: body))) as HostResultNote).value;

  /// Saves [body] if the note is still at [baseRevision]; a [ProtocolException]
  /// with [ProtocolException.isConflict] means it was saved elsewhere.
  Future<NoteView> updateNote(String noteId, String body, int baseRevision) async =>
      ((await _host(noteUpdate(noteId: noteId, body: body, baseRevision: baseRevision))) as HostResultNote).value;

  Future<NoteView> setNotePinned(String noteId, bool pinned) async =>
      ((await _host(noteSetPinned(noteId: noteId, pinned: pinned))) as HostResultNote).value;

  Future<void> deleteNote(String noteId) => _host(noteDelete(noteId: noteId));

  /// The ideas of every project; each keeps its own [IdeaView.projectId].
  Future<List<IdeaView>> allIdeas() async => (await _ask<ResponseBodyAllIdeas>(ideaListAll())).value.ideas;

  /// A project's ideas.
  Future<List<IdeaView>> ideas(String projectId) async => (await _ask<ResponseBodyIdeas>(ideaList(projectId: projectId))).value.ideas;

  /// A project's recurring tasks, oldest first.
  Future<List<TaskScheduleView>> taskSchedules({String? projectId}) async =>
      (await _ask<ResponseBodyTaskSchedules>(recurringTaskList(projectId: projectId))).value.schedules;

  /// A file kept with a message.
  Future<Uint8List> file(String id) async => base64Decode((await _ask<ResponseBodyFile>(fileGet(id: id))).dataBase64);

  /// The server fetches uploaded files by id (`upload.get`); older servers can't.
  bool get uploadsFetchable => linkState.has('upload_fetch');

  /// A file the user uploaded ([MessageFileView.id]), fetched part by part.
  Future<Uint8List> fetchUpload(String id) async {
    final bytes = BytesBuilder(copy: false);
    while (true) {
      final offset = bytes.length;
      final part = (await _ask<ResponseBodyUpload>(uploadGet(id: id, offset: offset))).value;
      bytes.add(base64Decode(part.dataBase64));
      // An empty part before the end would loop forever.
      if (part.done || bytes.length == offset) return bytes.takeBytes();
    }
  }

  /// An uploaded image as a JPEG that fits [maxSide] pixels.
  Future<Uint8List> uploadThumbnail(String id, int maxSide) async =>
      base64Decode((await _ask<ResponseBodyUpload>(uploadGet(id: id, maxSide: maxSide))).value.dataBase64);

  /// The Markdown files in a scope's work folder.
  Future<MarkdownFileList> markdownFiles(FileScope scope) async => (await _ask<ResponseBodyMarkdownFiles>(markdownList(scope: scope))).value;

  /// One Markdown file, by path relative to the work folder or absolute inside it.
  Future<MarkdownFileContent> markdownFile(FileScope scope, String path) async =>
      (await _ask<ResponseBodyMarkdownFile>(markdownGet(path: path, scope: scope))).value;

  /// Branch and uncommitted change where a conversation works;
  /// `null` when that folder is not in a Git repository.
  Future<GitStatusView?> gitStatus(String conversationId) async =>
      (await _ask<ResponseBodyGitStatus>(gitGetStatus(conversationId: conversationId))).value;

  /// The subagents running in one conversation's answers.
  Future<List<AgentView>> subagents(String conversationId) async =>
      (await _ask<ResponseBodySubagents>(subagentList(conversationId: conversationId))).value;

  /// The uncommitted change itself, for the diff view.
  Future<GitDiffView?> gitDiff(String conversationId) async => (await _ask<ResponseBodyGitDiff>(gitGetDiff(conversationId: conversationId))).value;

  /// The server's update state; `check` asks GitHub for the latest release now.
  Future<UpdateStatus> updateStatus({bool check = false}) async => (await _ask<ResponseBodyUpdate>(updateGet(check: check))).value;

  /// Download and verify the latest release on the server (can take minutes).
  Future<UpdateStatus> prepareUpdate() async => ((await _host(updatePrepare(), const Duration(minutes: 10))) as HostResultUpdate).value;

  /// Install the prepared release; the server restarts right after answering.
  Future<UpdateStatus> applyUpdate() async => ((await _host(updateApply(), const Duration(minutes: 2))) as HostResultUpdate).value;

  /// The keys of outside apps, newest first (feature `app_keys`). Never holds a secret.
  Future<List<AppKeyView>> appKeys() async => (await _ask<ResponseBodyAppKeys>(appKeyList())).value;

  /// Make a key. The answer holds the secrets (`amk_…`, and `whsec_…` with a webhook) once.
  Future<AppKeyCreated> createAppKey(CreateAppKey params) async =>
      ((await _host(appKeyCreate(params))) as HostResultAppKeyCreated).value;

  /// Revoke a key at once, also for the app's open sessions.
  Future<AppKeyView> revokeAppKey(String keyId) async => ((await _host(appKeyRevoke(keyId: keyId))) as HostResultAppKey).value;

  /// The user's Claude Code MCP servers and which are on for every chat, conversation, employee and task.
  Future<McpServers> mcpServers() async => (await _ask<ResponseBodyMcpServers>(mcpList())).value;

  /// Replace the enabled servers (Approve right); applies from the next run.
  Future<McpServers> setMcpServers(List<String> enabled) async => ((await _host(mcpSet(enabled: enabled))) as HostResultMcpServers).value;

  /// MCP login and logout changed (`mcp.login_finished`, `mcp.auth_changed`).
  Stream<Event> get mcpAuthChanges => events.where((e) => e.kind == EventKind.mcpLoginFinished || e.kind == EventKind.mcpAuthChanged);

  /// Start an OAuth login for an http/sse server (Approve right); open its `authorizationUrl`.
  Future<McpLogin> startMcpLogin(String name) async =>
      ((await _host(mcpLoginStart(name: name), const Duration(minutes: 1))) as HostResultMcpLogin).value;

  /// Finish a login with the address the phone's browser ended on (`…/callback?code=…&state=…`).
  Future<McpLogin> completeMcpLogin(String loginId, String redirectUrl) async =>
      ((await _host(mcpLoginComplete(loginId: loginId, redirectUrl: redirectUrl), const Duration(minutes: 1))) as HostResultMcpLogin).value;

  /// Where a login stands (pending, succeeded, failed, expired).
  Future<McpLogin> mcpLogin(String loginId) async => (await _ask<ResponseBodyMcpLogin>(mcpLoginGet(loginId: loginId))).value;

  /// Forget the server's tokens (Approve right); answers like [mcpServers].
  Future<McpServers> logoutMcp(String name) async => ((await _host(mcpLogout(name: name))) as HostResultMcpServers).value;

  /// The MCP tools answered with 'Always allow' (`mcp__<server>__<tool>`).
  Future<List<String>> mcpAlwaysAllowed() async => (await _ask<ResponseBodyMcpAlwaysAllow>(mcpAlwaysAllowList())).value.tools;

  /// Whether runs may publish Claude Code artifacts (pages on claude.ai).
  Future<ArtifactSetting> artifactSetting([Duration? timeout]) async =>
      (await _ask<ResponseBodyArtifactSetting>(artifactGetSetting(), timeout)).value;

  /// Turn artifacts on or off (Approve right); applies from the next run.
  Future<ArtifactSetting> setArtifactsEnabled(bool enabled) async =>
      ((await _host(artifactSetEnabled(enabled: enabled))) as HostResultArtifactSetting).value;

  /// The artifacts runs published, newest first; all given filters apply.
  Future<List<ArtifactView>> artifacts({String? conversationId, String? taskId, String? projectId}) async =>
      (await _ask<ResponseBodyArtifacts>(artifactList(conversationId: conversationId, taskId: taskId, projectId: projectId))).value.artifacts;

  /// The file the server kept of one artifact version ([version] null: the
  /// newest); `snapshot` is null when it kept none (published before servers kept them).
  Future<ArtifactContent> artifactContent(String url, {int? version}) async =>
      (await _ask<ResponseBodyArtifactContent>(artifactGetContent(url: url, version: version), const Duration(seconds: 20))).value;

  Future<List<ArtifactView>>? _artifactIndex;
  StreamSubscription<Event>? _artifactIndexWatch;

  /// The artifact at [url] as the server lists it (its real title, whether its
  /// file is kept), for cards in a conversation; null when unknown. Asked once
  /// and again after each publish, or when [fresh].
  Future<ArtifactView?> knownArtifact(String url, {bool fresh = false}) async {
    _artifactIndexWatch ??= changes.where((e) => e.kind == EventKind.artifactPublished).listen((_) => _artifactIndex = null);
    if (fresh) _artifactIndex = null;
    final index = _artifactIndex ??= artifacts().timeout(const Duration(seconds: 10));
    try {
      return (await index).where((a) => a.url == url).firstOrNull;
    } on Object {
      if (identical(_artifactIndex, index)) _artifactIndex = null;
      return null;
    }
  }

  Future<bool>? _artifactsSupported;

  /// Whether the server knows artifacts: `artifact.get_setting` answers.
  /// Older gateways do not answer and only time out. A yes is kept; a no is
  /// asked again next time (the server may have been updated).
  Future<bool> artifactsSupported() => _artifactsSupported ??= artifactSetting(const Duration(seconds: 10)).then((_) => true, onError: (Object _) {
        _artifactsSupported = null;
        return false;
      });

  /// Whether the safe read-only tools pass without a question (feature `auto_allow`).
  Future<AutoAllowSetting> autoAllowSetting([Duration? timeout]) async =>
      (await _ask<ResponseBodyAutoAllowSetting>(autoAllowGetSetting(), timeout)).value;

  /// Turn auto-allow of the safe tools on or off (Approve right); applies from the next run.
  Future<AutoAllowSetting> setAutoAllowEnabled(bool enabled) async =>
      ((await _host(autoAllowSetEnabled(enabled: enabled))) as HostResultAutoAllowSetting).value;

  /// Installed Claude Code plugins.
  Future<PluginsSnapshot> pluginsList() async => (await _ask<ResponseBodyPlugins>(pluginList())).value;

  /// Plugins of the marketplaces already added.
  Future<PluginCatalog> pluginsAvailable() async => (await _ask<ResponseBodyPluginCatalog>(pluginListAvailable())).value;

  /// Add the marketplace behind [source] and list its plugins (Approve right).
  Future<PluginPreview> pluginsPreview(String source) async =>
      ((await _host(pluginPreview(source: source), const Duration(minutes: 3))) as HostResultPluginPreview).value;

  /// Install [ids] and enable them for [employees] (Approve right).
  Future<PluginsSnapshot> pluginsInstall(List<String> ids, List<String> employees, {List<String> projects = const []}) async =>
      ((await _host(pluginInstall(ids: ids, employees: employees, projects: projects), const Duration(minutes: 10))) as HostResultPlugins).value;

  Future<PluginsSnapshot> pluginUpdateOne(String id) async =>
      ((await _host(pluginUpdate(id: id), const Duration(minutes: 5))) as HostResultPlugins).value;

  Future<PluginsSnapshot> pluginRemoveOne(String id) async =>
      ((await _host(pluginRemove(id: id), const Duration(minutes: 2))) as HostResultPlugins).value;

  Future<EmployeePlugins> employeePlugins(String employeeId) async =>
      (await _ask<ResponseBodyEmployeePlugins>(pluginGetForEmployee(employeeId: employeeId))).value;

  /// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
  Future<EmployeeInstructionVersionsSnapshot> instructionVersions(String employeeId) async =>
      (await _ask<ResponseBodyEmployeeInstructionVersions>(employeeInstructionVersions(employeeId: employeeId))).value;

  /// Replace the plugins enabled for one employee; applies from its next run.
  Future<EmployeePlugins> setEmployeePlugins(String employeeId, List<String> enabled) async =>
      ((await _host(pluginSetForEmployee(enabled: enabled, employeeId: employeeId))) as HostResultEmployeePlugins).value;

  /// The plugins a project's assistant loads (server feature `project_plugins`).
  Future<ProjectPlugins> projectPlugins(String projectId) async =>
      (await _ask<ResponseBodyProjectPlugins>(pluginGetForProject(projectId: projectId))).value;

  /// Replace the plugins enabled for a project's assistant; applies from its next run.
  Future<ProjectPlugins> setProjectPlugins(String projectId, List<String> enabled) async =>
      ((await _host(pluginSetForProject(enabled: enabled, projectId: projectId))) as HostResultProjectPlugins).value;

  /// The machine's load and when new runs may start.
  Future<LoadSnapshot> load() async => (await _ask<ResponseBodyLoad>(loadGet())).value;

  /// Whether a shutdown of the server's PC is pending.
  Future<PowerState> power() async => (await _ask<ResponseBodyPower>(powerGet())).value;

  /// Shut the server's PC down now or once all work is done (Owner right).
  Future<PowerState> shutdownPc(ShutdownWhen when) async => ((await _host(powerShutdown(when: when))) as HostResultPower).value;

  /// Withdraw a pending shutdown (Owner right).
  Future<PowerState> cancelShutdown() async => ((await _host(powerCancel())) as HostResultPower).value;

  /// Callable agent functions (Owner right): all of them, or one project's.
  Future<AgentFunctionsSnapshot> agentFunctions({String? projectId}) async =>
      (await _ask<ResponseBodyAgentFunctions>(agentFunctionList(projectId: projectId))).value;

  /// Recent calls, newest first.
  Future<List<FunctionCallView>> functionCalls({String? functionId, int limit = 50}) async =>
      (await _ask<ResponseBodyFunctionCalls>(functionCallList(functionId: functionId, limit: limit))).value.calls;

  Future<List<ApiKeyView>> apiKeys() async => (await _ask<ResponseBodyApiKeys>(apiKeyList())).value;

  /// Restart the server's background service. It answers, then closes the
  /// connection, so a [DisconnectedException] is expected.
  Future<void> restartServer() => _host(serverRestart(), const Duration(seconds: 15));

  /// Send a write built by the generated method functions (see `Commands`).
  /// `retry: false` for decisions that must never be replayed later (approvals, plans).
  Future<Receipt> write(Map<String, dynamic> request, {String? idempotencyKey, int? expectedVersion, bool retry = true}) =>
      connection.write(request, idempotencyKey: idempotencyKey, expectedVersion: expectedVersion, retry: retry);

  /// Take back a write still waiting on this phone (see `AureliusConnection.withdraw`).
  bool withdraw(String idempotencyKey) => connection.withdraw(idempotencyKey);

  /// Upload a file to the PC; returns the host path to send with a message.
  Future<UploadedFile> upload(File file, String name) async {
    try {
      return await connection.api.upload(host, await connection.sessionToken(), file, name);
    } on SessionExpiredException {
      return connection.api.upload(host, await connection.sessionToken(), file, name);
    }
  }

  Future<void> registerPush({required String token, required String environment}) async {
    await connection.api.registerPush(host, await connection.sessionToken(), platform: Platform.isIOS ? 'apns' : 'fcm', pushToken: token, environment: environment);
  }

  Future<void> unregisterPush() async => connection.api.unregisterPush(host, await connection.sessionToken());
}
