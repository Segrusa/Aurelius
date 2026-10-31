import 'package:aurelius_mobile/data/aurelius_repository.dart';
import 'package:aurelius_mobile/data/views.dart';
import 'package:flutter_bloc/flutter_bloc.dart';
import 'package:mocktail/mocktail.dart';

// Small builders for the generated protocol types, so tests only name what matters.

ProjectSummary projectOf({String id = 'p1', String name = 'Atlas', ProjectStatus status = ProjectStatus.active, int employeesActive = 0, String? folder, String? description, int createdAtMs = 0}) =>
    ProjectSummary(
      createdAtMs: createdAtMs,
      description: description,
      employeesActive: employeesActive,
      folder: folder,
      id: id,
      name: name,
      status: status,
      version: 1,
    );

EmployeeSummary employeeOf({
  String id = 'm1',
  List<String> projectsActive = const [],
  String name = 'Iris',
  String role = 'Writer',
  String? title,
  String? reportsTo,
  LifecycleStatus status = LifecycleStatus.active,
  bool working = false,
  AgentMode agentMode = AgentMode.ask,
  bool browserAllowed = false,
  bool screenAllowed = false,
  bool sequentialTasks = false,
  String conversationId = 'main',
  List<ConversationSummary> conversations = const [],
  List<AgentView> agents = const [],
  HeartbeatView? heartbeat,
  String? instructions,
  int version = 1,
}) =>
    EmployeeSummary(
      agentMode: agentMode,
      agents: agents,
      browserAllowed: browserAllowed,
      conversationId: conversationId,
      conversations: conversations,
      heartbeat: heartbeat,
      id: id,
      instructions: instructions,
      name: name,
      projectsActive: projectsActive,
      reportsTo: reportsTo,
      role: role,
      screenAllowed: screenAllowed,
      sequentialTasks: sequentialTasks,
      status: status,
      title: title,
      version: version,
      working: working,
    );

TaskView taskOf({
  String id = 't1',
  String projectId = 'p1',
  String title = 'Write the post',
  String goal = 'A post',
  TaskStatus status = TaskStatus.queued,
  String? waitingReason,
  String? employeeId = 'm1',
  String? employeeName = 'Iris',
  String? conversationId,
  int pendingApprovals = 0,
  int updatedAtMs = 0,
  List<String> children = const [],
  String? parentTaskId,
  String? delegatedBy,
  String? scheduleId,
  String? result,
  TaskRun run = const TaskRun(),
  List<String> criteria = const [],
  String? createdByApp,
  VariantPhase? variantPhase,
  LiveRun? liveRun,
  String? ticketId,
  int? ticketNumber,
}) =>
    TaskView(
      liveRun: liveRun,
      ticketId: ticketId,
      ticketNumber: ticketNumber,
      attachments: const [],
      createdByApp: createdByApp,
      children: children,
      conversationId: conversationId,
      createdAtMs: 0,
      criteria: criteria,
      delegatedBy: delegatedBy,
      dependsOn: const [],
      goal: goal,
      handoffs: const [],
      id: id,
      employeeId: employeeId,
      employeeName: employeeName,
      parentTaskId: parentTaskId,
      pendingApprovals: pendingApprovals,
      projectId: projectId,
      result: result,
      run: run,
      scheduleId: scheduleId,
      status: status,
      title: title,
      updatedAtMs: updatedAtMs,
      variantPhase: variantPhase,
      waitingReason: waitingReason,
    );

MessageView messageOf(
  String id,
  MessageRole role,
  String text, {
  MessageStatus status = MessageStatus.complete,
  int ordinal = 0,
  bool queued = false,
  String? runId,
  String? replyTo,
  String? update,
  String? approvalId,
  String? author,
  List<ActivityView> activity = const [],
  List<FileRef> files = const [],
  List<MessageFileView> uploads = const [],
}) =>
    MessageView(
      activity: activity,
      approvalId: approvalId,
      author: author,
      createdAtMs: 0,
      files: files,
      id: id,
      ordinal: ordinal,
      queued: queued,
      replyTo: replyTo,
      role: role,
      runId: runId ?? (role == MessageRole.assistant ? 'run' : null),
      sources: const [],
      status: status,
      text: text,
      update: update,
      uploads: uploads,
    );

