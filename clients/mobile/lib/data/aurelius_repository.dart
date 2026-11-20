import 'dart:async';
import 'dart:convert';
import 'dart:io';
import 'dart:typed_data';

import '../core/hosts/host.dart';
import '../core/net/connection.dart';
import '../core/net/gateway_api.dart';
import '../core/net/protocol.dart';
import 'drafts.dart';
import 'protocol/protocol.dart';
import 'tickets.dart';
import 'views.dart' show TaskViewState;

/// The data layer for one host: typed queries and writes over the
/// connection (the generated method functions), plus the change signals
/// blocs refresh on.
class AureliusRepository {
  AureliusRepository(this.connection);

  final AureliusConnection connection;

  Host get host => connection.host;

  /// The composers' unsent text (R3 M1). [drafts] is the phone's store.
  DraftStore drafts = DraftStore.instance;

  /// The signed-in board member's draft in [slot] ("ticket:…",
  /// "conversation:…"); null without a board member or a draft.
  String? draft(String slot) {
    final me = linkState.user?.id;
    return me == null ? null : drafts.read(host.hostId, me, slot);
  }

  /// Keep [text] as the signed-in board member's draft in [slot]; empty forgets it.
  void keepDraft(String slot, String text) {
    final me = linkState.user?.id;
    if (me != null) drafts.write(host.hostId, me, slot, text);
  }
  Stream<LinkState> get link => connection.states;
  LinkState get linkState => connection.state;
  Stream<Event> get events => connection.events;
  Stream<StreamAppend> get appends => connection.appends;

  /// Fires when the link comes (back) online: read models refetch then.
  Stream<void> get online => connection.states.where((s) => s.isOnline).map((_) {});

  /// Chat changes (messages, conversations, presence).
  Stream<Event> get chatChanges => events.where((e) => e.isChat && !e.isNoise);

  /// Anything a screen may show, without streaming bookkeeping.
  Stream<Event> get changes => events.where((e) => !e.isNoise);

  /// The machine's load as it changes.
  Stream<LoadSnapshot> get loadChanges => connection.loads;

  /// The pending shutdown of the server's PC as it changes.
  Stream<PowerState> get powerChanges => connection.powers;

  /// Company changes (projects, employees, tasks, approvals, memory, …).
  Stream<Event> get companyChanges => events.where((e) => !e.isChat && e.kind != EventKind.ticketFollowChanged);

  Future<R> _ask<R extends ResponseBody>(Map<String, dynamic> request, [Duration? timeout]) async {
    final body = timeout == null ? await connection.call(request) : await connection.call(request, timeout);
    return body as R;
  }

  /// A write the host answers with a [HostResult] (settings, power, …).
  Future<HostResult> _host(Map<String, dynamic> request, [Duration? timeout]) async {
    final receipt = await connection.write(request, retry: false, timeout: timeout);
    return (receipt.result as WriteResultHost).value;
  }

  /// The server's name, its source and the computer's name (`server.get_name`).
  /// Only ask when the link has the feature `server_name` (SN-02 AC4).
  Future<ServerNameView> serverName() => connection.refreshServerName();

  /// Every `server.get_name` answer, the reads after `server.renamed` too.
  Stream<ServerNameView> get serverNameChanges => connection.serverNames;

  /// Rename the server (`server.set_name`); empty or only spaces brings the
  /// computer's name back. Returns the name clients show now. Never resent
  /// after a reconnect: the board member sees the outcome in the dialog.
  Future<CommandResultServerNameSet> setServerName(String name) async {
    final receipt = await connection.write(serverSetName(name: name), retry: false);
    final set = (receipt.result as WriteResultCommand).value as CommandResultServerNameSet;
    await connection.applyServerName(set.name);
    return set;
  }

  /// The board members, oldest first, disabled ones too.
  Future<List<UserView>> boardMembers() async => (await _ask<ResponseBodyUsers>(userList())).value.users;

  /// A board member was added or disabled (here or on another client).
  Stream<Event> get boardMemberChanges => events.where((e) => e.kind == EventKind.userCreated || e.kind == EventKind.userDisabled);

  /// Sign in on this phone (`user.login`, MU3-02).
  Future<LoginResult> signIn(String username, String password) => connection.signIn(username, password);

  /// Sign out on this phone (`user.logout`, MU3-04 AC3).
  Future<void> signOut() => connection.signOut();

