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

  /// Anything a screen may show, without streaming bookkeeping (notes have [noteChanges]).
  Stream<Event> get changes => events.where((e) => !e.isNoise && !e.isNote);

  /// Notes created, saved, pinned or deleted (also by this phone).
  Stream<Event> get noteChanges => events.where((e) => e.isNote);

  /// The machine's load as it changes.
  Stream<LoadSnapshot> get loadChanges => connection.loads;

  /// The pending shutdown of the server's PC as it changes.
  Stream<PowerState> get powerChanges => connection.powers;

  /// Company changes (projects, employees, tasks, approvals, memory, …).
  Stream<Event> get companyChanges => events.where((e) => !e.isChat && !e.isNote && e.kind != EventKind.ticketFollowChanged);

  Future<R> _ask<R extends ResponseBody>(Map<String, dynamic> request, [Duration? timeout]) async {
    final body = timeout == null ? await connection.call(request) : await connection.call(request, timeout);
    return body as R;
  }

  /// A write the host answers with a [HostResult] (settings, plugins, power, …).
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
  Future<BoardView> board({String? projectId}) async => (await _ask<ResponseBodyBoard>(boardGet(projectId: projectId))).value;

  /// One ticket with its main task and every task linked to it.
  Future<TicketDetail> ticket(String ticketId) async => (await _ask<ResponseBodyTicket>(ticketGet(ticketId: ticketId))).value;

  /// Who follows a ticket (feature `ticket_follow`).
  Future<TicketFollowersView> followers(String ticketId) async => (await _ask<ResponseBodyTicketFollowers>(ticketFollowers(ticketId: ticketId))).value;

  /// Where a message is, for a link to it (feature `message_locate`).
  Future<MessageLocation> messageLocation(String messageId) async => (await _ask<ResponseBodyMessageLocation>(messageLocate(messageId: messageId))).value;

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

  /// The company's group chat (Team): the user and every employee. With
  /// [channel] (feature `team_channels`) a project's channel, or (feature
  /// `ticket_channels`) a ticket's; none: General.
  Future<MessagesSnapshot> groupChat({String? channel}) async => (await _ask<ResponseBodyMessages>(groupChatGet(channel: channel))).value;

  /// One thread of a channel (feature `chat_threads`): the message [messageId]
  /// and its replies. Reading it marks the thread read for the board member.
  Future<ThreadSnapshot> thread(String messageId) async => (await _ask<ResponseBodyThread>(threadRead(messageId: messageId))).value;

  /// The Team chat's channels (feature `team_channels`): General first, then one per project.
  Future<List<ChannelView>> channels() async => (await _ask<ResponseBodyChannels>(channelList())).value.channels;

  /// The channels whose name or ticket number matches [query] (feature
  /// `ticket_channels`), archived ones too, in the order of [channels].
  Future<List<ChannelView>> searchChannels(String query) async => (await _ask<ResponseBodyChannels>(channelSearch(query: query))).value.channels;

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

  /// The server has the vault (feature `vault`, Fase 4).
  bool get vaultEnabled => linkState.has('vault');

  /// A scope's vault: [projectId] null is the company's; a project's comes
  /// with the company's items apart. Never a secret's value.
  Future<VaultList> vault({String? projectId}) async => (await _ask<ResponseBodyVault>(vaultList(projectId: projectId))).value;

  /// One item's value (`vault.reveal`, audited as show or copy). The answer is
  /// never kept, logged or printed: callers hold it only while it is shown.
  Future<String> revealVaultValue(String itemId, RevealPurpose purpose) async =>
      (await _ask<ResponseBodyVaultValue>(vaultReveal(itemId: itemId, purpose: purpose))).value.value;

  /// The vault's audit log, newest first: the company's, [projectId]'s, every
  /// scope's ([all]) or one item's.
  Future<VaultAuditPage> vaultAuditLog({String? projectId, bool all = false, String? itemId, int? before, int limit = 50}) async =>
      (await _ask<ResponseBodyVaultAudit>(vaultAudit(all: all ? true : null, before: before, itemId: itemId, limit: limit, projectId: projectId))).value;

  /// A new item; never retried, so the value leaves this phone's outbox at once.
  Future<VaultItemView> createVaultItem(VaultCreateParams params) async => ((await _host(vaultCreate(params))) as HostResultVaultItem).value;

  /// Change an item that is still at [version] (`version_conflict` otherwise).
  Future<VaultItemView> updateVaultItem(VaultUpdateParams params, {required int version}) async {
    final receipt = await connection.write(vaultUpdate(params), expectedVersion: version, retry: false);
    return ((receipt.result as WriteResultHost).value as HostResultVaultItem).value;
  }

  /// "Delete": runs and employees no longer get it; its history stays.
  Future<VaultItemView> archiveVaultItem(String itemId) async => ((await _host(vaultArchive(itemId: itemId))) as HostResultVaultItem).value;

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

  /// Every version of a guideline, newest first.
  Future<List<GuidelineVersionView>> guidelineVersionList(String guidelineId) async =>
      (await _ask<ResponseBodyGuidelineVersions>(guidelineVersions(guidelineId: guidelineId))).value.versions;

  /// What a run of [employeeId] in [projectId] would get now (B3 order, B4 total).
  Future<EffectiveGuidelines> effectiveGuidelines({String? employeeId, String? projectId}) async =>
      (await _ask<ResponseBodyEffectiveGuidelines>(guidelineEffective(employeeId: employeeId, projectId: projectId))).value;

  /// Proposals of `propose_guideline` (B5): pending first.
  Future<List<GuidelineProposalView>> guidelineProposalList({ProposalStatus? status}) async =>
      (await _ask<ResponseBodyGuidelineProposals>(guidelineProposals(status: status))).value.proposals;

  /// Who a change of a guideline reaches from their next run (B6).
  Future<GuidelineAffected> guidelineReach(String guidelineId) async =>
      (await _ask<ResponseBodyGuidelineAffected>(guidelineAffected(guidelineId: guidelineId))).value;

  /// The templates of the starter set (B7).
  Future<List<StarterTemplate>> starterTemplates() async => (await _ask<ResponseBodyStarterSet>(guidelineStarterSet())).value.templates;

  /// The guideline versions [runId] had in its system text (B6).
  Future<List<RunGuideline>> runGuidelineList(String runId) async =>
      (await _ask<ResponseBodyRunGuidelines>(runGuidelines(runId: runId))).value.guidelines;

  /// A guideline or one of its proposals changed (here or on another client).
  Stream<Event> get guidelineChanges => events.where((e) => e.kind.wire.startsWith('guideline.'));

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