MessagesSnapshot snapshotOf(String conversationId, {List<MessageView> messages = const [], AgentMode agentMode = AgentMode.ask, bool browserAllowed = false, bool screenAllowed = false, bool? readOnly, TaskRun run = const TaskRun()}) =>
    MessagesSnapshot(
      agentMode: agentMode,
      asOfSeq: 1,
      attachments: const [],
      browserAllowed: browserAllowed,
      conversationId: conversationId,
      messages: messages,
      readOnly: readOnly,
      run: run,
      screenAllowed: screenAllowed,
      shares: const [],
    );

ApprovalView approvalOf(
  String id, {
  String tool = 'fake.publish',
  String effect = 'external_write',
  Object? arguments = const {'text': 'x'},
  String? conversationId = 'conv-1',
  String requester = 'Chat',
  ApprovalStatus status = ApprovalStatus.pending,
  int? expiresInMinutes = 30,
  String? taskId,
  bool alwaysAllowEligible = false,
  String runId = 'run',
}) =>
    ApprovalView(
      account: '',
      actionDigest: 'digest',
      alwaysAllowEligible: alwaysAllowEligible,
      arguments: arguments,
      conversationId: conversationId,
      effect: effect,
      expiresAtMs: DateTime.now().add(Duration(minutes: expiresInMinutes ?? 30)).millisecondsSinceEpoch,
      id: id,
      reasons: const [],
      requestedAtMs: 0,
      requester: requester,
      resources: const [],
      runId: runId,
      status: status,
      taskId: taskId,
      tool: tool,
    );

IdeaView ideaOf(String id, String title, {String projectId = 'p1', String author = 'You', String? employeeId, IdeaKind kind = IdeaKind.feature, IdeaStatus status = IdeaStatus.open, String? note, String? taskId, TaskStatus? taskStatus, List<MessageFileView> attachments = const []}) =>
    IdeaView(
      attachments: attachments,
      author: author,
      createdAtMs: 0,
      id: id,
      kind: kind,
      employeeId: employeeId,
      note: note,
      projectId: projectId,
      status: status,
      taskId: taskId,
      taskStatus: taskStatus,
      title: title,
      updatedAtMs: 0,
    );

ConversationSummary conversationOf(String id, {String title = '', int messageCount = 0, int updatedAtMs = 0}) =>
    ConversationSummary(createdAtMs: 0, id: id, messageCount: messageCount, title: title, updatedAtMs: updatedAtMs);

ProposalView proposalOf(String id, {String name = 'Sam', String role = 'Designer', String? title, String proposedByName = 'Iris', String? reportsTo, String? reportsToName, ProposalStatus status = ProposalStatus.pending, String reason = 'We need design help'}) =>
    ProposalView(
      createdAtMs: 0,
      id: id,
      instructions: 'Design things',
      name: name,
      proposedBy: 'm1',
      proposedByName: proposedByName,
      reason: reason,
      reportsTo: reportsTo,
      reportsToName: reportsToName,
      role: role,
      status: status,
      title: title,
    );

/// New instructions Iris proposed for Max (feature `instruction_proposals`); stale when [currentVersion] differs from [baseVersion].
ProposalView instructionProposalOf(String id, {int baseVersion = 2, int currentVersion = 2, String current = 'Be brief.\nShip on Fridays.', String proposed = 'Be brief.\nRun the tests before you ship.'}) =>
    ProposalView(
      baseVersion: baseVersion,
      createdAtMs: 0,
      currentInstructions: current,
      currentVersion: currentVersion,
      evidence: const [ProposalEvidence(id: 't7', kind: 'task', label: 'Fix the build'), ProposalEvidence(id: 'mem1', kind: 'memory', label: 'Lesson: tests first')],
      id: id,
      instructions: proposed,
      kind: ProposalKind.instructions,
      name: 'Max',
      proposedBy: 'm1',
      proposedByName: 'Iris',
      reason: 'Two tasks failed on untested changes',
      role: 'Developer',
      status: ProposalStatus.pending,
      targetEmployeeId: 'm2',
      targetEmployeeName: 'Max',
    );