  /// A new board member; returns its id. The first one signs this phone in.
  Future<String> createBoardMember(String username, String password) => connection.createBoardMember(username, password);

  /// Own password ([oldPassword], the other sessions end) or a temporary one
  /// for [userId] (B10, all their sessions end).
  Future<void> setPassword({required String newPassword, String? oldPassword, String? userId}) =>
      connection.setPassword(newPassword: newPassword, oldPassword: oldPassword, userId: userId);

  /// Disable [userId] (MU3-05 AC3); `last_board_member` for the last active one.
  Future<UserView> disableBoardMember(String userId) async => ((await _host(userDisable(userId: userId))) as HostResultUser).value;

  /// The signed-in devices of [userId] (none: this phone's board member).
  Future<SessionsSnapshot> sessions({String? userId}) async => (await _ask<ResponseBodySessions>(sessionList(userId: userId))).value;

  /// End [sessionId]; that device falls back to its sign-in screen (MU3-04 AC2).
  Future<void> revokeSession(String sessionId) => _host(sessionRevoke(sessionId: sessionId));

  /// The own Claude login (`claude_login.status`).
  Future<ClaudeLoginView?> claudeLogin() => connection.refreshClaudeLogin();

  /// Start Connect (`claude_login.link`); what Claude asks comes on [claudeLoginPrompts].
  Future<String> linkClaudeLogin() async => ((await _host(claudeLoginLink())) as HostResultClaudeLoginLink).linkId;

  /// What `claude auth login` asks during Connect (only this connection's).
  Stream<ClaudeLoginPrompt> get claudeLoginPrompts => connection.claudeLoginPrompts;

  /// The code Claude showed, for [linkId] (`claude_login.input`, a session
  /// method). Sent once, never kept or logged.
  Future<ClaudeLoginView> sendClaudeLoginCode(String linkId, String code) async =>
      (await _ask<ResponseBodyClaudeLogin>(claudeLoginInput(ClaudeLoginInputParams(linkId: linkId, text: code)))).value;

  /// Stop a running Connect.
  Future<void> cancelClaudeLogin(String linkId) => _host(claudeLoginCancel(linkId: linkId));

  /// [login] as the own Claude login (an answer that carries it).
  void applyClaudeLogin(ClaudeLoginView login) => connection.applyClaudeLogin(login);

  /// The sign-in screen showed "You were signed out on this device.".
  void dismissSignedOut() => connection.dismissSignedOut();

  /// Disconnect the own Claude login (MU3-09 AC4).
  Future<void> unlinkClaudeLogin() async {
    final result = await _host(claudeLoginUnlink());
    if (result is HostResultClaudeLogin) applyClaudeLogin(result.value);
  }

  /// The Board: its columns (six, seven with Testing) with their cards, for one project or all (B4).
  /// Who is busy and what waits (feature `activity_overview`).
  Future<ActivityOverview> overview() async => (await _ask<ResponseBodyActivityOverview>(activityOverview())).value;

  Future<BoardView> board({String? projectId}) async => (await _ask<ResponseBodyBoard>(boardGet(projectId: projectId))).value;

  /// One ticket with its main task and every task linked to it.
  Future<TicketDetail> ticket(String ticketId) async => (await _ask<ResponseBodyTicket>(ticketGet(ticketId: ticketId))).value;

  /// The model and effort a new run of each employee uses (feature `employee_run_overview`).
  Future<EmployeeRunsView> employeeRuns() async => (await _ask<ResponseBodyEmployeeRuns>(employeeRunOverview())).value;

  /// Who follows a ticket (feature `ticket_follow`).
  Future<TicketFollowersView> followers(String ticketId) async => (await _ask<ResponseBodyTicketFollowers>(ticketFollowers(ticketId: ticketId))).value;

  /// The tickets of the signed-in board member (feature `my_tickets`); [includeDone] adds Done and archived ones.
  Future<MyTicketsView> myTickets({String? projectId, bool includeDone = false}) async =>
      (await _ask<ResponseBodyMyTickets>(ticketMyTickets(projectId: projectId, includeDone: includeDone ? true : null))).value;

  /// The earlier texts of an edited message, newest first (feature `message_edits`).
  Future<MessageHistoryView> messageEdits(String messageId) async => (await _ask<ResponseBodyMessageHistory>(messageHistory(messageId: messageId))).value;