TaskVariantView variantOf(int n, {String hint = '', VariantStatus status = VariantStatus.done, int? rank, String? judgeNote, String diff = '--- a/x\n+++ b/x\n-old\n+new', String report = 'Did it.'}) => TaskVariantView(
      deletions: 1,
      diff: diff,
      diffTruncated: false,
      errors: 0,
      files: 1,
      hint: hint,
      insertions: 1,
      judgeNote: judgeNote,
      n: n,
      rank: rank,
      report: report,
      status: status,
      usage: const TokenUsage(cacheCreationTokens: 100, cachedTokens: 0, estimate: false, inputTokens: 1000, outputTokens: 400, runs: 1),
    );

TaskVariantsView variantsOf(String taskId, {VariantPhase phase = VariantPhase.ready, int? judgePick = 2, String? compose = 'Variant 2 keeps the API.', List<TaskVariantView>? variants}) => TaskVariantsView(
      compose: compose,
      count: variants?.length ?? 2,
      judgePick: judgePick,
      phase: phase,
      taskId: taskId,
      variants: variants ?? [variantOf(1, hint: 'Small patch', rank: 2), variantOf(2, hint: 'Refactor', rank: 1, judgeNote: 'Cleaner')],
    );

Receipt receiptOf(CommandResult result) => Receipt(committedSeq: 1, idempotencyKey: 'k', replayed: false, result: WriteResultCommand(value: result));

Receipt hostReceiptOf(HostResult result) => Receipt(committedSeq: 1, idempotencyKey: 'k', replayed: false, result: WriteResultHost(value: result));

LoadLimits limitsOf({int maxRuns = 3, int cpuHigh = 85, int cpuLow = 60, int memMinMb = 2048, bool enabled = true}) =>
    LoadLimits(cpuHigh: cpuHigh, cpuLow: cpuLow, enabled: enabled, maxRuns: maxRuns, memMinMb: memMinMb);

LoadSnapshot loadOf({double cpuPct = 20, int memFreeMb = 8192, int memTotalMb = 16384, int running = 1, int held = 0, LoadGate gate = LoadGate.open, LoadHold? hold, LoadLimits? limits, bool checksActive = true}) =>
    LoadSnapshot(
      checksActive: checksActive,
      cpuPct: cpuPct,
      gate: gate,
      held: held,
      hold: hold,
      limits: limits ?? limitsOf(),
      memFreeMb: memFreeMb,
      memTotalMb: memTotalMb,
      running: running,
      sampledAtMs: 0,
    );

/// The writes a mocked repository got, in order (every write goes through `write`).
List<Map<String, dynamic>> writesOf(AureliusRepository repo) =>
    verify(() => repo.write(captureAny(), idempotencyKey: any(named: 'idempotencyKey'), expectedVersion: any(named: 'expectedVersion'), retry: any(named: 'retry'))).captured.cast<Map<String, dynamic>>();

/// The `params` of a request built by the generated method functions.
Map<String, dynamic> paramsOf(Map<String, dynamic> request) => request['params'] as Map<String, dynamic>;

/// The bloc's first state (the current one included) that passes [test].
/// Await this instead of sleeping: refreshes go through a real debounce
/// timer, so a fixed wait races it on a loaded machine.
Future<S> stateWhere<S>(BlocBase<S> bloc, bool Function(S) test) async =>
    test(bloc.state) ? bloc.state : bloc.stream.firstWhere(test);

/// What `company.get` answers: [employees] with the roots derived as the server does.
CompanySnapshot companyOf({List<EmployeeSummary> employees = const [], bool followOrgChart = true}) => CompanySnapshot(
      asOfSeq: 1,
      company: const CompanyView(id: 'company', name: 'Aurelius', version: 1),
      employees: employees,
      followOrgChart: followOrgChart,
      roots: [for (final e in employees) if (e.reportsTo == null) e.id],
    );