  /// Whom `@Everyone` would reach in a discussion (feature `everyone_mention`):
  /// [channel] is the project channel's or ticket discussion's conversation; null is General.
  Future<DiscussionAudience> discussionAudienceOf({String? channel}) async =>
      (await _ask<ResponseBodyDiscussionAudience>(discussionAudience(channel: channel))).value;

  /// Where a message is, for a link to it (feature `message_locate`).
  Future<MessageLocation> messageLocation(String messageId) async => (await _ask<ResponseBodyMessageLocation>(messageLocate(messageId: messageId))).value;

  /// What changes a card or the columns: tickets, the columns, the tasks under
  /// tickets, projects and employees (dispatcher, names).
  Stream<Event> get boardChanges => changes.where((e) {
        final kind = e.kind.wire;
        return kind.startsWith('ticket.') || kind.startsWith('board.') || kind.startsWith('task.') || kind.startsWith('project.') || kind.startsWith('employee.') || kind.startsWith('company.') || kind.startsWith('approval.');
      });

  Future<List<ProjectSummary>> projects() async => (await _ask<ResponseBodyProjects>(projectList())).value.projects;

  Future<MessagesSnapshot> messages(String conversationId) async =>
      (await _ask<ResponseBodyMessages>(conversationGet(conversationId: conversationId))).value;

  /// The reader's work sessions, the latest activity first (feature `work_sessions`).
  Future<List<WorkSessionView>> workSessions() async => (await _ask<ResponseBodyWorkSessions>(workSessionList())).value.sessions;

  /// The channels whose name or ticket number matches [query] (feature
  /// `ticket_channels`), archived ones too.
  Future<List<ChannelView>> searchChannels(String query) async => (await _ask<ResponseBodyChannels>(channelSearch(query: query))).value.channels;

  /// One tool call of message [messageId] whole (`activity_preview`: messages
  /// and live updates carry shortened ones). Only on demand, when the user opens it.
  Future<ActivityView> activity(String messageId, String callId) async =>
      (await _ask<ResponseBodyActivity>(activityGet(messageId: messageId, callId: callId))).value;

  /// The guidelines the server put in a run's system text (`run.guidelines`).
  Future<List<RunGuideline>> guidelinesOfRun(String runId) async => (await _ask<ResponseBodyRunGuidelines>(runGuidelines(runId: runId))).value.guidelines;

  /// The company's employees (archived ones left out by the server).
  Future<List<EmployeeSummary>> employees() async => (await _ask<ResponseBodyEmployees>(employeeList())).value.employees;

  /// The company in one read: employees, Follow the org chart and the management team.
  Future<CompanySnapshot> company() async => (await _ask<ResponseBodyCompany>(companyGet())).value;

  /// Colleague and instruction proposals of the whole company, pending first.
  Future<List<ProposalView>> proposals() async => (await _ask<ResponseBodyProposals>(proposalList())).value.proposals;

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

  Future<ConnectionsSnapshot> connections() async => (await _ask<ResponseBodyConnections>(connectionList())).value;

  Future<NotificationsSnapshot> notifications() async => (await _ask<ResponseBodyNotifications>(notificationList())).value;

  Future<void> markNotificationsRead() => _host(notificationMarkAllRead());

  /// Mark one Inbox row read for this person (A3): opening a row does it.
  Future<void> markNotificationRead(String id) => _host(notificationMarkRead(id: id));

  /// Remove one notification (swiped away).
  Future<void> dismissNotification(String id) => _host(notificationDismiss(id: id));

  /// Remove every notification; the number removed.
  Future<int> dismissAllNotifications() async {
    final result = await _host(notificationDismissAll());
    return result is HostResultCount ? result.count : 0;
  }

  Future<UsageSnapshot> usage() async => (await _ask<ResponseBodyUsage>(usageGet())).value;

  Future<SubscriptionUsage> subscription({bool refresh = false}) async => (await _ask<ResponseBodySubscription>(subscriptionGet(refresh: refresh))).value;

  Future<ContextView> context(String runId) async => (await _ask<ResponseBodyContext>(runGetContext(runId: runId))).value;

  /// A screenshot the agent took (JPEG), by the id in its tool output.
  Future<Uint8List> screenshot(String id) async => base64Decode((await _ask<ResponseBodyScreenshot>(screenshotGet(id: id))).jpegBase64);

  /// A project's ideas.
  Future<List<IdeaView>> ideas(String projectId) async => (await _ask<ResponseBodyIdeas>(ideaList(projectId: projectId))).value.ideas;

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

  /// One Markdown file, by path relative to the work folder or absolute inside it.
  Future<MarkdownFileContent> markdownFile(FileScope scope, String path) async =>
      (await _ask<ResponseBodyMarkdownFile>(markdownGet(path: path, scope: scope))).value;

  /// The subagents running in one conversation's answers.
  Future<List<AgentView>> subagents(String conversationId) async =>
      (await _ask<ResponseBodySubagents>(subagentList(conversationId: conversationId))).value;

  /// The server has the vault (feature `vault`, Fase 4).
  bool get vaultEnabled => linkState.has('vault');

  /// A scope's vault: [projectId] null is the company's; a project's comes
  /// with the company's items apart. Never a secret's value.
  Future<VaultList> vault({String? projectId}) async => (await _ask<ResponseBodyVault>(vaultList(projectId: projectId))).value;

  /// An item was created, changed or archived (here or on another client).
  Stream<Event> get vaultChanges => events.where((e) => e.kind.wire.startsWith('vault.'));

  /// The server has guidelines (feature `guidelines`, AUR-GL-01).
  bool get guidelinesEnabled => linkState.has('guidelines');

  /// The library without texts, alphabetical: the active guidelines, or only the archived ones.
  Future<List<GuidelineView>> guidelines({bool archived = false}) async =>
      (await _ask<ResponseBodyGuidelines>(guidelineList(archived: archived ? true : null))).value.guidelines;

  /// One guideline with the text of its current version, or of [version].
  Future<GuidelineDetail> guideline(String guidelineId, {int? version}) async =>
      (await _ask<ResponseBodyGuideline>(guidelineGet(guidelineId: guidelineId, version: version))).value;

  /// The guidelines linked to an employee, with their texts, as its runs get them.
  Future<EffectiveGuidelines> effectiveGuidelines(String employeeId) async =>
      (await _ask<ResponseBodyEffectiveGuidelines>(guidelineEffective(employeeId: employeeId))).value;

  /// Proposals of `propose_guideline` (B5): pending first.
  Future<List<GuidelineProposalView>> guidelineProposalList({ProposalStatus? status}) async =>
      (await _ask<ResponseBodyGuidelineProposals>(guidelineProposals(status: status))).value.proposals;

  /// A guideline or one of its proposals changed (here or on another client).
  Stream<Event> get guidelineChanges => events.where((e) => e.kind.wire.startsWith('guideline.'));

  /// The user's Claude Code MCP servers and which are on for every chat, conversation, employee and task.
  Future<McpServers> mcpServers() async => (await _ask<ResponseBodyMcpServers>(mcpList())).value;

  /// MCP login and logout changed (`mcp.login_finished`, `mcp.auth_changed`).
  Stream<Event> get mcpAuthChanges => events.where((e) => e.kind == EventKind.mcpLoginFinished || e.kind == EventKind.mcpAuthChanged);


  /// The versions of an employee's instructions, newest first (feature `instruction_proposals`).
  Future<EmployeeInstructionVersionsSnapshot> instructionVersions(String employeeId) async =>
      (await _ask<ResponseBodyEmployeeInstructionVersions>(employeeInstructionVersions(employeeId: employeeId))).value;

  /// The machine's load and when new runs may start.
  Future<LoadSnapshot> load() async => (await _ask<ResponseBodyLoad>(loadGet())).value;

  /// Whether a shutdown of the server's PC is pending.
  Future<PowerState> power() async => (await _ask<ResponseBodyPower>(powerGet())).value;

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

/// What needs the signed-in board member, used by the company.
extension NeedsMeReads on AureliusRepository {
  /// [othersMainTasks] of [tasks]. Reads the board only when a main task waits
  /// on a server with a Responsible; a failed read leaves them counted as before.
  Future<Set<String>> mainTasksOfOthers(List<TaskView> tasks) async {
    if (!linkState.has(ticketResponsibleFeature) || !tasks.any((t) => t.isTicketMain && t.needsYou)) return const {};
    try {
      final read = await board();
      return othersMainTasks(tasks, read.columns.expand((c) => c.tickets), me: linkState.user?.id);
    } on DisconnectedException {
      return const {};
    } on ProtocolException {
      return const {};
    }
  }
}
